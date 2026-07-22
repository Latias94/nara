use std::{
    error::Error,
    fmt,
    num::NonZeroU32,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{
        PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId, PluginError,
        PluginId,
    },
    diagnostic::{Diagnostic, DiagnosticReport, DiagnosticSeverity},
    ecs::{error::BevyError, schedule::IntoScheduleConfigs},
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandKey,
        GameplayCommandPayload, GameplayCommandQueue, GameplayCommandRejection, GameplayCommandSet,
        GameplayCommandSource, GameplayCommandSourceSequence, GameplayCommandSubmission,
        GameplayCommandTick, GameplayCommandTypeId, GameplayCommandValue,
    },
    prelude::{App, CoreStage, FixedTime, FixedUpdateSet, Plugin, Res, ResMut, Resource},
    project_host::{
        DesktopRun, DesktopRunOutcome, EditorProjectSession, HeadlessRun, HeadlessRunOutcome,
    },
    tooling::{EditorPlayCommand, EditorPlayRequestResult, EditorPlayState},
};
use nara_reference_game::{
    MovementDirection, REFERENCE_WAVE_PLUGIN_ID, ReferenceWavePlugin, WaveSnapshot,
    movement_command, wave_desktop_intent, wave_editor_intent, wave_headless_intent,
};

mod support;
use support::project_root::open_project_root;

const HOST_PARITY_PROBE_PLUGIN_ID: PluginId = PluginId::new("reference-game.host-parity-probe");
const HOST_PARITY_PROBE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("reference-game.host-parity-probe", 1);
const HOST_PARITY_PROBE_REQUIREMENTS: &[PluginId] = &[REFERENCE_WAVE_PLUGIN_ID];
const HOST_PARITY_PROBE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(HOST_PARITY_PROBE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(HOST_PARITY_PROBE_REQUIREMENTS);
const PARITY_COMMAND_SOURCE: &str = "reference-game.host-parity";
const PARITY_COMMAND_TYPE: &str = "reference-game.host-parity-no-op-v1";
const PARITY_CAPTURE_TICK: u64 = 2;
const PARITY_FAULT_TICK: u64 = 3;
const PARITY_COMMAND_COUNT: usize = 5;
const MAX_PARITY_ENVELOPE_BYTES: usize = 16 * 1024;
const HOST_TIMEOUT: Duration = Duration::from_secs(30);

fn main() -> ExitCode {
    let mode = match HostMode::from_args() {
        Ok(mode) => mode,
        Err(code) => return fail(code),
    };
    let root = match open_project_root() {
        Ok(root) => root,
        Err(_) => return fail("host_parity_probe.root-failed"),
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    let fault_tick = Arc::new(AtomicU64::new(0));
    let probe = probe_definition(sender, Arc::clone(&fault_tick));

    let host_result = match mode {
        HostMode::Headless => run_headless(root, probe),
        HostMode::Desktop => run_desktop(root, probe),
        HostMode::Editor => run_editor(root, probe),
    };
    let fault = match host_result {
        Ok(fault) => fault,
        Err(code) => return fail(code),
    };
    let observed_fault_tick = fault_tick.load(Ordering::SeqCst);
    if observed_fault_tick != PARITY_FAULT_TICK {
        return fail("host_parity_probe.expected-fault-missing");
    }
    let evidence = match receiver.recv_timeout(Duration::from_secs(1)) {
        Ok(evidence) => evidence,
        Err(_) => return fail("host_parity_probe.evidence-missing"),
    };
    let envelope = match canonical_envelope(&evidence, &fault, observed_fault_tick) {
        Ok(envelope) => envelope,
        Err(code) => return fail(code),
    };
    println!("{envelope}");
    ExitCode::SUCCESS
}

fn fail(code: &'static str) -> ExitCode {
    eprintln!("{code}");
    ExitCode::FAILURE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostMode {
    Headless,
    Desktop,
    Editor,
}

impl HostMode {
    fn from_args() -> Result<Self, &'static str> {
        let mut args = std::env::args().skip(1);
        let mode = match args.next().as_deref() {
            Some("headless") => Self::Headless,
            Some("desktop") => Self::Desktop,
            Some("editor") => Self::Editor,
            _ => return Err("host_parity_probe.mode-invalid"),
        };
        if args.next().is_some() {
            return Err("host_parity_probe.arguments-invalid");
        }
        Ok(mode)
    }
}

fn run_headless(
    root: nara::fs::DirectoryCapability,
    probe: PluginDefinition,
) -> Result<RuntimeFaultEvidence, &'static str> {
    let maximum_ticks =
        NonZeroU32::new(PARITY_FAULT_TICK as u32).expect("the parity tick limit is non-zero");
    let intent = wave_headless_intent(maximum_ticks).insert_after::<ReferenceWavePlugin>(probe);
    let mut run = HeadlessRun::new(root, intent, Vec::new());
    let deadline = Instant::now() + HOST_TIMEOUT;
    let report = loop {
        let report = run.execute_bounded();
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            break report;
        }
        if Instant::now() >= deadline {
            return Err("host_parity_probe.headless-cleanup-timeout");
        }
        std::thread::yield_now();
    };
    if !matches!(report.outcome(), HeadlessRunOutcome::Failed) {
        return Err("host_parity_probe.headless-fault-mismatch");
    }
    runtime_fault_evidence(
        report.diagnostics(),
        "project.run.runtime-faulted",
        None,
        false,
    )
}

