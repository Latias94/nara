#[path = "support/domain_reconciliation.rs"]
#[expect(dead_code, reason = "The shared fixture also serves domain-only tests")]
mod domain;
#[path = "support/image_reconciliation.rs"]
mod support;

use std::time::Duration;

use domain::{
    Configuration, Domain, HookOwned, NativeRequired, NativeState, Readiness, Request,
    UnavailableReason, candidate, configuration_entity, empty_candidate, invalid_candidate,
};
use nara_app::{RuntimeControl, RuntimeControlRequestResult, RuntimeState};
use nara_asset::{AssetStates, Assets};
use nara_fs::ContentDigest;
use nara_image::{ImageAsset, ImageReloadStats, PreparedImages, image_resource_key};
use nara_tasks::{TaskPoolKind, TaskPools, TaskShutdownReport};
use support::{IoGate, assert_image_queued, drive_until, with_runtime};

#[test]
fn real_image_completion_uses_latest_accepted_revision_and_cannot_revive_unloaded_native_state() {
    with_runtime(|fixture, runtime| {
        fixture.load_initial_image(runtime);
        let initial_entity = configuration_entity(runtime.world()).unwrap();
        let initial_native = runtime.world().get::<NativeState>(initial_entity).unwrap();
        let initial_identity = initial_native.image.as_ref().unwrap().identity().clone();
        let initial_hash = initial_native
            .image
            .as_ref()
            .unwrap()
            .resource()
            .source_hash();
        assert!(
            !initial_native
                .image
                .as_ref()
                .unwrap()
                .resource()
                .retains_source_allocation()
        );
        assert_eq!(initial_native.allocation(), 1);

        let gate = IoGate::hold(runtime);
        fixture.reload_second_image();
        assert_image_queued(runtime);
        assert!(fixture.budget.snapshot().active_encoded_bytes() > 0);
        assert_eq!(fixture.budget.snapshot().active_reservations(), 1);
        fixture.control.submit(Request::Accept(candidate(2)));
        runtime.drive(Duration::ZERO).unwrap();
        let pending_revision = runtime.world().resource::<Domain>().session.revision();
        assert_eq!(
            runtime.world().resource::<Domain>().readiness,
            Readiness::Pending(pending_revision)
        );
        fixture.control.submit(Request::Accept(candidate(3)));
        runtime.drive(Duration::ZERO).unwrap();
        let accepted_revision = runtime.world().resource::<Domain>().session.revision();
        assert_ne!(accepted_revision, pending_revision);
        let accepted_document = runtime
            .world()
            .resource::<Domain>()
            .session
            .document()
            .clone();
        let accepted_entity = configuration_entity(runtime.world()).unwrap();
        assert_eq!(
            runtime
                .world()
                .get::<Configuration>(accepted_entity)
                .unwrap()
                .value,
            3
        );
        assert!(
            runtime
                .world()
                .get::<NativeState>(accepted_entity)
                .is_none()
        );
        assert_eq!(fixture.control.effects().released, [1]);
        assert!(runtime.world().get_entity(initial_entity).is_err());

        fixture.control.fail_next_creation();
        gate.release();
        drive_until(runtime, |runtime| {
            runtime.world().resource::<Domain>().readiness
                == Readiness::Unavailable(accepted_revision, UnavailableReason::NativeCreation)
        });
        assert_eq!(runtime.world().resource::<ImageReloadStats>().applied, 2);
        assert_eq!(runtime.world().resource::<ImageReloadStats>().pending, 0);
        assert_eq!(
            runtime.world().resource::<Domain>().session.document(),
            &accepted_document
        );
        assert_eq!(
            runtime.world().resource::<Domain>().session.revision(),
            accepted_revision
        );
        assert_eq!(fixture.control.effects().created, [(1, 1)]);
        assert_eq!(
            fixture.control.effects().creation_failures,
            [accepted_revision]
        );
        let failed_effects = fixture.control.effects();
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(fixture.control.effects(), failed_effects);

        fixture.control.submit(Request::Retry);
        runtime.drive(Duration::ZERO).unwrap();
        let entity = configuration_entity(runtime.world()).unwrap();
        let native = runtime.world().get::<NativeState>(entity).unwrap();
        let snapshot = native.image.as_ref().unwrap();
        assert_eq!(native.revision, accepted_revision);
        assert_eq!(native.value, 3);
        assert_eq!(native.allocation(), 2);
        assert_ne!(snapshot.identity(), &initial_identity);
        assert_ne!(snapshot.resource().source_hash(), initial_hash);
        let prepared = runtime
            .world()
            .resource::<PreparedImages>()
            .get(image_resource_key(fixture.handle))
            .unwrap();
        assert_eq!(snapshot.identity(), prepared.identity());
        assert_eq!(snapshot.resource(), prepared.resource());
        let facade_capture = nara_engine::image::PreparedImageSnapshot::capture(
            fixture.handle,
            runtime.world().resource::<Assets<ImageAsset>>(),
            runtime.world().resource::<AssetStates>(),
        )
        .unwrap();
        assert_eq!(snapshot, &facade_capture);
        assert!(!snapshot.resource().retains_source_allocation());
        let retained_snapshot = snapshot.clone();
        let retained_pixel_digest = ContentDigest::of_bytes(snapshot.resource().pixels());
        let retained_pixel_len = snapshot.resource().pixel_len();
        assert!(retained_pixel_len > 0);
        fixture.assert_import_work_released(runtime);
        assert!(runtime.world().get::<NativeRequired>(entity).is_some());
        assert_eq!(
            runtime.world().get::<HookOwned>(entity).unwrap().allocation,
            2
        );
        assert_eq!(fixture.control.effects().created, [(1, 1), (2, 3)]);
        fixture.control.submit(Request::Accept(invalid_candidate()));
        runtime.drive(Duration::ZERO).unwrap();
        assert!(runtime.world().resource::<Domain>().rejected_document);
        assert_eq!(
            runtime.world().resource::<Domain>().session.document(),
            &accepted_document
        );
        assert_eq!(configuration_entity(runtime.world()), Some(entity));
        assert_eq!(fixture.control.effects().created, [(1, 1), (2, 3)]);

        let gate = IoGate::hold(runtime);
        fixture.reload_first_image();
        assert_image_queued(runtime);
        fixture.control.submit(Request::Accept(empty_candidate()));
        runtime.drive(Duration::ZERO).unwrap();
        let unloaded_revision = runtime.world().resource::<Domain>().session.revision();
        assert_eq!(
            runtime.world().resource::<Domain>().readiness,
            Readiness::Ready(unloaded_revision)
        );
        assert!(configuration_entity(runtime.world()).is_none());
        assert!(runtime.world().get_entity(entity).is_err());
        let unloaded_effects = fixture.control.effects();
        assert_eq!(unloaded_effects.released, [1, 2]);
        assert_eq!(unloaded_effects.removed, [(initial_entity, 1), (entity, 2)]);
        assert_eq!(unloaded_effects.required_removed, [initial_entity, entity]);
        assert_eq!(
            unloaded_effects.hook_owned_removed,
            [(initial_entity, 1), (entity, 2)]
        );
        assert!(unloaded_effects.live.is_empty());
        gate.release();
        drive_until(runtime, |runtime| {
            runtime.world().resource::<ImageReloadStats>().applied == 3
        });
        assert_eq!(fixture.control.effects(), unloaded_effects);
        assert!(configuration_entity(runtime.world()).is_none());
        assert!(
            runtime
                .world()
                .resource::<Domain>()
                .session
                .document()
                .entities
                .is_empty()
        );
        assert_eq!(
            runtime.world().resource::<Domain>().session.revision(),
            unloaded_revision
        );

        fixture.remove_image();
        drive_until(runtime, |runtime| {
            runtime
                .world()
                .resource::<Assets<ImageAsset>>()
                .get(fixture.handle)
                .is_none()
                && runtime
                    .world()
                    .resource::<PreparedImages>()
                    .get(image_resource_key(fixture.handle))
                    .is_none()
        });
        assert_eq!(fixture.control.effects(), unloaded_effects);
        assert_eq!(retained_snapshot.resource().pixel_len(), retained_pixel_len);
        assert_eq!(
            ContentDigest::of_bytes(retained_snapshot.resource().pixels()),
            retained_pixel_digest
        );
        fixture.assert_import_work_released(runtime);
    });
}

