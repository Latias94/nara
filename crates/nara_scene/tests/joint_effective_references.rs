#[path = "support/compiled_scene.rs"]
mod compiled_scene;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_asset::{
    AssetPathScope, AssetProductRef, AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath,
    ImportedProductId, ProjectAssetDatabase, StableAssetId,
};
use nara_core::diagnostic::{DiagnosticReport, DiagnosticValueRef, ReportDiagnostic as Diagnostic};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::bevy_ecs::resource::IsResource;
use nara_ecs::identity::{EntityLookup, EntityReference, WorldIdentityDomain};
use nara_ecs::{Component, Entity, World};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistryBuilder, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueCost, ComponentValueKind, ComponentValueMaterializationLimits,
    ComponentValueShape, PersistentComponentSchemaOwnerId, encode_asset_reference_value,
};
use nara_scene::{
    SceneComponentBindings, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SceneOccurrenceKey, SceneSpawnReport, SourceDocumentKey, SourceDocumentKind,
};

const NATIVE: &str = "test.JointNative";
const DATA: &str = "test.JointData";

#[derive(Component)]
struct NativePayload(ComponentValue);

#[derive(Component, Debug, PartialEq)]
struct Sentinel(u32);

#[derive(Default)]
struct CodecCalls {
    decode: AtomicUsize,
    encode: AtomicUsize,
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.joint-references").unwrap()
}

fn asset_id() -> StableAssetId {
    StableAssetId::parse_str("34638a9e-71ce-4768-a515-275ca2d6aadc").unwrap()
}

fn asset_path() -> AssetRef {
    AssetRef::path("textures/joint.png").unwrap()
}

fn database() -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            asset_id(),
            AssetSourcePath::new("textures/joint.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    database
}

fn local(target: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(target) })
}

fn schema(name: &str) -> ComponentSchema {
    ComponentSchema::new(component(name), name, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("assets"),
                "Assets",
                ComponentFieldPath::from_fields(["assets"]),
                ComponentValueShape::list(ComponentValueShape::map(ComponentValueShape::optional(
                    ComponentValueKind::AssetRef.into(),
                ))),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::AssetRef),
            ComponentFieldSchema::required(
                ComponentFieldId::new("targets"),
                "Targets",
                ComponentFieldPath::from_fields(["targets"]),
                ComponentValueShape::list(ComponentValueShape::map(ComponentValueShape::optional(
                    ComponentValueShape::EntityRef,
                ))),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef),
            ComponentFieldSchema::required(
                ComponentFieldId::new("payload"),
                "Payload",
                ComponentFieldPath::from_fields(["payload"]),
                ComponentValueShape::opaque(ComponentValueKind::List),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("default_asset"),
                "Default asset",
                ComponentFieldPath::from_fields(["default_asset"]),
                ComponentValueKind::AssetRef,
                encode_asset_reference_value(&asset_path()),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::AssetRef),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("default_target"),
                "Default target",
                ComponentFieldPath::from_fields(["default_target"]),
                ComponentValueKind::EntityRef,
                local("z-target"),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("enabled"),
                "Enabled",
                ComponentFieldPath::from_fields(["settings", "enabled"]),
                ComponentValueKind::Bool,
                ComponentValue::Bool(true),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ])
}

fn bindings_with_native_schema(
    native: ComponentSchema,
    calls: &Arc<CodecCalls>,
) -> SceneComponentBindings {
    let decode = Arc::clone(calls);
    let encode = Arc::clone(calls);
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_with_codec::<NativePayload, _, _>(
            native,
            move |value| {
                decode.decode.fetch_add(1, Ordering::SeqCst);
                Ok(NativePayload(value.to_owned()))
            },
            move |payload| {
                encode.encode.fetch_add(1, Ordering::SeqCst);
                Ok(payload.0.clone())
            },
        )
        .unwrap();
    let data = ComponentSchemaOwnerSnapshot::admit(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema(DATA)],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    SceneComponentBindings::new(registry.freeze().unwrap(), [data]).unwrap()
}

fn bindings(calls: &Arc<CodecCalls>) -> SceneComponentBindings {
    bindings_with_native_schema(schema(NATIVE), calls)
}

