use super::*;
use crate::project_host::runtime::editor::EditorProjectOpenError;
use crate::project_host::runtime::editor::persistence::HostCustody;
use crate::project_host::runtime::editor::test_fixtures::HeldCompute;
use nara_app::{
    Plugin, PluginCategory, PluginDeclaration, PluginError, PluginHook, PluginId,
    PluginShutdownObligationId, RuntimeCandidateRetirementState, RuntimeCloseCause,
    RuntimeCloseContext, RuntimeCloseErrorDisposition, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseParticipantPhase,
    RuntimeCloseProgress, RuntimeFaultDetail, RuntimeFaultKind, StartupStage,
    runtime_quarantine_status,
};
use nara_ecs::bevy_ecs::error::BevyError;
use nara_tasks::TaskReturnState;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Instant;

const WAIT: Duration = Duration::from_secs(5);
const FAILURE_CODE: &str = "project.editor.preview-plugin-install-failed";
const BUILD_REJECTION: &str = "injected authoring configuration failure";

#[cfg(feature = "presentation-ui")]
#[test]
fn canvas_decode_rejects_scratch_identity_and_type_claim_until_the_image_owner_binds_them() {
    use nara_asset::{
        AssetPathScope, AssetProductRef, AssetRef, AssetSourcePath, ImportedProductId,
        ProjectAssetDatabase, StableAssetId,
    };

    let scope = AssetPathScope::under(AssetSourcePath::new("content/art").unwrap());
    let source: StableAssetId = "86db6196-0386-4dbe-a5cd-a7c521ed6b27".parse().unwrap();
    let record = AssetRecord::new(
        source,
        AssetSourcePath::new("content/art/icon.png").unwrap(),
        AssetSourceKind::Image,
    );
    let mut database = ProjectAssetDatabase::default();
    database.insert(record.clone()).unwrap();
    let references = [
        AssetRef::path("icon.png").unwrap(),
        AssetRef::StableId(source),
        AssetRef::Product(AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap()),
    ];

    for reserve_untyped_identity in [false, true] {
        let mut server = AssetServer::new();
        if reserve_untyped_identity {
            server.reserve_record_id(&record).unwrap();
        }
        let before = server.clone();
        for reference in &references {
            let result = EditorCanvasImageSources::new(&server, database.resolver(&scope))
                .with_decode_context(|context| {
                    context
                        .resolve_asset_ref_with_kind::<ImageAsset>(
                            reference,
                            &AssetSourceKind::Image,
                        )
                        .unwrap()
                });
            assert_eq!(result, Err(EditorImageRuntimeFailure::AssetIdentity));
            assert_eq!(server, before);
        }

        // A later real Image publication supplies the binding used by a Canvas retry.
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let published = server.clone();
        for reference in &references {
            let decoded = EditorCanvasImageSources::new(&server, database.resolver(&scope))
                .with_decode_context(|context| {
                    context
                        .resolve_asset_ref_with_kind::<ImageAsset>(
                            reference,
                            &AssetSourceKind::Image,
                        )
                        .unwrap()
                })
                .unwrap()
                .unwrap();
            assert_eq!(decoded, handle);
            assert_eq!(server, published);
        }
    }
}

struct RejectPlugin {
    finishes: Arc<AtomicUsize>,
}

impl Plugin for RejectPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.authoring-configuration-rejection"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: Self::declaration().id,
            message: BUILD_REJECTION.to_owned(),
        })
    }

    fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
        self.finishes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Clone)]
struct StartupCloseProbe {
    begins: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
}

const STARTUP_CLOSE_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.authoring-startup-close");
const STARTUP_CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.authoring-startup-close");
const STARTUP_CLOSE_FAILURE: &str = "nara.test.authoring-startup-close-failed";

impl Plugin for StartupCloseProbe {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.authoring-startup-close"),
            PluginCategory::Runtime,
        )
        .shutdown_obligations(&[STARTUP_CLOSE_OBLIGATION]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            STARTUP_CLOSE_OBLIGATION,
            STARTUP_CLOSE_PARTICIPANT,
            self.clone(),
        )?;
        Ok(())
    }
}

impl RuntimeCloseParticipant for StartupCloseProbe {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begins.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::CompleteWithError {
            code: STARTUP_CLOSE_FAILURE,
        })
    }
}

