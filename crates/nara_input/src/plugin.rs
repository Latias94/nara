use nara_app::{App, CoreStage, FrameCleanup, FrameIntent, FrameIntentSet, Plugin, PluginError};
use nara_ecs::{
    ResMut,
    bevy_ecs::message::Messages,
    schedule::{IntoScheduleConfigs, SystemSet},
};

use crate::{
    ActionMaps, ActionState, ButtonInput, GamepadInput, ImeEvent, InputEdge, InputFrame, KeyCode,
    MouseButton, MouseWheel, PointerState, TextInput, TouchInput,
    actions::resolve_actions,
    physical::{PhysicalInput, project_input},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum InputSet {
    /// Reads queued ingress during `First`; producers may run before this set.
    ProjectInput,
    /// Routes `InputFrame` before gameplay resolution. Exact steps route in `FrameIntent`.
    RouteUi,
    /// Resolves gameplay actions in `PreUpdate`, before `FrameIntent` and `FixedUpdate`.
    ResolveActions,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct InputPlugin;

pub const INPUT_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.input");
pub const INPUT_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(INPUT_PLUGIN_ID, nara_app::PluginCategory::Input);

impl Plugin for InputPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &INPUT_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<ActionMaps>()?;
        app.init_resource::<InputFrame>()?;
        app.init_resource::<PhysicalInput>()?;
        app.init_resource::<ActionState>()?;
        app.init_resource::<ButtonInput<KeyCode>>()?;
        app.init_resource::<ButtonInput<MouseButton>>()?;
        app.init_resource::<PointerState>()?;
        app.init_resource::<Messages<InputEdge>>()?;
        app.init_resource::<Messages<TextInput>>()?;
        app.init_resource::<Messages<ImeEvent>>()?;
        app.init_resource::<Messages<MouseWheel>>()?;
        app.init_resource::<Messages<TouchInput>>()?;
        app.init_resource::<Messages<GamepadInput>>()?;
        app.configure_sets(
            CoreStage::First,
            (InputSet::ProjectInput, InputSet::RouteUi).chain(),
        )?
        .configure_sets(
            FrameIntent,
            (
                InputSet::ProjectInput,
                InputSet::RouteUi,
                InputSet::ResolveActions,
            )
                .chain()
                .in_set(FrameIntentSet::Prepare),
        )?
        .add_systems(
            CoreStage::First,
            project_input.in_set(InputSet::ProjectInput),
        )?
        .add_systems(
            CoreStage::PreUpdate,
            resolve_actions.in_set(InputSet::ResolveActions),
        )?
        .add_systems(FrameIntent, project_input.in_set(InputSet::ProjectInput))?
        .add_systems(
            FrameIntent,
            resolve_actions.in_set(InputSet::ResolveActions),
        )?
        .add_systems(
            FrameCleanup,
            (resolve_actions, clear_frame, advance_messages).chain(),
        )?;
        Ok(())
    }
}

fn clear_frame(
    mut frame: ResMut<InputFrame>,
    mut actions: ResMut<ActionState>,
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    frame.clear();
    actions.clear_edges();
    keyboard.clear_transitions();
    mouse.clear_transitions();
}

fn advance_messages(
    mut text: ResMut<Messages<TextInput>>,
    mut ime: ResMut<Messages<ImeEvent>>,
    mut wheel: ResMut<Messages<MouseWheel>>,
    mut touch: ResMut<Messages<TouchInput>>,
    mut gamepad: ResMut<Messages<GamepadInput>>,
) {
    text.update();
    ime.update();
    wheel.update();
    touch.update();
    gamepad.update();
}
