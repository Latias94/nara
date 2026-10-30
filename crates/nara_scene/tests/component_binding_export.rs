//! Independent consumers export actual live native and data components through one Scene remap.

use std::{collections::BTreeSet, sync::Arc};

use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_asset::{
    AssetRecord, AssetRef, AssetRefExportPolicy, AssetServer, AssetSourceKind, AssetSourcePath,
    Handle, ProjectAssetDatabase, StableAssetId,
};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::{Component, Entity, World};
use nara_identity::{
    EntityLookup, EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId,
    PersistentRuntimeReference, SpawnedSceneInstance, WorldIdentityDomain, adopt_identity_entity,
    register_persistent_identity, spawn_identity_entity,
};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentRegistry, ComponentRegistryPlugin,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueMaterializationLimits, ComponentValueShape,
    PersistentComponentSchemaOwnerId, PreparedComponentCandidate,
};
use nara_scene::{
    ExpandedSceneCandidate, Name, SceneComponentBindings, SceneComponentBindingsPlugin,
    SceneComponentRecord, SceneComponentsPlugin, SceneDataComponents, SceneDocument, SceneEntityId,
    SceneEntityRecord, SceneExportOptions, SceneExportOutput, SceneExportReport,
    SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind, export_scene,
    export_scene_with_options, retire_and_despawn_scene_entity, scene_component_bindings,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn owner_id() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.export-data").unwrap()
}

fn schema(name: &str, field: ComponentFieldSchema) -> ComponentSchema {
    ComponentSchema::new(component(name), name, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field.with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn field(name: &str, shape: ComponentValueShape) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new(name),
        name,
        ComponentFieldPath::from_fields([name]),
        shape,
    )
}

fn data_owner() -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        owner_id(),
        ComponentSchemaCatalog {
            components: vec![
                schema(
                    "game.Links",
                    field(
                        "targets",
                        ComponentValueShape::list(ComponentValueShape::map(
                            ComponentValueShape::EntityRef,
                        )),
                    )
                    .with_capability(ComponentCapability::EntityRef),
                ),
                schema(
                    "game.Images",
                    field(
                        "images",
                        ComponentValueShape::list(ComponentValueShape::map(
                            ComponentValueShape::AssetRef,
                        )),
                    )
                    .with_capability(ComponentCapability::AssetRef),
                ),
                schema(
                    "game.Score",
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("value"),
                        "Value",
                        ComponentFieldPath::empty(),
                        ComponentValueShape::scalar(nara_reflect::ComponentValueKind::U64),
                    ),
                ),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap()
}

#[derive(Component)]
struct NativeLink(EntityReference);

struct FixtureImage;

#[derive(Component)]
struct NativeImage(Handle<FixtureImage>);

