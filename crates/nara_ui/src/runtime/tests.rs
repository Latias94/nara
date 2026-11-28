use std::{sync::Arc, time::Duration};

use nara_app::{App, CoreStage, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance};
use nara_ecs::{
    Res, ResMut, Resource, World,
    bevy_ecs::{hierarchy::ChildOf, message::Messages},
};
use nara_input::{
    ActionBinding, ActionId, ActionMap, ActionMaps, ActionState, ButtonTransitionPhase as Phase,
    InputEdge, InputEdgeKind, InputPlugin, InputPointerId, InputSeatId, InputWindowEvent,
    InputWindowId, KeyCode, MouseButton,
};

use crate::{
    UiActionEvent, UiActivationSource, UiButton, UiLayer, UiNode, UiPlugin, UiRoot, UiRuntimeKey,
    UiSafeAreaInsets, UiSize, UiStyle, UiText, kernel::UiPaintPrimitive,
};

use super::*;

const WINDOW: InputWindowId = InputWindowId::PRIMARY;

fn surface() -> UiSurfaceId {
    UiSurfaceId::new(1).unwrap()
}

fn endpoint() -> UiSurfaceEndpoint {
    UiSurfaceEndpoint::new(WINDOW, InputSeatId::PRIMARY, InputPointerId::PRIMARY)
}

fn transform() -> UiSurfaceContentTransform {
    UiSurfaceContentTransform::new(
        UiSize::new(200.0, 100.0).unwrap(),
        UiSize::new(400.0, 300.0).unwrap(),
        UiSafeAreaInsets::default(),
        2.0,
    )
    .unwrap()
}

#[derive(Default, Resource)]
struct GameplayFire(bool);

fn app(font: FontFace) -> App {
    let mut app = App::new();
    app.add_plugins((InputPlugin, UiPlugin::with_font(font)))
        .unwrap();
    app.insert_resource(ActionMaps::new(
        ActionMap::from_bindings([ActionBinding::mouse(
            ActionId::new("fire").unwrap(),
            MouseButton::Left,
        )])
        .unwrap(),
    ))
    .unwrap();
    app.init_resource::<GameplayFire>().unwrap();
    app.add_systems(
        CoreStage::Update,
        |state: Res<ActionState>, mut fire: ResMut<GameplayFire>| {
            fire.0 = state.button("fire");
        },
    )
    .unwrap();
    install_ui_surface(
        app.world_mut().unwrap(),
        UiSurfaceSlot::new(surface(), endpoint(), transform()).unwrap(),
    )
    .unwrap();
    app
}

fn default_app() -> App {
    app(crate::text::bundled_font_face().unwrap())
}

fn frame(runtime: &RuntimeInstance) -> Arc<UiFrame> {
    ui_candidate_frame(runtime.world(), surface())
        .unwrap()
        .unwrap()
}

fn in_world<T>(runtime: &mut RuntimeInstance, operation: impl FnOnce(&mut World) -> T) -> T {
    let result = runtime
        .with_driver_scope(|scope| scope.with_world_mut(operation))
        .unwrap();
    assert!(result.post_operation_fault.is_none());
    result.output
}

fn submit(
    runtime: &mut RuntimeInstance,
    frame: &Arc<UiFrame>,
    epoch: u64,
) -> UiPresentationReceipt {
    in_world(runtime, |world| {
        publish_presented_frame(world, frame, epoch)
    })
    .unwrap()
}

fn input(
    runtime: &mut RuntimeInstance,
    window: InputWindowId,
    edges: impl IntoIterator<Item = InputEdgeKind>,
) {
    input_at(runtime, window, InputSeatId::PRIMARY, edges);
}

fn input_at(
    runtime: &mut RuntimeInstance,
    window: InputWindowId,
    seat: InputSeatId,
    edges: impl IntoIterator<Item = InputEdgeKind>,
) {
    in_world(runtime, |world| {
        for edge in edges {
            world
                .write_message(InputEdge::new(window, seat, edge))
                .unwrap();
        }
    });
}

fn mouse(phase: Phase, position: [f32; 2]) -> InputEdgeKind {
    InputEdgeKind::PointerButton {
        pointer: InputPointerId::PRIMARY,
        button: MouseButton::Left,
        phase,
        position: Some(position.into()),
    }
}

