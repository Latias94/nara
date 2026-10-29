//! Native asset codecs consume complete Runtime product bindings without source metadata.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_asset::{
    AssetPath, AssetProductRef, AssetRecord, AssetRef, AssetServer, AssetSourceKind, Assets,
    Handle, ImportedProductId, ProjectAssetDatabase, StableAssetId,
};
use nara_core::ItemLimit;
use nara_ecs::{Component, Entity, Resource, World};
use nara_identity::{EntityLookup, SpawnedSceneInstance, WorldIdentityDomain};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValueShape, PreparedComponentCandidate, decode_asset_reference_value,
    encode_asset_reference_value,
};
use nara_scene::{
    ExpandedSceneCandidate, RuntimeSceneCandidate, RuntimeSceneDocumentCandidate,
    RuntimeSceneLimits, SceneComponentBindings, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityRecord, SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind,
    advanced::{
        SceneAssetBindings, SceneNativeRetirementClaims, SceneProductResource,
        SceneProductTransactionLimits,
    },
    retire_and_despawn_scene_instance,
};

#[derive(Component)]
struct Sample(Handle<String>);

#[derive(Resource, Default)]
struct TextProducts {
    values: Assets<String>,
    revision: Arc<()>,
}

impl TextProducts {
    fn insert(&mut self, handle: Handle<String>, value: String) {
        self.values.insert(handle, value);
        // Retain write history even when the same slot returns to the same content.
        self.revision = Arc::new(());
    }
}

impl SceneProductResource for TextProducts {
    type Baseline = Arc<()>;

    fn capture_baseline(&self) -> Self::Baseline {
        Arc::clone(&self.revision)
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        Arc::ptr_eq(&self.revision, baseline)
    }
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("consumer.runtime.Sample")
}
fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn source() -> SourceDocumentKey {
    SourceDocumentKey::named(
        "7b0cf713-1626-4ba9-ad62-d3684b61bff9".parse().unwrap(),
        "processed-assets",
        SourceDocumentKind::Scene,
    )
    .unwrap()
}
fn source_id() -> StableAssetId {
    "b156e916-8932-48e5-aa12-c022c4f529f7".parse().unwrap()
}
fn product(selection: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(source_id(), selection).unwrap()
}
fn subproduct_id(value: &str) -> ImportedProductId {
    ImportedProductId::parse_subproduct(value).unwrap()
}
fn second() -> AssetProductRef {
    product(subproduct_id("5171f84c-c2b0-4b57-991f-5d25778896a4"))
}
fn missing() -> AssetProductRef {
    product(subproduct_id("cead7ad0-f919-4c9a-8cc2-471a18ccb507"))
}
fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("consumer.runtime-assets", "current").unwrap()
}

fn bindings(calls: &Arc<AtomicUsize>) -> SceneComponentBindings {
    let mut registry = ComponentRegistry::new();
    let calls = Arc::clone(calls);
    registry
        .register_persistent_component_codec_with_context::<Sample, _, _>(
            ComponentSchema::new(component(), "Sample", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("value"),
                    "Value",
                    ComponentFieldPath::empty(),
                    ComponentValueShape::AssetRef,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_capability(ComponentCapability::AssetRef)]),
            move |value, _| {
                let reference = decode_asset_reference_value(value)?;
                let calls = Arc::clone(&calls);
                Ok(PreparedComponentCandidate::with_asset_server(
                    move |context| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        context
                            .resolve_asset_ref::<String>(&reference)
                            .map(Sample)
                            .map_err(|error| ComponentCodecError::Message(error.to_string()))
                    },
                ))
            },
            |sample, world, context| {
                let reference = AssetRef::from_handle_with_policy(
                    world.resource::<AssetServer>(),
                    sample.0,
                    context.asset_ref_export_policy(),
                )
                .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                Ok(encode_asset_reference_value(&reference))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    SceneComponentBindings::new(registry.snapshot().unwrap(), []).unwrap()
}