fn native_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    nara_scene::register_scene_components(&mut registry).unwrap();
    registry
        .register_persistent_component_with_codec::<NativeLink, _, _>(
            schema(
                "game.NativeLink",
                field("target", ComponentValueShape::EntityRef)
                    .with_capability(ComponentCapability::EntityRef),
            ),
            |value| Ok(NativeLink(value.field_entity_reference("target")?.clone())),
            |link| {
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(link.0.clone()),
                )]))
            },
        )
        .unwrap();
    registry
        .register_persistent_component_codec_with_context::<NativeImage, _, _>(
            schema(
                "game.NativeImage",
                ComponentFieldSchema::required(
                    ComponentFieldId::new("image"),
                    "Image",
                    ComponentFieldPath::from_fields(["nested", "image"]),
                    ComponentValueShape::AssetRef,
                )
                .with_capability(ComponentCapability::AssetRef),
            ),
            |value, context| {
                let reference = read_asset(value.field("nested")?.field("image")?)?;
                if let Some(handle) = context.resolve_asset_ref_with_kind::<FixtureImage>(
                    &reference,
                    &AssetSourceKind::Image,
                ) {
                    return handle
                        .map(|handle| PreparedComponentCandidate::insert(NativeImage(handle)))
                        .map_err(|_| {
                            ComponentCodecError::invalid_field("image", "admitted image")
                        });
                }
                let resolver = context
                    .asset_resolution()
                    .ok_or_else(|| ComponentCodecError::missing_field("asset resolution"))?;
                let record = reference
                    .validate_with_database_as(resolver, &AssetSourceKind::Image)
                    .map_err(|_| ComponentCodecError::invalid_field("image", "admitted image"))?
                    .clone();
                Ok(PreparedComponentCandidate::with_asset_server(
                    move |context| {
                        context
                            .reserve_asset_record(&record)
                            .map(NativeImage)
                            .map_err(|_| {
                                ComponentCodecError::invalid_field("image", "admitted image")
                            })
                    },
                ))
            },
            |image, world, context| {
                let server = world
                    .get_resource::<AssetServer>()
                    .ok_or_else(|| ComponentCodecError::missing_field("AssetServer"))?;
                let reference = AssetRef::from_handle_with_policy(
                    server,
                    image.0,
                    context.asset_path_scope(),
                    context.asset_ref_export_policy(),
                )
                .map_err(|_| ComponentCodecError::invalid_field("image", "known native handle"))?;
                Ok(ComponentValue::map([(
                    "nested",
                    ComponentValue::map([("image", asset_value(&reference))]),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn empty_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    registry
}

fn data_record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner_id())
}

fn source_key(slot: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "e758bd18-2f06-4736-9752-dcfe038c3289".parse().unwrap(),
        slot,
        SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn spawn(
    bindings: &SceneComponentBindings,
    world: &mut World,
    document: SceneDocument,
    slot: &str,
    database: Option<nara_asset::AssetReferenceResolver<'_>>,
) -> SpawnedSceneInstance {
    let candidate = ExpandedSceneCandidate::plain(source_key(slot), Arc::new(document));
    let occurrence = SceneOccurrenceKey::new("game.export-tests", slot).unwrap();
    let report = bindings
        .prepare_scene(world, &candidate, &occurrence, database)
        .unwrap_or_else(|report| panic!("preparation failed: {report:?}"))
        .commit(world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    report.instance.unwrap()
}

fn live_entity(world: &World, instance: &SpawnedSceneInstance, name: &str) -> Entity {
    let locator = instance.locator(&id(name)).unwrap();
    match world
        .resource::<WorldIdentityDomain>()
        .lookup(world, locator.entity())
    {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected a live entity, got {lookup:?}"),
    }
}

fn exported<'a>(
    output: &'a SceneExportOutput,
    instance: &SpawnedSceneInstance,
    name: &str,
) -> &'a SceneEntityRecord {
    let mapped = output
        .remap
        .get(&instance.locator(&id(name)).unwrap())
        .unwrap();
    output
        .document
        .entities
        .iter()
        .find(|entity| &entity.id == mapped)
        .unwrap()
}

fn output(report: SceneExportReport) -> SceneExportOutput {
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    report.into_output().unwrap()
}

fn rejected(report: SceneExportReport, code: &str) {
    assert!(report.output().is_none(), "{report:?}");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "{report:?}"
    );
}

fn persistent(value: &str) -> PersistentRuntimeReference {
    PersistentRuntimeReference::new(
        PersistentRuntimeNamespaceId::new("save").unwrap(),
        PersistentRuntimeId::parse_str(value).unwrap(),
    )
}

fn linked_scene(
    internal: &PersistentRuntimeReference,
    external: &PersistentRuntimeReference,
) -> SceneDocument {
    let local = EntityReference::SceneLocal {
        entity: id("target"),
    };
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("source"))
            .with_component(
                component("nara.scene.Name"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("source".into()),
                ),
            )
            .with_component(
                component("game.NativeLink"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([(
                        "target",
                        ComponentValue::EntityReference(local.clone()),
                    )]),
                ),
            )
            .with_component(
                component("game.Links"),
                data_record(ComponentValue::map([(
                    "targets",
                    ComponentValue::List(vec![ComponentValue::map([
                        ("local", ComponentValue::EntityReference(local)),
                        (
                            "internal",
                            ComponentValue::EntityReference(EntityReference::Persistent {
                                entity: internal.clone(),
                            }),
                        ),
                        (
                            "external",
                            ComponentValue::EntityReference(EntityReference::Persistent {
                                entity: external.clone(),
                            }),
                        ),
                    ])]),
                )])),
            )
            .with_component(component("game.Score"), data_record(ComponentValue::U64(7))),
        SceneEntityRecord::new(id("target")),
        SceneEntityRecord::new(id("retired")).with_component(
            component("game.Score"),
            data_record(ComponentValue::U64(42)),
        ),
    ])
}

#[test]
fn live_export_observes_retirement_later_instances_and_nested_persistent_references() {
    let registry = native_registry();
    let bindings =
        SceneComponentBindings::new(registry.snapshot().unwrap(), [data_owner()]).unwrap();
    let internal = persistent("11111111-1111-4111-8111-111111111111");
    let external = persistent("22222222-2222-4222-8222-222222222222");
    let mut world = World::new();
    let first = spawn(
        &bindings,
        &mut world,
        linked_scene(&internal, &external),
        "first",
        None,
    );
    let first_target = live_entity(&world, &first, "target");
    let token = adopt_identity_entity(&mut world, first_target).unwrap();
    let persistent_locator =
        register_persistent_identity(&mut world, token, internal.clone()).unwrap();
    let external_token = spawn_identity_entity(&mut world).unwrap();
    register_persistent_identity(&mut world, external_token, external.clone()).unwrap();
    let retired = live_entity(&world, &first, "retired");
    retire_and_despawn_scene_entity(&mut world, retired).unwrap();

    let mut later_document = linked_scene(&internal, &external);
    later_document
        .entities
        .retain(|entity| entity.id != id("retired"));
    later_document
        .roots
        .retain(|entity| *entity != id("retired"));
    let later = spawn(&bindings, &mut world, later_document, "later", None);
    let source = live_entity(&world, &first, "source");
    world.get_mut::<Name>(source).unwrap().0 = "live native name".into();
    let result = output(bindings.export_scene(&world));
    assert_eq!(result.document.entities.len(), 4);
    assert!(
        result
            .remap
            .get(&first.locator(&id("retired")).unwrap())
            .is_none()
    );
    let mapped_first_target = result.remap.get(&persistent_locator).unwrap().clone();
    let mut mapped_targets = BTreeSet::new();
    for instance in [&first, &later] {
        let source = exported(&result, instance, "source");
        let target = exported(&result, instance, "target");
        mapped_targets.insert(target.id.clone());
        let local = EntityReference::SceneLocal {
            entity: target.id.clone(),
        };
        assert_eq!(
            source.components[&component("game.NativeLink")]
                .value
                .field_entity_reference("target")
                .unwrap(),
            &local,
        );
        let links = &source.components[&component("game.Links")];
        assert_eq!(links.owner(), Some(&owner_id()));
        let ComponentValue::List(targets) = links.value.field("targets").unwrap() else {
            panic!("nested links must remain a list");
        };
        assert_eq!(targets[0].field_entity_reference("local").unwrap(), &local);
        assert_eq!(
            targets[0].field_entity_reference("internal").unwrap(),
            &EntityReference::SceneLocal {
                entity: mapped_first_target.clone(),
            },
        );
        assert_eq!(
            targets[0].field_entity_reference("external").unwrap(),
            &EntityReference::Persistent {
                entity: external.clone(),
            },
        );
        assert_eq!(
            source.components[&component("game.Score")].owner(),
            Some(&owner_id()),
        );
    }
    assert_eq!(mapped_targets.len(), 2);
    assert_eq!(
        exported(&result, &first, "source").components[&component("nara.scene.Name")]
            .value
            .as_str(),
        Some("live native name"),
    );
}

