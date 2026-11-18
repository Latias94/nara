use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    process::ExitCode,
    rc::Rc,
    time::{Duration, Instant},
};

use nara::{
    app::{FixedTime, FrameExecutionStart, RuntimeControl, RuntimeControlStatus, RuntimeState},
    fs::{ContentDigest, RelativePath},
    project_host::{
        DesktopControlError, DesktopPresentationObservation, DesktopRun, DesktopRunOptions,
        DesktopRunOutcome, DesktopRunProgress, DesktopRunReport, WinitInputActivation,
        open_executable_relative_runtime_pack,
    },
    runtime_content::{
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMemberKind,
        RuntimeContentTarget,
    },
};
use serde_json::{Value, json};
use two_level_game::{
    Scenario,
    animation::{atlas_product, clips_product},
    levels::LevelId,
    markers::marker_product,
};

use super::{
    arguments::Options,
    desktop_output::Record,
    generated, output,
    presentation::{self, Presentation, SharedPresentation},
    verification,
};

const POLL_INTERVAL: Duration = Duration::from_millis(2);
const FIRST_PRESENT_TIMEOUT: Duration = Duration::from_secs(30);
const VERIFICATION_TIMEOUT: Duration = Duration::from_secs(60);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) trait PlayerExtension {
    fn recipe(&self) -> nara::ProductRecipe;

    fn content_profile(&self) -> &'static str;

    fn isolate_input(&self) -> bool {
        false
    }

    fn observe(&mut self, _facts: &DesktopPresentationObservation<'_>) -> Result<(), String> {
        Ok(())
    }

    fn poll(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn completed(&mut self) -> Result<bool, String> {
        Ok(true)
    }

    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn evidence(&self) -> Option<Value> {
        None
    }
}

struct SilentDesktop;

impl PlayerExtension for SilentDesktop {
    fn recipe(&self) -> nara::ProductRecipe {
        two_level_game::cooked_desktop_recipe()
    }

    fn content_profile(&self) -> &'static str {
        "two-level-desktop-shipping"
    }
}

#[allow(dead_code)]
pub(super) fn run(options: Options) -> ExitCode {
    run_with(options, SilentDesktop)
}

