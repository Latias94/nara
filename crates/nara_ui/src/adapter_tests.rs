use std::{collections::BTreeMap, sync::Arc};

use nara_app::App;
use nara_asset::AssetRef;
use nara_ecs::{Entity, World};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_input::{InputSeatId, InputWindowId};
use nara_reflect::{
    ComponentRegistry, ComponentRegistryPlugin, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, SceneDocument, SceneEntityId,
    SceneEntityRecord, SceneEntitySource, ScenePatchDocument, spawn_scene_with_prefab_resolver,
};
use nara_ui_kernel::{
    UiActivationSource, UiButtonPhase, UiInputEvent, UiKey, UiLayer, UiLocalKey, UiPoint,
    UiPointerButton, UiPointerId, UiRootKey, UiRouteStamp, UiSeatId, UiSurfaceId,
};

use crate::{
    UiActionEvents, UiActionId, UiAdapterError, UiButtonVisuals, UiColor, UiColorRole, UiNode,
    UiPlugin, UiRoot, UiRuntimeKey, UiStyle, UiSurfaceDescriptor, UiVisual, commit_ui_presentation,
    dispatch_presented_ui_input, install_ui_surface, latest_ui_frame, register_ui_components,
    retire_ui_surface, ui_semantic_snapshot,
};

const TEST_DOMAIN: &str = "nara.test.ui";

fn ui_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_input::InputPlugin,
        UiPlugin,
    ))
    .expect("UI test plugins should install");
    app
}

fn surface_id() -> UiSurfaceId {
    UiSurfaceId::new(7).expect("test surface ID should be non-zero")
}

fn install_test_surface(world: &mut World, presentation_revision: u64) {
    install_ui_surface(
        world,
        UiSurfaceDescriptor::new(
            surface_id(),
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            200.0,
            100.0,
            1.0,
            1,
            presentation_revision,
            1,
        )
        .expect("test surface descriptor should be valid"),
    )
    .expect("test surface should install");
}

fn root_last() -> HierarchyPlacement {
    HierarchyPlacement::last(HierarchyPlacementOwner::Root)
}

fn child_last(parent: Entity) -> HierarchyPlacement {
    HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent })
}

fn place_ui_tree(world: &mut World, root: Entity, children: &[Entity]) {
    let mut operations = vec![HierarchyOperation::Place {
        entity: root,
        placement: root_last(),
    }];
    operations.extend(
        children
            .iter()
            .copied()
            .map(|entity| HierarchyOperation::Place {
                entity,
                placement: child_last(root),
            }),
    );
    HierarchyWriter::new(world)
        .apply_batch(&operations)
        .expect("test UI hierarchy should be valid");
}

fn expected_runtime_root(local: &str) -> UiRootKey {
    UiRootKey::runtime(TEST_DOMAIN, local).expect("test root key should be valid")
}

fn expected_runtime_node(root: &str, local: &str) -> UiKey {
    UiKey::scoped(
        expected_runtime_root(root),
        UiLocalKey::runtime(TEST_DOMAIN, local).expect("test local key should be valid"),
    )
    .expect("test scoped key should be valid")
}

fn button_bundle(
    local: &str,
    action: &str,
    color: UiColor,
) -> (crate::UiButton, UiNode, UiVisual, UiRuntimeKey) {
    (
        crate::UiButton::new(
            local,
            UiActionId::new(action).expect("test action should be valid"),
            16.0,
        )
        .expect("test button should be valid"),
        UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 50.0)),
        UiVisual::solid(color),
        UiRuntimeKey::new(TEST_DOMAIN, local).expect("test runtime key should be valid"),
    )
}

fn frozen_ui_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_ui_components(&mut registry).expect("UI component registration should succeed");
    registry
        .freeze()
        .expect("UI component registry should freeze");
    registry
}

#[test]
fn schema_catalog_is_canonical_closed_and_requires_every_field() {
    let catalog = crate::codec::ui_schema_catalog();
    let ids = catalog
        .components
        .iter()
        .map(|schema| schema.id().as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        [
            "nara.ui.UiRoot",
            "nara.ui.UiNode",
            "nara.ui.UiVisual",
            "nara.ui.UiText",
            "nara.ui.UiButton",
            "nara.ui.UiButtonVisuals",
            "nara.ui.UiSemantics",
            "nara.ui.UiTheme",
        ]
    );
    assert!(catalog.type_tombstones.is_empty());
    for schema in &catalog.components {
        assert_eq!(schema.version(), ComponentSchemaVersion::ONE);
        assert!(schema.field_tombstones().is_empty());
        assert!(!schema.fields().is_empty());
        assert!(
            schema
                .fields()
                .iter()
                .all(|field| field.required && field.default_value.is_none())
        );
    }

    let registry = frozen_ui_registry();
    assert!(
        registry
            .schema(&ComponentTypeId::new("nara.ui.UiPanel"))
            .is_none()
    );
    assert!(
        registry
            .schema(&ComponentTypeId::new("nara.ui.UiInteractionState"))
            .is_none()
    );
}

