use std::{collections::BTreeMap, sync::Arc, time::Duration};

use nara_app::{
    App, CoreStage, RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeControl,
    RuntimeControlRequestResult, RuntimeInstance, RuntimeObligationLedger, RuntimeState,
};
use nara_asset::AssetRef;
use nara_core::Vec2;
use nara_ecs::{Entity, ResMut, Resource, World};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_input::{
    ButtonDriverInput, InputPointerId, InputSeatId, InputTimeline, InputWindowEvent, InputWindowId,
    MouseButton, PointerDriverInput, apply_mouse_driver_input_at, apply_pointer_driver_input_at,
};
use nara_reflect::{
    ComponentRegistry, ComponentRegistryPlugin, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, SceneDocument, SceneEntityId,
    SceneEntityRecord, SceneEntitySource, ScenePatchDocument, spawn_scene_with_prefab_resolver,
};
use nara_ui_kernel::{
    UiActivationSource, UiButtonPhase, UiInputEvent, UiKey, UiLayer, UiLocalKey, UiNonActivation,
    UiPaintPrimitive, UiPoint, UiPointerButton, UiPointerId, UiResolvedBrush, UiRootKey,
    UiRootStatus, UiRouteStamp, UiSeatId, UiSurfaceId, UiTheme,
};

use crate::interaction::dispatch_presented_ui_input;
use crate::{
    UiActionAdmissionError, UiActionConsumerId, UiActionCursor, UiActionId, UiActionJournal,
    UiAdapterError, UiButtonVisuals, UiColor, UiColorRole, UiNode, UiPlugin, UiRoot,
    UiRouteDisposition, UiRouteRejection, UiRouteRejectionLog, UiRuntimeKey, UiStyle,
    UiSurfaceDescriptor, UiVisual, active_ui_surface_descriptors, commit_ui_presentation,
    install_ui_surface, latest_ui_frame, register_ui_components, retire_ui_surface,
    ui_semantic_snapshot, update_ui_surface_input_endpoint, update_ui_surface_logical_size,
    update_ui_surface_mapping,
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

fn start_runtime(app: App) -> RuntimeInstance {
    RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote()
}

fn stop_runtime(runtime: &mut RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    if runtime.state() == RuntimeState::Stopping {
        runtime.drive(Duration::ZERO).unwrap();
    }
}

fn surface_id() -> UiSurfaceId {
    UiSurfaceId::new(7).expect("test surface ID should be non-zero")
}

fn install_test_surface(world: &mut World) {
    install_ui_surface(
        world,
        UiSurfaceDescriptor::new(
            surface_id(),
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            200.0,
            100.0,
            1.0,
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

fn place_invalid_button_modal(world: &mut World, root_local: &str, button_local: &str) {
    let modal = world
        .spawn((
            UiRoot::new(UiLayer::Modal),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(TEST_DOMAIN, root_local).unwrap(),
        ))
        .id();
    let (button, node, _visual, key) = button_bundle(
        button_local,
        "invalid-modal-action",
        UiColor::rgba(0.8, 0.2, 0.2, 1.0),
    );
    let invalid_child = world.spawn((button, node, key)).id();
    place_ui_tree(world, modal, &[invalid_child]);
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
    let mut app = ui_app();
    let descriptor = UiSurfaceDescriptor::new(
        surface_id(),
        InputWindowId::new(11),
        InputSeatId::PRIMARY,
        960.0,
        540.0,
        1.0,
    )
    .expect("test surface descriptor should be valid");
    let world = app
        .world_mut()
        .expect("test App should allow world mutation");
    install_ui_surface(world, descriptor).unwrap();

    let render_changed = update_ui_surface_mapping(world, surface_id(), 2.0, 1.0)
        .expect("render mapping should update");
    assert_eq!(render_changed.route_revision(), 2);
    assert_eq!(render_changed.presentation_revision(), 2);
    assert_eq!(render_changed.scale_factor(), 2.0);
    assert_eq!(render_changed.input_scale_factor(), 1.0);

    let input_changed = update_ui_surface_mapping(world, surface_id(), 2.0, 0.5)
        .expect("input-only mapping should update");
    assert_eq!(input_changed.route_revision(), 3);
    assert_eq!(input_changed.presentation_revision(), 2);
    assert_eq!(input_changed.input_scale_factor(), 0.5);
    assert_eq!(
        update_ui_surface_mapping(world, surface_id(), 2.0, 0.5)
            .expect("an unchanged mapping should remain valid"),
        input_changed
    );
}

#[test]
fn surface_size_and_endpoint_updates_own_exact_invalidation() {
    let mut app = ui_app();
    let descriptor = UiSurfaceDescriptor::new(
        surface_id(),
        InputWindowId::new(11),
        InputSeatId::PRIMARY,
        960.0,
        540.0,
        1.0,
    )
    .expect("test surface descriptor should be valid");
    let world = app
        .world_mut()
        .expect("test App should allow world mutation");
    install_ui_surface(world, descriptor).unwrap();

    let resized = update_ui_surface_logical_size(world, surface_id(), 1_280.0, 720.0)
        .expect("logical size should update");
    assert_eq!(resized.logical_size(), (1_280.0, 720.0));
    assert_eq!(resized.route_revision(), 2);
    assert_eq!(resized.presentation_revision(), 2);

    let moved = update_ui_surface_input_endpoint(
        world,
        surface_id(),
        InputWindowId::new(12),
        InputSeatId::PRIMARY,
    )
    .expect("input endpoint should update");
    assert_eq!(moved.input_window(), InputWindowId::new(12));
    assert_eq!(moved.route_revision(), 3);
    assert_eq!(moved.presentation_revision(), 2);
    assert_eq!(
        update_ui_surface_logical_size(world, surface_id(), 1_280.0, 720.0)
            .expect("an unchanged logical size should remain valid"),
        moved
    );
}

#[test]
fn descriptor_snapshots_cannot_bypass_runtime_owned_revisions() {
    let mut app = ui_app();
    let descriptor = UiSurfaceDescriptor::new(
        surface_id(),
        InputWindowId::new(11),
        InputSeatId::PRIMARY,
        960.0,
        540.0,
        1.0,
    )
    .unwrap();
    let stale_snapshot = descriptor
        .with_surface_mapping(1.5, 1.0)
        .unwrap()
        .with_surface_mapping(2.0, 0.5)
        .unwrap();
    let world = app.world_mut().unwrap();
    install_ui_surface(world, descriptor).unwrap();
    let resized = update_ui_surface_logical_size(world, surface_id(), 1_280.0, 720.0).unwrap();
    let current = update_ui_surface_mapping(world, surface_id(), 2.0, 1.0).unwrap();

    assert_eq!(stale_snapshot.route_revision(), 1);
    assert_eq!(stale_snapshot.presentation_revision(), 1);
    assert_eq!(resized.route_revision(), 2);
    assert_eq!(current.route_revision(), 3);
    assert_eq!(current.presentation_revision(), 3);
    assert!(matches!(
        install_ui_surface(world, stale_snapshot),
        Err(UiAdapterError::SurfaceAlreadyInstalled { surface: 7 })
    ));
    assert_eq!(active_ui_surface_descriptors(world).unwrap(), [current]);
}

#[test]
fn duplicate_input_endpoint_is_rejected_before_any_presentation() {
    let mut app = ui_app();
    install_test_surface(
        app.world_mut()
            .expect("test App should allow world mutation"),
    );
    let conflicting_surface = UiSurfaceId::new(8).unwrap();
    let conflicting = UiSurfaceDescriptor::new(
        conflicting_surface,
        InputWindowId::new(11),
        InputSeatId::PRIMARY,
        200.0,
        100.0,
        1.0,
    )
    .unwrap();

    assert!(matches!(
        install_ui_surface(
            app.world_mut()
                .expect("test App should allow world mutation"),
            conflicting,
        ),
        Err(UiAdapterError::InputEndpointConflict {
            window: 11,
            seat: 0,
            surface: 7,
        })
    ));
}

fn compiled_order(reverse_allocation: bool) -> (Vec<UiKey>, Vec<UiKey>, UiKey) {
    let mut app = ui_app();
    install_test_surface(
        app.world_mut()
            .expect("test App should allow world mutation"),
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

fn build_unstarted_button_app() -> (App, Entity) {
    build_unstarted_button_app_with_layer(UiLayer::Base)
}

fn build_unstarted_button_app_with_layer(layer: UiLayer) -> (App, Entity) {
    let mut app = ui_app();
    install_test_surface(
        app.world_mut()
            .expect("test App should allow world mutation"),
    );
    let world = app
        .world_mut()
        .expect("test App should allow world mutation");
    let root = world
        .spawn((
            UiRoot::new(layer),
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
    (app, button)
}

fn build_button_app() -> (App, Entity) {
    let (mut app, button) = build_unstarted_button_app();
    app.update().expect("UI test frame should update");
    (app, button)
}

#[test]
fn focused_disabled_button_keeps_a_non_color_focus_indicator() {
    let (mut app, button) = build_button_app();
    let initial = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("initial frame should compile");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        initial,
        1,
    )
    .unwrap();
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 1),
            UiInputEvent::Navigate {
                seat: UiSeatId::PRIMARY,
                direction: nara_ui_kernel::UiNavigation::Next,
            },
        ),
        UiRouteDisposition::Consumed(_)
    ));

    let disabled = app
        .world()
        .get::<crate::UiButton>(button)
        .expect("test button should exist")
        .clone()
        .with_enabled(false);
    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(button)
        .insert(disabled);
    app.update().expect("disabled frame should compile");
    let disabled_frame = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("disabled frame should exist");
    let key = expected_runtime_node("root", "retry");
    let palette = UiTheme::default().palette();
    let (background, border) = disabled_frame
        .paint()
        .iter()
        .find_map(|primitive| match primitive {
            UiPaintPrimitive::Rect {
                key: Some(actual),
                brush: Some(background),
                border: Some(border),
                ..
            } if actual == &key => Some((background, *border)),
            _ => None,
        })
        .expect("focused disabled button should retain its focus border");
    assert_eq!(background, &UiResolvedBrush::Solid(palette.disabled));
    assert_eq!(border.width(), 2.0);
    assert_eq!(border.color(), palette.focus);
    assert!(disabled_frame.paint().iter().any(|primitive| {
        matches!(
            primitive,
            UiPaintPrimitive::Text {
                key: Some(actual),
                color,
                ..
            } if actual == &key && *color == palette.secondary_text
        )
    }));

    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        disabled_frame,
        2,
    )
    .unwrap();
    let semantics = ui_semantic_snapshot(app.world(), surface_id(), 2, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &key)
        .expect("disabled button should retain semantics");
    assert!(retry.state().focused());
    assert!(!retry.state().enabled());
}

#[test]
fn invalid_controlled_child_rejects_its_complete_root_instead_of_publishing_a_partial_tree() {
    let (mut app, invalid_button) = build_unstarted_button_app();
    let root = app
        .world()
        .iter_entities()
        .find(|entity| entity.contains::<UiRoot>())
        .expect("test UI root should exist")
        .id();
    let valid_button = app
        .world_mut()
        .expect("test App should allow world mutation")
        .spawn(button_bundle(
            "secondary",
            "secondary-action",
            UiColor::rgba(0.3, 0.7, 0.4, 1.0),
        ))
        .id();
    HierarchyWriter::new(
        app.world_mut()
            .expect("test App should allow world mutation"),
    )
    .place(valid_button, child_last(root))
    .unwrap();
    app.update().expect("valid UI root should compile");
    let initial = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("valid UI root should publish");
    assert_eq!(initial.semantics().len(), 2);

    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(invalid_button)
        .remove::<UiVisual>();
    app.update()
        .expect("invalid root should fail closed without aborting the frame");
    let rejected = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("root rejection should still publish a bounded frame");

    assert!(rejected.semantics().is_empty());
    assert!(
        rejected
            .root_outcomes()
            .iter()
            .all(|outcome| outcome.status() != UiRootStatus::Published)
    );
}

#[test]
fn rejected_projected_root_keeps_its_envelope_without_suppressing_a_valid_sibling() {
    let mut app = ui_app();
    install_test_surface(
        app.world_mut()
            .expect("test App should allow world mutation"),
    );
    let world = app
        .world_mut()
        .expect("test App should allow world mutation");
    let rejected_root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(TEST_DOMAIN, "rejected").unwrap(),
        ))
        .id();
    let (button, node, _visual, key) = button_bundle(
        "missing-visual",
        "missing-visual-action",
        UiColor::rgba(0.8, 0.2, 0.2, 1.0),
    );
    let rejected_child = world.spawn((button, node, key)).id();
    let published_root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiVisual::solid(UiColor::rgba(0.2, 0.7, 0.3, 1.0)),
            UiRuntimeKey::new(TEST_DOMAIN, "published").unwrap(),
        ))
        .id();
    place_ui_tree(world, rejected_root, &[rejected_child]);
    place_ui_tree(world, published_root, &[]);

    app.update()
        .expect("a rejected root should not abort a valid sibling");
    let frame = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("the mixed candidate should publish");
    let rejected = frame
        .root_outcomes()
        .iter()
        .find(|outcome| outcome.key() == &expected_runtime_root("rejected"))
        .unwrap();
    let published = frame
        .root_outcomes()
        .iter()
        .find(|outcome| outcome.key() == &expected_runtime_root("published"))
        .unwrap();

    assert_eq!(rejected.status(), UiRootStatus::Absent);
    assert_eq!(published.status(), UiRootStatus::Published);
    assert_eq!(frame.paint().len(), 1);
}

