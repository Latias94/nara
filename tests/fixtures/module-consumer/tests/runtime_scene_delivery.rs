//! File-backed processed Scenes consumed without authoring sources or a second publication path.

use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara_app::{App, RuntimeInstance, StartupStage};
use nara_asset::{
    AssetPath, AssetProductRef, AssetRecord, AssetRef, AssetSourceKind, ImportedProductId,
    ProjectAssetDatabase, StableAssetId,
};
use nara_core::ItemLimit;
use nara_ecs::{
    Component, Entity, Resource, World,
    bevy_ecs::{lifecycle::HookContext, world::DeferredWorld},
};
use nara_hierarchy::Parent;
use nara_identity::{EntityLookup, EntityReference, SpawnedSceneInstance, WorldIdentityDomain};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryPlugin, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
    decode_asset_reference_value, encode_asset_reference_value,
};
use nara_scene::{
    ExpandedSceneCandidate, InMemoryPrefabSourceResolver, Name, PrefabAddition, PrefabDocument,
    PrefabDocumentCandidate, PrefabEntityTarget, PrefabInstance, PrefabOverlay, PrefabPlacement,
    PrefabPlacementParent, PrefabPlacementSlot, PrefabProjectionStep, RuntimeSceneCandidate,
    RuntimeSceneDocumentCandidate, RuntimeSceneLimits, SceneComponentBindings,
    SceneComponentBindingsPlugin, SceneComponentRecord, SceneComponentsPlugin, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SceneEntitySource,
    SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind,
    advanced::{
        SceneAssetBindings, SceneNativeRetirementClaims, SceneProductResource,
        SceneProductTransactionLimits,
    },
    retire_and_despawn_scene_instance, scene_component_bindings,
};

const PROBE: &str = "consumer.delivery.Probe";
const LINK: &str = "consumer.delivery.Link";
const ASSETS: &str = "consumer.delivery.Assets";
const DEFAULTS: &str = "consumer.delivery.Defaults";

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("consumer.delivery.data").unwrap()
}

fn source(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "2b923e45-7989-4345-aea8-e873b363183f".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("consumer.delivery", "active").unwrap()
}

fn asset_id() -> StableAssetId {
    "c92ab90f-f06a-47e2-9ffb-219a68e41f53".parse().unwrap()
}

fn product(product: ImportedProductId) -> AssetRef {
    AssetRef::Product(AssetProductRef::new(asset_id(), product).unwrap())
}

fn auxiliary() -> ImportedProductId {
    ImportedProductId::parse_subproduct("4a3dcd3a-2e22-4b52-a67c-31e1519bc6da").unwrap()
}