#[test]
fn legacy_root_and_defaulted_node_shapes_are_rejected() {
    let registry = frozen_ui_registry();

    let legacy_root = ComponentValue::map([
        (
            "target",
            ComponentValue::String("primary_window".to_string()),
        ),
        ("order", ComponentValue::I64(0)),
    ]);
    assert!(
        registry
            .preflight_component(&ComponentTypeId::new("nara.ui.UiRoot"), &legacy_root)
            .expect("UiRoot should have a registered codec")
            .is_err()
    );

    let defaulted_node = ComponentValue::map([(
        "style",
        ComponentValue::map([("visible", ComponentValue::Bool(true))]),
    )]);
    assert!(
        registry
            .preflight_component(&ComponentTypeId::new("nara.ui.UiNode"), &defaulted_node)
            .expect("UiNode should have a registered codec")
            .is_err()
    );
}

#[test]
fn button_visual_roles_round_trip_as_canonical_scene_data() {
    let registry = frozen_ui_registry();
    let component_id = ComponentTypeId::new("nara.ui.UiButtonVisuals");
    let expected = UiButtonVisuals::new()
        .with_normal_text(UiColorRole::SecondaryText)
        .with_hovered(UiColorRole::Warning, UiColorRole::Surface)
        .with_pressed(UiColorRole::Danger, UiColorRole::PrimaryText)
        .with_focused(UiColorRole::Focus, UiColorRole::Focus, 3.0)
        .expect("test focus style should be valid")
        .with_disabled(UiColorRole::Disabled, UiColorRole::SecondaryText);
    let mut source = World::new();
    let source_entity = source.spawn(expected).id();
    let encoded = registry
        .encode_component(&component_id, &source, source_entity)
        .expect("button visuals should have a registered codec")
        .expect("button visuals should encode")
        .expect("the source entity should contain button visuals");
    let prepared = registry
        .preflight_component(&component_id, &encoded)
        .expect("button visuals should have a registered codec")
        .expect("encoded button visuals should preflight");
    let mut target = World::new();
    let target_entity = target.spawn_empty().id();
    prepared
        .apply(&mut target, target_entity)
        .expect("prepared button visuals should apply");

    assert_eq!(
        target.get::<UiButtonVisuals>(target_entity),
        Some(&expected)
    );
}

#[test]
fn surface_mapping_invalidates_exact_routes_and_render_revisions_independently() {
    let descriptor = UiSurfaceDescriptor::new(
        surface_id(),
        InputWindowId::new(11),
        InputSeatId::PRIMARY,
        960.0,
        540.0,
        1.0,
        7,
        11,
        1,
    )
    .expect("test surface descriptor should be valid");

    let render_changed = descriptor
        .with_surface_mapping(2.0, 1.0)
        .expect("render mapping should update");
    assert_eq!(render_changed.route_revision(), 8);
    assert_eq!(render_changed.presentation_revision(), 12);
    assert_eq!(render_changed.scale_factor(), 2.0);
    assert_eq!(render_changed.input_scale_factor(), 1.0);

    let input_changed = render_changed
        .with_surface_mapping(2.0, 0.5)
        .expect("input-only mapping should update");
    assert_eq!(input_changed.route_revision(), 9);
    assert_eq!(input_changed.presentation_revision(), 12);
    assert_eq!(input_changed.input_scale_factor(), 0.5);
    assert_eq!(
        input_changed
            .with_surface_mapping(2.0, 0.5)
            .expect("an unchanged mapping should remain valid"),
        input_changed
    );
}