#[test]
fn retained_modal_projection_is_inert_but_keeps_the_current_barrier() {
    let (mut app, button) = build_unstarted_button_app_with_layer(UiLayer::Modal);
    app.update().expect("modal baseline should compile");
    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(button)
        .remove::<UiVisual>();
    app.update()
        .expect("a compatible modal visual should be retained");
    let retained = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("retained modal candidate should publish");

    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert!(nara_ui_kernel::__private::frame_blocks_unhandled_input(
        &retained
    ));
    assert!(retained.hit_regions().is_empty());
    assert!(retained.focus_order().is_empty());
    assert!(retained.semantics().is_empty());

    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        retained,
        1,
    )
    .unwrap();
    app.world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<InputTimeline>()
        .admit_wheel(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            Vec2::new(0.0, 1.0),
        )
        .unwrap();
    app.update()
        .expect("retained modal wheel routing should remain fail-closed");
    let rejection = app
        .world()
        .resource::<UiRouteRejectionLog>()
        .as_slice()
        .first()
        .expect("retained modal should block unhandled wheel input");
    assert_eq!(
        rejection.rejection(),
        &UiRouteRejection::UnhandledInputBlocked
    );
}

#[test]
fn rejected_modal_without_fallback_discards_the_unpresented_candidate() {
    let (mut app, _) = build_button_app();
    let stale = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("baseline candidate should exist");
    place_invalid_button_modal(
        app.world_mut()
            .expect("test App should allow world mutation"),
        "modal-without-fallback",
        "invalid-modal-button",
    );

    app.update()
        .expect("modal candidate rejection should remain adapter-local");
    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        commit_ui_presentation(
            app.world_mut()
                .expect("test App should allow world mutation"),
            stale,
            1,
        ),
        Err(UiAdapterError::FrameNotLatest { surface: 7 })
    ));
}