#[test]
fn configuration_failure_retains_authoring_app_until_body_and_late_result_return() {
    let quarantine_before = runtime_quarantine_status().current_thread_retained();
    let finishes = Arc::new(AtomicUsize::new(0));
    let startup_runs = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    configure_authoring_app(&mut app).expect("the real authoring composition must configure");
    let world = app.world().id();
    let custody = app.world().resource::<HostCustody>().clone();
    let startup = Arc::clone(&startup_runs);
    app.add_systems(StartupStage::Runtime, move || {
        startup.fetch_add(1, Ordering::SeqCst);
    })
    .expect("install the Startup observation before configuration fails");
    let held = HeldCompute::start(app.world().resource::<TaskPools>(), || {});
    assert_eq!(held.returned.state(), TaskReturnState::Pending);
    let rejection = app
        .add_plugin(RejectPlugin {
            finishes: Arc::clone(&finishes),
        })
        .err();
    let diagnostics = runtime_error(FAILURE_CODE);
    // Exercise adapter custody after real acquisition; no allocation failure is injected.
    let mut failure = EditorAuthoringConstructionFailure {
        diagnostics: runtime_error(FAILURE_CODE),
        retirement: EditorAuthoringRetirement::Configuration(
            app.begin_retirement(RuntimeClosePolicy::default()),
        ),
    };
    // On unwind, release both gates before dropping the failed configuration owner.
    let mut held = held;
    let initial_state = failure.retirement.state();
    let accepting_before_poll = custody.is_accepting();
    let finishes_before_poll = finishes.load(Ordering::SeqCst);
    let startup_before_poll = startup_runs.load(Ordering::SeqCst);
    let cancelled = held.handle.cancel();
    let terminal_cancelled = matches!(held.handle.try_take(), Some(TaskTerminal::Cancelled(_)));

    for late_result in [false, true] {
        if late_result {
            held.release_body_until_drop();
        }
        for _ in 0..4 {
            let before = Instant::now();
            let progress = failure.retirement.poll();
            assert!(
                before.elapsed() < WAIT,
                "retirement polling must be bounded"
            );
            let EditorAuthoringRuntimeCloseProgress::Incomplete(report) = progress else {
                panic!("configuration retirement must retain executor-owned work");
            };
            assert!(!report.is_complete());
            assert!(!report.has_cleanup_failure());
            assert_eq!(report.for_kind(TaskPoolKind::Compute).joined_workers, 0);
            assert_eq!(
                failure.retirement.state(),
                RuntimeCandidateRetirementState::Retiring
            );
            assert!(held.handle.is_finished());
            assert_eq!(held.returned.state(), TaskReturnState::Pending);
            assert!(!custody.is_accepting());
            assert!(!failure.retirement.has_close_failure());
            let evidence = failure
                .retirement
                .close_evidence()
                .expect("retain the configured App's close evidence");
            assert!(!evidence.has_terminal_failure());
            assert!(
                evidence
                    .pending_participants()
                    .iter()
                    .any(|pending| pending.participant().as_str() == "nara.tasks.pools")
            );
            assert_eq!(failure.diagnostics, diagnostics);
            assert_eq!(
                runtime_quarantine_status().current_thread_retained(),
                quarantine_before
            );
        }
    }

    held.release();
    let deadline = Instant::now() + WAIT;
    let report = loop {
        match failure.retirement.poll() {
            EditorAuthoringRuntimeCloseProgress::Complete(report) => break report,
            EditorAuthoringRuntimeCloseProgress::Incomplete(_) => {
                assert!(
                    Instant::now() < deadline,
                    "the configured App did not retire"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    };
    let evidence = failure
        .retirement
        .close_evidence()
        .expect("completed configuration retirement must retain close evidence")
        .clone();

    let rejection = rejection.expect("the additional plugin must reject configuration");
    let build_failure = rejection
        .plugin_failure()
        .expect("configuration must fail in the plugin's build hook");
    assert_eq!(build_failure.hook(), PluginHook::Build);
    assert_eq!(
        build_failure.error(),
        &PluginError::SetupFailed {
            plugin: RejectPlugin::declaration().id,
            message: BUILD_REJECTION.to_owned(),
        }
    );
    assert_eq!(initial_state, RuntimeCandidateRetirementState::Retiring);
    assert!(accepting_before_poll);
    assert_eq!(finishes_before_poll, 0);
    assert_eq!(startup_before_poll, 0);
    assert!(cancelled);
    assert!(terminal_cancelled);
    assert_eq!(held.returned.state(), TaskReturnState::Returned);
    assert!(report.is_complete());
    assert!(!report.has_cleanup_failure());
    for (kind, joined) in [
        (TaskPoolKind::Io, 0),
        (TaskPoolKind::Compute, 1),
        (TaskPoolKind::AsyncCompute, 0),
    ] {
        assert_eq!(report.for_kind(kind).joined_workers, joined);
    }
    let EditorAuthoringRetirement::Configuration(configuration) = &failure.retirement else {
        panic!("retain the original configuration owner");
    };
    let retained_world = configuration.world().expect("retain the configured World");
    assert_eq!(retained_world.id(), world);
    assert_eq!(retained_world.resource::<TaskShutdownReport>(), &report);
    assert!(evidence.pending_participants().is_empty());
    assert!(!evidence.has_terminal_failure());
    for _ in 0..4 {
        let EditorAuthoringRuntimeCloseProgress::Complete(repeated) = failure.retirement.poll()
        else {
            panic!("completed configuration retirement must remain complete");
        };
        assert_eq!(repeated, report);
        assert_eq!(
            failure.retirement.state(),
            RuntimeCandidateRetirementState::Retired
        );
        assert_eq!(failure.retirement.close_evidence(), Some(&evidence));
        assert!(!failure.retirement.has_close_failure());
        assert_eq!(failure.diagnostics, diagnostics);
    }
    assert_eq!(finishes.load(Ordering::SeqCst), 0);
    assert_eq!(startup_runs.load(Ordering::SeqCst), 0);
    drop(held);
    drop(failure);
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        quarantine_before
    );
}

#[test]
fn startup_failure_retains_actual_candidate_through_body_and_late_result_disposal() {
    let quarantine_before = runtime_quarantine_status().current_thread_retained();
    let startup_runs = Arc::new(AtomicUsize::new(0));
    let close = StartupCloseProbe {
        begins: Arc::new(AtomicUsize::new(0)),
        polls: Arc::new(AtomicUsize::new(0)),
    };
    let detail = RuntimeFaultDetail::new(
        "nara.test.authoring-startup-failed",
        "Authoring Startup initialization failed",
        "nara.test.authoring-startup",
    )
    .expect("the Startup failure uses valid static metadata");
    let mut app = App::new();
    configure_authoring_app(&mut app).expect("the real authoring composition must configure");
    app.add_plugin(close.clone())
        .expect("install the close callback observation before sealing");
    let startup = Arc::clone(&startup_runs);
    app.add_systems(StartupStage::Runtime, move || -> Result<(), BevyError> {
        startup.fetch_add(1, Ordering::SeqCst);
        Err(detail.into_bevy_error())
    })
    .expect("install the failing Startup system through App");
    let world = app.world().id();
    let custody = app.world().resource::<HostCustody>().clone();
    let held = HeldCompute::start(app.world().resource::<TaskPools>(), || {});
    assert_eq!(held.returned.state(), TaskReturnState::Pending);

    // Keep the result owner before the release guard, including an unexpected successful start.
    let mut result = start_authoring_app(app).map_err(EditorProjectOpenError::from);
    let mut held = held;
    let failure = result
        .as_mut()
        .expect_err("the actual Startup schedule must fail authoring construction");
    let diagnostics = failure.diagnostics().clone();
    assert_eq!(
        diagnostics,
        runtime_error("project.editor.preview-runtime-start-failed")
    );
    assert_eq!(startup_runs.load(Ordering::SeqCst), 1);
    assert_eq!(
        failure.retirement_state(),
        RuntimeCandidateRetirementState::Retiring
    );
    assert_eq!(close.begins.load(Ordering::SeqCst), 0);
    assert_eq!(close.polls.load(Ordering::SeqCst), 0);
    assert!(custody.is_accepting());
    assert!(!held.handle.is_finished());
    assert_eq!(held.returned.state(), TaskReturnState::Pending);
    let EditorAuthoringRetirement::Startup(startup) = failure
        .retirement
        .as_deref()
        .expect("Open must retain the authoring construction owner")
    else {
        panic!("start_authoring_app must return the actual failed Startup candidate");
    };
    assert_eq!(startup.world().id(), world);
    assert!(
        startup
            .world()
            .get_resource::<TaskShutdownReport>()
            .is_none()
    );
    let fault = startup.fault().clone();
    assert_eq!(fault.kind(), RuntimeFaultKind::System);
    assert_eq!(fault.source(), "nara.ecs.fallible-execution");
    assert_eq!(fault.detail(), Some(detail));
    assert!(startup.close_evidence().causes().is_empty());
    assert!(!startup.close_evidence().pending_participants().is_empty());
    assert!(
        startup
            .close_evidence()
            .pending_participants()
            .iter()
            .all(|pending| pending.phase() == RuntimeCloseParticipantPhase::Begin)
    );

    assert!(held.handle.cancel());
    assert!(matches!(
        held.handle.try_take(),
        Some(TaskTerminal::Cancelled(_))
    ));
    let terminal_cause = RuntimeCloseCause::ParticipantError {
        participant: STARTUP_CLOSE_PARTICIPANT,
        phase: RuntimeCloseParticipantPhase::Poll,
        code: STARTUP_CLOSE_FAILURE,
        disposition: RuntimeCloseErrorDisposition::Terminal,
    };
    for late_result in [false, true] {
        if late_result {
            held.release_body_until_drop();
        }
        for _ in 0..4 {
            let before = Instant::now();
            assert_eq!(
                failure.advance_retirement(),
                RuntimeCandidateRetirementState::Retiring
            );
            assert!(
                before.elapsed() < WAIT,
                "retirement polling must be bounded"
            );
            assert!(held.handle.is_finished());
            assert_eq!(held.returned.state(), TaskReturnState::Pending);
            assert!(!custody.is_accepting());
            assert_eq!(close.begins.load(Ordering::SeqCst), 1);
            assert_eq!(close.polls.load(Ordering::SeqCst), 1);
            let owner = failure.retirement.as_deref().unwrap();
            assert!(owner.has_close_failure());
            let EditorAuthoringRetirement::Startup(startup) = owner else {
                panic!("retirement must keep the failed Startup owner");
            };
            assert_eq!(startup.world().id(), world);
            assert_eq!(startup.fault(), &fault);
            let report = startup.world().resource::<TaskShutdownReport>();
            assert!(!report.is_complete());
            assert!(!report.has_cleanup_failure());
            assert_eq!(report.for_kind(TaskPoolKind::Compute).joined_workers, 0);
            let evidence = failure.close_evidence().unwrap();
            assert_eq!(evidence.causes(), &[terminal_cause]);
            assert!(evidence.has_terminal_failure());
            assert!(
                evidence
                    .pending_participants()
                    .iter()
                    .any(|pending| pending.participant().as_str() == "nara.tasks.pools")
            );
            assert_eq!(failure.diagnostics(), &diagnostics);
            assert_eq!(startup_runs.load(Ordering::SeqCst), 1);
            assert_eq!(
                runtime_quarantine_status().current_thread_retained(),
                quarantine_before
            );
        }
    }

    held.release_and_wait(TaskReturnState::Returned);
    let deadline = Instant::now() + WAIT;
    while failure.retirement_state() != RuntimeCandidateRetirementState::Retired {
        assert!(
            Instant::now() < deadline,
            "the failed Startup candidate did not retire"
        );
        let _ = failure.advance_retirement();
        std::thread::sleep(Duration::from_millis(1));
    }
    let owner = failure.retirement.as_deref_mut().unwrap();
    let EditorAuthoringRuntimeCloseProgress::Complete(report) = owner.poll() else {
        panic!("the adapter must return the actual completed task shutdown report");
    };
    assert!(report.is_complete());
    assert!(!report.has_cleanup_failure());
    for (kind, joined) in [
        (TaskPoolKind::Io, 0),
        (TaskPoolKind::Compute, 1),
        (TaskPoolKind::AsyncCompute, 0),
    ] {
        assert_eq!(report.for_kind(kind).joined_workers, joined);
    }
    let EditorAuthoringRetirement::Startup(startup) = &*owner else {
        panic!("completion must retain the original failed Startup owner");
    };
    assert_eq!(startup.world().id(), world);
    assert_eq!(startup.world().resource::<TaskShutdownReport>(), &report);
    assert_eq!(startup.fault(), &fault);
    let evidence = failure.close_evidence().unwrap().clone();
    assert!(evidence.pending_participants().is_empty());
    assert!(evidence.has_terminal_failure());
    assert_eq!(evidence.causes(), &[terminal_cause]);
    for _ in 0..4 {
        assert_eq!(
            failure.advance_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        let owner = failure.retirement.as_deref_mut().unwrap();
        let EditorAuthoringRuntimeCloseProgress::Complete(repeated) = owner.poll() else {
            panic!("completed Startup retirement must remain complete");
        };
        assert_eq!(repeated, report);
        assert!(owner.has_close_failure());
        assert_eq!(failure.close_evidence(), Some(&evidence));
        assert_eq!(failure.diagnostics(), &diagnostics);
        assert_eq!(close.begins.load(Ordering::SeqCst), 1);
        assert_eq!(close.polls.load(Ordering::SeqCst), 1);
        assert_eq!(startup_runs.load(Ordering::SeqCst), 1);
    }
    drop(held);
    drop(result);
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        quarantine_before
    );
}
