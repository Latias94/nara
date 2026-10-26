use std::{collections::BTreeMap, sync::Arc, time::Duration};

use nara_app::{
    App, CoreStage, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance, RuntimeState,
};
use nara_asset::AssetRef;
use nara_core::Vec2;
use nara_ecs::{Entity, Res, ResMut, Resource, World};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_input::{
    ActionId, ActionMap, ActionOutcomes, ActionPhase, ButtonDriverInput, ButtonTransitionPhase,
    InputEventKind, InputGeometryGeneration, InputPointerId, InputPresentationEpoch,
    InputPresentedRoute, InputRouteGeneration, InputSeatId, InputSlotGeneration, InputSurfaceId,
    InputTimeline, InputWindowEvent, InputWindowId, KeyCode, MAX_INPUT_TIMELINE_ENTRIES,
    MouseButton, PointerDriverInput, apply_mouse_driver_input_at, apply_pointer_driver_input_at,
};
use nara_reflect::{
    ComponentRegistry, ComponentRegistryPlugin, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, SceneDocument, SceneEntityId,
    SceneEntityRecord, SceneEntitySource, ScenePatchDocument,
};
use nara_ui_kernel::{
    UiAccessibilityInput, UiActivationSource, UiButtonPhase, UiInputEvent, UiKey, UiLayer,
    UiLocalKey, UiNonActivation, UiPaintPrimitive, UiPoint, UiPointerButton, UiPointerId,
    UiPresentationReceipt, UiResolvedBrush, UiRootKey, UiRootStatus, UiRouteStamp,
    UiSafeAreaInsets, UiSeatId, UiSemanticRole, UiSize, UiSurfaceContentTransform,
    UiSurfaceGenerations, UiSurfaceId, UiTheme,
};

use crate::interaction::dispatch_presented_ui_input;
use crate::{
    UiActionAdmissionError, UiActionConsumerId, UiActionCursor, UiActionId, UiActionJournal,
    UiAdapterError, UiBrush, UiButton, UiButtonVisuals, UiColor, UiColorRole, UiNode, UiPlugin,
    UiRoot, UiRouteDisposition, UiRouteRejection, UiRouteRejectionLog, UiRuntimeKey, UiSemantics,
    UiStyle, UiSurfaceAvailability, UiSurfaceEndpoint, UiSurfaceSlot, UiSurfaceSuspensionReason,
    UiText, UiTextAlignment, UiTextOverflow, UiVisual, active_ui_surface_slots,
    dispatch_ui_accessibility_input, install_ui_surface, latest_ui_frame, register_ui_components,
    restore_ui_surface, retire_ui_surface, suspend_ui_surface, ui_semantic_snapshot,
    ui_surface_slot_snapshots, update_ui_surface_slot,
};

#[path = "adapter_test_driver.rs"]
mod driver;
use driver::UiTestApp;

const TEST_DOMAIN: &str = "nara.test.ui";

#[test]
fn dynamic_text_content_preserves_authored_adapter_presentation() {
    let authored = UiText::new("Score", 16.0)
        .unwrap()
        .with_brush(UiBrush::Role(UiColorRole::Warning))
        .with_alignment(UiTextAlignment::Center)
        .with_overflow(UiTextOverflow::Clip)
        .with_semantic_text("Authored score");

    let projected = authored.clone().with_content("Score 000300");
    let authored = authored.kernel();
    let projected = projected.kernel();

    assert_eq!(projected.content(), "Score 000300");
    assert_eq!(projected.logical_size(), authored.logical_size());
    assert_eq!(projected.brush(), authored.brush());
    assert_eq!(projected.alignment(), authored.alignment());
    assert_eq!(projected.overflow(), authored.overflow());
    assert_eq!(projected.semantic_text(), authored.semantic_text());
}

#[derive(Debug, Default, Resource)]
struct CapturedGameplayPhases(Vec<ActionPhase>);

fn capture_gameplay_phases(
    outcomes: Res<ActionOutcomes>,
    mut captured: ResMut<CapturedGameplayPhases>,
) {
    captured
        .0
        .extend(outcomes.as_slice().iter().map(|outcome| outcome.phase));
}

fn install_gameplay_capture(app: &mut UiTestApp) {
    app.configuration_world()
        .insert_resource(CapturedGameplayPhases::default());
    app.configuration_world()
        .resource_mut::<ActionMap>()
        .bind_mouse(ActionId::new("fire").unwrap(), MouseButton::Left);
    app.configuration()
        .add_systems(CoreStage::Update, capture_gameplay_phases)
        .expect("gameplay action capture should install");
}

fn ui_app() -> UiTestApp {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_input::InputPlugin,
        UiPlugin::default(),
    ))
    .expect("UI test plugins should install");
    UiTestApp::new(app)
}