#[test]
fn missing_root_identity_rejects_the_whole_projection() {
    let (mut app, _) = build_button_app();
    let stale = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("baseline candidate should exist");
    let missing_identity = app
        .world_mut()
        .expect("test App should allow world mutation")
        .spawn((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())))
        .id();
    HierarchyWriter::new(
        app.world_mut()
            .expect("test App should allow world mutation"),
    )
    .place(missing_identity, root_last())
    .unwrap();

    app.update()
        .expect("identity rejection should remain adapter-local");
    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        commit_ui_presentation(
            app.world_mut()
                .expect("test App should allow world mutation"),
            stale,
            1,
        ),
        Err(UiAdapterError::FrameNotLatest { surface: 7 })
    ));
}

#[test]
fn whole_candidate_rejection_discards_new_work_but_preserves_presented_authority() {
    let (mut app, button) = build_button_app();
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("candidate-rejection-test").unwrap())
        .unwrap();
    let presented = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("baseline candidate should exist");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        presented,
        1,
    )
    .unwrap();

    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(button)
        .insert(
            crate::UiButton::new(
                "retry",
                UiActionId::new("unpresented-replacement").unwrap(),
                16.0,
            )
            .unwrap(),
        );
    app.update()
        .expect("replacement candidate should compile without being presented");
    let unpresented = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("replacement candidate should exist");
    place_invalid_button_modal(
        app.world_mut()
            .expect("test App should allow world mutation"),
        "later-invalid-modal",
        "later-invalid-button",
    );

    app.update()
        .expect("whole-candidate rejection should preserve the presented session");
    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        commit_ui_presentation(
            app.world_mut()
                .expect("test App should allow world mutation"),
            unpresented,
            2,
        ),
        Err(UiAdapterError::FrameNotLatest { surface: 7 })
    ));

    assert!(
        dispatch_pointer_click(&mut app, UiRouteStamp::new(surface_id(), 1))
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );
    let pending = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].action().as_str(), "retry-draft");
    assert_eq!(pending[0].presentation_epoch(), 1);
}

