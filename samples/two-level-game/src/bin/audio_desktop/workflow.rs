use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::ExitCode,
    rc::Rc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    app::{FixedTime, RuntimeControl, RuntimeControlStatus, RuntimeState},
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandPayload,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTick,
        GameplayCommandTypeId, GameplayCommandValue,
    },
    project_host::{
        DesktopControlError, DesktopPresentationControl, DesktopPresentationObservation,
        DesktopRun, DesktopRunIntent, DesktopRunOptions, DesktopRunProgress, WinitEventLoop,
    },
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use serde::Serialize;
use two_level_game::{
    GamePhase, GameSnapshot, RunOptions, Scenario,
    audio::{self, AudioCause, AudioOptions, AudioSnapshot, AudioWitness},
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
};

use super::oracle::{self, CHANNELS, MAX_OUTPUT_FRAMES, PcmEvidence, SAMPLE_RATE};

const POLL: Duration = Duration::from_millis(2);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(15);
const GAME_TIMEOUT: Duration = Duration::from_secs(60);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Adapter {
    name: String,
    backend: String,
    device_type: String,
    driver: String,
    driver_info: String,
    backend_instance: u64,
    device_epoch: u64,
}

#[derive(Default, Serialize)]
struct NativeEvidence {
    runtime_terminal: bool,
    world_unavailable: bool,
    native_destroyed: bool,
    surface_retired: bool,
    provider_released: bool,
    target_fault_absent: bool,
    physical_event_loop_recovered: bool,
    original_presentations: u64,
    adapter: Option<Adapter>,
}

#[derive(Serialize)]
struct GameEvidence {
    level: Option<LevelKey>,
    instance: Option<u64>,
    logical_tick: u64,
    score: u32,
    collected: usize,
    visible_pickups: usize,
    markers: Vec<u64>,
    travel_count: u32,
    restarts: u32,
    saves: u32,
    restores: u32,
    script_verified: bool,
}

impl From<&GameSnapshot> for GameEvidence {
    fn from(value: &GameSnapshot) -> Self {
        Self {
            level: value.level,
            instance: value.current_instance.map(|id| id.get()),
            logical_tick: value.logical_tick,
            score: value.score,
            collected: value.collected,
            visible_pickups: value.visible_pickups,
            markers: value.markers.iter().map(|id| id.get()).collect(),
            travel_count: value.travel_count,
            restarts: value.restarts,
            saves: value.saves,
            restores: value.restores,
            script_verified: value.verified,
        }
    }
}

#[derive(Default, Serialize)]
struct ControlEvidence {
    exact_steps: u64,
    pickup_step_tick: u64,
    paused_tick: u64,
    paused_output_start: u64,
    paused_output_end: u64,
    resumed_tick: u64,
    failed_travel: Option<GameEvidence>,
    failed_travel_error: Option<String>,
    failed_travel_audio_accepted: u64,
    changed_bank_error: Option<String>,
    changed_bank_audio_accepted: u64,
    revised_scene_travels: u64,
    retry_before_instance: Option<u64>,
    retry_after_instance: Option<u64>,
    retry_audio_accepted: u64,
    save_restore_audio_accepted: u64,
}

#[derive(Serialize)]
struct RunEvidence {
    trajectory: &'static str,
    failure: Option<String>,
    game: Option<GameEvidence>,
    controls: Option<ControlEvidence>,
    audio: Option<AudioSnapshot>,
    pcm: Option<PcmEvidence>,
    native: NativeEvidence,
}

impl RunEvidence {
    fn new(trajectory: &'static str) -> Self {
        Self {
            trajectory,
            failure: None,
            game: None,
            controls: None,
            audio: None,
            pcm: None,
            native: NativeEvidence::default(),
        }
    }

    fn fail(&mut self, message: impl AsRef<str>) {
        match &mut self.failure {
            Some(failure) => {
                failure.push_str("; ");
                failure.push_str(message.as_ref());
            }
            None => self.failure = Some(message.as_ref().to_owned()),
        }
    }
}

#[derive(Serialize)]
struct Report {
    format: &'static str,
    accepted: bool,
    failure: Option<String>,
    runs: Vec<RunEvidence>,
    fresh_runtime_generation: bool,
    retired_generation_unchanged: bool,
    physical_event_loop_recovered_and_dropped: bool,
    temporary_project_removed: bool,
    gameplay_deadline_ms: u128,
    close_deadline_ms: u128,
    maximum_capture_frames_per_run: u64,
    deadline_scope: &'static str,
}

#[derive(Default)]
struct Observed {
    witness: Option<AudioWitness>,
    target: Option<BackendWindowHandles>,
    adapter: Option<Adapter>,
    presented: Option<GameSnapshot>,
    presentations: u64,
    last_frame: Option<(u64, u64)>,
    output: Vec<f32>,
    instances: BTreeMap<u64, LevelKey>,
    failure: Option<String>,
}

impl Observed {
    fn drain(&mut self) {
        let Some(witness) = &self.witness else { return };
        for chunk in witness.drain_output() {
            if self.failure.is_some() {
                continue;
            }
            if chunk.channels as usize != CHANNELS
                || chunk.first_channel != 0
                || chunk.samples.is_empty()
                || !chunk.samples.len().is_multiple_of(CHANNELS)
                || chunk.first_frame != (self.output.len() / CHANNELS) as u64
            {
                self.failure =
                    Some("callback capture is non-stereo, discontinuous, or incomplete".into());
                continue;
            }
            if self.output.len() + chunk.samples.len() > MAX_OUTPUT_FRAMES as usize * CHANNELS {
                self.failure =
                    Some("complete callback output exceeded its finite capture allowance".into());
                continue;
            }
            self.output.extend(chunk.samples);
        }
        let snapshot = witness.snapshot();
        if self.failure.is_none() && (snapshot.faults != 0 || snapshot.error.is_some()) {
            self.failure = Some(format!(
                "native audio prerequisite or runtime failure: {:?}; faults={:#x}",
                snapshot.error, snapshot.faults
            ));
        }
    }