fn compiled_order(reverse_allocation: bool) -> (Vec<UiKey>, Vec<UiKey>, UiKey) {
    let mut app = ui_app();
    install_test_surface(
        app.world_mut()
            .expect("test App should allow world mutation"),
        1,
    );
    let world = app
        .world_mut()
        .expect("test App should allow world mutation");
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(TEST_DOMAIN, "root").expect("test key should be valid"),
        ))
        .id();
    let _unrelated = world.spawn_empty().id();
    let lower_bundle = button_bundle("lower", "lower", UiColor::rgba(1.0, 0.0, 0.0, 1.0));
    let upper_bundle = button_bundle("upper", "upper", UiColor::rgba(0.0, 1.0, 0.0, 1.0));
    let (lower, upper) = if reverse_allocation {
        let upper = world.spawn(upper_bundle).id();
        let lower = world.spawn(lower_bundle).id();
        (lower, upper)
    } else {
        let lower = world.spawn(lower_bundle).id();
        let upper = world.spawn(upper_bundle).id();
        (lower, upper)
    };
    place_ui_tree(world, root, &[lower, upper]);

    app.update().expect("UI test frame should update");
    let frame = latest_ui_frame(app.world(), surface_id())
        .expect("UI runtime should be available")
        .expect("UI frame should compile");
    let keyed_layouts = frame
        .layouts()
        .iter()
        .filter_map(|fact| fact.key().cloned())
        .collect::<Vec<_>>();
    let hit_order = frame
        .hit_regions()
        .iter()
        .map(|region| region.key().clone())
        .collect::<Vec<_>>();
    let hit = frame
        .hit_test(UiPoint::new(10.0, 10.0))
        .expect("overlapping buttons should hit")
        .key()
        .clone();
    (keyed_layouts, hit_order, hit)
}

#[test]
fn hierarchy_structural_order_drives_layout_paint_and_hit_independent_of_allocation() {
    let forward = compiled_order(false);
    let reverse = compiled_order(true);
    let lower = expected_runtime_node("root", "lower");
    let upper = expected_runtime_node("root", "upper");

    assert_eq!(forward, reverse);
    let lower_index = forward
        .0
        .iter()
        .position(|key| key == &lower)
        .expect("lower button should be laid out");
    let upper_index = forward
        .0
        .iter()
        .position(|key| key == &upper)
        .expect("upper button should be laid out");
    assert!(lower_index < upper_index);
    assert_eq!(forward.1, [upper.clone(), lower]);
    assert_eq!(forward.2, upper);
}

fn build_button_app() -> (App, Entity) {
    let mut app = ui_app();
    install_test_surface(
        app.world_mut()
            .expect("test App should allow world mutation"),
        1,
    );
    let world = app
        .world_mut()
        .expect("test App should allow world mutation");
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(TEST_DOMAIN, "root").expect("test key should be valid"),
        ))
        .id();
    let button = world
        .spawn(button_bundle(
            "retry",
            "retry-draft",
            UiColor::rgba(0.2, 0.4, 0.9, 1.0),
        ))
        .id();
    place_ui_tree(world, root, &[button]);
    app.update().expect("UI test frame should update");
    (app, button)
}

#[test]
fn presented_button_dispatch_publishes_one_typed_action_and_semantics() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id())
        .expect("UI runtime should be available")
        .expect("UI frame should compile");
    let route = UiRouteStamp::new(surface_id(), 1);
    let pointer = UiPointerId::PRIMARY;
    let seat = UiSeatId::PRIMARY;
    let point = UiPoint::new(10.0, 10.0);

    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            route,
            UiInputEvent::PointerMoved {
                seat,
                pointer,
                position: point,
            },
        ),
        Err(UiAdapterError::Dispatch(_))
    ));

    let receipt = commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        Arc::clone(&frame),
        1,
    )
    .expect("exact latest frame should commit");
    assert_eq!(receipt.surface_id(), surface_id());
    assert_eq!(receipt.frame_revision(), frame.revision());

    for input in [
        UiInputEvent::PointerMoved {
            seat,
            pointer,
            position: point,
        },
        UiInputEvent::PointerButton {
            seat,
            pointer,
            button: UiPointerButton::Primary,
            phase: UiButtonPhase::Pressed,
            position: Some(point),
        },
        UiInputEvent::PointerButton {
            seat,
            pointer,
            button: UiPointerButton::Primary,
            phase: UiButtonPhase::Released,
            position: Some(point),
        },
    ] {
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            route,
            input,
        )
        .expect("presented UI input should dispatch");
    }

    let events = app.world().resource::<UiActionEvents>();
    assert_eq!(events.as_slice().len(), 1);
    let event = &events.as_slice()[0];
    assert_eq!(event.target(), &expected_runtime_node("root", "retry"));
    assert_eq!(event.action().as_str(), "retry-draft");
    assert_eq!(event.source(), UiActivationSource::Pointer);

    let semantics = ui_semantic_snapshot(app.world(), surface_id(), 1, seat)
        .expect("presented frame should publish semantics");
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == event.target())
        .expect("Retry semantics should share the action key");
    assert_eq!(retry.name(), "retry");
    assert!(retry.state().enabled());
}