fn dispatch_pointer_click(app: &mut App, route: UiRouteStamp) -> Vec<UiRouteDisposition> {
    let seat = UiSeatId::PRIMARY;
    let pointer = UiPointerId::PRIMARY;
    let point = UiPoint::new(10.0, 10.0);
    [
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
    ]
    .into_iter()
    .map(|input| {
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            route,
            input,
        )
    })
    .collect()
}

#[test]
fn presented_button_dispatch_publishes_one_typed_action_and_semantics() {
    let (mut app, _button) = build_button_app();
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("adapter-test").unwrap())
        .expect("test action consumer should register");
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
        UiRouteDisposition::RejectedClosed(UiRouteRejection::PresentedHistory(_))
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
        assert!(matches!(
            dispatch_presented_ui_input(
                app.world_mut()
                    .expect("test App should allow world mutation"),
                route,
                input,
            ),
            UiRouteDisposition::Consumed(_)
        ));
    }

    let (event_target, event_sequence) = {
        let events = app
            .world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .expect("test action cursor should remain registered");
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.input_sequence(), None);
        assert_eq!(event.surface_id(), surface_id());
        assert_eq!(event.presentation_epoch(), 1);
        assert_eq!(event.frame_revision(), frame.revision().get());
        assert_eq!(event.target(), &expected_runtime_node("root", "retry"));
        assert_eq!(event.action().as_str(), "retry-draft");
        assert_eq!(event.source(), UiActivationSource::Pointer);
        (event.target().clone(), event.sequence())
    };

    let semantics = ui_semantic_snapshot(app.world(), surface_id(), 1, seat)
        .expect("presented frame should publish semantics");
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &event_target)
        .expect("Retry semantics should share the action key");
    assert_eq!(retry.name(), "retry");
    assert!(retry.state().enabled());

    app.world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .acknowledge(&cursor, event_sequence)
        .expect("test action should acknowledge");
}

#[test]
fn resize_while_pressed_cancels_without_activating_the_button() {
    let (mut app, _) = build_button_app();
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("resize-cancellation-test").unwrap())
        .unwrap();
    let frame = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("test frame should compile");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        frame,
        1,
    )
    .unwrap();

    {
        let mut timeline = app
            .world_mut()
            .expect("test App should allow world mutation")
            .resource_mut::<InputTimeline>();
        timeline
            .admit_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(10.0, 10.0)),
            )
            .unwrap();
        timeline
            .admit_mouse_button(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
    }
    app.update().unwrap();
    let pressed = ui_semantic_snapshot(app.world(), surface_id(), 1, UiSeatId::PRIMARY).unwrap();
    assert!(
        pressed
            .nodes()
            .iter()
            .find(|node| node.key() == &expected_runtime_node("root", "retry"))
            .unwrap()
            .state()
            .pressed()
    );

    let admission = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<InputTimeline>()
        .admit_window_event(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputWindowEvent::Resized {
                logical_size: Vec2::new(640.0, 360.0),
            },
        )
        .unwrap();
    assert_eq!(admission.admitted(), 3);
    app.update().unwrap();

    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
    let cancelled = ui_semantic_snapshot(app.world(), surface_id(), 1, UiSeatId::PRIMARY).unwrap();
    assert!(
        !cancelled
            .nodes()
            .iter()
            .find(|node| node.key() == &expected_runtime_node("root", "retry"))
            .unwrap()
            .state()
            .pressed()
    );
}

#[test]
fn unpresented_candidate_cannot_replace_presented_behavior_authority() {
    let (mut app, button) = build_button_app();
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("presented-authority-test").unwrap())
        .unwrap();
    let first = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("first candidate should compile");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        Arc::clone(&first),
        1,
    )
    .unwrap();

    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(button)
        .insert(
            crate::UiButton::new(
                "retry",
                UiActionId::new("replacement-action").unwrap(),
                16.0,
            )
            .unwrap(),
        );
    app.update().expect("replacement candidate should compile");
    let second = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("replacement candidate should exist");
    assert_ne!(first.revision(), second.revision());

    assert!(
        dispatch_pointer_click(&mut app, UiRouteStamp::new(surface_id(), 1))
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );
    let pending = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].action().as_str(), "retry-draft");
    assert_eq!(pending[0].presentation_epoch(), 1);
    assert_eq!(pending[0].frame_revision(), first.revision().get());
    let first_sequence = pending[0].sequence();
    app.world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .acknowledge(&cursor, first_sequence)
        .unwrap();

    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none(),
        "interaction committed after candidate compilation must invalidate that stale candidate"
    );
    app.update()
        .expect("replacement behavior should recompile from the committed interaction state");
    let second = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("replacement candidate should be rebuilt");

    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        Arc::clone(&second),
        2,
    )
    .unwrap();
    assert!(
        dispatch_pointer_click(&mut app, UiRouteStamp::new(surface_id(), 1))
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );
    let retained = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].action().as_str(), "retry-draft");
    assert_eq!(retained[0].presentation_epoch(), 1);
    assert_eq!(retained[0].frame_revision(), first.revision().get());
}

