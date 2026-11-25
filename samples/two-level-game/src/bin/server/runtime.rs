use std::{
    collections::BTreeSet,
    io::{self, Write},
    num::NonZeroU64,
    panic::{AssertUnwindSafe, catch_unwind},
    process::ExitCode,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::CurrentScene,
    app::{FixedCatchUpPolicy, FixedTime},
    fs::{ContentDigest, RelativePath},
    identity::EntityLookup,
    project::{ProductCapability, ProductCapabilitySet, RuntimePreset},
    project_host::{
        HeadlessSession, HeadlessSessionClose, HeadlessSessionError,
        open_executable_relative_runtime_pack,
    },
    runtime_content::{
        RuntimeContentBase, RuntimeContentExpectation, RuntimeContentLimits,
        RuntimeContentMemberKind, RuntimeContentTarget,
    },
};
use serde_json::{Value, json};
use two_level_game::{
    GamePhase, GameSnapshot, RunOptions, Scenario,
    command::{
        CommandKey, CommandSequence, CommandSource, CommandSubmitError, CommandTick, GameCommand,
        MAX_COMMAND_TICK_OFFSET, SERVER_COMMAND_SOURCE, ScheduledCommand,
    },
    delivery::{PlayerSelection, RuntimeKind},
    levels::{LevelId, LevelKey},
    markers::marker_product,
    save::{CURRENT_FORMAT_VERSION, io::SaveSlot},
    side_room,
};

use super::{
    Invocation, RESULT, Report, clock::FrameOverrun, generated, output::Output, regions, service,
};

pub(super) const MAX_FRAMES: usize = 200_000;

pub(super) struct ScenarioProgress {
    latest: GameSnapshot,
    visited_levels: BTreeSet<LevelId>,
}

impl ScenarioProgress {
    fn new(latest: GameSnapshot) -> Self {
        Self {
            visited_levels: latest.level.map(LevelKey::level).into_iter().collect(),
            latest,
        }
    }

    pub(super) fn latest(&self) -> &GameSnapshot {
        &self.latest
    }

    pub(super) fn step(
        &mut self,
        session: &mut HeadlessSession,
        deadline: Instant,
    ) -> Result<&GameSnapshot, Failure> {
        if Instant::now() >= deadline {
            return Err("Server exceeded its 60-second execution deadline".into());
        }
        if session.step()?.is_some() {
            return Err("Server exited before the finite scenario completed".into());
        }
        self.latest = snapshot(session)?;
        if let Some(level) = self.latest.level {
            self.visited_levels.insert(level.level());
        }
        check_gameplay(&self.latest)?;
        Ok(&self.latest)
    }
}

pub(super) enum Failure {
    Rejected(String),
    Error(String),
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        Self::Error(message.to_owned())
    }
}

impl From<HeadlessSessionError> for Failure {
    fn from(error: HeadlessSessionError) -> Self {
        match &error {
            HeadlessSessionError::Runtime(diagnostics) => {
                Self::Error(format!("{error}: {diagnostics:?}"))
            }
            _ => Self::Error(error.to_string()),
        }
    }
}

impl From<CommandSubmitError> for Failure {
    fn from(error: CommandSubmitError) -> Self {
        match error {
            CommandSubmitError::Rejected(error) => Self::Rejected(error.to_string()),
            CommandSubmitError::Headless(error) => Self::from(error),
            CommandSubmitError::Runtime { output, fault } => Self::Error(format!(
                "command admission fault: outcome={output:?}, fault={fault:?}"
            )),
            #[cfg(feature = "desktop")]
            CommandSubmitError::Desktop(error) => Self::Error(error.to_string()),
        }
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Error(error.to_string())
    }
}

impl From<FrameOverrun> for Failure {
    fn from(error: FrameOverrun) -> Self {
        Self::Error(error.to_string())
    }
}

