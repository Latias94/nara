//! Component value programs exercised through public expansion and complete source namespaces.

#[path = "component_program_tests/property_limits.rs"]
mod property_limits;
#[path = "component_program_tests/reference_paths.rs"]
mod reference_paths;

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::{Diagnostic, DiagnosticValueRef};
use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget, EntityReference};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentRegistryError, ComponentRegistrySnapshot, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueShape, PersistentComponentSchemaOwnerId,
};

use crate::{
    CompiledSceneComponent, CompiledSceneDocument, CompiledSceneEntity,
    InMemoryPrefabSourceResolver, PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode,
    PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop,
    PrefabOverlay, PrefabPlacement, PrefabPlacementParent, PrefabPlacementSlot,
    PrefabProjectionStep, PrefabPropertyClaim, PrefabPropertyIntent, PrefabSourceResolver,
    SceneCompilationLimits, SceneCompilationOptions, SceneCompilationReport, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
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
    effective_record(value)
        .with_contextual_default(ComponentFieldId::new("target"), local(&id("socket")))
}

fn effective_record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner())
}

fn authored(reference: AuthoredEntityReference) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored { reference })
}

fn selected_socket() -> ComponentValue {
    authored(AuthoredEntityReference {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("instance")),
            expected_base: key("base"),
        }],
        target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("socket"))),
    })
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

fn registry(native: bool) -> ComponentRegistrySnapshot {
    let mut registry = nara_reflect::ComponentRegistryBuilder::from_owner_catalog_candidate(
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
    registry.freeze().unwrap()
}

fn instance(base: &str, overlay: Option<PrefabOverlay>) -> PrefabInstance {
    PrefabInstance {
        source: request(base),
        arguments: None,
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
        record: effective_record(empty())
            .with_contextual_default(ComponentFieldId::new("target"), selected_socket()),
    }
}

fn expand(
    registry: &ComponentRegistrySnapshot,
    document: &SceneDocument,
    sources: &InMemoryPrefabSourceResolver,
    limits: SceneCompilationLimits,
) -> SceneCompilationReport {
    let before_document = document.clone();
    let before_sources = sources.clone();
    let report = document.compile_with_options(
        registry,
        sources,
        SceneCompilationOptions::default().with_limits(limits),
    );
    assert_eq!(
        document, &before_document,
        "source intent must be immutable"
    );
    assert_eq!(sources, &before_sources, "captured bases must be immutable");
    report
}

fn expanded(report: &SceneCompilationReport) -> &CompiledSceneDocument {
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report
        .document()
        .expect("expected a complete public candidate")
}

fn projected(report: &SceneCompilationReport, name: &str) -> SceneEntityId {
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

fn entity<'a>(report: &'a SceneCompilationReport, name: &str) -> &'a CompiledSceneEntity {
    let entity = projected(report, name);
    expanded(report)
        .entities()
        .iter()
        .find(|record| record.id() == &entity)
        .unwrap()
}

fn stored<'a>(report: &'a SceneCompilationReport, name: &str) -> &'a CompiledSceneComponent {
    &entity(report, name).components()[&component()]
}

fn assert_record(actual: &CompiledSceneComponent, expected: &SceneComponentRecord) {
    assert_eq!(actual.schema_owner(), expected.owner());
    assert_eq!(actual.version(), expected.version);
    assert_eq!(actual.value(), &expected.value);
}

fn field<'a>(diagnostic: &'a Diagnostic, name: &str) -> DiagnosticValueRef<'a> {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == name)
        .unwrap_or_else(|| panic!("missing {name}: {diagnostic:?}"))
        .value()
}

fn rejected<'a>(report: &'a SceneCompilationReport, code: &str) -> &'a Diagnostic {
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

fn conflict(report: &SceneCompilationReport, expected: &str) {
    assert_eq!(
        field(
            rejected(report, "scene.prefab-compilation-failed"),
            "failure-kind"
        ),
        DiagnosticValueRef::Identifier(expected),
    );
}