#[test]
fn equivalent_presentation_preserves_one_inflight_click() {
    let (mut app, _button) = build_button_app();
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("equivalent-presentation-test").unwrap())
        .unwrap();
    let initial = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("initial frame should compile");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        initial,
        10,
    )
    .unwrap();
    let point = UiPoint::new(10.0, 10.0);
    for input in [
        UiInputEvent::PointerMoved {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            position: point,
        },
        UiInputEvent::PointerButton {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            button: UiPointerButton::Primary,
            phase: UiButtonPhase::Pressed,
            position: Some(point),
        },
    ] {
        assert!(matches!(
            dispatch_presented_ui_input(
                app.world_mut()
                    .expect("test App should allow world mutation"),
                UiRouteStamp::new(surface_id(), 10),
                input,
            ),
            UiRouteDisposition::Consumed(_)
        ));
    }

    app.update()
        .expect("equivalent pressed-state frame should compile");
    let equivalent = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("equivalent frame should exist");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        equivalent,
        11,
    )
    .unwrap();
    let semantics = ui_semantic_snapshot(app.world(), surface_id(), 11, UiSeatId::PRIMARY).unwrap();
    assert!(
        semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &expected_runtime_node("root", "retry"))
            .unwrap()
            .state()
            .pressed()
    );

    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 11),
            UiInputEvent::PointerButton {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Released,
                position: Some(point),
            },
        ),
        UiRouteDisposition::Consumed(_)
    ));
    assert_eq!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 11),
            UiInputEvent::PointerButton {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Released,
                position: Some(point),
            },
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::NonActivation(
            UiNonActivation::NoPointerCapture
        ))
    ));
    let actions = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action().as_str(), "retry-draft");
}

#[test]
fn label_only_successor_with_stable_geometry_preserves_the_inflight_click() {
    let (mut app, button) = build_button_app();
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("label-successor-test").unwrap())
        .unwrap();
    let initial = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        initial,
        1,
    )
    .unwrap();
    let point = UiPoint::new(10.0, 10.0);
    for input in [
        UiInputEvent::PointerMoved {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            position: point,
        },
        UiInputEvent::PointerButton {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            button: UiPointerButton::Primary,
            phase: UiButtonPhase::Pressed,
            position: Some(point),
        },
    ] {
        assert!(matches!(
            dispatch_presented_ui_input(
                app.world_mut()
                    .expect("test App should allow world mutation"),
                UiRouteStamp::new(surface_id(), 1),
                input,
            ),
            UiRouteDisposition::Consumed(_)
        ));
    }

    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(button)
        .insert(
            crate::UiButton::new("again", UiActionId::new("retry-draft").unwrap(), 16.0).unwrap(),
        );
    app.update().expect("label successor should compile");
    let successor = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        successor,
        2,
    )
    .unwrap();
    assert!(
        ui_semantic_snapshot(app.world(), surface_id(), 2, UiSeatId::PRIMARY)
            .unwrap()
            .nodes()
            .iter()
            .find(|node| node.key() == &expected_runtime_node("root", "retry"))
            .unwrap()
            .state()
            .pressed()
    );
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 2),
            UiInputEvent::PointerButton {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Released,
                position: Some(point),
            },
        ),
        UiRouteDisposition::Consumed(_)
    ));
    let actions = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action().as_str(), "retry-draft");
}

#[test]
fn changed_behavior_requires_a_fresh_press_before_the_new_action_can_activate() {
    let (mut app, button) = build_button_app();
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("changed-behavior-test").unwrap())
        .unwrap();
    let first = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("initial frame should compile");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        first,
        1,
    )
    .unwrap();
    let point = UiPoint::new(10.0, 10.0);
    for input in [
        UiInputEvent::PointerMoved {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            position: point,
        },
        UiInputEvent::PointerButton {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            button: UiPointerButton::Primary,
            phase: UiButtonPhase::Pressed,
            position: Some(point),
        },
    ] {
        assert!(matches!(
            dispatch_presented_ui_input(
                app.world_mut()
                    .expect("test App should allow world mutation"),
                UiRouteStamp::new(surface_id(), 1),
                input,
            ),
            UiRouteDisposition::Consumed(_)
        ));
    }

    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(button)
        .insert(
            crate::UiButton::new(
                "retry",
                UiActionId::new("replacement-action").unwrap(),
                16.0,
            )
            .unwrap(),
        );
    app.update().expect("changed behavior should compile");
    let changed = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("changed behavior frame should exist");
    let key = expected_runtime_node("root", "retry");
    let pressed_color = UiTheme::default().palette().positive;
    assert!(
        changed.paint().iter().all(|primitive| {
            !matches!(
                primitive,
                UiPaintPrimitive::Rect {
                    key: Some(actual),
                    brush: Some(UiResolvedBrush::Solid(color)),
                    ..
                } if actual == &key && *color == pressed_color
            )
        }),
        "candidate paint must reflect staged pointer cancellation before backend acceptance"
    );
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        changed,
        2,
    )
    .unwrap();

    let semantics = ui_semantic_snapshot(app.world(), surface_id(), 2, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &key)
        .unwrap();
    assert!(retry.state().focused());
    assert!(!retry.state().pressed());
    let reducer = app
        .world()
        .resource::<crate::runtime::UiRuntimeState>()
        .surface(surface_id())
        .unwrap()
        .reducer_states()
        .get(&expected_runtime_node("root", "retry"))
        .copied()
        .unwrap();
    assert!(!reducer.hovered());
    assert!(!reducer.pressed());
    assert!(reducer.focused());
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 2),
            UiInputEvent::PointerButton {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Released,
                position: Some(point),
            },
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::NonActivation(
            UiNonActivation::NoPointerCapture
        ))
    ));
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 1),
            UiInputEvent::PointerButton {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Pressed,
                position: Some(point),
            },
        ),
        UiRouteDisposition::Consumed(_)
    ));
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 2),
            UiInputEvent::PointerButton {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Released,
                position: Some(point),
            },
        ),
        UiRouteDisposition::Consumed(_)
    ));
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );

    assert!(
        dispatch_pointer_click(&mut app, UiRouteStamp::new(surface_id(), 2))
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );
    let actions = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action().as_str(), "replacement-action");
}

