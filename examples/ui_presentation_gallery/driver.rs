use std::{error::Error, sync::Arc};

use nara::advanced_prelude::*;
use nara::app::{__RuntimeDriverPort, RuntimeInstance, RuntimeWorldAccessError};
use nara::ui_render::advanced::UiPaintPacket;

use super::presenter::{HeadlessTestOutcome, HeadlessTestPresenter, commit_headless_test_outcome};

#[derive(Resource)]
pub(super) struct GalleryDriverPort;

pub(super) enum GalleryDriverInput {
    Submit {
        presenter: HeadlessTestPresenter,
        packet: Arc<UiPaintPacket>,
        epoch: u64,
    },
    Commit(HeadlessTestOutcome),
    Accessibility(UiAccessibilityInput),
    UpdateSurface {
        surface: UiSurfaceId,
        endpoint: UiSurfaceEndpoint,
        transform: UiSurfaceContentTransform,
    },
    RetireSurface(UiSurfaceId),
    InstallSurface(UiSurfaceSlot),
}

pub(super) enum GalleryDriverOutput {
    Submitted(HeadlessTestPresenter, HeadlessTestOutcome),
    Committed(Option<UiPresentationReceipt>),
    Accessibility(UiRouteDisposition),
    Updated(UiSurfaceSlot),
    Retired(Option<UiSurfaceRetirement>),
    Installed(Result<(), UiAdapterError>),
}

impl __RuntimeDriverPort for GalleryDriverPort {
    type Input = GalleryDriverInput;
    type Output = Result<GalleryDriverOutput, Box<dyn Error>>;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        Err("gallery operations require the managed runtime world".into())
    }
}

impl GalleryDriverPort {
    pub(super) fn apply_driver_input_in_world(
        world: &mut World,
        input: GalleryDriverInput,
    ) -> Result<
        Result<GalleryDriverOutput, Box<dyn Error>>,
        (RuntimeWorldAccessError, GalleryDriverInput),
    > {
        Ok(apply_gallery_operation(world, input))
    }
}

fn apply_gallery_operation(
    world: &mut World,
    input: GalleryDriverInput,
) -> Result<GalleryDriverOutput, Box<dyn Error>> {
    Ok(match input {
        GalleryDriverInput::Submit {
            mut presenter,
            packet,
            epoch,
        } => {
            let outcome = presenter.accept_and_submit(world, packet, epoch)?;
            GalleryDriverOutput::Submitted(presenter, outcome)
        }
        GalleryDriverInput::Commit(outcome) => {
            GalleryDriverOutput::Committed(commit_headless_test_outcome(world, outcome)?)
        }
        GalleryDriverInput::Accessibility(input) => {
            GalleryDriverOutput::Accessibility(dispatch_ui_accessibility_input(world, input))
        }
        GalleryDriverInput::UpdateSurface {
            surface,
            endpoint,
            transform,
        } => GalleryDriverOutput::Updated(update_ui_surface_slot(
            world, surface, endpoint, transform,
        )?),
        GalleryDriverInput::RetireSurface(surface) => {
            GalleryDriverOutput::Retired(retire_ui_surface(world, surface)?)
        }
        GalleryDriverInput::InstallSurface(slot) => {
            GalleryDriverOutput::Installed(install_ui_surface(world, slot))
        }
    })
}

pub(super) fn apply(
    runtime: &mut RuntimeInstance,
    input: GalleryDriverInput,
) -> Result<GalleryDriverOutput, Box<dyn Error>> {
    runtime
        .with_driver_scope(|scope| scope.__apply_port::<GalleryDriverPort>(input))?
        .output?
}