    fn observe(
        &mut self,
        observation: DesktopPresentationObservation<'_>,
        verify: bool,
    ) -> Result<(), String> {
        if let Some(world) = observation.world() {
            if self.witness.is_none() {
                self.witness = audio::witness(world);
            }
            if self.target.is_none() {
                self.target = world.get_resource::<BackendWindowHandles>().cloned();
            }
            if let Some(snapshot) = world.get_resource::<GameSnapshot>()
                && let (Some(instance), Some(level)) = (snapshot.current_instance, snapshot.level)
            {
                if self
                    .instances
                    .insert(instance.get(), level)
                    .is_some_and(|prior| prior != level)
                {
                    return Err("one actual Scene instance changed its game-owned level key".into());
                }
            }
        }
        self.drain();
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let Some(receipt) = observation.presented_frame() else {
            return Ok(());
        };
        let identity = (
            receipt.app_frame_index(),
            receipt.render_transaction_frame_index(),
        );
        if self.last_frame == Some(identity) {
            return Ok(());
        }
        if self
            .last_frame
            .is_some_and(|prior| identity.0 <= prior.0 || identity.1 <= prior.1)
        {
            return Err("original App/render presentation identities regressed".into());
        }
        let world = observation
            .world()
            .ok_or("original presented World is unavailable")?;
        let adapter = observation
            .adapter_summary()
            .ok_or("original adapter summary is unavailable")?;
        if receipt.runtime_generation() != observation.runtime_generation()
            || self.witness.as_ref().is_some_and(|witness| {
                witness.snapshot().runtime != receipt.runtime_generation().get()
            })
            || receipt.window_id() != WindowId::PRIMARY
            || receipt.backend_instance_id() != observation.wgpu().instance_id()
            || receipt.device_epoch() != observation.wgpu().device_epoch()
        {
            return Err(
                "presentation receipt disagrees with its original Runtime/window/backend".into(),
            );
        }
        if verify
            && (std::env::var("WGPU_BACKEND").as_deref() != Ok(adapter.backend())
                || !matches!(adapter.device_type(), "discrete-gpu" | "integrated-gpu"))
        {
            return Err(
                "verification requires the actually observed selected hardware backend".into(),
            );
        }
        let target = self
            .target
            .as_ref()
            .ok_or("original native target is unavailable")?
            .snapshot(WindowId::PRIMARY)
            .map_err(|error| error.to_string())?;
        if target.phase != WindowTargetPhase::Active
            || target.fault.is_some()
            || !target.surface_active
            || !target.provider_present
            || target.native_destroyed
            || target.surface_generation != Some(receipt.surface_generation())
        {
            return Err(
                "original successful presentation has no matching live native surface".into(),
            );
        }
        let adapter = Adapter {
            name: adapter.name().into(),
            backend: adapter.backend().into(),
            device_type: adapter.device_type().into(),
            driver: adapter.driver().into(),
            driver_info: adapter.driver_info().into(),
            backend_instance: receipt.backend_instance_id(),
            device_epoch: receipt.device_epoch(),
        };
        if self.adapter.as_ref().is_some_and(|prior| prior != &adapter) {
            return Err("original hardware backend changed during gameplay".into());
        }
        self.adapter = Some(adapter);
        self.presented = Some(world.resource::<GameSnapshot>().clone());
        self.presentations += 1;
        self.last_frame = Some(identity);
        Ok(())
    }
}

struct Session {
    run: Option<DesktopRun>,
    observed: Rc<RefCell<Observed>>,
    observer_panicked: Rc<Cell<bool>>,
    sequence: u64,
    exact_steps: u64,
    game_deadline: Option<Instant>,
}