pub(super) fn run(invocation: Invocation) -> ExitCode {
    let mut report = Report::new(Some(invocation.name()), "open_pack");
    let output = if matches!(&invocation, Invocation::Serve(_)) {
        match Output::spawn() {
            Ok(output) => Some(output),
            Err(error) => return report.emit("error", Some(&error.to_string())),
        }
    } else {
        None
    };
    let mut session = None;
    let mut scenario_progress = None;
    let deadline = Instant::now() + Duration::from_secs(60);
    let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<(), Failure> {
        let base = open_pack().map_err(Failure::Rejected)?;
        report.stage = "startup";
        let original = session.insert(HeadlessSession::from_runtime_content(base));
        original
            .start()
            .map_err(|error| match Failure::from(error) {
                Failure::Error(message) | Failure::Rejected(message) => Failure::Rejected(message),
            })?;
        if Instant::now() >= deadline || original.frame(Duration::ZERO)?.is_some() {
            return Err("Server did not complete its initial maintenance frame".into());
        }
        let initial = snapshot(original)?;
        check_gameplay(&initial)?;
        if initial.tick != 0
            || !initial.initialized
            || initial.phase != GamePhase::Running
            || initial.level != Some(LevelKey::main(LevelId::Meadow))
        {
            return Err(Failure::Rejected(
                "Server did not initialize Meadow at tick zero".into(),
            ));
        }
        verify_runtime(original, &initial)?;
        if !matches!(&invocation, Invocation::Serve(_)) {
            report.ready = true;
            report.stage = "scenario";
        }
        match invocation {
            Invocation::Player(options) => {
                let progress = scenario_progress.insert(ScenarioProgress::new(initial));
                run_player(original, options, deadline, progress, &mut report)?;
            }
            Invocation::Regions => {
                let progress = scenario_progress.insert(ScenarioProgress::new(initial));
                prepare_verification(original)?;
                let evidence = regions::verify(original, progress, deadline)?;
                verify_runtime(original, progress.latest())?;
                report.server_evidence = Some(evidence);
            }
            Invocation::Serve(options) => {
                report.state = Some(state(&initial));
                service::run(
                    original,
                    options,
                    deadline,
                    output
                        .as_ref()
                        .expect("Server service owns the output writer"),
                    &mut report,
                )?;
            }
        }
        Ok(())
    }));

    let failed_diagnostics = if let Some(original) = &mut session {
        report.close = close_original(original);
        report.cleanup_complete = matches!(report.close, "complete" | "failed");
        let failed = original.diagnostics().has_errors();
        if failed && output.is_none() {
            let _ = writeln!(
                io::stderr().lock(),
                "two_level_server: {:?}",
                original.diagnostics()
            );
        }
        failed
    } else {
        false
    };
    if let Some(progress) = scenario_progress {
        report.state = Some(state(&progress.latest));
        report.visited_levels = json!(progress.visited_levels);
    }
    let (status, message) = match outcome {
        Ok(Ok(())) if report.close == "complete" && !failed_diagnostics => {
            report.stage = "complete";
            ("ok", None)
        }
        Ok(Ok(())) => {
            report.stage = "cleanup";
            (
                "error",
                Some("original Headless session failed to close cleanly".into()),
            )
        }
        Ok(Err(Failure::Rejected(message))) => ("rejected", Some(message)),
        Ok(Err(Failure::Error(message))) => ("error", Some(message)),
        Err(panic) => {
            let detail = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic payload");
            ("error", Some(format!("Server panicked: {detail}")))
        }
    };
    let cleanup_complete = report.cleanup_complete;
    let exit = if let Some(output) = output {
        let result = report.into_record(status, message.as_deref());
        if output.finish(RESULT, result).is_ok() && status == "ok" {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    } else {
        report.emit(status, message.as_deref())
    };
    if cleanup_complete {
        exit
    } else {
        ExitCode::FAILURE
    }
}

fn open_pack() -> Result<RuntimeContentBase, String> {
    let selection = PlayerSelection::from_content_profile(generated::CONTENT_PROFILE)?;
    if selection.runtime() != RuntimeKind::Server {
        return Err("compiled content profile does not belong to the Server".into());
    }
    selection.validate_target(generated::CONTENT_TARGET)?;
    let target = RuntimeContentTarget::new(generated::CONTENT_TARGET, generated::CONTENT_PROFILE)
        .map_err(|error| error.to_string())?;
    let expected = RuntimeContentExpectation::new(
        ContentDigest::from_parts(generated::CATALOG_LENGTH, generated::CATALOG_HASH),
        target,
    );
    let base = open_executable_relative_runtime_pack(
        &RelativePath::new(generated::PACK_FILE).map_err(|error| error.to_string())?,
        &expected,
        selection.recipe()?,
        RuntimeContentLimits::default(),
    )
    .map_err(|error| error.to_string())?;
    let catalog = base.catalog();
    let plan = base.plan();
    let settings = plan.settings();
    let expected_scenes = [
        LevelId::Meadow.scene_product(),
        LevelId::Cavern.scene_product(),
        marker_product(),
        side_room::scene_product(),
        side_room::lookout_scene_product(),
    ];
    if settings.runtime_preset() != RuntimePreset::Server
        || settings.window().enabled
        || settings.input().action_map.is_some()
        || plan.normalized_capabilities()
            != ProductCapabilitySet::singleton(ProductCapability::RuntimeCore)
        || catalog.startup_scene() != LevelId::Meadow.scene_product()
        || catalog.scenes().len() != LevelId::ALL.len()
        || !LevelId::ALL
            .into_iter()
            .all(|level| catalog.scenes().contains(&level.scene_product()))
        || !catalog.addressable_roots().is_empty()
        || catalog.members().len() != expected_scenes.len()
        || expected_scenes.into_iter().any(|product| {
            !base.member(product).is_some_and(|member| {
                matches!(member.kind(), RuntimeContentMemberKind::Scene { .. })
            })
        })
    {
        return Err(
            "compiled Server catalog differs from its gameplay-only Scene selection".into(),
        );
    }
    Ok(base)
}