#[test]
fn custom_font_uses_ordinary_adapter_and_configured_presentation() {
    fn compile_packets(world: &mut World) {
        let frames = crate::latest_ui_frames(world).unwrap();
        let stats = nara_ui_render::compile_ui_paint_packets(world, &frames).unwrap();
        assert_eq!(stats.admitted_packets, 1);
        assert_eq!(stats.rejected_packets, 0);
    }

    let font = nara_text::FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let bundled = nara_text::bundled_font_face().unwrap();
    assert_ne!(font.id(), bundled.id());
    assert_eq!(
        crate::plugin(font.clone()).key(),
        crate::plugin(font.clone()).key()
    );
    assert_ne!(
        crate::plugin(font.clone()).key(),
        crate::plugin(bundled.clone()).key()
    );
    assert_eq!(
        nara_ui_render::plugin(font.clone()).key(),
        nara_ui_render::plugin(font.clone()).key()
    );
    assert_ne!(
        nara_ui_render::plugin(font.clone()).key(),
        nara_ui_render::plugin(bundled).key()
    );

    let mut configuration = App::new();
    configuration
        .add_plugins((
            ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_transform::TransformPlugin,
            nara_input::InputPlugin,
            (
                nara_render::RenderComponentsPlugin,
                nara_render::RenderPlugin::default(),
                nara_image::ImagePreparePlugin,
            ),
            crate::plugin(font.clone()),
            nara_ui_render::plugin(font.clone()),
        ))
        .unwrap();
    configuration
        .add_systems(CoreStage::Queue, compile_packets)
        .unwrap();
    let world = configuration.world_mut().unwrap();
    install_test_surface(world);
    nara_ui_render::__private::replace_ui_presentation_routes(
        world,
        [nara_ui_render::__private::UiPresentationRoute::new(
            surface_id(),
            nara_window::WindowId::PRIMARY,
        )
        .unwrap()],
    )
    .unwrap();
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(TEST_DOMAIN, "custom-font-root").unwrap(),
        ))
        .id();
    let label = world
        .spawn((
            UiNode::new(UiStyle::absolute(8.0, 6.0, 180.0, 40.0)),
            UiText::new("Nara mono", 18.0).unwrap(),
            UiRuntimeKey::new(TEST_DOMAIN, "custom-font-label").unwrap(),
        ))
        .id();
    place_ui_tree(world, root, &[label]);

    let mut app = UiTestApp::new(configuration);
    app.update().unwrap();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let glyphs = frame
        .paint()
        .iter()
        .filter_map(|primitive| match primitive {
            UiPaintPrimitive::Text { layout, .. } => Some(layout.glyphs()),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();
    assert!(!glyphs.is_empty());
    assert!(
        glyphs
            .iter()
            .all(|glyph| glyph.key().face_id() == font.id())
    );
    let (packets, _) = nara_ui_render::__private::snapshot_ui_paint_packets(app.world())
        .unwrap()
        .into_parts();
    assert_eq!(packets.len(), 1);
    assert!(Arc::ptr_eq(&packets[0].frame(), &frame));
    assert!(
        packets[0]
            .glyph_pages()
            .iter()
            .any(|page| page.pixels().iter().any(|coverage| *coverage != 0))
    );
}

fn commit_ui_presentation(
    world: &mut World,
    frame: Arc<nara_ui_kernel::UiFrame>,
    presentation_epoch: u64,
) -> Result<UiPresentationReceipt, UiAdapterError> {
    let prepared = crate::__private::prepare_ui_presentation(world, frame, presentation_epoch)?;
    Ok(crate::__private::commit_prepared_ui_presentation(
        world, prepared,
    ))
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

fn route_stamp(presentation_epoch: u64) -> UiRouteStamp {
    route_stamp_for_generations(presentation_epoch, 1, 1, 1, 1)
}

fn route_stamp_for_generations(
    presentation_epoch: u64,
    slot: u64,
    route: u64,
    geometry: u64,
    presentation: u64,
) -> UiRouteStamp {
    let generations = UiSurfaceGenerations::new(slot, route, geometry, presentation)
        .expect("test surface generations should be valid");
    UiRouteStamp::new(
        surface_id(),
        generations.slot_generation(),
        generations.route_revision(),
        generations.geometry_generation(),
        presentation_epoch,
    )
}

fn test_receipt(world: &World, presentation_epoch: u64) -> UiPresentationReceipt {
    crate::runtime::presented_receipt(world, surface_id(), presentation_epoch)
        .expect("test presentation receipt should be retained")
}

fn test_endpoint(window: u64) -> UiSurfaceEndpoint {
    UiSurfaceEndpoint::new(
        InputWindowId::new(window),
        InputSeatId::PRIMARY,
        InputPointerId::PRIMARY,
    )
}

fn test_transform(
    authored_width: f32,
    authored_height: f32,
    physical_width: f32,
    physical_height: f32,
    platform_scale_factor: f32,
) -> UiSurfaceContentTransform {
    UiSurfaceContentTransform::new(
        UiSize::new(authored_width, authored_height).expect("test authored size should be valid"),
        UiSize::new(physical_width, physical_height).expect("test physical size should be valid"),
        UiSafeAreaInsets::default(),
        platform_scale_factor,
    )
    .expect("test content transform should be valid")
}

fn test_slot(
    surface: UiSurfaceId,
    window: u64,
    authored_width: f32,
    authored_height: f32,
) -> UiSurfaceSlot {
    UiSurfaceSlot::new(
        surface,
        test_endpoint(window),
        test_transform(
            authored_width,
            authored_height,
            authored_width,
            authored_height,
            1.0,
        ),
    )
    .expect("test surface slot should be valid")
}

fn install_test_surface(world: &mut World) {
    install_ui_surface(world, test_slot(surface_id(), 11, 200.0, 100.0))
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
    assert_eq!(catalog.generation(), 3);
    assert!(catalog.predecessor().is_some());
    for schema in &catalog.components {
        let brush_bearing = matches!(schema.id().as_str(), "nara.ui.UiVisual" | "nara.ui.UiText");
        assert_eq!(
            schema.version(),
            match schema.id().as_str() {
                "nara.ui.UiText" => ComponentSchemaVersion(3),
                "nara.ui.UiVisual" => ComponentSchemaVersion(2),
                _ => ComponentSchemaVersion::ONE,
            }
        );
        assert_eq!(schema.field_tombstones().is_empty(), !brush_bearing);
        assert!(!schema.fields().is_empty());
        assert!(schema.fields().iter().all(|field| {
            field.required == !field.id().as_str().ends_with(".image")
                && field.default_value.is_none()
        }));
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
fn legacy_brush_migrations_preserve_roles_and_expose_image_asset_references() {
    fn legacy_brush(kind: &str, reference: &str) -> ComponentValue {
        ComponentValue::map([
            ("kind", ComponentValue::String(kind.to_owned())),
            ("red", ComponentValue::f64(1.0).unwrap()),
            ("green", ComponentValue::f64(1.0).unwrap()),
            ("blue", ComponentValue::f64(1.0).unwrap()),
            ("alpha", ComponentValue::f64(1.0).unwrap()),
            ("reference", ComponentValue::String(reference.to_owned())),
        ])
    }

    let registry = frozen_ui_registry();
    let visual_id = ComponentTypeId::new("nara.ui.UiVisual");
    let legacy_visual = ComponentValue::map([
        ("background", legacy_brush("image", "assets/ui/panel.png")),
        ("border_width", ComponentValue::f64(0.0).unwrap()),
        ("border_brush", legacy_brush("solid", "")),
        (
            "radii",
            ComponentValue::map([
                ("top_left", ComponentValue::f64(0.0).unwrap()),
                ("top_right", ComponentValue::f64(0.0).unwrap()),
                ("bottom_right", ComponentValue::f64(0.0).unwrap()),
                ("bottom_left", ComponentValue::f64(0.0).unwrap()),
            ]),
        ),
    ]);
    let migrated_visual = registry
        .migrate_component_value(&visual_id, ComponentSchemaVersion::ONE, &legacy_visual)
        .expect("legacy visual must migrate");
    assert_eq!(migrated_visual.version, ComponentSchemaVersion(2));
    let references = nara_reflect::collect_declared_asset_references(
        &registry,
        &visual_id,
        migrated_visual.version,
        &migrated_visual.value,
    )
    .expect("migrated image brush must expose its semantic asset reference");
    assert_eq!(references.len(), 1);
    assert_eq!(
        references[0].asset_ref(),
        &AssetRef::path("assets/ui/panel.png").unwrap()
    );

    let text_id = ComponentTypeId::new("nara.ui.UiText");
    let legacy_text = ComponentValue::map([
        ("content", ComponentValue::String("Text".to_owned())),
        ("logical_size", ComponentValue::f64(16.0).unwrap()),
        ("brush", legacy_brush("role", "primary_text")),
        ("alignment", ComponentValue::String("start".to_owned())),
        ("overflow", ComponentValue::String("clip".to_owned())),
        ("has_semantic_text", ComponentValue::Bool(false)),
        ("semantic_text", ComponentValue::String(String::new())),
    ]);
    let migrated_text = registry
        .migrate_component_value(&text_id, ComponentSchemaVersion::ONE, &legacy_text)
        .expect("legacy text must migrate");
    assert_eq!(migrated_text.version, ComponentSchemaVersion(3));
    assert_eq!(
        migrated_text
            .value
            .field("brush")
            .unwrap()
            .field_str("role")
            .unwrap(),
        "primary_text"
    );
    assert_eq!(
        migrated_text
            .value
            .field("brush")
            .unwrap()
            .field("image")
            .unwrap(),
        &ComponentValue::Null
    );
}

#[test]
fn legacy_root_and_defaulted_node_shapes_are_rejected() {
    let registry = frozen_ui_registry();
    let snapshot = registry.snapshot().unwrap();

    let legacy_root = ComponentValue::map([
        (
            "target",
            ComponentValue::String("primary_window".to_string()),
        ),
        ("order", ComponentValue::I64(0)),
    ]);
    assert!(matches!(
        snapshot.preflight_component(&ComponentTypeId::new("nara.ui.UiRoot"), &legacy_root),
        Err(nara_reflect::ComponentCodecError::MissingField { field }) if field == "layer"
    ));

    let defaulted_node = ComponentValue::map([(
        "style",
        ComponentValue::map([("visible", ComponentValue::Bool(true))]),
    )]);
    assert!(matches!(
        snapshot.preflight_component(&ComponentTypeId::new("nara.ui.UiNode"), &defaulted_node),
        Err(nara_reflect::ComponentCodecError::MissingField { .. })
    ));
}

#[test]
fn button_visual_roles_round_trip_as_canonical_scene_data() {
    let registry = frozen_ui_registry();
    let snapshot = registry.snapshot().unwrap();
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
    let encoded = snapshot
        .encode_component(&component_id, &source, source_entity)
        .expect("button visuals should encode")
        .expect("the source entity should contain button visuals");
    let prepared = snapshot
        .preflight_component(&component_id, &encoded)
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
fn surface_transform_updates_advance_exact_slot_generations_atomically() {
    let mut app = ui_app();
    let slot = test_slot(surface_id(), 11, 960.0, 540.0);
    let world = app.configuration_world();
    install_ui_surface(world, slot).unwrap();

    let render_changed = update_ui_surface_slot(
        world,
        surface_id(),
        slot.endpoint(),
        test_transform(960.0, 540.0, 1_920.0, 1_080.0, 1.0),
    )
    .expect("content transform should update atomically");
    assert_eq!(render_changed.slot_generation(), 2);
    assert_eq!(render_changed.route_generation(), 2);
    assert_eq!(render_changed.geometry_generation(), 2);
    assert_eq!(render_changed.presentation_generation(), 2);
    assert_eq!(render_changed.transform().content_scale_factor(), 2.0);

    let platform_changed = update_ui_surface_slot(
        world,
        surface_id(),
        slot.endpoint(),
        test_transform(960.0, 540.0, 1_920.0, 1_080.0, 2.0),
    )
    .expect("platform mapping should update atomically");
    assert_eq!(platform_changed.slot_generation(), 3);
    assert_eq!(platform_changed.route_generation(), 3);
    assert_eq!(platform_changed.geometry_generation(), 3);
    assert_eq!(platform_changed.presentation_generation(), 3);
    assert_eq!(
        update_ui_surface_slot(
            world,
            surface_id(),
            platform_changed.endpoint(),
            platform_changed.transform(),
        )
        .expect("an unchanged slot should remain valid"),
        platform_changed
    );
}

#[test]
fn endpoint_and_transform_updates_have_distinct_exact_invalidation() {
    let mut app = ui_app();
    let slot = test_slot(surface_id(), 11, 960.0, 540.0);
    let world = app.configuration_world();
    install_ui_surface(world, slot).unwrap();

    let resized = update_ui_surface_slot(
        world,
        surface_id(),
        slot.endpoint(),
        test_transform(1_280.0, 720.0, 1_280.0, 720.0, 1.0),
    )
    .expect("logical extent should update with the transform");
    assert_eq!(resized.transform().authored_logical_size().width(), 1_280.0);
    assert_eq!(resized.slot_generation(), 2);
    assert_eq!(resized.geometry_generation(), 2);

    let moved_endpoint = UiSurfaceEndpoint::new(
        InputWindowId::new(12),
        InputSeatId::PRIMARY,
        InputPointerId::PRIMARY,
    );
    let moved = update_ui_surface_slot(world, surface_id(), moved_endpoint, resized.transform())
        .expect("endpoint should update atomically");
    assert_eq!(moved.endpoint(), moved_endpoint);
    assert_eq!(moved.slot_generation(), 3);
    assert_eq!(moved.route_generation(), 3);
    assert_eq!(moved.geometry_generation(), 2);
    assert_eq!(moved.presentation_generation(), 3);
}

#[test]
fn transform_update_unbinds_ui_route_but_preserves_the_physical_release() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("slot-cleanup-test").unwrap())
        .unwrap();
    {
        let mut timeline = app.timeline();
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
    let runtime = app.world().resource::<crate::runtime::UiRuntimeState>();
    let surface = runtime.surface(surface_id()).unwrap();
    assert!(surface.interaction().has_pointer_facts(surface_id()));
    assert!(
        surface
            .reducer_states()
            .values()
            .any(|state| state.pressed())
    );

    app.update_surface(
        surface_id(),
        test_endpoint(11),
        test_transform(200.0, 100.0, 400.0, 200.0, 1.0),
    )
    .unwrap();
    let runtime = app.world().resource::<crate::runtime::UiRuntimeState>();
    let surface = runtime.surface(surface_id()).unwrap();
    assert!(!surface.interaction().has_pointer_facts(surface_id()));
    assert!(
        surface
            .reducer_states()
            .values()
            .all(|state| !state.pressed())
    );
    let release = app
        .timeline()
        .admit_mouse_button(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();
    assert_eq!(release.admitted(), 1);
    assert!(
        !app.world()
            .resource::<InputTimeline>()
            .entries()
            .last()
            .expect("the physical release should remain in the timeline")
            .route()
            .is_ui_routable()
    );
    app.update()
        .expect("an unbound physical release should pass through without UI activation");
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        app.dispatch_input(
            UiRouteStamp::from_receipt(receipt),
            UiInputEvent::PointerMoved {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                position: UiPoint::new(10.0, 10.0),
            },
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::SurfaceGenerationMismatch)
    ));
}

#[test]
fn pass_through_press_remains_balanced_across_a_transform_update() {
    let (mut app, _button) = build_unstarted_button_app();
    app.update_surface(
        surface_id(),
        test_endpoint(11),
        test_transform(200.0, 100.0, 300.0, 100.0, 1.0),
    )
    .unwrap();
    app.update().unwrap();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(frame, 1).unwrap();
    {
        let mut timeline = app.timeline();
        timeline
            .admit_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(25.0, 10.0)),
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
    app.update()
        .expect("a non-modal letterbox press should pass through UI routing");

    app.update_surface(
        surface_id(),
        test_endpoint(11),
        test_transform(200.0, 100.0, 400.0, 200.0, 1.0),
    )
    .unwrap();
    app.update()
        .expect("the successor surface slot should compile before rebinding");
    let successor = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("the successor surface slot should expose a candidate");
    app.present_frame(successor, 2).unwrap();
    let release = app
        .timeline()
        .admit_mouse_button(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();

    assert_eq!(release.admitted(), 1);
    let release_entry = app
        .world()
        .resource::<InputTimeline>()
        .entries()
        .last()
        .copied()
        .expect("the release should remain observable");
    assert!(matches!(
        release_entry.kind(),
        InputEventKind::PointerButton {
            button: MouseButton::Left,
            phase: ButtonTransitionPhase::Released,
        }
    ));
    assert!(!release_entry.route().is_ui_routable());
}

#[test]
fn mismatched_exact_input_route_aborts_a_prepared_slot_update() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    let original = active_ui_surface_slots(app.world()).unwrap()[0];
    app.timeline()
        .publish_presented_route(InputPresentedRoute::new(
            original.endpoint().window(),
            InputSurfaceId::new(surface_id().get()),
            original.endpoint().seat(),
            InputPresentationEpoch::new(2).unwrap(),
            InputSlotGeneration::new(receipt.slot_generation().get()).unwrap(),
            InputRouteGeneration::new(receipt.route_revision().get()).unwrap(),
            InputGeometryGeneration::new(receipt.geometry_generation().get()).unwrap(),
        ))
        .unwrap();

    assert!(matches!(
        app.update_surface(
            surface_id(),
            original.endpoint(),
            test_transform(200.0, 100.0, 400.0, 200.0, 1.0),
        ),
        Err(UiAdapterError::Input(
            nara_input::InputIngressError::PresentedRouteMismatch { .. }
        ))
    ));
    assert_eq!(active_ui_surface_slots(app.world()).unwrap(), [original]);
    assert_eq!(test_receipt(app.world(), 1), receipt);
}

#[test]
fn prepared_presentation_reserves_the_route_and_commits_without_a_fallible_arm() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let prepared = app.prepare_presentation(Arc::clone(&frame), 1).unwrap();

    assert!(crate::runtime::presented_receipt(app.world(), surface_id(), 1).is_none());
    let slot = active_ui_surface_slots(app.world()).unwrap()[0];
    assert!(matches!(
        app.timeline()
            .publish_presented_route(InputPresentedRoute::new(
                slot.endpoint().window(),
                InputSurfaceId::new(surface_id().get()),
                slot.endpoint().seat(),
                InputPresentationEpoch::new(1).unwrap(),
                InputSlotGeneration::new(slot.slot_generation()).unwrap(),
                InputRouteGeneration::new(slot.route_generation()).unwrap(),
                InputGeometryGeneration::new(slot.geometry_generation()).unwrap(),
            )),
        Err(nara_input::InputIngressError::RouteReservationConflict { .. })
    ));

    let receipt = app.commit_presentation(prepared);
    assert_eq!(receipt, test_receipt(app.world(), 1));
}

#[test]
fn aborting_prepared_presentation_releases_the_exact_route_reservation() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let prepared = app.prepare_presentation(Arc::clone(&frame), 1).unwrap();
    app.abort_presentation(prepared);

    assert!(crate::runtime::presented_receipt(app.world(), surface_id(), 1).is_none());
    let retry = app
        .prepare_presentation(frame, 1)
        .expect("aborting must release route capacity and endpoint ownership");
    let receipt = app.commit_presentation(retry);
    assert_eq!(receipt, test_receipt(app.world(), 1));
}

#[test]
fn deferred_presentation_reprepares_an_equivalent_candidate_without_changing_packet_identity() {
    let (mut app, _button) = build_button_app();
    let original = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let prepared = app.prepare_presentation(Arc::clone(&original), 7).unwrap();
    let deferred = app.defer_presentation(prepared);

    app.update()
        .expect("an equivalent authoring frame should compile while backend work is deferred");
    let latest = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    assert!(!Arc::ptr_eq(&latest, &original));

    let crate::__private::UiDeferredPresentationRetry::Prepared(retried) =
        app.retry_presentation(deferred).unwrap()
    else {
        panic!("equivalent deferred facts must remain retryable");
    };
    assert!(Arc::ptr_eq(retried.frame(), &original));
    let receipt = app.commit_presentation(*retried);
    assert_eq!(receipt.frame_revision(), original.revision());
    assert_eq!(receipt.presentation_epoch(), 7);
}

#[test]
fn deferred_presentation_cannot_retry_after_behavior_changes() {
    let (mut app, button) = build_button_app();
    let original = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let prepared = app.prepare_presentation(original, 9).unwrap();
    let deferred = app.defer_presentation(prepared);

    let disabled = app
        .world()
        .get::<UiButton>(button)
        .expect("the test Button should remain installed")
        .clone()
        .with_enabled(false);
    app.set_button(button, disabled);
    app.update()
        .expect("the changed Button behavior should compile a successor candidate");

    assert!(matches!(
        app.retry_presentation(deferred).unwrap(),
        crate::__private::UiDeferredPresentationRetry::Superseded
    ));
    let latest = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let successor = app
        .prepare_presentation(latest, 10)
        .expect("superseding deferred work must leave route capacity reusable");
    app.abort_presentation(successor);
}

#[test]
fn copied_slots_cannot_bypass_runtime_owned_generations() {
    let mut app = ui_app();
    let stale_snapshot = test_slot(surface_id(), 11, 960.0, 540.0);
    let world = app.configuration_world();
    install_ui_surface(world, stale_snapshot).unwrap();
    let resized = update_ui_surface_slot(
        world,
        surface_id(),
        stale_snapshot.endpoint(),
        test_transform(1_280.0, 720.0, 1_280.0, 720.0, 1.0),
    )
    .unwrap();
    let current = update_ui_surface_slot(
        world,
        surface_id(),
        stale_snapshot.endpoint(),
        test_transform(1_280.0, 720.0, 2_560.0, 1_440.0, 1.0),
    )
    .unwrap();

    assert_eq!(stale_snapshot.slot_generation(), 1);
    assert_eq!(resized.slot_generation(), 2);
    assert_eq!(current.slot_generation(), 3);
    assert!(matches!(
        install_ui_surface(world, stale_snapshot),
        Err(UiAdapterError::SurfaceAlreadyInstalled { surface: 7 })
    ));
    assert_eq!(active_ui_surface_slots(world).unwrap(), [current]);
}

#[test]
fn duplicate_window_target_is_rejected_even_for_a_distinct_seat() {
    let mut app = ui_app();
    install_test_surface(app.configuration_world());
    let conflicting_surface = UiSurfaceId::new(8).unwrap();
    let conflicting = UiSurfaceSlot::new(
        conflicting_surface,
        UiSurfaceEndpoint::new(
            InputWindowId::new(11),
            InputSeatId::new(1),
            InputPointerId::PRIMARY,
        ),
        test_transform(200.0, 100.0, 200.0, 100.0, 1.0),
    )
    .unwrap();

    assert!(matches!(
        app.install_surface(conflicting,),
        Err(UiAdapterError::PresentationTargetConflict {
            window: 11,
            surface: 7,
        })
    ));
}

#[test]
fn occupied_target_conflicts_leave_update_and_restore_state_unchanged() {
    let mut app = ui_app();
    let primary = test_slot(surface_id(), 11, 200.0, 100.0);
    let other_surface = UiSurfaceId::new(8).unwrap();
    let other = test_slot(other_surface, 12, 200.0, 100.0);
    app.install_surface(primary).unwrap();
    app.install_surface(other).unwrap();

    let active_before = ui_surface_slot_snapshots(app.world()).unwrap();
    assert!(matches!(
        app.update_surface(surface_id(), test_endpoint(12), primary.transform(),),
        Err(UiAdapterError::PresentationTargetConflict {
            window: 12,
            surface: 8,
        })
    ));
    assert_eq!(
        ui_surface_slot_snapshots(app.world()).unwrap(),
        active_before
    );

    app.suspend_surface(surface_id(), UiSurfaceSuspensionReason::EndpointUnavailable)
        .unwrap()
        .expect("the primary surface should suspend");
    let suspended_before = ui_surface_slot_snapshots(app.world()).unwrap();
    assert!(matches!(
        app.restore_surface(surface_id(), test_endpoint(12), primary.transform(),),
        Err(UiAdapterError::PresentationTargetConflict {
            window: 12,
            surface: 8,
        })
    ));
    assert_eq!(
        ui_surface_slot_snapshots(app.world()).unwrap(),
        suspended_before
    );
    assert!(matches!(
        app.install_surface(test_slot(UiSurfaceId::new(9).unwrap(), 11, 200.0, 100.0),),
        Err(UiAdapterError::PresentationTargetConflict {
            window: 11,
            surface: 7,
        })
    ));
    assert!(matches!(
        app.install_surface(test_slot(UiSurfaceId::new(10).unwrap(), 12, 200.0, 100.0),),
        Err(UiAdapterError::PresentationTargetConflict {
            window: 12,
            surface: 8,
        })
    ));
}

#[test]
fn suspension_retires_presented_authority_and_restore_requires_a_fresh_receipt() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();

    let suspension = app
        .suspend_surface(surface_id(), UiSurfaceSuspensionReason::ZeroExtent)
        .unwrap()
        .expect("an active surface should suspend exactly once");
    assert_eq!(suspension.previous_slot_generation(), 1);
    assert_eq!(suspension.suspended_slot_generation(), 2);
    assert_eq!(suspension.reason(), UiSurfaceSuspensionReason::ZeroExtent);
    assert!(active_ui_surface_slots(app.world()).unwrap().is_empty());
    let snapshots = ui_surface_slot_snapshots(app.world()).unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].availability(),
        UiSurfaceAvailability::Suspended(UiSurfaceSuspensionReason::ZeroExtent)
    );
    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none()
    );
    assert!(ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).is_err());
    assert!(
        app.suspend_surface(surface_id(), UiSurfaceSuspensionReason::EndpointUnavailable,)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        app.update_surface(
            surface_id(),
            test_endpoint(11),
            suspension.snapshot().slot().transform(),
        ),
        Err(UiAdapterError::SurfaceSuspended { surface: 7 })
    ));

    let conflicting = test_slot(UiSurfaceId::new(8).unwrap(), 11, 200.0, 100.0);
    assert!(matches!(
        app.install_surface(conflicting),
        Err(UiAdapterError::PresentationTargetConflict {
            window: 11,
            surface: 7,
        })
    ));

    let restored = app
        .restore_surface(
            surface_id(),
            test_endpoint(11),
            suspension.snapshot().slot().transform(),
        )
        .expect("a suspended surface should restore with successor generations");
    assert_eq!(restored.slot_generation(), 3);
    assert_eq!(restored.route_generation(), 3);
    assert_eq!(restored.geometry_generation(), 3);
    assert_eq!(restored.presentation_generation(), 3);
    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        app.restore_surface(surface_id(), test_endpoint(11), restored.transform(),),
        Err(UiAdapterError::SurfaceAlreadyActive { surface: 7 })
    ));

    app.update()
        .expect("restore should compile a fresh candidate without reviving the old receipt");
    let successor = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let successor_receipt = app.present_frame(successor, 2).unwrap();
    assert!(ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).is_err());
    assert!(ui_semantic_snapshot(app.world(), successor_receipt, UiSeatId::PRIMARY).is_ok());
}