fn run_desktop(
    root: nara::fs::DirectoryCapability,
    probe: PluginDefinition,
) -> Result<RuntimeFaultEvidence, &'static str> {
    let intent = wave_desktop_intent().insert_after::<ReferenceWavePlugin>(probe);
    let mut run = DesktopRun::new(root, intent);
    let cleanup_deadline = Instant::now() + HOST_TIMEOUT;
    let report = loop {
        let report = run.execute();
        if report.outcome() != DesktopRunOutcome::CleanupIncomplete {
            break report;
        }
        if Instant::now() >= cleanup_deadline {
            return Err("host_parity_probe.desktop-cleanup-timeout");
        }
        std::thread::park_timeout(Duration::from_millis(1));
    };
    if report.outcome() != DesktopRunOutcome::Failed {
        return Err("host_parity_probe.desktop-fault-mismatch");
    }
    runtime_fault_evidence(
        report.diagnostics(),
        "project.desktop.runner-failed",
        Some("managed-runtime"),
        true,
    )
}

fn run_editor(
    root: nara::fs::DirectoryCapability,
    probe: PluginDefinition,
) -> Result<RuntimeFaultEvidence, &'static str> {
    let intent = wave_editor_intent().insert_after::<ReferenceWavePlugin>(probe);
    let mut session =
        EditorProjectSession::open(root, intent).map_err(|_| "host_parity_probe.editor-open")?;
    if !matches!(
        session.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    ) {
        return Err("host_parity_probe.editor-play-rejected");
    }

    let deadline = Instant::now() + HOST_TIMEOUT;
    loop {
        session.drive_editor_frame(FixedTime::DEFAULT_TIMESTEP);
        match session.play_view().state() {
            EditorPlayState::Faulted => break,
            EditorPlayState::Empty | EditorPlayState::CloseIncomplete => {
                return Err("host_parity_probe.editor-fault-mismatch");
            }
            _ if Instant::now() >= deadline => {
                return Err("host_parity_probe.editor-fault-timeout");
            }
            _ => {}
        }
    }
    let fault = runtime_fault_evidence(
        session.diagnostics(),
        "project.run.runtime-faulted",
        None,
        false,
    )?;
    if !matches!(
        session.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    ) {
        return Err("host_parity_probe.editor-stop-rejected");
    }
    loop {
        session.drive_editor_frame(Duration::ZERO);
        match session.play_view().state() {
            EditorPlayState::Empty => break,
            EditorPlayState::CloseIncomplete => {
                if !matches!(
                    session.request_play(EditorPlayCommand::RetryClose),
                    EditorPlayRequestResult::Accepted
                ) {
                    return Err("host_parity_probe.editor-close-retry-rejected");
                }
            }
            _ if Instant::now() >= deadline => {
                return Err("host_parity_probe.editor-cleanup-timeout");
            }
            _ => {}
        }
    }
    Ok(fault)
}

#[derive(Debug, Clone)]
struct RuntimeFaultEvidence {
    kind: String,
    source: String,
}

