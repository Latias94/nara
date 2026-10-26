//! Component value programs exercised through public expansion and complete source namespaces.

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::{Diagnostic, DiagnosticValueRef};
use nara_identity::EntityReference;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueShape, PersistentComponentSchemaOwnerId,
};

use crate::{
    InMemoryPrefabSourceResolver, PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode,
    PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, PrefabExpansionLimits,
    PrefabExpansionOptions, PrefabExpansionReport, PrefabInstance, PrefabInstanceHop,
    PrefabOverlay, PrefabPlacement, PrefabPlacementParent, PrefabPlacementSlot,
    PrefabProjectionStep, PrefabPropertyClaim, PrefabPropertyIntent, PrefabReferenceBinding,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, SourceDocumentKey,
    SourceDocumentKind,
};

#[derive(nara_ecs::Component)]
struct Link(EntityReference);

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn request(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/component-programs/{name}.ron")).unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.OverlayLink")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.overlay-links").unwrap()
}

fn local(entity: &SceneEntityId) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: entity.clone(),
    })
}

fn payload(entity: &SceneEntityId) -> ComponentValue {
    ComponentValue::map([("target", local(entity))])
}

fn empty() -> ComponentValue {
    ComponentValue::map([] as [(&str, ComponentValue); 0])
}

fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner())
}

fn schema() -> ComponentSchema {
    ComponentSchema::new(component(), "Link", ComponentSchemaVersion::ONE)
        .with_capabilities([ComponentCapability::Scene])
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::EntityRef,
            local(&id("socket")),
        )
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::EntityRef])])
}

fn registry(native: bool) -> ComponentRegistry {
    let mut registry = ComponentRegistry::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("game.overlay-links"),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    if native {
        registry
            .register_persistent_component_with_codec::<Link, _, _>(
                schema(),
                |value| Ok(Link(value.field_entity_reference("target")?.clone())),
                |value| {
                    Ok(ComponentValue::map([(
                        "target",
                        ComponentValue::EntityReference(value.0.clone()),
                    )]))
                },
            )
            .unwrap();
    }
    registry.freeze().unwrap();
    assert!(registry.is_frozen());
    registry
}

fn binding(slot: ComponentFieldPath) -> PrefabReferenceBinding {
    PrefabReferenceBinding {
        slot,
        expected: id("socket"),
        target: PrefabEntityTarget::source(id("socket")),
    }
}

fn instance(base: &str, overlay: Option<PrefabOverlay>) -> PrefabInstance {
    PrefabInstance {
        source: request(base),
        overrides: overlay,
    }
}

fn scene(overlay: PrefabOverlay) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("instance"));
    anchor.prefab = Some(instance("base", Some(overlay)));
    SceneDocument::from_ordered_roots([anchor])
}

fn fixture() -> InMemoryPrefabSourceResolver {
    InMemoryPrefabSourceResolver::new().with_prefab(
        request("base"),
        key("base"),
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("host"))
                .with_component(component(), record(payload(&id("inherited")))),
            SceneEntityRecord::new(id("socket")),
            SceneEntityRecord::new(id("inherited")),
        ]),
    )
}

fn add(overlay: &mut PrefabOverlay, addition: PrefabAddition) {
    overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(addition.id.clone()),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    overlay.additions.push(addition);
}

fn set(mode: PrefabComponentSetMode, target: PrefabEntityTarget) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode,
        target,
        component: component(),
        record: record(empty()),
        bindings: vec![binding(ComponentFieldPath::from_fields(["target"]))],
    }
}

fn expand(
    registry: &ComponentRegistry,
    document: &SceneDocument,
    sources: &InMemoryPrefabSourceResolver,
    limits: PrefabExpansionLimits,
) -> PrefabExpansionReport {
    let before_document = document.clone();
    let before_sources = sources.clone();
    let report = document.expand_prefabs_with_options(
        registry,
        sources,
        PrefabExpansionOptions::default().with_limits(limits),
    );
    assert_eq!(
        document, &before_document,
        "source intent must be immutable"
    );
    assert_eq!(sources, &before_sources, "captured bases must be immutable");
    report
}

fn expanded(report: &PrefabExpansionReport) -> &SceneDocument {
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report
        .document()
        .expect("expected a complete public candidate")
}

