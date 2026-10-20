use std::sync::Mutex;

use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, WindowEvent},
};

use super::{GameInputWorkflow, control_rect};

const CANVAS_BUTTON_ACTION: &str = "test.canvas-runtime-button";
static CANVAS_BUTTON_PROBE: Mutex<Option<Result<CanvasButtonProbe, String>>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct CanvasButtonProbe {
    physical_center: [f32; 2],
    surface_extent: [f32; 2],
    actions: usize,
}

#[derive(nara::ecs::Resource)]
struct CanvasButtonConsumer {
    cursor: nara::ui::UiActionCursor,
    actions: usize,
}

pub(super) fn install_canvas_button(world: &mut nara::ecs::World) {
    if !world.contains_resource::<nara::render::WorldCanvas>() {
        return;
    }
    use nara::ui::{
        UiActionConsumerId, UiActionId, UiActionJournal, UiButton, UiColor, UiLayer, UiNode,
        UiRoot, UiRuntimeKey, UiStyle, UiVisual,
    };
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Overlay),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new("test.canvas-button", "root").unwrap(),
        ))
        .id();
    let button = world
        .spawn((
            UiNode::new(UiStyle::absolute(20.0, 20.0, 100.0, 40.0)),
            UiButton::new(
                "Canvas click",
                UiActionId::new(CANVAS_BUTTON_ACTION).unwrap(),
                16.0,
            )
            .unwrap(),
            UiVisual::solid(UiColor::rgba(0.1, 0.4, 0.8, 1.0)),
            UiRuntimeKey::new("test.canvas-button", "button").unwrap(),
        ))
        .id();
    use nara::hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
    };
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: button,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: root,
                }),
            },
        ])
        .unwrap();
    let cursor = world
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("test.canvas-button").unwrap())
        .unwrap();
    world.insert_resource(CanvasButtonConsumer { cursor, actions: 0 });
}

pub(super) fn observe_canvas_button(world: &mut nara::ecs::World) {
    if !world.contains_resource::<CanvasButtonConsumer>() {
        return;
    }
    let observed = read_canvas_button_probe(world);
    *CANVAS_BUTTON_PROBE.lock().unwrap() = observed.transpose();
}

fn read_canvas_button_probe(
    world: &mut nara::ecs::World,
) -> Result<Option<CanvasButtonProbe>, String> {
    world.resource_scope(
        |world, mut consumer: nara::ecs::Mut<'_, CanvasButtonConsumer>| {
            let mut journal = world.resource_mut::<nara::ui::UiActionJournal>();
            let pending = journal
                .pending(&consumer.cursor)
                .map_err(|error| error.to_string())?;
            let count = pending
                .iter()
                .filter(|event| event.action().as_str() == CANVAS_BUTTON_ACTION)
                .count();
            let latest = pending.last().map(|event| event.sequence());
            if let Some(latest) = latest {
                journal
                    .acknowledge(&consumer.cursor, latest)
                    .map_err(|error| error.to_string())?;
            }
            consumer.actions += count;
            Ok::<(), String>(())
        },
    )?;
    let Some(frame) = nara::ui::latest_ui_frame(world, nara::ui::UiSurfaceId::new(1).unwrap())
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let transform = frame.environment().content_transform();
    let button = frame
        .semantics()
        .iter()
        .find(|node| {
            node.actions()
                .iter()
                .any(|action| action.as_str() == CANVAS_BUTTON_ACTION)
        })
        .ok_or_else(|| {
            format!(
                "the compiled Runtime UI button action is missing: roots={:?}, diagnostics={:?}",
                frame.root_outcomes(),
                frame.diagnostics(),
            )
        })?;
    let rect = button.rect();
    let origin = transform
        .authored_to_physical(rect.origin())
        .ok_or("the compiled Runtime UI button is outside its authored surface")?;
    let scale = transform.content_scale_factor();
    let extent = transform.physical_surface_size();
    Ok(Some(CanvasButtonProbe {
        physical_center: [
            origin.x() + rect.size().width() * scale * 0.5,
            origin.y() + rect.size().height() * scale * 0.5,
        ],
        surface_extent: [extent.width(), extent.height()],
        actions: world.resource::<CanvasButtonConsumer>().actions,
    }))
}

pub(super) fn reset_probe() {
    *CANVAS_BUTTON_PROBE.lock().unwrap() = None;
}

impl GameInputWorkflow<'_> {
    pub(super) fn click_runtime_canvas_button(&mut self) -> Result<(), String> {
        let before = CANVAS_BUTTON_PROBE
            .lock()
            .unwrap()
            .clone()
            .ok_or("the Runtime UI button has no compiled geometry")??;
        let observation = self.observe()?;
        let extent = observation.submitted.image.target().extent();
        if before.surface_extent != [extent.width as f32, extent.height as f32] {
            return Err("Runtime UI button geometry did not track the real Game target".into());
        }
        let image = control_rect("embedded:Game image").ok_or("the Game image is missing")?;
        let mapping = nara::render::DisplayMapping2d::fit(
            extent,
            image,
            self.native.context.io().display_framebuffer_scale()[0],
            nara::render::DisplayScale::Fit,
        )
        .ok_or("the actual Game image mapping is invalid")?;
        self.pointer = mapping
            .uv_to_panel([
                before.physical_center[0] / before.surface_extent[0],
                before.physical_center[1] / before.surface_extent[1],
            ])
            .ok_or("the Runtime UI button is outside its actual Game image")?;
        self.paint(|_| {})?;
        // Feed the native route after the exact new image is presented. Running paint revokes
        // its previous input stamp before driving, so its preparatory cursor event is withheld.
        let origin = self.native.context.main_viewport().pos();
        let scale = f64::from(self.native.context.io().display_framebuffer_scale()[0]);
        self.native.input.handle_event(
            self.native.window.id(),
            &WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: PhysicalPosition::new(
                    f64::from(self.pointer[0] - origin[0]) * scale,
                    f64::from(self.pointer[1] - origin[1]) * scale,
                ),
            },
            &self.native.context,
        );
        self.native
            .input
            .flush(self.application)
            .map_err(|error| self.frame_error("Runtime UI button pointer", error))?;
        for state in [ElementState::Pressed, ElementState::Released] {
            self.native.input.handle_event(
                self.native.window.id(),
                &WindowEvent::MouseInput {
                    device_id: DeviceId::dummy(),
                    state,
                    button: winit::event::MouseButton::Left,
                },
                &self.native.context,
            );
            self.native
                .input
                .flush(self.application)
                .map_err(|error| self.frame_error("Runtime UI button input", error))?;
            self.paint(|_| {})?;
        }
        self.settle()?;
        let after = CANVAS_BUTTON_PROBE
            .lock()
            .unwrap()
            .clone()
            .ok_or("the Runtime UI button lost its compiled geometry")??;
        if after.actions != before.actions + 1 {
            return Err(format!(
                "the actual Runtime UI button must journal one activation: {} -> {}",
                before.actions, after.actions
            ));
        }
        Ok(())
    }
}
