use std::sync::Arc;

use crate::kernel::{UiFrame, UiNonActivation, UiPoint};
use nara_app::App;
use nara_core::Vec2;
use nara_ecs::World;
use nara_input::{
    ButtonDriverInput, InputPointerId, InputSeatId, InputTimeline, InputWindowEvent, InputWindowId,
    MouseButton, PointerDriverInput,
};
use nara_scene::hierarchy::{HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter};

use crate::{
    UiActionConsumerId, UiActionId, UiActionJournal, UiActivationSource, UiButton, UiColorRole,
    UiNode, UiOverflow, UiPlugin, UiPresentationReceipt, UiRoot, UiRouteRejection,
    UiRouteRejectionLog, UiRuntimeKey, UiSeatId, UiSize, UiStyle, UiSurfaceContentTransform,
    UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot, UiVisual, install_ui_surface,
    interaction::route_timeline_ui_input,
    presentation::{
        UiPresentationCommit, commit_prepared_ui_presentation, prepare_ui_presentation,
    },
    runtime::compile_ui_frames,
    ui_candidate_frame, ui_semantic_snapshot,
};

const SURFACE: UiSurfaceId = UiSurfaceId::new(7).unwrap();
const WINDOW: InputWindowId = InputWindowId::new(11);
const SEAT: InputSeatId = InputSeatId::PRIMARY;
const POINTER: InputPointerId = InputPointerId::PRIMARY;

fn present(world: &mut World, epoch: u64) -> (Arc<UiFrame>, UiPresentationReceipt) {
    compile_ui_frames(world);
    let frame = ui_candidate_frame(world, SURFACE).unwrap().unwrap();
    let prepared = prepare_ui_presentation(world, Arc::clone(&frame), epoch).unwrap();
    let receipt = match commit_prepared_ui_presentation(world, prepared) {
        UiPresentationCommit::Committed(receipt) => receipt,
        UiPresentationCommit::Invalidated { reason } => {
            panic!("test frame must publish its input route: {reason:?}");
        }
    };
    (frame, receipt)
}

fn route_button_edge(world: &mut World, input: ButtonDriverInput<MouseButton>) -> u64 {
    let admission = world
        .resource_mut::<InputTimeline>()
        .admit_mouse_button(WINDOW, SEAT, POINTER, input)
        .unwrap();
    route_timeline_ui_input(world);
    admission.first_sequence().unwrap()
}