impl Session {
    fn new(
        project: &Path,
        options: RunOptions,
        verify: bool,
        paused: bool,
        event_loop: Option<WinitEventLoop>,
    ) -> Result<Self, String> {
        let root = two_level_game::open_explicit_project_root(project)
            .map_err(|error| error.to_string())?;
        let observed = Rc::new(RefCell::new(Observed::default()));
        let observer = observed.clone();
        let observer_panicked = Rc::new(Cell::new(false));
        let panic_flag = observer_panicked.clone();
        let mut execution = DesktopRunOptions::new()
            .with_cleanup_timeout(Duration::from_secs(5))
            .with_native_retirement_budget(Duration::from_secs(2))
            .with_presentation_observer(move |observation| {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    observer.borrow_mut().observe(observation, verify)
                }));
                match result {
                    Ok(Ok(())) => DesktopPresentationControl::Continue,
                    Ok(Err(error)) => {
                        observer.borrow_mut().failure.get_or_insert(error);
                        DesktopPresentationControl::Exit(nara::app::AppExit::Requested)
                    }
                    Err(payload) => {
                        std::mem::forget(payload);
                        panic_flag.set(true);
                        DesktopPresentationControl::Exit(nara::app::AppExit::Requested)
                    }
                }
            });
        if paused {
            execution = execution.with_initial_pause();
        }
        if verify && !paused {
            execution = execution.with_input_isolation(true);
        }
        if let Some(event_loop) = event_loop {
            execution = execution.with_event_loop(event_loop);
        }
        let recipe = audio::recipe(
            options,
            AudioOptions {
                capture_output: verify,
            },
        );
        Ok(Self {
            run: Some(DesktopRun::new(
                root,
                DesktopRunIntent::new()
                    .with_profile("desktop")
                    .with_recipe(recipe)
                    .with_execution(execution),
            )),
            observed,
            observer_panicked,
            sequence: 0,
            exact_steps: 0,
            game_deadline: verify.then(|| Instant::now() + GAME_TIMEOUT),
        })
    }

    fn run(&mut self) -> &mut DesktopRun {
        self.run
            .as_mut()
            .expect("original desktop owner is retained")
    }

    fn healthy(&mut self) -> Result<(), String> {
        self.observed.borrow_mut().drain();
        if self.observer_panicked.get() {
            return Err("original presentation observer panicked".into());
        }
        if let Some(error) = &self.observed.borrow().failure {
            return Err(error.clone());
        }
        Ok(())
    }

    fn poll(&mut self, deadline: Instant) -> Result<(), String> {
        let deadline = self
            .game_deadline
            .map_or(deadline, |game| deadline.min(game));
        if Instant::now() >= deadline {
            return Err("game operation exceeded its cooperative deadline".into());
        }
        let progress = self.run().poll(POLL);
        self.healthy()?;
        if Instant::now() >= deadline {
            return Err("original game poll returned after its cooperative deadline".into());
        }
        if progress != DesktopRunProgress::Running {
            return Err(format!(
                "ordinary desktop stopped before trajectory completion: {progress:?}"
            ));
        }
        Ok(())
    }

    fn snapshot(&mut self) -> Result<GameSnapshot, String> {
        self.run()
            .capture(|world| world.resource::<GameSnapshot>().clone())
            .map_err(|error| error.to_string())
    }

    fn settle(&mut self, allow_game_error: bool) -> Result<GameSnapshot, String> {
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        loop {
            self.poll(deadline)?;
            match self
                .run()
                .capture(|world| world.resource::<GameSnapshot>().clone())
            {
                Ok(snapshot) => {
                    if !allow_game_error && let Some(error) = &snapshot.error {
                        return Err(error.clone());
                    }
                    if snapshot.phase == GamePhase::Degraded {
                        return Err("ordinary gameplay reconstruction degraded".into());
                    }
                    if snapshot.initialized && snapshot.phase == GamePhase::Running {
                        return Ok(snapshot);
                    }
                }
                Err(DesktopControlError::NotPaused) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    fn control(&mut self, control: RuntimeControl) -> Result<(), String> {
        let ticket = self
            .run()
            .request_control(control)
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        loop {
            self.poll(deadline)?;
            match self.run().control_status(ticket) {
                Some(RuntimeControlStatus::Applied) => return Ok(()),
                Some(RuntimeControlStatus::Pending) => {}
                status => {
                    return Err(format!(
                        "original desktop control {control:?} failed: {status:?}"
                    ));
                }
            }
        }
    }

    fn step(
        &mut self,
        command: Option<GameplayCommandDraft>,
        allow_game_error: bool,
    ) -> Result<GameSnapshot, String> {
        let tick = self
            .run()
            .capture(|world| world.resource::<FixedTime>().tick())
            .map_err(|error| error.to_string())?;
        if let Some(command) = command {
            self.sequence = self
                .sequence
                .checked_add(1)
                .ok_or("command sequence exhausted")?;
            let submission = GameplayCommandSubmission::new(
                GameplayCommandTick::new(tick.checked_add(1).ok_or("game tick exhausted")?)
                    .ok_or("gameplay command tick is zero")?,
                GameplayCommandIngressSource::external("two-level-game.audio-desktop")
                    .map_err(|error| error.to_string())?,
                GameplayCommandSourceSequence::new(self.sequence)
                    .ok_or("gameplay source sequence is zero")?,
                command,
            );
            self.run()
                .submit(submission)
                .map_err(|error| error.to_string())?;
            if self.run().capture(|_| ()).is_ok() {
                return Err("semantic admission did not invalidate paused capture".into());
            }
        }
        self.control(RuntimeControl::StepFixedTick)?;
        let snapshot = self.settle(allow_game_error)?;
        let after = self
            .run()
            .capture(|world| world.resource::<FixedTime>().tick())
            .map_err(|error| error.to_string())?;
        if after != tick + 1 || self.run().state() != Some(RuntimeState::Paused) {
            return Err(
                "exact Step did not advance exactly one original fixed tick while paused".into(),
            );
        }
        self.exact_steps += 1;
        Ok(snapshot)
    }

    fn audio(&self) -> Result<AudioSnapshot, String> {
        self.observed.borrow().witness.as_ref().map(AudioWitness::snapshot)
            .ok_or_else(|| "audio device did not publish a read-only witness; an available output device is required".into())
    }

    fn wait_audio(
        &mut self,
        predicate: impl Fn(&AudioSnapshot) -> bool,
    ) -> Result<AudioSnapshot, String> {
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        loop {
            self.healthy()?;
            let snapshot = self.audio()?;
            if predicate(&snapshot) {
                return Ok(snapshot);
            }
            self.poll(deadline)?;
        }
    }

    fn controlled(
        &mut self,
        project: &ProjectCopy,
    ) -> Result<(GameSnapshot, ControlEvidence), String> {
        let initial = self.settle(false)?;
        if initial.score != 0
            || initial.logical_tick != 0
            || initial.held != [false; 4]
            || self.audio()?.accepted != 0
        {
            return Err(
                "fresh paused Runtime did not start with an empty game/audio history".into(),
            );
        }
        let mut current = self.step(Some(movement(true)?), false)?;
        for _ in 0..128 {
            if current.score >= 10 {
                break;
            }
            current = self.step(None, false)?;
        }
        if current.score != 10
            || current.collected != 1
            || current.level != Some(LevelKey::main(LevelId::Meadow))
        {
            return Err(
                "actual movement/exact Step did not collect precisely the Meadow pickup".into(),
            );
        }
        let mut evidence = ControlEvidence {
            pickup_step_tick: current.logical_tick,
            ..Default::default()
        };
        evidence.paused_tick = current.logical_tick;
        evidence.paused_output_start = self.audio()?.output_frames;
        let complete = self.wait_audio(|audio| {
            audio.accepted == 1
                && audio.completed == 1
                && audio.output_frames >= evidence.paused_output_start + SAMPLE_RATE / 4
        })?;
        evidence.paused_output_end = complete.output_frames;
        let paused = self.snapshot()?;
        if paused.logical_tick != current.logical_tick
            || paused.logical_elapsed_nanos != current.logical_elapsed_nanos
            || paused.player != current.player
            || complete.started != 1
            || complete.cancelled != 0
        {
            return Err("Pause stopped the one-shot or advanced the gameplay clock".into());
        }
        current = self.step(Some(movement(false)?), false)?;
        for _ in 0..3 {
            current = self.step(None, false)?;
        }
        if self.audio()?.accepted != 1 {
            return Err("repeated pickup observation published a duplicate sound".into());
        }

        self.control(RuntimeControl::Resume)?;
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        loop {
            let tick = self
                .run()
                .observe(|world| world.resource::<GameSnapshot>().logical_tick)
                .map_err(|error| error.to_string())?;
            if tick > current.logical_tick {
                evidence.resumed_tick = tick;
                break;
            }
            self.poll(deadline)?;
        }
        self.control(RuntimeControl::Pause)?;
        current = self.settle(false)?;
        if current.score != 10 || current.held != [false; 4] || self.audio()?.accepted != 1 {
            return Err(
                "Resume replayed the completed pickup or changed released gameplay input".into(),
            );
        }

        project.remove_cavern()?;
        let before = current.clone();
        let refused = self.step(Some(action("two-level-game.travel")?), true)?;
        if refused.error.is_none()
            || refused.level != before.level
            || refused.current_instance != before.current_instance
            || refused.score != before.score
            || refused.travel_count != before.travel_count
            || refused.restarts != before.restarts
        {
            return Err("missing copied Cavern did not refuse actual travel while retaining the incumbent game".into());
        }
        let refused_output = self.audio()?.output_frames;
        let audio =
            self.wait_audio(|audio| audio.output_frames >= refused_output + SAMPLE_RATE / 10)?;
        if audio.accepted != 1 {
            return Err("failed actual travel invented an audio cue".into());
        }
        evidence.failed_travel_error = refused.error.clone();
        evidence.failed_travel = Some(GameEvidence::from(&refused));
        evidence.failed_travel_audio_accepted = audio.accepted;
        project.repair_cavern()?;

        project.change_travel_samples(true)?;
        let changed_bank = self.step(Some(action("two-level-game.travel")?), true)?;
        if !changed_bank
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Sound Bank changed"))
            || changed_bank.current_instance != before.current_instance
            || changed_bank.level != before.level
            || changed_bank.score != before.score
            || changed_bank.player != before.player
            || changed_bank.collected != before.collected
            || changed_bank.markers != before.markers
            || changed_bank.travel_count != before.travel_count
            || self.audio()?.accepted != 1
        {
            return Err(
                "changed Sound Bank did not refuse travel before game/audio publication".into(),
            );
        }
        evidence.changed_bank_error = changed_bank.error.clone();
        evidence.changed_bank_audio_accepted = self.audio()?.accepted;
        project.change_travel_samples(false)?;

        let entered = self.step(Some(action("two-level-game.travel")?), false)?;
        if entered.level != Some(LevelKey::main(LevelId::Cavern))
            || entered.travel_count != 1
            || entered.current_instance == before.current_instance
            || entered.score != 10
        {
            return Err("repaired Cavern did not commit actual paused travel exactly once".into());
        }
        self.wait_audio(|audio| audio.accepted == 2 && audio.started == 2)?;
        evidence.retry_before_instance = entered.current_instance.map(|id| id.get());
        let retried = self.step(Some(action("two-level-game.retry")?), false)?;
        let retired = self.wait_audio(|audio| {
            audio.desired_instance == audio.applied_instance
                && audio.completed + audio.cancelled == audio.accepted
        })?;
        if retried.level != entered.level
            || retried.score != 10
            || retried.restarts != 1
            || retried.travel_count != 1
            || retried.current_instance == entered.current_instance
            || Some(retired.applied_instance) != retried.current_instance.map(|id| id.get())
            || retired.accepted != 2
            || retired.cancelled == 0
        {
            return Err("Retry failed to cancel the old-instance travel one-shot without publishing a new cue".into());
        }
        evidence.retry_after_instance = retried.current_instance.map(|id| id.get());
        evidence.retry_audio_accepted = retired.accepted;
        let saved = self.step(Some(action("two-level-game.save")?), false)?;
        if saved.saves != 1 {
            return Err("ordinary Save did not publish its checkpoint".into());
        }
        let restored = self.step(Some(action("two-level-game.restore")?), false)?;
        if restored.restores != 1
            || restored.saves != 1
            || restored.restarts != 1
            || restored.travel_count != 2
            || restored.score != 10
            || restored.collected != 1
            || restored.level != entered.level
            || restored.current_instance == retried.current_instance
        {
            return Err(
                "ordinary checkpoint reconstruction changed the committed game facts".into(),
            );
        }
        let audio = self.wait_audio(|audio| {
            audio.desired_instance == audio.applied_instance
                && audio.output_frames >= retired.output_frames + SAMPLE_RATE / 10
        })?;
        if audio.accepted != 2
            || Some(audio.applied_instance) != restored.current_instance.map(|id| id.get())
        {
            return Err("Save/Restore reconstruction replayed a game sound".into());
        }
        evidence.save_restore_audio_accepted = audio.accepted;
        let mut final_game = restored;
        for (revision, level) in [LevelId::Meadow, LevelId::Cavern, LevelId::Meadow]
            .into_iter()
            .enumerate()
        {
            project.edit_scene_position(level, revision)?;
            let before = final_game;
            final_game = self.step(Some(action("two-level-game.travel")?), false)?;
            if final_game.level != Some(LevelKey::main(level))
                || final_game.current_instance == before.current_instance
                || final_game.score != before.score
                || final_game.collected != before.collected
                || final_game.travel_count != before.travel_count + 1
                || final_game.restarts != before.restarts
                || final_game.restores != before.restores
            {
                return Err(
                    "unchanged Sound Bank blocked or changed a newly authored Scene revision"
                        .into(),
                );
            }
            let accepted = 3 + revision as u64;
            self.wait_audio(|audio| {
                audio.accepted == accepted && audio.completed + audio.cancelled == accepted
            })?;
            evidence.revised_scene_travels += 1;
        }
        evidence.exact_steps = self.exact_steps;
        Ok((final_game, evidence))
    }

    fn scripted(&mut self) -> Result<GameSnapshot, String> {
        let deadline = Instant::now() + GAME_TIMEOUT;
        loop {
            self.poll(deadline)?;
            let presented = self.observed.borrow().presented.clone();
            if let Some(snapshot) = presented {
                if let Some(error) = &snapshot.error {
                    return Err(error.clone());
                }
                if !snapshot.verified {
                    continue;
                }
                if snapshot.level != Some(LevelKey::main(LevelId::Meadow))
                    || snapshot.score != 30
                    || snapshot.collected != 3
                    || snapshot.visible_pickups != 1
                    || !snapshot.markers.iter().map(|id| id.get()).eq([1])
                    || snapshot.travel_count != 4
                    || snapshot.restarts != 1
                    || snapshot.saves != 0
                    || snapshot.restores != 0
                    || snapshot.held != [false; 4]
                    || snapshot.scene_members != AUTHORED_LEVEL_MEMBERS
                    || snapshot.phase != GamePhase::Running
                    || snapshot.player.to_array() != [-120.0, 0.0]
                {
                    return Err(
                        "original Scenario::Travel presented an incorrect final game".into(),
                    );
                }
                self.wait_audio(|audio| {
                    audio.accepted == 7 && audio.completed + audio.cancelled == 7
                })?;
                return Ok(snapshot);
            }
        }
    }

    fn interactive(&mut self) -> Result<(), String> {
        loop {
            let progress = self.run().poll(Duration::from_millis(16));
            self.healthy()?;
            match progress {
                DesktopRunProgress::Running => {}
                DesktopRunProgress::Failed => {
                    return Err("ordinary audible game failed during startup or execution".into());
                }
                _ => return Ok(()),
            }
        }
    }

    fn retire(&mut self) -> Result<(), String> {
        self.run().request_close();
        let deadline = Instant::now() + CLOSE_TIMEOUT;
        let mut retries = 0;
        loop {
            if Instant::now() >= deadline {
                return Err(
                    "original native cleanup remains retained at its cooperative deadline".into(),
                );
            }
            match self.run().poll(POLL) {
                DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed => return Ok(()),
                DesktopRunProgress::Retiring | DesktopRunProgress::Running => {}
                DesktopRunProgress::CleanupIncomplete if retries < 2 => {
                    retries += 1;
                    self.run().execute();
                }
                progress => {
                    return Err(format!(
                        "original cleanup requires further retained-owner recovery: {progress:?}"
                    ));
                }
            }
            self.observed.borrow_mut().drain();
        }
    }

    fn finish(
        &mut self,
        mut evidence: RunEvidence,
        causes: Option<&[AudioCause]>,
        require_cancellation: bool,
    ) -> (Option<WinitEventLoop>, RunEvidence) {
        if let Err(error) = protected(|| self.retire()) {
            evidence.fail(error);
            if let Err(error) = protected(|| self.retire()) {
                evidence.fail(error);
            }
        }
        evidence.native.runtime_terminal = matches!(
            self.run().progress(),
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
        );
        if self.run().progress() == DesktopRunProgress::Failed {
            let report = self.run().execute();
            evidence.fail(format!(
                "ordinary desktop reported failure: {:?}",
                report.diagnostics()
            ));
        }
        evidence.native.world_unavailable = self.run().observe(|_| ()).is_err();
        let physical = self.run().take_event_loop();
        evidence.native.physical_event_loop_recovered = physical.is_some();
        if evidence.native.runtime_terminal {
            drop(self.run.take());
        }
        let mut observed = self.observed.borrow_mut();
        observed.drain();
        evidence.native.adapter = observed.adapter.clone();
        evidence.native.original_presentations = observed.presentations;
        if let Some(target) = &observed.target {
            match target.snapshot(WindowId::PRIMARY) {
                Ok(target) => {
                    evidence.native.native_destroyed = target.phase
                        == WindowTargetPhase::NativeDestroyed
                        && target.native_destroyed;
                    evidence.native.surface_retired = !target.surface_active;
                    evidence.native.provider_released = !target.provider_present;
                    evidence.native.target_fault_absent = target.fault.is_none();
                }
                Err(error) => evidence.fail(error.to_string()),
            }
        }
        if let Some(error) = &observed.failure {
            evidence.fail(error);
        }
        if self.observer_panicked.get() {
            evidence.fail("original presentation observer panicked");
        }
        if let Some(witness) = &observed.witness {
            let snapshot = witness.snapshot();
            if !snapshot.play_admitted
                || !snapshot.physical_complete
                || !snapshot.stream_drop_returned
                || !snapshot.data_callback_released
                || !snapshot.error_callback_released
                || snapshot.admission_open
                || snapshot.faults != 0
                || snapshot.error.is_some()
            {
                evidence.fail("held read witness did not confirm complete original stream/callback retirement");
            }
            if evidence.failure.is_none()
                && let Some(causes) = causes
            {
                match oracle::verify(
                    &snapshot,
                    &observed.output,
                    causes,
                    &observed.instances,
                    require_cancellation,
                ) {
                    Ok(pcm) => evidence.pcm = Some(pcm),
                    Err(error) => evidence.fail(error),
                }
            }
            evidence.audio = Some(snapshot);
        } else {
            evidence
                .fail("native audio device was unavailable or failed before witness publication");
        }
        let native = &evidence.native;
        if !native.runtime_terminal
            || !native.world_unavailable
            || !native.native_destroyed
            || !native.surface_retired
            || !native.provider_released
            || !native.target_fault_absent
            || !native.physical_event_loop_recovered
            || native.original_presentations == 0
        {
            evidence.fail("original Runtime/window/surface/pump cleanup evidence is incomplete");
        }
        (physical, evidence)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(run) = self.run.take() {
            if matches!(
                run.progress(),
                DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
            ) {
                drop(run);
            } else {
                std::mem::forget(run);
                eprintln!(
                    "audio_desktop: unresolved original native custody retained until process exit"
                );
            }
        }
    }
}

struct ProjectCopy {
    temporary_parent: PathBuf,
    directory: PathBuf,
    project: PathBuf,
    cavern: Vec<u8>,
    meadow: Vec<u8>,
    travel: Vec<u8>,
}

impl ProjectCopy {
    fn new() -> Result<Self, String> {
        let temporary_parent =
            fs::canonicalize(std::env::temp_dir()).map_err(|error| error.to_string())?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory = temporary_parent.join(format!(
            "two-level-native-audio-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let directory = fs::canonicalize(directory).map_err(|error| error.to_string())?;
        if directory.parent() != Some(temporary_parent.as_path()) {
            return Err("temporary project escaped its original TEMP parent".into());
        }
        let project = directory.join("project");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        for relative in [
            "nara.toml",
            "scenes/meadow.scene.json",
            "scenes/cavern.scene.json",
            "assets/textures/meadow.png",
            "assets/textures/meadow.png.meta",
            "assets/textures/cavern.png",
            "assets/textures/cavern.png.meta",
            "assets/textures/explorer.png",
            "assets/textures/explorer.png.meta",
            "assets/audio/pickup.wav",
            "assets/audio/travel.wav",
            "assets/audio/game.soundbank.json",
            "assets/audio/game.soundbank.json.meta",
            "schema/authoring.bundle.json",
            "prefabs/README.md",
        ] {
            let destination = project.join(relative);
            fs::create_dir_all(destination.parent().ok_or("copied file has no parent")?)
                .map_err(|error| error.to_string())?;
            let original = fs::read(source.join(relative)).map_err(|error| error.to_string())?;
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&destination)
                .map_err(|error| error.to_string())?;
            file.write_all(&original)
                .map_err(|error| error.to_string())?;
            if fs::read(destination).map_err(|error| error.to_string())? != original {
                return Err("temporary game content copy differs from the source".into());
            }
        }
        let project = fs::canonicalize(project).map_err(|error| error.to_string())?;
        let original = fs::canonicalize(source).map_err(|error| error.to_string())?;
        if !project.starts_with(&directory)
            || project.starts_with(&original)
            || original.starts_with(&directory)
        {
            return Err("temporary project overlaps the original checkout".into());
        }
        let cavern = fs::read(project.join("scenes/cavern.scene.json"))
            .map_err(|error| error.to_string())?;
        let meadow = fs::read(project.join("scenes/meadow.scene.json"))
            .map_err(|error| error.to_string())?;
        let travel =
            fs::read(project.join("assets/audio/travel.wav")).map_err(|error| error.to_string())?;
        Ok(Self {
            temporary_parent,
            directory,
            project,
            cavern,
            meadow,
            travel,
        })
    }

    fn edit_scene_position(&self, level: LevelId, revision: usize) -> Result<(), String> {
        let path = fs::canonicalize(
            self.project
                .join("scenes")
                .join(level.scene_path().as_str()),
        )
        .map_err(|error| error.to_string())?;
        if !path.starts_with(&self.project) || !path.starts_with(&self.directory) {
            return Err("copied Scene escaped its original temporary project".into());
        }
        let original = match level {
            LevelId::Meadow => &self.meadow,
            LevelId::Cavern => &self.cavern,
        };
        let mut scene: serde_json::Value =
            serde_json::from_slice(original).map_err(|error| error.to_string())?;
        let pickup = scene["payload"]["entities"]
            .as_array_mut()
            .and_then(|entities| entities.iter_mut().find(|entity| entity["id"] == "gem-far"))
            .ok_or("copied level lacks the expected authored pickup")?;
        pickup["components"]["nara.transform.Transform2d"]["value"]["value"]["translation"]["value"]
            ["x"]["value"] = serde_json::json!(81 + revision);
        let bytes = serde_json::to_vec_pretty(&scene).map_err(|error| error.to_string())?;
        fs::write(&path, &bytes).map_err(|error| error.to_string())?;
        if fs::read(&path).map_err(|error| error.to_string())? != bytes {
            return Err("copied Scene edit differs from its requested authored revision".into());
        }
        Ok(())
    }

    fn restore_scene_positions(&self) -> Result<(), String> {
        for (level, bytes) in [
            (LevelId::Meadow, &self.meadow),
            (LevelId::Cavern, &self.cavern),
        ] {
            let path = fs::canonicalize(
                self.project
                    .join("scenes")
                    .join(level.scene_path().as_str()),
            )
            .map_err(|error| error.to_string())?;
            if !path.starts_with(&self.project) || !path.starts_with(&self.directory) {
                return Err("copied Scene escaped its original temporary project".into());
            }
            fs::write(&path, bytes).map_err(|error| error.to_string())?;
            if fs::read(&path).map_err(|error| error.to_string())? != *bytes {
                return Err("repaired copied level differs from its original bytes".into());
            }
        }
        Ok(())
    }

    fn change_travel_samples(&self, changed: bool) -> Result<(), String> {
        let path = fs::canonicalize(self.project.join("assets/audio/travel.wav"))
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&self.project) || !path.starts_with(&self.directory) {
            return Err("copied WAV escaped the original temporary project".into());
        }
        let mut bytes = self.travel.clone();
        if changed {
            let sample = bytes
                .get_mut(48)
                .ok_or("copied WAV lacks its PCM samples")?;
            *sample ^= 1;
        }
        fs::write(&path, &bytes).map_err(|error| error.to_string())?;
        if fs::read(&path).map_err(|error| error.to_string())? != bytes {
            return Err("copied WAV replacement differs from the requested complete bytes".into());
        }
        Ok(())
    }

    fn remove_cavern(&self) -> Result<(), String> {
        let path = fs::canonicalize(self.project.join("scenes/cavern.scene.json"))
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&self.project)
            || !path.starts_with(&self.directory)
            || path.starts_with(
                fs::canonicalize(env!("CARGO_MANIFEST_DIR")).map_err(|error| error.to_string())?,
            )
        {
            return Err("refusing to remove a Cavern outside the validated private copy".into());
        }
        fs::remove_file(path).map_err(|error| error.to_string())
    }

    fn repair_cavern(&self) -> Result<(), String> {
        let parent =
            fs::canonicalize(self.project.join("scenes")).map_err(|error| error.to_string())?;
        if !parent.starts_with(&self.project) {
            return Err("copied Scene directory escaped the temporary project".into());
        }
        let path = parent.join("cavern.scene.json");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        file.write_all(&self.cavern)
            .map_err(|error| error.to_string())?;
        drop(file);
        if fs::read(path).map_err(|error| error.to_string())? != self.cavern {
            return Err("repaired Cavern differs from its original bytes".into());
        }
        Ok(())
    }

    fn remove(self) -> Result<(), String> {
        let actual = fs::canonicalize(&self.directory).map_err(|error| error.to_string())?;
        let original =
            fs::canonicalize(env!("CARGO_MANIFEST_DIR")).map_err(|error| error.to_string())?;
        if actual != self.directory
            || actual.parent() != Some(self.temporary_parent.as_path())
            || original.starts_with(&actual)
            || actual.starts_with(&original)
        {
            return Err("refusing to remove an unverified temporary project directory".into());
        }
        fs::remove_dir_all(actual).map_err(|error| error.to_string())
    }
}

fn protected<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or_else(|payload| {
        std::mem::forget(payload);
        Err("native game operation panicked; original cleanup remains required".into())
    })
}