#[test]
fn addition_defaults_bind_once_and_preserve_owner_and_inherited_values() {
    let mut addition = PrefabAddition::new(id("new"));
    addition.components.insert(
        component(),
        effective_record(empty())
            .with_contextual_default(ComponentFieldId::new("target"), selected_socket()),
    );
    let mut overlay = PrefabOverlay::new(key("base"));
    add(&mut overlay, addition);
    let report = expand(
        &registry(true),
        &scene(overlay),
        &fixture(),
        Default::default(),
    );

    assert_eq!(expanded(&report).entities().len(), 5);
    assert_record(
        stored(&report, "new"),
        &effective_record(payload(&projected(&report, "socket"))),
    );
    assert_record(
        stored(&report, "host"),
        &effective_record(payload(&projected(&report, "inherited"))),
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

        assert_eq!(expanded(&report).entities().len(), 4);
        assert_record(
            stored(&report, target),
            &effective_record(payload(&projected(&report, "socket"))),
        );
        if mode == PrefabComponentSetMode::Add {
            assert_record(
                stored(&report, "host"),
                &effective_record(payload(&projected(&report, "inherited"))),
            );
        }
    }
}

#[test]
fn selected_scalar_and_absent_default_resolve_the_selected_source() {
    for intent in [
        PrefabPropertyIntent::Set(selected_socket()),
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
            }],
        });
        let report = expand(
            &registry(true),
            &scene(overlay),
            &fixture(),
            Default::default(),
        );
        assert_record(
            stored(&report, "host"),
            &effective_record(payload(&projected(&report, "socket"))),
        );
    }
}

#[test]
fn later_property_pin_replaces_an_unresolved_inherited_reference_before_target_lookup() {
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("base"),
        key("base"),
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("host"))
                .with_component(component(), record(payload(&id("missing")))),
            SceneEntityRecord::new(id("socket")),
        ]),
    );
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("host")),
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
            intent: PrefabPropertyIntent::Set(selected_socket()),
        }],
    });
    let report = expand(
        &registry(true),
        &scene(overlay),
        &sources,
        Default::default(),
    );
    assert_record(
        stored(&report, "host"),
        &effective_record(payload(&projected(&report, "socket"))),
    );
    let mut anchor = SceneEntityRecord::new(id("instance"));
    anchor.prefab = Some(instance("base", None));
    let uncorrected = expand(
        &registry(true),
        &SceneDocument::from_ordered_roots([anchor]),
        &sources,
        Default::default(),
    );
    assert!(uncorrected.candidate.is_none());
    assert_eq!(
        uncorrected.failure.as_ref().unwrap().reason(),
        &crate::SceneCompilationFailureKind::Reference,
    );
}