#[test]
fn native_only_missing_and_same_byte_foreign_selections_refuse_actual_data() {
    let native = empty_registry();
    let owner = data_owner();
    let bindings =
        SceneComponentBindings::new(native.snapshot().unwrap(), [owner.clone()]).unwrap();
    let mut world = World::new();
    spawn(
        &bindings,
        &mut world,
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("data"))
            .with_component(component("game.Score"), data_record(ComponentValue::U64(7)))]),
        "data-only",
        None,
    );
    output(bindings.export_scene(&world));
    rejected(
        export_scene(&world, &native),
        "scene.export-data-binding-unavailable",
    );
    rejected(
        export_scene_with_options(&world, &native, SceneExportOptions::default()),
        "scene.export-data-binding-unavailable",
    );

    let missing = SceneComponentBindings::new(native.snapshot().unwrap(), []).unwrap();
    rejected(
        missing.export_scene(&world),
        "scene.export-data-binding-unavailable",
    );
    let foreign = SceneComponentBindings::new(native.snapshot().unwrap(), [data_owner()]).unwrap();
    rejected(
        foreign.export_scene(&world),
        "scene.export-data-owner-mismatch",
    );
    let shared = SceneComponentBindings::new(native.snapshot().unwrap(), [owner]).unwrap();
    output(shared.export_scene(&world));
}

#[derive(Component)]
struct RuntimeOnly;

#[derive(Component)]
struct DifferentNativeName(String);

#[test]
fn native_export_rejects_same_stable_id_bound_to_a_different_rust_type() {
    let registered = native_registry();
    let bindings = SceneComponentBindings::new(registered.snapshot().unwrap(), []).unwrap();
    let mut world = World::new();
    spawn(
        &bindings,
        &mut world,
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("named")).with_component(
            component("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("actual Name".into()),
            ),
        )]),
        "native-type-coverage",
        None,
    );
    let mut incompatible = ComponentRegistry::new();
    incompatible
        .register_persistent_component_with_codec::<DifferentNativeName, _, _>(
            registered
                .schema(&component("nara.scene.Name"))
                .unwrap()
                .clone(),
            |value| {
                value
                    .as_str()
                    .map(|name| DifferentNativeName(name.to_owned()))
                    .ok_or_else(|| ComponentCodecError::invalid_field("value", "string"))
            },
            |name| Ok(ComponentValue::String(name.0.clone())),
        )
        .unwrap();
    incompatible.freeze().unwrap();
    rejected(
        export_scene(&world, &incompatible),
        "scene.export-native-binding-unavailable",
    );
    let mixed = SceneComponentBindings::new(incompatible.snapshot().unwrap(), []).unwrap();
    rejected(
        mixed.export_scene(&world),
        "scene.export-native-binding-unavailable",
    );
    output(export_scene(&world, &registered));
}

#[test]
fn native_export_rejects_unselected_live_members_but_allows_removed_members() {
    let registered = native_registry();
    let bindings = SceneComponentBindings::new(registered.snapshot().unwrap(), []).unwrap();
    let mut world = World::new();
    let instance = spawn(
        &bindings,
        &mut world,
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("named")).with_component(
            component("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("live".into()),
            ),
        )]),
        "native-coverage",
        None,
    );
    let empty = empty_registry();
    rejected(
        export_scene(&world, &empty),
        "scene.export-native-binding-unavailable",
    );
    let unselected = SceneComponentBindings::new(empty.snapshot().unwrap(), []).unwrap();
    rejected(
        unselected.export_scene(&world),
        "scene.export-native-binding-unavailable",
    );
    let entity = live_entity(&world, &instance, "named");
    world
        .entity_mut(entity)
        .remove::<Name>()
        .insert(RuntimeOnly);
    let result = output(export_scene(&world, &empty));
    assert!(exported(&result, &instance, "named").components.is_empty());
    output(unselected.export_scene(&world));
}

#[test]
fn native_export_rejects_known_persistent_members_without_their_entity_receipt() {
    let registry = native_registry();
    let bindings = SceneComponentBindings::new(registry.snapshot().unwrap(), []).unwrap();
    let mut world = World::new();
    let instance = spawn(
        &bindings,
        &mut world,
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("named")).with_component(
                component("nara.scene.Name"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("admitted".into()),
                ),
            ),
            SceneEntityRecord::new(id("unclaimed")),
        ]),
        "receipt-coverage",
        None,
    );
    let unclaimed = live_entity(&world, &instance, "unclaimed");
    world.entity_mut(unclaimed).insert(Name::new("unguarded"));
    rejected(
        export_scene(&world, &registry),
        "scene.export-native-binding-unavailable",
    );
    world.entity_mut(unclaimed).remove::<Name>();
    output(export_scene(&world, &registry));
}

#[test]
fn moving_a_real_carrier_to_another_registered_entity_refuses_export() {
    let native = empty_registry();
    let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), [data_owner()]).unwrap();
    let mut world = World::new();
    let instance = spawn(
        &bindings,
        &mut world,
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("source"))
                .with_component(component("game.Score"), data_record(ComponentValue::U64(7))),
            SceneEntityRecord::new(id("other")),
        ]),
        "location",
        None,
    );
    let source = live_entity(&world, &instance, "source");
    let other = live_entity(&world, &instance, "other");
    let carrier = world
        .entity_mut(source)
        .take::<SceneDataComponents>()
        .unwrap();
    world.entity_mut(other).insert(carrier);
    rejected(
        bindings.export_scene(&world),
        "scene.export-data-location-mismatch",
    );
}