#[test]
fn stale_frames_and_retired_surface_identities_fail_closed() {
    let (mut app, button) = build_button_app();
    let first = latest_ui_frame(app.world(), surface_id())
        .expect("UI runtime should be available")
        .expect("first UI frame should compile");

    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(button)
        .insert(UiNode::new(UiStyle::absolute(10.0, 5.0, 120.0, 60.0)));
    app.update().expect("second UI test frame should update");
    let second = latest_ui_frame(app.world(), surface_id())
        .expect("UI runtime should be available")
        .expect("second UI frame should compile");
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(matches!(
        commit_ui_presentation(
            app.world_mut()
                .expect("test App should allow world mutation"),
            first,
            1,
        ),
        Err(UiAdapterError::FrameNotLatest { surface: 7 })
    ));
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        second,
        1,
    )
    .expect("latest frame should commit");

    assert!(
        retire_ui_surface(
            app.world_mut()
                .expect("test App should allow world mutation"),
            surface_id(),
        )
        .expect("surface retirement should succeed")
    );
    let descriptor = UiSurfaceDescriptor::new(
        surface_id(),
        InputWindowId::new(11),
        InputSeatId::PRIMARY,
        200.0,
        100.0,
        1.0,
        2,
        2,
        1,
    )
    .expect("replacement descriptor should be valid");
    assert!(matches!(
        install_ui_surface(
            app.world_mut()
                .expect("test App should allow world mutation"),
            descriptor,
        ),
        Err(UiAdapterError::RetiredSurface { surface: 7 })
    ));
}

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).expect("test scene entity ID should be valid")
}

fn prefab_anchor(id: &str, source: AssetRef) -> SceneEntityRecord {
    SceneEntityRecord {
        id: scene_id(id),
        children: Vec::new(),
        components: BTreeMap::new(),
        prefab: Some(PrefabInstance {
            source,
            overrides: ScenePatchDocument::default(),
        }),
    }
}

#[test]
fn repeated_prefab_roots_have_distinct_stable_keys_from_structured_provenance() {
    let mut app = ui_app();
    install_test_surface(
        app.world_mut()
            .expect("test App should allow world mutation"),
        1,
    );
    let mut registry = ComponentRegistry::new();
    registry
        .freeze()
        .expect("empty scene registry should freeze");
    let source = AssetRef::path("prefabs/ui-root.ron").expect("test asset ref should be valid");
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("root"))]),
    );
    let document = SceneDocument::from_ordered_roots([
        prefab_anchor("left", source.clone()),
        prefab_anchor("right", source),
    ]);
    let report = spawn_scene_with_prefab_resolver(
        app.world_mut()
            .expect("test App should allow world mutation"),
        &registry,
        &document,
        &resolver,
    );
    assert!(!report.diagnostics.has_errors());

    let projected = app
        .world()
        .iter_entities()
        .filter_map(|entity| {
            entity
                .get::<SceneEntitySource>()
                .filter(|source| source.source_entity_id.as_str() == "root")
                .map(|source| (entity.id(), source.clone()))
        })
        .collect::<Vec<_>>();
    assert_eq!(projected.len(), 2);
    assert!(
        projected
            .iter()
            .all(|(_, source)| source.projection_path.is_some())
    );
    for (entity, _) in &projected {
        app.world_mut()
            .expect("test App should allow world mutation")
            .entity_mut(*entity)
            .insert((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())));
    }

    app.update().expect("prefab UI test frame should update");
    let frame = latest_ui_frame(app.world(), surface_id())
        .expect("UI runtime should be available")
        .expect("prefab UI frame should compile");
    let keys = frame
        .root_outcomes()
        .iter()
        .map(|outcome| outcome.key().clone())
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 2);
    assert_ne!(keys[0], keys[1]);

    app.update().expect("prefab UI rebuild should update");
    let rebuilt = latest_ui_frame(app.world(), surface_id())
        .expect("UI runtime should be available")
        .expect("rebuilt prefab UI frame should compile");
    assert_eq!(
        rebuilt
            .root_outcomes()
            .iter()
            .map(|outcome| outcome.key().clone())
            .collect::<Vec<_>>(),
        keys
    );
}
