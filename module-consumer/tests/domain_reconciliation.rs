#[path = "support/domain_reconciliation.rs"]
mod support;

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::Duration,
};

use nara_app::{RuntimeCandidateRetirementState, RuntimeInstance, RuntimeStartError};
use support::{
    Configuration, Control, Domain, HookOwned, NativeRequired, NativeState, Readiness, Request,
    UnavailableReason, candidate, configuration_entity, empty_candidate, invalid_candidate, start,
};

#[test]
fn persistent_configuration_and_hook_owned_native_state_share_one_entity() {
    let control = Control::default();
    let entity = with_runtime(&control, |runtime| {
        let world = runtime.world();
        let domain = world.resource::<Domain>();
        let revision = domain.session.revision();
        let entity = configuration_entity(world).unwrap();
        let native = world.get::<NativeState>(entity).unwrap();

        assert_eq!(domain.readiness, Readiness::Ready(revision));
        assert_eq!(world.get::<Configuration>(entity).unwrap().value, 1);
        assert_eq!(native.revision, revision);
        assert_eq!(native.value, 1);
        assert!(world.get::<NativeRequired>(entity).is_some());
        assert_eq!(
            world.get::<HookOwned>(entity).unwrap().allocation,
            native.allocation()
        );
        let effects = control.effects();
        assert_eq!(effects.accepted, [revision]);
        assert_eq!(effects.created, [(native.allocation(), 1)]);
        assert_eq!(effects.added, [(entity, native.allocation())]);

        entity
    });
    let closed = control.effects();
    assert!(closed.live.is_empty());
    assert_eq!(closed.released, [1]);
    assert_eq!(closed.removed, [(entity, 1)]);
    assert_eq!(closed.required_removed, [entity]);
    assert_eq!(closed.hook_owned_removed, [(entity, 1)]);
}

#[test]
fn legacy_lifecycle_free_sync_and_clear_reject_the_same_entity_native_owner() {
    let control = Control::default();
    with_runtime(&control, |runtime| {
        let entity = configuration_entity(runtime.world()).unwrap();
        let revision = runtime.world().resource::<Domain>().session.revision();
        let before = control.effects();

        control.submit(Request::LegacySync);
        runtime.drive(Duration::ZERO).unwrap();
        assert!(runtime.world().resource::<Domain>().legacy_sync_rejected);
        control.submit(Request::LegacyClear);
        runtime.drive(Duration::ZERO).unwrap();

        let domain = runtime.world().resource::<Domain>();
        assert!(domain.legacy_clear_rejected);
        assert_eq!(domain.session.revision(), revision);
        assert_eq!(domain.readiness, Readiness::Ready(revision));
        assert_eq!(configuration_entity(runtime.world()), Some(entity));
        assert_eq!(control.effects(), before);
    });
}

#[test]
fn invalid_document_preserves_the_accepted_revision_and_native_effects() {
    let control = Control::default();
    with_runtime(&control, |runtime| {
        let entity = configuration_entity(runtime.world()).unwrap();
        let revision = runtime.world().resource::<Domain>().session.revision();
        let document = runtime
            .world()
            .resource::<Domain>()
            .session
            .document()
            .clone();
        let before = control.effects();

        control.submit(Request::Accept(invalid_candidate()));
        runtime.drive(Duration::ZERO).unwrap();

        let domain = runtime.world().resource::<Domain>();
        assert!(domain.rejected_document);
        assert_eq!(domain.session.revision(), revision);
        assert_eq!(domain.session.document(), &document);
        assert_eq!(domain.readiness, Readiness::Ready(revision));
        assert_eq!(configuration_entity(runtime.world()), Some(entity));
        assert_eq!(control.effects(), before);
    });
}

#[test]
fn accepted_replacement_failure_requires_explicit_retry_and_does_not_restore_old_native_state() {
    let control = Control::default();
    with_runtime(&control, |runtime| {
        let old_entity = configuration_entity(runtime.world()).unwrap();
        let old_revision = runtime.world().resource::<Domain>().session.revision();
        control.fail_next_creation();

        control.submit(Request::Accept(candidate(2)));
        runtime.drive(Duration::ZERO).unwrap();

        let domain = runtime.world().resource::<Domain>();
        let accepted_revision = domain.session.revision();
        assert_ne!(accepted_revision, old_revision);
        assert!(!domain.rejected_document);
        assert_eq!(
            domain.readiness,
            Readiness::Unavailable(accepted_revision, UnavailableReason::NativeCreation),
            "native retirement belongs before the strict persistent replacement transaction"
        );
        let effects = control.effects();
        assert_eq!(effects.accepted, [old_revision, accepted_revision]);
        assert_eq!(effects.creation_failures, [accepted_revision]);
        assert_eq!(effects.released, [1]);
        assert_eq!(effects.removed, [(old_entity, 1)]);
        assert_eq!(effects.required_removed, [old_entity]);
        assert_eq!(effects.hook_owned_removed, [(old_entity, 1)]);
        assert!(effects.live.is_empty());
        assert!(runtime.world().get_entity(old_entity).is_err());
        let failed_entity = configuration_entity(runtime.world()).unwrap();
        assert_eq!(
            runtime
                .world()
                .get::<Configuration>(failed_entity)
                .unwrap()
                .value,
            2
        );
        assert!(runtime.world().get::<NativeState>(failed_entity).is_none());

        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(
            control.effects(),
            effects,
            "ordinary frames must not silently retry"
        );
        assert_eq!(
            runtime.world().resource::<Domain>().session.revision(),
            accepted_revision
        );
        control.submit(Request::Retry);
        runtime.drive(Duration::ZERO).unwrap();

        let world = runtime.world();
        assert_eq!(
            world.resource::<Domain>().readiness,
            Readiness::Ready(accepted_revision)
        );
        assert_eq!(
            world.resource::<Domain>().session.revision(),
            accepted_revision
        );
        let entity = configuration_entity(world).unwrap();
        let native = world.get::<NativeState>(entity).unwrap();
        assert_eq!(native.revision, accepted_revision);
        assert_eq!(native.value, 2);
        assert!(world.get::<NativeRequired>(entity).is_some());
        assert_eq!(
            world.get::<HookOwned>(entity).unwrap().allocation,
            native.allocation()
        );
        assert_eq!(control.effects().live.len(), 1);
        assert_eq!(control.effects().created, [(1, 1), (2, 2)]);
    });
    assert!(control.effects().live.is_empty());
    assert_eq!(control.effects().released, [1, 2]);
}