struct ManagedExportFixture;

const MANAGED_EXPORT_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PluginId::new("game.export-fixture"), PluginCategory::Core)
        .requires_plugins(&[
            nara_scene::SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
            nara_scene::SCENE_COMPONENTS_PLUGIN_ID,
        ]);

impl Plugin for ManagedExportFixture {
    fn declaration() -> &'static PluginDeclaration {
        &MANAGED_EXPORT_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        let bindings = scene_component_bindings(app.world()).unwrap().clone();
        spawn(
            &bindings,
            app.world_mut()?,
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("managed"))
                .with_component(component("game.Score"), data_record(ComponentValue::U64(7)))]),
            "managed",
            None,
        );
        Ok(())
    }
}

#[test]
fn managed_export_requires_the_installed_exact_definitions() {
    let owner = data_owner();
    let mut app = App::new();
    app.add_plugin(ComponentRegistryPlugin).unwrap();
    app.add_plugin(SceneComponentsPlugin).unwrap();
    app.add_plugin(SceneComponentBindingsPlugin::new([owner.clone()]))
        .unwrap();
    app.add_plugin(ManagedExportFixture).unwrap();
    let app = app.seal().unwrap();
    let installed = scene_component_bindings(app.world()).unwrap();
    let expected = output(installed.export_scene(app.world()));
    assert_eq!(expected.document.entities.len(), 1);
    let shared = SceneComponentBindings::new(installed.native_snapshot().clone(), [owner]).unwrap();
    assert_eq!(output(shared.export_scene(app.world())), expected);
    let foreign =
        SceneComponentBindings::new(installed.native_snapshot().clone(), [data_owner()]).unwrap();
    rejected(
        foreign.export_scene(app.world()),
        "scene.export-bindings-authority-invalid",
    );
    let missing = SceneComponentBindings::new(installed.native_snapshot().clone(), []).unwrap();
    rejected(
        missing.export_scene(app.world()),
        "scene.export-bindings-authority-invalid",
    );
}

#[test]
fn managed_export_cannot_install_caller_data_definitions_implicitly() {
    let mut app = App::new();
    app.add_plugin(ComponentRegistryPlugin).unwrap();
    let app = app.seal().unwrap();
    let bindings = SceneComponentBindings::new(
        nara_reflect::component_registry(app.world())
            .unwrap()
            .snapshot()
            .unwrap(),
        [data_owner()],
    )
    .unwrap();
    rejected(
        bindings.export_scene(app.world()),
        "scene.export-bindings-authority-invalid",
    );
}

#[test]
fn a_retired_declared_target_refuses_the_entire_live_export() {
    let native = empty_registry();
    let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), [data_owner()]).unwrap();
    let mut world = World::new();
    let instance = spawn(
        &bindings,
        &mut world,
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("source")).with_component(
                component("game.Links"),
                data_record(ComponentValue::map([(
                    "targets",
                    ComponentValue::List(vec![ComponentValue::map([(
                        "local",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: id("target"),
                        }),
                    )])]),
                )])),
            ),
            SceneEntityRecord::new(id("target")),
        ]),
        "retired-target",
        None,
    );
    output(bindings.export_scene(&world));
    let target = live_entity(&world, &instance, "target");
    retire_and_despawn_scene_entity(&mut world, target).unwrap();
    rejected(
        bindings.export_scene(&world),
        "scene.export-entity-reference-rewrite-failed",
    );
}

fn asset_value(reference: &AssetRef) -> ComponentValue {
    nara_reflect::encode_asset_reference_value(reference)
}

fn read_asset(value: &ComponentValue) -> Result<AssetRef, ComponentCodecError> {
    nara_reflect::decode_asset_reference_value(value)
}

fn database(path: &str) -> (ProjectAssetDatabase, AssetRef) {
    let stable = StableAssetId::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            stable,
            AssetSourcePath::new(path).unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    (database, AssetRef::StableId(stable))
}