fn projected(report: &PrefabExpansionReport, name: &str) -> SceneEntityId {
    expanded(report);
    let mut matches = report.provenance().unwrap().iter().filter(|(_, origin)| {
        origin.local_entity_id() == &id(name) && origin.projection_path().is_some()
    });
    let (entity, origin) = matches.next().expect("expected projected source identity");
    assert!(matches.next().is_none(), "projection must be unique");
    assert_eq!(
        origin.projection_path().unwrap().root_anchor(),
        &id("instance")
    );
    entity.clone()
}

fn entity<'a>(report: &'a PrefabExpansionReport, name: &str) -> &'a SceneEntityRecord {
    let entity = projected(report, name);
    expanded(report)
        .entities
        .iter()
        .find(|record| record.id == entity)
        .unwrap()
}

fn stored<'a>(report: &'a PrefabExpansionReport, name: &str) -> &'a SceneComponentRecord {
    &entity(report, name).components[&component()]
}

fn field<'a>(diagnostic: &'a Diagnostic, name: &str) -> DiagnosticValueRef<'a> {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == name)
        .unwrap_or_else(|| panic!("missing {name}: {diagnostic:?}"))
        .value()
}

fn rejected<'a>(report: &'a PrefabExpansionReport, code: &str) -> &'a Diagnostic {
    assert!(report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.candidate.is_none());
    assert!(report.document().is_none());
    assert!(report.provenance().is_none());
    let mut diagnostics = report.diagnostics.iter();
    let diagnostic = diagnostics.next().expect("expected rejection diagnostic");
    assert_eq!(diagnostic.code().as_str(), code, "{:?}", report.diagnostics);
    assert!(diagnostics.next().is_none(), "{:?}", report.diagnostics);
    diagnostic
}

fn conflict(report: &PrefabExpansionReport, expected: &str) {
    assert_eq!(
        field(
            rejected(report, "scene.prefab-overlay-conflict"),
            "conflict-kind"
        ),
        DiagnosticValueRef::Identifier(expected),
    );
}

#[test]
fn addition_defaults_bind_once_and_preserve_owner_and_inherited_values() {
    let mut addition = PrefabAddition::new(id("new"));
    addition.components.insert(component(), record(empty()));
    addition.bindings.insert(
        component(),
        vec![binding(ComponentFieldPath::from_fields(["target"]))],
    );
    let mut overlay = PrefabOverlay::new(key("base"));
    add(&mut overlay, addition);
    let report = expand(
        &registry(true),
        &scene(overlay),
        &fixture(),
        Default::default(),
    );

    assert_eq!(expanded(&report).entities.len(), 5);
    assert_eq!(
        stored(&report, "new"),
        &record(payload(&projected(&report, "socket"))),
    );
    assert_eq!(
        stored(&report, "host"),
        &record(payload(&projected(&report, "inherited"))),
    );
    assert_eq!(
        report.provenance().unwrap()[&projected(&report, "new")]
            .projection_path()
            .unwrap()
            .steps(),
        &[PrefabProjectionStep::InstanceAddition { entity: id("new") }],
    );
}

#[test]
fn full_set_defaults_bind_for_add_and_replace_and_keep_the_declared_owner() {
    for (mode, target) in [
        (PrefabComponentSetMode::Add, "socket"),
        (PrefabComponentSetMode::Replace, "host"),
    ] {
        let mut overlay = PrefabOverlay::new(key("base"));
        overlay
            .components
            .push(set(mode, PrefabEntityTarget::source(id(target))));
        let report = expand(
            &registry(true),
            &scene(overlay),
            &fixture(),
            Default::default(),
        );

        assert_eq!(expanded(&report).entities.len(), 4);
        assert_eq!(
            stored(&report, target),
            &record(payload(&projected(&report, "socket"))),
        );
        if mode == PrefabComponentSetMode::Add {
            assert_eq!(
                stored(&report, "host"),
                &record(payload(&projected(&report, "inherited"))),
            );
        }
    }
}

#[test]
fn selected_scalar_and_absent_default_use_empty_payload_slots() {
    for intent in [
        PrefabPropertyIntent::Set(local(&id("socket"))),
        PrefabPropertyIntent::Absent,
    ] {
        let mut overlay = PrefabOverlay::new(key("base"));
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("host")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                intent,
                bindings: vec![binding(ComponentFieldPath::empty())],
            }],
        });
        let report = expand(
            &registry(true),
            &scene(overlay),
            &fixture(),
            Default::default(),
        );
        assert_eq!(
            stored(&report, "host"),
            &record(payload(&projected(&report, "socket"))),
        );
    }
}

