#![cfg(all(feature = "runtime-core", feature = "serde"))]

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use nara::{
    asset::{
        AssetPathScope, AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath,
        ProjectAssetDatabase,
    },
    core::{ByteLimit, DepthLimit, ItemLimit},
    diagnostic::{DiagnosticCode, DiagnosticReport, DiagnosticValueRef, SafeSummary},
    ecs::{Component, World},
    identity::WorldIdentityDomain,
    reflect::{
        ComponentCapability, ComponentCodecError, ComponentDecodeContext, ComponentDomainError,
        ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentRegistryBuilder,
        ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueRef,
        ComponentValueShape, EntityReference, PersistentComponentSchemaOwnerId,
        PreparedComponentCandidate, encode_asset_reference_value,
    },
    scene::{
        CompiledSceneCandidate, InMemoryPrefabSourceResolver, RuntimeSceneCandidate,
        SceneComponentBindings, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
        SceneEntityId, SceneEntityRecord, SceneOccurrenceKey, SourceDocumentKey,
        SourceDocumentKind,
    },
};

#[derive(Default)]
struct Calls {
    validators: AtomicUsize,
    preflights: AtomicUsize,
    drops: AtomicUsize,
    backing: AtomicUsize,
    observed_drops: Mutex<Vec<usize>>,
}

impl Calls {
    fn counts(&self) -> (usize, usize, usize) {
        (
            self.validators.load(Ordering::SeqCst),
            self.preflights.load(Ordering::SeqCst),
            self.drops.load(Ordering::SeqCst),
        )
    }
}

#[derive(Component)]
struct Probe {
    value: i64,
    calls: Arc<Calls>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.calls.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.DiagnosticCodec")
}

fn schema() -> ComponentSchema {
    ComponentSchema::new(component(), "Diagnostic codec", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("value"),
                "Value",
                ComponentFieldPath::from_fields(["value"]),
                ComponentValueKind::I64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("cells"),
                "Cells",
                ComponentFieldPath::from_fields(["cells"]),
                ComponentValueShape::list(ComponentValueKind::I64.into()),
                ComponentValue::List(Vec::new()),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("enabled"),
                "Enabled",
                ComponentFieldPath::from_fields(["enabled"]),
                ComponentValueKind::Bool,
                ComponentValue::Bool(true),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ])
}

fn payload(value: i64) -> ComponentValue {
    ComponentValue::map([
        ("value", ComponentValue::I64(value)),
        (
            "cells",
            ComponentValue::List(vec![ComponentValue::I64(3); 4]),
        ),
    ])
}

fn document(values: &[(&str, i64)]) -> SceneDocument {
    SceneDocument::from_ordered_roots(values.iter().map(|(name, value)| {
        SceneEntityRecord::new(SceneEntityId::new(*name).unwrap()).with_component(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, payload(*value)),
        )
    }))
}

fn inspect(
    value: ComponentValueRef<'_>,
    context: &mut ComponentDecodeContext<'_>,
    calls: &Calls,
    changed_limits: Option<ComponentValueMaterializationLimits>,
) -> Result<i64, ComponentCodecError> {
    let number = value.field_i64("value")?;
    assert!(value.field_bool("enabled")?);
    let cells = value.field("cells")?.as_value().unwrap();
    calls
        .backing
        .store(std::ptr::from_ref(cells) as usize, Ordering::SeqCst);
    if number == 1
        && let Some(limits) = changed_limits
    {
        context.set_materialization_limits(limits);
    }
    if number < 0 {
        return Err(ComponentDomainError::new(
            DiagnosticCode::new("test.diagnostic-codec.negative").unwrap(),
            SafeSummary::new("Value must be nonnegative").unwrap(),
        )
        .with_field(ComponentFieldId::new("value"))
        .into());
    }
    Ok(number)
}