fn runtime_fault_evidence(
    report: &DiagnosticReport,
    expected_code: &str,
    expected_reason: Option<&str>,
    require_exclusive_error: bool,
) -> Result<RuntimeFaultEvidence, &'static str> {
    let mut error_count = 0;
    let mut expected = None;
    for diagnostic in report
        .iter()
        .filter(|diagnostic| diagnostic.severity() == DiagnosticSeverity::Error)
    {
        error_count += 1;
        if diagnostic.code().as_str() == expected_code
            && diagnostic_field(diagnostic, "fault-kind").is_some()
            && diagnostic_field(diagnostic, "fault-source").is_some()
        {
            expected = Some(diagnostic);
        }
    }
    if require_exclusive_error && error_count != 1 {
        return Err("host_parity_probe.fault-diagnostic-mismatch");
    }
    let diagnostic = expected.ok_or("host_parity_probe.fault-diagnostic-missing")?;
    if expected_reason
        .is_some_and(|reason| diagnostic_field(diagnostic, "reason").as_deref() != Some(reason))
    {
        return Err("host_parity_probe.fault-reason-mismatch");
    }
    let kind =
        diagnostic_field(diagnostic, "fault-kind").ok_or("host_parity_probe.fault-kind-missing")?;
    let source = diagnostic_field(diagnostic, "fault-source")
        .ok_or("host_parity_probe.fault-source-missing")?;
    if kind != "system" || source != "nara.ecs.fallible-execution" {
        return Err("host_parity_probe.fault-identity-mismatch");
    }
    Ok(RuntimeFaultEvidence { kind, source })
}

fn diagnostic_field(diagnostic: &Diagnostic, key: &str) -> Option<String> {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .map(|field| field.display_value().into_owned())
}

fn probe_definition(
    sender: SyncSender<HostParityEvidence>,
    fault_tick: Arc<AtomicU64>,
) -> PluginDefinition {
    PluginDefinition::infallible::<HostParityProbePlugin, _>(
        HOST_PARITY_PROBE_DEFINITION_ID,
        b"reference-game-host-parity-probe-v1",
        move || HostParityProbePlugin {
            sender: sender.clone(),
            fault_tick: Arc::clone(&fault_tick),
        },
    )
}

#[derive(Debug)]
struct HostParityProbePlugin {
    sender: SyncSender<HostParityEvidence>,
    fault_tick: Arc<AtomicU64>,
}

