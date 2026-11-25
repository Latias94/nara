use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use nara_core::diagnostic::{DiagnosticValueRef, ReportDiagnostic as Diagnostic};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::identity::{EntityLookup, WorldIdentityDomain};
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistryBuilder, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueCost, ComponentValueKind, ComponentValueMaterializationLimits,
    ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    CompiledSceneCandidate, CompiledSceneComponent, InMemoryPrefabSourceResolver,
    SceneComponentBindings, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind,
};

const DATA: &str = "test.ValueMetricsData";
const NATIVE: &str = "test.ValueMetricsNative";
const TRACKED_TEXT: &str = "value-metrics-owned-source-payload";

#[global_allocator]
static ALLOCATOR: LifetimeAllocator = LifetimeAllocator;

static TRACKED_ADDRESS: AtomicUsize = AtomicUsize::new(0);
static TRACKED_FREED: AtomicBool = AtomicBool::new(false);

struct LifetimeAllocator;

// SAFETY: Allocation and deallocation are forwarded unchanged to System. Observation uses only
// atomics and neither allocates nor accesses the allocation's contents.
unsafe impl GlobalAlloc for LifetimeAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller's valid layout is forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if TRACKED_ADDRESS
            .compare_exchange(pointer.addr(), 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            TRACKED_FREED.store(true, Ordering::SeqCst);
        }
        // SAFETY: The caller's live pointer and original layout are forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[derive(Component)]
struct NativePayload(ComponentValue);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.value-metrics-data").unwrap()
}

fn schema(name: &str) -> ComponentSchema {
    ComponentSchema::new(component(name), name, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("payload"),
                "Payload",
                ComponentFieldPath::from_fields(["payload"]),
                ComponentValueShape::opaque(ComponentValueKind::List),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("visible"),
                "Visible",
                ComponentFieldPath::from_fields(["settings", "visible"]),
                ComponentValueKind::Bool,
                ComponentValue::Bool(true),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ])
}

fn bindings_with_schemas(
    native_schema: ComponentSchema,
    data_schema: ComponentSchema,
    calls: &Arc<AtomicUsize>,
) -> SceneComponentBindings {
    let observed = Arc::clone(calls);
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_with_codec::<NativePayload, _, _>(
            native_schema,
            move |value| {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(NativePayload(value.to_owned()))
            },
            |payload| Ok(payload.0.clone()),
        )
        .unwrap();
    let data_owner = ComponentSchemaOwnerSnapshot::admit(
        owner(),
        ComponentSchemaCatalog {
            components: vec![data_schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    SceneComponentBindings::new(registry.freeze().unwrap(), [data_owner]).unwrap()
}

fn bindings(calls: &Arc<AtomicUsize>) -> SceneComponentBindings {
    bindings_with_schemas(schema(NATIVE), schema(DATA), calls)
}

fn value(payload: Vec<ComponentValue>) -> ComponentValue {
    ComponentValue::map([("payload", ComponentValue::List(payload))])
}

fn explicit_value(value: ComponentValue) -> ComponentValue {
    let ComponentValue::Map(mut fields) = value else {
        panic!("the fixture has a map root")
    };
    fields
        .entry("settings".into())
        .or_insert_with(|| ComponentValue::map([("visible", ComponentValue::Bool(true))]));
    ComponentValue::Map(fields)
}

fn document() -> SceneDocument {
    let mut document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("z"))
            .with_component(
                component(NATIVE),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    value(vec![ComponentValue::List(vec![ComponentValue::List(
                        vec![ComponentValue::U64(9)],
                    )])]),
                ),
            )
            .with_component(
                component(DATA),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    value(vec![ComponentValue::String(TRACKED_TEXT.into())]),
                )
                .with_owner(owner()),
            ),
        SceneEntityRecord::new(id("a"))
            .with_component(
                component(NATIVE),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    value(vec![ComponentValue::U64(1)]),
                ),
            )
            .with_component(
                component(DATA),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    explicit_value(value(vec![])),
                )
                .with_owner(owner()),
            ),
    ]);
    document.entities.reverse();
    document
}