fn images_value(references: impl IntoIterator<Item = (&'static str, AssetRef)>) -> ComponentValue {
    ComponentValue::map([(
        "images",
        ComponentValue::List(vec![ComponentValue::map(
            references
                .into_iter()
                .map(|(name, reference)| (name, asset_value(&reference))),
        )]),
    )])
}

fn options(policy: AssetRefExportPolicy) -> SceneExportOptions {
    SceneExportOptions {
        asset_ref_export_policy: policy,
        ..Default::default()
    }
}

#[test]
fn scoped_scene_prepare_cook_and_export_preserve_identity_without_canonical_prefixes() {
    use nara_asset::{AssetPathScope, AssetProductRef, ImportedProductId};
    use nara_scene::{RuntimeSceneCandidate, RuntimeSceneLimits};

    let assets = AssetPathScope::under(AssetSourcePath::new("game/art").unwrap());
    let foreign = AssetPathScope::under(AssetSourcePath::new("package/art").unwrap());
    let registry = native_registry();
    let bindings =
        SceneComponentBindings::new(registry.snapshot().unwrap(), [data_owner()]).unwrap();
    let (mut database, stable) = database("game/art/shared.png");
    let source = stable.as_stable_id().unwrap();
    database
        .insert(AssetRecord::new(
            "22222222-2222-4222-8222-222222222222".parse().unwrap(),
            AssetSourcePath::new("package/art/shared.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    let path = AssetRef::path("shared.png").unwrap();
    let product =
        AssetRef::Product(AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap());
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("image"))
        .with_component(
            component("game.Images"),
            data_record(images_value([("stable", stable)])),
        )
        .with_component(
            component("game.NativeImage"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "nested",
                    ComponentValue::map([("image", asset_value(&path))]),
                )]),
            ),
        )]);
    let resolver = database.resolver(&assets);
    let diagnostics = bindings.validate_scene(&document, Some(resolver), Default::default());
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    let candidate = ExpandedSceneCandidate::plain(source_key("scoped"), Arc::new(document.clone()));
    let cooked = RuntimeSceneCandidate::cook(
        &candidate,
        &bindings,
        resolver,
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let cooked_native =
        &cooked.document().entities[0].components[&component("game.NativeImage")].value;
    assert_eq!(
        cooked_native
            .field("nested")
            .unwrap()
            .field("image")
            .unwrap(),
        &asset_value(&product)
    );

    let mut world = World::new();
    let instance = spawn(&bindings, &mut world, document, "scoped", Some(resolver));
    let saved = output(bindings.export_scene_with_options(
        &world,
        options(AssetRefExportPolicy::Path),
        Some(resolver),
    ));
    let saved_record = exported(&saved, &instance, "image");
    assert_eq!(
        saved_record.components[&component("game.Images")].value,
        images_value([("stable", path.clone())])
    );
    assert_eq!(
        saved_record.components[&component("game.NativeImage")]
            .value
            .field("nested")
            .unwrap()
            .field("image")
            .unwrap(),
        &asset_value(&path)
    );
    let saved_entity_id = saved_record.id.clone();
    let mut reopened = World::new();
    let reopened_instance = spawn(
        &bindings,
        &mut reopened,
        saved.document,
        "reopened",
        Some(resolver),
    );
    let reopened_entity = live_entity(&reopened, &reopened_instance, saved_entity_id.as_str());
    let reopened_handle = reopened.get::<NativeImage>(reopened_entity).unwrap().0;
    let reopened_server = reopened.resource::<AssetServer>();
    assert_eq!(
        reopened_server.path(reopened_handle.id()).unwrap().as_str(),
        "game/art/shared.png"
    );
    assert_eq!(
        reopened_server.stable_id(reopened_handle.id()),
        Some(source)
    );

    let cross_root = output(bindings.export_scene_with_options(
        &world,
        options(AssetRefExportPolicy::Path),
        Some(database.resolver(&foreign)),
    ));
    let cross_record = exported(&cross_root, &instance, "image");
    assert_eq!(
        cross_record.components[&component("game.Images")].value,
        images_value([("stable", product.clone())])
    );
    assert_eq!(
        cross_record.components[&component("game.NativeImage")]
            .value
            .field("nested")
            .unwrap()
            .field("image")
            .unwrap(),
        &asset_value(&product)
    );
}

#[test]
fn prefab_lookup_scope_is_separate_from_component_resolution_and_keeps_owner_source_keys() {
    use nara_asset::{AssetPathScope, AssetReferenceResolver};
    use nara_scene::{
        InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, PrefabSourceResolver,
        ResolvedPrefabSource,
    };

    struct Prefabs<'a> {
        database: &'a ProjectAssetDatabase,
        declarations: AssetPathScope,
        path: AssetSourcePath,
        source: ResolvedPrefabSource,
    }
    impl PrefabSourceResolver for Prefabs<'_> {
        fn resolve_prefab(&self, reference: &AssetRef) -> Option<ResolvedPrefabSource> {
            if !reference.is_primary() {
                return None;
            }
            let resolver: AssetReferenceResolver<'_> = self.database.resolver(&self.declarations);
            let record = resolver.resolve_ref(reference).ok()?;
            (record.path() == &self.path).then(|| self.source.clone())
        }
    }

    let registry = native_registry();
    let bindings =
        SceneComponentBindings::new(registry.snapshot().unwrap(), [data_owner()]).unwrap();
    let assets = AssetPathScope::under(AssetSourcePath::new("game/art").unwrap());
    let declarations = AssetPathScope::under(AssetSourcePath::new("game/prefabs").unwrap());
    let (mut database, _) = database("game/art/shared.png");
    let canonical = AssetSourcePath::new("game/prefabs/shared.png").unwrap();
    database
        .insert(AssetRecord::new(
            "44444444-4444-4444-8444-444444444444".parse().unwrap(),
            canonical.clone(),
            AssetSourceKind::Prefab,
        ))
        .unwrap();
    let request = AssetRef::path("shared.png").unwrap();
    let key = SourceDocumentKey::project_location(
        "e758bd18-2f06-4736-9752-dcfe038c3289".parse().unwrap(),
        canonical.clone(),
        SourceDocumentKind::Prefab,
    );
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))
        .with_component(
            component("game.NativeImage"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "nested",
                    ComponentValue::map([("image", asset_value(&request))]),
                )]),
            ),
        )]);
    let direct = Prefabs {
        database: &database,
        declarations,
        path: canonical,
        source: ResolvedPrefabSource::bound(Arc::new(prefab.clone()), key.clone()),
    };
    assert_eq!(direct.resolve_prefab(&request).unwrap().key(), &key);
    let snapshot = InMemoryPrefabSourceResolver::new().with_prefab(request.clone(), key, prefab);
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: request,
        overrides: None,
    });
    let document = SceneDocument::from_ordered_roots([anchor]);
    let expand = |lookup: &dyn PrefabSourceResolver| {
        bindings
            .try_expand_prefabs_with_loader(
                &document,
                Default::default(),
                Some(database.resolver(&assets)),
                |reference, _| Ok::<_, std::convert::Infallible>(lookup.resolve_prefab(reference)),
            )
            .unwrap()
    };
    let direct = expand(&direct);
    let snapshot = expand(&snapshot);
    assert!(!direct.diagnostics.has_errors(), "{:?}", direct.diagnostics);
    assert!(
        !snapshot.diagnostics.has_errors(),
        "{:?}",
        snapshot.diagnostics
    );
    assert_eq!(direct.document(), snapshot.document());
    assert_eq!(
        direct.candidate.unwrap().provenance(),
        snapshot.candidate.unwrap().provenance()
    );
}