#[test]
fn overwritten_property_reference_does_not_read_its_missing_nested_source() {
    let inner_target = |name: &str| PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
            expected_base: key("base"),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id(name)),
    };
    let mut inner = PrefabOverlay::new(key("base"));
    inner.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("host")),
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
            intent: PrefabPropertyIntent::Set(authored(AuthoredEntityReference {
                through: vec![
                    PrefabInstanceHop {
                        anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                        expected_base: key("base"),
                    },
                    PrefabInstanceHop {
                        anchor: PrefabEntityOrigin::SourceEntity(id("discarded")),
                        expected_base: key("missing"),
                    },
                ],
                target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                    "socket",
                ))),
            })),
        }],
    });
    let mut discarded = SceneEntityRecord::new(id("discarded"));
    discarded.prefab = Some(instance("missing", None));
    let mut nested = SceneEntityRecord::new(id("nested"));
    nested.prefab = Some(instance("base", Some(inner)));
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            request("base"),
            key("base"),
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("host"))
                    .with_component(component(), record(payload(&id("socket")))),
                SceneEntityRecord::new(id("socket")),
                discarded,
            ]),
        )
        .with_prefab(
            request("outer"),
            key("outer"),
            PrefabDocument::from_ordered_roots([nested]),
        );
    for replace in [true, false] {
        let mut outer = PrefabOverlay::new(key("outer"));
        outer.suppressions.push(inner_target("discarded"));
        if replace {
            outer.components.push(PrefabComponentIntent::Properties {
                target: inner_target("host"),
                component: component(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                    intent: PrefabPropertyIntent::Set(authored(AuthoredEntityReference {
                        through: vec![
                            PrefabInstanceHop {
                                anchor: PrefabEntityOrigin::SourceEntity(id("instance")),
                                expected_base: key("outer"),
                            },
                            PrefabInstanceHop {
                                anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                                expected_base: key("base"),
                            },
                        ],
                        target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(
                            id("socket"),
                        )),
                    })),
                }],
            });
        }
        let mut anchor = SceneEntityRecord::new(id("instance"));
        anchor.prefab = Some(instance("outer", Some(outer)));
        let document = SceneDocument::from_ordered_roots([anchor]);
        let before = document.clone();
        let mut reads = Vec::new();
        let report = document
            .try_compile_with_loader(
                &registry(true),
                SceneCompilationOptions::default(),
                None,
                |reference, _| {
                    reads.push(reference.clone());
                    Ok::<_, std::convert::Infallible>(sources.resolve_prefab(reference))
                },
            )
            .unwrap();
        assert_eq!(document, before);
        if replace {
            assert_eq!(reads, [request("outer"), request("base")]);
            assert_record(
                stored(&report, "host"),
                &effective_record(payload(&projected(&report, "socket"))),
            );
        } else {
            assert!(report.candidate.is_none());
            assert!(report.diagnostics.has_errors());
            assert_eq!(&reads[..2], &[request("outer"), request("base")]);
            assert!(
                reads[2..]
                    .iter()
                    .all(|source| source == &request("missing"))
            );
        }
    }
}

#[test]
fn absent_default_and_untouched_references_keep_the_original_source_scope() {
    #[derive(nara_ecs::Component)]
    struct LinkPair(EntityReference, EntityReference);

    let mut fields = schema().fields().to_vec();
    fields.push(
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("untouched"),
            "Untouched",
            ComponentFieldPath::from_fields(["untouched"]),
            ComponentValueShape::EntityRef,
            local(&id("socket")),
        )
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::EntityRef]),
    );
    let mut definitions = nara_reflect::ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("game.overlay-links"),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    definitions
        .register_persistent_component_with_codec::<LinkPair, _, _>(
            ComponentSchema::new(component(), "Link pair", ComponentSchemaVersion::ONE)
                .with_capabilities([ComponentCapability::Scene])
                .with_fields(fields),
            |value| {
                Ok(LinkPair(
                    value.field_entity_reference("target")?.clone(),
                    value.field_entity_reference("untouched")?.clone(),
                ))
            },
            |value| {
                Ok(ComponentValue::map([
                    ("target", ComponentValue::EntityReference(value.0.clone())),
                    (
                        "untouched",
                        ComponentValue::EntityReference(value.1.clone()),
                    ),
                ]))
            },
        )
        .unwrap();
    let definitions = definitions.freeze().unwrap();
    let mut nested = SceneEntityRecord::new(id("nested"));
    nested.prefab = Some(instance("base", None));
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            request("base"),
            key("base"),
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("host")).with_component(
                    component(),
                    record(ComponentValue::map([
                        ("target", local(&id("socket"))),
                        ("untouched", local(&id("socket"))),
                    ]))
                    .with_contextual_default(
                        ComponentFieldId::new("untouched"),
                        local(&id("socket")),
                    ),
                ),
                SceneEntityRecord::new(id("socket")),
            ]),
        )
        .with_prefab(
            request("outer"),
            key("outer"),
            PrefabDocument::from_ordered_roots([nested, SceneEntityRecord::new(id("socket"))]),
        );
    let mut overlay = PrefabOverlay::new(key("outer"));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                expected_base: key("base"),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("host")),
        },
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
            intent: PrefabPropertyIntent::Absent,
        }],
    });
    let mut anchor = SceneEntityRecord::new(id("instance"));
    anchor.prefab = Some(instance("outer", Some(overlay)));
    let report = expand(
        &definitions,
        &SceneDocument::from_ordered_roots([anchor]),
        &sources,
        Default::default(),
    );
    let socket = |source: &str| {
        expanded(&report);
        report
            .provenance()
            .unwrap()
            .iter()
            .find_map(|(entity, origin)| {
                (origin.local_entity_id() == &id("socket")
                    && matches!(
                        origin.projection_path().and_then(|path| path.steps().last()),
                        Some(PrefabProjectionStep::SourceEntity { canonical_source, .. })
                            if canonical_source == &key(source)
                    ))
                .then(|| entity.clone())
            })
            .unwrap()
    };
    assert_record(
        stored(&report, "host"),
        &effective_record(ComponentValue::map([
            ("target", local(&socket("base"))),
            ("untouched", local(&socket("base"))),
        ])),
    );
    assert_ne!(socket("base"), socket("outer"));
}

