use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use nara_ecs::World;
use nara_render::RenderTarget;
use nara_ui::UiPresentationReceipt;
use nara_ui_kernel::UiFrame;
use nara_ui_render::{
    UiPreparedImages, UiPresentationAttemptId, UiPresentationBackendGeneration,
    UiPresentationCapabilityGeneration, UiPresentationEpoch, UiPresentationOutcome,
    UiPresentationPlanner, UiPresentationRuntimeGeneration,
};
use nara_window::WindowId;

static NEXT_BACKEND_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_PRESENTATION_ATTEMPT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct HeadlessInteractionPresenter {
    planner: UiPresentationPlanner,
    backend_generation: UiPresentationBackendGeneration,
}

impl HeadlessInteractionPresenter {
    pub(super) fn new() -> Self {
        Self {
            planner: UiPresentationPlanner::default(),
            backend_generation: UiPresentationBackendGeneration::new(
                NEXT_BACKEND_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
                1,
            )
            .expect("the interaction lab uses non-zero backend generations"),
        }
    }

    pub(super) fn present(
        &mut self,
        world: &mut World,
        frame: Arc<UiFrame>,
        presentation_epoch: u64,
    ) -> Result<UiPresentationReceipt, Box<dyn Error>> {
        let target = RenderTarget::Window(WindowId::new(frame.environment().surface_id().get()));
        let packet = self
            .planner
            .plan(frame, target, &UiPreparedImages::default())?;
        let epoch = UiPresentationEpoch::new(presentation_epoch)
            .ok_or("the interaction lab requires a non-zero presentation epoch")?;
        let authority =
            nara_ui::__private::prepare_ui_presentation(world, packet.frame(), presentation_epoch)?;
        let prepared = nara_ui_render::__private::prepare_backend_submission(
            UiPresentationAttemptId::new(
                NEXT_PRESENTATION_ATTEMPT_ID.fetch_add(1, Ordering::Relaxed),
            )
            .expect("the interaction lab uses non-zero attempt identities"),
            Arc::clone(&packet),
            epoch,
            UiPresentationRuntimeGeneration::new(1)
                .expect("the interaction lab uses a non-zero runtime generation"),
            UiPresentationCapabilityGeneration::new(1)
                .expect("the interaction lab uses a non-zero capability generation"),
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
                "the interaction lab could not bind backend generation {:?} to {:?}",
                error.actual(),
                error.required()
            )
        })?;
        let submitted = submitted
            .pop()
            .expect("one prepared attempt binds one backend submission");
        outcomes.commit_backend(submitted.presented());

        let mut backend_world = World::new();
        backend_world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut backend_world)?;
        let UiPresentationOutcome::Presented(commit) = published
            .pop()
            .expect("one backend submission publishes one exact outcome")
        else {
            unreachable!("accepted backend evidence always produces Presented")
        };
        let accepted = nara_ui_render::__private::discard_ui_backend_acceptance(commit);
        if !Arc::ptr_eq(accepted.packet(), &packet) {
            nara_ui::__private::supersede_prepared_ui_presentation(world, authority);
            return Err("backend acceptance did not retain the exact planned packet".into());
        }

        let identity = accepted.packet().identity();
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
        Ok(receipt)
    }
}
