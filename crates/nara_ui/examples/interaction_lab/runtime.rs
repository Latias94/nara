use std::{error::Error, sync::Arc, time::Duration};

use nara_app::{App, AppRunError, RuntimeInstance};
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
    pub(super) fn start(mut app: App) -> Result<Self, Box<dyn Error>> {
        app.insert_resource(LabDriver {
            presenter: HeadlessInteractionPresenter::new(),
        })?;
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
        let outcome = self.runtime.with_driver_scope(|scope| {
            scope.with_world_mut(|world| LabDriver::apply(world, operation))
        })?;
        if let Some(fault) = outcome.post_operation_fault {
            return Err(Box::new(AppRunError::managed_runtime_fault(&fault)));
        }
        outcome.output
    }
}

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
    presenter: HeadlessInteractionPresenter,
}

impl LabDriver {
    fn apply(
        world: &mut World,
        input: LabOperation,
    ) -> Result<Option<UiPresentationReceipt>, Box<dyn Error>> {
        world.resource_scope(|world, mut driver: Mut<Self>| {
            match input {
                LabOperation::Present { frame, epoch } => {
                    return driver.presenter.present(world, frame, epoch).map(Some);
                }
                LabOperation::SetPrimaryButton(button) => {
                    let entity = world.resource::<LabEntities>().primary_button;
                    world.entity_mut(entity).insert(button);
                }
                LabOperation::SetOverlayLayer(layer) => {
                    let entity = world.resource::<LabEntities>().scene_overlay;
                    world.entity_mut(entity).insert(UiRoot::new(layer));
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
        })
    }
}
