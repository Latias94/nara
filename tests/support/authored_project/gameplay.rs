use nara::{
    ecs::{Local, With, schedule::IntoScheduleConfigs},
    hierarchy::{
        HIERARCHY_PLUGIN_ID, HierarchyCommandsExt, HierarchyOperation, HierarchyPlacement,
        HierarchyPlacementOwner,
    },
    input::{ActionContext, ActionId, INPUT_PLUGIN_ID, InputSeatId, SemanticActions},
    prelude::{
        App, Color, Commands, Component, CoreStage, FixedTime, FixedUpdateSet, Name, Plugin,
        PluginCategory, PluginDeclaration, PluginError, PluginId, Query, Res, Sprite, StartupStage,
        Transform2d, Vec2, Visibility,
    },
    scene::TRANSFORM_COMPONENTS_PLUGIN_ID,
    sprite::SPRITE_PLUGIN_ID,
};

const MOVE_SPEED: f32 = 240.0;
const DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("starter.desktop-2d.gameplay"),
    PluginCategory::Runtime,
)
.requires_plugins(&[
    INPUT_PLUGIN_ID,
    HIERARCHY_PLUGIN_ID,
    TRANSFORM_COMPONENTS_PLUGIN_ID,
    SPRITE_PLUGIN_ID,
]);

#[derive(Default)]
pub(crate) struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Scene, spawn_player)?
            .add_systems(
                CoreStage::FixedUpdate,
                move_player.in_set(FixedUpdateSet::Simulate),
            )?;
        Ok(())
    }
}

#[derive(Component)]
struct Player;

fn spawn_player(mut commands: Commands) {
    let player = commands
        .spawn((
            Player,
            Name::new("player"),
            Transform2d::default(),
            Sprite::from_color(Vec2::splat(48.0), Color::rgb(0.2, 0.8, 0.9)),
            Visibility::Visible,
        ))
        .id();
    commands.apply_hierarchy_operation(HierarchyOperation::Place {
        entity: player,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    });
}

struct PlayerActions {
    context: ActionContext,
    left: ActionId,
    right: ActionId,
    up: ActionId,
    down: ActionId,
    confirm: ActionId,
}

impl Default for PlayerActions {
    fn default() -> Self {
        let action = |id: &str| ActionId::new(id).expect("valid starter action ID");
        Self {
            context: ActionContext::gameplay(),
            left: action("move.left"),
            right: action("move.right"),
            up: action("move.up"),
            down: action("move.down"),
            confirm: action("confirm"),
        }
    }
}

fn move_player(
    time: Res<FixedTime>,
    held: Res<SemanticActions>,
    actions: Local<PlayerActions>,
    mut players: Query<&mut Transform2d, With<Player>>,
) {
    let pressed = |action: &ActionId| held.pressed(InputSeatId::PRIMARY, action, &actions.context);
    let horizontal = i8::from(pressed(&actions.right)) - i8::from(pressed(&actions.left));
    let vertical = i8::from(pressed(&actions.up)) - i8::from(pressed(&actions.down));
    let direction = Vec2::new(f32::from(horizontal), f32::from(vertical));
    let confirm_held = pressed(&actions.confirm);
    let delta_seconds = time.delta().as_secs_f32();

    for mut transform in &mut players {
        transform.translation = step_player(
            transform.translation,
            direction,
            confirm_held,
            delta_seconds,
        );
    }
}

fn step_player(position: Vec2, direction: Vec2, confirm_held: bool, delta_seconds: f32) -> Vec2 {
    if confirm_held {
        Vec2::ZERO
    } else {
        position + direction.normalize_or_zero() * (MOVE_SPEED * delta_seconds)
    }
}