#[derive(Component)]
struct Probe {
    amount: i64,
    drops: Arc<AtomicUsize>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

struct Definition {
    bindings: SceneComponentBindings,
    calls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl Definition {
    fn new() -> Self {
        let calls = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let native_calls = Arc::clone(&calls);
        let native_drops = Arc::clone(&drops);
        let mut registry = ComponentRegistry::new();
        registry
            .register_persistent_component_with_codec::<Probe, _, _>(
                root_schema(PROBE, ComponentValueShape::scalar(ComponentValueKind::I64)),
                move |value| {
                    native_calls.fetch_add(1, Ordering::SeqCst);
                    let amount = value.as_i64().ok_or_else(|| {
                        ComponentCodecError::Message("expected a signed amount".into())
                    })?;
                    if amount < 0 {
                        return Err(ComponentCodecError::Message(
                            "rejected native amount".into(),
                        ));
                    }
                    Ok(Probe {
                        amount,
                        drops: Arc::clone(&native_drops),
                    })
                },
                |probe| Ok(ComponentValue::I64(probe.amount)),
            )
            .unwrap();
        registry.freeze().unwrap();
        let defaults =
            ComponentSchema::new(component(DEFAULTS), "Defaults", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("label"),
                    "Label",
                    ComponentFieldPath::from_fields(["label"]),
                    ComponentValueKind::String,
                    ComponentValue::String("prefabs/ordinary-text.ron".into()),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
        let data = ComponentSchemaOwnerSnapshot::admit(
            owner(),
            ComponentSchemaCatalog {
                components: vec![
                    root_schema(LINK, ComponentValueShape::EntityRef),
                    ComponentSchema::new(component(ASSETS), "Assets", ComponentSchemaVersion::ONE)
                        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                        .with_fields([ComponentFieldSchema::required(
                            ComponentFieldId::new("images"),
                            "Images",
                            ComponentFieldPath::from_fields(["images"]),
                            ComponentValueShape::list(ComponentValueShape::AssetRef),
                        )
                        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                        .with_capability(ComponentCapability::AssetRef)]),
                    defaults,
                ],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        Self {
            bindings: SceneComponentBindings::new(registry.snapshot().unwrap(), [data]).unwrap(),
            calls,
            drops,
        }
    }

    fn cook_files(&self, directory: &Path, name: &str, amount: i64) -> RuntimeSceneCandidate {
        let leaf_path = directory.join(format!("{name}-leaf.prefab.json"));
        let wrapper_path = directory.join(format!("{name}-wrapper.prefab.json"));
        let scene_path = directory.join(format!("{name}.scene.json"));
        let leaf_ref = AssetRef::path(format!("prefabs/{name}-leaf.prefab.json")).unwrap();
        let wrapper_ref = AssetRef::path(format!("prefabs/{name}-wrapper.prefab.json")).unwrap();
        let leaf = PrefabDocument::new(
            [id("actor")],
            [
                native("actor", amount)
                    .with_children([id("friend")])
                    .with_component(
                        component(LINK),
                        data(ComponentValue::EntityReference(
                            EntityReference::SceneLocal {
                                entity: id("friend"),
                            },
                        )),
                    )
                    .with_component(
                        component(ASSETS),
                        data(ComponentValue::map([(
                            "images",
                            ComponentValue::List(vec![
                                encode_asset_reference_value(
                                    &AssetRef::path("images/shared.png").unwrap(),
                                ),
                                encode_asset_reference_value(&product(auxiliary())),
                            ]),
                        )])),
                    )
                    .with_component(
                        component(DEFAULTS),
                        data(ComponentValue::map([] as [(&str, ComponentValue); 0])),
                    ),
                native("friend", 5),
            ],
        );
        let wrapper = PrefabDocument::from_ordered_roots([instance("nested", leaf_ref, None)]);
        let wrapper_key = source(&format!("{name}.wrapper"), SourceDocumentKind::Prefab);
        let mut overlay = PrefabOverlay::new(wrapper_key.clone());
        let mut addition = PrefabAddition::new(id("added"));
        addition
            .components
            .insert(component(PROBE), native_record(9));
        overlay.additions.push(addition);
        overlay.placements.push(PrefabPlacement {
            target: PrefabEntityTarget::addition(id("added")),
            parent: PrefabPlacementParent::Root,
            slot: PrefabPlacementSlot::Last,
        });
        let document = SceneDocument::from_ordered_roots([
            instance("anchor_a", wrapper_ref.clone(), Some(overlay)),
            instance("anchor_b", wrapper_ref.clone(), None),
            native("local", 2),
        ]);
        fs::write(&leaf_path, leaf.to_json_string().unwrap()).unwrap();
        fs::write(&wrapper_path, wrapper.to_json_string().unwrap()).unwrap();
        fs::write(&scene_path, document.to_json_string().unwrap()).unwrap();
        let leaf = PrefabDocumentCandidate::decode_json_bytes(&fs::read(&leaf_path).unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        let wrapper = PrefabDocumentCandidate::decode_json_bytes(&fs::read(&wrapper_path).unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        let resolver = InMemoryPrefabSourceResolver::new()
            .with_prefab(
                AssetRef::path(format!("prefabs/{name}-leaf.prefab.json")).unwrap(),
                source(&format!("{name}.leaf"), SourceDocumentKind::Prefab),
                leaf,
            )
            .with_prefab(wrapper_ref, wrapper_key, wrapper);
        let document = SceneDocumentCandidate::decode_json_bytes(&fs::read(&scene_path).unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        let report = self.bindings.expand_prefabs(&document, &resolver);
        assert!(!report.diagnostics.has_errors(), "{report:?}");
        let expanded: ExpandedSceneCandidate = report
            .candidate
            .unwrap()
            .bind_source(source(name, SourceDocumentKind::Scene));
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(AssetRecord::new(
                asset_id(),
                AssetPath::new("images/shared.png").unwrap(),
                AssetSourceKind::Image,
            ))
            .unwrap();
        let cooked = RuntimeSceneCandidate::cook(
            &expanded,
            &self.bindings,
            &database,
            RuntimeSceneLimits::default(),
        )
        .unwrap();
        // Only the processed bytes and selected compiled definitions survive into execution.
        fs::remove_file(scene_path).unwrap();
        fs::remove_file(wrapper_path).unwrap();
        fs::remove_file(leaf_path).unwrap();
        cooked
    }
}

fn root_schema(name: &str, shape: ComponentValueShape) -> ComponentSchema {
    let reference = match shape {
        ComponentValueShape::EntityRef => Some(ComponentCapability::EntityRef),
        _ => None,
    };
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::empty(),
        shape,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    if let Some(capability) = reference {
        field = field.with_capability(capability);
    }
    ComponentSchema::new(component(name), name, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field])
}

fn native_record(amount: i64) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::I64(amount))
}

fn native(name: &str, amount: i64) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name)).with_component(component(PROBE), native_record(amount))
}

fn data(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner())
}

fn instance(name: &str, source: AssetRef, overrides: Option<PrefabOverlay>) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(name));
    entity.prefab = Some(PrefabInstance { source, overrides });
    entity
}