fn registry(
    schema: ComponentSchema,
    owner: Option<&'static str>,
    changed_limits: Option<ComponentValueMaterializationLimits>,
) -> (ComponentRegistrySnapshot, Arc<Calls>) {
    let mut builder = owner.map_or_else(ComponentRegistryBuilder::new, |owner| {
        ComponentRegistryBuilder::from_owner_catalog_candidate(
            ComponentSchemaOwnerId::new(owner),
            ComponentSchemaCatalog::default(),
            None,
        )
        .unwrap()
    });
    let calls = Arc::new(Calls::default());
    let validation_calls = Arc::clone(&calls);
    let preflight_calls = Arc::clone(&calls);
    builder
        .register_persistent_component_codec_with_context_and_validation::<Probe, _, _, _>(
            schema,
            move |value, context| {
                validation_calls.validators.fetch_add(1, Ordering::SeqCst);
                inspect(value, context, &validation_calls, changed_limits).map(|_| ())
            },
            move |value, context| {
                preflight_calls.preflights.fetch_add(1, Ordering::SeqCst);
                let value = inspect(value, context, &preflight_calls, changed_limits)?;
                Ok(PreparedComponentCandidate::insert(Probe {
                    value,
                    calls: Arc::clone(&preflight_calls),
                }))
            },
            |probe, _, _| Ok(payload(probe.value)),
        )
        .unwrap();
    (builder.freeze().unwrap(), calls)
}