fn verify_runtime(session: &HeadlessSession, snapshot: &GameSnapshot) -> Result<(), Failure> {
    session.observe(|world| -> Result<(), Failure> {
        #[cfg(feature = "local-player")]
        super::client_services::verify_absent(world)?;
        if world.resource::<FixedTime>().catch_up_policy() != FixedCatchUpPolicy::PreserveDebt {
            return Err("Server World lost its fixed-time debt policy".into());
        }
        let current = CurrentScene::get(world).ok_or("Server has no live Scene receipt")?;
        let receipt = current.receipt();
        let processed = current.source_view().with_runtime_scene(|candidate| {
            let entities = candidate.document().entities();
            receipt.len() == entities.len()
                && entities.iter().all(|entity| {
                    matches!(
                        receipt.resolve(world, entity.id()),
                        EntityLookup::Resolved(_)
                    )
                })
        }) == Some(true);
        #[cfg(feature = "local-player")]
        let processed = processed && current.source_content_stamp().is_none();
        if !processed || Some(receipt.instance_id()) != snapshot.current_instance {
            return Err("Server did not retain the live processed Scene members".into());
        }
        Ok(())
    })?
}

fn run_player(
    session: &mut HeadlessSession,
    options: RunOptions,
    deadline: Instant,
    progress: &mut ScenarioProgress,
    report: &mut Report,
) -> Result<(), Failure> {
    let Some(command) =
        two_level_game::player_launch(options.clone()).map_err(Failure::Rejected)?
    else {
        return Ok(());
    };
    prepare_verification(session)?;
    command.submit_headless(session)?;
    let mut restored = None;
    for _ in 0..MAX_FRAMES {
        progress.step(session, deadline)?;
        let snapshot = progress.latest();
        if snapshot.restores == 1 && restored.is_none() {
            restored = Some(snapshot.clone());
        }
        if snapshot.verified {
            verify_runtime(session, snapshot)?;
            let launch = session.observe(|world| {
                two_level_game::player_launch_result(world).is_some_and(|result| result.is_ok())
            })?;
            if !launch {
                return Err("Server did not apply the original first-tick launch".into());
            }
            let (level, marker) = if options.scenario == Scenario::Save {
                (LevelId::Cavern, 2)
            } else {
                (LevelId::Meadow, 1)
            };
            if !snapshot.initialized
                || snapshot.phase != GamePhase::Running
                || snapshot.restoring
                || progress.visited_levels.len() != LevelId::ALL.len()
                || snapshot.level != Some(LevelKey::main(level))
                || snapshot.score != 30
                || snapshot.collected != 3
                || !snapshot.markers.iter().map(|id| id.get()).eq([marker])
                || (options.scenario == Scenario::Save && snapshot.saves != 1)
                || (options.scenario == Scenario::Restore && snapshot.restores != 1)
            {
                return Err(
                    "completed Server scenario disagrees with the original game progress".into(),
                );
            }
            report.checkpoint = checkpoint(&options, restored.as_ref())?;
            return Ok(());
        }
        std::thread::yield_now();
    }
    Err("Server exhausted its finite 200000-frame budget".into())
}

fn prepare_verification(session: &mut HeadlessSession) -> Result<(), Failure> {
    if session.pause()?.is_some() {
        return Err("Server exited before entering exact-tick verification".into());
    }
    Ok(())
}

fn snapshot(session: &HeadlessSession) -> Result<GameSnapshot, Failure> {
    Ok(session.observe(|world| world.resource::<GameSnapshot>().clone())?)
}

fn check_gameplay(snapshot: &GameSnapshot) -> Result<(), Failure> {
    if let Some(error) = &snapshot.error {
        return Err(Failure::Rejected(format!(
            "gameplay rejected the scenario: {error}"
        )));
    }
    Ok(())
}

pub(super) fn submit_external(
    session: &mut HeadlessSession,
    last_sequence: &mut u64,
    intent: GameCommand,
) -> Result<CommandKey, Failure> {
    submit_external_after(session, last_sequence, intent, NonZeroU64::MIN)
}