#[test]
fn suspended_surface_can_retire_without_an_intermediate_restoration() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();

    let suspension = app
        .suspend_surface(surface_id(), UiSurfaceSuspensionReason::EndpointUnavailable)
        .unwrap()
        .expect("the active surface should suspend");
    let retirement = app
        .retire_surface(surface_id())
        .unwrap()
        .expect("a suspended surface should retire directly");

    assert_eq!(
        retirement.slot_generation(),
        suspension.suspended_slot_generation()
    );
    assert_eq!(
        retirement.retirement_generation(),
        suspension.suspended_slot_generation() + 1
    );
    assert!(ui_surface_slot_snapshots(app.world()).unwrap().is_empty());
    assert!(ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).is_err());
    app.install_surface(test_slot(UiSurfaceId::new(8).unwrap(), 11, 200.0, 100.0))
        .expect("retirement should release the suspended target reservation");
    assert!(matches!(
        app.install_surface(test_slot(surface_id(), 13, 200.0, 100.0),),
        Err(UiAdapterError::RetiredSurface { surface: 7 })
    ));
}

#[test]
fn restoring_on_a_new_endpoint_moves_the_unique_target_binding_atomically() {
    let mut app = ui_app();
    let original = test_slot(surface_id(), 11, 200.0, 100.0);
    app.install_surface(original).unwrap();
    app.suspend_surface(surface_id(), UiSurfaceSuspensionReason::EndpointUnavailable)
        .unwrap()
        .expect("the original target should suspend");

    let restored = app
        .restore_surface(surface_id(), test_endpoint(12), original.transform())
        .expect("restore should transfer the target binding");
    assert_eq!(restored.endpoint().window(), InputWindowId::new(12));
    app.install_surface(test_slot(UiSurfaceId::new(8).unwrap(), 11, 200.0, 100.0))
        .expect("the predecessor target should be reusable after exact handoff");
    assert!(matches!(
        app.install_surface(test_slot(UiSurfaceId::new(9).unwrap(), 12, 200.0, 100.0),),
        Err(UiAdapterError::PresentationTargetConflict {
            window: 12,
            surface: 7,
        })
    ));
}

#[test]
fn endpoint_handoff_keeps_a_held_edge_owned_by_its_predecessor_lineage() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(frame, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("endpoint-handoff-test").unwrap())
        .unwrap();

    app.timeline()
        .admit_keyboard(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            ButtonDriverInput::Press(KeyCode::Enter),
        )
        .unwrap();
    app.update()
        .expect("an unfocused Enter press should settle as predecessor-owned gameplay input");
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );

    let original = active_ui_surface_slots(app.world()).unwrap()[0];
    app.suspend_surface(surface_id(), UiSurfaceSuspensionReason::EndpointUnavailable)
        .unwrap()
        .expect("the predecessor endpoint should suspend exactly once");
    app.restore_surface(surface_id(), test_endpoint(12), original.transform())
        .expect("the successor endpoint should acquire the Surface Slot");
    app.update()
        .expect("the successor endpoint should compile a fresh candidate");
    let successor = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(successor, 2).unwrap();

    let release = app
        .timeline()
        .admit_keyboard(
            InputWindowId::new(12),
            InputSeatId::PRIMARY,
            ButtonDriverInput::Release(KeyCode::Enter),
        )
        .unwrap();
    assert_eq!(release.admitted(), 1);
    let release_entry = app
        .world()
        .resource::<InputTimeline>()
        .entries()
        .last()
        .copied()
        .expect("the balanced release should remain in the timeline");
    assert_eq!(release_entry.route().window(), InputWindowId::new(11));
    assert!(!release_entry.route().is_ui_routable());

    app.update()
        .expect("the predecessor-owned release must not activate successor UI");
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
}