fn admit(
    cooked: &RuntimeSceneCandidate,
    bindings: &SceneComponentBindings,
) -> RuntimeSceneCandidate {
    RuntimeSceneDocumentCandidate::from_json_bytes(
        &cooked.to_json_bytes().unwrap(),
        cooked.limits(),
    )
    .unwrap()
    .admit(cooked.source_key(), bindings)
    .unwrap()
}

fn resolve(world: &World, receipt: &SpawnedSceneInstance, id: &SceneEntityId) -> Entity {
    match receipt.resolve(world, id) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live Scene member: {other:?}"),
    }
}

#[test]
fn processed_file_runs_native_data_hierarchy_and_typed_provenance_after_source_removal() {
    let definition = Definition::new();
    let directory = tempfile::tempdir().unwrap();
    let cooked = definition.cook_files(directory.path(), "startup", 7);
    assert_eq!(definition.calls.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    let bytes = cooked.to_json_bytes().unwrap();
    let runtime_path = directory.path().join("startup.runtime.json");
    fs::write(&runtime_path, &bytes).unwrap();
    let source_key = cooked.source_key().clone();
    let provenance = cooked.provenance().clone();
    let expected = cooked.document().clone();
    drop(cooked);
    let candidate = RuntimeSceneDocumentCandidate::from_json_bytes(
        &fs::read(runtime_path).unwrap(),
        RuntimeSceneLimits::default(),
    )
    .unwrap()
    .admit(&source_key, &definition.bindings)
    .unwrap();
    assert_eq!(definition.calls.load(Ordering::SeqCst), 0);
    assert_eq!(candidate.provenance(), &provenance);
    assert_eq!(candidate.to_json_bytes().unwrap(), bytes);
    assert!(
        candidate
            .document()
            .entities
            .iter()
            .all(|entity| entity.prefab.is_none())
    );
    let mut world = World::new();
    let prepared = definition
        .bindings
        .prepare_runtime_scene(&mut world, &candidate, &occurrence())
        .unwrap();
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    let decoded = definition.calls.load(Ordering::SeqCst);
    assert!(decoded > 0);
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let receipt = report.instance.unwrap();
    assert_eq!(definition.calls.load(Ordering::SeqCst), decoded);
    let mut nested_roots = Vec::new();
    let mut additions = 0;
    for record in &expected.entities {
        let entity = resolve(&world, &receipt, &record.id);
        let actual = world.get::<SceneEntitySource>(entity).unwrap();
        assert_eq!(actual.containing_source(), &source_key);
        assert_eq!(actual.occurrence(), &occurrence());
        assert_eq!(actual.provenance(), &provenance[&record.id]);
        for child in &record.children {
            let child = resolve(&world, &receipt, child);
            assert_eq!(world.get::<Parent>(child).unwrap().parent(), entity);
        }
        if let Some(path) = actual.provenance().projection_path() {
            if actual.provenance().local_entity_id() == &id("actor") {
                assert_eq!(path.steps().len(), 2);
                nested_roots.push(path.root_anchor().clone());
            }
            if matches!(
                path.steps().last(),
                Some(PrefabProjectionStep::InstanceAddition { .. })
            ) {
                additions += 1;
            }
        }
        if let Some(link) = definition
            .bindings
            .data_component(&world, entity, &component(LINK))
            .unwrap()
        {
            let ComponentValue::EntityReference(EntityReference::SceneLocal { entity: target }) =
                &link.value
            else {
                panic!("expected a remapped local reference")
            };
            let target = resolve(&world, &receipt, target);
            assert_eq!(world.get::<Probe>(target).unwrap().amount, 5);
            assert_eq!(world.get::<Parent>(target).unwrap().parent(), entity);
        }
        if let Some(assets) = definition
            .bindings
            .data_component(&world, entity, &component(ASSETS))
            .unwrap()
        {
            let ComponentValue::List(images) = assets.value.get("images").unwrap() else {
                panic!("expected asset list")
            };
            assert_eq!(
                decode_asset_reference_value(&images[0]).unwrap(),
                product(ImportedProductId::PRIMARY)
            );
            assert_eq!(
                decode_asset_reference_value(&images[1]).unwrap(),
                product(auxiliary())
            );
        }
        if let Some(defaults) = definition
            .bindings
            .data_component(&world, entity, &component(DEFAULTS))
            .unwrap()
        {
            assert_eq!(
                defaults.value.get("label").unwrap(),
                &ComponentValue::String("prefabs/ordinary-text.ron".into())
            );
        }
    }
    nested_roots.sort();
    assert_eq!(nested_roots, [id("anchor_a"), id("anchor_b")]);
    assert_eq!(additions, 1);
    assert_eq!(
        retire_and_despawn_scene_instance(&mut world, &receipt).unwrap(),
        expected.entities.len()
    );
    assert_eq!(definition.drops.load(Ordering::SeqCst), decoded);
}

#[derive(Resource)]
struct Revision(u32, Arc<()>);

impl Revision {
    fn new(value: u32) -> Self {
        Self(value, Arc::new(()))
    }
}

impl SceneProductResource for Revision {
    type Baseline = (u32, Arc<()>);

    fn capture_baseline(&self) -> Self::Baseline {
        (self.0, Arc::clone(&self.1))
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        self.0 == baseline.0 && Arc::ptr_eq(&self.1, &baseline.1)
    }
}

#[derive(Resource, Default)]
struct NativeRetirements(usize);

#[derive(Component)]
#[component(on_remove = record_native_retirement)]
struct NativeOwned;

fn record_native_retirement(mut world: DeferredWorld<'_>, _: HookContext) {
    world.resource_mut::<NativeRetirements>().0 += 1;
}

#[test]
fn processed_replacement_shares_last_good_and_product_retry_without_repeating_callbacks() {
    let definition = Definition::new();
    let directory = tempfile::tempdir().unwrap();
    let initial = admit(
        &definition.cook_files(directory.path(), "initial", 1),
        &definition.bindings,
    );
    let next = admit(
        &definition.cook_files(directory.path(), "next", 8),
        &definition.bindings,
    );
    let rejected = admit(
        &definition.cook_files(directory.path(), "rejected", -1),
        &definition.bindings,
    );
    let mut world = World::new();
    world.insert_resource(Revision::new(1));
    world.init_resource::<NativeRetirements>();
    let current = definition
        .bindings
        .prepare_runtime_scene(&mut world, &initial, &occurrence())
        .unwrap()
        .commit(&mut world)
        .instance
        .unwrap();
    let old_entity = resolve(&world, &current, &id("local"));
    let identity = world.resource::<WorldIdentityDomain>().stats();
    assert!(
        definition
            .bindings
            .prepare_runtime_scene_replacement(&mut world, &rejected, &current)
            .is_err()
    );
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    assert_eq!(world.resource::<Revision>().0, 1);
    assert_eq!(world.get::<Probe>(old_entity).unwrap().amount, 2);
    let stale = definition
        .bindings
        .prepare_runtime_scene_replacement(&mut world, &next, &current)
        .unwrap();
    world.entity_mut(old_entity).insert(NativeOwned);
    let claims = SceneNativeRetirementClaims::from_component_ids([world
        .component_id::<NativeOwned>()
        .unwrap()]);
    let callbacks = AtomicUsize::new(0);
    let assets = SceneAssetBindings::from_world(&world);
    let pending = definition
        .bindings
        .prepare_runtime_scene_replacement_with_claims(
            &mut world,
            &next,
            &current,
            assets,
            &claims,
            SceneProductTransactionLimits::new(ItemLimit::new(4).unwrap(), ItemLimit::ONE),
            &[],
            |writer| {
                callbacks.fetch_add(1, Ordering::SeqCst);
                writer.replace_resource(Revision::new(2));
            },
        )
        .unwrap_or_else(|report| panic!("advanced preparation failed: {report:?}"));
    assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(world.resource::<Revision>().0, 1);
    let decoded = definition.calls.load(Ordering::SeqCst);
    let mut foreign = World::new();
    let pending = match pending.commit(&mut foreign) {
        Err((pending, report)) => {
            assert!(report.diagnostics.has_errors());
            pending
        }
        Ok(_) => panic!("foreign World accepted prepared replacement"),
    };
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    pending.validate_baseline(&mut world).unwrap();
    let pending = match pending.commit(&mut world) {
        Err((pending, report)) => {
            assert!(report.diagnostics.has_errors());
            pending
        }
        Ok(_) => panic!("claimed native owner had not completed its retirement"),
    };
    assert_eq!(world.resource::<NativeRetirements>().0, 0);
    assert_eq!(world.resource::<Revision>().0, 1);
    assert_eq!(resolve(&world, &current, &id("local")), old_entity);
    world.entity_mut(old_entity).remove::<NativeOwned>();
    assert_eq!(world.resource::<NativeRetirements>().0, 1);
    let report = match pending.commit(&mut world) {
        Ok(report) => report,
        Err(_) => panic!("replacement did not commit in its original World"),
    };
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let current = report.instance.unwrap();
    assert_eq!(world.resource::<Revision>().0, 2);
    assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(definition.calls.load(Ordering::SeqCst), decoded);
    assert_eq!(world.resource::<NativeRetirements>().0, 1);
    assert!(world.get_entity(old_entity).is_err());
    let before = world.resource::<WorldIdentityDomain>().stats();
    let refused = stale.commit(&mut world);
    assert!(refused.instance.is_none());
    assert!(refused.diagnostics.has_errors());
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), before);
    assert_eq!(world.resource::<Revision>().0, 2);
    assert_eq!(
        world
            .get::<SceneEntitySource>(resolve(&world, &current, &id("local")))
            .unwrap()
            .containing_source(),
        next.source_key()
    );
    retire_and_despawn_scene_instance(&mut world, &current).unwrap();
}

