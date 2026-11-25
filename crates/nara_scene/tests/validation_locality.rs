use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_core::diagnostic::DiagnosticValueRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::identity::{EntityLookup, WorldIdentityDomain};
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentNumericDomain, ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueMaterializationLimits, ComponentValueShape,
};
use nara_scene::{
    CompiledSceneCandidate, InMemoryPrefabSourceResolver, SceneCompilationFailureKind,
    SceneComponentBindings, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind, prepare_scene,
};

#[derive(Component)]
struct Payload(ComponentValue);

#[derive(Default)]
struct DecodeProbe {
    calls: AtomicUsize,
    cells_address: AtomicUsize,
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ValidationLocality")
}

fn root() -> SceneEntityId {
    SceneEntityId::new("terrain").unwrap()
}

fn registry_builder(
    version: ComponentSchemaVersion,
    probe: &Arc<DecodeProbe>,
) -> ComponentRegistryBuilder {
    let schema = ComponentSchema::new(component(), "Validation locality", version)
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
                ComponentFieldId::new("layers"),
                "Layers",
                ComponentFieldPath::from_fields(["layers"]),
                ComponentValueKind::U64,
            )
            .with_numeric_domain(Some(ComponentNumericDomain::U8))
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::required(
                ComponentFieldId::new("cells"),
                "Cells",
                ComponentFieldPath::from_fields(["cells"]),
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
        ]);
    let probe = Arc::clone(probe);
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_with_codec::<Payload, _, _>(
            schema,
            move |value| {
                probe.calls.fetch_add(1, Ordering::SeqCst);
                if let Some(cells) = value.get("cells").and_then(|cells| cells.as_value()) {
                    probe
                        .cells_address
                        .store(std::ptr::from_ref(cells).addr(), Ordering::SeqCst);
                }
                Ok(Payload(value.to_owned()))
            },
            |payload| Ok(payload.0.clone()),
        )
        .unwrap();
    builder
}

fn source_value() -> ComponentValue {
    ComponentValue::map([
        ("label", ComponentValue::String("Terrain".into())),
        ("layers", ComponentValue::U64(3)),
        (
            "cells",
            ComponentValue::List(vec![
                ComponentValue::map([
                    (
                        "position",
                        ComponentValue::List(vec![ComponentValue::I64(2), ComponentValue::I64(-1)]),
                    ),
                    ("tile", ComponentValue::U64(7)),
                ]),
                ComponentValue::map([
                    (
                        "position",
                        ComponentValue::List(vec![ComponentValue::I64(3), ComponentValue::I64(-1)]),
                    ),
                    ("paint", ComponentValue::String("moss".into())),
                ]),
            ]),
        ),
    ])
}

fn effective_value() -> ComponentValue {
    let ComponentValue::Map(mut fields) = source_value() else {
        unreachable!()
    };
    fields.insert(
        "settings".into(),
        ComponentValue::map([("visible", ComponentValue::Bool(true))]),
    );
    ComponentValue::Map(fields)
}

fn document(value: ComponentValue) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(root()).with_component(
        component(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
    )])
}