#[test]
fn orphan_addition_bindings_and_duplicate_component_declarations_refuse() {
    let mut addition = PrefabAddition::new(id("new"));
    addition.bindings.insert(component(), Vec::new());
    let mut overlay = PrefabOverlay::new(key("base"));
    add(&mut overlay, addition);
    let report = expand(
        &registry(true),
        &scene(overlay),
        &fixture(),
        Default::default(),
    );
    conflict(&report, "reference");

    let mut addition = PrefabAddition::new(id("new"));
    addition.components.insert(component(), record(empty()));
    addition.bindings.insert(
        component(),
        vec![binding(ComponentFieldPath::from_fields(["target"]))],
    );
    let mut overlay = PrefabOverlay::new(key("base"));
    add(&mut overlay, addition);
    overlay.components.push(PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::addition(id("new")),
        component: component(),
    });
    let report = expand(
        &registry(true),
        &scene(overlay),
        &fixture(),
        Default::default(),
    );
    conflict(&report, "duplicate-intent");

    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(set(
        PrefabComponentSetMode::Replace,
        PrefabEntityTarget::source(id("host")),
    ));
    overlay.components.push(PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::source(id("host")),
        component: component(),
    });
    let report = expand(
        &registry(true),
        &scene(overlay),
        &fixture(),
        Default::default(),
    );
    conflict(&report, "duplicate-intent");
}

#[test]
fn missing_native_noncurrent_and_foreign_owner_records_refuse_activation() {
    // A known schema without its native binding cannot reach a frozen registry.
    let mut schema_only = ComponentRegistry::new();
    schema_only.register_component_schema(schema()).unwrap();
    assert!(matches!(
        schema_only.freeze(),
        Err(ComponentRegistryError::MissingNativeBinding { component_id })
            if component_id == component()
    ));

    let mut foreign = record(empty());
    foreign.owner = Some(PersistentComponentSchemaOwnerId::try_new("game.foreign").unwrap());
    let mut noncurrent = record(empty());
    noncurrent.version = ComponentSchemaVersion::new(2).unwrap();
    for (native, replacement, expected) in [
        (
            false,
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, empty()),
            "component",
        ),
        (true, noncurrent, "component"),
        (true, foreign, "owner-admission"),
    ] {
        let mut overlay = PrefabOverlay::new(key("base"));
        let mut intent = set(
            PrefabComponentSetMode::Replace,
            PrefabEntityTarget::source(id("host")),
        );
        let PrefabComponentIntent::Set { record, .. } = &mut intent else {
            unreachable!()
        };
        *record = replacement;
        overlay.components.push(intent);
        let report = expand(
            &registry(native),
            &scene(overlay),
            &fixture(),
            Default::default(),
        );
        conflict(&report, expected);
    }
}

