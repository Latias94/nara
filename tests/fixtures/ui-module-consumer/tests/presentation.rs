//! The backend below is a simulated trusted CPU backend, not native GPU presentation evidence.
#![cfg(feature = "presentation")]
#![forbid(unsafe_code)]

use std::sync::Arc;

use nara_asset::{AssetEvents, AssetStates, Assets, allocate_anonymous_handle};
use nara_image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat};
use nara_render::{RenderResourceKind, RenderTarget};
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiActionId, UiActivationSource, UiAuthoringSnapshotBuilder, UiBrush, UiColorRole, UiCompiler,
    UiEnvironment, UiFrame, UiImageId, UiInputEvent, UiInteractionSession, UiKey, UiLayer,
    UiLocalKey, UiPresentationReceipt, UiRoot, UiRootKey, UiRouteStamp, UiSeatId, UiSize, UiStyle,
    UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme, UiVisual,
};
use nara_ui_module_consumer::presentation::{CapturedUiImage, SWATCH_RGBA, capture_swatch};
use nara_ui_render::{
    UiPaintPacket, UiPreparedImages, UiPresentationBackendGeneration, UiPresentationContentError,
    UiPresentationOutcome, UiSurfacePresentationError, UiSurfacePresentationEvent,
    UiSurfacePresenter,
    advanced::{
        UiBackendPresentationFrame, UiBackendSubmission, UiSubmittedBackendSubmission,
        bind_backend_submission_batch,
    },
};
use nara_ui_widgets::{UiButton, UiButtonState, UiIntent, UiIntentBatch};

struct Consumer {
    surface: UiSurfaceId,
    compiler: UiCompiler,
    presenter: UiSurfacePresenter,
    image: CapturedUiImage,
    button: UiButton,
}

fn presenter(surface: UiSurfaceId) -> UiSurfacePresenter {
    UiSurfacePresenter::with_fonts(
        surface,
        7,
        RenderTarget::PrimaryWindow,
        vec![bundled_font_face().unwrap()],
        Default::default(),
        Default::default(),
    )
    .unwrap()
}

impl Consumer {
    fn new(surface: u64) -> Self {
        let surface = UiSurfaceId::new(surface).unwrap();
        let root = UiRootKey::runtime("neutral-presentation", "root").unwrap();
        Self {
            surface,
            compiler: UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap(),
            presenter: presenter(surface),
            image: capture_swatch(Arc::new(())).unwrap(),
            button: UiButton::new(
                UiKey::scoped(
                    root,
                    UiLocalKey::runtime("neutral-presentation", "apply").unwrap(),
                )
                .unwrap(),
                "Apply",
                UiActionId::new("apply").unwrap(),
            )
            .unwrap(),
        }
    }

    fn frame(&mut self, revision: u64) -> Arc<UiFrame> {
        let root = self.button.key().root_key();
        let snapshot = UiAuthoringSnapshotBuilder::new(revision)
            .with_root(
                UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill())
                    .with_child(self.image.authored_node(
                        UiLocalKey::runtime("neutral-presentation", "image").unwrap(),
                        UiStyle::absolute(8.0, 8.0, 32.0, 32.0),
                    ))
                    .with_child(
                        self.button
                            .authored_node(
                                root,
                                UiStyle::absolute(8.0, 48.0, 120.0, 32.0),
                                UiVisual::role(UiColorRole::Accent),
                                UiBrush::Role(UiColorRole::PrimaryText),
                                16.0,
                            )
                            .unwrap(),
                    ),
            )
            .build()
            .unwrap();
        let environment = UiEnvironment::for_surface(
            self.surface,
            UiSurfaceContentTransform::direct(UiSize::new(160.0, 96.0).unwrap(), 1.25).unwrap(),
            UiSurfaceGenerations::new(7, revision, revision, revision).unwrap(),
            1,
            1,
        )
        .unwrap();
        let frame = self
            .compiler
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap();
        assert!(frame.diagnostics().is_empty());
        frame
    }

    fn prepare(&mut self, frame: Arc<UiFrame>) -> UiBackendPresentationFrame {
        self.presenter
            .prepare(frame, &self.image.prepared_images())
            .unwrap()
    }
}