fn compiled_order(reverse_allocation: bool) -> (Vec<UiKey>, Vec<UiKey>, UiKey) {
    let mut app = ui_app();
    install_test_surface(app.configuration_world());
    let world = app.configuration_world();
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

fn build_unstarted_button_app() -> (UiTestApp, Entity) {
    build_unstarted_button_app_with_layer(UiLayer::Base)
}

fn build_unstarted_button_app_with_layer(layer: UiLayer) -> (UiTestApp, Entity) {
    let mut app = ui_app();
    install_test_surface(app.configuration_world());
    let world = app.configuration_world();
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

fn build_button_app() -> (UiTestApp, Entity) {
    let (mut app, button) = build_unstarted_button_app();
    app.update().expect("UI test frame should update");
    (app, button)
}

#[test]
fn nonmodal_content_miss_reaches_gameplay_and_releases_over_a_button() {
    let (mut app, _) = build_unstarted_button_app();
    install_gameplay_capture(&mut app);
    app.update().unwrap();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    assert!(!frame.blocks_unhandled_input());
    app.present_frame(frame, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("content-miss-test").unwrap())
        .unwrap();
    {
        let mut timeline = app.timeline();
        timeline
            .admit_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(150.0, 75.0)),
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
    let pressed = app.world().resource::<CapturedGameplayPhases>().0.clone();
    app.clear_gameplay_capture();
    {
        let mut timeline = app.timeline();
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
                ButtonDriverInput::Release(MouseButton::Left),
            )
            .unwrap();
    }
    app.update().unwrap();
    let released = app.world().resource::<CapturedGameplayPhases>().0.clone();
    let actions = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap()
        .len();
    drop(app);

    assert_eq!(pressed, [ActionPhase::Started]);
    assert_eq!(released, [ActionPhase::Released]);
    assert_eq!(actions, 0);
}

#[test]
fn nonmodal_unsupported_pointer_button_reaches_gameplay() {
    let (mut app, _) = build_unstarted_button_app();
    install_gameplay_capture(&mut app);
    app.configuration_world()
        .resource_mut::<ActionMap>()
        .bind_mouse(ActionId::new("inspect").unwrap(), MouseButton::Right);
    app.update().unwrap();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(frame, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("unsupported-button-test").unwrap())
        .unwrap();
    app.timeline()
        .admit_pointer(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(10.0, 10.0)),
        )
        .unwrap();
    for input in [
        ButtonDriverInput::Press(MouseButton::Right),
        ButtonDriverInput::Release(MouseButton::Right),
    ] {
        app.timeline()
            .admit_mouse_button(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                input,
            )
            .unwrap();
        app.update().unwrap();
    }
    let phases = app.world().resource::<CapturedGameplayPhases>().0.clone();
    let actions = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap()
        .len();
    drop(app);

    assert_eq!(phases, [ActionPhase::Started, ActionPhase::Released]);
    assert_eq!(actions, 0);
}

#[test]
fn nonmodal_unfocused_activation_reaches_gameplay() {
    let (mut app, _) = build_unstarted_button_app();
    install_gameplay_capture(&mut app);
    app.configuration_world()
        .resource_mut::<ActionMap>()
        .bind_key(ActionId::new("interact").unwrap(), KeyCode::Enter);
    app.update().unwrap();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(frame, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("unfocused-activation-test").unwrap())
        .unwrap();
    for input in [
        ButtonDriverInput::Press(KeyCode::Enter),
        ButtonDriverInput::Release(KeyCode::Enter),
    ] {
        app.timeline()
            .admit_keyboard(InputWindowId::new(11), InputSeatId::PRIMARY, input)
            .unwrap();
        app.update().unwrap();
    }
    let phases = app.world().resource::<CapturedGameplayPhases>().0.clone();
    let actions = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap()
        .len();
    drop(app);

    assert_eq!(phases, [ActionPhase::Started, ActionPhase::Released]);
    assert_eq!(actions, 0);
}

#[test]
fn initial_modal_presentation_commits_focus_for_the_surface_owner_seat() {
    let (mut app, _) = build_unstarted_button_app_with_layer(UiLayer::Modal);
    app.update().expect("initial modal frame should compile");
    let frame = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("initial modal frame should exist");
    let retry_key = expected_runtime_node("root", "retry");
    let palette = UiTheme::default().palette();
    let border = frame
        .paint()
        .iter()
        .find_map(|primitive| match primitive {
            UiPaintPrimitive::Rect {
                key: Some(actual),
                border: Some(border),
                ..
            } if actual == &retry_key => Some(*border),
            _ => None,
        })
        .expect("modal focus must be compiled into the candidate paint facts");
    assert_eq!(border.width(), 2.0);
    assert_eq!(border.color(), palette.focus);
    let cursor = app
        .register_consumer(UiActionConsumerId::new("modal-entry-focus-test").unwrap())
        .unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();

    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &retry_key)
        .expect("initial modal should expose its Retry control");
    assert!(
        retry.state().focused(),
        "modal entry should commit focus for the Surface Slot owner seat before input"
    );
    let reducer = app
        .world()
        .resource::<crate::runtime::UiRuntimeState>()
        .surface(surface_id())
        .unwrap()
        .reducer_states()
        .get(&retry_key)
        .copied()
        .expect("the presented modal must retain its staged Button reducer");
    assert!(reducer.focused());
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty(),
        "modal focus reconciliation must not emit an activation intent"
    );
}

#[test]
fn modal_endpoint_replacement_seeds_only_the_new_owner_seat() {
    let (mut app, _) = build_unstarted_button_app_with_layer(UiLayer::Modal);
    app.update().expect("initial modal frame should compile");
    let first = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("initial modal frame should exist");
    let first_receipt = app.present_frame(first, 1).unwrap();
    let retry_key = expected_runtime_node("root", "retry");
    assert!(
        ui_semantic_snapshot(app.world(), first_receipt, UiSeatId::PRIMARY)
            .unwrap()
            .nodes()
            .iter()
            .find(|node| node.key() == &retry_key)
            .unwrap()
            .state()
            .focused()
    );

    let next_input_seat = InputSeatId::new(1);
    let next_ui_seat = UiSeatId::new(next_input_seat.get());
    let current = active_ui_surface_slots(app.world()).unwrap()[0];
    app.update_surface(
        surface_id(),
        UiSurfaceEndpoint::new(
            InputWindowId::new(12),
            next_input_seat,
            InputPointerId::PRIMARY,
        ),
        current.transform(),
    )
    .unwrap();
    app.update()
        .expect("the successor endpoint modal frame should compile");
    let successor = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("the successor endpoint modal frame should exist");
    let successor_receipt = app.present_frame(successor, 2).unwrap();

    let next_owner = ui_semantic_snapshot(app.world(), successor_receipt, next_ui_seat).unwrap();
    assert!(
        next_owner
            .nodes()
            .iter()
            .find(|node| node.key() == &retry_key)
            .unwrap()
            .state()
            .focused(),
        "the successor endpoint owner seat should receive modal-entry focus"
    );
    assert!(matches!(
        ui_semantic_snapshot(app.world(), successor_receipt, UiSeatId::PRIMARY),
        Err(UiAdapterError::SeatOwnerMismatch {
            expected: 1,
            actual: 0,
        })
    ));
}

#[test]
fn actionable_semantics_without_an_exact_controlled_behavior_owner_are_rejected() {
    let mut app = ui_app();
    install_test_surface(app.configuration_world());
    let world = app.configuration_world();
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(TEST_DOMAIN, "root").unwrap(),
        ))
        .id();
    let orphan = world
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 100.0, 50.0)),
            UiRuntimeKey::new(TEST_DOMAIN, "orphan-action").unwrap(),
            UiSemantics::new(UiSemanticRole::Button, "Orphan action")
                .unwrap()
                .with_action(UiActionId::new("orphan-action").unwrap())
                .unwrap(),
        ))
        .id();
    place_ui_tree(world, root, &[orphan]);

    app.update()
        .expect("a typed candidate rejection should not abort the frame");

    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none()
    );
    let diagnostic = app
        .world()
        .resource::<crate::UiAdapterDiagnostics>()
        .as_slice()
        .iter()
        .find(|entry| entry.code() == "candidate-preparation-rejected")
        .expect("the missing behavior owner should be diagnosed");
    assert!(diagnostic.message().contains("controlled Button owner"));
}

#[test]
fn accessibility_actions_use_the_exact_presented_behavior_snapshot() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let button = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .expect("the controlled button should be advertised");
    let action = button.actions()[0].clone();
    let target = button.key().clone();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("accessibility-owner-test").unwrap())
        .unwrap();

    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::semantic_action(
            receipt,
            UiSeatId::PRIMARY,
            target.clone(),
            action.clone(),
        ),),
        UiRouteDisposition::Consumed(_)
    ));
    let pending = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].action(), &action);
    assert_eq!(pending[0].input_sequence(), None);
    assert_eq!(pending[0].source(), UiActivationSource::Accessibility);

    app.update_surface(
        surface_id(),
        test_endpoint(11),
        test_transform(200.0, 100.0, 400.0, 200.0, 1.0),
    )
    .unwrap();
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::semantic_action(
            receipt,
            UiSeatId::PRIMARY,
            target,
            action,
        ),),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::SurfaceGenerationMismatch)
    ));
}

#[test]
fn accessibility_rejects_a_foreign_seat_before_it_can_claim_session_state() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    let foreign_seat = UiSeatId::new(1);

    assert!(matches!(
        ui_semantic_snapshot(app.world(), receipt, foreign_seat),
        Err(UiAdapterError::SeatOwnerMismatch {
            expected: 0,
            actual: 1,
        })
    ));
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::semantic_action(
            receipt,
            foreign_seat,
            retry.key().clone(),
            retry.actions()[0].clone(),
        ),),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::SeatOwnerMismatch)
    ));

    assert!(
        dispatch_pointer_click(&mut app, UiRouteStamp::from_receipt(receipt))
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );
}

#[test]
fn foreign_pointer_input_is_rejected_closed_by_the_surface_owner() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(frame, 1).unwrap();
    app.timeline()
        .admit_pointer(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::new(9),
            PointerDriverInput::Moved(Vec2::new(10.0, 10.0)),
        )
        .unwrap();

    app.update()
        .expect("foreign pointer rejection should not abort the frame");

    assert!(matches!(
        app.world()
            .resource::<UiRouteRejectionLog>()
            .as_slice()
            .first()
            .map(|record| record.rejection()),
        Some(UiRouteRejection::PointerOwnerMismatch)
    ));
}

