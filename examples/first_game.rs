use nara::{
    asset::add_asset,
    ecs::{Local, Single, schedule::IntoScheduleConfigs},
    prelude::*,
    render::animation::{Clip, Frame, PlaybackMode},
};
use std::{error::Error, time::Duration};

#[derive(Component)]
struct Player(Clip, TopDownCharacter, Handle<AudioClip>);

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new("Nara First Game"))?;
    app.add_systems(Startup, setup)?
        .add_systems(FixedUpdate, play.before(Physics2dSet::Step))?;
    app.run()?;
    Ok(())
}

fn setup(world: &mut World) -> nara::ecs::bevy_ecs::error::Result {
    use AxisDirection::{Negative, Positive};
    let bindings = [
        ("move.x", KeyCode::KeyA, Negative),
        ("move.x", KeyCode::KeyD, Positive),
        ("move.y", KeyCode::KeyS, Negative),
        ("move.y", KeyCode::KeyW, Positive),
    ];
    let bindings = bindings
        .map(|(id, key, sign)| ActionBinding::key_axis(ActionId::new(id).unwrap(), key, sign));
    world.insert_resource(ActionMaps::new(ActionMap::from_bindings(bindings)?));
    let pixels = vec![50, 220, 255, 255, 232, 250, 125, 255];
    let image = ImageAsset::new(
        ImageExtent::new(2, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        pixels,
    )?;
    let texture = add_asset(world, image)?;
    let sound = AudioClip::from_bytes(include_bytes!("assets/pickup.wav"))?;
    let sound = add_asset(world, sound)?;
    let half = Vec2::new(0.5, 1.0);
    let frames = [Vec2::ZERO, Vec2::new(0.5, 0.0)]
        .map(|min| Frame::new(TextureRegion::new(min, half), Duration::from_millis(150)).unwrap());
    let clip = Clip::new(PlaybackMode::Loop, frames)?;
    let character = TopDownCharacter::new(Collider2d::Circle { radius: 16.0 }, 0.1)?;
    let camera = Camera2d {
        viewport_height: 300.0,
        ..Default::default()
    };
    world.spawn((camera, Transform2d::default()));
    world.spawn((
        Player(clip, character, sound),
        Transform2d::from_xy(-135.0, 0.0),
        Sprite::from_texture(texture, Vec2::splat(32.0)),
    ));
    let collider = Collider2d::Box {
        half_extents: Vec2::new(14.0, 90.0),
    };
    let sprite = Sprite::from_color(Vec2::new(28.0, 180.0), Color::rgb(0.82, 0.29, 0.20));
    world.spawn((Transform2d::from_xy(90.0, 0.0), collider, sprite));
    let mut floor = Tilemap::new(Vec2::splat(24.0)).with_layer(-1);
    let cells = [Color::rgb(0.10, 0.13, 0.17), Color::rgb(0.14, 0.19, 0.24)]
        .map(|color| TileCell::new(TileIndex::new(1)).with_color(color));
    for x in -10..10 {
        for y in -5..5 {
            floor.set_cell(TileCoord::new(x, y), cells[((x + y) & 1) as usize].clone());
        }
    }
    world.spawn((floor, Transform2d::default()));
    let hud = UiText::new("WASD  /  Move into the wall to play a sound", 21.0)?;
    let hud_key = UiRuntimeKey::new("first-game", "hud")?;
    world.spawn((UiRoot::default(), UiNode::default(), hud_key, hud));
    Ok(())
}

fn play(
    mut commands: Commands,
    input: Res<ActionState>,
    time: Res<FixedTime>,
    physics: Res<PhysicsWorld2d>,
    mut player: Single<(&Player, &mut Transform2d, &mut Sprite)>,
    mut hit: Local<bool>,
) -> nara::ecs::bevy_ecs::error::Result {
    let (player, transform, sprite) = &mut *player;
    let axis = input.axis_2d("move").clamp_length_max(1.0);
    let desire = axis * 130.0 * time.delta().as_secs_f32();
    let pose = PhysicsPose2d::new(transform.translation, transform.rotation);
    let motion = physics.move_top_down_character(&player.1, pose, desire, time.delta(), None)?;
    let collided = motion.collision_count > 0;
    if collided && !*hit {
        commands.spawn(AudioPlayer::one_shot(player.2));
    }
    *hit = collided;
    transform.translation += motion.translation;
    sprite.texture_region = Some(player.0.sample(time.elapsed()).region);
    Ok(())
}