#[test]
fn mixed_nested_assets_obey_both_policies_and_missing_conversion_has_no_output() {
    let registry = native_registry();
    let bindings =
        SceneComponentBindings::new(registry.snapshot().unwrap(), [data_owner()]).unwrap();
    let path = AssetRef::path("textures/icon.png").unwrap();
    let (database, stable) = database("textures/icon.png");
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("images"))
        .with_component(
            component("game.Images"),
            data_record(images_value([
                ("path", path.clone()),
                ("stable", stable.clone()),
            ])),
        )
        .with_component(
            component("game.NativeImage"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "nested",
                    ComponentValue::map([("image", asset_value(&stable))]),
                )]),
            ),
        )]);
    let mut world = World::new();
    let instance = spawn(
        &bindings,
        &mut world,
        document,
        "mixed-assets",
        Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
    );
    let server = world.resource::<AssetServer>().clone();
    for (policy, expected) in [
        (AssetRefExportPolicy::Path, &path),
        (AssetRefExportPolicy::StableIdWhenKnown, &stable),
    ] {
        let result = output(bindings.export_scene_with_options(
            &world,
            options(policy),
            Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
        ));
        let record = exported(&result, &instance, "images");
        let native = &record.components[&component("game.NativeImage")].value;
        assert_eq!(
            native.field("nested").unwrap().field("image").unwrap(),
            &asset_value(expected),
        );
        let data = &record.components[&component("game.Images")];
        assert_eq!(data.owner(), Some(&owner_id()));
        assert_eq!(
            data.value,
            images_value([("path", expected.clone()), ("stable", expected.clone())]),
        );
    }
    let missing = ProjectAssetDatabase::default();
    for metadata in [
        None,
        Some(missing.resolver(&nara_asset::AssetPathScope::ROOT)),
    ] {
        rejected(
            bindings.export_scene_with_options(
                &world,
                options(AssetRefExportPolicy::Path),
                metadata,
            ),
            "scene.export-data-asset-metadata-unavailable",
        );
    }
    let preserved = output(bindings.export_scene_with_options(
        &world,
        options(AssetRefExportPolicy::StableIdWhenKnown),
        None,
    ));
    assert_eq!(
        exported(&preserved, &instance, "images").components[&component("game.Images")].value,
        images_value([("path", path), ("stable", stable)]),
    );
    assert_eq!(world.resource::<AssetServer>(), &server);
}

#[test]
fn unknown_data_paths_remain_paths_without_native_handles_or_loading() {
    let native = empty_registry();
    let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), [data_owner()]).unwrap();
    let value = images_value([(
        "unindexed",
        AssetRef::path("textures/unindexed.png").unwrap(),
    )]);
    let mut world = World::new();
    world.insert_resource(AssetServer::new());
    let instance = spawn(
        &bindings,
        &mut world,
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("data"))
            .with_component(component("game.Images"), data_record(value.clone()))]),
        "unknown-assets",
        None,
    );
    let server = world.resource::<AssetServer>().clone();
    let metadata = ProjectAssetDatabase::default();
    for policy in [
        AssetRefExportPolicy::Path,
        AssetRefExportPolicy::StableIdWhenKnown,
    ] {
        let result = output(bindings.export_scene_with_options(
            &world,
            options(policy),
            Some(metadata.resolver(&nara_asset::AssetPathScope::ROOT)),
        ));
        assert_eq!(
            exported(&result, &instance, "data").components[&component("game.Images")].value,
            value,
        );
    }
    assert_eq!(world.resource::<AssetServer>(), &server);
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

#[test]
fn native_and_data_values_share_exact_aggregate_output_limits() {
    let registry = native_registry();
    let bindings =
        SceneComponentBindings::new(registry.snapshot().unwrap(), [data_owner()]).unwrap();
    let mut world = World::new();
    for slot in ["first", "second"] {
        spawn(
            &bindings,
            &mut world,
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("entity"))
                .with_component(
                    component("nara.scene.Name"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::String("n".into()),
                    ),
                )
                .with_component(component("game.Score"), data_record(ComponentValue::U64(7)))]),
            slot,
            None,
        );
    }
    let before = world.iter_entities().count();
    let mut export_options = options(AssetRefExportPolicy::Path);
    export_options.value_limits = limits(4, 18, 1);
    output(bindings.export_scene_with_options(&world, export_options, None));
    for below in [limits(3, 18, 1), limits(4, 17, 1)] {
        export_options.value_limits = below;
        rejected(
            bindings.export_scene_with_options(&world, export_options, None),
            "scene.export-value-budget-exceeded",
        );
    }
    assert_eq!(world.iter_entities().count(), before);
}

#[derive(Component)]
struct NativeDefault;