pub(super) fn run_with(options: Options, extension: impl PlayerExtension + 'static) -> ExitCode {
    let started = Instant::now();
    let shared = Presentation::new(options.observe, options.game.scenario);
    let extension = Rc::new(RefCell::new(extension));
    let mut original = None;
    let mut save = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        if generated::CONTENT_PROFILE != extension.borrow().content_profile() {
            return Err("compiled content profile does not belong to this Player".into());
        }
        let expected = RuntimeContentExpectation::new(
            ContentDigest::from_parts(generated::CATALOG_LENGTH, generated::CATALOG_HASH),
            RuntimeContentTarget::new(generated::CONTENT_TARGET, generated::CONTENT_PROFILE)
                .map_err(|error| error.to_string())?,
        );
        let base = open_executable_relative_runtime_pack(
            &RelativePath::new(generated::PACK_FILE).map_err(|error| error.to_string())?,
            &expected,
            extension.borrow().recipe(),
            RuntimeContentLimits::default(),
        )
        .map_err(|error| error.to_string())?;
        if base.catalog().startup_scene() != LevelId::STARTUP.scene_product()
            || base.catalog().scenes().len() != LevelId::ALL.len()
            || base.catalog().members().len() != 9
            || base
                .catalog()
                .members()
                .iter()
                .filter(|member| matches!(member.kind(), RuntimeContentMemberKind::Scene { .. }))
                .count()
                != 3
            || !base.catalog().addressable_roots().is_empty()
            || base.plan().settings().input().action_map
                != Some(two_level_game::input::input_product())
            || base.member(atlas_product()).is_none()
            || base.member(clips_product()).is_none()
            || base.member(marker_product()).is_none_or(|member| {
                member.products() != [marker_product()]
                    || !matches!(member.kind(), RuntimeContentMemberKind::Scene { .. })
                    || !member.dependencies().is_empty()
            })
            || base
                .member(two_level_game::sound_bank::bank_product())
                .is_none()
            || LevelId::ALL.into_iter().any(|level| {
                !base.catalog().scenes().contains(&level.scene_product())
                    || base.member(level.image_product()).is_none()
            })
        {
            return Err(
                "compiled desktop pack does not contain the game's exact two-level Product catalog"
                    .into(),
            );
        }
        let observed = Rc::clone(&shared);
        let observer_extension = Rc::clone(&extension);
        let execution = DesktopRunOptions::new()
            .with_initial_pause()
            .with_input_isolation(extension.borrow().isolate_input())
            .with_presentation_observer(move |facts| {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    observer_extension.borrow_mut().observe(&facts)
                }));
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        observed.borrow_mut().failure.get_or_insert(error);
                    }
                    Err(panic) => {
                        let mut state = observed.borrow_mut();
                        let detail = presentation::panic_detail(panic.as_ref());
                        state.failure.get_or_insert_with(|| {
                            format!("Player extension observer panicked: {detail}")
                        });
                        state.panic.get_or_insert(panic);
                    }
                }
                presentation::observer(&observed, facts)
            });
        let run = original.insert(DesktopRun::from_runtime_content(base, execution));
        drive(run, &shared, &options, started, &extension)?;
        save = verification::save(&options.game)?;
        Ok(())
    }));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            shared.borrow_mut().failure.get_or_insert(error);
        }
        Err(panic) => {
            let mut state = shared.borrow_mut();
            let detail = presentation::panic_detail(panic.as_ref());
            state
                .failure
                .get_or_insert_with(|| format!("desktop Player panicked: {detail}"));
            if state.panic.is_none() {
                state.panic = Some(panic);
            }
        }
    }
    let had_original = original.is_some();
    let report = original
        .as_mut()
        .map(|run| close_original(run, &shared, &extension));
    let closed = original.as_ref().is_some_and(|run| {
        matches!(
            run.progress(),
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
        )
    });
    let pump_returned = match catch_unwind(AssertUnwindSafe(|| {
        let event_loop = original.as_mut().and_then(DesktopRun::take_event_loop);
        let returned = event_loop.is_some();
        drop(event_loop);
        drop(original.take());
        returned
    })) {
        Ok(returned) => returned,
        Err(_panic) => retain_until_exit(
            "original desktop owner or returned event loop panicked during destruction",
        ),
    };
    let mut state = shared.borrow_mut();
    let mut message = state.failure.clone();
    let mut panic = state.panic.take();
    let cleanup_complete = !had_original || (closed && pump_returned);
    let diagnostics = report
        .as_ref()
        .map(|report| output::bounded(&format!("{:?}", report.diagnostics())));
    if message.is_none()
        && report.as_ref().is_some_and(|report| {
            !matches!(report.outcome(), DesktopRunOutcome::Completed(_))
                || report.diagnostics().has_errors()
                || report.startup_content_closure().is_some()
        })
    {
        message = Some(
            "original processed desktop Runtime physically closed with failure diagnostics".into(),
        );
    }
    if message.is_none() && (!closed || !pump_returned || !state.ready) {
        message = Some("desktop Player did not complete its original presentation, Resume and physical retirement".into());
    }
    let finished = catch_unwind(AssertUnwindSafe(|| extension.borrow_mut().finish()));
    match finished {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            message.get_or_insert(error);
        }
        Err(payload) => {
            message.get_or_insert_with(|| {
                format!(
                    "Player extension retirement verification panicked: {}",
                    presentation::panic_detail(payload.as_ref())
                )
            });
            panic.get_or_insert(payload);
        }
    }
    if let Some(message) = &message {
        let _ = output::diagnostic_to(&mut std::io::stderr().lock(), message);
    }
    let mut record = state.record(options.game.scenario, message.as_deref(), save);
    record["original_runtime_retired"] = json!(closed && pump_returned);
    record["original_pump_returned"] = json!(pump_returned);
    record["cleanup_complete"] = json!(cleanup_complete);
    record["runtime_diagnostics"] = diagnostics.into();
    if let Some(evidence) = extension.borrow().evidence() {
        record["audio"] = evidence;
    }
    let written = state.emitter.emit(Record::Result, record);
    if let Err(error) = &written {
        state.failure.get_or_insert_with(|| error.clone());
        let _ = output::diagnostic_to(&mut std::io::stderr().lock(), error);
    }
    drop(state);
    if let Some(panic) = panic {
        resume_unwind(panic);
    }
    if message.is_none() && written.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn drive(
    original: &mut DesktopRun,
    shared: &SharedPresentation,
    options: &Options,
    started: Instant,
    extension: &Rc<RefCell<impl PlayerExtension>>,
) -> Result<(), String> {
    let first_deadline = started + FIRST_PRESENT_TIMEOUT;
    wait_initial_pause(original, shared, first_deadline, extension)?;
    let activate_input = extension.borrow().isolate_input();
    if activate_input {
        let bindings = two_level_game::movement_keyboard_bindings(original)
            .map_err(|error| error.to_string())?;
        let holds = original
            .isolate_keyboard_input(&bindings)
            .map_err(|error| error.to_string())?;
        if holds.iter().any(|hold| hold.held) {
            return Err(
                "audio verification requires a neutral initial game keyboard inventory".into(),
            );
        }
        wait_initial_pause(original, shared, first_deadline, extension)?;
        original
            .request_input_activation()
            .map_err(|error| error.to_string())?;
        loop {
            if let Some(error) = original.input_activation_error() {
                return Err(format!("original Player input activation failed: {error}"));
            }
            match original.input_activation() {
                Some(WinitInputActivation::Active) => break,
                Some(
                    WinitInputActivation::AwaitingFocus
                    | WinitInputActivation::ReconciliationPending,
                ) => {
                    poll_startup(original, shared, first_deadline, extension)?;
                }
                state => {
                    return Err(format!(
                        "original Player input activation did not remain live: {state:?}"
                    ));
                }
            }
        }
        wait_initial_pause(original, shared, first_deadline, extension)?;
        if original
            .capture_keyboard_holds(&bindings)
            .map_err(|error| error.to_string())?
            .iter()
            .any(|hold| hold.held)
        {
            return Err(
                "original Player input activation changed the neutral game inventory".into(),
            );
        }
    }
    if let Some(command) = two_level_game::player_launch(options.game.clone())? {
        original
            .submit(command)
            .map_err(|error| error.to_string())?;
        shared.borrow_mut().launch_required = true;
    }
    shared.borrow_mut().launch_admitted = true;
    let ticket = original
        .request_control(RuntimeControl::Resume)
        .map_err(|error| error.to_string())?;
    if Some(ticket.generation()) != original.runtime_generation() {
        return Err("Resume ticket does not belong to the original desktop Runtime".into());
    }
    loop {
        poll_startup(original, shared, first_deadline, extension)?;
        match original.control_status(ticket) {
            Some(RuntimeControlStatus::Pending) => {}
            Some(RuntimeControlStatus::Applied)
                if original.state() == Some(RuntimeState::Running) =>
            {
                shared.borrow_mut().resume_applied = true;
                break;
            }
            status => return Err(format!("original desktop Resume failed: {status:?}")),
        }
    }
    let verification_deadline = started + VERIFICATION_TIMEOUT;
    loop {
        {
            let state = shared.borrow();
            if let Some(error) = &state.failure {
                return Err(error.clone());
            }
            if state.ready && options.verify_first_present {
                return Ok(());
            }
            if state.ready
                && options.game.scenario != Scenario::Interactive
                && verification::completed(&state.evidence, options.game.scenario)?
                && extension.borrow_mut().completed()?
            {
                return Ok(());
            }
            if !state.ready && Instant::now() >= first_deadline {
                return Err(
                    "desktop Player exceeded its 30-second first-presentation deadline".into(),
                );
            }
        }
        if options.game.scenario != Scenario::Interactive && Instant::now() >= verification_deadline
        {
            return Err("desktop Player exceeded its 60-second verification deadline".into());
        }
        let progress = original.poll(POLL_INTERVAL);
        extension.borrow_mut().poll()?;
        let state = shared.borrow();
        if let Some(error) = &state.failure {
            return Err(error.clone());
        }
        match progress {
            DesktopRunProgress::Running => {}
            DesktopRunProgress::Retiring | DesktopRunProgress::Completed(_)
                if state.ready
                    && options.game.scenario == Scenario::Interactive
                    && !options.verify_first_present =>
            {
                return Ok(());
            }
            progress => {
                return Err(format!(
                    "original desktop execution stopped before completion: {progress:?}"
                ));
            }
        }
    }
}

