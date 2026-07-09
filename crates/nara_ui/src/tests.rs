use std::time::Duration;

use nara_app::App;
use nara_asset::{
    AssetId, AssetPath, AssetRecord, AssetRef, AssetServer, AssetSourceKind, Handle,
    ProjectAssetDatabase, StableAssetId,
};
use nara_core::{Color, Vec2};
use nara_ecs::World;
use nara_input::{ButtonInput, MouseButton, PointerState};
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_reflect::{
    ComponentDecodeContext, ComponentEncodeContext, ComponentRegistry, ComponentTypeId,
    ComponentValue, ComponentValueKind,
};
use nara_render::{Camera2d, RenderImage2d, RenderTarget, ViewportRect};
use nara_scene::Parent;

use crate::{
    ComputedUiLayouts, UiInteractionState, UiNode, UiPanel, UiPlugin, UiPointerRoute, UiRoot,
    UiStyle, UiVal, register_ui_components,
};

#[test]
fn root_targeting_primary_view_produces_child_rectangles() {
    let mut app = App::new();
    app.add_plugin(UiPlugin).unwrap();
    app.world_mut().spawn(Camera2d {
        viewport: Some(ViewportRect::new(0, 0, 200, 100).unwrap()),
        ..Camera2d::default()
    });
    let root = app.world_mut().spawn(UiRoot::primary_window()).id();
    let child = app
        .world_mut()
        .spawn((
            UiNode::new(
                UiStyle::default()
                    .with_position(UiVal::Percent(0.5), UiVal::Px(10.0))
                    .with_size(UiVal::Percent(0.25), UiVal::Px(20.0)),
            ),
            Parent(root),
        ))
        .id();

    app.run_once(Duration::ZERO).unwrap();

    let layouts = app.world().resource::<ComputedUiLayouts>();
    assert_eq!(
        layouts.get(root).unwrap().rect,
        crate::UiRect::from_origin_size(0.0, 0.0, 200.0, 100.0)
    );
    assert_eq!(
        layouts.get(child).unwrap().rect,
        crate::UiRect::from_origin_size(100.0, 10.0, 50.0, 20.0)
    );
}

#[test]
fn computed_layout_and_interaction_state_are_runtime_only() {
    let mut registry = ComponentRegistry::new();
    register_ui_components(&mut registry);

    assert!(
        registry
            .schema(&ComponentTypeId::new("nara.ui.UiRoot"))
            .is_some()
    );
    assert!(
        registry
            .schema(&ComponentTypeId::new("nara.ui.UiNode"))
            .is_some()
    );
    assert!(
        registry
            .schema(&ComponentTypeId::new("nara.ui.UiPanel"))
            .is_some()
    );
    assert!(
        registry
            .schema(&ComponentTypeId::new("nara.ui.ComputedUiLayout"))
            .is_none()
    );
    assert!(
        registry
            .schema(&ComponentTypeId::new("nara.ui.UiInteractionState"))
            .is_none()
    );
}