#[test]
fn residual_admission_accepts_zero_byte_defaults_and_charges_default_keys() {
    for zero_byte in [true, false] {
        let (path, shape, default, raw) = if zero_byte {
            (
                ComponentFieldPath::new([ComponentFieldPathSegment::index(0)]),
                ComponentValueShape::scalar(nara_reflect::ComponentValueKind::Null),
                ComponentValue::Null,
                ComponentValue::List(Vec::new()),
            )
        } else {
            (
                ComponentFieldPath::from_fields(["value"]),
                ComponentValueShape::scalar(nara_reflect::ComponentValueKind::String),
                ComponentValue::String(String::new()),
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            )
        };
        let definition = |name| {
            schema(
                name,
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("value"),
                    "Value",
                    path.clone(),
                    shape.clone(),
                    default.clone(),
                ),
            )
        };
        let owner = ComponentSchemaOwnerSnapshot::admit(
            owner_id(),
            ComponentSchemaCatalog {
                components: vec![definition("zz.DataDefault")],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let mut native = ComponentRegistry::new();
        nara_scene::register_scene_components(&mut native).unwrap();
        let encoded = raw.clone();
        native
            .register_persistent_component_with_codec::<NativeDefault, _, _>(
                definition("zz.NativeDefault"),
                |_| Ok(NativeDefault),
                move |_| Ok(encoded.clone()),
            )
            .unwrap();
        native.freeze().unwrap();
        let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), [owner]).unwrap();
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("defaulted"))
            .with_component(
                component("nara.scene.Name"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("n".into()),
                ),
            )
            .with_component(
                component("zz.NativeDefault"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, raw.clone()),
            )
            .with_component(component("zz.DataDefault"), data_record(raw))]);
        let mut world = World::new();
        let instance = spawn(&bindings, &mut world, document, "defaults", None);
        let total_bytes = if zero_byte { 1 } else { 11 };
        let export_options = SceneExportOptions {
            value_limits: limits(5, total_bytes, 2),
            ..Default::default()
        };
        let result = output(bindings.export_scene_with_options(&world, export_options, None));
        let record = exported(&result, &instance, "defaulted");
        assert_eq!(
            record.components[&component("zz.NativeDefault")].value,
            record.components[&component("zz.DataDefault")].value,
        );
        if !zero_byte {
            for bytes in [1, total_bytes - 1] {
                rejected(
                    bindings.export_scene_with_options(
                        &world,
                        SceneExportOptions {
                            value_limits: limits(5, bytes, 2),
                            ..export_options
                        },
                        None,
                    ),
                    "scene.export-value-budget-exceeded",
                );
            }
        }
    }
}

#[test]
fn generated_entity_reference_growth_uses_the_shared_output_budget() {
    let native = empty_registry();
    let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), [data_owner()]).unwrap();
    let mut world = World::new();
    for slot in ["first", "second"] {
        spawn(
            &bindings,
            &mut world,
            SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(id("s")).with_component(
                    component("game.Links"),
                    data_record(ComponentValue::map([(
                        "targets",
                        ComponentValue::List(vec![ComponentValue::map([(
                            "t",
                            ComponentValue::EntityReference(EntityReference::SceneLocal {
                                entity: id("t"),
                            }),
                        )])]),
                    )])),
                ),
                SceneEntityRecord::new(id("t")),
            ]),
            slot,
            None,
        );
    }
    let expected = output(bindings.export_scene(&world));
    let (nodes, bytes) = expected
        .document
        .entities
        .iter()
        .flat_map(|entity| entity.components.values())
        .map(|record| record.value.cost())
        .fold((0, 0), |(nodes, bytes), cost| {
            (nodes + cost.nodes(), bytes + cost.logical_bytes())
        });
    let mut export_options = options(AssetRefExportPolicy::Path);
    export_options.value_limits = limits(nodes, bytes, 4);
    assert_eq!(
        output(bindings.export_scene_with_options(&world, export_options, None)),
        expected,
    );
    export_options.value_limits = limits(nodes, bytes - 1, 4);
    rejected(
        bindings.export_scene_with_options(&world, export_options, None),
        "scene.export-value-budget-exceeded",
    );
}

#[test]
fn asset_policy_growth_is_admitted_before_owning_replacement() {
    let native = empty_registry();
    let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), [data_owner()]).unwrap();
    let path = format!("textures/{}.png", "x".repeat(160));
    let (database, stable) = database(&path);
    let mut world = World::new();
    let instance = spawn(
        &bindings,
        &mut world,
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("data")).with_component(
            component("game.Images"),
            data_record(images_value([("image", stable)])),
        )]),
        "growth",
        Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
    );
    let expected = images_value([("image", AssetRef::path(&path).unwrap())]);
    let cost = expected.cost();
    let mut export_options = options(AssetRefExportPolicy::Path);
    export_options.value_limits = limits(cost.nodes(), cost.logical_bytes(), 5);
    let result = output(bindings.export_scene_with_options(
        &world,
        export_options,
        Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
    ));
    assert_eq!(
        exported(&result, &instance, "data").components[&component("game.Images")].value,
        expected,
    );
    export_options.value_limits = limits(cost.nodes(), cost.logical_bytes() - 1, 5);
    rejected(
        bindings.export_scene_with_options(
            &world,
            export_options,
            Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
        ),
        "scene.export-value-budget-exceeded",
    );
    export_options.value_limits = limits(cost.nodes(), cost.logical_bytes(), 4);
    rejected(
        bindings.export_scene_with_options(
            &world,
            export_options,
            Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
        ),
        "scene.export-value-budget-exceeded",
    );
}