fn original(backend: UiBackendPresentationFrame) -> UiBackendSubmission {
    let (packets, mut submissions) = backend.into_parts();
    assert_eq!((packets.len(), submissions.len()), (1, 1));
    let submission = submissions.pop().unwrap();
    assert!(Arc::ptr_eq(&packets[0], submission.submission().packet()));
    submission
}

fn cpu_backend(backend: UiBackendPresentationFrame) -> UiSubmittedBackendSubmission {
    bind_backend_submission_batch(
        vec![original(backend)],
        UiPresentationBackendGeneration::new(17, 3).unwrap(),
    )
    .unwrap()
    .pop()
    .unwrap()
}

fn presented(events: Vec<UiSurfacePresentationEvent>) -> UiPresentationReceipt {
    let [UiSurfacePresentationEvent::Presented(receipt)] = events.as_slice() else {
        panic!("expected the original acceptance: {events:?}");
    };
    *receipt
}

fn simulate_presentation(consumer: &mut Consumer, frame: Arc<UiFrame>) -> UiPresentationReceipt {
    let completion = cpu_backend(consumer.prepare(frame)).presented();
    consumer.presenter.return_outcome(completion).unwrap();
    presented(consumer.presenter.drain().unwrap())
}

fn assert_capture(packet: &UiPaintPacket, image: &CapturedUiImage) {
    let [captured] = packet.images() else {
        panic!("expected one retained image: {:?}", packet.images());
    };
    assert_eq!(captured.id(), image.id());
    assert_eq!(captured.snapshot(), image.snapshot());
    assert_eq!(
        captured.snapshot().identity().key().asset_id(),
        image.handle().id(),
    );
    assert_eq!(
        captured.snapshot().identity().key().kind(),
        RenderResourceKind::IMAGE_2D,
    );
    assert_eq!(captured.snapshot().resource().format(), ImageFormat::Rgba8);
    assert_eq!(
        captured.snapshot().resource().color_space(),
        ImageColorSpace::Srgb,
    );
    assert_eq!(
        captured.snapshot().resource().pixels(),
        SWATCH_RGBA.as_slice()
    );
    assert_eq!(
        captured.snapshot().resource().pixels().as_ptr(),
        image.snapshot().resource().pixels().as_ptr(),
    );
    assert!(captured.snapshot().resource().has_retention_owner());
}

#[test]
fn preparation_cannot_publish_and_completion_preserves_the_callers_interaction_state() {
    let mut consumer = Consumer::new(211);
    let frame = consumer.frame(1);
    let submitted = cpu_backend(consumer.prepare(Arc::clone(&frame)));
    let identity = submitted.submission().identity();
    assert_capture(submitted.submission().packet(), &consumer.image);
    assert!(
        consumer
            .presenter
            .journal()
            .latest(consumer.surface)
            .is_err()
    );
    assert!(!consumer.presenter.is_quiescent());
    consumer
        .presenter
        .return_outcome(submitted.presented())
        .unwrap();
    assert!(
        consumer
            .presenter
            .journal()
            .latest(consumer.surface)
            .is_err()
    );
    let receipt = presented(consumer.presenter.drain().unwrap());
    assert!(identity.matches_receipt(receipt));
    let route = UiRouteStamp::from_receipt(receipt);
    assert!(Arc::ptr_eq(
        consumer.presenter.journal().resolve(route).unwrap().frame(),
        &frame,
    ));

    let mut interaction = UiInteractionSession::default();
    let mut state = UiButtonState::default();
    let mut intents = UiIntentBatch::default();
    for input in [
        UiInputEvent::FocusTarget {
            seat: UiSeatId::PRIMARY,
            target: consumer.button.key().clone(),
        },
        UiInputEvent::ActivateFocused {
            seat: UiSeatId::PRIMARY,
            source: UiActivationSource::Keyboard,
        },
    ] {
        let dispatched = interaction
            .dispatch(consumer.presenter.journal(), route, input)
            .unwrap();
        state
            .reduce_batch(
                &consumer.button,
                dispatched.consumed().unwrap(),
                &mut intents,
            )
            .unwrap();
    }
    assert!(state.focused());
    assert!(
        matches!(intents.intents(), [UiIntent::Activate { target, action, .. }]
        if target == consumer.button.key() && action == consumer.button.action())
    );

    let next = consumer.frame(2);
    let pending = cpu_backend(consumer.prepare(next));
    let cancelled = interaction
        .dispatch(
            consumer.presenter.journal(),
            route,
            UiInputEvent::SeatCancelled {
                seat: UiSeatId::PRIMARY,
            },
        )
        .unwrap();
    state
        .reduce_batch(
            &consumer.button,
            cancelled.consumed().unwrap(),
            &mut intents,
        )
        .unwrap();
    assert!(!state.focused());
    consumer
        .presenter
        .return_outcome(pending.presented())
        .unwrap();
    presented(consumer.presenter.drain().unwrap());
    if let Some(reconciled) = interaction
        .reconcile_latest_presentation(consumer.presenter.journal(), consumer.surface, &[])
        .unwrap()
    {
        state
            .reduce_batch(&consumer.button, &reconciled, &mut intents)
            .unwrap();
    }
    assert!(!state.focused());
    assert_eq!(intents.intents().len(), 1);
    assert!(consumer.presenter.is_quiescent());
}