#[test]
fn two_record_defaults_and_bindings_share_exact_value_and_generated_id_allowances() {
    let registry = registry(true);
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("base"),
        key("base"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("socket"))]),
    );
    let final_value = payload(&id("prefab_0")).cost();
    let nodes = final_value.nodes() * 2;
    let bytes = final_value.logical_bytes() * 2;
    // Three projected IDs, three anchor-child IDs, and two bound reference IDs.
    let generated = (3 + 3 + 2) * id("prefab_0").as_str().len();
    assert_eq!((nodes, bytes, generated), (4, 28, 64));
    let limits = |nodes, bytes, generated| {
        PrefabExpansionLimits::default()
            .with_materialized_components(ItemLimit::new(2).unwrap())
            .with_materialized_value_nodes(ItemLimit::new(nodes).unwrap())
            .with_materialized_value_bytes(ByteLimit::new(bytes).unwrap())
            .with_generated_identifier_bytes(ByteLimit::new(generated).unwrap())
    };

    for second_uses_default in [true, false] {
        let mut overlay = PrefabOverlay::new(key("base"));
        for name in ["first", "second"] {
            let mut addition = PrefabAddition::new(id(name));
            addition.components.insert(
                component(),
                record(if name == "second" && !second_uses_default {
                    payload(&id("socket"))
                } else {
                    empty()
                }),
            );
            addition.bindings.insert(
                component(),
                vec![binding(ComponentFieldPath::from_fields(["target"]))],
            );
            add(&mut overlay, addition);
        }
        let document = scene(overlay);
        let report = expand(
            &registry,
            &document,
            &sources,
            limits(nodes, bytes, generated),
        );
        assert_eq!(expanded(&report).entities.len(), 4);
        assert_eq!(projected(&report, "socket"), id("prefab_0"));
        for name in ["first", "second"] {
            assert_eq!(stored(&report, name), &record(payload(&id("prefab_0"))));
        }

        for (limited_nodes, limited_bytes) in [(nodes - 1, bytes), (nodes, bytes - 1)] {
            let report = expand(
                &registry,
                &document,
                &sources,
                limits(limited_nodes, limited_bytes, generated),
            );
            if second_uses_default {
                let diagnostic = rejected(&report, "scene.prefab-default-materialization-failed");
                assert_eq!(
                    field(diagnostic, "component-id"),
                    DiagnosticValueRef::Identifier(component().as_str())
                );
                assert_eq!(
                    field(diagnostic, "codec-error-kind"),
                    DiagnosticValueRef::Identifier("message")
                );
                assert_eq!(
                    field(diagnostic, "entity-id"),
                    DiagnosticValueRef::Identifier("second")
                );
            } else if limited_nodes < nodes {
                let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
                assert_eq!(
                    field(diagnostic, "budget-kind"),
                    DiagnosticValueRef::Identifier("materialized-value-nodes")
                );
                assert_eq!(
                    field(diagnostic, "observed"),
                    DiagnosticValueRef::Unsigned(nodes as u64)
                );
                assert_eq!(
                    field(diagnostic, "maximum"),
                    DiagnosticValueRef::Unsigned(limited_nodes as u64)
                );
            } else {
                let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
                assert_eq!(
                    field(diagnostic, "budget-kind"),
                    DiagnosticValueRef::Identifier("materialized-value-bytes")
                );
                assert_eq!(
                    field(diagnostic, "observed"),
                    DiagnosticValueRef::Unsigned(bytes as u64)
                );
                assert_eq!(
                    field(diagnostic, "maximum"),
                    DiagnosticValueRef::Unsigned(limited_bytes as u64)
                );
            }
        }

        let report = expand(
            &registry,
            &document,
            &sources,
            limits(nodes, bytes, generated - 1),
        );
        let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
        assert_eq!(
            field(diagnostic, "budget-kind"),
            DiagnosticValueRef::Identifier("generated-identifier-bytes")
        );
        assert_eq!(
            field(diagnostic, "observed"),
            DiagnosticValueRef::Unsigned(generated as u64)
        );
        assert_eq!(
            field(diagnostic, "maximum"),
            DiagnosticValueRef::Unsigned((generated - 1) as u64)
        );

        let report = expand(
            &registry,
            &document,
            &sources,
            limits(nodes, bytes, generated)
                .with_materialized_components(ItemLimit::new(1).unwrap()),
        );
        let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
        assert_eq!(
            field(diagnostic, "budget-kind"),
            DiagnosticValueRef::Identifier("materialized-components")
        );
        assert_eq!(
            field(diagnostic, "observed"),
            DiagnosticValueRef::Unsigned(2)
        );
        assert_eq!(
            field(diagnostic, "maximum"),
            DiagnosticValueRef::Unsigned(1)
        );
    }
}

#[test]
fn direct_addition_intents_refuse_even_when_the_component_was_not_declared() {
    let mut overlay = PrefabOverlay::new(key("base"));
    add(&mut overlay, PrefabAddition::new(id("new")));
    overlay.components.push(set(
        PrefabComponentSetMode::Add,
        PrefabEntityTarget::addition(id("new")),
    ));
    let report = expand(
        &registry(true),
        &scene(overlay),
        &fixture(),
        Default::default(),
    );
    conflict(&report, "duplicate-intent");
}