impl Plugin for HostParityProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &HOST_PARITY_PROBE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(HostParityProbeState {
            sender: self.sender.clone(),
            fault_tick: Arc::clone(&self.fault_tick),
            commands: None,
            published: false,
        })?
        .add_systems(
            CoreStage::FixedUpdate,
            (inject_parity_commands, trigger_expected_parity_fault)
                .in_set(FixedUpdateSet::Simulate)
                .before(GameplayCommandSet::Consume),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            publish_parity_evidence
                .in_set(FixedUpdateSet::Finalize)
                .after(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct HostParityProbeState {
    sender: SyncSender<HostParityEvidence>,
    fault_tick: Arc<AtomicU64>,
    commands: Option<[CommandObservation; PARITY_COMMAND_COUNT]>,
    published: bool,
}

#[derive(Debug, Clone)]
struct HostParityEvidence {
    commands: [CommandObservation; PARITY_COMMAND_COUNT],
    snapshot: WaveSnapshot,
}

#[derive(Debug, Clone)]
struct CommandObservation {
    label: &'static str,
    requested: GameplayCommandKey,
    result: Result<GameplayCommandKey, GameplayCommandRejection>,
}

fn inject_parity_commands(
    fixed_time: Res<FixedTime>,
    mut queue: ResMut<GameplayCommandQueue>,
    mut probe: ResMut<HostParityProbeState>,
) -> Result<(), BevyError> {
    if probe.commands.is_some() || fixed_time.tick() != 1 {
        return Ok(());
    }

    let accepted_submission = movement_command(PARITY_CAPTURE_TICK, 1, MovementDirection::Left)
        .map_err(|_| probe_error("the accepted parity command could not be created"))?;
    let duplicate_submission = accepted_submission.clone();
    let accepted_request = accepted_submission.key();
    let duplicate_request = duplicate_submission.key();
    let accepted_result = queue.submit(accepted_submission);
    let duplicate_result = queue.submit(duplicate_submission);

    let closed_through = queue.stats().closed_through_tick;
    let future_tick = closed_through
        .checked_add(queue.settings().future_ticks().get())
        .and_then(|tick| tick.checked_add(1))
        .ok_or_else(|| probe_error("the parity future tick overflowed"))?;
    let future_submission = parity_submission(future_tick, 2, parity_no_op_draft());
    let future_request = future_submission.key();
    let future_result = queue.submit(future_submission);

    let payload_limit = queue.settings().payload_bytes().get();
    let oversized_len = payload_limit
        .checked_add(1)
        .ok_or_else(|| probe_error("the parity payload size overflowed"))?;
    let mut oversized_payload = GameplayCommandPayload::new();
    oversized_payload
        .insert(
            "payload",
            GameplayCommandValue::String("x".repeat(oversized_len)),
        )
        .map_err(|_| probe_error("the bounded parity payload could not be created"))?;
    let oversized_submission = parity_submission(
        PARITY_CAPTURE_TICK,
        3,
        GameplayCommandDraft::new(parity_command_type()).with_payload(oversized_payload),
    );
    let oversized_request = oversized_submission.key();
    let oversized_result = queue.submit(oversized_submission);

    let late_submission = parity_submission(closed_through, 4, parity_no_op_draft());
    let late_request = late_submission.key();
    let late_result = queue.submit(late_submission);

    probe.commands = Some([
        CommandObservation {
            label: "accepted",
            requested: accepted_request,
            result: accepted_result,
        },
        CommandObservation {
            label: "duplicate",
            requested: duplicate_request,
            result: duplicate_result,
        },
        CommandObservation {
            label: "future",
            requested: future_request,
            result: future_result,
        },
        CommandObservation {
            label: "over-budget",
            requested: oversized_request,
            result: oversized_result,
        },
        CommandObservation {
            label: "late",
            requested: late_request,
            result: late_result,
        },
    ]);
    Ok(())
}

fn publish_parity_evidence(
    fixed_time: Res<FixedTime>,
    snapshot: Res<WaveSnapshot>,
    mut probe: ResMut<HostParityProbeState>,
) -> Result<(), BevyError> {
    if probe.published || fixed_time.tick() != PARITY_CAPTURE_TICK {
        return Ok(());
    }
    if snapshot.tick != PARITY_CAPTURE_TICK {
        return Err(probe_error(
            "the stable wave snapshot was not current at the capture boundary",
        ));
    }
    let commands = probe
        .commands
        .clone()
        .ok_or_else(|| probe_error("the parity commands were not injected"))?;
    let evidence = HostParityEvidence {
        commands,
        snapshot: (*snapshot).clone(),
    };
    match probe.sender.try_send(evidence) {
        Ok(()) => probe.published = true,
        Err(TrySendError::Full(_)) => {
            return Err(probe_error("the bounded parity sink was already full"));
        }
        Err(TrySendError::Disconnected(_)) => {
            return Err(probe_error("the bounded parity sink was disconnected"));
        }
    }
    Ok(())
}

fn trigger_expected_parity_fault(
    fixed_time: Res<FixedTime>,
    probe: Res<HostParityProbeState>,
) -> Result<(), BevyError> {
    if fixed_time.tick() < PARITY_FAULT_TICK || !probe.published {
        return Ok(());
    }
    probe.fault_tick.store(fixed_time.tick(), Ordering::SeqCst);
    Err(BevyError::error(ExpectedParityFault))
}

fn parity_submission(
    tick: u64,
    sequence: u64,
    command: GameplayCommandDraft,
) -> GameplayCommandSubmission {
    GameplayCommandSubmission::new(
        GameplayCommandTick::new(tick).expect("the test-owned parity tick is non-zero"),
        GameplayCommandIngressSource::test(PARITY_COMMAND_SOURCE)
            .expect("the test-owned parity source is valid"),
        GameplayCommandSourceSequence::new(sequence)
            .expect("the test-owned parity sequence is non-zero"),
        command,
    )
}

fn parity_no_op_draft() -> GameplayCommandDraft {
    GameplayCommandDraft::new(parity_command_type())
}

fn parity_command_type() -> GameplayCommandTypeId {
    GameplayCommandTypeId::new(PARITY_COMMAND_TYPE)
        .expect("the test-owned parity command type is valid")
}

#[derive(Debug)]
struct ProbeFailure(&'static str);

impl fmt::Display for ProbeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for ProbeFailure {}

fn probe_error(message: &'static str) -> BevyError {
    BevyError::error(ProbeFailure(message))
}

#[derive(Debug)]
struct ExpectedParityFault;

impl fmt::Display for ExpectedParityFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the host parity probe reached its expected fixed-simulate fault")
    }
}

impl Error for ExpectedParityFault {}

fn canonical_envelope(
    evidence: &HostParityEvidence,
    fault: &RuntimeFaultEvidence,
    fault_tick: u64,
) -> Result<String, &'static str> {
    let commands = evidence
        .commands
        .iter()
        .map(canonical_command)
        .collect::<Result<Vec<_>, _>>()?
        .join(";");
    let snapshot = &evidence.snapshot;
    let player_id = canonical_id(&snapshot.player.id)?;
    let enemies = snapshot
        .enemies
        .iter()
        .map(|enemy| {
            let id = canonical_id(&enemy.id)?;
            Ok(format!(
                "{id},{:08x},{:08x},{},{},{}",
                enemy.position.x.to_bits(),
                enemy.position.y.to_bits(),
                enemy.hit_points,
                enemy.spawn_tick,
                u8::from(enemy.active),
            ))
        })
        .collect::<Result<Vec<_>, &'static str>>()?
        .join(";");
    if !evidence
        .snapshot
        .enemies
        .windows(2)
        .all(|pair| pair[0].id < pair[1].id)
    {
        return Err("host_parity_probe.enemy-order-invalid");
    }
    let projectiles = snapshot
        .projectiles
        .iter()
        .map(|projectile| {
            format!(
                "{},{:08x},{:08x},{:08x},{:08x},{}",
                projectile.id,
                projectile.position.x.to_bits(),
                projectile.position.y.to_bits(),
                projectile.velocity.x.to_bits(),
                projectile.velocity.y.to_bits(),
                projectile.ttl_ticks,
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    if !snapshot
        .projectiles
        .windows(2)
        .all(|pair| pair[0].id < pair[1].id)
    {
        return Err("host_parity_probe.projectile-order-invalid");
    }
    let fault_kind = canonical_id(&fault.kind)?;
    let fault_source = canonical_id(&fault.source)?;
    let envelope = format!(
        "nara-host-parity-v1|commands={commands}|wave={},{},{},{},{},{}|player={player_id},{:08x},{:08x},{}|enemies={enemies}|projectiles={projectiles}|fault={fault_kind},{fault_source},{fault_tick}",
        snapshot.run_generation,
        snapshot.tick,
        snapshot.outcome.as_str(),
        snapshot.score,
        snapshot.planned_enemies,
        snapshot.defeated_enemies,
        snapshot.player.position.x.to_bits(),
        snapshot.player.position.y.to_bits(),
        snapshot.player.hit_points,
    );
    if envelope.len() > MAX_PARITY_ENVELOPE_BYTES {
        return Err("host_parity_probe.envelope-too-large");
    }
    Ok(envelope)
}

fn canonical_command(observation: &CommandObservation) -> Result<String, &'static str> {
    let requested = &observation.requested;
    let (source_kind, source_id) = match requested.source() {
        GameplayCommandSource::LocalAction => ("local", "none"),
        GameplayCommandSource::Test { driver } => ("test", canonical_id(driver.as_str())?),
        GameplayCommandSource::Replay { stream } => ("replay", canonical_id(stream.as_str())?),
        GameplayCommandSource::Ai { agent } => ("ai", canonical_id(agent.as_str())?),
        GameplayCommandSource::External { producer } => {
            ("external", canonical_id(producer.as_str())?)
        }
    };
    let outcome = match (observation.label, &observation.result) {
        ("accepted", Ok(actual)) if actual == requested => "accepted".to_owned(),
        ("duplicate", Err(GameplayCommandRejection::Duplicate)) => "duplicate".to_owned(),
        (
            "future",
            Err(GameplayCommandRejection::TooFarFuture {
                target,
                closed_through,
                maximum_distance,
            }),
        ) if *target == requested.tick().get() => {
            format!("too-far-future,{target},{closed_through},{maximum_distance}")
        }
        ("over-budget", Err(GameplayCommandRejection::PayloadByteLimit { requested, maximum })) => {
            format!("payload-byte-limit,{requested},{maximum}")
        }
        (
            "late",
            Err(GameplayCommandRejection::Late {
                target,
                closed_through,
            }),
        ) if *target == requested.tick().get() => format!("late,{target},{closed_through}"),
        _ => return Err("host_parity_probe.command-result-mismatch"),
    };
    Ok(format!(
        "{}@{}@{source_kind}@{source_id}@{}@{outcome}",
        observation.label,
        requested.tick().get(),
        requested.source_sequence().get(),
    ))
}

fn canonical_id(value: &str) -> Result<&str, &'static str> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
    {
        return Err("host_parity_probe.identifier-invalid");
    }
    Ok(value)
}