#[test]
fn real_image_task_close_keeps_incomplete_owner_and_retires_native_effects_exactly_once() {
    with_runtime(|fixture, runtime| {
        fixture.load_initial_image(runtime);
        let entity = configuration_entity(runtime.world()).unwrap();
        let revision = runtime.world().resource::<Domain>().session.revision();
        let accepted_document = runtime
            .world()
            .resource::<Domain>()
            .session
            .document()
            .clone();
        let gate = IoGate::hold(runtime);
        fixture.reload_second_image();
        assert_image_queued(runtime);

        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Accepted(_)
        ));
        drive_until(runtime, |runtime| {
            runtime.state() == RuntimeState::CloseIncomplete
        });
        assert!(runtime.world().contains_resource::<TaskPools>());
        let incomplete = runtime.world().resource::<TaskShutdownReport>();
        assert!(!incomplete.is_complete());
        assert!(incomplete.timed_out());
        assert_eq!(incomplete.for_kind(TaskPoolKind::Io).joined_workers, 0);
        assert!(incomplete.for_kind(TaskPoolKind::Io).cancelled_pending > 0);
        let retired = fixture.control.effects();
        assert_eq!(retired.released, [1]);
        assert_eq!(retired.removed, [(entity, 1)]);
        assert_eq!(retired.required_removed, [entity]);
        assert_eq!(retired.hook_owned_removed, [(entity, 1)]);
        assert!(retired.live.is_empty());
        assert!(runtime.world().get::<NativeState>(entity).is_none());
        assert!(runtime.world().get::<NativeRequired>(entity).is_none());
        assert!(runtime.world().get::<HookOwned>(entity).is_none());
        assert_eq!(
            runtime.world().resource::<Domain>().session.revision(),
            revision
        );
        assert_eq!(
            runtime.world().resource::<Domain>().session.document(),
            &accepted_document
        );

        gate.release();
        assert!(matches!(
            runtime.request_control(RuntimeControl::RetryClose),
            RuntimeControlRequestResult::Accepted(_)
        ));
        drive_until(runtime, |runtime| runtime.state() == RuntimeState::Stopped);
        let complete = runtime.world().resource::<TaskShutdownReport>();
        assert!(complete.is_complete());
        for kind in TaskPoolKind::ALL {
            assert_eq!(complete.for_kind(kind).joined_workers, 1);
        }
        assert!(!runtime.world().contains_resource::<TaskPools>());
        assert_eq!(fixture.control.effects(), retired);
        assert_eq!(runtime.world().resource::<ImageReloadStats>().applied, 1);
        assert!(
            runtime
                .world()
                .resource::<Assets<ImageAsset>>()
                .get(fixture.handle)
                .is_some()
        );
        let image = runtime
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(fixture.handle)
            .unwrap();
        let prepared = runtime
            .world()
            .resource::<PreparedImages>()
            .get(image_resource_key(fixture.handle))
            .unwrap();
        assert!(!image.pixels().is_empty());
        assert_eq!(prepared.resource().pixels(), image.pixels());
        assert!(!prepared.resource().retains_source_allocation());
        fixture.assert_import_work_released(runtime);
    });
}
