use std::sync::{Arc, Mutex};

use nara_core::ItemLimit;
use nara_core::diagnostic::{DiagnosticCode, DiagnosticValueRef, SafeSummary};
use nara_ecs::identity::{EntityLookup, WorldIdentityDomain};
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentDecodeContext, ComponentDomainError,
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentRegistryBuilder,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueMaterializationLimits, ComponentValueRef, ComponentValueShape,
    PreparedComponentCandidate,
};
use nara_scene::{
    CompiledSceneCandidate, InMemoryPrefabSourceResolver, SceneComponentBindings,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, SceneOccurrenceKey,
    SceneSpawnReport, SourceDocumentKey, SourceDocumentKind,
};

#[derive(Component)]
struct NativePayload(ComponentValue);

#[derive(Debug)]
struct DecodeObservation {
    value: ComponentValue,
    payload_address: usize,
    limits: ComponentValueMaterializationLimits,
}

type Decodes = Arc<Mutex<Vec<DecodeObservation>>>;

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.NativeDecodeAdmission")
}

fn root() -> SceneEntityId {
    SceneEntityId::new("root").unwrap()
}

fn bindings(decodes: &Decodes) -> SceneComponentBindings {
    let schema = ComponentSchema::new(component(), "Native payload", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("label"),
                "Label",
                ComponentFieldPath::from_fields(["label"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::required(
                ComponentFieldId::new("payload"),
                "Payload",
                ComponentFieldPath::from_fields(["payload"]),
                ComponentValueShape::opaque(ComponentValueKind::List),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("enabled"),
                "Enabled",
                ComponentFieldPath::from_fields(["settings", "enabled"]),
                ComponentValueKind::Bool,
                ComponentValue::Bool(true),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ]);
    let decodes = Arc::clone(decodes);
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_codec_with_context::<NativePayload, _, _>(
            schema,
            move |value, context| {
                let payload_address = payload_address(value);
                let decoded = value.to_owned();
                if value.field_str("label")? == "tighten" {
                    context.set_materialization_limits(
                        context
                            .materialization_limits()
                            .with_nodes(ItemLimit::new(decoded.cost().nodes()).unwrap()),
                    );
                }
                decodes.lock().unwrap().push(DecodeObservation {
                    value: decoded.clone(),
                    payload_address,
                    limits: context.materialization_limits(),
                });
                if value.field_str("label")? == "blocked" {
                    return Err(ComponentDomainError::new(
                        DiagnosticCode::new("test.native-admission.blocked").unwrap(),
                        SafeSummary::new("Native payload is unavailable").unwrap(),
                    )
                    .with_field(ComponentFieldId::new("label"))
                    .into());
                }
                Ok(PreparedComponentCandidate::insert(NativePayload(decoded)))
            },
            |payload, _, _| Ok(payload.0.clone()),
        )
        .unwrap();
    SceneComponentBindings::new(registry.freeze().unwrap(), []).unwrap()
}

fn value(label: &str, enabled: Option<bool>) -> ComponentValue {
    let mut value = ComponentValue::map([
        ("label", ComponentValue::String(label.into())),
        (
            "payload",
            ComponentValue::List(vec![ComponentValue::map([
                (
                    "text",
                    ComponentValue::String("retained opaque data".into()),
                ),
                (
                    "cells",
                    ComponentValue::List(vec![
                        ComponentValue::I64(-7),
                        ComponentValue::U64(12),
                        ComponentValue::Bool(false),
                    ]),
                ),
            ])]),
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

fn document(value: ComponentValue) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(root()).with_component(
        component(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
    )])
}

fn source_key() -> SourceDocumentKey {
    SourceDocumentKey::named(
        "e7ac4195-342e-4f93-a5ec-f1d05e680c12".parse().unwrap(),
        "native-decode-admission.scene",
        SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("native-decode-admission", "main").unwrap()
}

fn compile(source: &SceneDocument, bindings: &SceneComponentBindings) -> CompiledSceneCandidate {
    let report = bindings.compile_scene(source, &InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.candidate.unwrap().bind_source(source_key())
}

fn payload_address(value: ComponentValueRef<'_>) -> usize {
    std::ptr::from_ref(value.field("payload").unwrap().as_value().unwrap()).addr()
}

#[cfg(feature = "serde")]
fn readmit(
    compiled: &CompiledSceneCandidate,
    source: &SceneComponentBindings,
    receiving: &SceneComponentBindings,
) -> nara_scene::RuntimeSceneCandidate {
    use nara_asset::{AssetPathScope, ProjectAssetDatabase};
    use nara_scene::{RuntimeSceneCandidate, RuntimeSceneDocumentCandidate, RuntimeSceneLimits};

    let database = ProjectAssetDatabase::default();
    let cooked = RuntimeSceneCandidate::cook(
        compiled,
        source,
        database.resolver(&AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let bytes = cooked.to_json_bytes().unwrap();
    let readmitted = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, cooked.limits())
        .unwrap()
        .admit(&source_key(), receiving)
        .unwrap();
    assert_eq!(readmitted.document(), cooked.document());
    readmitted
}

fn assert_published(
    world: &World,
    bindings: &SceneComponentBindings,
    report: SceneSpawnReport,
    id: &SceneEntityId,
    expected: &ComponentValue,
) {
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let instance = report.instance.unwrap();
    assert_eq!(instance.len(), 1);
    let EntityLookup::Resolved(entity) = instance.resolve(world, id) else {
        panic!("the committed native entity must resolve");
    };
    assert_eq!(&world.get::<NativePayload>(entity).unwrap().0, expected);
    assert_eq!(
        bindings
            .native_snapshot()
            .encode_component(&component(), world, entity)
            .unwrap(),
        Some(expected.clone()),
    );
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_scene_instances,
        1,
    );
}

fn assert_domain_refusal(world: &World, report: SceneSpawnReport) {
    assert!(report.instance.is_none(), "{report:?}");
    assert!(report.diagnostics.has_errors(), "{report:?}");
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "test.native-admission.blocked")
        .unwrap_or_else(|| panic!("the codec's domain error must propagate: {report:?}"));
    for (key, value) in [
        ("codec-error-kind", "domain"),
        ("component-id", "test.NativeDecodeAdmission"),
        ("component-field-id", "label"),
        ("scene-context-code", "scene.invalid-component-payload"),
    ] {
        assert!(
            diagnostic.fields().iter().any(|field| {
                field.key().as_str() == key
                    && field.value() == DiagnosticValueRef::Identifier(value)
            }),
            "{diagnostic:?}",
        );
    }
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}

#[test]
fn source_and_independently_readmitted_native_values_survive_prepare_and_commit() {
    for enabled in [None, Some(false)] {
        let decodes = Decodes::default();
        let bindings = bindings(&decodes);
        let source = document(value("ready", enabled));
        let original = source.clone();
        let address = payload_address(source.entities[0].components[&component()].value().into());
        let expected = value("ready", Some(enabled.unwrap_or(true)));
        let candidate = compile(&source, &bindings);
        let selected = candidate.document().entities()[0].components()[&component()].value();
        assert_eq!(selected.as_value().is_some(), enabled.is_some());
        assert_eq!(selected, &expected);
        assert_eq!(payload_address(selected), address);
        assert_eq!(source, original);

        #[cfg(feature = "serde")]
        let (receiving, runtime_decodes, runtime) = {
            let runtime_decodes = Decodes::default();
            let receiving = self::bindings(&runtime_decodes);
            let runtime = readmit(&candidate, &bindings, &receiving);
            let selected = runtime.document().entities()[0].components()[&component()].value();
            assert!(selected.as_value().is_some());
            assert_eq!(selected, &expected);
            assert_ne!(payload_address(selected), address);
            assert!(runtime_decodes.lock().unwrap().is_empty());
            (receiving, runtime_decodes, runtime)
        };

        assert!(decodes.lock().unwrap().is_empty());
        drop(source);
        drop(original);
        let mut world = World::new();
        let population = world.entities().len();
        let prepared = bindings
            .prepare_scene(&mut world, &candidate, &occurrence(), None)
            .unwrap();
        assert_eq!(world.entities().len(), population);
        assert!(!world.contains_resource::<WorldIdentityDomain>());
        {
            let decodes = decodes.lock().unwrap();
            assert_eq!(decodes.len(), 1);
            assert_eq!(decodes[0].value, expected);
            assert_eq!(decodes[0].payload_address, address);
            assert_eq!(
                decodes[0].limits,
                ComponentValueMaterializationLimits::default(),
            );
        }
        drop(candidate);
        let report = prepared.commit(&mut world);
        assert_published(&world, &bindings, report, &root(), &expected);
        assert_eq!(decodes.lock().unwrap().len(), 1);
        drop(bindings);

        #[cfg(feature = "serde")]
        {
            let runtime_root = runtime.document().roots()[0].clone();
            let address = payload_address(
                runtime.document().entities()[0].components()[&component()].value(),
            );
            let limits = runtime.limits().value_limits();
            let mut world = World::new();
            let population = world.entities().len();
            let prepared = receiving
                .prepare_runtime_scene(&mut world, &runtime, &occurrence())
                .unwrap();
            assert_eq!(world.entities().len(), population);
            assert!(!world.contains_resource::<WorldIdentityDomain>());
            drop(runtime);
            let report = prepared.commit(&mut world);
            assert_published(&world, &receiving, report, &runtime_root, &expected);
            let decodes = runtime_decodes.lock().unwrap();
            assert_eq!(decodes.len(), 1);
            assert_eq!(decodes[0].value, expected);
            assert_eq!(decodes[0].payload_address, address);
            assert_eq!(decodes[0].limits, limits);
        }
    }
}

#[test]
fn codec_domain_refusal_after_schema_admission_publishes_nothing_and_allows_retry() {
    let decodes = Decodes::default();
    let bindings = bindings(&decodes);
    let blocked = compile(&document(value("blocked", None)), &bindings);
    let valid = compile(&document(value("ready", None)), &bindings);
    assert!(decodes.lock().unwrap().is_empty());
    let mut world = World::new();
    let population = world.entities().len();
    let report = bindings
        .prepare_scene(&mut world, &blocked, &occurrence(), None)
        .unwrap_err();
    assert_domain_refusal(&world, report);
    assert_eq!(world.entities().len(), population);
    assert_eq!(
        decodes.lock().unwrap()[0].value,
        value("blocked", Some(true))
    );
    let population = world.entities().len();
    let prepared = bindings
        .prepare_scene(&mut world, &valid, &occurrence(), None)
        .unwrap();
    assert_eq!(world.entities().len(), population);
    let report = prepared.commit(&mut world);
    assert_published(
        &world,
        &bindings,
        report,
        &root(),
        &value("ready", Some(true)),
    );
    assert_eq!(decodes.lock().unwrap().len(), 2);

    #[cfg(feature = "serde")]
    {
        let runtime_decodes = Decodes::default();
        let receiving = self::bindings(&runtime_decodes);
        let blocked = readmit(&blocked, &bindings, &receiving);
        let valid = readmit(&valid, &bindings, &receiving);
        assert!(runtime_decodes.lock().unwrap().is_empty());
        let mut world = World::new();
        let population = world.entities().len();
        let report = receiving
            .prepare_runtime_scene(&mut world, &blocked, &occurrence())
            .unwrap_err();
        assert_domain_refusal(&world, report);
        assert_eq!(world.entities().len(), population);
        assert_eq!(
            runtime_decodes.lock().unwrap()[0].value,
            value("blocked", Some(true)),
        );
        let population = world.entities().len();
        let prepared = receiving
            .prepare_runtime_scene(&mut world, &valid, &occurrence())
            .unwrap();
        assert_eq!(world.entities().len(), population);
        let report = prepared.commit(&mut world);
        assert_published(
            &world,
            &receiving,
            report,
            &valid.document().roots()[0],
            &value("ready", Some(true)),
        );
        assert_eq!(runtime_decodes.lock().unwrap().len(), 2);
    }
}

#[test]
fn earlier_codec_limit_changes_reject_a_later_payload_before_its_codec_runs() {
    let decodes = Decodes::default();
    let bindings = bindings(&decodes);
    let component_id = component();
    let policy_id = SceneEntityId::new("a-policy").unwrap();
    let payload_id = SceneEntityId::new("z-payload").unwrap();
    let mut policy = value("tighten", Some(true));
    policy
        .set_path(
            &ComponentFieldPath::from_fields(["payload"]),
            ComponentValue::List(Vec::new()),
        )
        .unwrap();
    let tightened = ComponentValueMaterializationLimits::default()
        .with_nodes(ItemLimit::new(policy.cost().nodes()).unwrap());
    let source = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(payload_id.clone()).with_component(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value("ready", None)),
        ),
        SceneEntityRecord::new(policy_id.clone()).with_component(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, policy.clone()),
        ),
    ]);
    let candidate = compile(&source, &bindings);
    assert_eq!(candidate.document().entities()[0].id(), &policy_id);
    let later = &candidate.document().entities()[1];
    assert_eq!(later.id(), &payload_id);
    let later_value = later.components()[&component()].value();
    assert!(later_value.as_value().is_none());
    assert!(later_value.cost().nodes() > tightened.nodes());
    let mut context = ComponentDecodeContext::new().with_materialization_limits(tightened);
    let error = bindings
        .native_snapshot()
        .preflight_component_with_context(&component(), later_value, &mut context)
        .err()
        .expect("public preflight must reject the tightened limits");
    let ComponentCodecError::MaterializationNodeLimit { observed, maximum } = error else {
        panic!("the public preflight must reject the tightened node allowance: {error:?}");
    };
    assert!(decodes.lock().unwrap().is_empty());

    let mut world = World::new();
    let population = world.entities().len();
    let report = bindings
        .prepare_scene(&mut world, &candidate, &occurrence(), None)
        .unwrap_err();
    assert!(report.instance.is_none(), "{report:?}");
    assert!(report.diagnostics.has_errors(), "{report:?}");
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.invalid-component-payload")
        .unwrap_or_else(|| panic!("the original limit diagnostic must propagate: {report:?}"));
    for (key, expected) in [
        (
            "codec-error-kind",
            DiagnosticValueRef::Identifier("materialization-node-limit"),
        ),
        (
            "entity-id",
            DiagnosticValueRef::Identifier(payload_id.as_str()),
        ),
        (
            "component-id",
            DiagnosticValueRef::Identifier(component_id.as_str()),
        ),
        ("observed", DiagnosticValueRef::Unsigned(observed as u64)),
        ("maximum", DiagnosticValueRef::Unsigned(maximum as u64)),
    ] {
        assert!(
            diagnostic
                .fields()
                .iter()
                .any(|field| field.key().as_str() == key && field.value() == expected),
            "{diagnostic:?}",
        );
    }
    let decodes = decodes.lock().unwrap();
    assert_eq!(decodes.len(), 1);
    assert_eq!(decodes[0].value, policy);
    assert_eq!(decodes[0].limits, tightened);
    assert_eq!(world.entities().len(), population);
    let mut native = world.query::<&NativePayload>();
    assert!(native.iter(&world).next().is_none());
    assert!(!world.contains_resource::<WorldIdentityDomain>());
}