#[test]
fn held_resume_releases_once_across_keyed_hud_reorder_and_clipping() {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_scene::hierarchy::HierarchyPlugin,
        nara_input::InputPlugin,
        UiPlugin::default(),
    ))
    .unwrap();
    let world = app.world_mut().unwrap();
    {
        let mut timeline = world.resource_mut::<InputTimeline>();
        timeline.connect_endpoint(WINDOW, SEAT).unwrap();
        timeline.connect_pointer(WINDOW, SEAT, POINTER).unwrap();
        timeline
            .admit_window_event(WINDOW, SEAT, InputWindowEvent::Focused(true))
            .unwrap();
    }
    let size = UiSize::new(200.0, 100.0).unwrap();
    let transform = UiSurfaceContentTransform::direct(size, 1.0).unwrap();
    let endpoint = UiSurfaceEndpoint::new(WINDOW, SEAT, POINTER);
    let slot = UiSurfaceSlot::new(SURFACE, endpoint, transform).unwrap();
    install_ui_surface(world, slot).unwrap();
    let cursor = world
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("pointer-continuity").unwrap())
        .unwrap();

    let key = |name| UiRuntimeKey::new("nara.test.ui", name).unwrap();
    let button = |action| UiButton::new("Resume", UiActionId::new(action).unwrap(), 16.0).unwrap();
    let clip_style =
        |width| UiStyle::absolute(0.0, 64.0, width, 24.0).with_overflow(UiOverflow::Clip);
    let bar_style = |x, z| UiStyle::absolute(x, 2.0, 24.0, 8.0).with_z_index(z);
    let root = world
        .spawn((UiRoot::default(), UiNode::default(), key("menu")))
        .id();
    let resume = world
        .spawn((
            button("resume"),
            UiNode::new(UiStyle::absolute(10.0, 10.0, 80.0, 40.0)),
            UiVisual::role(UiColorRole::Focus),
            key("resume"),
        ))
        .id();
    let clip = world.spawn(UiNode::new(clip_style(100.0))).id();
    let mut spawn_bar = |name, x, z| {
        world
            .spawn((
                UiNode::new(bar_style(x, z)),
                UiVisual::role(UiColorRole::Focus),
                key(name),
            ))
            .id()
    };
    let moving_bar = spawn_bar("moving-bar", 44.0, 4);
    let fixed_bar = spawn_bar("fixed-bar", 4.0, 5);
    let mut writer = HierarchyWriter::new(world);
    writer
        .place(
            root,
            HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        )
        .unwrap();
    for (entity, parent) in [
        (resume, root),
        (clip, root),
        (moving_bar, clip),
        (fixed_bar, clip),
    ] {
        let placement = HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent });
        writer.place(entity, placement).unwrap();
    }
    nara_scene::hierarchy::complete_fixed_input_hierarchy(world).unwrap();

    let (initial, receipt) = present(world, 1);
    let position = Vec2::new(20.0, 20.0);
    let target = initial
        .hit_test(UiPoint::new(position.x, position.y))
        .unwrap()
        .key();
    let is_pressed = |world: &World, receipt| {
        ui_semantic_snapshot(world, receipt, UiSeatId::PRIMARY)
            .unwrap()
            .nodes()
            .iter()
            .find(|node| node.key() == target)
            .unwrap()
            .state()
            .pressed()
    };
    let painted_bars = |frame: &UiFrame| {
        frame
            .paint()
            .iter()
            .filter(|primitive| primitive.rect().origin().y() == 66.0)
            .filter_map(|primitive| primitive.key().cloned())
            .collect::<Vec<_>>()
    };
    world
        .resource_mut::<InputTimeline>()
        .admit_pointer(WINDOW, SEAT, POINTER, PointerDriverInput::Moved(position))
        .unwrap();
    route_button_edge(world, ButtonDriverInput::Press(MouseButton::Left));
    assert!(is_pressed(world, receipt));

    world
        .entity_mut(moving_bar)
        .insert(UiNode::new(bar_style(44.0, 6)));
    let (reordered, receipt) = present(world, 2);
    let bars = painted_bars(&initial);
    assert_eq!(bars.len(), 2);
    assert_eq!(painted_bars(&reordered), [bars[1].clone(), bars[0].clone()]);
    assert!(is_pressed(world, receipt));

    world.entity_mut(clip).insert(UiNode::new(clip_style(32.0)));
    let (clipped, receipt) = present(world, 3);
    assert_eq!(painted_bars(&clipped), [bars[1].clone()]);
    for frame in [reordered, clipped] {
        assert_eq!(frame.environment(), initial.environment());
        assert_eq!(frame.hit_regions(), initial.hit_regions());
        assert_eq!(frame.focus_order(), initial.focus_order());
        assert_eq!(frame.semantics(), initial.semantics());
    }
    assert!(is_pressed(world, receipt));
    assert!(
        world
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
    let release = ButtonDriverInput::Release(MouseButton::Left);
    let release_sequence = route_button_edge(world, release);
    let actions = world
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    let [action] = actions else {
        panic!("the original button release must produce exactly one action: {actions:?}");
    };
    assert_eq!(action.action().as_str(), "resume");
    assert_eq!(action.target(), target);
    assert_eq!(action.source(), UiActivationSource::Pointer);
    assert_eq!(action.input_sequence(), Some(release_sequence));
    assert_eq!(action.presentation_epoch(), receipt.presentation_epoch());
    assert!(!is_pressed(world, receipt));

    route_button_edge(world, ButtonDriverInput::Press(MouseButton::Left));
    assert!(is_pressed(world, receipt));
    world.entity_mut(resume).insert(button("replacement"));
    let (replaced, receipt) = present(world, 4);
    assert_eq!(replaced.hit_regions(), initial.hit_regions());
    assert!(!is_pressed(world, receipt));
    let release_sequence = route_button_edge(world, release);
    let rejection = world
        .resource::<UiRouteRejectionLog>()
        .as_slice()
        .first()
        .unwrap();
    assert_eq!(rejection.input_sequence(), Some(release_sequence));
    assert_eq!(
        rejection.rejection(),
        &UiRouteRejection::NonActivation(UiNonActivation::NoPointerCapture)
    );
    assert_eq!(
        world
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .len(),
        1
    );
}