fn value(reference: &AssetRef, target: &str, enabled: Option<bool>) -> ComponentValue {
    let mut value = ComponentValue::map([
        (
            "assets",
            ComponentValue::List(vec![ComponentValue::map([
                ("primary", encode_asset_reference_value(reference)),
                ("optional", ComponentValue::Null),
            ])]),
        ),
        (
            "targets",
            ComponentValue::List(vec![ComponentValue::map([
                ("primary", local(target)),
                ("optional", ComponentValue::Null),
            ])]),
        ),
        (
            "payload",
            ComponentValue::List((0..1024).map(ComponentValue::U64).collect()),
        ),
    ]);
    if let Some(enabled) = enabled {
        value
            .set_path(
                &ComponentFieldPath::from_fields(["settings"]),
                ComponentValue::map([("enabled", ComponentValue::Bool(enabled))]),
            )
            .unwrap();
    }
    value
}

fn record(name: &str, value: ComponentValue) -> SceneComponentRecord {
    let record = SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
        .with_contextual_default(ComponentFieldId::new("default_target"), local("z-target"));
    if name == DATA {
        record.with_owner(owner())
    } else {
        record
    }
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots(
        [
            ("a-source", asset_path(), None),
            ("z-target", AssetRef::StableId(asset_id()), Some(false)),
        ]
        .map(|(entity, reference, enabled)| {
            let value = value(&reference, "z-target", enabled);
            SceneEntityRecord::new(id(entity))
                .with_component(component(NATIVE), record(NATIVE, value.clone()))
                .with_component(component(DATA), record(DATA, value))
        }),
    )
}

fn expected(entity: &str, processed: bool) -> ComponentValue {
    let path = asset_path();
    let reference = if processed {
        AssetRef::Product(AssetProductRef::new(asset_id(), ImportedProductId::PRIMARY).unwrap())
    } else if entity == "a-source" {
        path.clone()
    } else {
        AssetRef::StableId(asset_id())
    };
    let mut value = value(&reference, "z-target", Some(entity == "a-source"));
    for (field, default) in [
        (
            "default_asset",
            encode_asset_reference_value(if processed { &reference } else { &path }),
        ),
        ("default_target", local("z-target")),
    ] {
        value
            .set_path(&ComponentFieldPath::from_fields([field]), default)
            .unwrap();
    }
    value
}

fn source_key() -> SourceDocumentKey {
    SourceDocumentKey::named(
        "96ae2c89-62c2-4ff4-b611-6d12ba4cbf87".parse().unwrap(),
        "joint-effective-references.scene",
        SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("joint-effective-references", "main").unwrap()
}

fn assert_field(diagnostic: &Diagnostic, key: &str, expected: DiagnosticValueRef<'_>) {
    assert!(
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == key && field.value() == expected),
        "{diagnostic:?}",
    );
}

fn diagnostic<'a>(report: &'a DiagnosticReport, code: &str) -> &'a Diagnostic {
    report
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == code)
        .unwrap_or_else(|| panic!("expected {code}: {report:?}"))
}

fn assert_no_local_error(report: &DiagnosticReport) {
    assert!(
        report
            .iter()
            .all(|diagnostic| diagnostic.code().as_str() != "scene.local-reference-invalid"),
        "{report:?}",
    );
}

fn assert_no_scene_publication(world: &World, sentinel: Entity) {
    let entities = world
        .iter_entities()
        .filter(|entity| !entity.contains::<IsResource>())
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    assert_eq!(entities, [sentinel]);
    assert_eq!(world.get::<Sentinel>(sentinel), Some(&Sentinel(7)));
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

fn assert_published(
    world: &World,
    bindings: &SceneComponentBindings,
    report: SceneSpawnReport,
    calls: &CodecCalls,
    processed: bool,
) {
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let instance = report.instance.unwrap();
    assert_eq!(instance.len(), 2);
    for name in ["a-source", "z-target"] {
        let EntityLookup::Resolved(entity) = instance.resolve(world, &id(name)) else {
            panic!("the committed entity must resolve: {name}")
        };
        let expected = expected(name, processed);
        assert_eq!(world.get::<NativePayload>(entity).unwrap().0, expected);
        assert_eq!(
            bindings
                .native_snapshot()
                .encode_component(&component(NATIVE), world, entity)
                .unwrap(),
            Some(expected.clone()),
        );
        let data = bindings
            .data_component(world, entity, &component(DATA))
            .unwrap()
            .unwrap();
        assert_eq!(data.value(), &expected);
        assert_eq!(data.schema_owner(), Some(&owner()));
    }
    assert_eq!(calls.decode.load(Ordering::SeqCst), 2);
    assert_eq!(calls.encode.load(Ordering::SeqCst), 2);
    assert_eq!(
        world
            .iter_entities()
            .filter(|entity| !entity.contains::<IsResource>())
            .count(),
        3
    );
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_scene_instances,
        1,
    );
}