fn action(id: &str) -> Result<GameplayCommandDraft, String> {
    Ok(GameplayCommandDraft::new(
        GameplayCommandTypeId::new(id).map_err(|error| error.to_string())?,
    ))
}

fn movement(pressed: bool) -> Result<GameplayCommandDraft, String> {
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("direction", GameplayCommandValue::I64(1))
        .map_err(|error| error.to_string())?;
    payload
        .insert("pressed", GameplayCommandValue::Bool(pressed))
        .map_err(|error| error.to_string())?;
    Ok(action("two-level-game.move")?.with_payload(payload))
}

fn causes(scripted: bool) -> Vec<AudioCause> {
    let meadow = LevelKey::main(LevelId::Meadow);
    let cavern = LevelKey::main(LevelId::Cavern);
    let mut causes = vec![
        AudioCause::Pickup {
            level: meadow,
            entity: "gem-near".into(),
        },
        AudioCause::Travel { level: cavern },
    ];
    if scripted {
        let other = LevelKey::new(LevelId::Cavern, 1).expect("fixed game level instance");
        causes.extend([
            AudioCause::Pickup {
                level: cavern,
                entity: "gem-near".into(),
            },
            AudioCause::Travel { level: other },
            AudioCause::Pickup {
                level: other,
                entity: "gem-near".into(),
            },
            AudioCause::Travel { level: cavern },
            AudioCause::Travel { level: meadow },
        ]);
    } else {
        causes.extend([
            AudioCause::Travel { level: meadow },
            AudioCause::Travel { level: cavern },
            AudioCause::Travel { level: meadow },
        ]);
    }
    causes
}