#[test]
fn deferred_reference_metadata_does_not_inflate_a_one_byte_final_local_id() {
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("host")).with_component(component(), record(payload(&id("a")))),
        SceneEntityRecord::new(id("a")),
    ]);
    let cost = payload(&id("a")).cost();
    assert_eq!(cost.logical_bytes(), 7);
    let limits = SceneCompilationLimits::default()
        .with_materialized_value_nodes(ItemLimit::new(cost.nodes()).unwrap())
        .with_materialized_value_bytes(ByteLimit::new(cost.logical_bytes()).unwrap());
    let sources = InMemoryPrefabSourceResolver::new();
    let accepted = expand(&registry(true), &document, &sources, limits);
    assert_record(
        &expanded(&accepted)
            .entities()
            .iter()
            .find(|record| record.id() == &id("host"))
            .unwrap()
            .components()[&component()],
        &effective_record(payload(&id("a"))),
    );
    let compiled = expanded(&accepted)
        .entities()
        .iter()
        .find(|record| record.id() == &id("a"))
        .unwrap();
    let source = document
        .entities
        .iter()
        .find(|record| record.id == id("a"))
        .unwrap();
    assert_eq!(compiled.id(), &source.id);
    assert_eq!(compiled.children(), source.children.as_slice());
    assert!(compiled.components().is_empty());
    assert!(source.components.is_empty());
    let refused = expand(
        &registry(true),
        &document,
        &sources,
        limits.with_materialized_value_bytes(ByteLimit::new(cost.logical_bytes() - 1).unwrap()),
    );
    assert!(refused.candidate.is_none());
    assert_eq!(
        refused.failure.as_ref().unwrap().reason(),
        &crate::SceneCompilationFailureKind::WorkLimit,
    );
}