#[test]
fn joint_source_references_preserve_defaults_data_and_every_native_codec_call() {
    let calls = Arc::new(CodecCalls::default());
    let bindings = bindings(&calls);
    let database = database();
    let resolver = database.resolver(&AssetPathScope::ROOT);
    let source = document();
    let original = source.clone();
    let candidate = compiled_scene::mixed(&bindings, &source, source_key(), Some(resolver));
    for entity in candidate.document().entities() {
        for record in entity.components().values() {
            assert_eq!(record.value(), &expected(entity.id().as_str(), false));
        }
    }
    assert_eq!(source, original);
    assert_eq!(calls.decode.load(Ordering::SeqCst), 0);
    let mut world = World::new();
    let sentinel = world.spawn(Sentinel(7)).id();
    let prepared = bindings
        .prepare_scene(&mut world, &candidate, &occurrence(), Some(resolver))
        .unwrap();
    assert_no_scene_publication(&world, sentinel);
    assert_eq!(calls.decode.load(Ordering::SeqCst), 2);
    drop(candidate);
    let report = prepared.commit(&mut world);
    assert_published(&world, &bindings, report, &calls, false);
    assert_eq!(source, original);
}

#[cfg(feature = "serde")]
#[test]
fn processed_joint_references_readmit_exact_definitions_and_retain_first_diagnostics() {
    use nara_scene::{RuntimeSceneCandidate, RuntimeSceneDocumentCandidate, RuntimeSceneLimits};

    let calls = Arc::new(CodecCalls::default());
    let bindings = bindings(&calls);
    let database = database();
    let source = document();
    let original = source.clone();
    let compiled = compiled_scene::mixed(
        &bindings,
        &source,
        source_key(),
        Some(database.resolver(&AssetPathScope::ROOT)),
    );
    let cooked = RuntimeSceneCandidate::cook(
        &compiled,
        &bindings,
        database.resolver(&AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let bytes = cooked.to_json_bytes().unwrap();
    let runtime = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, cooked.limits())
        .unwrap()
        .admit(&source_key(), &bindings)
        .unwrap();
    assert_eq!(runtime.document(), cooked.document());
    assert!(runtime.definitions().matches(&bindings.clone()));
    for entity in runtime.document().entities() {
        for record in entity.components().values() {
            assert_eq!(record.value(), &expected(entity.id().as_str(), true));
        }
    }

    let mut unprocessed = expected("a-source", true);
    unprocessed
        .set_path(
            &ComponentFieldPath::from_fields(["assets"]),
            value(&asset_path(), "missing", None)
                .get("assets")
                .unwrap()
                .clone(),
        )
        .unwrap();
    unprocessed
        .set_path(
            &ComponentFieldPath::from_fields(["targets"]),
            value(&asset_path(), "missing", None)
                .get("targets")
                .unwrap()
                .clone(),
        )
        .unwrap();
    let mut hidden = expected("a-source", true);
    hidden
        .set_path(
            &ComponentFieldPath::from_fields(["payload"]),
            ComponentValue::List(vec![local("missing")]),
        )
        .unwrap();
    let mut malformed = hidden.clone();
    malformed
        .set_path(
            &ComponentFieldPath::from_fields(["default_asset"]),
            ComponentValue::map([
                ("kind", ComponentValue::String("stable_id".into())),
                ("value", ComponentValue::String("invalid-asset-id".into())),
            ]),
        )
        .unwrap();
    let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (value, code, kind) in [
        (
            unprocessed,
            "scene.runtime-unprocessed-asset-reference",
            None,
        ),
        (hidden, "scene.invalid-component-payload", Some("message")),
        (
            malformed,
            "scene.invalid-component-payload",
            Some("invalid-field"),
        ),
    ] {
        let mut changed = wire.clone();
        changed["payload"]["entities"][0]["components"][NATIVE]["value"] =
            serde_json::to_value(value).unwrap();
        let error = RuntimeSceneDocumentCandidate::from_json_bytes(
            &serde_json::to_vec(&changed).unwrap(),
            cooked.limits(),
        )
        .unwrap()
        .admit(&source_key(), &bindings)
        .unwrap_err();
        let diagnostic = diagnostic(error.diagnostics(), code);
        assert_field(
            diagnostic,
            "entity-id",
            DiagnosticValueRef::Identifier("a-source"),
        );
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
        } else {
            assert_field(
                diagnostic,
                "field-id",
                DiagnosticValueRef::Identifier("assets"),
            );
        }
        assert_no_local_error(error.diagnostics());
    }
    let tight = cooked.limits().with_working_bytes(ByteLimit::ONE);
    let error = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, tight).unwrap_err();
    let budget = diagnostic(error.diagnostics(), "scene.runtime-budget");
    assert_field(
        budget,
        "budget-kind",
        DiagnosticValueRef::Identifier("working-bytes"),
    );
    assert_field(budget, "maximum", DiagnosticValueRef::Unsigned(1));
    assert_eq!(calls.decode.load(Ordering::SeqCst), 0);

    let foreign_calls = Arc::new(CodecCalls::default());
    let foreign = self::bindings(&foreign_calls);
    assert!(!runtime.definitions().matches(&foreign));
    let mut world = World::new();
    let sentinel = world.spawn(Sentinel(7)).id();
    let refused = foreign
        .prepare_runtime_scene(&mut world, &runtime, &occurrence())
        .unwrap_err();
    assert!(refused.instance.is_none());
    diagnostic(&refused.diagnostics, "scene.prepared-definition-changed");
    assert_eq!(foreign_calls.decode.load(Ordering::SeqCst), 0);
    assert_no_scene_publication(&world, sentinel);
    let prepared = bindings
        .prepare_runtime_scene(&mut world, &runtime, &occurrence())
        .unwrap();
    assert_no_scene_publication(&world, sentinel);
    assert_eq!(calls.decode.load(Ordering::SeqCst), 2);
    drop(runtime);
    let report = prepared.commit(&mut world);
    assert_published(&world, &bindings, report, &calls, true);
    assert_eq!(source, original);
}

