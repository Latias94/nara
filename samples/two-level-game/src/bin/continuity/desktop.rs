//! The source-desktop placement retains one main-thread product, native pump and GPU Host.
//!
//! All waits below are cooperative. Their deadlines cannot interrupt callbacks, synchronous
//! startup, drivers or native APIs. Pending physical close keeps this original run for another
//! explicit Close; the outer process-exit fallback retains a live owner instead of dropping it.

use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    time::{Duration, Instant},
};

use nara::{
    app::{
        FrameExecutionStart, RuntimeControl, RuntimeControlStatus, RuntimeControlTicket,
        RuntimeState,
    },
    fs::DirectoryCapability,
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick,
    },
    input::{ButtonDriverInput, KeyCode, KeyboardHold},
    project_host::{
        DesktopControlError, DesktopPresentationControl, DesktopRun, DesktopRunIntent,
        DesktopRunOptions, DesktopRunProgress, WinitInputActivation,
    },
};
use two_level_game::{
    GamePhase, GameSnapshot, capture_desktop_resume,
    development::presentation::{DevelopmentPresentationPlugin, join_presented, state_digest},
    movement_keyboard_bindings, restore_desktop_resume,
    resume::{ResumeState, encode_resume},
};

use super::protocol::{Command, DesktopPresentation, InputActivation, Reply, SemanticCommand};
use super::{POLL_INTERVAL, check_deadline, command_draft, observed_snapshot};

const MAX_PRESENTATIONS: usize = 8;

#[derive(Default)]
struct Presentations {
    frames: VecDeque<DesktopPresentation>,
    failure: Option<String>,
}

pub(super) struct DesktopGame {
    run: DesktopRun,
    sequence: u64,
    execution_failed: bool,
    closing: bool,
    presentations: Rc<RefCell<Presentations>>,
}

impl DesktopGame {
    pub(super) fn new(root: DirectoryCapability) -> Result<Self, String> {
        let presentations = Rc::new(RefCell::new(Presentations::default()));
        let observed = Rc::clone(&presentations);
        let options = DesktopRunOptions::new()
            .with_initial_pause()
            .with_initial_visibility(false)
            .with_input_isolation(true)
            .with_presentation_observer(move |observation| {
                let result = join_presented(observation);
                let mut state = observed.borrow_mut();
                match result {
                    Ok(Some(frame)) => {
                        if state.frames.len() == MAX_PRESENTATIONS {
                            state.frames.pop_front();
                        }
                        state.frames.push_back(frame);
                    }
                    Ok(None) => {}
                    Err(error) => state.failure = Some(error),
                }
                DesktopPresentationControl::Continue
            });
        let recipe = two_level_game::product_recipe()
            .add_plugin::<DevelopmentPresentationPlugin>()
            .map_err(|error| format!("compose source-desktop presentation witness: {error}"))?;
        Ok(Self {
            run: DesktopRun::new(
                root,
                DesktopRunIntent::new()
                    .with_profile("desktop")
                    .with_recipe(recipe)
                    .with_execution(options),
            ),
            sequence: 0,
            execution_failed: false,
            closing: false,
            presentations,
        })
    }

    pub(super) fn initialize(&mut self, deadline: Instant) -> Result<(), String> {
        self.wait_quiescent(deadline)?;
        let snapshot = self.snapshot()?;
        if snapshot.tick != 0 || snapshot.restoring || snapshot.phase != GamePhase::Running {
            return Err("desktop startup did not reach a fresh initialized paused game".into());
        }
        let holds = movement_keyboard_bindings()
            .into_iter()
            .map(|binding| KeyboardHold {
                binding,
                held: false,
            })
            .collect();
        self.run
            .restore_keyboard_holds(holds)
            .map_err(|error| format!("install neutral movement inventory: {error}"))?;
        self.maintenance(deadline)?;
        let canonical = self.canonical()?;
        if canonical.held != [false; 4]
            || self.run.input_activation() != Some(WinitInputActivation::Isolated)
        {
            return Err("desktop startup did not retain the neutral isolated inventory".into());
        }
        check_deadline(deadline)
    }

    pub(super) fn execution_failed(&self) -> bool {
        self.execution_failed
    }

    pub(super) fn is_closed(&self) -> bool {
        matches!(
            self.run.progress(),
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
        )
    }