fn source() -> SourceDocumentKey {
    SourceDocumentKey::named(
        "91c3fc5d-bb63-4638-8620-404ed4c2d302".parse().unwrap(),
        "diagnostic-codecs.scene",
        SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("diagnostic-codecs", "main").unwrap()
}

fn compile(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
) -> CompiledSceneCandidate {
    let report = document.compile(registry, &InMemoryPrefabSourceResolver::new());
    assert_clean(&report.diagnostics);
    report.candidate.unwrap().bind_source(source())
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn assert_clean(report: &DiagnosticReport) {
    assert!(!report.has_errors(), "{report:?}");
}

fn has_field(report: &DiagnosticReport, key: &str, value: &str) -> bool {
    report.iter().any(|diagnostic| {
        diagnostic.fields().iter().any(|field| {
            field.key().as_str() == key && field.value() == DiagnosticValueRef::Identifier(value)
        })
    })
}

fn assert_receiving_refused(
    bindings: &SceneComponentBindings,
    candidate: &CompiledSceneCandidate,
    limits: ComponentValueMaterializationLimits,
) {
    let report = bindings.validate_compiled_scene(candidate.document(), None, limits);
    assert!(report.has_errors(), "{report:?}");
    let mut world = World::new();
    let population = world.entities().len();
    let rejected = bindings
        .prepare_scene_with_limits(&mut world, candidate, &occurrence(), None, limits)
        .expect_err("complete receiving admission must precede every callback");
    assert_eq!(
        report.iter().collect::<Vec<_>>(),
        rejected.diagnostics.iter().collect::<Vec<_>>(),
    );
    assert_eq!(world.entities().len(), population);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn scene_diagnostics_use_explicit_validation_and_preparation_keeps_candidate_custody() {
    let (registry, calls) = registry(schema(), None, None);
    let bindings = SceneComponentBindings::new(registry.clone(), []).unwrap();
    let document = document(&[("a", 7)]);
    let original = document.clone();
    let raw = document.entities[0].components[&component()].value();
    let backing = std::ptr::from_ref(raw.get("cells").unwrap()) as usize;
    let compiled = compile(&document, &registry);
    let frozen = &compiled.document().entities()[0].components()[&component()];
    assert!(frozen.value().as_value().is_none());
    assert!(frozen.value().field_bool("enabled").unwrap());
    assert_eq!(calls.counts(), (0, 0, 0));

    let database = ProjectAssetDatabase::default();
    let resolver = database.resolver(&AssetPathScope::ROOT);
    assert_clean(&document.validate(&registry));
    assert_clean(&document.validate_with_asset_resolution(&registry, resolver));
    assert_clean(&bindings.validate_scene(&document, None, Default::default()));
    assert_clean(&bindings.validate_compiled_scene(compiled.document(), None, Default::default()));
    assert_eq!(calls.counts(), (4, 0, 0));
    assert_eq!(calls.backing.load(Ordering::SeqCst), backing);

    let encoded = document.to_json_string().unwrap();
    let published = SceneDocumentCandidate::decode_json_str(&encoded)
        .unwrap()
        .publish(&registry)
        .unwrap();
    assert_eq!(published.document(), &original);
    assert_eq!(calls.counts(), (5, 0, 0));

    let mut world = World::new();
    let population = world.entities().len();
    let discarded = bindings
        .prepare_scene(&mut world, &compiled, &occurrence(), None)
        .unwrap();
    assert_eq!(calls.counts(), (5, 1, 0));
    assert_eq!(world.entities().len(), population);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    drop(discarded);
    assert_eq!(calls.counts(), (5, 1, 1));
    let prepared = bindings
        .prepare_scene(&mut world, &compiled, &occurrence(), None)
        .unwrap();
    assert_eq!(calls.counts(), (5, 2, 1));
    assert_clean(&prepared.commit(&mut world).diagnostics);
    assert_eq!(calls.counts(), (5, 2, 1));
    assert_eq!(world.query::<&Probe>().single(&world).unwrap().value, 7);
    assert_eq!(document, original);
    assert!(raw.get("enabled").is_none());
    assert!(std::ptr::eq(
        frozen.value().field("cells").unwrap().as_value().unwrap(),
        raw.get("cells").unwrap(),
    ));
    drop(world);
    assert_eq!(calls.counts(), (5, 2, 2));
}

#[test]
fn complete_receiving_admission_preempts_all_diagnostic_and_preparation_callbacks() {
    let mut permissive = schema();
    permissive.fields[0].shape = ComponentValueShape::optional(ComponentValueKind::I64.into());
    let (source_registry, source_calls) = registry(permissive, None, None);
    let valid = document(&[("a", 7), ("zz", 8)]);
    let compiled = compile(&valid, &source_registry);
    let mut malformed = valid.clone();
    malformed.entities[1]
        .components
        .get_mut(&component())
        .unwrap()
        .set_value(ComponentValue::map([("value", ComponentValue::Null)]));
    let malformed = compile(&malformed, &source_registry);
    let (receiving, calls) = registry(schema(), None, None);
    let bindings = SceneComponentBindings::new(receiving, []).unwrap();
    assert_receiving_refused(&bindings, &malformed, Default::default());

    let mut extra_default = schema();
    extra_default.fields.push(
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("added"),
            "Added",
            ComponentFieldPath::from_fields(["added"]),
            ComponentValueKind::Bool,
            ComponentValue::Bool(true),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    );
    let mut future_version = schema();
    future_version.version = ComponentSchemaVersion::new(2).unwrap();
    let mut missing_capability = schema();
    missing_capability.fields[0]
        .capabilities
        .remove(&ComponentCapability::Scene);
    for receiving_schema in [extra_default, future_version, missing_capability] {
        let (receiving, calls) = registry(receiving_schema, None, None);
        let bindings = SceneComponentBindings::new(receiving, []).unwrap();
        assert_receiving_refused(&bindings, &compiled, Default::default());
        assert_eq!(calls.counts(), (0, 0, 0));
    }
    let (owned, owned_calls) = registry(schema(), Some("test.diagnostic-receiver"), None);
    let owned = SceneComponentBindings::new(owned, []).unwrap();
    assert_receiving_refused(&owned, &compiled, Default::default());
    assert_eq!(owned_calls.counts(), (0, 0, 0));

    let records = compiled
        .document()
        .entities()
        .iter()
        .flat_map(|entity| entity.components().values());
    let (nodes, bytes) = records.fold((0, 0), |(nodes, bytes), record| {
        (
            nodes + record.value_cost().nodes(),
            bytes + record.value_cost().logical_bytes(),
        )
    });
    for limits in [
        limits(nodes - 1, bytes, 64),
        limits(nodes, bytes - 1, 64),
        limits(nodes, bytes, 2),
    ] {
        assert_receiving_refused(&bindings, &compiled, limits);
    }
    assert_eq!(calls.counts(), (0, 0, 0));
    assert_eq!(source_calls.counts(), (0, 0, 0));
}

#[test]
fn malformed_values_and_domain_errors_preserve_public_diagnostics() {
    let (registry, calls) = registry(schema(), None, None);
    let bindings = SceneComponentBindings::new(registry.clone(), []).unwrap();
    let mut malformed = document(&[("a", 7)]);
    malformed.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .set_value(ComponentValue::map([(
            "value",
            ComponentValue::Bool(false),
        )]));
    let report = malformed.validate(&registry);
    assert!(report.has_errors());
    assert_eq!(report.iter().count(), 1);
    assert_eq!(
        report.iter().next().unwrap().code().as_str(),
        "scene.prefab-compilation-failed",
    );
    assert!(has_field(&report, "failure-kind", "component"));
    assert_eq!(calls.counts(), (0, 0, 0));

    let invalid_domain = document(&[("a", -3)]);
    let compiled = compile(&invalid_domain, &registry);
    let report = bindings.validate_compiled_scene(compiled.document(), None, Default::default());
    let diagnostic = report.iter().next().unwrap();
    assert_eq!(diagnostic.code().as_str(), "test.diagnostic-codec.negative");
    assert_eq!(diagnostic.summary().as_str(), "Value must be nonnegative");
    assert!(has_field(&report, "component-field-id", "value"));
    assert!(has_field(&report, "entity-id", "a"));
    assert!(has_field(&report, "component-id", component().as_str()));
    assert_eq!(calls.counts(), (1, 0, 0));
    let mut world = World::new();
    let rejected = bindings
        .prepare_scene(&mut world, &compiled, &occurrence(), None)
        .unwrap_err();
    assert_eq!(
        report.iter().collect::<Vec<_>>(),
        rejected.diagnostics.iter().collect::<Vec<_>>(),
    );
    assert_eq!(calls.counts(), (1, 1, 0));
}

#[test]
fn legacy_fallback_keeps_earlier_candidates_alive_until_complete_scene_diagnostics_return() {
    let calls = Arc::new(Calls::default());
    let preflight_calls = Arc::clone(&calls);
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_codec_with_context::<Probe, _, _>(
            schema(),
            move |value, context| {
                preflight_calls.preflights.fetch_add(1, Ordering::SeqCst);
                preflight_calls
                    .observed_drops
                    .lock()
                    .unwrap()
                    .push(preflight_calls.drops.load(Ordering::SeqCst));
                let value = inspect(value, context, &preflight_calls, None)?;
                if value == 7 {
                    context.set_materialization_limits(limits(1024, 1024, 64));
                }
                Ok(PreparedComponentCandidate::insert(Probe {
                    value,
                    calls: Arc::clone(&preflight_calls),
                }))
            },
            |probe, _, _| Ok(payload(probe.value)),
        )
        .unwrap();
    let registry = builder.freeze().unwrap();
    let bindings = SceneComponentBindings::new(registry.clone(), []).unwrap();
    let document = document(&[("a", 7), ("m", 8), ("zz", 9)]);
    let compiled = compile(&document, &registry);
    let database = ProjectAssetDatabase::default();
    for index in 0..4 {
        let report = match index {
            0 => document.validate(&registry),
            1 => document.validate_with_asset_resolution(
                &registry,
                database.resolver(&AssetPathScope::ROOT),
            ),
            2 => bindings.validate_scene(&document, None, Default::default()),
            _ => bindings.validate_compiled_scene(compiled.document(), None, Default::default()),
        };
        assert_clean(&report);
        assert_eq!(calls.counts(), (0, (index + 1) * 3, (index + 1) * 3));
        assert_eq!(
            &calls.observed_drops.lock().unwrap()[index * 3..],
            &[index * 3; 3],
        );
    }
    SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .publish(&registry)
        .unwrap();
    assert_eq!(&calls.observed_drops.lock().unwrap()[12..], &[12, 13, 14]);
    assert_eq!(calls.counts(), (0, 15, 15));
    let mut world = World::new();
    let prepared = bindings
        .prepare_scene(&mut world, &compiled, &occurrence(), None)
        .unwrap();
    assert_eq!(&calls.observed_drops.lock().unwrap()[15..], &[15, 15, 15]);
    assert_eq!(calls.counts(), (0, 18, 15));
    assert_clean(&prepared.commit(&mut world).diagnostics);
    assert_eq!(calls.counts(), (0, 18, 15));
    drop(world);
    assert_eq!(calls.counts(), (0, 18, 18));
}

#[test]
fn context_limit_changes_force_complete_readmission_before_later_validators() {
    for (limits, kind) in [
        (limits(1, 4096, 64), "materialization-node-limit"),
        (limits(4096, 1, 64), "materialization-byte-limit"),
        (limits(4096, 4096, 1), "materialization-depth-limit"),
    ] {
        let (registry, calls) = registry(schema(), None, Some(limits));
        let bindings = SceneComponentBindings::new(registry.clone(), []).unwrap();
        let document = document(&[("a", 1), ("zz", 2)]);
        let original = document.clone();
        let compiled = compile(&document, &registry);
        let report =
            bindings.validate_compiled_scene(compiled.document(), None, Default::default());
        assert!(report.has_errors());
        assert!(has_field(&report, "codec-error-kind", kind));
        assert!(has_field(&report, "entity-id", "zz"));
        assert_eq!(calls.counts(), (1, 0, 0));
        let mut world = World::new();
        let rejected = bindings
            .prepare_scene(&mut world, &compiled, &occurrence(), None)
            .unwrap_err();
        assert_eq!(
            report.iter().collect::<Vec<_>>(),
            rejected.diagnostics.iter().collect::<Vec<_>>(),
        );
        assert_eq!(calls.counts(), (1, 1, 1));
        assert_eq!(document, original);
    }
}

#[test]
fn mixed_data_receiving_checks_precede_callbacks_and_cooked_preparation_uses_original_codec() {
    let (registry, calls) = registry(schema(), None, None);
    let data_component = ComponentTypeId::new("test.DiagnosticData");
    let owner = PersistentComponentSchemaOwnerId::try_new("test.diagnostic-data").unwrap();
    let data_schema =
        ComponentSchema::new(data_component.clone(), "Data", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([
                ComponentFieldSchema::required(
                    ComponentFieldId::new("asset"),
                    "Asset",
                    ComponentFieldPath::from_fields(["asset"]),
                    ComponentValueKind::AssetRef,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_capability(ComponentCapability::AssetRef),
                ComponentFieldSchema::optional(
                    ComponentFieldId::new("target"),
                    "Target",
                    ComponentFieldPath::from_fields(["target"]),
                    ComponentValueKind::EntityRef,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_capability(ComponentCapability::EntityRef),
            ]);
    let data_owner = ComponentSchemaOwnerSnapshot::admit(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![data_schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let bindings = SceneComponentBindings::new(registry.clone(), [data_owner]).unwrap();
    let mut document = document(&[("a", 7)]);
    let asset = encode_asset_reference_value(&AssetRef::path("images/shared.png").unwrap());
    document.entities.push(
        SceneEntityRecord::new(SceneEntityId::new("zz").unwrap()).with_component(
            data_component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("asset", asset.clone())]),
            )
            .with_owner(owner),
        ),
    );
    document = SceneDocument::from_ordered_roots(document.entities);
    let original = document.clone();
    let report = bindings.compile_scene(&document, &InMemoryPrefabSourceResolver::new());
    assert_clean(&report.diagnostics);
    let compiled = report.candidate.unwrap().bind_source(source());
    let mut database = ProjectAssetDatabase::default();
    assert!(
        bindings
            .validate_scene(
                &document,
                Some(database.resolver(&AssetPathScope::ROOT)),
                Default::default(),
            )
            .has_errors()
    );
    assert!(
        bindings
            .validate_compiled_scene(
                compiled.document(),
                Some(database.resolver(&AssetPathScope::ROOT)),
                Default::default(),
            )
            .has_errors()
    );
    assert_eq!(calls.counts(), (0, 0, 0));

    let mut invalid_reference = document.clone();
    invalid_reference.entities[1]
        .components
        .get_mut(&data_component)
        .unwrap()
        .set_value(ComponentValue::map([
            ("asset", asset),
            (
                "target",
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: SceneEntityId::new("missing").unwrap(),
                }),
            ),
        ]));
    assert!(
        bindings
            .validate_scene(&invalid_reference, None, Default::default())
            .has_errors()
    );
    assert_eq!(calls.counts(), (0, 0, 0));
    database
        .insert(AssetRecord::new(
            "00000000-0000-4000-8000-000000000041".parse().unwrap(),
            AssetSourcePath::new("images/shared.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    let runtime = RuntimeSceneCandidate::cook(
        &compiled,
        &bindings,
        database.resolver(&AssetPathScope::ROOT),
        Default::default(),
    )
    .unwrap();
    let frozen = &compiled.document().entities()[0].components()[&component()];
    let compiled_backing = frozen.value().field("cells").unwrap().as_value().unwrap();
    let runtime_bytes = runtime.to_json_bytes().unwrap();
    assert_eq!(calls.counts(), (0, 0, 0));
    assert_clean(&bindings.validate_compiled_scene(runtime.document(), None, Default::default()));
    assert_eq!(calls.counts(), (1, 0, 0));
    let mut world = World::new();
    let prepared = bindings
        .prepare_runtime_scene(&mut world, &runtime, &occurrence())
        .unwrap();
    assert_eq!(calls.counts(), (1, 1, 0));
    assert_clean(&prepared.commit(&mut world).diagnostics);
    assert_eq!(calls.counts(), (1, 1, 0));
    assert_eq!(runtime.to_json_bytes().unwrap(), runtime_bytes);
    assert_eq!(document, original);
    assert!(std::ptr::eq(
        frozen.value().field("cells").unwrap().as_value().unwrap(),
        compiled_backing,
    ));
}