#[test]
fn letterbox_input_is_not_clamped_and_cancels_existing_pointer_facts() {
    let (mut app, _button) = build_unstarted_button_app();
    app.update_surface(
        surface_id(),
        test_endpoint(11),
        test_transform(200.0, 100.0, 300.0, 100.0, 1.0),
    )
    .unwrap();
    app.update().expect("letterboxed frame should compile");
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("letterbox-input-test").unwrap())
        .unwrap();

    {
        let mut timeline = app.timeline();
        timeline
            .admit_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(25.0, 10.0)),
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
        timeline
            .admit_mouse_button(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Release(MouseButton::Left),
            )
            .unwrap();
    }
    app.update()
        .expect("unowned letterbox input should pass through without clamping");
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );

    {
        let mut timeline = app.timeline();
        timeline
            .admit_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(60.0, 10.0)),
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
    app.update().expect("inside pointer press should route");
    let pressed = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    assert!(
        pressed
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .expect("the controlled button should remain semantic")
            .state()
            .pressed()
    );

    app.timeline()
        .admit_pointer(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(25.0, 10.0)),
        )
        .unwrap();
    app.update()
        .expect("leaving content should consume one pointer cancellation");
    let cancelled = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    assert!(
        !cancelled
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .expect("the controlled button should remain semantic")
            .state()
            .pressed()
    );

    app.timeline()
        .admit_mouse_button(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();
    app.update()
        .expect("post-cancellation letterbox release should pass through");
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn modal_letterbox_input_blocks_underlay_before_and_after_pointer_cancellation() {
    let (mut app, _button) = build_unstarted_button_app();
    let root = app
        .world()
        .iter_entities()
        .find(|entity| entity.contains::<UiRoot>())
        .unwrap()
        .id();
    app.set_root(root, UiRoot::new(UiLayer::Modal));
    app.update_surface(
        surface_id(),
        test_endpoint(11),
        test_transform(200.0, 100.0, 300.0, 100.0, 1.0),
    )
    .unwrap();
    app.update().unwrap();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("modal-letterbox-test").unwrap())
        .unwrap();

    app.timeline()
        .admit_pointer(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(25.0, 10.0)),
        )
        .unwrap();
    app.update().unwrap();
    assert!(matches!(
        app.world()
            .resource::<UiRouteRejectionLog>()
            .as_slice()
            .first()
            .map(|record| record.rejection()),
        Some(UiRouteRejection::UnhandledInputBlocked)
    ));

    {
        let mut timeline = app.timeline();
        timeline
            .admit_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(60.0, 10.0)),
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
    assert!(
        ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY)
            .unwrap()
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .pressed()
    );

    app.timeline()
        .admit_pointer(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(25.0, 10.0)),
        )
        .unwrap();
    app.update().unwrap();
    assert!(
        !ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY)
            .unwrap()
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .pressed()
    );

    let rejection_count = app
        .world()
        .resource::<UiRouteRejectionLog>()
        .as_slice()
        .len();
    let release = app
        .timeline()
        .admit_mouse_button(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();
    let release_sequence = release
        .first_sequence()
        .expect("the modal release should enter the exact UI route");
    assert_eq!(release.last_sequence(), Some(release_sequence));
    app.update().unwrap();
    let rejections = app.world().resource::<UiRouteRejectionLog>();
    assert_eq!(rejections.as_slice().len(), rejection_count + 1);
    let release_rejection = rejections
        .as_slice()
        .first()
        .expect("the modal release should record a new fail-closed decision");
    assert_eq!(release_rejection.input_sequence(), Some(release_sequence));
    assert!(matches!(
        release_rejection.rejection(),
        UiRouteRejection::UnhandledInputBlocked
    ));
    assert!(app.world().resource::<InputTimeline>().cursors().ui() >= release_sequence);
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn focused_disabled_button_keeps_a_non_color_focus_indicator() {
    let (mut app, button) = build_button_app();
    let initial = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("initial frame should compile");
    app.present_frame(initial, 1).unwrap();
    assert!(matches!(
        app.dispatch_input(
            route_stamp(1),
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
    app.set_button(button, disabled);
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

    app.present_frame(disabled_frame, 2).unwrap();
    let semantics =
        ui_semantic_snapshot(app.world(), test_receipt(app.world(), 2), UiSeatId::PRIMARY).unwrap();
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
        .configuration_world()
        .spawn(button_bundle(
            "secondary",
            "secondary-action",
            UiColor::rgba(0.3, 0.7, 0.4, 1.0),
        ))
        .id();
    app.place_entity(valid_button, child_last(root)).unwrap();
    app.update().expect("valid UI root should compile");
    let initial = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("valid UI root should publish");
    assert_eq!(initial.semantics().len(), 2);

    app.remove_visual(invalid_button);
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
    install_test_surface(app.configuration_world());
    let world = app.configuration_world();
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
    app.remove_visual(button);
    app.update()
        .expect("a compatible modal visual should be retained");
    let retained = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("retained modal candidate should publish");

    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert!(retained.blocks_unhandled_input());
    assert!(retained.hit_regions().is_empty());
    assert!(retained.focus_order().is_empty());
    assert!(retained.semantics().is_empty());

    app.present_frame(retained, 1).unwrap();
    app.timeline()
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
    app.place_invalid_modal(
        "modal-without-fallback".into(),
        "invalid-modal-button".into(),
    );

    app.update()
        .expect("modal candidate rejection should remain adapter-local");
    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        app.present_frame(stale, 1,),
        Err(UiAdapterError::FrameNotLatest { surface: 7 })
    ));
}

#[test]
fn missing_root_identity_rejects_the_whole_projection() {
    let (mut app, _) = build_button_app();
    let stale = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("baseline candidate should exist");
    let missing_identity = app.spawn_missing_identity_root();
    app.place_entity(missing_identity, root_last()).unwrap();

    app.update()
        .expect("identity rejection should remain adapter-local");
    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        app.present_frame(stale, 1,),
        Err(UiAdapterError::FrameNotLatest { surface: 7 })
    ));
}

#[test]
fn whole_candidate_rejection_discards_new_work_but_preserves_presented_authority() {
    let (mut app, button) = build_button_app();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("candidate-rejection-test").unwrap())
        .unwrap();
    let presented = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("baseline candidate should exist");
    app.present_frame(presented, 1).unwrap();

    app.set_button(
        button,
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
    app.place_invalid_modal("later-invalid-modal".into(), "later-invalid-button".into());

    app.update()
        .expect("whole-candidate rejection should preserve the presented session");
    assert!(
        latest_ui_frame(app.world(), surface_id())
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        app.present_frame(unpresented, 2,),
        Err(UiAdapterError::FrameNotLatest { surface: 7 })
    ));

    assert!(
        dispatch_pointer_click(&mut app, route_stamp(1))
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

fn dispatch_pointer_click(app: &mut UiTestApp, route: UiRouteStamp) -> Vec<UiRouteDisposition> {
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
    .map(|input| app.dispatch_input(route, input))
    .collect()
}

#[test]
fn presented_button_dispatch_publishes_one_typed_action_and_semantics() {
    let (mut app, _button) = build_button_app();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("adapter-test").unwrap())
        .expect("test action consumer should register");
    let frame = latest_ui_frame(app.world(), surface_id())
        .expect("UI runtime should be available")
        .expect("UI frame should compile");
    let route = route_stamp(1);
    let pointer = UiPointerId::PRIMARY;
    let seat = UiSeatId::PRIMARY;
    let point = UiPoint::new(10.0, 10.0);

    assert!(matches!(
        app.dispatch_input(
            route,
            UiInputEvent::PointerMoved {
                seat,
                pointer,
                position: point,
            },
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::PresentedHistory(_))
    ));

    let receipt = app
        .present_frame(Arc::clone(&frame), 1)
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
            app.dispatch_input(route, input,),
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

    let semantics = ui_semantic_snapshot(app.world(), test_receipt(app.world(), 1), seat)
        .expect("presented frame should publish semantics");
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &event_target)
        .expect("Retry semantics should share the action key");
    assert_eq!(retry.name(), "retry");
    assert!(retry.state().enabled());

    app.acknowledge(cursor, event_sequence);
}

#[test]
fn resize_while_pressed_cancels_without_activating_the_button() {
    let (mut app, _) = build_button_app();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("resize-cancellation-test").unwrap())
        .unwrap();
    let frame = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("test frame should compile");
    app.present_frame(frame, 1).unwrap();

    {
        let mut timeline = app.timeline();
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
    let pressed =
        ui_semantic_snapshot(app.world(), test_receipt(app.world(), 1), UiSeatId::PRIMARY).unwrap();
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
        .timeline()
        .admit_window_event(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputWindowEvent::Resized {
                logical_size: Vec2::new(640.0, 360.0),
            },
        )
        .unwrap();
    assert_eq!(admission.admitted(), 1);
    app.update().unwrap();

    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
    let cancelled =
        ui_semantic_snapshot(app.world(), test_receipt(app.world(), 1), UiSeatId::PRIMARY).unwrap();
    assert!(
        !cancelled
            .nodes()
            .iter()
            .find(|node| node.key() == &expected_runtime_node("root", "retry"))
            .unwrap()
            .state()
            .pressed()
    );

    let release = app
        .timeline()
        .admit_mouse_button(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();
    assert_eq!(release.admitted(), 1);
    assert!(
        !app.world()
            .resource::<InputTimeline>()
            .entries()
            .last()
            .expect("the physical release should remain observable")
            .route()
            .is_ui_routable()
    );
    app.update().unwrap();
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unpresented_candidate_cannot_replace_presented_behavior_authority() {
    let (mut app, button) = build_button_app();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("presented-authority-test").unwrap())
        .unwrap();
    let first = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("first candidate should compile");
    app.present_frame(Arc::clone(&first), 1).unwrap();

    app.set_button(
        button,
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
        dispatch_pointer_click(&mut app, route_stamp(1))
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
    let cursor = app.acknowledge(cursor, first_sequence);

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

    app.present_frame(Arc::clone(&second), 2).unwrap();
    assert!(
        dispatch_pointer_click(&mut app, route_stamp(1))
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
        .register_consumer(UiActionConsumerId::new("equivalent-presentation-test").unwrap())
        .unwrap();
    let initial = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("initial frame should compile");
    app.present_frame(initial, 10).unwrap();
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
            app.dispatch_input(route_stamp(10), input,),
            UiRouteDisposition::Consumed(_)
        ));
    }

    app.update()
        .expect("equivalent pressed-state frame should compile");
    let equivalent = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("equivalent frame should exist");
    app.present_frame(equivalent, 11).unwrap();
    let semantics = ui_semantic_snapshot(
        app.world(),
        test_receipt(app.world(), 11),
        UiSeatId::PRIMARY,
    )
    .unwrap();
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
        app.dispatch_input(
            route_stamp(11),
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
        app.dispatch_input(
            route_stamp(11),
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
        .register_consumer(UiActionConsumerId::new("label-successor-test").unwrap())
        .unwrap();
    let initial = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(initial, 1).unwrap();
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
            app.dispatch_input(route_stamp(1), input,),
            UiRouteDisposition::Consumed(_)
        ));
    }

    app.set_button(
        button,
        crate::UiButton::new("again", UiActionId::new("retry-draft").unwrap(), 16.0).unwrap(),
    );
    app.update().expect("label successor should compile");
    let successor = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(successor, 2).unwrap();
    assert!(
        ui_semantic_snapshot(app.world(), test_receipt(app.world(), 2), UiSeatId::PRIMARY)
            .unwrap()
            .nodes()
            .iter()
            .find(|node| node.key() == &expected_runtime_node("root", "retry"))
            .unwrap()
            .state()
            .pressed()
    );
    assert!(matches!(
        app.dispatch_input(
            route_stamp(2),
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
        .register_consumer(UiActionConsumerId::new("changed-behavior-test").unwrap())
        .unwrap();
    let first = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("initial frame should compile");
    app.present_frame(first, 1).unwrap();
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
            app.dispatch_input(route_stamp(1), input,),
            UiRouteDisposition::Consumed(_)
        ));
    }

    app.set_button(
        button,
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
    app.present_frame(changed, 2).unwrap();

    let semantics =
        ui_semantic_snapshot(app.world(), test_receipt(app.world(), 2), UiSeatId::PRIMARY).unwrap();
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
        app.dispatch_input(
            route_stamp(2),
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
        app.dispatch_input(
            route_stamp(1),
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
        app.dispatch_input(
            route_stamp(2),
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
        dispatch_pointer_click(&mut app, route_stamp(2))
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
    app.present_frame(Arc::clone(&first), 1).unwrap();
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
            app.dispatch_input(route_stamp(1), input,),
            UiRouteDisposition::Consumed(_)
        ));
    }

    let previous = HierarchyPlacement::current(app.world(), button).unwrap();
    app.remove_entity(button, previous).unwrap();
    app.update().expect("retired-button frame should compile");
    let second = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("retired-button frame should exist");
    app.present_frame(second, 2).unwrap();

    let retired_move = UiInputEvent::PointerMoved {
        seat: UiSeatId::PRIMARY,
        pointer: UiPointerId::PRIMARY,
        position: point,
    };
    assert!(matches!(
        app.dispatch_input(route_stamp(1), retired_move.clone(),),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::NonActivation(
            UiNonActivation::TargetRetired { .. }
        ))
    ));
    assert!(matches!(
        app.dispatch_input(route_stamp(1), retired_move,),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::NonActivation(
            UiNonActivation::TargetRetired { .. }
        ))
    ));

    app.replace_button_key(
        button,
        UiRuntimeKey::new(TEST_DOMAIN, "replacement").unwrap(),
        crate::UiButton::new(
            "replacement",
            UiActionId::new("replacement-action").unwrap(),
            16.0,
        )
        .unwrap(),
    );
    app.place_entity(button, child_last(root)).unwrap();
    app.update().expect("replacement frame should compile");
    let third = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("replacement frame should exist");
    app.present_frame(third, 3).unwrap();

    assert!(
        dispatch_pointer_click(&mut app, route_stamp(3))
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );
}