#[test]
fn foreign_completion_returns_the_original_unchanged_even_with_equal_nominal_identities() {
    let mut consumer = Consumer::new(212);
    let mut foreign = presenter(consumer.surface);
    let frame = consumer.frame(1);
    let images = consumer.image.prepared_images();
    let owner_submission = cpu_backend(
        consumer
            .presenter
            .prepare(Arc::clone(&frame), &images)
            .unwrap(),
    );
    let foreign_submission = cpu_backend(foreign.prepare(Arc::clone(&frame), &images).unwrap());
    let identity = owner_submission.submission().identity();
    let packet = Arc::clone(owner_submission.submission().packet());
    assert_eq!(foreign_submission.submission().identity(), identity);
    let returned = foreign
        .return_outcome(owner_submission.presented())
        .unwrap_err();
    assert!(foreign.drain().is_err());
    assert!(foreign.journal().latest(consumer.surface).is_err());
    assert!(
        consumer
            .presenter
            .journal()
            .latest(consumer.surface)
            .is_err()
    );
    consumer.presenter.return_outcome(returned).unwrap();
    let receipt = presented(consumer.presenter.drain().unwrap());
    assert!(identity.matches_receipt(receipt));
    assert!(Arc::ptr_eq(
        consumer
            .presenter
            .journal()
            .latest(consumer.surface)
            .unwrap()
            .frame(),
        &packet.frame(),
    ));
    assert_capture(&packet, &consumer.image);
    foreign
        .return_outcome(foreign_submission.presented())
        .unwrap();
    assert!(identity.matches_receipt(presented(foreign.drain().unwrap())));
    assert!(consumer.presenter.is_quiescent() && foreign.is_quiescent());
    assert!(consumer.presenter.drain().unwrap().is_empty());
}