fn cooked(
    bindings: &SceneComponentBindings,
    references: &[AssetProductRef],
) -> RuntimeSceneCandidate {
    let document = SceneDocument::from_ordered_roots(references.iter().enumerate().map(
        |(index, reference)| {
            SceneEntityRecord::new(id(&format!("sample{index}"))).with_component(
                component(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    encode_asset_reference_value(&AssetRef::Product(*reference)),
                ),
            )
        },
    ));
    let expanded = ExpandedSceneCandidate::plain(source(), Arc::new(document));
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            source_id(),
            AssetPath::new("source/bundle.txt").unwrap(),
            AssetSourceKind::Other("consumer.text".into()),
        ))
        .unwrap();
    let cooked = RuntimeSceneCandidate::cook(
        &expanded,
        bindings,
        &database,
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let bytes = cooked.to_json_bytes().unwrap();
    drop(cooked);
    drop(expanded);
    drop(database);
    RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, RuntimeSceneLimits::default())
        .unwrap()
        .admit(&source(), bindings)
        .unwrap()
}

fn world_with_products() -> (World, Handle<String>, Handle<String>) {
    let mut server = AssetServer::new();
    let primary = server
        .reserve_product::<String>(product(ImportedProductId::PRIMARY))
        .unwrap();
    let auxiliary = server.reserve_product::<String>(second()).unwrap();
    assert_ne!(primary.id(), auxiliary.id());
    assert!(server.path(primary.id()).is_none());
    assert!(server.path(auxiliary.id()).is_none());
    let mut values = TextProducts::default();
    values.insert(primary, "processed-primary".into());
    values.insert(auxiliary, "processed-auxiliary".into());
    let mut world = World::new();
    world.insert_resource(server);
    world.insert_resource(values);
    (world, primary, auxiliary)
}

fn resolve(world: &World, receipt: &SpawnedSceneInstance, index: usize) -> Entity {
    match receipt.resolve(world, &id(&format!("sample{index}"))) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live Scene member: {other:?}"),
    }
}

