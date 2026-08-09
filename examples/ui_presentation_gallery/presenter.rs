use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use nara::advanced_prelude::*;
use nara::prelude::World;
use nara::ui_render::advanced::{
    OneShotPresentationCommit, UiPaintPacket, UiPresentationAttemptIdentity,
    UiPresentationBackendGeneration, UiPresentationOutcome,
};

static NEXT_BACKEND_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_PRESENTATION_ATTEMPT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) enum HeadlessTestOutcome {
    Presented {
        commit: OneShotPresentationCommit,
        authority: nara_ui::__private::UiPreparedPresentationAuthority,
    },
    ContentRejected {
        attempt: UiPresentationAttemptIdentity,
    },
}

impl HeadlessTestOutcome {
    pub(super) const fn attempt(&self) -> UiPresentationAttemptIdentity {
        match self {
            Self::Presented { commit, .. } => commit.attempt(),
            Self::ContentRejected { attempt } => *attempt,
        }
    }
}

pub(super) struct HeadlessTestPresenter {
    target: RenderTarget,
    backend_generation: UiPresentationBackendGeneration,
    submitted: Option<Arc<UiPaintPacket>>,
}

impl HeadlessTestPresenter {
    pub(super) fn new(target: RenderTarget) -> Self {
        Self {
            target,
            backend_generation: UiPresentationBackendGeneration::new(
                NEXT_BACKEND_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
                1,
            )
            .expect("the headless presenter uses non-zero backend generations"),
            submitted: None,
        }
    }

    pub(super) fn accept_and_submit(
        &mut self,
        world: &mut World,
        packet: Arc<UiPaintPacket>,
        presentation_epoch: u64,
    ) -> Result<HeadlessTestOutcome, Box<dyn Error>> {
        if packet.target() != self.target {
            return Ok(HeadlessTestOutcome::ContentRejected {
                attempt: nara_ui_render::__private::presentation_attempt(&packet),
            });
        }

        assert!(!packet.primitives().is_empty());
        let epoch = nara_ui_render::UiPresentationEpoch::new(presentation_epoch)
            .ok_or("the headless presenter requires a non-zero presentation epoch")?;
        let authority =
            nara_ui::__private::prepare_ui_presentation(world, packet.frame(), presentation_epoch)?;
        let prepared = nara_ui_render::__private::prepare_backend_submission(
            nara_ui_render::UiPresentationAttemptId::new(
                NEXT_PRESENTATION_ATTEMPT_ID.fetch_add(1, Ordering::Relaxed),
            )
            .expect("the headless presenter uses non-zero attempt identities"),
            Arc::clone(&packet),
            epoch,
            nara_ui_render::UiPresentationRuntimeGeneration::new(1)
                .expect("the headless presenter uses a non-zero runtime generation"),
            nara_ui_render::UiPresentationCapabilityGeneration::new(1)
                .expect("the headless presenter uses a non-zero capability generation"),
        );
        let mut outcomes = nara_ui_render::__private::UiPresentationOutcomes::default();
        let permit = outcomes
            .reserve_backend(std::slice::from_ref(&prepared))?
            .pop()
            .expect("one prepared attempt reserves one backend outcome");
        let envelope = nara_ui_render::__private::UiBackendSubmission::pair(prepared, permit)?;
        let mut submitted = nara_ui_render::__private::bind_backend_submission_batch(
            vec![envelope],
            self.backend_generation,
        )
        .map_err(|error| {
            format!(
                "the headless presenter could not bind backend generation {:?} to {:?}",
                error.actual(),
                error.required()
            )
        })?;
        let submitted = submitted
            .pop()
            .expect("one prepared attempt binds one backend submission");
        self.submitted = Some(Arc::clone(&packet));
        outcomes.commit_backend(submitted.presented());
        let mut backend_world = World::new();
        backend_world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut backend_world)?;
        let UiPresentationOutcome::Presented(commit) = published
            .pop()
            .expect("one backend submission publishes one exact outcome")
        else {
            unreachable!("issuing accepted backend evidence always produces Presented")
        };
        Ok(HeadlessTestOutcome::Presented { commit, authority })
    }

    pub(super) fn submitted(&self) -> Option<&Arc<UiPaintPacket>> {
        self.submitted.as_ref()
    }
}

pub(super) fn commit_headless_test_outcome(
    world: &mut World,
    outcome: HeadlessTestOutcome,
) -> Result<Option<UiPresentationReceipt>, Box<dyn Error>> {
    match outcome {
        HeadlessTestOutcome::Presented { commit, authority } => {
            let accepted = nara_ui_render::__private::discard_ui_backend_acceptance(commit);
            let identity = accepted.packet().identity();
            if !Arc::ptr_eq(&accepted.packet().frame(), authority.frame()) {
                nara_ui::__private::supersede_prepared_ui_presentation(world, authority);
                return Err("backend acceptance did not retain the exact prepared frame".into());
            }
            let receipt = nara_ui::__private::commit_prepared_ui_presentation(world, authority);
            assert_eq!(receipt.surface_id(), identity.surface_id());
            assert_eq!(receipt.frame_revision(), identity.frame_revision());
            assert_eq!(receipt.route_revision(), identity.route_revision());
            assert_eq!(
                receipt.presentation_revision(),
                identity.presentation_revision()
            );
            assert_eq!(
                receipt.presentation_epoch(),
                accepted.identity().presentation_epoch().get()
            );
            Ok(Some(receipt))
        }
        HeadlessTestOutcome::ContentRejected { .. } => Ok(None),
    }
}