#[test]
fn deferred_retry_keeps_the_original_packet_and_required_backend_generation() {
    let mut consumer = Consumer::new(213);
    let frame = consumer.frame(1);
    let pending = cpu_backend(consumer.prepare(frame));
    let identity = pending.submission().identity();
    let packet = Arc::clone(pending.submission().packet());
    let generation = pending.submission().backend_generation();
    consumer
        .presenter
        .return_outcome(pending.deferred("CPU target temporarily unavailable".into()))
        .unwrap();
    assert!(matches!(
        consumer.presenter.drain().unwrap().as_slice(),
        [UiSurfacePresentationEvent::Deferred { attempt, .. }]
            if attempt.backend_submission() == Some(identity)
    ));
    assert!(
        consumer
            .presenter
            .journal()
            .latest(consumer.surface)
            .is_err()
    );
    assert!(!consumer.presenter.is_quiescent());
    let retry = original(consumer.presenter.retry().unwrap());
    assert_eq!(retry.submission().identity(), identity);
    assert_eq!(
        retry.submission().required_backend_generation(),
        Some(generation)
    );
    assert!(Arc::ptr_eq(retry.submission().packet(), &packet));
    let wrong = UiPresentationBackendGeneration::new(
        generation.backend_instance_id(),
        generation.device_epoch() + 1,
    )
    .unwrap();
    let refused = bind_backend_submission_batch(vec![retry], wrong).unwrap_err();
    assert_eq!(refused.required(), generation);
    assert_eq!(refused.actual(), wrong);
    let originals = refused.into_submissions();
    assert_eq!(originals.len(), 1);
    assert_eq!(originals[0].submission().identity(), identity);
    assert!(Arc::ptr_eq(originals[0].submission().packet(), &packet));
    assert!(consumer.presenter.drain().is_err());
    let retried = bind_backend_submission_batch(originals, generation)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(retried.submission().backend_generation(), generation);
    assert_capture(retried.submission().packet(), &consumer.image);
    consumer
        .presenter
        .return_outcome(retried.presented())
        .unwrap();
    assert!(identity.matches_receipt(presented(consumer.presenter.drain().unwrap())));
    assert!(consumer.presenter.is_quiescent());
}

#[test]
fn dropping_and_retiring_settle_original_permits_without_reviving_input_routes() {
    for outcome in 0..3 {
        let mut consumer = Consumer::new(220 + outcome);
        let first = consumer.frame(1);
        let abandoned = original(consumer.prepare(Arc::clone(&first)));
        let identity = abandoned.submission().identity();
        drop(abandoned);
        assert!(matches!(consumer.presenter.drain().unwrap().as_slice(),
            [UiSurfacePresentationEvent::Terminal(UiPresentationOutcome::Aborted { attempt, .. })]
                if attempt.backend_submission() == Some(identity)));
        assert!(consumer.presenter.is_quiescent());
        assert!(
            consumer
                .presenter
                .journal()
                .latest(consumer.surface)
                .is_err()
        );
        let receipt = simulate_presentation(&mut consumer, first);
        let route = UiRouteStamp::from_receipt(receipt);
        let next = consumer.frame(2);
        let late = cpu_backend(consumer.prepare(next));
        let identity = late.submission().identity();
        consumer.presenter.retire(19).unwrap().unwrap();
        assert!(!consumer.presenter.is_quiescent());
        assert!(consumer.presenter.journal().resolve(route).is_err());
        assert!(consumer.presenter.drain().is_err());
        match outcome {
            0 => consumer.presenter.return_outcome(late.presented()).unwrap(),
            1 => consumer
                .presenter
                .return_outcome(late.deferred("already retired".into()))
                .unwrap(),
            _ => drop(late),
        }
        assert!(!consumer.presenter.is_quiescent());
        assert!(matches!(consumer.presenter.drain().unwrap().as_slice(),
            [UiSurfacePresentationEvent::Discarded { attempt: Some(attempt) }]
                if attempt.backend_submission() == Some(identity)));
        assert!(consumer.presenter.is_quiescent());
        assert!(
            consumer
                .presenter
                .journal()
                .latest(consumer.surface)
                .is_err()
        );
        assert!(
            UiInteractionSession::default()
                .dispatch(
                    consumer.presenter.journal(),
                    route,
                    UiInputEvent::ActivateFocused {
                        seat: UiSeatId::PRIMARY,
                        source: UiActivationSource::Keyboard,
                    },
                )
                .is_err()
        );
        assert!(consumer.presenter.retry().is_err());
        assert!(consumer.presenter.drain().unwrap().is_empty());
        assert!(consumer.presenter.retire(20).unwrap().is_none());
    }
}

fn pixels() -> ImageAsset {
    ImageAsset::new(
        ImageExtent::new(2, 2),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        SWATCH_RGBA.to_vec(),
    )
    .unwrap()
}