fn compile(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
) -> CompiledSceneCandidate {
    let report = document.compile(registry, &InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.candidate.unwrap().bind_source(
        SourceDocumentKey::named(
            "696f7a3a-2879-47bb-9c33-d1c5145d2060".parse().unwrap(),
            "validation-locality.scene",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
    )
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("validation-locality", "terrain").unwrap()
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

#[test]
fn frozen_defaults_retain_opaque_source_data_through_native_preparation() {
    let probe = Arc::new(DecodeProbe::default());
    let registry = registry_builder(ComponentSchemaVersion::ONE, &probe)
        .freeze()
        .unwrap();
    let source = document(source_value());
    let before = source.clone();
    let cells = source.entities[0].components[&component()]
        .value()
        .get("cells")
        .unwrap();
    let cells_address = std::ptr::from_ref(cells).addr();
    let candidate = compile(&source, &registry);
    let value = candidate.document().entities()[0].components()[&component()].value();
    assert!(value.as_value().is_none());
    assert_eq!(value, &effective_value());
    assert!(std::ptr::eq(
        value.get("cells").unwrap().as_value().unwrap(),
        cells,
    ));
    assert_eq!(source, before);
    assert!(
        source.entities[0].components[&component()]
            .value()
            .get("settings")
            .is_none()
    );
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    drop(source);
    drop(before);

    let mut world = World::new();
    let population = world.entities().len();
    let prepared = prepare_scene(&mut world, &registry, &candidate, &occurrence(), None).unwrap();
    assert_eq!(world.entities().len(), population);
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert_eq!(probe.cells_address.load(Ordering::SeqCst), cells_address);
    drop(candidate);
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let instance = report.instance.unwrap();
    let EntityLookup::Resolved(entity) = instance.resolve(&world, &root()) else {
        panic!("the prepared terrain must resolve");
    };
    assert_eq!(world.get::<Payload>(entity).unwrap().0, effective_value());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn frozen_default_materialization_keeps_required_type_coverage_and_numeric_checks() {
    let probe = Arc::new(DecodeProbe::default());
    let registry = registry_builder(ComponentSchemaVersion::ONE, &probe)
        .freeze()
        .unwrap();
    let previous = compile(&document(source_value()), &registry);
    let ComponentValue::Map(fields) = source_value() else {
        unreachable!()
    };
    let mut missing = fields.clone();
    missing.remove("label");
    let mut wrong_type = fields.clone();
    wrong_type.insert("layers".into(), ComponentValue::String("three".into()));
    let mut unknown = fields.clone();
    unknown.insert("visibile".into(), ComponentValue::Bool(true));
    let mut out_of_range = fields;
    out_of_range.insert("layers".into(), ComponentValue::U64(256));

    for (case, fields) in [
        ("required field", missing),
        ("wire type", wrong_type),
        ("unknown field", unknown),
        ("numeric domain", out_of_range),
    ] {
        let source = document(ComponentValue::Map(fields));
        let before = source.clone();
        let report = source.compile(&registry, &InMemoryPrefabSourceResolver::new());
        assert!(report.candidate.is_none(), "{case}: {report:?}");
        assert_eq!(
            report.failure.as_ref().unwrap().reason(),
            &SceneCompilationFailureKind::Component,
            "{case}: {report:?}",
        );
        assert!(report.diagnostics.has_errors(), "{case}: {report:?}");
        assert_eq!(source, before, "{case}");
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0, "{case}");
        assert_eq!(
            previous.document().entities()[0].components()[&component()].value(),
            &effective_value(),
            "{case}",
        );
    }
}

#[test]
fn migrated_out_of_range_output_does_not_inherit_source_admission() {
    let probe = Arc::new(DecodeProbe::default());
    let previous_registry = registry_builder(ComponentSchemaVersion::ONE, &probe)
        .freeze()
        .unwrap();
    let source = document(source_value());
    let before = source.clone();
    let previous = compile(&source, &previous_registry);
    let migrations = Arc::new(AtomicUsize::new(0));
    let version = ComponentSchemaVersion::new(2).unwrap();
    let mut receiving = registry_builder(version, &probe);
    receiving
        .register_component_migration(&component(), ComponentSchemaVersion::ONE, version, {
            let migrations = Arc::clone(&migrations);
            move |mut value| {
                migrations.fetch_add(1, Ordering::SeqCst);
                let ComponentValue::Map(fields) = &mut value else {
                    unreachable!()
                };
                fields.insert("layers".into(), ComponentValue::U64(256));
                Ok(value)
            }
        })
        .unwrap();
    let receiving = receiving.freeze().unwrap();

    let report = source.compile(&receiving, &InMemoryPrefabSourceResolver::new());
    assert!(report.candidate.is_none(), "{report:?}");
    assert_eq!(
        report.failure.as_ref().unwrap().reason(),
        &SceneCompilationFailureKind::Component,
    );
    assert!(report.diagnostics.has_errors(), "{report:?}");
    assert_eq!(migrations.load(Ordering::SeqCst), 1);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    assert_eq!(source, before);
    assert_eq!(
        previous.document().entities()[0].components()[&component()].value(),
        &effective_value(),
    );
}

#[test]
fn receiving_frozen_values_keeps_exact_node_byte_and_depth_limits_before_decoding() {
    let probe = Arc::new(DecodeProbe::default());
    let registry = registry_builder(ComponentSchemaVersion::ONE, &probe)
        .freeze()
        .unwrap();
    let candidate = compile(&document(source_value()), &registry);
    assert!(
        candidate.document().entities()[0].components()[&component()]
            .value()
            .as_value()
            .is_none()
    );
    let bindings = SceneComponentBindings::new(registry, []).unwrap();
    let cost = effective_value().cost();
    let mut world = World::new();
    let sentinel = world.spawn_empty().id();
    let population = world.entities().len();

    for (kind, allowance) in [
        (
            "materialization-node-limit",
            limits(cost.nodes() - 1, cost.logical_bytes(), 5),
        ),
        (
            "materialization-byte-limit",
            limits(cost.nodes(), cost.logical_bytes() - 1, 5),
        ),
        (
            "materialization-depth-limit",
            limits(cost.nodes(), cost.logical_bytes(), 4),
        ),
    ] {
        let rejected = bindings
            .prepare_scene_with_limits(&mut world, &candidate, &occurrence(), None, allowance)
            .expect_err(kind);
        assert!(
            rejected.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.invalid-component-payload"
                    && diagnostic.fields().iter().any(|field| {
                        field.key().as_str() == "codec-error-kind"
                            && field.value() == DiagnosticValueRef::Identifier(kind)
                    })
            }),
            "{kind}: {rejected:?}",
        );
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0, "{kind}");
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
            limits(cost.nodes(), cost.logical_bytes(), 5),
        )
        .unwrap();
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert_eq!(world.entities().len(), population);
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let instance = report.instance.unwrap();
    let EntityLookup::Resolved(entity) = instance.resolve(&world, &root()) else {
        panic!("the terrain must resolve after the earlier refusals");
    };
    assert_eq!(world.get::<Payload>(entity).unwrap().0, effective_value());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert!(world.get_entity(sentinel).is_ok());
}