#[test]
fn ui_node_codec_roundtrips_stable_authoring_fields() {
    let mut registry = ComponentRegistry::new();
    register_ui_components(&mut registry);
    let id = ComponentTypeId::new("nara.ui.UiNode");
    let value = ComponentValue::map([
        (
            "style",
            ComponentValue::map([
                ("left", ui_val_value("px", 8.0)),
                ("top", ui_val_value("percent", 0.25)),
                ("width", ui_val_value("px", 64.0)),
                ("height", ui_val_value("auto", 0.0)),
            ]),
        ),
        ("z_index", ComponentValue::I64(7)),
        ("visible", ComponentValue::Bool(false)),
        ("focusable", ComponentValue::Bool(true)),
        ("clip", ComponentValue::Bool(true)),
    ]);
    let prepared = registry.preflight_component(&id, &value).unwrap().unwrap();
    let mut world = World::new();
    let entity = world.spawn_empty().id();

    prepared.apply(&mut world, entity).unwrap();

    let node = world.get::<UiNode>(entity).unwrap();
    assert_eq!(node.style.left, UiVal::Px(8.0));
    assert_eq!(node.style.top, UiVal::Percent(0.25));
    assert_eq!(node.style.width, UiVal::Px(64.0));
    assert_eq!(node.style.height, UiVal::Auto);
    assert_eq!(node.z_index, 7);
    assert!(!node.visible);
    assert!(node.focusable);
    assert!(node.clip);

    let encoded = registry
        .encode_component(&id, &world, entity)
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(encoded.field_i64("z_index").unwrap(), 7);
    assert_eq!(
        registry
            .schema(&id)
            .unwrap()
            .fields
            .iter()
            .map(|field| (field.path.to_string(), field.value_kind, field.required))
            .collect::<Vec<_>>(),
        vec![
            ("clip".to_string(), ComponentValueKind::Bool, false),
            ("focusable".to_string(), ComponentValueKind::Bool, false),
            (
                "style.height.kind".to_string(),
                ComponentValueKind::String,
                false
            ),
            (
                "style.height.value".to_string(),
                ComponentValueKind::F64,
                false
            ),
            (
                "style.left.kind".to_string(),
                ComponentValueKind::String,
                false
            ),
            (
                "style.left.value".to_string(),
                ComponentValueKind::F64,
                false
            ),
            (
                "style.top.kind".to_string(),
                ComponentValueKind::String,
                false
            ),
            (
                "style.top.value".to_string(),
                ComponentValueKind::F64,
                false
            ),
            (
                "style.width.kind".to_string(),
                ComponentValueKind::String,
                false
            ),
            (
                "style.width.value".to_string(),
                ComponentValueKind::F64,
                false
            ),
            ("visible".to_string(), ComponentValueKind::Bool, false),
            ("z_index".to_string(), ComponentValueKind::I64, false),
        ]
    );
}

#[test]
fn hidden_and_zero_size_nodes_do_not_hit_test() {
    let mut app = App::new();
    app.add_plugin(UiPlugin).unwrap();
    app.world_mut().spawn(Camera2d {
        viewport: Some(ViewportRect::new(0, 0, 200, 100).unwrap()),
        ..Camera2d::default()
    });
    let root = app.world_mut().spawn(UiRoot::primary_window()).id();
    app.world_mut().spawn((
        UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 100.0)).with_visible(false),
        Parent(root),
    ));
    app.world_mut().spawn((
        UiNode::new(UiStyle::absolute(0.0, 0.0, 0.0, 100.0)),
        Parent(root),
    ));
    app.world_mut()
        .resource_mut::<PointerState>()
        .set_position(Vec2::new(10.0, 10.0));

    app.run_once(Duration::ZERO).unwrap();

    let interaction = app.world().resource::<UiInteractionState>();
    assert_eq!(interaction.hovered(), None);
    assert_eq!(interaction.pressed(), None);
    assert_eq!(interaction.focused(), None);
}

#[test]
fn overlapping_nodes_choose_highest_order_and_focus_on_press() {
    let mut app = App::new();
    app.add_plugin(UiPlugin).unwrap();
    app.world_mut().spawn(Camera2d {
        viewport: Some(ViewportRect::new(0, 0, 200, 100).unwrap()),
        ..Camera2d::default()
    });
    let root = app.world_mut().spawn(UiRoot::primary_window()).id();
    let lower = app
        .world_mut()
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 100.0)).with_z_index(1),
            Parent(root),
        ))
        .id();
    let upper = app
        .world_mut()
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 100.0))
                .with_z_index(2)
                .focusable(),
            Parent(root),
        ))
        .id();
    app.world_mut()
        .resource_mut::<PointerState>()
        .set_position(Vec2::new(10.0, 10.0));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);

    app.run_once(Duration::ZERO).unwrap();

    let interaction = app.world().resource::<UiInteractionState>();
    assert_ne!(interaction.hovered(), Some(lower));
    assert_eq!(interaction.hovered(), Some(upper));
    assert_eq!(interaction.pressed(), Some(upper));
    assert_eq!(interaction.focused(), Some(upper));

    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.run_once(Duration::ZERO).unwrap();

    let interaction = app.world().resource::<UiInteractionState>();
    assert_eq!(interaction.hovered(), Some(upper));
    assert_eq!(interaction.pressed(), None);
    assert_eq!(interaction.focused(), Some(upper));
}

