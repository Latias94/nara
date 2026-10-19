use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginDomainError, PluginDomainErrorError, PluginError, PluginFailure, PluginHook, PluginId,
    PluginLifecycleState, PluginPlan, PluginPreflightContext, PluginShutdownContext,
    RuntimeAdmissionReservation, RuntimeCandidateRetirementState, RuntimeClosePolicy,
    RuntimeInstance, RuntimeObligationLedger,
};

const ACTUAL: PluginId = PluginId::new("test.domain.actual");
const FALSE: PluginId = PluginId::new("test.domain.false");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ACTUAL, PluginCategory::Core);

fn domain() -> PluginDomainError {
    PluginDomainError::new(
        "test.plugin.unavailable",
        "Required plugin capability is unavailable",
    )
    .unwrap()
}

struct RejectingPlugin {
    hook: PluginHook,
    shutdowns: Arc<AtomicUsize>,
}

impl Plugin for RejectingPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn preflight(&self, _: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        if self.hook == PluginHook::Preflight {
            Err(domain().into())
        } else {
            Ok(())
        }
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        if self.hook == PluginHook::Build {
            Err(domain().into())
        } else {
            Ok(())
        }
    }

    fn finish(&self, _: &mut App) -> Result<(), PluginError> {
        if self.hook == PluginHook::Finish {
            Err(domain().into())
        } else {
            Ok(())
        }
    }

    fn shutdown(&self, _: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
        Err(
            PluginDomainError::new("test.plugin.close-failed", "Plugin shutdown failed")
                .unwrap()
                .into(),
        )
    }
}

fn definition(hook: PluginHook, shutdowns: &Arc<AtomicUsize>) -> PluginDefinition {
    let shutdowns = Arc::clone(shutdowns);
    PluginDefinition::infallible::<RejectingPlugin, _>(
        PluginDefinitionId::new("test.domain.definition", 1),
        format!("{hook:?}"),
        move || RejectingPlugin {
            hook,
            shutdowns: Arc::clone(&shutdowns),
        },
    )
}

fn assert_primary(failure: &PluginFailure, hook: PluginHook) {
    assert_eq!(failure.plugin(), ACTUAL);
    assert_eq!(failure.hook(), hook);
    assert_eq!(failure.error().domain_error(), Some(&domain()));
}

#[test]
fn static_plugin_facts_reject_unsafe_and_oversized_metadata() {
    for code in ["", "UPPER", "test/path", "test:code", "test code"] {
        assert_eq!(
            PluginDomainError::new(code, "Safe summary"),
            Err(PluginDomainErrorError::InvalidCode)
        );
    }
    for summary in [
        "",
        "bad\nsummary",
        "bad\u{202e}summary",
        "C:\\private\\file",
        "/private/file",
        "https://example.com",
        "Bearer sensitive",
    ] {
        assert_eq!(
            PluginDomainError::new("test.safe", summary),
            Err(PluginDomainErrorError::InvalidSummary)
        );
    }
    let oversized_code = Box::leak(
        "a".repeat(PluginDomainError::MAX_CODE_BYTES + 1)
            .into_boxed_str(),
    );
    let oversized_summary = Box::leak(
        "a".repeat(PluginDomainError::MAX_SUMMARY_BYTES + 1)
            .into_boxed_str(),
    );
    assert_eq!(
        PluginDomainError::new(oversized_code, "Safe summary"),
        Err(PluginDomainErrorError::InvalidCode)
    );
    assert_eq!(
        PluginDomainError::new("test.safe", oversized_summary),
        Err(PluginDomainErrorError::InvalidSummary)
    );
    assert_eq!(
        domain().summary(),
        "Required plugin capability is unavailable"
    );
}