#[test]
fn source_reference_closure_and_receiving_asset_checks_keep_their_original_order() {
    let calls = Arc::new(CodecCalls::default());
    let bindings = bindings(&calls);
    let complete = database();
    let missing = ProjectAssetDatabase::default();
    for name in [NATIVE, DATA] {
        let source = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("a-source")).with_component(
                component(name),
                record(name, value(&asset_path(), "missing", None)),
            ),
            SceneEntityRecord::new(id("z-target")),
        ]);
        let original = source.clone();
        for database in [&missing, &complete] {
            let report = bindings
                .try_compile_scene_with_loader(
                    &source,
                    Default::default(),
                    Some(database.resolver(&AssetPathScope::ROOT)),
                    |_, _| -> Result<_, std::convert::Infallible> {
                        panic!("a source without Prefab declarations must not invoke a loader")
                    },
                )
                .unwrap();
            assert!(report.candidate.is_none(), "{report:?}");
            let diagnostic = diagnostic(
                &report.diagnostics,
                "scene.prefab-entity-reference-rewrite-failed",
            );
            assert_field(
                diagnostic,
                "component-id",
                DiagnosticValueRef::Identifier(name),
            );
            assert_field(
                diagnostic,
                "rewrite-error-kind",
                DiagnosticValueRef::Identifier("missing-source-entity"),
            );
            assert_no_local_error(&report.diagnostics);
            assert_eq!(source, original);
        }
    }
    let source = document();
    let original = source.clone();
    let candidate = compiled_scene::mixed(
        &bindings,
        &source,
        source_key(),
        Some(complete.resolver(&AssetPathScope::ROOT)),
    );
    let mut world = World::new();
    let sentinel = world.spawn(Sentinel(7)).id();
    let refused = bindings
        .prepare_scene(
            &mut world,
            &candidate,
            &occurrence(),
            Some(missing.resolver(&AssetPathScope::ROOT)),
        )
        .unwrap_err();
    assert!(refused.instance.is_none());
    assert_field(
        diagnostic(&refused.diagnostics, "scene.invalid-component-payload"),
        "codec-error-kind",
        DiagnosticValueRef::Identifier("invalid-asset-ref"),
    );
    assert_no_local_error(&refused.diagnostics);
    assert_no_scene_publication(&world, sentinel);
    assert_eq!(calls.decode.load(Ordering::SeqCst), 0);
    assert_eq!(source, original);
}