#[test]
fn presentation_retirement_reconciles_before_historical_route_rejection() {
    let (mut app, button) = build_button_app();
    let root = app
        .world()
        .iter_entities()
        .find(|entity| entity.contains::<UiRoot>())
        .expect("test UI root should exist")
        .id();
    let first = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("first frame should compile");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        Arc::clone(&first),
        1,
    )
    .unwrap();
    let point = UiPoint::new(10.0, 10.0);
    for input in [
        UiInputEvent::PointerMoved {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            position: point,
        },
        UiInputEvent::PointerButton {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            button: UiPointerButton::Primary,
            phase: UiButtonPhase::Pressed,
            position: Some(point),
        },
    ] {
        assert!(matches!(
            dispatch_presented_ui_input(
                app.world_mut()
                    .expect("test App should allow world mutation"),
                UiRouteStamp::new(surface_id(), 1),
                input,
            ),
            UiRouteDisposition::Consumed(_)
        ));
    }

    let previous = HierarchyPlacement::current(app.world(), button).unwrap();
    HierarchyWriter::new(
        app.world_mut()
            .expect("test App should allow world mutation"),
    )
    .remove(button, previous)
    .unwrap();
    app.update().expect("retired-button frame should compile");
    let second = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("retired-button frame should exist");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        second,
        2,
    )
    .unwrap();

    let retired_move = UiInputEvent::PointerMoved {
        seat: UiSeatId::PRIMARY,
        pointer: UiPointerId::PRIMARY,
        position: point,
    };
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 1),
            retired_move.clone(),
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::NonActivation(
            UiNonActivation::TargetRetired { .. }
        ))
    ));
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 1),
            retired_move,
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::NonActivation(
            UiNonActivation::TargetRetired { .. }
        ))
    ));

    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(button)
        .insert((
            UiRuntimeKey::new(TEST_DOMAIN, "replacement").unwrap(),
            crate::UiButton::new(
                "replacement",
                UiActionId::new("replacement-action").unwrap(),
                16.0,
            )
            .unwrap(),
        ));
    HierarchyWriter::new(
        app.world_mut()
            .expect("test App should allow world mutation"),
    )
    .place(button, child_last(root))
    .unwrap();
    app.update().expect("replacement frame should compile");
    let third = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("replacement frame should exist");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        third,
        3,
    )
    .unwrap();

    assert!(
        dispatch_pointer_click(&mut app, UiRouteStamp::new(surface_id(), 3))
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );
}

#[test]
fn presentation_removal_cannot_resurrect_interaction_for_the_same_stable_key() {
    let (mut app, button) = build_button_app();
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("presentation-reconciliation-test").unwrap())
        .unwrap();
    let root = app
        .world()
        .iter_entities()
        .find(|entity| entity.contains::<UiRoot>())
        .expect("test UI root should exist")
        .id();
    let first = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("initial frame should compile");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        first,
        1,
    )
    .unwrap();

    let point = UiPoint::new(10.0, 10.0);
    for input in [
        UiInputEvent::PointerMoved {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            position: point,
        },
        UiInputEvent::PointerButton {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            button: UiPointerButton::Primary,
            phase: UiButtonPhase::Pressed,
            position: Some(point),
        },
    ] {
        assert!(matches!(
            dispatch_presented_ui_input(
                app.world_mut()
                    .expect("test App should allow world mutation"),
                UiRouteStamp::new(surface_id(), 1),
                input,
            ),
            UiRouteDisposition::Consumed(_)
        ));
    }

    let previous = HierarchyPlacement::current(app.world(), button).unwrap();
    HierarchyWriter::new(
        app.world_mut()
            .expect("test App should allow world mutation"),
    )
    .remove(button, previous)
    .unwrap();
    app.update().expect("removed-button frame should compile");
    let removed = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("removed-button frame should exist");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        removed,
        2,
    )
    .unwrap();
    assert!(
        !app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .reducer_states()
            .contains_key(&expected_runtime_node("root", "retry"))
    );

    HierarchyWriter::new(
        app.world_mut()
            .expect("test App should allow world mutation"),
    )
    .place(button, child_last(root))
    .unwrap();
    app.update()
        .expect("reinserted-button frame should compile");
    let reinserted = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("reinserted-button frame should exist");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        reinserted,
        3,
    )
    .unwrap();
    assert!(
        !app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .reducer_states()
            .contains_key(&expected_runtime_node("root", "retry"))
    );

    let semantics = ui_semantic_snapshot(app.world(), surface_id(), 3, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &expected_runtime_node("root", "retry"))
        .unwrap();
    assert!(!retry.state().focused());
    assert!(!retry.state().pressed());
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 3),
            UiInputEvent::PointerButton {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Released,
                position: Some(point),
            },
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::NonActivation(
            UiNonActivation::NoPointerCapture
        ))
    ));
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn modal_presented_session_rejects_unhandled_wheel_input_before_gameplay() {
    let (mut app, _) = build_unstarted_button_app_with_layer(UiLayer::Modal);
    app.update().expect("modal frame should compile");
    let frame = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("modal frame should exist");
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        frame,
        1,
    )
    .unwrap();
    let admission = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<InputTimeline>()
        .admit_wheel(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            Vec2::new(0.0, 1.0),
        )
        .unwrap();
    assert_eq!(admission.admitted(), 1);

    app.update()
        .expect("modal wheel routing should fail closed without aborting the frame");
    let rejection = app
        .world()
        .resource::<UiRouteRejectionLog>()
        .as_slice()
        .first()
        .expect("modal wheel input should record one rejection");
    assert_eq!(
        rejection.rejection(),
        &UiRouteRejection::UnhandledInputBlocked
    );
}