#[test]
fn same_version_slot_and_store_replacement_preserve_escaped_pixels_and_their_last_owner() {
    let owner = Arc::new(());
    let retained = Arc::downgrade(&owner);
    let mut image = pixels();
    assert!(image.try_attach_retention_owner(owner));
    let handle = allocate_anonymous_handle::<ImageAsset>().unwrap();
    let mut store = Assets::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let version = store
        .commit_loaded(handle, image, &mut states, &mut events, None, None)
        .unwrap();
    let id = UiImageId::new("product:manual/primary").unwrap();
    let captured = CapturedUiImage::capture(id.clone(), handle, &store, &states).unwrap();
    let snapshot = captured.snapshot().clone();
    assert_eq!(snapshot.identity().asset_version(), version);
    assert_eq!(
        snapshot.identity().slot_revision(),
        &store.slot_revision(handle)
    );
    assert_eq!(
        snapshot.identity().content_hash().as_bytes(),
        snapshot.resource().content_hash().as_bytes(),
    );
    let mut consumer = Consumer::new(214);
    consumer.image = captured.clone();
    let frame = consumer.frame(1);
    let submission = original(consumer.prepare(frame));
    let escaped = Arc::clone(submission.submission().packet());
    drop(submission);
    consumer.presenter.drain().unwrap();

    drop(store.insert(handle, pixels()).unwrap());
    let replacement = CapturedUiImage::capture(id.clone(), handle, &store, &states).unwrap();
    assert_eq!(
        replacement.snapshot().identity().key(),
        snapshot.identity().key()
    );
    assert_eq!(replacement.snapshot().identity().asset_version(), version);
    assert_eq!(
        replacement.snapshot().identity().content_hash(),
        snapshot.identity().content_hash(),
    );
    assert_ne!(
        replacement.snapshot().identity().slot_revision(),
        snapshot.identity().slot_revision(),
    );
    assert_ne!(replacement.snapshot(), &snapshot);
    assert_ne!(replacement.snapshot().identity(), snapshot.identity());
    let removed = store.remove(handle).unwrap();
    assert!(CapturedUiImage::capture(id.clone(), handle, &store, &states).is_err());
    assert_ne!(
        &store.slot_revision(handle),
        replacement.snapshot().identity().slot_revision()
    );
    store.insert(handle, removed);
    let restored = CapturedUiImage::capture(id.clone(), handle, &store, &states).unwrap();
    assert_eq!(restored.snapshot().identity().asset_version(), version);
    assert_ne!(
        restored.snapshot().identity().slot_revision(),
        replacement.snapshot().identity().slot_revision(),
    );
    let mut changed_pixels = SWATCH_RGBA;
    changed_pixels[0] = 64;
    store.insert(
        handle,
        ImageAsset::new(
            ImageExtent::new(2, 2),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            changed_pixels.to_vec(),
        )
        .unwrap(),
    );
    let changed = CapturedUiImage::capture(id.clone(), handle, &store, &states).unwrap();
    assert_eq!(changed.snapshot().identity().asset_version(), version);
    assert_ne!(
        changed.snapshot().identity().content_hash(),
        snapshot.identity().content_hash(),
    );
    assert_ne!(
        changed.snapshot().identity().slot_revision(),
        restored.snapshot().identity().slot_revision(),
    );
    assert_eq!(
        changed.snapshot().resource().pixels(),
        changed_pixels.as_slice()
    );

    let mut successor_store = Assets::default();
    let mut successor_states = AssetStates::default();
    let successor_version = successor_store
        .commit_loaded(
            handle,
            pixels(),
            &mut successor_states,
            &mut events,
            None,
            None,
        )
        .unwrap();
    assert_eq!(successor_version, version);
    let successor =
        CapturedUiImage::capture(id, handle, &successor_store, &successor_states).unwrap();
    assert_eq!(
        successor.snapshot().identity().key(),
        snapshot.identity().key()
    );
    assert_eq!(successor.snapshot().identity().asset_version(), version);
    assert_eq!(
        successor.snapshot().identity().content_hash(),
        snapshot.identity().content_hash(),
    );
    assert_ne!(
        successor.snapshot().identity().slot_revision(),
        snapshot.identity().slot_revision(),
    );
    assert_ne!(successor.snapshot(), &snapshot);
    assert_ne!(successor.snapshot().identity(), snapshot.identity());
    drop(store);
    drop(states);
    assert_capture(&escaped, &captured);
    drop(captured);
    drop(consumer);
    let final_packet = Arc::clone(&escaped);
    drop(escaped);
    assert!(retained.upgrade().is_some());
    drop(snapshot);
    assert!(retained.upgrade().is_some());
    assert_eq!(
        final_packet.images()[0].snapshot().resource().pixels(),
        SWATCH_RGBA.as_slice()
    );
    drop(final_packet);
    assert!(retained.upgrade().is_none());
}