#[test]
fn processed_native_assets_resolve_actual_primary_and_auxiliary_values_without_source_database() {
    let calls = Arc::new(AtomicUsize::new(0));
    let bindings = bindings(&calls);
    let references = [product(ImportedProductId::PRIMARY), second()];
    let candidate = cooked(&bindings, &references);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let (mut world, primary, auxiliary) = world_with_products();
    let before = world.resource::<AssetServer>().clone();
    let pending = bindings
        .prepare_runtime_scene(&mut world, &candidate, &occurrence())
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    let report = pending.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let current = report.instance.unwrap();
    for (index, expected, text) in [
        (0, primary, "processed-primary"),
        (1, auxiliary, "processed-auxiliary"),
    ] {
        let sample = world
            .get::<Sample>(resolve(&world, &current, index))
            .unwrap();
        assert_eq!(sample.0, expected);
        assert_eq!(
            world
                .resource::<TextProducts>()
                .values
                .get(sample.0)
                .unwrap(),
            text
        );
        assert_eq!(
            world.resource::<AssetServer>().product(sample.0.id()),
            Some(references[index])
        );
    }
    let output = bindings.export_scene(&world).into_output().unwrap();
    for reference in references {
        assert!(output.document.entities.iter().any(|entity| {
            decode_asset_reference_value(&entity.components[&component()].value).unwrap()
                == AssetRef::Product(reference)
        }));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    retire_and_despawn_scene_instance(&mut world, &current).unwrap();
    assert_eq!(
        world
            .resource::<TextProducts>()
            .values
            .get(primary)
            .unwrap(),
        "processed-primary"
    );
    assert_eq!(
        world
            .resource::<TextProducts>()
            .values
            .get(auxiliary)
            .unwrap(),
        "processed-auxiliary"
    );
}

#[test]
fn processed_native_missing_wrong_typed_and_stale_bindings_preserve_current_scene_and_products() {
    let calls = Arc::new(AtomicUsize::new(0));
    let bindings = bindings(&calls);
    let initial = cooked(&bindings, &[product(ImportedProductId::PRIMARY)]);
    let bad = cooked(&bindings, &[missing()]);
    let next = cooked(&bindings, &[second()]);
    let (mut world, primary, auxiliary) = world_with_products();
    let current = bindings
        .prepare_runtime_scene(&mut world, &initial, &occurrence())
        .unwrap()
        .commit(&mut world)
        .instance
        .unwrap();
    let old = resolve(&world, &current, 0);
    let identity = world.resource::<WorldIdentityDomain>().stats();
    let before = world.resource::<AssetServer>().clone();
    assert!(
        bindings
            .prepare_runtime_scene_replacement(&mut world, &bad, &current)
            .is_err()
    );
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    assert_eq!(resolve(&world, &current, 0), old);
    world
        .resource_mut::<AssetServer>()
        .reserve_product::<u64>(missing())
        .unwrap();
    let before = world.resource::<AssetServer>().clone();
    assert!(
        bindings
            .prepare_runtime_scene_replacement(&mut world, &bad, &current)
            .is_err()
    );
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    let stale = bindings
        .prepare_runtime_scene_replacement(&mut world, &next, &current)
        .unwrap();
    let extra = product(subproduct_id("6082cf35-9084-4c08-9e41-4d6d27e28814"));
    world
        .resource_mut::<AssetServer>()
        .reserve_product::<String>(extra)
        .unwrap();
    let before = world.resource::<AssetServer>().clone();
    let report = stale.commit(&mut world);
    assert!(report.instance.is_none());
    assert!(report.diagnostics.has_errors());
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    assert_eq!(resolve(&world, &current, 0), old);
    assert_eq!(world.get::<Sample>(old).unwrap().0, primary);
    assert_eq!(
        world
            .resource::<TextProducts>()
            .values
            .get(primary)
            .unwrap(),
        "processed-primary"
    );
    assert_eq!(
        world
            .resource::<TextProducts>()
            .values
            .get(auxiliary)
            .unwrap(),
        "processed-auxiliary"
    );
    let current = bindings
        .prepare_runtime_scene_replacement(&mut world, &next, &current)
        .unwrap()
        .commit(&mut world)
        .instance
        .unwrap();
    assert_eq!(
        world.get::<Sample>(resolve(&world, &current, 0)).unwrap().0,
        auxiliary
    );
    retire_and_despawn_scene_instance(&mut world, &current).unwrap();
}

fn replacement_values(
    world: &World,
    primary: Handle<String>,
    auxiliary: Handle<String>,
    next: Handle<String>,
) -> TextProducts {
    let current = &world.resource::<TextProducts>().values;
    let mut values = TextProducts::default();
    values.insert(primary, current.get(primary).unwrap().clone());
    values.insert(auxiliary, current.get(auxiliary).unwrap().clone());
    values.insert(next, "new-processed-product".to_owned());
    values
}

#[test]
fn runtime_product_binding_value_and_native_scene_publish_through_one_existing_transaction() {
    let calls = Arc::new(AtomicUsize::new(0));
    let bindings = bindings(&calls);
    let initial = cooked(&bindings, &[product(ImportedProductId::PRIMARY)]);
    let next_ref = product(subproduct_id("c8e74ecd-cdea-4db1-a190-bb30434c1cbf"));
    let next = cooked(&bindings, &[next_ref]);
    let bad = cooked(&bindings, &[next_ref, missing()]);
    let (mut world, primary, auxiliary) = world_with_products();
    let current = bindings
        .prepare_runtime_scene(&mut world, &initial, &occurrence())
        .unwrap()
        .commit(&mut world)
        .instance
        .unwrap();
    let old = resolve(&world, &current, 0);
    let before = world.resource::<AssetServer>().clone();
    let identity = world.resource::<WorldIdentityDomain>().stats();
    let callbacks = AtomicUsize::new(0);
    let claims = SceneNativeRetirementClaims::default();
    let limits = SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE);

    let mut assets = SceneAssetBindings::from_world(&world);
    let rejected_handle = assets.reserve_product::<String>(next_ref).unwrap();
    let values = replacement_values(&world, primary, auxiliary, rejected_handle);
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert!(
        bindings
            .prepare_runtime_scene_replacement_with_claims(
                &mut world,
                &bad,
                &current,
                assets,
                &claims,
                limits,
                &[],
                |writer| {
                    callbacks.fetch_add(1, Ordering::SeqCst);
                    writer.replace_resource(values);
                },
            )
            .is_err()
    );
    // Configuration stages private values before deferred native materialization. A later
    // codec refusal cancels publication; it does not undo the callback's external counter.
    assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    assert_eq!(resolve(&world, &current, 0), old);
    assert!(
        world
            .resource::<TextProducts>()
            .values
            .get(rejected_handle)
            .is_none()
    );

    let mut assets = SceneAssetBindings::from_world(&world);
    let stale_handle = assets.reserve_product::<String>(next_ref).unwrap();
    let values = replacement_values(&world, primary, auxiliary, stale_handle);
    let pending = bindings
        .prepare_runtime_scene_replacement_with_claims(
            &mut world,
            &next,
            &current,
            assets,
            &claims,
            limits,
            &[],
            |writer| {
                callbacks.fetch_add(1, Ordering::SeqCst);
                writer.replace_resource(values);
            },
        )
        .unwrap_or_else(|report| panic!("processed replacement prepare failed: {report:?}"));
    pending.validate_baseline(&mut world).unwrap();
    let decoded = calls.load(Ordering::SeqCst);
    let ticks = world.get_resource_change_ticks::<TextProducts>().unwrap();
    {
        let mut products = world.resource_mut::<TextProducts>();
        products.insert(primary, "intervening-content".into());
        products.insert(primary, "processed-primary".into());
        assert_eq!(products.values.len(), 2);
    }
    let live_baseline = world.resource::<TextProducts>().capture_baseline();
    assert_eq!(
        world
            .get_resource_change_ticks::<TextProducts>()
            .unwrap()
            .changed,
        ticks.changed
    );
    let (pending, report) = match pending.commit(&mut world) {
        Err(failure) => failure,
        Ok(_) => panic!("a same-slot content ABA must reject product replacement"),
    };
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.product-resource-changed" })
    );
    assert!(report.instance.is_none());
    assert_eq!(callbacks.load(Ordering::SeqCst), 2);
    assert_eq!(calls.load(Ordering::SeqCst), decoded);
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    assert_eq!(resolve(&world, &current, 0), old);
    assert!(
        world
            .resource::<TextProducts>()
            .matches_baseline(&live_baseline)
    );
    assert!(
        world
            .resource::<TextProducts>()
            .values
            .get(stale_handle)
            .is_none()
    );
    assert!(pending.discard(&mut world).is_ok());
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);

    let mut assets = SceneAssetBindings::from_world(&world);
    let canceled_handle = assets.reserve_product::<String>(next_ref).unwrap();
    assert_ne!(rejected_handle.id(), canceled_handle.id());
    let values = replacement_values(&world, primary, auxiliary, canceled_handle);
    let pending = bindings
        .prepare_runtime_scene_replacement_with_claims(
            &mut world,
            &next,
            &current,
            assets,
            &claims,
            limits,
            &[],
            |writer| {
                callbacks.fetch_add(1, Ordering::SeqCst);
                writer.replace_resource(values);
            },
        )
        .unwrap_or_else(|report| panic!("processed replacement prepare failed: {report:?}"));
    assert_eq!(callbacks.load(Ordering::SeqCst), 3);
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert!(pending.discard(&mut world).is_ok());
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    assert!(
        world
            .resource::<TextProducts>()
            .values
            .get(canceled_handle)
            .is_none()
    );

    let mut assets = SceneAssetBindings::from_world(&world);
    let next_handle = assets.reserve_product::<String>(next_ref).unwrap();
    assert_ne!(canceled_handle.id(), next_handle.id());
    let values = replacement_values(&world, primary, auxiliary, next_handle);
    let pending = bindings
        .prepare_runtime_scene_replacement_with_claims(
            &mut world,
            &next,
            &current,
            assets,
            &claims,
            limits,
            &[],
            |writer| {
                callbacks.fetch_add(1, Ordering::SeqCst);
                writer.replace_resource(values);
            },
        )
        .unwrap_or_else(|report| panic!("processed replacement prepare failed: {report:?}"));
    let decoded = calls.load(Ordering::SeqCst);
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert_eq!(resolve(&world, &current, 0), old);
    assert!(
        world
            .resource::<TextProducts>()
            .values
            .get(next_handle)
            .is_none()
    );
    let report = pending
        .commit(&mut world)
        .unwrap_or_else(|_| panic!("processed replacement commit failed"));
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let current = report.instance.unwrap();
    assert_eq!(callbacks.load(Ordering::SeqCst), 4);
    assert_eq!(calls.load(Ordering::SeqCst), decoded);
    assert!(world.get_entity(old).is_err());
    assert_eq!(
        world.get::<Sample>(resolve(&world, &current, 0)).unwrap().0,
        next_handle
    );
    assert_eq!(
        world.resource::<AssetServer>().product(next_handle.id()),
        Some(next_ref)
    );
    assert_eq!(
        world
            .resource::<TextProducts>()
            .values
            .get(next_handle)
            .unwrap(),
        "new-processed-product"
    );
    retire_and_despawn_scene_instance(&mut world, &current).unwrap();
}
