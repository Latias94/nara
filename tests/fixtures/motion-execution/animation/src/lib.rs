#![forbid(unsafe_code)]

use motion_contract::{Actors, MotionInput, Phase, RootMotionIntent, Trace};
use nara_app::{
    App, CoreStage, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError,
    PluginId,
};
use nara_ecs::{Commands, Res, SystemSet, schedule::IntoScheduleConfigs};

pub const ANIMATION_PLUGIN_ID: PluginId = PluginId::new("fixture.motion.animation");

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration =
            PluginDeclaration::new(ANIMATION_PLUGIN_ID, PluginCategory::Runtime);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            emit_intent
                .in_set(MotionIntent)
                .in_set(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct MotionIntent;

pub fn emit_intent(
    actors: Res<Actors>,
    input: Res<MotionInput>,
    trace: Res<Trace>,
    mut commands: Commands,
) {
    commands.entity(actors.kinematic).insert(RootMotionIntent {
        tick: input.tick,
        delta: input.delta,
    });
    trace.record(input.tick, Phase::Intent);
}