#[test]
fn current_receiving_definitions_and_aggregate_limits_reject_before_any_codec() {
    let calls = Arc::new(CodecCalls::default());
    let bindings = bindings(&calls);
    let database = database();
    let resolver = database.resolver(&AssetPathScope::ROOT);
    let source = document();
    let original = source.clone();
    let candidate = compiled_scene::mixed(&bindings, &source, source_key(), Some(resolver));
    let mut narrower = schema(NATIVE);
    narrower.fields[2].shape = ComponentValueShape::list(ComponentValueShape::map(
        ComponentValueShape::optional(ComponentValueKind::U64.into()),
    ));
    let mut added_default = schema(NATIVE);
    added_default.fields.push(
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("receiving_default"),
            "Receiving default",
            ComponentFieldPath::from_fields(["receiving_default"]),
            ComponentValueKind::Bool,
            ComponentValue::Bool(true),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    );
    let mut world = World::new();
    let sentinel = world.spawn(Sentinel(7)).id();
    for (schema, code) in [
        (narrower, "scene.invalid-component-payload"),
        (added_default, "scene.compiled-incomplete-value"),
    ] {
        let receiving_calls = Arc::new(CodecCalls::default());
        let receiving = bindings_with_native_schema(schema, &receiving_calls);
        let refused = receiving
            .prepare_scene(&mut world, &candidate, &occurrence(), Some(resolver))
            .unwrap_err();
        assert!(refused.instance.is_none());
        assert_field(
            diagnostic(&refused.diagnostics, code),
            "component-id",
            DiagnosticValueRef::Identifier(NATIVE),
        );
        assert_eq!(receiving_calls.decode.load(Ordering::SeqCst), 0);
        assert_no_scene_publication(&world, sentinel);
    }
    let records = candidate
        .document()
        .entities()
        .iter()
        .flat_map(|entity| entity.components().values());
    let total = records
        .clone()
        .fold(ComponentValueCost::ZERO, |cost, record| {
            cost.saturating_add(record.value_cost())
        });
    let depth = records.map(|record| record.value_depth()).max().unwrap();
    let exact = ComponentValueMaterializationLimits::new(
        ItemLimit::new(total.nodes()).unwrap(),
        ByteLimit::new(total.logical_bytes()).unwrap(),
        DepthLimit::new(depth).unwrap(),
    );
    for (limits, kind) in [
        (
            exact.with_nodes(ItemLimit::new(total.nodes() - 1).unwrap()),
            "materialization-node-limit",
        ),
        (
            exact.with_bytes(ByteLimit::new(total.logical_bytes() - 1).unwrap()),
            "materialization-byte-limit",
        ),
        (
            ComponentValueMaterializationLimits::new(
                ItemLimit::new(total.nodes()).unwrap(),
                ByteLimit::new(total.logical_bytes()).unwrap(),
                DepthLimit::new(depth - 1).unwrap(),
            ),
            "materialization-depth-limit",
        ),
    ] {
        let refused = bindings
            .prepare_scene_with_limits(
                &mut world,
                &candidate,
                &occurrence(),
                Some(resolver),
                limits,
            )
            .unwrap_err();
        assert!(refused.instance.is_none());
        assert_field(
            diagnostic(&refused.diagnostics, "scene.invalid-component-payload"),
            "codec-error-kind",
            DiagnosticValueRef::Identifier(kind),
        );
        assert_eq!(calls.decode.load(Ordering::SeqCst), 0, "{kind}");
        assert_no_scene_publication(&world, sentinel);
    }
    let prepared = bindings
        .prepare_scene_with_limits(&mut world, &candidate, &occurrence(), Some(resolver), exact)
        .unwrap();
    assert_no_scene_publication(&world, sentinel);
    let report = prepared.commit(&mut world);
    assert_published(&world, &bindings, report, &calls, false);
    assert_eq!(source, original);
}