#[test]
fn inherited_addition_intents_with_a_nonempty_instance_route_remain_legal() {
    let mut inner = PrefabOverlay::new(key("inner"));
    add(&mut inner, PrefabAddition::new(id("child")));
    let mut nested = SceneEntityRecord::new(id("nested"));
    nested.prefab = Some(instance("inner", Some(inner)));
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            request("inner"),
            key("inner"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("inner-root"))]),
        )
        .with_prefab(
            request("base"),
            key("base"),
            PrefabDocument::from_ordered_roots([nested, SceneEntityRecord::new(id("socket"))]),
        );
    let mut outer = PrefabOverlay::new(key("base"));
    outer.components.push(set(
        PrefabComponentSetMode::Add,
        PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                expected_base: key("inner"),
            }],
            entity: PrefabEntityOrigin::InstanceAddition(id("child")),
        },
    ));
    let report = expand(&registry(true), &scene(outer), &sources, Default::default());
    assert_eq!(expanded(&report).entities.len(), 5);
    assert_eq!(
        stored(&report, "child"),
        &record(payload(&projected(&report, "socket"))),
    );
    assert_eq!(
        report.provenance().unwrap()[&projected(&report, "child")]
            .projection_path()
            .unwrap()
            .steps(),
        &[
            PrefabProjectionStep::SourceEntity {
                source_document: request("base"),
                canonical_source: key("base"),
                entity: id("nested"),
            },
            PrefabProjectionStep::InstanceAddition {
                entity: id("child")
            },
        ],
    );
}

#[test]
fn suppress_frees_selected_materialization_budget_for_a_later_add() {
    let registry = registry(true);
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::source(id("host")),
        component: component(),
    });
    overlay.components.push(set(
        PrefabComponentSetMode::Add,
        PrefabEntityTarget::source(id("socket")),
    ));
    let final_value = payload(&id("prefab_2")).cost();
    let bytes = final_value.logical_bytes();
    let generated = (3 + 3 + 1) * id("prefab_2").as_str().len();
    assert_eq!((final_value.nodes(), bytes, generated), (2, 14, 56));
    let limits = PrefabExpansionLimits::default()
        .with_materialized_components(ItemLimit::new(1).unwrap())
        .with_materialized_value_nodes(ItemLimit::new(final_value.nodes()).unwrap())
        .with_materialized_value_bytes(ByteLimit::new(bytes).unwrap())
        .with_generated_identifier_bytes(ByteLimit::new(generated).unwrap());
    let report = expand(&registry, &scene(overlay.clone()), &fixture(), limits);
    assert!(entity(&report, "host").components.is_empty());
    assert_eq!(
        stored(&report, "socket"),
        &record(payload(&projected(&report, "socket"))),
    );

    overlay.components.remove(0);
    let document = scene(overlay);
    let report = expand(
        &registry,
        &document,
        &fixture(),
        limits
            .with_materialized_value_nodes(ItemLimit::new(4).unwrap())
            .with_materialized_value_bytes(ByteLimit::new(2 * bytes + 1).unwrap()),
    );
    let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
    assert_eq!(
        field(diagnostic, "budget-kind"),
        DiagnosticValueRef::Identifier("materialized-components")
    );
    assert_eq!(
        field(diagnostic, "observed"),
        DiagnosticValueRef::Unsigned(2)
    );
    assert_eq!(
        field(diagnostic, "maximum"),
        DiagnosticValueRef::Unsigned(1)
    );

    let limits = limits.with_materialized_components(ItemLimit::new(2).unwrap());
    let report = expand(
        &registry,
        &document,
        &fixture(),
        limits.with_materialized_value_bytes(ByteLimit::new(2 * bytes + 1).unwrap()),
    );
    let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
    assert_eq!(
        field(diagnostic, "budget-kind"),
        DiagnosticValueRef::Identifier("materialized-value-nodes")
    );
    assert_eq!(
        field(diagnostic, "observed"),
        DiagnosticValueRef::Unsigned(3)
    );
    assert_eq!(
        field(diagnostic, "maximum"),
        DiagnosticValueRef::Unsigned(final_value.nodes() as u64)
    );
    let report = expand(
        &registry,
        &document,
        &fixture(),
        limits.with_materialized_value_nodes(ItemLimit::new(final_value.nodes() * 2).unwrap()),
    );
    let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
    assert_eq!(
        field(diagnostic, "budget-kind"),
        DiagnosticValueRef::Identifier("materialized-value-bytes")
    );
    assert_eq!(
        field(diagnostic, "observed"),
        DiagnosticValueRef::Unsigned(payload(&id("inherited")).cost().logical_bytes() as u64)
    );
    assert_eq!(
        field(diagnostic, "maximum"),
        DiagnosticValueRef::Unsigned(bytes as u64)
    );
}