#[test]
fn native_coverage_refusal_happens_before_any_encoder_callback() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let native = native_registry();
    let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), []).unwrap();
    let mut world = World::new();
    spawn(
        &bindings,
        &mut world,
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("a-source")).with_component(
                component("nara.scene.Name"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("live".into()),
                ),
            ),
            SceneEntityRecord::new(id("z-unselected")).with_component(
                component("game.NativeLink"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([(
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: id("z-unselected"),
                        }),
                    )]),
                ),
            ),
        ]),
        "coverage-before-code",
        None,
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut subset = ComponentRegistry::new();
    subset
        .register_persistent_component_with_codec::<Name, _, _>(
            native
                .schema(&component("nara.scene.Name"))
                .unwrap()
                .clone(),
            |_| Ok(Name::new("unused")),
            move |_| {
                observed.fetch_add(1, Ordering::Relaxed);
                Ok(ComponentValue::String("foreign".into()))
            },
        )
        .unwrap();
    subset.freeze().unwrap();
    rejected(
        export_scene(&world, &subset),
        "scene.export-native-binding-unavailable",
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn native_managed_export_refuses_same_type_foreign_admission_before_callback() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut app = App::new();
    app.add_plugins((ComponentRegistryPlugin, SceneComponentsPlugin))
        .unwrap();
    app.add_systems(nara_app::StartupStage::Scene, |world: &mut World| {
        let native = ComponentRegistry::from_snapshot(
            nara_reflect::component_registry(world)
                .unwrap()
                .snapshot()
                .unwrap(),
        );
        let candidate = ExpandedSceneCandidate::plain(
            source_key("managed-native"),
            Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                id("entity"),
            )
            .with_component(
                component("nara.scene.Name"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("live".into()),
                ),
            )])),
        );
        let report = nara_scene::prepare_scene(
            world,
            &native,
            &candidate,
            &SceneOccurrenceKey::new("export-tests", "managed-native").unwrap(),
            None,
        )
        .unwrap()
        .commit(world);
        assert!(report.instance.is_some(), "{report:?}");
    })
    .unwrap();
    let runtime = nara_app::RuntimeInstance::start(app).unwrap();
    let installed = nara_reflect::component_registry(runtime.world()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut foreign = ComponentRegistry::new();
    foreign
        .register_persistent_component_with_codec::<Name, _, _>(
            installed
                .schema(&component("nara.scene.Name"))
                .unwrap()
                .clone(),
            |_| Ok(Name::new("unused")),
            move |_| {
                observed.fetch_add(1, Ordering::Relaxed);
                Ok(ComponentValue::String("foreign".into()))
            },
        )
        .unwrap();
    foreign.freeze().unwrap();
    rejected(
        export_scene(runtime.world(), &foreign),
        "scene.export-native-authority-invalid",
    );
    output(export_scene(runtime.world(), installed));
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    runtime.finish().unwrap();
}

#[test]
fn native_and_data_asset_defaults_use_the_same_metadata_policy() {
    let path = AssetRef::path("textures/default.png").unwrap();
    let (database, stable) = database("textures/default.png");
    for default in [&stable, &path] {
        let definition = |name| {
            schema(
                name,
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("image"),
                    "Image",
                    ComponentFieldPath::from_fields(["image"]),
                    ComponentValueShape::AssetRef,
                    asset_value(default),
                )
                .with_capability(ComponentCapability::AssetRef),
            )
        };
        let owner = ComponentSchemaOwnerSnapshot::admit(
            owner_id(),
            ComponentSchemaCatalog {
                components: vec![definition("game.DataDefault")],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let mut native = ComponentRegistry::new();
        native
            .register_persistent_component_with_codec::<NativeDefault, _, _>(
                definition("game.NativeDefault"),
                |_| Ok(NativeDefault),
                |_| Ok(ComponentValue::Map(Default::default())),
            )
            .unwrap();
        native.freeze().unwrap();
        let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), [owner]).unwrap();
        let mut world = World::new();
        let instance = spawn(
            &bindings,
            &mut world,
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("defaulted"))
                .with_component(
                    component("game.DataDefault"),
                    data_record(ComponentValue::Map(Default::default())),
                )
                .with_component(
                    component("game.NativeDefault"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::Map(Default::default()),
                    ),
                )]),
            "default-assets",
            Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
        );
        let native_only = SceneComponentBindings::new(native.snapshot().unwrap(), []).unwrap();
        let mut native_world = World::new();
        spawn(
            &native_only,
            &mut native_world,
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("native-default"))
                .with_component(
                    component("game.NativeDefault"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::Map(Default::default()),
                    ),
                )]),
            "native-default-assets",
            Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
        );
        let server = world.get_resource::<AssetServer>().cloned();
        for (policy, expected) in [
            (AssetRefExportPolicy::Path, &path),
            (AssetRefExportPolicy::StableIdWhenKnown, &stable),
        ] {
            let result = output(bindings.export_scene_with_options(
                &world,
                options(policy),
                Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
            ));
            let record = exported(&result, &instance, "defaulted");
            for name in ["game.NativeDefault", "game.DataDefault"] {
                assert_eq!(
                    record.components[&component(name)].value,
                    ComponentValue::map([("image", asset_value(expected))])
                );
            }
            let native_result = output(nara_scene::export_scene_with_asset_resolution(
                &native_world,
                &native,
                options(policy),
                database.resolver(&nara_asset::AssetPathScope::ROOT),
            ));
            assert_eq!(
                native_result.document.entities[0].components[&component("game.NativeDefault")]
                    .value,
                ComponentValue::map([("image", asset_value(expected))])
            );
        }
        if default == &stable {
            rejected(
                bindings.export_scene_with_options(
                    &world,
                    options(AssetRefExportPolicy::Path),
                    None,
                ),
                "scene.export-data-asset-metadata-unavailable",
            );
            rejected(
                export_scene_with_options(
                    &native_world,
                    &native,
                    options(AssetRefExportPolicy::Path),
                ),
                "scene.export-data-asset-metadata-unavailable",
            );
        }
        assert_eq!(world.get_resource::<AssetServer>(), server.as_ref());
    }
}