    pub(super) fn poll_idle(&mut self, max_wait: Duration) -> Result<(), String> {
        let progress = self.run.poll(max_wait);
        if progress == DesktopRunProgress::Running || self.closing {
            return Ok(());
        }
        Err(format!(
            "source-desktop child stopped outside Close: {progress:?}"
        ))
    }

    fn poll_live(&mut self, deadline: Instant) -> Result<(), String> {
        check_deadline(deadline)?;
        let wait = POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now()));
        let progress = self.run.poll(wait);
        if progress != DesktopRunProgress::Running || self.run.state() != Some(RuntimeState::Paused)
        {
            return Err(format!("original paused desktop stopped: {progress:?}"));
        }
        check_deadline(deadline)
    }

    fn snapshot(&self) -> Result<GameSnapshot, String> {
        self.run
            .capture(|world| world.resource::<GameSnapshot>().clone())
            .map_err(|error| format!("paused desktop observation failed: {error}"))
    }

    fn canonical(&self) -> Result<ResumeState, String> {
        capture_desktop_resume(&self.run)
            .map_err(|error| format!("desktop canonical capture was refused: {error}"))
    }

    pub(super) fn observed(&self) -> Result<Reply, String> {
        let runtime = self
            .run
            .runtime_generation()
            .ok_or("the original desktop Runtime generation is unavailable")?;
        Ok(observed_snapshot(runtime.get(), self.snapshot()?))
    }

    fn wait_quiescent(&mut self, deadline: Instant) -> Result<(), String> {
        loop {
            self.poll_live(deadline)?;
            match self
                .run
                .capture(|world| world.resource::<GameSnapshot>().clone())
            {
                Ok(snapshot) => {
                    if let Some(error) = snapshot.error {
                        return Err(format!("original desktop game failed: {error}"));
                    }
                    if snapshot.initialized
                        && snapshot.phase == GamePhase::Running
                        && !snapshot.restoring
                    {
                        return Ok(());
                    }
                    self.run
                        .request_paused_maintenance()
                        .map_err(|error| error.to_string())?;
                }
                Err(DesktopControlError::NotPaused) => {}
                Err(error) => return Err(format!("desktop paused boundary unavailable: {error}")),
            }
        }
    }

    fn maintenance(&mut self, deadline: Instant) -> Result<(), String> {
        check_deadline(deadline)?;
        self.run
            .request_paused_maintenance()
            .map_err(|error| format!("request original paused maintenance: {error}"))?;
        self.wait_quiescent(deadline)
    }

    pub(super) fn execute(&mut self, command: Command, deadline: Instant) -> Result<Reply, String> {
        if (self.execution_failed || self.closing) && !matches!(&command, Command::Close) {
            return Err("the original desktop child retains only physical cleanup".into());
        }
        if matches!(&command, Command::ActivateInput) {
            let error = self.activate_input(deadline).err();
            if self.run.progress() != DesktopRunProgress::Running {
                self.closing = true;
            }
            return Ok(Reply::Activated {
                activation: activation(self.run.input_activation()),
                error,
            });
        }
        let mutating = matches!(
            &command,
            Command::Step { .. } | Command::Frame | Command::Restore { .. }
        );
        check_deadline(deadline)?;
        let result = self.execute_inner(command, deadline);
        if result.is_err() && mutating {
            self.execution_failed |= self.canonical().is_err();
        }
        if result.is_ok()
            && !self.closing
            && let Err(error) = check_deadline(deadline)
        {
            self.execution_failed |= mutating;
            return Err(error);
        }
        result
    }

    fn execute_inner(&mut self, command: Command, deadline: Instant) -> Result<Reply, String> {
        match command {
            Command::Observe => {
                self.wait_quiescent(deadline)?;
                self.observed()
            }
            Command::Frame => {
                self.maintenance(deadline)?;
                self.observed()
            }
            Command::Step { commands } => {
                self.step(commands, deadline)?;
                self.observed()
            }
            Command::Capture => {
                self.wait_quiescent(deadline)?;
                self.canonical().map(|state| Reply::Captured { state })
            }
            Command::Restore { state } => {
                self.wait_quiescent(deadline)?;
                restore_desktop_resume(&mut self.run, &state, deadline)
                    .map_err(|error| format!("desktop reconstruction was refused: {error}"))?;
                self.observed()
            }
            Command::ConfirmPresentation => self
                .confirm_presentation(deadline)
                .map(|presentation| Reply::Presented { presentation }),
            Command::IsolateInput => {
                self.isolate_input(deadline)?;
                self.observed()
            }
            Command::ActivateInput => Err("activation must return its retained outcome".into()),
            Command::Close => self.close(deadline),
        }
    }

    fn isolate_input(&mut self, deadline: Instant) -> Result<(), String> {
        self.wait_quiescent(deadline)?;
        let before = encode_resume(&self.canonical()?).map_err(|error| error.to_string())?;
        self.run
            .isolate_keyboard_input(&movement_keyboard_bindings())
            .map_err(|error| format!("freeze original native keyboard ingress: {error}"))?;
        self.maintenance(deadline)?;
        if encode_resume(&self.canonical()?).map_err(|error| error.to_string())? != before {
            return Err("the original canonical state changed during input isolation".into());
        }
        check_deadline(deadline)
    }

    fn step(&mut self, commands: Vec<SemanticCommand>, deadline: Instant) -> Result<(), String> {
        self.wait_quiescent(deadline)?;
        let tick = self
            .snapshot()?
            .tick
            .checked_add(1)
            .and_then(GameplayCommandTick::new)
            .ok_or("desktop engine tick is exhausted")?;
        let bindings = movement_keyboard_bindings();
        let operations = commands
            .into_iter()
            .map(|command| match command {
                SemanticCommand::Move { direction, pressed } => {
                    let key = bindings
                        .get(direction)
                        .ok_or("movement direction is out of range")?
                        .key;
                    if self.run.input_activation() != Some(WinitInputActivation::Isolated) {
                        return Err(
                            "controlled desktop movement requires explicit input isolation".into(),
                        );
                    }
                    Ok(StepOperation::Keyboard(if pressed {
                        ButtonDriverInput::Press(key)
                    } else {
                        ButtonDriverInput::Release(key)
                    }))
                }
                command => command_draft(command).map(StepOperation::Gameplay),
            })
            .collect::<Result<Vec<_>, String>>()?;
        let gameplay_count = operations
            .iter()
            .filter(|operation| matches!(operation, StepOperation::Gameplay(_)))
            .count();
        self.sequence
            .checked_add(gameplay_count as u64)
            .ok_or("desktop command source sequence is exhausted")?;
        let source = GameplayCommandIngressSource::external("two-level-game.continuity")
            .map_err(|error| format!("command source is invalid: {error}"))?;
        let mut admitted = false;
        for operation in operations {
            if let Err(error) = check_deadline(deadline) {
                self.execution_failed |= admitted;
                return Err(error);
            }
            let result = match operation {
                StepOperation::Keyboard(input) => self
                    .run
                    .admit_isolated_keyboard_input(input)
                    .map(|_| ())
                    .map_err(|error| {
                        format!("original Synthetic keyboard admission failed: {error}")
                    }),
                StepOperation::Gameplay(draft) => {
                    self.sequence += 1;
                    let sequence = GameplayCommandSourceSequence::new(self.sequence)
                        .ok_or("desktop command source sequence is zero")?;
                    self.run
                        .submit(GameplayCommandSubmission::new(
                            tick,
                            source.clone(),
                            sequence,
                            draft,
                        ))
                        .map(|_| ())
                        .map_err(|error| {
                            format!("desktop semantic command admission failed: {error}")
                        })
                }
            };
            if let Err(error) = result {
                self.execution_failed |= admitted;
                return Err(error);
            }
            admitted = true;
        }
        if admitted {
            self.maintenance(deadline)?;
        }
        let ticket = self
            .run
            .request_control(RuntimeControl::StepFixedTick)
            .map_err(|error| format!("original desktop Step was refused: {error}"))?;
        self.wait_step(ticket, deadline)?;
        self.maintenance(deadline)?;
        if self.snapshot()?.tick != tick.get() {
            return Err("the original desktop Step did not complete exactly one fixed tick".into());
        }
        Ok(())
    }

    fn wait_step(&mut self, ticket: RuntimeControlTicket, deadline: Instant) -> Result<(), String> {
        loop {
            self.poll_live(deadline)?;
            match self.run.control_status(ticket) {
                Some(RuntimeControlStatus::Applied) => return Ok(()),
                Some(RuntimeControlStatus::Pending) => {}
                status => return Err(format!("original desktop Step failed: {status:?}")),
            }
        }
    }

    fn confirm_presentation(&mut self, deadline: Instant) -> Result<DesktopPresentation, String> {
        self.wait_quiescent(deadline)?;
        let expected = state_digest(&self.canonical()?)?;
        let runtime = self
            .run
            .runtime_generation()
            .ok_or("the original desktop Runtime generation is unavailable")?
            .get();
        let minimum_frame = self
            .run
            .capture(|world| world.resource::<FrameExecutionStart>().frame())
            .map_err(|error| error.to_string())?
            .checked_add(1)
            .ok_or("desktop App frame is exhausted")?;
        self.maintenance(deadline)?;
        loop {
            if state_digest(&self.canonical()?)? != expected {
                return Err(
                    "canonical desktop state changed during presentation confirmation".into(),
                );
            }
            {
                let state = self.presentations.borrow();
                if let Some(error) = &state.failure {
                    return Err(error.clone());
                }
                if let Some(frame) = state.frames.iter().find(|frame| {
                    frame.runtime_generation == runtime
                        && frame.app_frame >= minimum_frame
                        && frame.state_digest == expected
                }) {
                    return Ok(frame.clone());
                }
            }
            self.maintenance(deadline)?;
        }
    }

    fn activate_input(&mut self, deadline: Instant) -> Result<(), String> {
        self.wait_quiescent(deadline)?;
        if self.run.input_activation() == Some(WinitInputActivation::Active) {
            return check_deadline(deadline);
        }
        self.run
            .reveal()
            .map_err(|error| format!("reveal original adopted window: {error}"))?;
        self.maintenance(deadline)?;
        self.run
            .request_input_activation()
            .map_err(|error| format!("request original native activation: {error}"))?;
        let mut stepped = false;
        loop {
            check_deadline(deadline)?;
            if let Some(error) = self.run.input_activation_error() {
                return Err(error.to_string());
            }
            match self.run.input_activation() {
                Some(WinitInputActivation::Active) => {
                    self.wait_quiescent(deadline)?;
                    if self.run.input_activation() == Some(WinitInputActivation::Active) {
                        return check_deadline(deadline);
                    }
                }
                Some(WinitInputActivation::AwaitingFocus) => self.poll_live(deadline)?,
                Some(WinitInputActivation::ReconciliationPending) => {
                    self.maintenance(deadline)?;
                    if self.run.input_activation()
                        != Some(WinitInputActivation::ReconciliationPending)
                    {
                        continue;
                    }
                    if stepped {
                        return Err("native activation still requires reconciliation after its original Step".into());
                    }
                    match self.run.request_control(RuntimeControl::StepFixedTick) {
                        Ok(ticket) => {
                            stepped = true;
                            self.wait_step(ticket, deadline)?;
                            self.maintenance(deadline)?;
                        }
                        Err(DesktopControlError::ControlRejected(_)) => self.poll_live(deadline)?,
                        Err(error) => {
                            return Err(format!(
                                "activation reconciliation Step was refused: {error}"
                            ));
                        }
                    }
                }
                state => return Err(format!("native activation is unavailable: {state:?}")),
            }
        }
    }

    pub(super) fn close(&mut self, deadline: Instant) -> Result<Reply, String> {
        self.closing = true;
        self.run.request_close();
        if self.run.progress() == DesktopRunProgress::CleanupIncomplete {
            let _ = self.run.execute();
        }
        loop {
            match self.run.progress() {
                DesktopRunProgress::Completed(_) => return Ok(Reply::Closed { failed: false }),
                DesktopRunProgress::Failed => return Ok(Reply::Closed { failed: true }),
                DesktopRunProgress::CleanupIncomplete => return Ok(Reply::Closing),
                _ => {}
            }
            if Instant::now() >= deadline {
                return Ok(Reply::Closing);
            }
            let wait = POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now()));
            self.run.poll(wait);
        }
    }
}

enum StepOperation {
    Keyboard(ButtonDriverInput<KeyCode>),
    Gameplay(GameplayCommandDraft),
}

fn activation(state: Option<WinitInputActivation>) -> InputActivation {
    match state {
        Some(WinitInputActivation::Isolated) => InputActivation::Isolated,
        Some(WinitInputActivation::AwaitingFocus) => InputActivation::AwaitingFocus,
        Some(WinitInputActivation::ReconciliationPending) => InputActivation::ReconciliationPending,
        Some(WinitInputActivation::Active) => InputActivation::Active,
        Some(WinitInputActivation::Unavailable) | None => InputActivation::Unavailable,
    }
}