#[test]
fn processed_candidates_require_expected_source_exact_definitions_and_cancellable_preparation() {
    let definition = Definition::new();
    let directory = tempfile::tempdir().unwrap();
    let candidate = definition.cook_files(directory.path(), "identity", 3);
    let bytes = candidate.to_json_bytes().unwrap();
    assert!(
        RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, candidate.limits())
            .unwrap()
            .admit(
                &source("other", SourceDocumentKind::Scene),
                &definition.bindings
            )
            .is_err()
    );
    let equivalent = Definition::new();
    let mut world = World::new();
    assert!(
        equivalent
            .bindings
            .prepare_runtime_scene(&mut world, &candidate, &occurrence())
            .is_err()
    );
    assert_eq!(equivalent.calls.load(Ordering::SeqCst), 0);
    let mut control = World::new();
    control.init_resource::<nara_ecs::bevy_ecs::entity_disabling::DefaultQueryFilters>();
    let pending = definition
        .bindings
        .prepare_runtime_scene(&mut world, &candidate, &occurrence())
        .unwrap();
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    let decoded = definition.calls.load(Ordering::SeqCst);
    assert!(decoded > 0);
    assert_eq!(definition.drops.load(Ordering::SeqCst), 0);
    drop(pending);
    assert_eq!(definition.drops.load(Ordering::SeqCst), decoded);
    assert_eq!(world.spawn_empty().id(), control.spawn_empty().id());
    // Explicit readmission can choose an equivalent new selection; a candidate cannot silently
    // cross that boundary merely because its persisted Schema declarations are equal.
    let readmitted = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, candidate.limits())
        .unwrap()
        .admit(candidate.source_key(), &equivalent.bindings)
        .unwrap();
    assert!(readmitted.definitions().matches(&equivalent.bindings));
    assert!(!readmitted.definitions().matches(&definition.bindings));
}