fn wait_initial_pause(
    original: &mut DesktopRun,
    shared: &SharedPresentation,
    deadline: Instant,
    extension: &Rc<RefCell<impl PlayerExtension>>,
) -> Result<(), String> {
    loop {
        poll_startup(original, shared, deadline, extension)?;
        if original.state() != Some(RuntimeState::Paused) {
            continue;
        }
        let frame = match original.capture(|world| {
            world
                .get_resource::<FixedTime>()
                .zip(world.get_resource::<FrameExecutionStart>())
                .map(|(fixed, frame)| (fixed.tick(), frame.frame()))
        }) {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                return Err(
                    "original paused desktop World has no complete clock/frame boundary".into(),
                );
            }
            Err(DesktopControlError::NotPaused) => continue,
            Err(error) => return Err(error.to_string()),
        };
        if frame.0 != 0 {
            return Err(
                "desktop Player advanced a fixed tick before first-tick launch admission".into(),
            );
        }
        let state = shared.borrow();
        if state.frame.as_ref().is_some_and(|presented| {
            Some(presented.runtime_generation)
                == original
                    .runtime_generation()
                    .map(|generation| generation.get())
                && presented.app_frame == frame.1
        }) {
            return Ok(());
        }
    }
}

fn poll_startup(
    original: &mut DesktopRun,
    shared: &SharedPresentation,
    deadline: Instant,
    extension: &Rc<RefCell<impl PlayerExtension>>,
) -> Result<(), String> {
    if Instant::now() >= deadline {
        return Err("desktop Player exceeded its 30-second first-presentation deadline".into());
    }
    let progress =
        original.poll(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    extension.borrow_mut().poll()?;
    if let Some(error) = &shared.borrow().failure {
        return Err(error.clone());
    }
    if progress != DesktopRunProgress::Running {
        return Err(format!("original desktop startup stopped: {progress:?}"));
    }
    if Instant::now() >= deadline {
        return Err("desktop Player exceeded its 30-second first-presentation deadline".into());
    }
    Ok(())
}

fn close_original(
    original: &mut DesktopRun,
    shared: &SharedPresentation,
    extension: &Rc<RefCell<impl PlayerExtension>>,
) -> DesktopRunReport {
    if catch_unwind(AssertUnwindSafe(|| original.request_close())).is_err() {
        retain_until_exit(
            "original desktop close request panicked; retaining its owner until process exit",
        );
    }
    let deadline = Instant::now() + CLEANUP_TIMEOUT;
    loop {
        let drained = catch_unwind(AssertUnwindSafe(|| extension.borrow_mut().poll()));
        match drained {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                shared.borrow_mut().failure.get_or_insert(error);
            }
            Err(payload) => {
                let mut state = shared.borrow_mut();
                state.failure.get_or_insert_with(|| {
                    format!(
                        "Player extension cleanup observation panicked: {}",
                        presentation::panic_detail(payload.as_ref())
                    )
                });
                state.panic.get_or_insert(payload);
            }
        }
        let progress = original.progress();
        if matches!(
            progress,
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
        ) {
            return match catch_unwind(AssertUnwindSafe(|| original.execute())) {
                Ok(report) => report,
                Err(_panic) => retain_until_exit(
                    "original desktop terminal report panicked; retaining its owner until process exit",
                ),
            };
        }
        if Instant::now() >= deadline {
            retain_until_exit(
                "original desktop cleanup exceeded 10 seconds; retaining its owner until process exit",
            );
        }
        let advanced = catch_unwind(AssertUnwindSafe(|| {
            if progress == DesktopRunProgress::CleanupIncomplete {
                original.execute();
            } else {
                original.poll(POLL_INTERVAL);
            }
        }));
        if let Err(_panic) = advanced {
            retain_until_exit(
                "original desktop cleanup panicked; retaining its owner until process exit",
            );
        }
    }
}

fn retain_until_exit(message: &str) -> ! {
    let _ = output::diagnostic_to(&mut std::io::stderr().lock(), message);
    std::process::exit(1)
}