#[test]
fn routed_pointer_hits_only_matching_view_target() {
    let mut app = App::new();
    app.add_plugin(UiPlugin).unwrap();
    let first_target = render_image_target(1);
    let second_target = render_image_target(2);
    app.world_mut().spawn(Camera2d {
        target: first_target,
        viewport: Some(ViewportRect::new(0, 0, 200, 100).unwrap()),
        order: 0,
        ..Camera2d::default()
    });
    app.world_mut().spawn(Camera2d {
        target: second_target,
        viewport: Some(ViewportRect::new(0, 0, 200, 100).unwrap()),
        order: 1,
        ..Camera2d::default()
    });
    let first_root = app
        .world_mut()
        .spawn(UiRoot {
            target: first_target,
            order: 0,
        })
        .id();
    let second_root = app
        .world_mut()
        .spawn(UiRoot {
            target: second_target,
            order: 0,
        })
        .id();
    let first = app
        .world_mut()
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 100.0)),
            Parent(first_root),
        ))
        .id();
    let second = app
        .world_mut()
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 100.0)),
            Parent(second_root),
        ))
        .id();
    app.world_mut()
        .resource_mut::<PointerState>()
        .set_position(Vec2::new(10.0, 10.0));
    app.world_mut()
        .resource_mut::<UiInteractionState>()
        .set_pointer_route(UiPointerRoute::for_target_view(second_target, 1));

    app.run_once(Duration::ZERO).unwrap();

    let interaction = app.world().resource::<UiInteractionState>();
    let hovered = interaction.hovered_target().unwrap();
    assert_ne!(hovered.entity, first);
    assert_eq!(hovered.entity, second);
    assert_eq!(hovered.target, second_target);
    assert_eq!(hovered.view_index, 1);
}

#[test]
fn pressed_node_remains_captured_until_release_after_pointer_leaves_rect() {
    let mut app = App::new();
    app.add_plugin(UiPlugin).unwrap();
    app.world_mut().spawn(Camera2d {
        viewport: Some(ViewportRect::new(0, 0, 200, 100).unwrap()),
        ..Camera2d::default()
    });
    let root = app.world_mut().spawn(UiRoot::primary_window()).id();
    let button = app
        .world_mut()
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 100.0)).focusable(),
            Parent(root),
        ))
        .id();
    app.world_mut()
        .resource_mut::<PointerState>()
        .set_position(Vec2::new(10.0, 10.0));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);

    app.run_once(Duration::ZERO).unwrap();
    assert_eq!(
        app.world().resource::<UiInteractionState>().pressed(),
        Some(button)
    );
    assert!(
        !app.world()
            .resource::<ButtonInput<MouseButton>>()
            .just_pressed(MouseButton::Left)
    );

    app.world_mut()
        .resource_mut::<PointerState>()
        .set_position(Vec2::new(150.0, 10.0));
    app.run_once(Duration::ZERO).unwrap();

    let interaction = app.world().resource::<UiInteractionState>();
    assert_eq!(interaction.hovered(), None);
    assert_eq!(interaction.pressed(), Some(button));
    assert_eq!(interaction.focused(), Some(button));

    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.run_once(Duration::ZERO).unwrap();

    assert_eq!(app.world().resource::<UiInteractionState>().pressed(), None);
}

