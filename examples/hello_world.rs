use nara::hierarchy::{
    HierarchyCommandsExt, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner,
};
use nara::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins(Runtime2dPlugins)?
        .add_systems(StartupStage::Scene, setup_scene)?
        .add_systems(CoreStage::Update, move_sprites)?;

    let mut runtime = RuntimeInstance::start(app)?;
    runtime.drive(std::time::Duration::ZERO)?;
    runtime.finish()?;
    Ok(())
}

fn setup_scene(mut commands: Commands) {
    let player = commands
        .spawn((
            Name::new("player"),
            Transform2d::from_translation(Vec2::new(0.0, 0.0)),
            Sprite::from_color(Vec2::new(32.0, 32.0), Color::rgb(0.2, 0.7, 1.0)),
            Visibility::Visible,
        ))
        .id();
    commands.apply_hierarchy_operation(HierarchyOperation::Place {
        entity: player,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    });
}

fn move_sprites(mut transforms: Query<&mut Transform2d>) {
    for mut transform in &mut transforms {
        transform.translation.x += 1.0;
    }
}
