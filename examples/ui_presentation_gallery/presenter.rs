use std::{error::Error, sync::Arc};

use nara::advanced_prelude::*;
use nara::prelude::World;
use nara::ui_render::advanced::{UiPaintPacket, UiPresentationOutcome};

pub(super) struct HeadlessTestPresenter {
    target: RenderTarget,
    submitted: Option<Arc<UiPaintPacket>>,
}

impl HeadlessTestPresenter {
    pub(super) fn new(target: RenderTarget) -> Self {
        Self {
            target,
            submitted: None,
        }
    }

    pub(super) fn accept_and_submit(
        &mut self,
        packet: Arc<UiPaintPacket>,
        presentation_epoch: u64,
    ) -> UiPresentationOutcome {
        if packet.target() != self.target {
            return UiPresentationOutcome::ContentRejected {
                attempt: packet.identity(),
                message: "the headless test presenter does not own this target".to_owned(),
            };
        }

        assert!(!packet.primitives().is_empty());
        self.submitted = Some(Arc::clone(&packet));
        UiPresentationOutcome::Presented {
            packet,
            presentation_epoch,
        }
    }

    pub(super) fn submitted(&self) -> Option<&Arc<UiPaintPacket>> {
        self.submitted.as_ref()
    }
}

pub(super) fn commit_headless_test_outcome(
    world: &mut World,
    outcome: UiPresentationOutcome,
) -> Result<Option<UiPresentationReceipt>, Box<dyn Error>> {
    match outcome {
        UiPresentationOutcome::Presented {
            packet,
            presentation_epoch,
        } => {
            let identity = packet.identity();
            let receipt = commit_ui_presentation(world, packet.frame(), presentation_epoch)?;
            assert_eq!(receipt.surface_id(), identity.surface_id());
            assert_eq!(receipt.frame_revision(), identity.frame_revision());
            assert_eq!(receipt.route_revision(), identity.route_revision());
            assert_eq!(
                receipt.presentation_revision(),
                identity.presentation_revision()
            );
            assert_eq!(receipt.presentation_epoch(), presentation_epoch);
            Ok(Some(receipt))
        }
        UiPresentationOutcome::ContentRejected { .. } => Ok(None),
        UiPresentationOutcome::Deferred { .. } => {
            Err("the headless test presenter unexpectedly deferred submission".into())
        }
        UiPresentationOutcome::BackendFault { .. } => {
            Err("the headless test presenter unexpectedly faulted".into())
        }
    }
}