#[test]
fn duplicate_component_declarations_refuse() {
    let mut addition = PrefabAddition::new(id("new"));
    addition.components.insert(component(), record(empty()));
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
    let mut schema_only = nara_reflect::ComponentRegistryBuilder::new();
    schema_only.register_component_schema(schema()).unwrap();
    assert!(matches!(
        schema_only.freeze().map_err(|error| error.into_error()),
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
        if !native {
            let diagnostic = rejected(&report, "scene.unknown-component");
            assert_eq!(
                field(diagnostic, "component-id"),
                DiagnosticValueRef::Identifier(component().as_str()),
            );
            assert_eq!(
                report.failure.as_ref().unwrap().reason(),
                &crate::SceneCompilationFailureKind::Component,
            );
        } else if expected == "owner-admission" {
            let diagnostic = rejected(&report, "scene.component-owner-mismatch");
            assert_eq!(
                field(diagnostic, "component-id"),
                DiagnosticValueRef::Identifier(component().as_str())
            );
            assert_eq!(
                report.failure.as_ref().unwrap().reason(),
                &crate::SceneCompilationFailureKind::OwnerAdmission
            );
        } else {
            conflict(&report, expected);
        }
    }
}

fn assert_omitted_default_node_budget(
    document: &SceneDocument,
    sources: &InMemoryPrefabSourceResolver,
) {
    let registry = registry(true);
    let exact =
        SceneCompilationLimits::default().with_materialized_value_nodes(ItemLimit::new(4).unwrap());
    let report = expand(&registry, document, sources, exact);
    assert!(!expanded(&report).entities().is_empty());
    let limits =
        SceneCompilationLimits::default().with_materialized_value_nodes(ItemLimit::new(3).unwrap());
    let report = expand(&registry, document, sources, limits);
    let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
    assert_eq!(
        field(diagnostic, "budget-kind"),
        DiagnosticValueRef::Identifier("materialized-value-nodes"),
    );
    // Each final component occupies two nodes. Defaults and explicit pins share the same
    // aggregate output allowance after occurrence references have lowered to actual IDs.
    assert_eq!(
        field(diagnostic, "observed"),
        DiagnosticValueRef::Unsigned(4)
    );
    assert_eq!(
        field(diagnostic, "maximum"),
        DiagnosticValueRef::Unsigned(3)
    );
    let failure = report.failure.as_ref().unwrap();
    assert_eq!(
        failure.reason(),
        &crate::SceneCompilationFailureKind::WorkLimit
    );
    assert!(failure.location().is_none());
    assert!(failure.slot().is_none());

    #[cfg(feature = "serde")]
    {
        use crate::{
            __private::{AuthoringAdapterReceipt, try_compile_authoring_scene_with_loader},
            AuthoringSchemaBasis, PrefabSourceResolver, ResolvedPrefabSource,
        };
        use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleOwner};
        use std::{convert::Infallible, sync::Arc};

        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner(),
            ComponentSchemaCatalog {
                components: vec![schema().with_capabilities([ComponentCapability::Inspect])],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        let adapter =
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap();
        let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
        let report = try_compile_authoring_scene_with_loader(
            document,
            &basis,
            SceneCompilationOptions::default().with_limits(limits),
            None,
            |reference, _| {
                Ok::<_, Infallible>(sources.resolve_prefab(reference).map(|source| {
                    ResolvedPrefabSource::bound_authoring_source(
                        Arc::new(source.document().clone()),
                        basis.clone(),
                        source.key().clone(),
                    )
                }))
            },
        )
        .unwrap();
        assert!(report.candidate().is_none());
        assert!(report.budget_exceeded());
        let failure = report.failure().unwrap();
        assert_eq!(
            failure.reason(),
            &crate::SceneCompilationFailureKind::WorkLimit
        );
        assert!(failure.location().is_none());
        assert!(failure.slot().is_none());
        let diagnostic = report.diagnostics().iter().next().unwrap();
        assert_eq!(
            diagnostic.code().as_str(),
            "scene.prefab-expansion-budget-exceeded"
        );
        assert_eq!(
            field(diagnostic, "observed"),
            DiagnosticValueRef::Unsigned(4)
        );
        assert_eq!(
            field(diagnostic, "maximum"),
            DiagnosticValueRef::Unsigned(3)
        );
    }
}

#[test]
fn set_omitted_default_aggregate_node_refusal_has_no_repair_location() {
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(set(
        PrefabComponentSetMode::Add,
        PrefabEntityTarget::source(id("socket")),
    ));
    assert_omitted_default_node_budget(&scene(overlay), &fixture());
}

#[test]
fn addition_omitted_default_aggregate_node_refusal_has_no_repair_location() {
    let mut addition = PrefabAddition::new(id("new"));
    addition.components.insert(
        component(),
        effective_record(empty())
            .with_contextual_default(ComponentFieldId::new("target"), selected_socket()),
    );
    let mut overlay = PrefabOverlay::new(key("base"));
    add(&mut overlay, addition);
    assert_omitted_default_node_budget(&scene(overlay), &fixture());
}

#[test]
fn semantic_default_materialization_failure_keeps_its_component_intent_location() {
    let mut intent = set(
        PrefabComponentSetMode::Add,
        PrefabEntityTarget::source(id("socket")),
    );
    let PrefabComponentIntent::Set { record, .. } = &mut intent else {
        unreachable!()
    };
    record.value = ComponentValue::Bool(false);
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(intent);
    let report = expand(
        &registry(true),
        &scene(overlay),
        &fixture(),
        Default::default(),
    );
    let diagnostic = rejected(&report, "scene.prefab-default-materialization-failed");
    assert_eq!(
        field(diagnostic, "codec-error-kind"),
        DiagnosticValueRef::Identifier("invalid-field"),
    );
    let failure = report.failure.as_ref().unwrap();
    assert_eq!(
        failure.reason(),
        &crate::SceneCompilationFailureKind::Component
    );
    assert_eq!(failure.location().unwrap().path().anchor, id("instance"));
    assert_eq!(
        failure.slot(),
        Some(crate::PrefabOverlaySlot::Component { index: 0 })
    );
}

#[test]
fn two_record_defaults_and_inline_references_share_exact_value_and_generated_id_allowances() {
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
        SceneCompilationLimits::default()
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
                effective_record(if name == "second" && !second_uses_default {
                    ComponentValue::map([("target", selected_socket())])
                } else {
                    empty()
                })
                .with_contextual_default(ComponentFieldId::new("target"), selected_socket()),
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
        assert_eq!(expanded(&report).entities().len(), 4);
        assert_eq!(projected(&report, "socket"), id("prefab_0"));
        for name in ["first", "second"] {
            assert_record(
                stored(&report, name),
                &effective_record(payload(&id("prefab_0"))),
            );
        }

        for (limited_nodes, limited_bytes) in [(nodes - 1, bytes), (nodes, bytes - 1)] {
            let report = expand(
                &registry,
                &document,
                &sources,
                limits(limited_nodes, limited_bytes, generated),
            );
            let diagnostic = rejected(&report, "scene.prefab-expansion-budget-exceeded");
            assert_eq!(
                field(diagnostic, "budget-kind"),
                DiagnosticValueRef::Identifier(if limited_nodes < nodes {
                    "materialized-value-nodes"
                } else {
                    "materialized-value-bytes"
                }),
            );
            if limited_nodes < nodes {
                assert_eq!(
                    field(diagnostic, "observed"),
                    DiagnosticValueRef::Unsigned(nodes as u64)
                );
                assert_eq!(
                    field(diagnostic, "maximum"),
                    DiagnosticValueRef::Unsigned(limited_nodes as u64)
                );
            } else {
                // Exact byte growth is admitted only after final IDs have been allocated.
                assert_eq!(
                    field(diagnostic, "observed"),
                    DiagnosticValueRef::Unsigned(bytes as u64)
                );
                assert_eq!(
                    field(diagnostic, "maximum"),
                    DiagnosticValueRef::Unsigned(limited_bytes as u64)
                );
            }
            let failure = report.failure.as_ref().unwrap();
            assert_eq!(
                failure.reason(),
                &crate::SceneCompilationFailureKind::WorkLimit
            );
            assert!(failure.location().is_none());
            assert!(failure.slot().is_none());
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
    assert_eq!(expanded(&report).entities().len(), 5);
    assert_record(
        stored(&report, "child"),
        &effective_record(payload(&projected(&report, "socket"))),
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
    let limits = SceneCompilationLimits::default()
        .with_materialized_components(ItemLimit::new(1).unwrap())
        .with_materialized_value_nodes(ItemLimit::new(final_value.nodes()).unwrap())
        .with_materialized_value_bytes(ByteLimit::new(bytes).unwrap())
        .with_generated_identifier_bytes(ByteLimit::new(generated).unwrap());
    let report = expand(&registry, &scene(overlay.clone()), &fixture(), limits);
    assert!(entity(&report, "host").components().is_empty());
    assert_record(
        stored(&report, "socket"),
        &effective_record(payload(&projected(&report, "socket"))),
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
        DiagnosticValueRef::Unsigned((final_value.nodes() * 2) as u64)
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
        DiagnosticValueRef::Unsigned((2 * bytes) as u64)
    );
    assert_eq!(
        field(diagnostic, "maximum"),
        DiagnosticValueRef::Unsigned(bytes as u64)
    );
}