#[test]
fn presentation_removal_cannot_resurrect_interaction_for_the_same_stable_key() {
    let (mut app, button) = build_button_app();
    let cursor = app
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
    app.present_frame(first, 1).unwrap();

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
            app.dispatch_input(route_stamp(1), input,),
            UiRouteDisposition::Consumed(_)
        ));
    }

    let previous = HierarchyPlacement::current(app.world(), button).unwrap();
    app.remove_entity(button, previous).unwrap();
    app.update().expect("removed-button frame should compile");
    let removed = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("removed-button frame should exist");
    app.present_frame(removed, 2).unwrap();
    assert!(
        !app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .reducer_states()
            .contains_key(&expected_runtime_node("root", "retry"))
    );

    app.place_entity(button, child_last(root)).unwrap();
    app.update()
        .expect("reinserted-button frame should compile");
    let reinserted = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .expect("reinserted-button frame should exist");
    app.present_frame(reinserted, 3).unwrap();
    assert!(
        !app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .reducer_states()
            .contains_key(&expected_runtime_node("root", "retry"))
    );

    let semantics =
        ui_semantic_snapshot(app.world(), test_receipt(app.world(), 3), UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &expected_runtime_node("root", "retry"))
        .unwrap();
    assert!(!retry.state().focused());
    assert!(!retry.state().pressed());
    assert!(matches!(
        app.dispatch_input(
            route_stamp(3),
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
    app.present_frame(frame, 1).unwrap();
    let admission = app
        .timeline()
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
    app.present_frame(initial, 1).unwrap();

    let root = app
        .world()
        .iter_entities()
        .find(|entity| entity.contains::<UiRoot>())
        .expect("test UI root should exist")
        .id();
    app.set_root(root, UiRoot::new(UiLayer::Modal));
    app.update().expect("modal successor should compile");
    let modal = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(modal, 2).unwrap();

    let state = app.world().resource::<crate::runtime::UiRuntimeState>();
    let historical = state
        .surface(surface_id())
        .unwrap()
        .resolve_presented(route_stamp(1))
        .unwrap();
    assert!(historical.blocks_unhandled_input());
}

#[test]
fn interaction_and_reducer_visual_state_are_isolated_per_surface() {
    let (mut app, _button) = build_button_app();
    let second_surface = UiSurfaceId::new(8).unwrap();
    app.install_surface(test_slot(second_surface, 12, 200.0, 100.0))
        .unwrap();
    app.update()
        .expect("both surface candidates should compile");
    let first_before = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let second_before = latest_ui_frame(app.world(), second_surface)
        .unwrap()
        .unwrap();
    assert_eq!(first_before.paint(), second_before.paint());
    app.present_frame(first_before.clone(), 1).unwrap();
    app.present_frame(second_before.clone(), 1).unwrap();

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
            app.dispatch_input(route_stamp(1), input,),
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
fn distinct_surface_routes_keep_geometry_focus_capture_and_actions_isolated() {
    let (mut app, _button) = build_button_app();
    let second_surface = UiSurfaceId::new(8).unwrap();
    app.install_surface(
        UiSurfaceSlot::new(
            second_surface,
            test_endpoint(12),
            test_transform(200.0, 100.0, 400.0, 100.0, 1.0),
        )
        .unwrap(),
    )
    .unwrap();
    app.update().unwrap();
    let first = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let second = latest_ui_frame(app.world(), second_surface)
        .unwrap()
        .unwrap();
    let first_receipt = app.present_frame(first, 1).unwrap();
    let second_receipt = app.present_frame(second, 1).unwrap();
    let first_initial =
        ui_semantic_snapshot(app.world(), first_receipt, UiSeatId::PRIMARY).unwrap();
    let second_initial =
        ui_semantic_snapshot(app.world(), second_receipt, UiSeatId::PRIMARY).unwrap();
    let second_retry = second_initial
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap()
        .key()
        .clone();
    assert!(
        !first_initial
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .focused()
    );
    assert!(
        !second_initial
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .focused()
    );
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::focus_target(
            second_receipt,
            UiSeatId::PRIMARY,
            second_retry,
        ),),
        UiRouteDisposition::Consumed(_)
    ));
    let first_after_second_focus =
        ui_semantic_snapshot(app.world(), first_receipt, UiSeatId::PRIMARY).unwrap();
    let second_after_second_focus =
        ui_semantic_snapshot(app.world(), second_receipt, UiSeatId::PRIMARY).unwrap();
    assert!(
        !first_after_second_focus
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .focused()
    );
    assert!(
        second_after_second_focus
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .focused()
    );
    let cursor = app
        .register_consumer(UiActionConsumerId::new("two-surface-route-test").unwrap())
        .unwrap();

    {
        let mut timeline = app.timeline();
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
        timeline
            .admit_pointer(
                InputWindowId::new(12),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(110.0, 10.0)),
            )
            .unwrap();
        timeline
            .admit_mouse_button(
                InputWindowId::new(12),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
    }
    app.update().unwrap();
    let first_semantics =
        ui_semantic_snapshot(app.world(), first_receipt, UiSeatId::PRIMARY).unwrap();
    let second_semantics =
        ui_semantic_snapshot(app.world(), second_receipt, UiSeatId::PRIMARY).unwrap();
    assert!(
        first_semantics
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .pressed()
    );
    assert!(
        first_semantics
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .focused()
    );
    assert!(
        second_semantics
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .pressed()
    );
    assert!(
        second_semantics
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .focused()
    );

    app.retire_surface(surface_id()).unwrap().unwrap();
    let surviving_semantics =
        ui_semantic_snapshot(app.world(), second_receipt, UiSeatId::PRIMARY).unwrap();
    assert!(
        surviving_semantics
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .focused()
    );
    let retained_before_retired_release = app.world().resource::<InputTimeline>().retained_len();
    let retired_release = app
        .timeline()
        .admit_mouse_button(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();
    assert!(retired_release.is_ignored());
    assert_eq!(
        app.world().resource::<InputTimeline>().retained_len(),
        retained_before_retired_release
    );
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
    app.timeline()
        .admit_mouse_button(
            InputWindowId::new(12),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();
    app.update().unwrap();
    let pending = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].surface_id(), second_surface);
    assert_eq!(pending[0].presentation_epoch(), 1);
}

#[test]
fn retirement_route_mismatch_preserves_surface_session_and_identity() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    let replacement_route = app
        .timeline()
        .publish_presented_route(InputPresentedRoute::new(
            InputWindowId::new(11),
            InputSurfaceId::new(surface_id().get()),
            InputSeatId::PRIMARY,
            InputPresentationEpoch::new(2).unwrap(),
            InputSlotGeneration::new(receipt.slot_generation().get()).unwrap(),
            InputRouteGeneration::new(receipt.route_revision().get()).unwrap(),
            InputGeometryGeneration::new(receipt.geometry_generation().get()).unwrap(),
        ))
        .unwrap();
    assert_ne!(replacement_route.presentation_epoch().unwrap().get(), 1);

    assert!(matches!(
        app.retire_surface(surface_id()),
        Err(UiAdapterError::Input(
            nara_input::InputIngressError::PresentedRouteMismatch {
                window,
                seat,
            }
        )) if window == InputWindowId::new(11) && seat == InputSeatId::PRIMARY
    ));
    let state = app.world().resource::<crate::runtime::UiRuntimeState>();
    assert!(state.surface(surface_id()).is_some());
    assert!(!state.is_retired(surface_id()));
    assert_eq!(active_ui_surface_slots(app.world()).unwrap().len(), 1);
    assert!(ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).is_ok());
}

#[test]
fn surface_retirement_releases_only_gameplay_owned_held_edges() {
    let mut gameplay_owned = build_unstarted_button_app().0;
    install_gameplay_capture(&mut gameplay_owned);
    gameplay_owned
        .update_surface(
            surface_id(),
            test_endpoint(11),
            test_transform(200.0, 100.0, 300.0, 100.0, 1.0),
        )
        .unwrap();
    gameplay_owned.update().unwrap();
    let frame = latest_ui_frame(gameplay_owned.world(), surface_id())
        .unwrap()
        .unwrap();
    gameplay_owned.present_frame(frame, 1).unwrap();
    {
        let mut timeline = gameplay_owned.timeline();
        timeline
            .admit_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(25.0, 10.0)),
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
    gameplay_owned.update().unwrap();
    assert_eq!(
        gameplay_owned
            .world()
            .resource::<CapturedGameplayPhases>()
            .0,
        [ActionPhase::Started]
    );
    gameplay_owned.clear_gameplay_capture();

    gameplay_owned
        .retire_surface(surface_id())
        .unwrap()
        .unwrap();
    gameplay_owned.update().unwrap();
    assert_eq!(
        gameplay_owned
            .world()
            .resource::<CapturedGameplayPhases>()
            .0,
        [ActionPhase::Released]
    );

    let mut ui_owned = build_unstarted_button_app().0;
    install_gameplay_capture(&mut ui_owned);
    ui_owned.update().unwrap();
    let frame = latest_ui_frame(ui_owned.world(), surface_id())
        .unwrap()
        .unwrap();
    ui_owned.present_frame(frame, 1).unwrap();
    {
        let mut timeline = ui_owned.timeline();
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
    ui_owned.update().unwrap();
    assert!(
        ui_owned
            .world()
            .resource::<CapturedGameplayPhases>()
            .0
            .is_empty()
    );

    ui_owned.retire_surface(surface_id()).unwrap().unwrap();
    ui_owned.update().unwrap();
    assert!(
        ui_owned
            .world()
            .resource::<CapturedGameplayPhases>()
            .0
            .is_empty()
    );
}

#[test]
fn full_input_timeline_aborts_coordinated_surface_retirement_without_partial_state() {
    let (mut app, _button) = build_button_app();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    app.update().unwrap();
    let window = InputWindowId::new(11);
    let seat = InputSeatId::PRIMARY;
    let pointer = InputPointerId::PRIMARY;
    let expected_route = app
        .timeline()
        .publish_presented_route(InputPresentedRoute::new(
            window,
            InputSurfaceId::new(surface_id().get()),
            seat,
            InputPresentationEpoch::new(receipt.presentation_epoch()).unwrap(),
            InputSlotGeneration::new(receipt.slot_generation().get()).unwrap(),
            InputRouteGeneration::new(receipt.route_revision().get()).unwrap(),
            InputGeometryGeneration::new(receipt.geometry_generation().get()).unwrap(),
        ))
        .unwrap();
    {
        let mut timeline = app.timeline();
        timeline
            .admit_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Moved(Vec2::new(20.0, 20.0)),
            )
            .unwrap();
        timeline
            .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap();
        timeline
            .admit_mouse_button(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
        while timeline.retained_len() < MAX_INPUT_TIMELINE_ENTRIES {
            let button = u16::try_from(timeline.retained_len()).unwrap();
            timeline
                .admit_gamepad_button(window, seat, 0, button, ButtonTransitionPhase::Pressed)
                .unwrap();
        }
    }

    assert!(matches!(
        app.retire_surface(surface_id()),
        Err(UiAdapterError::Input(
            nara_input::InputIngressError::CapacityExceeded {
                route_invalidated: false,
                ..
            }
        ))
    ));
    assert!(
        app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .is_some()
    );
    assert!(ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).is_ok());

    let rebound = app
        .timeline()
        .publish_presented_route(InputPresentedRoute::new(
            window,
            expected_route.surface().unwrap(),
            seat,
            InputPresentationEpoch::new(expected_route.presentation_epoch().unwrap().get())
                .unwrap(),
            InputSlotGeneration::new(expected_route.slot_generation().unwrap().get()).unwrap(),
            InputRouteGeneration::new(expected_route.route_generation().unwrap().get()).unwrap(),
            InputGeometryGeneration::new(expected_route.geometry_generation().unwrap().get())
                .unwrap(),
        ))
        .unwrap();
    assert_eq!(
        rebound.binding_generation(),
        expected_route.binding_generation()
    );
}