#[test]
fn historical_non_modal_route_cannot_weaken_the_latest_modal_barrier() {
    let (mut app, _) = build_button_app();
    let initial = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        initial,
        1,
    )
    .unwrap();

    let root = app
        .world()
        .iter_entities()
        .find(|entity| entity.contains::<UiRoot>())
        .expect("test UI root should exist")
        .id();
    app.world_mut()
        .expect("test App should allow world mutation")
        .entity_mut(root)
        .insert(UiRoot::new(UiLayer::Modal));
    app.update().expect("modal successor should compile");
    let modal = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        modal,
        2,
    )
    .unwrap();

    let state = app.world().resource::<crate::runtime::UiRuntimeState>();
    let historical = state
        .surface(surface_id())
        .unwrap()
        .resolve_presented(UiRouteStamp::new(surface_id(), 1))
        .unwrap();
    assert!(historical.blocks_unhandled_input());
}

#[test]
fn interaction_and_reducer_visual_state_are_isolated_per_surface() {
    let (mut app, _button) = build_button_app();
    let second_surface = UiSurfaceId::new(8).unwrap();
    install_ui_surface(
        app.world_mut()
            .expect("test App should allow world mutation"),
        UiSurfaceDescriptor::new(
            second_surface,
            InputWindowId::new(12),
            InputSeatId::PRIMARY,
            200.0,
            100.0,
            1.0,
        )
        .unwrap(),
    )
    .unwrap();
    app.update()
        .expect("both surface candidates should compile");
    let first_before = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let second_before = latest_ui_frame(app.world(), second_surface)
        .unwrap()
        .unwrap();
    assert_eq!(first_before.paint(), second_before.paint());
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        first_before.clone(),
        1,
    )
    .unwrap();
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        second_before.clone(),
        1,
    )
    .unwrap();

    for input in [
        UiInputEvent::PointerMoved {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            position: UiPoint::new(10.0, 10.0),
        },
        UiInputEvent::PointerButton {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            button: UiPointerButton::Primary,
            phase: UiButtonPhase::Pressed,
            position: Some(UiPoint::new(10.0, 10.0)),
        },
    ] {
        assert!(matches!(
            dispatch_presented_ui_input(
                app.world_mut()
                    .expect("test App should allow world mutation"),
                UiRouteStamp::new(surface_id(), 1),
                input,
            ),
            UiRouteDisposition::Consumed(_)
        ));
    }
    app.update()
        .expect("surface-local reducer state should recompile");
    let first_after = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let second_after = latest_ui_frame(app.world(), second_surface)
        .unwrap()
        .unwrap();
    assert_ne!(first_before.paint(), first_after.paint());
    assert_eq!(second_before.paint(), second_after.paint());
}

#[test]
fn intent_backpressure_drops_activation_but_commits_the_physical_release() {
    let (mut app, _button) = build_button_app();
    app.world_mut()
        .expect("test App should allow world mutation")
        .insert_resource(UiActionJournal::with_limits(1, 1).unwrap());
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("backpressure-test").unwrap())
        .unwrap();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    commit_ui_presentation(
        app.world_mut()
            .expect("test App should allow world mutation"),
        frame,
        1,
    )
    .unwrap();
    let route = UiRouteStamp::new(surface_id(), 1);
    assert!(
        dispatch_pointer_click(&mut app, route)
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );

    let point = UiPoint::new(10.0, 10.0);
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            route,
            UiInputEvent::PointerButton {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Pressed,
                position: Some(point),
            },
        ),
        UiRouteDisposition::Consumed(_)
    ));
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            route,
            UiInputEvent::PointerButton {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Released,
                position: Some(point),
            },
        ),
        UiRouteDisposition::Consumed(_)
    ));
    let rejection = app
        .world()
        .resource::<UiRouteRejectionLog>()
        .as_slice()
        .first()
        .expect("dropped activation should retain typed backpressure evidence");
    assert!(matches!(
        rejection.rejection(),
        UiRouteRejection::IntentJournal(UiActionAdmissionError::CapacityExceeded { .. })
    ));
    let semantics = ui_semantic_snapshot(app.world(), surface_id(), 1, UiSeatId::PRIMARY).unwrap();
    assert!(
        !semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &expected_runtime_node("root", "retry"))
            .unwrap()
            .state()
            .pressed()
    );

    let first_sequence = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap()[0]
        .sequence();
    app.world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .acknowledge(&cursor, first_sequence)
        .unwrap();
    assert!(
        dispatch_pointer_click(&mut app, route)
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );
    let pending = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].sequence().get(), 2);
}

#[derive(Resource)]
struct MappingReplacementProbe {
    scale_factor: f32,
    input_scale_factor: f32,
    completed: bool,
}