#[test]
fn clipped_child_does_not_hit_test_outside_parent_clip() {
    let mut app = App::new();
    app.add_plugin(UiPlugin).unwrap();
    app.world_mut().spawn(Camera2d {
        viewport: Some(ViewportRect::new(0, 0, 200, 100).unwrap()),
        ..Camera2d::default()
    });
    let root = app.world_mut().spawn(UiRoot::primary_window()).id();
    let clipped_parent = app
        .world_mut()
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 50.0, 50.0)).clipping_children(),
            Parent(root),
        ))
        .id();
    app.world_mut().spawn((
        UiNode::new(UiStyle::absolute(40.0, 40.0, 50.0, 50.0)),
        Parent(clipped_parent),
    ));
    app.world_mut()
        .resource_mut::<PointerState>()
        .set_position(Vec2::new(75.0, 75.0));

    app.run_once(Duration::ZERO).unwrap();

    assert_eq!(app.world().resource::<UiInteractionState>().hovered(), None);
}

#[test]
fn ui_panel_codec_resolves_stable_image_refs_during_preflight() {
    let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            stable_id,
            AssetPath::new("textures/ui/panel.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    let mut asset_server = AssetServer::new();
    let mut context = ComponentDecodeContext::with_asset_server(&mut asset_server)
        .with_project_asset_database(&database);
    let mut registry = ComponentRegistry::new();
    register_ui_components(&mut registry);
    let id = ComponentTypeId::new("nara.ui.UiPanel");
    let value = ui_panel_value(AssetRef::StableId(stable_id));

    let prepared = registry
        .preflight_component_with_context(&id, &value, &mut context)
        .unwrap()
        .unwrap();
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    prepared.apply(&mut world, entity).unwrap();

    let panel = world.get::<UiPanel>(entity).unwrap();
    let image = panel.material.image.unwrap();
    assert_eq!(asset_server.path(image.id()), Some("textures/ui/panel.png"));
    assert_eq!(panel.material.sampler, SamplerDescriptor::NEAREST_CLAMP);
    assert_eq!(panel.material.alpha_mode, AlphaMode2d::Blend);

    world.insert_resource(asset_server);
    let encoded = registry
        .encode_component_with_context(&id, &world, entity, &ComponentEncodeContext::new())
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        encoded.field("material").unwrap(),
        ComponentValue::Map(_)
    ));
}

fn ui_panel_value(image: AssetRef) -> ComponentValue {
    ComponentValue::map([(
        "material",
        ComponentValue::map([
            ("image", asset_ref_value(&image)),
            (
                "sampler",
                ComponentValue::map([
                    ("min_filter", ComponentValue::String("nearest".to_string())),
                    ("mag_filter", ComponentValue::String("nearest".to_string())),
                    (
                        "mipmap_filter",
                        ComponentValue::String("nearest".to_string()),
                    ),
                    (
                        "address_mode_u",
                        ComponentValue::String("clamp_to_edge".to_string()),
                    ),
                    (
                        "address_mode_v",
                        ComponentValue::String("clamp_to_edge".to_string()),
                    ),
                ]),
            ),
            ("alpha_mode", ComponentValue::String("blend".to_string())),
            ("tint", color_value(Color::WHITE)),
        ]),
    )])
}

fn ui_val_value(kind: &str, value: f32) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_string())),
        ("value", ComponentValue::f64(f64::from(value)).unwrap()),
    ])
}

fn color_value(value: Color) -> ComponentValue {
    ComponentValue::map([
        ("r", ComponentValue::f64(f64::from(value.r)).unwrap()),
        ("g", ComponentValue::f64(f64::from(value.g)).unwrap()),
        ("b", ComponentValue::f64(f64::from(value.b)).unwrap()),
        ("a", ComponentValue::f64(f64::from(value.a)).unwrap()),
    ])
}

fn asset_ref_value(asset_ref: &AssetRef) -> ComponentValue {
    match asset_ref {
        AssetRef::Path(path) => ComponentValue::map([
            ("kind", ComponentValue::String("path".to_string())),
            ("value", ComponentValue::String(path.as_str().to_string())),
        ]),
        AssetRef::StableId(id) => ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_string())),
            ("value", ComponentValue::String(id.to_string())),
        ]),
    }
}

fn stable_id(id: &str) -> StableAssetId {
    StableAssetId::parse_str(id).unwrap()
}

fn render_image_target(id: u64) -> RenderTarget {
    RenderTarget::Image(Handle::<RenderImage2d>::new(AssetId::from_raw(id)))
}