#[test]
fn named_and_external_image_identities_keep_exact_bindings_and_sticky_ambiguity() {
    struct FirstProvider;
    struct SecondProvider;

    let ids = [
        UiImageId::new("product:manual/primary").unwrap(),
        UiImageId::external::<FirstProvider>(17),
        UiImageId::external::<SecondProvider>(17),
    ];
    assert_eq!(ids[0].as_name(), Some("product:manual/primary"));
    assert_ne!(ids[1], ids[2]);
    assert!(ids[1].as_name().is_none());
    let captures = ids.map(|id| {
        CapturedUiImage::rgba8(
            id,
            ImageExtent::new(2, 2),
            ImageColorSpace::Srgb,
            SWATCH_RGBA.to_vec(),
            Arc::new(()),
        )
        .unwrap()
    });
    let mut consumer = Consumer::new(215);
    let mut builder = UiAuthoringSnapshotBuilder::new(1);
    let mut images = UiPreparedImages::new();
    for (index, capture) in captures.iter().enumerate() {
        builder = builder.with_root(capture.authored_root(
            UiRootKey::runtime("independent.images", index.to_string()).unwrap(),
            UiLayer::Base,
            UiStyle::absolute(index as f32 * 40.0, 0.0, 32.0, 32.0),
        ));
        capture.insert_into(&mut images);
        capture.insert_into(&mut images);
    }
    let environment = UiEnvironment::for_surface(
        consumer.surface,
        UiSurfaceContentTransform::direct(UiSize::new(160.0, 96.0).unwrap(), 1.25).unwrap(),
        UiSurfaceGenerations::new(7, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap();
    let frame = consumer
        .compiler
        .compile(&builder.build().unwrap(), environment, UiTheme::default())
        .unwrap();
    assert!(frame.diagnostics().is_empty());
    let submission = original(
        consumer
            .presenter
            .prepare(Arc::clone(&frame), &images)
            .unwrap(),
    );
    let packet = Arc::clone(submission.submission().packet());
    assert_eq!(packet.images().len(), 3);
    for capture in &captures {
        let retained = packet
            .images()
            .iter()
            .find(|image| image.id() == capture.id())
            .unwrap();
        assert_eq!(retained.snapshot(), capture.snapshot());
    }
    drop(submission);
    consumer.presenter.drain().unwrap();
    assert!(matches!(
        consumer
            .presenter
            .prepare(Arc::clone(&frame), &UiPreparedImages::new()),
        Err(UiSurfacePresentationError::Content(
            UiPresentationContentError::MissingImage { .. }
        ))
    ));
    let conflict = CapturedUiImage::rgba8(
        captures[0].id().clone(),
        ImageExtent::new(2, 2),
        ImageColorSpace::Srgb,
        SWATCH_RGBA.to_vec(),
        Arc::new(()),
    )
    .unwrap();
    conflict.insert_into(&mut images);
    captures[0].insert_into(&mut images);
    assert!(matches!(
        consumer.presenter.prepare(frame, &images),
        Err(UiSurfacePresentationError::Content(
            UiPresentationContentError::AmbiguousImage { first, second, .. }
        )) if first == captures[0].snapshot().identity().key()
            && second == conflict.snapshot().identity().key()
    ));
    assert!(consumer.presenter.is_quiescent());
    assert!(
        consumer
            .presenter
            .journal()
            .latest(consumer.surface)
            .is_err()
    );
    assert!(consumer.presenter.drain().unwrap().is_empty());
}
