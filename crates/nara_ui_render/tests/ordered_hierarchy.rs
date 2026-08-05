use nara_app::App;
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Query, system::SystemState};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
    HierarchyWriter,
};
use nara_input::InputPlugin;
use nara_render::{Camera2d, RenderPlugin, ViewportRect};
use nara_transform::{Transform2d, TransformPlugin};
use nara_ui::{ComputedUiLayouts, UiNode, UiPanel, UiPlugin, UiRoot, UiStyle, top_hit};

use nara_ui_render::{__private::UiBatchReader, UiRenderPlugin};

fn ui_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        HierarchyPlugin,
        TransformPlugin,
        RenderPlugin,
        InputPlugin,
        nara_image::ImagePreparePlugin,
        UiPlugin,
        UiRenderPlugin,
    ))
    .unwrap();
    app.world_mut().unwrap().spawn((
        Camera2d {
            viewport: Some(ViewportRect::new(0, 0, 200, 100).unwrap()),
            ..Camera2d::default()
        },
        Transform2d::default(),
    ));
    app
}

fn place_ui(world: &mut nara_ecs::World, root: Entity, first: Entity, second: Entity) {
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: first,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: root,
                }),
            },
            HierarchyOperation::Place {
                entity: second,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: root,
                }),
            },
        ])
        .unwrap();
}

#[test]
fn layout_hit_and_render_batch_follow_structural_order_not_entity_allocation() {
    let mut app = ui_app();
    let root = app
        .world_mut()
        .unwrap()
        .spawn(UiRoot::primary_window())
        .id();
    let allocated_early_color = Color::rgba(0.0, 0.0, 1.0, 1.0);
    let allocated_late_color = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let allocated_early = app
        .world_mut()
        .unwrap()
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 100.0)),
            UiPanel::from_color(allocated_early_color),
        ))
        .id();
    let _unrelated = app.world_mut().unwrap().spawn_empty().id();
    let allocated_late = app
        .world_mut()
        .unwrap()
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 100.0)),
            UiPanel::from_color(allocated_late_color),
        ))
        .id();
    let (first, first_color, second, second_color) =
        if allocated_early.to_bits() > allocated_late.to_bits() {
            (
                allocated_early,
                allocated_early_color,
                allocated_late,
                allocated_late_color,
            )
        } else {
            (
                allocated_late,
                allocated_late_color,
                allocated_early,
                allocated_early_color,
            )
        };
    assert!(first.to_bits() > second.to_bits());
    place_ui(app.world_mut().unwrap(), root, first, second);

    app.world_mut()
        .unwrap()
        .resource_mut::<nara_input::PointerState>()
        .set_position(Vec2::new(10.0, 10.0));
    app.run_once(std::time::Duration::ZERO).unwrap();

    let layouts = app.world().resource::<ComputedUiLayouts>().clone();
    let child_layouts = layouts
        .as_slice()
        .iter()
        .filter(|layout| layout.entity == first || layout.entity == second)
        .collect::<Vec<_>>();
    assert_eq!(child_layouts.len(), 2);
    assert_eq!(child_layouts[0].entity, first);
    assert_eq!(child_layouts[1].entity, second);
    assert!(child_layouts[0].structural_index < child_layouts[1].structural_index);

    let mut node_state = SystemState::<Query<&UiNode>>::new(app.world_mut().unwrap());
    let nodes = {
        let world = app.world_mut().unwrap();
        node_state
            .get(world)
            .expect("UI node query should validate")
    };
    assert_eq!(
        top_hit(Vec2::new(10.0, 10.0), &layouts, &nodes,),
        Some(second)
    );

    let mut batch_state = SystemState::<UiBatchReader>::new(app.world_mut().unwrap());
    let snapshot = {
        let world = app.world_mut().unwrap();
        batch_state
            .get(world)
            .expect("UI batch reader should validate")
            .snapshot()
    }
    .expect("UI render extraction should publish a batch snapshot");
    let colors = snapshot
        .as_slice()
        .iter()
        .flat_map(|batch| batch.instances.first().map(|instance| instance.color))
        .collect::<Vec<_>>();
    assert_eq!(colors, [first_color, second_color]);
}