fn exercise_managed_runtime_scene(world: &mut World) {
    let selected = scene_component_bindings(world).unwrap().clone();
    let input = ExpandedSceneCandidate::plain(
        source("managed", SourceDocumentKind::Scene),
        Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            id("managed"),
        )
        .with_component(
            component("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("processed".into()),
            ),
        )
        .with_component(
            component(DEFAULTS),
            data(ComponentValue::map([] as [(&str, ComponentValue); 0])),
        )])),
    );
    let candidate = RuntimeSceneCandidate::cook(
        &input,
        &selected,
        &ProjectAssetDatabase::default(),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let candidate = admit(&candidate, &selected);
    let current = selected
        .prepare_runtime_scene(world, &candidate, &occurrence())
        .unwrap()
        .commit(world)
        .instance
        .unwrap();
    let entity = resolve(world, &current, &id("managed"));
    assert_eq!(world.get::<Name>(entity), Some(&Name::new("processed")));
    assert_eq!(
        selected
            .data_component(world, entity, &component(DEFAULTS))
            .unwrap()
            .unwrap()
            .value
            .get("label"),
        Some(&ComponentValue::String("prefabs/ordinary-text.ron".into()))
    );
    let foreign = SceneComponentBindings::new(
        selected.native_snapshot().clone(),
        Definition::new().bindings.data_owners().iter().cloned(),
    )
    .unwrap();
    // Readmission proves compatibility with the supplied definitions, but does not replace the
    // managed World's installed owner admissions.
    let foreign_candidate = admit(&candidate, &foreign);
    assert!(
        foreign
            .prepare_runtime_scene_replacement(world, &foreign_candidate, &current)
            .is_err()
    );
    assert_eq!(resolve(world, &current, &id("managed")), entity);
    retire_and_despawn_scene_instance(world, &current).unwrap();
}

#[test]
fn processed_native_data_scene_uses_actual_installed_managed_definitions() {
    let definition = Definition::new();
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        SceneComponentsPlugin,
        SceneComponentBindingsPlugin::new(definition.bindings.data_owners().iter().cloned()),
    ))
    .unwrap();
    app.add_systems(StartupStage::Scene, exercise_managed_runtime_scene)
        .unwrap();
    let runtime = RuntimeInstance::start(app).unwrap();
    runtime.finish().unwrap();
}
