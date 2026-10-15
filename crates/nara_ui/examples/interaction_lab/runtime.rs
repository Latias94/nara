use std::{error::Error, sync::Arc, time::Duration};

use nara_app::{__RuntimeDriverPort, App, RuntimeInstance, RuntimeWorldAccessError};
use nara_ecs::{Mut, Resource, World};
use nara_ui::{
    advanced::{
        UiAccessibilityInput, UiFrame, UiPresentationReceipt, UiRouteDisposition, UiSurfaceId,
        dispatch_ui_accessibility_input, retire_ui_surface,
    },
    prelude::{UiActionCursor, UiActionJournal, UiButton, UiLayer, UiRoot},
};

use super::{fixture::LabEntities, presenter::HeadlessInteractionPresenter};

pub(super) struct LabRuntime {
    runtime: RuntimeInstance,
}

impl LabRuntime {
    pub(super) fn start(mut app: App, entities: LabEntities) -> Result<Self, Box<dyn Error>> {
        app.insert_resource(LabDriver {
            entities,
            presenter: HeadlessInteractionPresenter::new(),
        })?;
        app.__register_driver_port::<LabDriver>(LabDriver::apply_driver_input_in_world)?;
        Ok(Self {
            runtime: RuntimeInstance::start(app)?,
        })
    }

    pub(super) fn world(&self) -> &World {
        self.runtime.world()
    }

    pub(super) fn runtime_mut(&mut self) -> &mut RuntimeInstance {
        &mut self.runtime
    }

    pub(super) fn tick(&mut self) -> Result<(), Box<dyn Error>> {
        self.runtime.drive(Duration::ZERO)?;
        Ok(())
    }

    pub(super) fn present(
        &mut self,
        frame: Arc<UiFrame>,
        epoch: u64,
    ) -> Result<UiPresentationReceipt, Box<dyn Error>> {
        self.apply(LabOperation::Present { frame, epoch })?
            .ok_or_else(|| "the presentation operation did not produce a receipt".into())
    }

    pub(super) fn set_primary_button(&mut self, button: UiButton) -> Result<(), Box<dyn Error>> {
        self.apply(LabOperation::SetPrimaryButton(button))?;
        Ok(())
    }

    pub(super) fn set_overlay_layer(&mut self, layer: UiLayer) -> Result<(), Box<dyn Error>> {
        self.apply(LabOperation::SetOverlayLayer(layer))?;
        Ok(())
    }

    pub(super) fn activate_accessibility(
        &mut self,
        input: UiAccessibilityInput,
    ) -> Result<(), Box<dyn Error>> {
        self.apply(LabOperation::Accessibility(input))?;
        Ok(())
    }

    pub(super) fn acknowledge_latest(
        &mut self,
        cursor: &Arc<UiActionCursor>,
    ) -> Result<(), Box<dyn Error>> {
        self.apply(LabOperation::AcknowledgeLatest(Arc::clone(cursor)))?;
        Ok(())
    }

    pub(super) fn retire_surface(&mut self, surface: UiSurfaceId) -> Result<(), Box<dyn Error>> {
        self.apply(LabOperation::RetireSurface(surface))?;
        Ok(())
    }

    pub(super) fn close(self) -> Result<(), Box<dyn Error>> {
        self.runtime.finish()?;
        Ok(())
    }

    fn apply(
        &mut self,
        operation: LabOperation,
    ) -> Result<Option<UiPresentationReceipt>, Box<dyn Error>> {
        self.runtime
            .with_driver_scope(|scope| scope.__apply_port::<LabDriver>(operation))??
    }
}

// This example's concrete operations cannot replace runtime authority or expose its World.
enum LabOperation {
    Present { frame: Arc<UiFrame>, epoch: u64 },
    SetPrimaryButton(UiButton),
    SetOverlayLayer(UiLayer),
    Accessibility(UiAccessibilityInput),
    AcknowledgeLatest(Arc<UiActionCursor>),
    RetireSurface(UiSurfaceId),
}

#[derive(Resource)]
struct LabDriver {
    entities: LabEntities,
    presenter: HeadlessInteractionPresenter,
}

impl __RuntimeDriverPort for LabDriver {
    type Input = LabOperation;
    type Output = Result<Option<UiPresentationReceipt>, Box<dyn Error>>;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        Err("lab operations require the coordinated driver port".into())
    }
}

impl LabDriver {
    fn apply_driver_input_in_world(
        world: &mut World,
        input: LabOperation,
    ) -> Result<
        Result<Option<UiPresentationReceipt>, Box<dyn Error>>,
        (RuntimeWorldAccessError, LabOperation),
    > {
        Ok(world.resource_scope(|world, mut driver: Mut<Self>| {
            match input {
                LabOperation::Present { frame, epoch } => {
                    return driver.presenter.present(world, frame, epoch).map(Some);
                }
                LabOperation::SetPrimaryButton(button) => {
                    world
                        .entity_mut(driver.entities.primary_button)
                        .insert(button);
                }
                LabOperation::SetOverlayLayer(layer) => {
                    world
                        .entity_mut(driver.entities.scene_overlay)
                        .insert(UiRoot::new(layer));
                }
                LabOperation::Accessibility(input) => {
                    assert!(matches!(
                        dispatch_ui_accessibility_input(world, input),
                        UiRouteDisposition::Consumed(_)
                    ));
                }
                LabOperation::AcknowledgeLatest(cursor) => {
                    let latest = world
                        .resource::<UiActionJournal>()
                        .pending(&cursor)?
                        .last()
                        .map(|event| event.sequence());
                    if let Some(sequence) = latest {
                        world
                            .resource_mut::<UiActionJournal>()
                            .acknowledge(&cursor, sequence)?;
                    }
                }
                LabOperation::RetireSurface(surface) => {
                    assert!(retire_ui_surface(world, surface)?.is_some());
                }
            }
            Ok(None)
        }))
    }
}