fn present_then_replace_surface_mapping(world: &mut World) {
    if world.resource::<MappingReplacementProbe>().completed {
        return;
    }
    let frame = latest_ui_frame(world, surface_id())
        .unwrap()
        .expect("first runtime frame should compile before the Last-stage test presenter");
    commit_ui_presentation(world, frame, 1).unwrap();
    let probe = world.resource::<MappingReplacementProbe>();
    let scale_factor = probe.scale_factor;
    let input_scale_factor = probe.input_scale_factor;
    update_ui_surface_mapping(world, surface_id(), scale_factor, input_scale_factor).unwrap();
    world.resource_mut::<MappingReplacementProbe>().completed = true;
}

#[test]
fn retained_presented_epoch_uses_its_exact_input_mapping() {
    let (mut app, _button) = build_unstarted_button_app();
    let presented_descriptor = update_ui_surface_mapping(
        app.world_mut()
            .expect("test App should allow world mutation"),
        surface_id(),
        1.0,
        2.0,
    )
    .unwrap();
    assert_eq!(presented_descriptor.input_scale_factor(), 2.0);
    assert_eq!(presented_descriptor.route_revision(), 2);
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("mapping-history-test").unwrap())
        .unwrap();
    app.insert_resource(MappingReplacementProbe {
        scale_factor: 1.0,
        input_scale_factor: 0.5,
        completed: false,
    })
    .unwrap()
    .add_systems(CoreStage::Last, present_then_replace_surface_mapping)
    .unwrap();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<MappingReplacementProbe>()
            .completed
    );
    runtime
        .with_driver_scope(|scope| {
            apply_pointer_driver_input_at(
                scope,
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(150.0, 10.0)),
            )?;
            apply_mouse_driver_input_at(
                scope,
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Press(MouseButton::Left),
            )?;
            apply_mouse_driver_input_at(
                scope,
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Release(MouseButton::Left),
            )?;
            Ok::<(), nara_input::ButtonInputDriverError>(())
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let pending = runtime
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].action().as_str(), "retry-draft");
    assert_eq!(pending[0].input_sequence(), Some(3));
    stop_runtime(&mut runtime);
}

#[derive(Resource)]
struct PauseSafeActionConsumer {
    cursor: UiActionCursor,
    consumed: usize,
}

#[derive(Default, Resource)]
struct PauseSafePresentation {
    committed: bool,
}

fn commit_pause_safe_presentation(world: &mut World) {
    if world.resource::<PauseSafePresentation>().committed {
        return;
    }
    let frame = latest_ui_frame(world, surface_id())
        .unwrap()
        .expect("the first runtime frame should compile before presentation");
    commit_ui_presentation(world, frame, 1).unwrap();
    world.resource_mut::<PauseSafePresentation>().committed = true;
}

fn consume_pause_safe_actions(
    mut journal: ResMut<UiActionJournal>,
    mut consumer: ResMut<PauseSafeActionConsumer>,
) {
    let (pending_count, latest_sequence) = {
        let pending = journal.pending(&consumer.cursor).unwrap();
        (pending.len(), pending.last().map(|event| event.sequence()))
    };
    if let Some(latest_sequence) = latest_sequence {
        journal
            .acknowledge(&consumer.cursor, latest_sequence)
            .unwrap();
        consumer.consumed += pending_count;
    }
}

#[test]
fn paused_ui_actions_resume_through_the_journal_exactly_once() {
    let (mut app, _button) = build_unstarted_button_app();
    let cursor = app
        .world_mut()
        .expect("test App should allow world mutation")
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("pause-safe-test").unwrap())
        .unwrap();
    app.insert_resource(PauseSafeActionConsumer {
        cursor,
        consumed: 0,
    })
    .unwrap()
    .init_resource::<PauseSafePresentation>()
    .unwrap()
    .add_systems(CoreStage::PreUpdate, consume_pause_safe_actions)
    .unwrap()
    .add_systems(CoreStage::Last, commit_pause_safe_presentation)
    .unwrap();

    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<PauseSafePresentation>()
            .committed
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Paused);

    runtime
        .with_driver_scope(|scope| {
            apply_pointer_driver_input_at(
                scope,
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(10.0, 10.0)),
            )?;
            apply_mouse_driver_input_at(
                scope,
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Press(MouseButton::Left),
            )?;
            apply_mouse_driver_input_at(
                scope,
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Release(MouseButton::Left),
            )?;
            Ok::<(), nara_input::ButtonInputDriverError>(())
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let paused_world = runtime.world();
    let consumer = paused_world.resource::<PauseSafeActionConsumer>();
    assert_eq!(consumer.consumed, 0);
    assert_eq!(
        paused_world
            .resource::<UiActionJournal>()
            .pending(&consumer.cursor)
            .unwrap()
            .len(),
        1
    );

    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Running);
    assert_eq!(
        runtime
            .world()
            .resource::<PauseSafeActionConsumer>()
            .consumed,
        1
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<PauseSafeActionConsumer>()
            .consumed,
        1
    );
    stop_runtime(&mut runtime);
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
    assert!(matches!(
        dispatch_presented_ui_input(
            app.world_mut()
                .expect("test App should allow world mutation"),
            UiRouteStamp::new(surface_id(), 1),
            UiInputEvent::PointerMoved {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                position: UiPoint::new(10.0, 10.0),
            },
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::RetiredSurface { surface: 7 })
    ));
    let descriptor = UiSurfaceDescriptor::new(
        surface_id(),
        InputWindowId::new(11),
        InputSeatId::PRIMARY,
        200.0,
        100.0,
        1.0,
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