fn click(position: [f32; 2]) -> [InputEdgeKind; 2] {
    [
        mouse(Phase::Pressed, position),
        mouse(Phase::Released, position),
    ]
}

fn button(world: &mut World, parent: Entity, left: f32) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            UiRuntimeKey::new("test", "retry").unwrap(),
            UiNode::new(UiStyle::absolute(left, 12.0, 68.0, 34.0)),
            UiButton::new("Retry", UiActionId::new("retry").unwrap(), 14.0).unwrap(),
        ))
        .id()
}

#[test]
fn code_first_children_paint_the_configured_font_and_reuse_an_unchanged_frame() {
    let font = FontFace::from_bytes(
        include_bytes!("../../tests/text/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let mut app = app(font.clone());
    let world = app.world_mut().unwrap();
    let root = world.spawn(UiRoot::default()).id();
    world.spawn((ChildOf(root), UiText::new("HUD ready", 20.0).unwrap()));
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let compiled = frame(&runtime);
    assert_eq!(compiled.layouts().len(), 2);
    let [UiPaintPrimitive::Text { layout, .. }] = compiled.paint() else {
        panic!("expected HUD text")
    };
    assert!(!layout.glyphs().is_empty());
    assert!(
        layout
            .glyphs()
            .iter()
            .all(|glyph| glyph.key().face_id() == font.id())
    );
    assert!(latest_presented_ui_frame(runtime.world(), surface()).is_none());

    runtime.drive(Duration::ZERO).unwrap();
    assert!(Arc::ptr_eq(&compiled, &frame(&runtime)));
    let receipt = submit(&mut runtime, &compiled, 1);
    let (presented, retained) = latest_presented_ui_frame(runtime.world(), surface()).unwrap();
    assert_eq!(presented, receipt);
    assert!(Arc::ptr_eq(&compiled, &retained));
    runtime.finish().unwrap();
}

#[test]
fn ordered_pointer_input_uses_the_accepted_layout_and_consumes_the_gameplay_press() {
    let mut app = default_app();
    let world = app.world_mut().unwrap();
    let root = world.spawn(UiRoot::default()).id();
    let target = button(world, root, 12.0);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let mut cursor = runtime
        .world()
        .resource::<Messages<UiActionEvent>>()
        .get_cursor();
    let original = frame(&runtime);
    input(&mut runtime, WINDOW, click([30.0, 48.0]));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(cursor.read(runtime.world().resource()).count(), 0);
    submit(&mut runtime, &original, 1);

    in_world(&mut runtime, |world| {
        world
            .entity_mut(target)
            .insert(UiNode::new(UiStyle::absolute(112.0, 12.0, 68.0, 34.0)));
    });
    runtime.drive(Duration::ZERO).unwrap();
    let moved = frame(&runtime);
    assert!(moved.revision() > original.revision());
    input(
        &mut runtime,
        WINDOW,
        [
            InputEdgeKind::Window(InputWindowEvent::Resized {
                logical_size: [200.0, 150.0].into(),
            }),
            mouse(Phase::Pressed, [30.0, 48.0]),
        ],
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!runtime.world().resource::<GameplayFire>().0);
    input(&mut runtime, WINDOW, [mouse(Phase::Released, [30.0, 48.0])]);
    runtime.drive(Duration::ZERO).unwrap();
    let [action] = cursor.read(runtime.world().resource()).collect::<Vec<_>>()[..] else {
        panic!("expected one activation")
    };
    assert_eq!(
        (action.target(), action.action().as_str(), action.source()),
        (target, "retry", UiActivationSource::Pointer)
    );

    submit(&mut runtime, &moved, 2);
    input(&mut runtime, WINDOW, click([30.0, 48.0]));
    input(&mut runtime, WINDOW, click([140.0, 4.0]));
    input(&mut runtime, InputWindowId::new(99), click([140.0, 48.0]));
    input(&mut runtime, WINDOW, click([140.0, 48.0]));
    runtime.drive(Duration::ZERO).unwrap();
    let [action] = cursor.read(runtime.world().resource()).collect::<Vec<_>>()[..] else {
        panic!("expected only the in-content click for this window")
    };
    assert_eq!((action.target(), action.sequence()), (target, 2));
    assert!(matches!(
        in_world(&mut runtime, |world| publish_presented_frame(
            world, &original, 3
        )),
        Err(UiAdapterError::StalePresentation)
    ));
    runtime.finish().unwrap();
}

#[test]
fn losing_the_display_clears_hits_until_the_candidate_is_presented_again() {
    let mut app = default_app();
    let world = app.world_mut().unwrap();
    let root = world.spawn(UiRoot::default()).id();
    let target = button(world, root, 12.0);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let presented = frame(&runtime);
    submit(&mut runtime, &presented, 1);
    let mut cursor = runtime
        .world()
        .resource::<Messages<UiActionEvent>>()
        .get_cursor();
    in_world(&mut runtime, |world| {
        clear_presented_ui_frame(world, surface());
        assert!(latest_presented_ui_frame(world, surface()).is_none());
        assert!(Arc::ptr_eq(
            &ui_candidate_frame(world, surface()).unwrap().unwrap(),
            &presented
        ));
    });
    input(&mut runtime, WINDOW, click([30.0, 48.0]));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(cursor.read(runtime.world().resource()).count(), 0);
    submit(&mut runtime, &presented, 2);
    input(&mut runtime, WINDOW, click([30.0, 48.0]));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        cursor
            .read(runtime.world().resource())
            .next()
            .unwrap()
            .target(),
        target
    );
    runtime.finish().unwrap();
}

#[test]
fn discontinuities_and_entity_replacement_cancel_pointer_activation() {
    let mut app = default_app();
    let world = app.world_mut().unwrap();
    let root = world.spawn(UiRoot::default()).id();
    let target = button(world, root, 12.0);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let presented = frame(&runtime);
    submit(&mut runtime, &presented, 1);
    let mut cursor = runtime
        .world()
        .resource::<Messages<UiActionEvent>>()
        .get_cursor();

    input(
        &mut runtime,
        WINDOW,
        [
            mouse(Phase::Pressed, [30.0, 48.0]),
            InputEdgeKind::Window(InputWindowEvent::Focused(false)),
        ],
    );
    runtime.drive(Duration::ZERO).unwrap();
    input(
        &mut runtime,
        WINDOW,
        [
            InputEdgeKind::Window(InputWindowEvent::Focused(true)),
            InputEdgeKind::Window(InputWindowEvent::ScaleFactorChanged { scale_factor: 1.5 }),
        ],
    );
    runtime.drive(Duration::ZERO).unwrap();
    input(&mut runtime, WINDOW, click([30.0, 48.0]));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(cursor.read(runtime.world().resource()).count(), 0);

    in_world(&mut runtime, |world| {
        suspend_ui_surface(world, surface(), UiSurfaceSuspensionReason::ZeroExtent).unwrap();
        restore_ui_surface(world, surface(), endpoint(), transform()).unwrap();
        assert!(latest_presented_ui_frame(world, surface()).is_none());
        assert!(matches!(
            publish_presented_frame(world, &presented, 2),
            Err(UiAdapterError::FrameSuperseded)
        ));
    });
    runtime.drive(Duration::ZERO).unwrap();
    let restored = frame(&runtime);
    submit(&mut runtime, &restored, 2);
    input(&mut runtime, WINDOW, [mouse(Phase::Pressed, [30.0, 48.0])]);
    runtime.drive(Duration::ZERO).unwrap();
    let replacement = in_world(&mut runtime, |world| {
        assert!(world.despawn(target));
        button(world, root, 12.0)
    });
    assert_ne!(target, replacement);
    input(&mut runtime, WINDOW, [mouse(Phase::Released, [30.0, 48.0])]);
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(cursor.read(runtime.world().resource()).count(), 0);
    runtime.finish().unwrap();
}

#[test]
fn window_discontinuities_reach_a_hud_bound_to_another_seat() {
    let seat = InputSeatId::new(4);
    let mut app = default_app();
    let world = app.world_mut().unwrap();
    update_ui_surface_slot(
        world,
        surface(),
        UiSurfaceEndpoint::new(WINDOW, seat, InputPointerId::PRIMARY),
        transform(),
    )
    .unwrap();
    let root = world.spawn(UiRoot::default()).id();
    let target = button(world, root, 12.0);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let presented = frame(&runtime);
    submit(&mut runtime, &presented, 1);
    let mut cursor = runtime
        .world()
        .resource::<Messages<UiActionEvent>>()
        .get_cursor();

    let point = [30.0, 48.0];
    input_at(&mut runtime, WINDOW, seat, [mouse(Phase::Pressed, point)]);
    input(
        &mut runtime,
        WINDOW,
        [InputEdgeKind::PointerLeft {
            pointer: InputPointerId::PRIMARY,
        }],
    );
    input_at(&mut runtime, WINDOW, seat, [mouse(Phase::Released, point)]);
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        cursor
            .read(runtime.world().resource())
            .map(UiActionEvent::target)
            .collect::<Vec<_>>(),
        [target]
    );

    input(
        &mut runtime,
        WINDOW,
        [false, true].map(|focused| InputEdgeKind::Window(InputWindowEvent::Focused(focused))),
    );
    input_at(
        &mut runtime,
        WINDOW,
        seat,
        [InputEdgeKind::Key {
            key: KeyCode::Enter,
            phase: Phase::Pressed,
        }],
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(cursor.read(runtime.world().resource()).count(), 0);

    input(
        &mut runtime,
        WINDOW,
        [InputEdgeKind::Window(
            InputWindowEvent::ScaleFactorChanged { scale_factor: 1.5 },
        )],
    );
    input_at(&mut runtime, WINDOW, seat, click(point));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(cursor.read(runtime.world().resource()).count(), 0);
    runtime.finish().unwrap();
}

#[test]
fn exact_steps_route_keyboard_once_and_hidden_modal_releases_gameplay() {
    let mut app = default_app();
    let world = app.world_mut().unwrap();
    let root = world.spawn(UiRoot::new(UiLayer::Modal)).id();
    let target = button(world, root, 12.0);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let visible = frame(&runtime);
    submit(&mut runtime, &visible, 1);
    assert!(visible.blocks_unhandled_input());

    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let mut cursor = runtime
        .world()
        .resource::<Messages<UiActionEvent>>()
        .get_cursor();
    input(
        &mut runtime,
        WINDOW,
        [KeyCode::Tab, KeyCode::Enter].map(|key| InputEdgeKind::Key {
            key,
            phase: Phase::Pressed,
        }),
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let [action] = cursor.read(runtime.world().resource()).collect::<Vec<_>>()[..] else {
        panic!("exact step must route one activation")
    };
    assert_eq!(
        (action.target(), action.source()),
        (target, UiActivationSource::Keyboard)
    );

    in_world(&mut runtime, |world| {
        world
            .entity_mut(root)
            .insert(UiNode::new(UiStyle::fill().with_visible(false)));
    });
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let hidden = frame(&runtime);
    assert!(!hidden.blocks_unhandled_input());
    submit(&mut runtime, &hidden, 2);
    input(&mut runtime, WINDOW, [mouse(Phase::Pressed, [30.0, 48.0])]);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<GameplayFire>().0);
    assert_eq!(cursor.read(runtime.world().resource()).count(), 0);
    runtime.finish().unwrap();
}
#[test]
fn ui_plugin_loads_an_authored_font_through_the_asset_server() {
    use nara_asset::{
        AssetAppExt, AssetPlugin, Assets, RuntimeAssetMode, RuntimeAssetSource, load_asset_sync,
        source::asset_id_path,
    };

    let face = crate::text::bundled_font_face().unwrap();
    let id = "0193a100-1111-7000-8000-000000000021".parse().unwrap();
    let mut files = nara_asset::source::MemoryAssetSource::new();
    files.insert(&asset_id_path(id), face.bytes()).unwrap();
    let mut app = App::new();
    app.add_plugins((AssetPlugin, InputPlugin, UiPlugin::default()))
        .unwrap();
    app.set_asset_source(RuntimeAssetSource::from_memory(
        RuntimeAssetMode::Source,
        files,
    ))
    .unwrap();
    let handle = load_asset_sync::<FontFace>(app.world_mut().unwrap(), id).unwrap();
    assert_eq!(
        app.world().resource::<Assets<FontFace>>().get(handle),
        Some(&face)
    );
}