#[test]
fn intent_backpressure_drops_activation_but_commits_the_physical_release() {
    let (mut app, _button) = build_button_app();
    app.replace_journal(UiActionJournal::with_limits(1, 1).unwrap());
    let cursor = app
        .register_consumer(UiActionConsumerId::new("backpressure-test").unwrap())
        .unwrap();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    app.present_frame(frame, 1).unwrap();
    let route = route_stamp(1);
    assert!(
        dispatch_pointer_click(&mut app, route)
            .iter()
            .all(|disposition| matches!(disposition, UiRouteDisposition::Consumed(_)))
    );

    let point = UiPoint::new(10.0, 10.0);
    assert!(matches!(
        app.dispatch_input(
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
        app.dispatch_input(
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
    let semantics =
        ui_semantic_snapshot(app.world(), test_receipt(app.world(), 1), UiSeatId::PRIMARY).unwrap();
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
    let cursor = app.acknowledge(cursor, first_sequence);
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
struct SlotReplacementProbe {
    transform: UiSurfaceContentTransform,
    completed: bool,
}

fn present_then_replace_surface_slot(world: &mut World) {
    if world.resource::<SlotReplacementProbe>().completed {
        return;
    }
    let frame = latest_ui_frame(world, surface_id())
        .unwrap()
        .expect("first runtime frame should compile before the Last-stage test presenter");
    commit_ui_presentation(world, frame, 1).unwrap();
    let transform = world.resource::<SlotReplacementProbe>().transform;
    update_ui_surface_slot(world, surface_id(), test_endpoint(11), transform).unwrap();
    world.resource_mut::<SlotReplacementProbe>().completed = true;
}

#[test]
fn surface_slot_replacement_invalidates_the_retained_route_generation() {
    let (mut app, _button) = build_unstarted_button_app();
    app.configuration()
        .insert_resource(SlotReplacementProbe {
            transform: test_transform(200.0, 100.0, 400.0, 200.0, 1.0),
            completed: false,
        })
        .unwrap()
        .add_systems(CoreStage::Last, present_then_replace_surface_slot)
        .unwrap();
    app.update().unwrap();
    assert!(app.world().resource::<SlotReplacementProbe>().completed);
    let current = active_ui_surface_slots(app.world()).unwrap()[0];
    assert_eq!(current.slot_generation(), 2);
    assert_eq!(current.geometry_generation(), 2);
    assert_eq!(current.route_generation(), 2);
    assert_eq!(current.presentation_generation(), 2);
    assert!(matches!(
        app.dispatch_input(
            route_stamp(1),
            UiInputEvent::PointerMoved {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                position: UiPoint::new(10.0, 10.0),
            },
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::SurfaceGenerationMismatch)
    ));
}

#[test]
fn consecutive_transform_then_endpoint_updates_do_not_transfer_the_old_seat_session() {
    let (mut app, _button) = build_button_app();
    let first = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let first_receipt = app.present_frame(first, 1).unwrap();
    assert!(matches!(
        app.dispatch_input(
            UiRouteStamp::from_receipt(first_receipt),
            UiInputEvent::Navigate {
                seat: UiSeatId::PRIMARY,
                direction: nara_ui_kernel::UiNavigation::Next,
            },
        ),
        UiRouteDisposition::Consumed(_)
    ));
    assert!(
        app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .reducer_states()
            .values()
            .any(|state| state.focused())
    );

    let resized = app
        .update_surface(
            surface_id(),
            test_endpoint(11),
            test_transform(200.0, 100.0, 400.0, 200.0, 1.0),
        )
        .unwrap();
    let next_endpoint = UiSurfaceEndpoint::new(
        InputWindowId::new(12),
        InputSeatId::new(1),
        InputPointerId::PRIMARY,
    );
    let moved = app
        .update_surface(surface_id(), next_endpoint, resized.transform())
        .unwrap();
    assert_eq!(moved.slot_generation(), 3);
    assert!(
        app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .reducer_states()
            .is_empty()
    );

    app.update().unwrap();
    let successor = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let successor_receipt = app.present_frame(successor, 2).unwrap();
    let next_seat = UiSeatId::new(1);
    let semantics = ui_semantic_snapshot(app.world(), successor_receipt, next_seat).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    assert!(!retry.state().focused());
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::focus_target(
            successor_receipt,
            next_seat,
            retry.key().clone(),
        ),),
        UiRouteDisposition::Consumed(_)
    ));
    let focused = ui_semantic_snapshot(app.world(), successor_receipt, next_seat).unwrap();
    assert!(
        focused
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .focused()
    );
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
        .configuration_world()
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("pause-safe-test").unwrap())
        .unwrap();
    app.configuration()
        .insert_resource(PauseSafeActionConsumer {
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

    let mut runtime = RuntimeInstance::start(app.into_configuration()).unwrap();
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
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
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

    app.set_node(
        button,
        UiNode::new(UiStyle::absolute(10.0, 5.0, 120.0, 60.0)),
    );
    app.update().expect("second UI test frame should update");
    let second = latest_ui_frame(app.world(), surface_id())
        .expect("UI runtime should be available")
        .expect("second UI frame should compile");
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(matches!(
        app.present_frame(first, 1,),
        Err(UiAdapterError::FrameNotLatest { surface: 7 })
    ));
    app.present_frame(second, 1)
        .expect("latest frame should commit");

    let retirement = app
        .retire_surface(surface_id())
        .expect("surface retirement should succeed")
        .expect("installed surface should produce an exact tombstone");
    assert_eq!(retirement.surface_id(), surface_id());
    assert_eq!(retirement.slot_generation(), 1);
    assert_eq!(retirement.retirement_generation(), 2);
    assert!(matches!(
        app.dispatch_input(
            route_stamp(1),
            UiInputEvent::PointerMoved {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                position: UiPoint::new(10.0, 10.0),
            },
        ),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::RetiredSurface { surface: 7 })
    ));
    let slot = test_slot(surface_id(), 11, 200.0, 100.0);
    assert!(matches!(
        app.install_surface(slot,),
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
    install_test_surface(app.configuration_world());
    let source = AssetRef::path("prefabs/ui-root.ron").expect("test asset ref should be valid");
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        nara_scene::SourceDocumentKey::named(
            "58735ed4-a767-46cc-a257-168970b29c68".parse().unwrap(),
            "ui-test.root",
            nara_scene::SourceDocumentKind::Prefab,
        )
        .unwrap(),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("root"))]),
    );
    let document = SceneDocument::from_ordered_roots([
        prefab_anchor("left", source.clone()),
        prefab_anchor("right", source),
    ]);
    app.configuration()
        .add_systems(nara_app::StartupStage::Scene, move |world: &mut World| {
            let registry = ComponentRegistry::from_snapshot(
                nara_reflect::component_registry(world)
                    .unwrap()
                    .snapshot()
                    .unwrap(),
            );
            let expansion = document.expand_prefabs(&registry, &resolver);
            assert!(
                !expansion.diagnostics.has_errors(),
                "{:?}",
                expansion.diagnostics
            );
            let candidate = expansion
                .candidate
                .unwrap()
                .bind_source(scene_source_key("repeated-roots"));
            let report = nara_scene::spawn_scene(
                world,
                &registry,
                &candidate,
                &scene_occurrence("roots"),
                None,
            );
            assert!(!report.diagnostics.has_errors());

            let projected = world
                .iter_entities()
                .filter_map(|entity| {
                    entity
                        .get::<SceneEntitySource>()
                        .filter(|source| source.provenance().local_entity_id().as_str() == "root")
                        .map(|_| entity.id())
                })
                .collect::<Vec<_>>();
            for entity in projected {
                world
                    .entity_mut(entity)
                    .insert((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())));
            }
        })
        .unwrap();
    app.update().expect("prefab UI test frame should update");

    let projected = app
        .world()
        .iter_entities()
        .filter_map(|entity| {
            entity
                .get::<SceneEntitySource>()
                .filter(|source| source.provenance().local_entity_id().as_str() == "root")
                .map(|source| (entity.id(), source.clone()))
        })
        .collect::<Vec<_>>();
    assert_eq!(projected.len(), 2);
    for (_, source) in &projected {
        assert_eq!(
            source.containing_source(),
            &scene_source_key("repeated-roots"),
        );
        assert_eq!(source.occurrence(), &scene_occurrence("roots"),);
    }
    assert!(
        projected
            .iter()
            .all(|(_, source)| source.provenance().projection_path().is_some())
    );
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

fn scene_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

#[test]
fn root_capacity_is_admitted_before_constructing_the_excess_root_identity() {
    let mut app = ui_app();
    let world = app.configuration_world();
    install_test_surface(world);
    for index in 0..65 {
        let root = world
            .spawn((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())))
            .id();
        if index < 64 {
            world
                .entity_mut(root)
                .insert(UiRuntimeKey::new("capacity", format!("root-{index}")).unwrap());
        }
        place_ui_tree(world, root, &[]);
    }
    app.update().unwrap();
    let diagnostics = app
        .world()
        .resource::<crate::UiAdapterDiagnostics>()
        .as_slice()
        .to_vec();
    let absent = latest_ui_frame(app.world(), surface_id())
        .unwrap()
        .is_none();
    drop(app);
    assert!(absent);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == "snapshot-capacity-rejected")
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code() != "root-key-missing")
    );
}