fn verify() -> Report {
    let mut report = Report {
        format: "two-level-native-game-audio/v1",
        accepted: false,
        failure: None,
        runs: Vec::new(),
        fresh_runtime_generation: false,
        retired_generation_unchanged: false,
        physical_event_loop_recovered_and_dropped: false,
        temporary_project_removed: false,
        gameplay_deadline_ms: GAME_TIMEOUT.as_millis(),
        close_deadline_ms: CLOSE_TIMEOUT.as_millis(),
        maximum_capture_frames_per_run: MAX_OUTPUT_FRAMES,
        deadline_scope: "cooperative original polls and native calls; no hard timeout or human-audibility claim",
    };
    let project = match ProjectCopy::new() {
        Ok(project) => project,
        Err(error) => {
            report.failure = Some(error);
            return report;
        }
    };
    let mut event_loop = None;
    let mut retained_witnesses = Vec::new();
    for scripted in [false, true] {
        let mut evidence = RunEvidence::new(if scripted {
            "ordinary_travel_script"
        } else {
            "interactive_commands"
        });
        let options = RunOptions {
            scenario: if scripted {
                Scenario::Travel
            } else {
                Scenario::Interactive
            },
            save_path: (!scripted).then(|| project.directory.join("checkpoint.json")),
        };
        let mut session = match Session::new(
            &project.project,
            options,
            true,
            !scripted,
            event_loop.take(),
        ) {
            Ok(session) => session,
            Err(error) => {
                evidence.fail(error);
                report.runs.push(evidence);
                break;
            }
        };
        let trajectory = protected(|| {
            if scripted {
                session.scripted().map(|game| (game, None))
            } else {
                session
                    .controlled(&project)
                    .map(|(game, controls)| (game, Some(controls)))
            }
        });
        match trajectory {
            Ok((game, controls)) => {
                evidence.game = Some(GameEvidence::from(&game));
                evidence.controls = controls;
            }
            Err(error) => evidence.fail(error),
        }
        if let Some(witness) = session.observed.borrow().witness.clone() {
            retained_witnesses.push(witness);
        }
        let (physical, evidence) = session.finish(evidence, Some(&causes(scripted)), !scripted);
        event_loop = physical;
        let failed = evidence.failure.is_some();
        report.runs.push(evidence);
        if failed {
            break;
        }
        if !scripted && let Err(error) = project.restore_scene_positions() {
            report.failure = Some(error);
            break;
        }
    }
    report.physical_event_loop_recovered_and_dropped = event_loop.is_some();
    drop(event_loop);
    if report.runs.len() == 2 {
        if let (Some(first), Some(second)) = (&report.runs[0].audio, &report.runs[1].audio) {
            report.fresh_runtime_generation =
                first.runtime != 0 && second.runtime != 0 && first.runtime != second.runtime;
        }
    }
    report.retired_generation_unchanged = retained_witnesses.len() == 2
        && retained_witnesses
            .iter()
            .zip(&report.runs)
            .all(|(witness, run)| {
                let now = witness.snapshot();
                run.audio.as_ref().is_some_and(|closed| {
                    now.runtime == closed.runtime
                        && now.output_frames == closed.output_frames
                        && now.callbacks == closed.callbacks
                        && now.accepted == closed.accepted
                        && now.started == closed.started
                        && now.completed == closed.completed
                        && now.cancelled == closed.cancelled
                        && now.records == closed.records
                        && now.faults == 0
                        && now.error.is_none()
                        && now.physical_complete
                        && now.stream_drop_returned
                        && now.data_callback_released
                        && now.error_callback_released
                        && !now.admission_open
                })
            });
    let all_retired = report.runs.iter().all(|run| run.native.runtime_terminal);
    if all_retired {
        match project.remove() {
            Ok(()) => report.temporary_project_removed = true,
            Err(error) => report.failure = Some(error),
        }
    }
    if report.failure.is_none() {
        report.failure = report.runs.iter().find_map(|run| run.failure.clone());
    }
    report.accepted = report.failure.is_none()
        && report.runs.len() == 2
        && report.fresh_runtime_generation
        && report.retired_generation_unchanged
        && report.physical_event_loop_recovered_and_dropped
        && report.temporary_project_removed;
    if !report.accepted && report.failure.is_none() {
        report.failure = Some("native game/audio acceptance evidence is incomplete".into());
    }
    report
}