#[test]
fn first_preflight_retains_actual_context_without_installation_or_poison() {
    let shutdowns = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    let error = match app.add_plugins(RejectingPlugin {
        hook: PluginHook::Preflight,
        shutdowns: Arc::clone(&shutdowns),
    }) {
        Ok(_) => panic!("the first preflight must reject"),
        Err(error) => error,
    };
    assert_primary(error.plugin_failure().unwrap(), PluginHook::Preflight);
    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );
    assert!(app.plugin_failure_report().is_none());
    let plan = PluginPlan::resolve(definition(PluginHook::Preflight, &shutdowns)).unwrap();
    let failure = plan.instantiate_retained().unwrap_err();
    assert_primary(failure.plugin_failure().unwrap(), PluginHook::Preflight);
    assert!(failure.plugin_failure_report().is_none());
    let mut failure = plan
        .instantiate_runtime_candidate(
            RuntimeAdmissionReservation::try_acquire().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap_err();
    assert_primary(failure.plugin_failure().unwrap(), PluginHook::Preflight);
    assert!(failure.plugin_failure_report().is_none());
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(shutdowns.load(Ordering::SeqCst), 0);
}

#[test]
fn build_and_finish_preserve_primary_and_secondary_facts_until_close_once() {
    for hook in [PluginHook::Build, PluginHook::Finish] {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let plan = PluginPlan::resolve(definition(hook, &shutdowns)).unwrap();
        let mut failure = plan
            .instantiate_runtime_candidate(
                RuntimeAdmissionReservation::try_acquire().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap_err();
        assert_primary(failure.plugin_failure().unwrap(), hook);
        let report = failure.plugin_failure_report().unwrap();
        assert_primary(report.primary().unwrap(), hook);
        assert_eq!(report.shutdown_failures().len(), 1);
        assert_eq!(report.shutdown_failures()[0].plugin(), ACTUAL);
        assert_eq!(report.shutdown_failures()[0].hook(), PluginHook::Shutdown);
        assert_eq!(
            report.shutdown_failures()[0]
                .error()
                .domain_error()
                .unwrap()
                .code(),
            "test.plugin.close-failed"
        );
        assert!(report.shutdown_complete());
        assert_eq!(
            failure.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        assert_eq!(
            failure.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        assert_primary(failure.plugin_failure().unwrap(), hook);
        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn preparation_uses_checked_facts_without_fabricating_a_hook() {
    let plan = PluginPlan::resolve(PluginDefinition::fallible::<RejectingPlugin, _>(
        PluginDefinitionId::new("test.domain.factory", 1),
        [],
        || Err(domain()),
    ))
    .unwrap();
    let failure = plan.instantiate_retained().unwrap_err();
    let error = failure.error().prepare_error().unwrap();
    assert_eq!(error.code(), domain().code());
    assert_eq!(error.summary(), domain().summary());
    assert!(failure.plugin_failure().is_none());
    assert!(failure.plugin_failure_report().is_none());
}

#[test]
fn late_preflight_preserves_safe_facts_through_the_committed_wrapper() {
    struct Prefix(Arc<AtomicUsize>);
    impl Plugin for Prefix {
        fn declaration() -> &'static PluginDeclaration {
            static DECLARATION: PluginDeclaration =
                PluginDeclaration::new(PluginId::new("test.domain.prefix"), PluginCategory::Core);
            &DECLARATION
        }
        fn build(&self, _: &mut App) -> Result<(), PluginError> {
            Ok(())
        }
        fn shutdown(&self, _: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    let prefix_shutdowns = Arc::new(AtomicUsize::new(0));
    let rejected_shutdowns = Arc::new(AtomicUsize::new(0));
    let retained = Arc::clone(&prefix_shutdowns);
    let prefix = PluginDefinition::infallible::<Prefix, _>(
        PluginDefinitionId::new("test.domain.prefix-definition", 1),
        [],
        move || Prefix(Arc::clone(&retained)),
    );
    let plan = PluginPlan::resolve((
        prefix,
        definition(PluginHook::Preflight, &rejected_shutdowns),
    ))
    .unwrap();
    let failure = plan.instantiate_retained().unwrap_err();
    let invocation = failure.plugin_failure().unwrap();
    assert_primary(invocation, PluginHook::Preflight);
    assert!(matches!(
        invocation.error(),
        PluginError::CommittedPreflightRejected { plugin: ACTUAL, .. }
    ));
    assert_primary(
        failure.plugin_failure_report().unwrap().primary().unwrap(),
        PluginHook::Preflight,
    );
    assert_eq!(prefix_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(rejected_shutdowns.load(Ordering::SeqCst), 0);
}

#[test]
fn invocation_context_ignores_self_reported_legacy_identity() {
    struct FalseIdentity;
    impl Plugin for FalseIdentity {
        fn declaration() -> &'static PluginDeclaration {
            &DECLARATION
        }
        fn build(&self, _: &mut App) -> Result<(), PluginError> {
            Err(PluginError::HookPanicked {
                plugin: FALSE,
                hook: PluginHook::Shutdown,
            })
        }
    }
    let plan = PluginPlan::resolve(PluginDefinition::infallible::<FalseIdentity, _>(
        PluginDefinitionId::new("test.domain.false-identity", 1),
        [],
        || FalseIdentity,
    ))
    .unwrap();
    let failure = plan.instantiate_retained().unwrap_err();
    let context = failure.plugin_failure().unwrap();
    assert_eq!(context.plugin(), ACTUAL);
    assert_eq!(context.hook(), PluginHook::Build);

    struct FalsePreflightIdentity;
    impl Plugin for FalsePreflightIdentity {
        fn declaration() -> &'static PluginDeclaration {
            &DECLARATION
        }
        fn preflight(&self, _: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
            Err(PluginError::HookPanicked {
                plugin: FALSE,
                hook: PluginHook::Shutdown,
            })
        }
        fn build(&self, _: &mut App) -> Result<(), PluginError> {
            unreachable!()
        }
    }
    let mut app = App::new();
    let error = match app.add_plugins(FalsePreflightIdentity) {
        Ok(_) => panic!("forged legacy error still rejects the actual preflight invocation"),
        Err(error) => error,
    };
    assert_eq!(error.plugin_failure().unwrap().plugin(), ACTUAL);
    assert_eq!(
        error.plugin_failure().unwrap().hook(),
        PluginHook::Preflight
    );
    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );
    assert!(app.plugin_failure_report().is_none());
    let plan = PluginPlan::resolve(PluginDefinition::infallible::<FalsePreflightIdentity, _>(
        PluginDefinitionId::new("test.domain.false-preflight", 1),
        [],
        || FalsePreflightIdentity,
    ))
    .unwrap();
    let failure = plan.instantiate_retained().unwrap_err();
    assert_eq!(failure.plugin_failure().unwrap().plugin(), ACTUAL);
    assert_eq!(
        failure.plugin_failure().unwrap().hook(),
        PluginHook::Preflight
    );
    assert!(failure.plugin_failure_report().is_none());
}

#[test]
fn ordinary_runtime_retirement_exposes_shutdown_facts_after_owner_transfer() {
    let shutdowns = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    app.add_plugins(RejectingPlugin {
        hook: PluginHook::Shutdown,
        shutdowns: Arc::clone(&shutdowns),
    })
    .unwrap();
    let runtime = RuntimeInstance::start(app).unwrap();
    assert!(runtime.plugin_failure_report().is_none());
    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    let report = retirement.plugin_failure_report().unwrap();
    assert!(report.primary().is_none());
    assert_eq!(report.shutdown_failures().len(), 1);
    assert_eq!(
        report.shutdown_failures()[0]
            .error()
            .domain_error()
            .unwrap()
            .code(),
        "test.plugin.close-failed"
    );
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn direct_seal_preserves_second_plugin_finish_context_after_consuming_app() {
    struct FirstPlugin(Arc<AtomicUsize>);
    impl Plugin for FirstPlugin {
        fn declaration() -> &'static PluginDeclaration {
            static FIRST: PluginDeclaration =
                PluginDeclaration::new(PluginId::new("test.domain.first"), PluginCategory::Core);
            &FIRST
        }

        fn build(&self, _: &mut App) -> Result<(), PluginError> {
            Ok(())
        }

        fn shutdown(&self, _: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let first_shutdowns = Arc::new(AtomicUsize::new(0));
    let second_shutdowns = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    app.add_plugins((
        FirstPlugin(Arc::clone(&first_shutdowns)),
        RejectingPlugin {
            hook: PluginHook::Finish,
            shutdowns: Arc::clone(&second_shutdowns),
        },
    ))
    .unwrap();

    let error = app.seal().unwrap_err();
    assert_primary(error.plugin_failure().unwrap(), PluginHook::Finish);
    assert_eq!(error.error().domain_error(), Some(&domain()));
    assert_eq!(first_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(second_shutdowns.load(Ordering::SeqCst), 1);
    drop(error);
    assert_eq!(first_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(second_shutdowns.load(Ordering::SeqCst), 1);
}