fn compile(document: &SceneDocument, bindings: &SceneComponentBindings) -> CompiledSceneCandidate {
    let report = bindings.compile_scene(document, &InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.candidate.unwrap().bind_source(source_key())
}

fn source_key() -> SourceDocumentKey {
    SourceDocumentKey::named(
        "67558495-d689-463a-8e70-efc7daf0926e".parse().unwrap(),
        "compiled-value-metrics.scene",
        SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("compiled-value-metrics", "main").unwrap()
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn assert_metrics(record: &CompiledSceneComponent, expected: &ComponentValue, depth: usize) {
    assert_eq!(record.value(), expected);
    assert_eq!(record.value_cost(), expected.cost());
    assert_eq!(record.value_depth(), depth);
}

fn assert_field(diagnostic: &Diagnostic, key: &str, expected: DiagnosticValueRef<'_>) {
    let field = diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .unwrap_or_else(|| panic!("missing {key}: {diagnostic:?}"));
    assert_eq!(field.value(), expected, "{key}: {diagnostic:?}");
}

#[test]
fn sorted_records_keep_metrics_and_release_source_values_with_their_last_owner() {
    let calls = Arc::new(AtomicUsize::new(0));
    let bindings = bindings(&calls);
    let source = Arc::new(document());
    let weak_source = Arc::downgrade(&source);
    assert_eq!(source.entities[0].id, id("z"));
    let ComponentValue::List(payload) = source.entities[0].components[&component(DATA)]
        .value()
        .get("payload")
        .unwrap()
    else {
        panic!("the tracked source contains a list")
    };
    let ComponentValue::String(text) = &payload[0] else {
        panic!("the tracked source contains a string")
    };
    let address = text.as_ptr().addr();
    TRACKED_FREED.store(false, Ordering::SeqCst);
    assert_eq!(TRACKED_ADDRESS.swap(address, Ordering::SeqCst), 0);

    let candidate = compile(&source, &bindings);
    assert_eq!(candidate.document().roots(), &[id("z"), id("a")]);
    assert_eq!(candidate.document().entities()[0].id(), &id("a"));
    assert_eq!(candidate.document().entities()[1].id(), &id("z"));
    let mut expected = Vec::new();
    for entity in &source.entities {
        let compiled = candidate
            .document()
            .entities()
            .iter()
            .find(|compiled| compiled.id() == &entity.id)
            .unwrap();
        assert_eq!(
            compiled
                .components()
                .keys()
                .map(ComponentTypeId::as_str)
                .collect::<Vec<_>>(),
            [DATA, NATIVE],
        );
        for (component, source_record) in &entity.components {
            let depth = if entity.id == id("z") && component.as_str() == NATIVE {
                5
            } else {
                3
            };
            let effective = explicit_value(source_record.value().clone());
            let record = &compiled.components()[component];
            assert_metrics(record, &effective, depth);
            assert_eq!(
                record.value().as_value().is_none(),
                source_record.value().get("settings").is_none(),
            );
            expected.push((entity.id.clone(), component.clone(), effective, depth));
        }
    }
    let retained_cost =
        candidate.document().entities()[1].components()[&component(DATA)].value_cost();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(source);
    assert!(weak_source.upgrade().is_none());
    assert!(!TRACKED_FREED.load(Ordering::SeqCst));

    let mut world = World::new();
    let prepared = bindings
        .prepare_scene(&mut world, &candidate, &occurrence(), None)
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    drop(candidate);
    assert!(!TRACKED_FREED.load(Ordering::SeqCst));
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let instance = report.instance.unwrap();
    for (id, component, effective, depth) in &expected {
        let EntityLookup::Resolved(entity) = instance.resolve(&world, id) else {
            panic!("the prepared entity must resolve")
        };
        if component.as_str() == DATA {
            let record = bindings
                .data_component(&world, entity, component)
                .unwrap()
                .unwrap();
            assert_metrics(record, effective, *depth);
        } else {
            assert_eq!(&world.get::<NativePayload>(entity).unwrap().0, effective);
        }
    }
    let EntityLookup::Resolved(entity) = instance.resolve(&world, &id("z")) else {
        panic!("the tracked data entity must resolve")
    };
    let record = bindings
        .data_component(&world, entity, &component(DATA))
        .unwrap()
        .unwrap();
    let ComponentValue::List(payload) = record.value().get("payload").unwrap().as_value().unwrap()
    else {
        panic!("the original opaque list must survive preparation")
    };
    let ComponentValue::String(text) = &payload[0] else {
        panic!("the original opaque string must survive preparation")
    };
    assert_eq!(text.as_ptr().addr(), address);
    assert!(!TRACKED_FREED.load(Ordering::SeqCst));
    assert!(
        bindings
            .remove_data_component(&mut world, entity, &component(DATA))
            .unwrap()
    );
    assert!(TRACKED_FREED.load(Ordering::SeqCst));
    assert_eq!(TRACKED_ADDRESS.load(Ordering::SeqCst), 0);
    assert_eq!(
        retained_cost,
        explicit_value(value(vec![ComponentValue::String(TRACKED_TEXT.into())])).cost(),
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn aggregate_receiving_limits_preserve_exact_last_record_diagnostics_before_codecs() {
    let calls = Arc::new(AtomicUsize::new(0));
    let bindings = bindings(&calls);
    let candidate = compile(&document(), &bindings);
    let total = candidate
        .document()
        .entities()
        .iter()
        .flat_map(|entity| entity.components().values())
        .fold(ComponentValueCost::ZERO, |total, record| {
            total.saturating_add(record.value().cost())
        });
    let last = &candidate.document().entities()[1].components()[&component(NATIVE)];
    assert!(last.value().as_value().is_none());
    let last_cost = last.value().cost();
    let mut world = World::new();
    let sentinel = world.spawn_empty().id();
    let population = world.entities().len();

    for (kind, allowance, observed, maximum) in [
        (
            "materialization-node-limit",
            limits(total.nodes() - 1, total.logical_bytes(), 5),
            last_cost.nodes(),
            last_cost.nodes() - 1,
        ),
        (
            "materialization-byte-limit",
            limits(total.nodes(), total.logical_bytes() - 1, 5),
            last_cost.logical_bytes(),
            last_cost.logical_bytes() - 1,
        ),
        (
            "materialization-depth-limit",
            limits(total.nodes(), total.logical_bytes(), 4),
            5,
            4,
        ),
        (
            "materialization-depth-limit",
            limits(total.nodes(), total.logical_bytes(), 3),
            4,
            3,
        ),
    ] {
        let rejected = bindings
            .prepare_scene_with_limits(&mut world, &candidate, &occurrence(), None, allowance)
            .expect_err(kind);
        let diagnostic = rejected
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == "scene.invalid-component-payload")
            .unwrap_or_else(|| panic!("{kind}: {rejected:?}"));
        for (key, expected) in [
            ("codec-error-kind", DiagnosticValueRef::Identifier(kind)),
            ("entity-id", DiagnosticValueRef::Identifier("z")),
            ("component-id", DiagnosticValueRef::Identifier(NATIVE)),
            ("observed", DiagnosticValueRef::Unsigned(observed as u64)),
            ("maximum", DiagnosticValueRef::Unsigned(maximum as u64)),
        ] {
            assert_field(diagnostic, key, expected);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0, "{kind}");
        assert_eq!(world.entities().len(), population, "{kind}");
        assert!(world.get_entity(sentinel).is_ok(), "{kind}");
        assert!(!world.contains_resource::<WorldIdentityDomain>(), "{kind}");
    }

    let prepared = bindings
        .prepare_scene_with_limits(
            &mut world,
            &candidate,
            &occurrence(),
            None,
            limits(total.nodes(), total.logical_bytes(), 5),
        )
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(world.entities().len(), population);
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    assert_eq!(report.instance.unwrap().len(), 2);
    assert!(world.get_entity(sentinel).is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn receiving_schema_and_new_defaults_cannot_reuse_value_facts_as_definition_admission() {
    let source_calls = Arc::new(AtomicUsize::new(0));
    let source_bindings = bindings(&source_calls);
    let source = document();
    let candidate = compile(&source, &source_bindings);
    let before = candidate
        .document()
        .entities()
        .iter()
        .flat_map(|entity| entity.components().values())
        .map(|record| (record.value_cost(), record.value_depth()))
        .collect::<Vec<_>>();
    let mut narrower = schema(NATIVE);
    narrower.fields[0].shape = ComponentValueShape::list(ComponentValueKind::U64.into());
    let mut added_default = schema(NATIVE);
    added_default.fields.push(
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("added"),
            "Added",
            ComponentFieldPath::from_fields(["added"]),
            ComponentValueKind::String,
            ComponentValue::String("receiving-default".into()),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    );

    for (receiving_schema, code, kind) in [
        (
            narrower,
            "scene.invalid-component-payload",
            Some("invalid-field"),
        ),
        (added_default, "scene.compiled-incomplete-value", None),
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let receiving = bindings_with_schemas(receiving_schema, schema(DATA), &calls);
        let mut world = World::new();
        let sentinel = world.spawn_empty().id();
        let population = world.entities().len();
        let rejected = receiving
            .prepare_scene(&mut world, &candidate, &occurrence(), None)
            .expect_err("unchanged value metrics cannot admit changed receiving definitions");
        let diagnostic = rejected
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == code)
            .unwrap_or_else(|| panic!("{code}: {rejected:?}"));
        assert_field(
            diagnostic,
            "component-id",
            DiagnosticValueRef::Identifier(NATIVE),
        );
        if let Some(kind) = kind {
            assert_field(
                diagnostic,
                "codec-error-kind",
                DiagnosticValueRef::Identifier(kind),
            );
            assert_field(diagnostic, "entity-id", DiagnosticValueRef::Identifier("z"));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0, "{code}");
        assert_eq!(world.entities().len(), population, "{code}");
        assert!(world.get_entity(sentinel).is_ok(), "{code}");
        assert!(!world.contains_resource::<WorldIdentityDomain>(), "{code}");
        assert_eq!(
            candidate
                .document()
                .entities()
                .iter()
                .flat_map(|entity| entity.components().values())
                .map(|record| (record.value_cost(), record.value_depth()))
                .collect::<Vec<_>>(),
            before,
        );
    }
    assert_eq!(source_calls.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "serde")]
#[test]
fn data_replacement_cook_rewrite_and_processed_readmission_measure_their_own_values() {
    use nara_asset::{
        AssetPathScope, AssetProductRef, AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath,
        ImportedProductId, ProjectAssetDatabase,
    };
    use nara_reflect::encode_asset_reference_value;
    use nara_scene::{RuntimeSceneCandidate, RuntimeSceneDocumentCandidate, RuntimeSceneLimits};

    let mut data_schema = schema(DATA);
    data_schema.fields.push(
        ComponentFieldSchema::required(
            ComponentFieldId::new("image"),
            "Image",
            ComponentFieldPath::from_fields(["image"]),
            ComponentValueShape::from(ComponentValueKind::AssetRef),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::AssetRef),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let bindings = bindings_with_schemas(schema(NATIVE), data_schema, &calls);
    let stable = "f790246e-50f1-4537-8590-e211d917d075".parse().unwrap();
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            stable,
            AssetSourcePath::new("metrics.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    let resolver = database.resolver(&AssetPathScope::ROOT);
    let path = AssetRef::path("metrics.png").unwrap();
    let product =
        AssetRef::Product(AssetProductRef::new(stable, ImportedProductId::PRIMARY).unwrap());
    let with_image = |value, reference: &AssetRef| {
        let ComponentValue::Map(mut fields) = value else {
            panic!("the asset fixture has a map root")
        };
        fields.insert("image".into(), encode_asset_reference_value(reference));
        ComponentValue::Map(fields)
    };
    let source_value = with_image(value(vec![]), &path);
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("data"))
        .with_component(
            component(DATA),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, source_value.clone())
                .with_owner(owner()),
        )]);
    let original = compile(&source, &bindings);
    assert_metrics(
        &original.document().entities()[0].components()[&component(DATA)],
        &explicit_value(source_value),
        3,
    );
    let original_cost =
        original.document().entities()[0].components()[&component(DATA)].value_cost();
    let mut world = World::new();
    let report = bindings
        .prepare_scene(&mut world, &original, &occurrence(), Some(resolver))
        .unwrap()
        .commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let instance = report.instance.unwrap();
    let EntityLookup::Resolved(entity) = instance.resolve(&world, &id("data")) else {
        panic!("the data fixture must resolve")
    };
    let replacement = with_image(
        value(vec![ComponentValue::List(vec![ComponentValue::List(
            vec![ComponentValue::String("replacement-payload".into())],
        )])]),
        &path,
    );
    bindings
        .set_data_component(
            &mut world,
            entity,
            &component(DATA),
            replacement.clone(),
            Some(resolver),
            ComponentValueMaterializationLimits::default(),
        )
        .unwrap();
    let replacement = explicit_value(replacement);
    let published = bindings
        .data_component(&world, entity, &component(DATA))
        .unwrap()
        .unwrap();
    assert_metrics(published, &replacement, 5);
    assert!(published.value().as_value().is_some());
    assert_ne!(published.value_cost(), original_cost);
    assert_eq!(
        original.document().entities()[0].components()[&component(DATA)].value_cost(),
        original_cost,
    );

    let updated_source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("data"))
        .with_component(
            component(DATA),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, published.value().to_owned())
                .with_owner(owner()),
        )]);
    let updated = compile(&updated_source, &bindings);
    let cooked =
        RuntimeSceneCandidate::cook(&updated, &bindings, resolver, RuntimeSceneLimits::default())
            .unwrap();
    let cooked_value = with_image(replacement.clone(), &product);
    let cooked_record = &cooked.document().entities()[0].components()[&component(DATA)];
    assert_metrics(cooked_record, &cooked_value, 5);
    assert_ne!(cooked_record.value_cost(), replacement.cost());
    assert_eq!(
        updated.document().entities()[0].components()[&component(DATA)].value_cost(),
        replacement.cost(),
    );
    let bytes = cooked.to_json_bytes().unwrap();
    let readmitted = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, cooked.limits())
        .unwrap()
        .admit(&source_key(), &bindings)
        .unwrap();
    assert_eq!(readmitted.document(), cooked.document());

    let changed_value = explicit_value(with_image(
        value(vec![ComponentValue::List(vec![ComponentValue::List(
            vec![ComponentValue::List(vec![ComponentValue::String(
                "processed-payload".repeat(3),
            )])],
        )])]),
        &product,
    ));
    let mut wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    wire["payload"]["entities"][0]["components"][DATA]["value"] =
        serde_json::to_value(&changed_value).unwrap();
    let changed = RuntimeSceneDocumentCandidate::from_json_bytes(
        &serde_json::to_vec(&wire).unwrap(),
        cooked.limits(),
    )
    .unwrap()
    .admit(&source_key(), &bindings)
    .unwrap();
    let changed_record = &changed.document().entities()[0].components()[&component(DATA)];
    assert_metrics(changed_record, &changed_value, 6);
    assert_ne!(changed_record.value_cost(), cooked_record.value_cost());
    assert_eq!(cooked_record.value_cost(), cooked_value.cost());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