#[test]
fn accepted_replace_and_remove_retire_required_and_hook_owned_native_state() {
    let control = Control::default();
    let effects = with_runtime(&control, |runtime| {
        let old_entity = configuration_entity(runtime.world()).unwrap();

        control.submit(Request::Accept(candidate(3)));
        runtime.drive(Duration::ZERO).unwrap();

        let world = runtime.world();
        let revision = world.resource::<Domain>().session.revision();
        assert_eq!(
            world.resource::<Domain>().readiness,
            Readiness::Ready(revision)
        );
        assert!(world.get_entity(old_entity).is_err());
        let replacement = configuration_entity(world).unwrap();
        assert_eq!(world.get::<Configuration>(replacement).unwrap().value, 3);
        assert_eq!(world.get::<NativeState>(replacement).unwrap().value, 3);
        assert!(world.get::<NativeRequired>(replacement).is_some());
        assert!(world.get::<HookOwned>(replacement).is_some());
        assert_eq!(control.effects().released, [1]);

        control.submit(Request::Accept(empty_candidate()));
        runtime.drive(Duration::ZERO).unwrap();

        let world = runtime.world();
        let domain = world.resource::<Domain>();
        assert_eq!(
            domain.readiness,
            Readiness::Ready(domain.session.revision())
        );
        assert!(domain.session.document().entities.is_empty());
        assert!(configuration_entity(world).is_none());
        assert!(world.get_entity(replacement).is_err());
        let effects = control.effects();
        assert_eq!(effects.released, [1, 2]);
        assert_eq!(effects.removed, [(old_entity, 1), (replacement, 2)]);
        assert_eq!(effects.required_removed, [old_entity, replacement]);
        assert_eq!(
            effects.hook_owned_removed,
            [(old_entity, 1), (replacement, 2)]
        );
        assert!(effects.live.is_empty());
        effects
    });
    assert_eq!(
        control.effects(),
        effects,
        "close must not clean an allocation twice"
    );
}

#[test]
fn native_startup_failure_never_returns_a_published_runtime() {
    let control = Control::default();
    control.fail_next_creation();

    let mut failure = match start(&control) {
        Err(failure) => failure,
        Ok(runtime) => {
            runtime.finish().unwrap();
            panic!("native startup failure must remain unpublished");
        }
    };

    let startup_error = matches!(failure.error(), RuntimeStartError::Startup(_));
    let retirement = failure.drive_retirement();
    drop(failure);
    assert!(startup_error);
    assert_eq!(retirement, RuntimeCandidateRetirementState::Retired);
    let effects = control.effects();
    assert_eq!(effects.accepted.len(), 1);
    assert_eq!(effects.creation_failures, effects.accepted);
    assert!(effects.created.is_empty());
    assert!(effects.added.is_empty());
    assert!(effects.live.is_empty());
}

/// Keep the managed owner outside assertion unwinding, close it once, then resume the failure.
fn with_runtime<T>(control: &Control, exercise: impl FnOnce(&mut RuntimeInstance) -> T) -> T {
    let mut runtime = match start(control) {
        Ok(runtime) => runtime,
        Err(mut failure) => {
            let error = format!("{failure:?}");
            failure.drive_retirement();
            drop(failure);
            panic!("native domain startup failed: {error}");
        }
    };
    let result = catch_unwind(AssertUnwindSafe(|| exercise(&mut runtime)));
    let closed = runtime.finish().map_err(|owner| {
        let error = owner.to_string();
        // Drop outside unwinding so the retained owner gets its finite close attempt.
        drop(owner);
        error
    });
    closed.expect("the native domain fixture must close physically");
    match result {
        Ok(value) => value,
        Err(failure) => resume_unwind(failure),
    }
}