#[test]
fn persistent_sources_and_occurrences_isolate_presented_focus_press_and_actions() {
    let mut app = ui_app();
    install_test_surface(app.configuration_world());
    app.configuration()
        .add_systems(nara_app::StartupStage::Scene, |world: &mut World| {
            let registry = ComponentRegistry::from_snapshot(
                nara_reflect::component_registry(world)
                    .unwrap()
                    .snapshot()
                    .unwrap(),
            );
            let mut roots = Vec::new();
            let mut buttons = Vec::new();
            for (index, (source, slot)) in [("panel", "one"), ("panel", "two"), ("other", "three")]
                .into_iter()
                .enumerate()
            {
                let document = SceneDocument::new(
                    [scene_id("root")],
                    [
                        SceneEntityRecord::new(scene_id("root")).with_children([scene_id("retry")]),
                        SceneEntityRecord::new(scene_id("retry")),
                    ],
                );
                let candidate = nara_scene::ExpandedSceneCandidate::plain(
                    scene_source_key(source),
                    Arc::new(document),
                );
                let report = nara_scene::spawn_scene(
                    world,
                    &registry,
                    &candidate,
                    &scene_occurrence(slot),
                    None,
                );
                assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
                let instance = report.instance.unwrap();
                let entities = world
                    .iter_entities()
                    .filter_map(|entity| {
                        entity
                            .get::<SceneEntitySource>()
                            .filter(|source| source.instance_id == instance.instance_id())
                            .map(|source| {
                                (entity.id(), source.provenance().local_entity_id().clone())
                            })
                    })
                    .collect::<Vec<_>>();
                for (entity, local) in entities {
                    if local.as_str() == "root" {
                        roots.push(entity);
                        world.entity_mut(entity).insert((
                            UiRoot::new(UiLayer::Base),
                            UiNode::new(UiStyle::fill()),
                            UiRuntimeKey::new(TEST_DOMAIN, "root").unwrap(),
                        ));
                    } else {
                        buttons.push(entity);
                        world.entity_mut(entity).insert(button_bundle(
                            "retry",
                            "retry-action",
                            UiColor::rgba(0.2, 0.4, 0.8, 1.0),
                        ));
                        world
                            .entity_mut(entity)
                            .insert(UiNode::new(UiStyle::absolute(
                                (index % 2) as f32 * 100.0,
                                (index / 2) as f32 * 50.0,
                                100.0,
                                50.0,
                            )));
                    }
                }
            }
            let foreign = buttons[2];
            let previous = HierarchyPlacement::current(world, foreign).unwrap();
            HierarchyWriter::new(world)
                .move_entity(foreign, previous, child_last(roots[0]))
                .unwrap();
            let runtime_child = world
                .spawn(button_bundle(
                    "retry",
                    "retry-action",
                    UiColor::rgba(0.2, 0.4, 0.8, 1.0),
                ))
                .id();
            world
                .entity_mut(runtime_child)
                .insert(UiNode::new(UiStyle::absolute(100.0, 50.0, 100.0, 50.0)));
            HierarchyWriter::new(world)
                .apply(HierarchyOperation::Place {
                    entity: runtime_child,
                    placement: child_last(roots[0]),
                })
                .unwrap();
            let runtime_root = world
                .spawn((
                    UiRoot::new(UiLayer::Base),
                    UiNode::new(UiStyle::fill()),
                    UiRuntimeKey::new(TEST_DOMAIN, "mixed-root").unwrap(),
                ))
                .id();
            place_ui_tree(world, runtime_root, &[]);
            let previous = HierarchyPlacement::current(world, buttons[1]).unwrap();
            HierarchyWriter::new(world)
                .move_entity(buttons[1], previous, child_last(runtime_root))
                .unwrap();
        })
        .unwrap();
    app.update().unwrap();
    let frame = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    assert_eq!(frame.root_outcomes().len(), 4);
    assert_eq!(frame.focus_order().len(), 4);
    let points = [
        UiPoint::new(10.0, 10.0),
        UiPoint::new(110.0, 10.0),
        UiPoint::new(10.0, 60.0),
        UiPoint::new(110.0, 60.0),
    ];
    let keys = points
        .iter()
        .map(|point| {
            frame
                .semantics()
                .iter()
                .find(|node| node.name() == "retry" && node.rect().contains(*point))
                .unwrap()
                .key()
                .clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        keys.iter().collect::<std::collections::BTreeSet<_>>().len(),
        4
    );
    let cursor = app
        .register_consumer(UiActionConsumerId::new("source-isolation").unwrap())
        .unwrap();
    let receipt = app.present_frame(Arc::clone(&frame), 1).unwrap();
    let route = UiRouteStamp::from_receipt(receipt);
    for (index, point) in points.into_iter().enumerate() {
        for event in [
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
                app.dispatch_input(route, event),
                UiRouteDisposition::Consumed(_)
            ));
        }
        let semantic = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
        assert_eq!(
            semantic
                .nodes()
                .iter()
                .filter(|node| node.name() == "retry")
                .map(|node| node.key())
                .collect::<std::collections::BTreeSet<_>>(),
            keys.iter().collect::<std::collections::BTreeSet<_>>()
        );
        for node in semantic
            .nodes()
            .iter()
            .filter(|node| node.name() == "retry")
        {
            assert_eq!(node.state().pressed(), node.key() == &keys[index]);
            assert_eq!(node.state().focused(), node.key() == &keys[index]);
        }
        if index == 0 {
            app.update().unwrap();
            let pressed = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
            let color = |frame: &nara_ui_kernel::UiFrame, key: &UiKey| {
                frame
                    .paint()
                    .iter()
                    .find_map(|primitive| match primitive {
                        UiPaintPrimitive::Rect {
                            key: Some(actual),
                            brush: Some(UiResolvedBrush::Solid(color)),
                            ..
                        } if actual == key => Some(*color),
                        _ => None,
                    })
                    .expect("each controlled button should have a solid rectangle")
            };
            for (key_index, key) in keys.iter().enumerate() {
                if key_index == index {
                    assert_eq!(color(&pressed, key), UiTheme::default().palette().positive);
                    assert_ne!(color(&pressed, key), color(&frame, key));
                } else {
                    assert_eq!(color(&pressed, key), color(&frame, key));
                }
            }
        }
        assert!(matches!(
            app.dispatch_input(
                route,
                UiInputEvent::PointerButton {
                    seat: UiSeatId::PRIMARY,
                    pointer: UiPointerId::PRIMARY,
                    button: UiPointerButton::Primary,
                    phase: UiButtonPhase::Released,
                    position: Some(point),
                }
            ),
            UiRouteDisposition::Consumed(_)
        ));
    }
    let events = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(
        events
            .iter()
            .map(|event| event.target())
            .collect::<Vec<_>>(),
        keys.iter().collect::<Vec<_>>()
    );
    assert!(
        events
            .iter()
            .all(|event| event.action().as_str() == "retry-action")
    );
    let last = events.last().unwrap().sequence();
    app.acknowledge(cursor, last);
}

#[test]
fn persistent_rebuild_keeps_keys_but_not_runtime_or_unpresented_behavior_authority() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Resource)]
    struct CurrentScene(nara_scene::SpawnedSceneInstance);

    let mut app = ui_app();
    install_test_surface(app.configuration_world());
    let requested_step = Arc::new(AtomicUsize::new(0));
    let requested = Arc::clone(&requested_step);
    let mut current = None;
    let mut applied_step = None;
    app.configuration()
        .add_systems(CoreStage::Update, move |world: &mut World| {
            let step = requested.load(Ordering::Relaxed);
            if applied_step == Some(step) {
                return;
            }
            let registry = ComponentRegistry::from_snapshot(
                nara_reflect::component_registry(world)
                    .unwrap()
                    .snapshot()
                    .unwrap(),
            );
            let candidate = nara_scene::ExpandedSceneCandidate::plain(
                scene_source_key(if step == 2 { "other" } else { "panel" }),
                Arc::new(SceneDocument::new(
                    [scene_id("root")],
                    [
                        SceneEntityRecord::new(scene_id("root")).with_children([scene_id("retry")]),
                        SceneEntityRecord::new(scene_id("retry")),
                    ],
                )),
            );
            let prepared = match &current {
                Some(current) => nara_scene::prepare_scene_replacement(
                    world, &registry, &candidate, current, None,
                ),
                None => nara_scene::prepare_scene(
                    world,
                    &registry,
                    &candidate,
                    &scene_occurrence("rebuild"),
                    None,
                ),
            }
            .unwrap();
            let report = prepared.commit(world);
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let instance = report.instance.unwrap();
            let entities = world
                .iter_entities()
                .filter_map(|entity| {
                    entity
                        .get::<SceneEntitySource>()
                        .filter(|source| source.instance_id == instance.instance_id())
                        .map(|source| (entity.id(), source.provenance().local_entity_id().clone()))
                })
                .collect::<Vec<_>>();
            for (entity, local) in entities {
                if local.as_str() == "root" {
                    world
                        .entity_mut(entity)
                        .insert((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())));
                } else {
                    world.entity_mut(entity).insert(button_bundle(
                        "retry",
                        if step == 0 {
                            "old-action"
                        } else {
                            "new-action"
                        },
                        UiColor::rgba(0.2, 0.4, 0.8, 1.0),
                    ));
                }
            }
            world.insert_resource(CurrentScene(instance.clone()));
            current = Some(instance);
            applied_step = Some(step);
        })
        .unwrap();
    app.update().unwrap();
    let original = app.world().resource::<CurrentScene>().0.clone();
    let live = original.resolve(app.world(), &scene_id("retry"));
    let first = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let key = first.semantics()[0].key().clone();
    let receipt = app.present_frame(first, 1).unwrap();
    let cursor = app
        .register_consumer(UiActionConsumerId::new("persistent-rebuild").unwrap())
        .unwrap();

    requested_step.store(1, Ordering::Relaxed);
    app.update().unwrap();
    let successor = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    assert_eq!(successor.semantics()[0].key(), &key);
    assert_ne!(original.resolve(app.world(), &scene_id("retry")), live);
    let current = &app.world().resource::<CurrentScene>().0;
    assert_ne!(current.instance_id(), original.instance_id());
    assert_ne!(
        original.resolve(app.world(), &scene_id("retry")),
        current.resolve(app.world(), &scene_id("retry"))
    );
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::semantic_action(
            receipt,
            UiSeatId::PRIMARY,
            key.clone(),
            UiActionId::new("old-action").unwrap(),
        )),
        UiRouteDisposition::Consumed(_)
    ));
    assert!(matches!(
        app.present_frame(successor, 2),
        Err(UiAdapterError::FrameNotLatest { surface: 7 })
    ));
    app.update().unwrap();
    let successor = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    assert_eq!(successor.semantics()[0].key(), &key);
    let successor_receipt = app.present_frame(successor, 2).unwrap();
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::semantic_action(
            successor_receipt,
            UiSeatId::PRIMARY,
            key.clone(),
            UiActionId::new("new-action").unwrap(),
        )),
        UiRouteDisposition::Consumed(_)
    ));

    requested_step.store(2, Ordering::Relaxed);
    app.update().unwrap();
    let switched = latest_ui_frame(app.world(), surface_id()).unwrap().unwrap();
    let switched_key = switched.semantics()[0].key().clone();
    assert_ne!(switched_key, key);
    let switched_receipt = app.present_frame(switched, 3).unwrap();
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::semantic_action(
            switched_receipt,
            UiSeatId::PRIMARY,
            key.clone(),
            UiActionId::new("new-action").unwrap(),
        )),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::NonActivation(
            UiNonActivation::TargetRetired { .. }
        ))
    ));
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::semantic_action(
            switched_receipt,
            UiSeatId::PRIMARY,
            switched_key,
            UiActionId::new("new-action").unwrap(),
        )),
        UiRouteDisposition::Consumed(_)
    ));
    let actions = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(actions.len(), 3);
    assert_eq!(actions[0].target(), &key);
    assert_eq!(actions[0].action().as_str(), "old-action");
    assert_eq!(actions[1].action().as_str(), "new-action");
    let last = actions.last().unwrap().sequence();
    app.acknowledge(cursor, last);
    app.retire_surface(surface_id()).unwrap().unwrap();
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::semantic_action(
            receipt,
            UiSeatId::PRIMARY,
            key,
            UiActionId::new("old-action").unwrap(),
        )),
        UiRouteDisposition::RejectedClosed(UiRouteRejection::RetiredSurface { surface: 7 })
    ));
}

fn scene_occurrence(slot: &str) -> nara_scene::SceneOccurrenceKey {
    nara_scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