pub(super) fn submit_external_after(
    session: &mut HeadlessSession,
    last_sequence: &mut u64,
    intent: GameCommand,
    after_ticks: NonZeroU64,
) -> Result<CommandKey, Failure> {
    let current_tick = session.observe(|world| world.resource::<FixedTime>().tick())?;
    if after_ticks.get() > MAX_COMMAND_TICK_OFFSET {
        return Err(Failure::Rejected(format!(
            "Server command after_ticks exceeds the configured queue horizon of {MAX_COMMAND_TICK_OFFSET}"
        )));
    }
    let tick = current_tick
        .checked_add(after_ticks.get())
        .and_then(CommandTick::new)
        .ok_or("Server tick cannot accept another command")?;
    let sequence = last_sequence
        .checked_add(1)
        .and_then(CommandSequence::new)
        .ok_or("Server command sequence exhausted")?;
    let source =
        CommandSource::external(SERVER_COMMAND_SOURCE).expect("static Server command source");
    let result = ScheduledCommand::new(tick, source, sequence, intent).submit_headless(session);
    if matches!(
        &result,
        Ok(_) | Err(CommandSubmitError::Runtime { output: Ok(_), .. })
    ) {
        *last_sequence = sequence.get();
    }
    result.map_err(Into::into)
}

fn checkpoint(
    options: &RunOptions,
    restored: Option<&GameSnapshot>,
) -> Result<Option<Value>, Failure> {
    let operation = match options.scenario {
        Scenario::Save => "save",
        Scenario::Restore => "restore",
        Scenario::Travel | Scenario::Interactive => return Ok(None),
    };
    let path = options
        .save_path
        .as_deref()
        .ok_or("checkpoint path was not provided")?;
    let mut slot = SaveSlot::open(path)
        .map_err(|error| Failure::Error(format!("checkpoint open: {error}")))?;
    let decoded = slot
        .load()
        .map_err(|error| Failure::Error(format!("checkpoint readback: {error}")))?;
    let saved = &decoded.checkpoint;
    if options.scenario == Scenario::Save {
        if decoded.migrated_from.is_some()
            || saved.location.level != LevelKey::main(LevelId::Cavern)
            || saved.score != 30
            || saved.collected.len() != 3
            || !saved
                .markers
                .iter()
                .map(|marker| marker.id.get())
                .eq([1, 2, 3])
            || saved.next_marker_id.get() != 4
        {
            return Err("published checkpoint disagrees with the actual Save trajectory".into());
        }
    } else {
        let restored = restored.ok_or("Server never observed the checkpoint restoration")?;
        let saved_markers = saved
            .markers
            .iter()
            .filter_map(|marker| (marker.level == saved.location.level).then_some(marker.id));
        if restored.level != Some(saved.location.level)
            || restored.score != saved.score
            || restored.collected != saved.collected.len()
            || !restored.markers.iter().copied().eq(saved_markers)
        {
            return Err("checkpoint readback disagrees with the observed restoration".into());
        }
    }
    Ok(Some(json!({
        "operation": operation,
        "path": path,
        "format_version": CURRENT_FORMAT_VERSION,
        "migrated_from": decoded.migrated_from,
        "level": saved.location.level,
        "score": saved.score,
        "collected": saved.collected.len(),
        "markers": saved.markers,
        "next_marker_id": saved.next_marker_id.get(),
        "restored_state": restored.map(state),
    })))
}

pub(super) fn state(snapshot: &GameSnapshot) -> Value {
    json!({
        "tick": snapshot.tick,
        "logical_tick": snapshot.logical_tick,
        "level": snapshot.level,
        "player": snapshot.player.to_array(),
        "score": snapshot.score,
        "collected": snapshot.collected,
        "visible_pickups": snapshot.visible_pickups,
        "markers": snapshot.markers,
        "travel_count": snapshot.travel_count,
        "restarts": snapshot.restarts,
        "saves": snapshot.saves,
        "restores": snapshot.restores,
        "phase": format!("{:?}", snapshot.phase),
        "initialized": snapshot.initialized,
        "verified": snapshot.verified,
        "scene_members": snapshot.scene_members,
        "held": snapshot.held,
        "error": snapshot.error.as_deref().map(|error| error.chars().take(2048).collect::<String>()),
        "restoring": snapshot.restoring,
        "side_room": snapshot.side_room,
        "lookout": snapshot.lookout,
    })
}

fn close_original(session: &mut HeadlessSession) -> &'static str {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match catch_unwind(AssertUnwindSafe(|| session.close())) {
            Ok(HeadlessSessionClose::Complete) => return "complete",
            Ok(HeadlessSessionClose::Failed) => return "failed",
            Ok(HeadlessSessionClose::Pending) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(HeadlessSessionClose::Pending) => return "pending",
            Err(_) => return "panicked",
        }
    }
}