pub(super) fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let mut verification = false;
    let mut save_path = None;
    while let Some(argument) = args.next() {
        if argument == "--verify" && !verification {
            verification = true;
        } else if argument == "--save" && save_path.is_none() {
            save_path = args.next().map(PathBuf::from);
            if save_path.is_none() {
                return usage();
            }
        } else {
            return usage();
        }
    }
    if save_path.as_ref().is_some_and(|path| !path.is_absolute())
        || (verification && save_path.is_some())
    {
        return usage();
    }
    if verification {
        if !matches!(
            std::env::var("WGPU_BACKEND").as_deref(),
            Ok("dx12" | "vulkan")
        ) || std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() != Ok("0")
        {
            eprintln!(
                "audio_desktop: --verify requires WGPU_BACKEND=dx12 or vulkan and NARA_WGPU_FORCE_FALLBACK=0; a real WASAPI output device is required"
            );
            return ExitCode::FAILURE;
        }
        let report = verify();
        let mut text = match serde_json::to_vec(&report) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("audio_desktop: report serialization failed: {error}");
                return ExitCode::FAILURE;
            }
        };
        text.push(b'\n');
        if report.accepted {
            text.extend_from_slice(b"TWO_LEVEL_AUDIO_DESKTOP_OK gameplay=true independent_pcm=true original_native_retired=true\n");
        }
        if let Err(error) = std::io::stdout().lock().write_all(&text) {
            eprintln!("audio_desktop: report output failed: {error}");
            return ExitCode::FAILURE;
        }
        return if report.accepted {
            ExitCode::SUCCESS
        } else {
            eprintln!(
                "audio_desktop: {}",
                report.failure.as_deref().unwrap_or("acceptance failed")
            );
            ExitCode::FAILURE
        };
    }
    let mut session = match Session::new(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        RunOptions {
            save_path,
            ..Default::default()
        },
        false,
        false,
        None,
    ) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("audio_desktop: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut evidence = RunEvidence::new("interactive");
    if let Err(error) = protected(|| session.interactive()) {
        evidence.fail(error);
    }
    let (physical, evidence) = session.finish(evidence, None, false);
    drop(physical);
    match evidence.failure {
        Some(error) => {
            eprintln!("audio_desktop: {error}");
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: audio_desktop [--save ABSOLUTE_PATH] | --verify");
    ExitCode::FAILURE
}
