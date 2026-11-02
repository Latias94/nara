use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::Component;
use nara_identity::{AuthoredEntityReference, EntityReference, ReferenceInputId};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchemaCatalog,
    ComponentSchemaOwnerId, ComponentSchemaOwnerSnapshot, ComponentValueKind, ComponentValueShape,
};

use super::*;
use crate::{
    PrefabInstance, PrefabProjectionPath, SceneComponentBindings, SceneComponentRecord,
    SceneEntityProvenance, SceneEntityRecord,
};

#[derive(Component)]
struct Counted;

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.CompiledLink")
}

fn schema(default: Option<ComponentValue>) -> ComponentSchema {
    let field = match default {
        Some(value) => ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::optional(ComponentValueShape::EntityRef),
            value,
        ),
        None => ComponentFieldSchema::required(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::optional(ComponentValueShape::EntityRef),
        ),
    }
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    ComponentSchema::new(component(), "Compiled link", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field])
}

fn registry(
    schema: ComponentSchema,
    owner: Option<&'static str>,
) -> (ComponentRegistrySnapshot, Arc<AtomicUsize>) {
    let mut builder = match owner {
        Some(owner) => ComponentRegistryBuilder::from_owner_catalog_candidate(
            ComponentSchemaOwnerId::new(owner),
            ComponentSchemaCatalog::default(),
            None,
        )
        .unwrap(),
        None => ComponentRegistryBuilder::new(),
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let decode_calls = Arc::clone(&calls);
    builder
        .register_persistent_component_with_codec::<Counted, _, _>(
            schema,
            move |_| {
                decode_calls.fetch_add(1, Ordering::SeqCst);
                Ok(Counted)
            },
            |_| Ok(value(ComponentValue::Null)),
        )
        .unwrap();
    (builder.freeze().unwrap(), calls)
}

fn value(target: ComponentValue) -> ComponentValue {
    ComponentValue::map([("target", target)])
}

fn entity(name: &str, target: ComponentValue) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name)).with_component(
        component(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value(target)),
    )
}

fn provenance(document: &SceneDocument) -> SceneEntityProvenanceMap {
    document
        .entities
        .iter()
        .map(|entity| {
            (
                entity.id.clone(),
                SceneEntityProvenance::local(entity.id.clone()),
            )
        })
        .collect()
}

fn flat_claims(
    document: SceneDocument,
    origins: SceneEntityProvenanceMap,
) -> claims::FlatSceneClaims {
    claims::FlatSceneClaims {
        roots: document.roots,
        canonical_provenance: origins,
        entities: document
            .entities
            .into_iter()
            .map(|entity| {
                assert!(entity.prefab.is_none());
                claims::FlatSceneEntityClaim {
                    id: entity.id,
                    children: entity.children,
                    components: entity
                        .components
                        .into_iter()
                        .map(|(id, record)| {
                            assert!(record.contextual_defaults.is_empty());
                            (
                                id,
                                claims::FlatSceneComponentClaim {
                                    schema_owner: record.owner,
                                    version: record.version,
                                    value: record.value,
                                },
                            )
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}

fn seal(
    document: SceneDocument,
    registry: &ComponentRegistrySnapshot,
) -> Result<CompiledSceneDocument, CompiledSceneAdmissionError<Infallible>> {
    let origins = provenance(&document);
    seal_with_provenance(document, origins, registry)
}

fn seal_with_provenance(
    document: SceneDocument,
    origins: SceneEntityProvenanceMap,
    registry: &ComponentRegistrySnapshot,
) -> Result<CompiledSceneDocument, CompiledSceneAdmissionError<Infallible>> {
    admit_draft(
        document,
        origins,
        SchemaAuthority::Native(registry),
        ComponentValueMaterializationLimits::default(),
        None,
        |_, _, _| Ok(()),
    )
}

fn prefab_key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn source_step(lookup: &str, source: &SourceDocumentKey, entity: &str) -> PrefabProjectionStep {
    PrefabProjectionStep::SourceEntity {
        source_document: nara_asset::AssetRef::path(lookup).unwrap(),
        canonical_source: source.clone(),
        entity: id(entity),
    }
}

fn projected(
    anchor: &str,
    steps: impl IntoIterator<Item = PrefabProjectionStep>,
) -> SceneEntityProvenance {
    let steps = steps.into_iter().collect::<Vec<_>>();
    SceneEntityProvenance::projected(
        steps.last().unwrap().entity_id().clone(),
        PrefabProjectionPath::from_resolved_steps(id(anchor), steps),
    )
}

fn wide_optional_fixture() -> (SceneDocument, ComponentRegistrySnapshot, Arc<AtomicUsize>) {
    let fields = (0..1024).map(|index| {
        let name = format!("field_{index}");
        ComponentFieldSchema::optional(
            ComponentFieldId::new(name.clone()),
            name.clone(),
            ComponentFieldPath::from_fields([name]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    });
    let schema = ComponentSchema::new(component(), "Optional fields", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(fields);
    let (registry, calls) = registry(schema, None);
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("a"))
        .with_component(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::Map(BTreeMap::new()),
            ),
        )]);
    (document, registry, calls)
}

fn diagnostics<E: std::fmt::Debug>(
    result: Result<CompiledSceneDocument, CompiledSceneAdmissionError<E>>,
) -> DiagnosticReport {
    match result {
        Err(CompiledSceneAdmissionError::Diagnostics(report)) => report,
        other => panic!("expected document rejection, got {other:?}"),
    }
}

fn has_code(report: &DiagnosticReport, code: &str) -> bool {
    report
        .iter()
        .any(|diagnostic| diagnostic.code().as_str() == code)
}

fn authored() -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::input(ReferenceInputId::new("required").unwrap()),
    })
}

#[test]
fn admission_moves_payloads_and_preserves_order_canonical_identity_and_selected_owner() {
    let schema = schema(None).with_fields([ComponentFieldSchema::optional(
        ComponentFieldId::new("retained"),
        "Retained",
        ComponentFieldPath::from_fields(["retained"]),
        ComponentValueKind::String,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let (registry, calls) = registry(schema, Some("game.compiled-owner"));
    let mut root = entity("root", ComponentValue::Null).with_children([id("z"), id("a")]);
    let ComponentValue::Map(fields) = &mut root.components.get_mut(&component()).unwrap().value
    else {
        unreachable!()
    };
    fields.insert(
        "retained".into(),
        ComponentValue::String("owned payload".into()),
    );
    let ComponentValue::String(payload) = &fields["retained"] else {
        unreachable!()
    };
    let payload_pointer = payload.as_ptr();
    let document = SceneDocument {
        roots: vec![id("root"), id("other")],
        entities: vec![
            entity("z", ComponentValue::Null),
            root,
            entity("other", ComponentValue::Null),
            entity("a", ComponentValue::Null),
        ],
    };
    let expected_provenance = provenance(&document);
    let compiled = seal(document, &registry).unwrap();
    assert_eq!(compiled.roots(), [id("root"), id("other")]);
    assert_eq!(
        compiled
            .entities()
            .iter()
            .map(|entity| entity.id().as_str())
            .collect::<Vec<_>>(),
        ["a", "other", "root", "z"],
    );
    assert_eq!(compiled.entities()[2].children(), [id("z"), id("a")]);
    assert_eq!(compiled.canonical_provenance(), &expected_provenance);
    let record = &compiled.entities()[2].components()[&component()];
    assert_eq!(record.version(), ComponentSchemaVersion::ONE);
    assert_eq!(
        record.schema_owner().unwrap().as_str(),
        "game.compiled-owner",
    );
    let ComponentValue::Map(fields) = record.value() else {
        unreachable!()
    };
    let ComponentValue::String(payload) = &fields["retained"] else {
        unreachable!()
    };
    assert_eq!(payload.as_ptr(), payload_pointer);

    let source = SourceDocumentKey::named(
        "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
        "compiled.scene",
        crate::SourceDocumentKind::Scene,
    )
    .unwrap();
    let candidate = compiled.bind_source(source.clone());
    assert_eq!(candidate.source_key(), &source);
    assert_eq!(candidate.document().roots(), [id("root"), id("other")]);
    assert_eq!(candidate.canonical_provenance(), &expected_provenance);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn later_authored_leaves_in_declared_and_retained_values_refuse_the_complete_batch() {
    let (registry, calls) = registry(schema(None), None);
    for later_value in [
        value(authored()),
        ComponentValue::map([
            ("target", ComponentValue::Null),
            ("retained", ComponentValue::List(vec![authored()])),
        ]),
    ] {
        let mut later = entity("z", ComponentValue::Null);
        later.components.get_mut(&component()).unwrap().value = later_value;
        let document =
            SceneDocument::from_ordered_roots([entity("a", ComponentValue::Null), later]);
        let report = diagnostics(seal(document, &registry));
        assert!(
            has_code(&report, "scene.invalid-component-payload"),
            "{report:?}",
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn declared_local_references_must_close_over_the_complete_document() {
    let (registry, calls) = registry(schema(None), None);
    let document = SceneDocument::from_ordered_roots([entity(
        "a",
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("b") }),
    )]);
    let report = diagnostics(seal(document.clone(), &registry));
    assert!(
        has_code(&report, "scene.local-reference-invalid"),
        "{report:?}",
    );

    let mut complete = document;
    complete.entities.push(entity("b", ComponentValue::Null));
    complete.roots.push(id("b"));
    assert!(seal(complete, &registry).is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn hierarchy_and_canonical_provenance_are_both_required() {
    let (registry, calls) = registry(schema(None), None);
    let dangling = SceneDocument::from_ordered_roots([
        entity("a", ComponentValue::Null).with_children([id("missing")])
    ]);
    let report = diagnostics(seal(dangling, &registry));
    assert!(has_code(&report, "scene.dangling-child"), "{report:?}");
    let cycle = SceneDocument::new(
        Vec::<SceneEntityId>::new(),
        [
            entity("a", ComponentValue::Null).with_children([id("b")]),
            entity("b", ComponentValue::Null).with_children([id("a")]),
        ],
    );
    let report = diagnostics(seal(cycle, &registry));
    assert!(has_code(&report, "scene.placement-cycle"), "{report:?}");
    let document = SceneDocument::from_ordered_roots([entity("a", ComponentValue::Null)]);
    let report = diagnostics(admit_draft(
        document,
        SceneEntityProvenanceMap::new(),
        SchemaAuthority::Native(&registry),
        ComponentValueMaterializationLimits::default(),
        None,
        |_, _, _| Ok::<_, Infallible>(()),
    ));
    assert!(has_code(&report, "scene.invalid-provenance"), "{report:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn source_declarations_cannot_be_silently_discarded_by_the_sealer() {
    let (registry, calls) = registry(schema(Some(ComponentValue::Null)), None);
    let mut with_prefab = entity("a", ComponentValue::Null);
    with_prefab.prefab = Some(PrefabInstance {
        source: nara_asset::AssetRef::path("prefabs/source.prefab.ron").unwrap(),
        arguments: None,
        overrides: None,
    });
    let report = diagnostics(seal(
        SceneDocument::from_ordered_roots([with_prefab]),
        &registry,
    ));
    assert!(
        has_code(&report, "scene.prefab-instance-unsupported"),
        "{report:?}",
    );

    let mut contextual = entity("a", ComponentValue::Null);
    contextual
        .components
        .get_mut(&component())
        .unwrap()
        .contextual_defaults
        .insert(ComponentFieldId::new("target"), authored());
    let report = diagnostics(seal(
        SceneDocument::from_ordered_roots([contextual]),
        &registry,
    ));
    assert!(
        has_code(&report, "scene.invalid-component-payload"),
        "{report:?}",
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn implicit_defaults_are_refused_while_explicit_null_is_retained() {
    let (registry, calls) = registry(schema(Some(ComponentValue::Null)), None);
    let mut implicit = entity("a", ComponentValue::Null);
    implicit.components.get_mut(&component()).unwrap().value = ComponentValue::Map(BTreeMap::new());
    let report = diagnostics(seal(
        SceneDocument::from_ordered_roots([implicit]),
        &registry,
    ));
    assert!(
        has_code(&report, "scene.compiled-incomplete-value"),
        "{report:?}",
    );
    let explicit = seal(
        SceneDocument::from_ordered_roots([entity("a", ComponentValue::Null)]),
        &registry,
    )
    .unwrap();
    assert_eq!(
        explicit.entities()[0].components()[&component()].value(),
        &value(ComponentValue::Null),
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn current_definitions_and_source_owner_claims_are_checked_before_stamping() {
    let (registry, calls) = registry(schema(None), Some("game.selected"));
    let mut claimed = entity("a", ComponentValue::Null);
    claimed.components.get_mut(&component()).unwrap().owner =
        Some(PersistentComponentSchemaOwnerId::try_new("game.other").unwrap());
    let report = diagnostics(seal(
        SceneDocument::from_ordered_roots([claimed]),
        &registry,
    ));
    assert!(
        has_code(&report, "scene.component-owner-mismatch"),
        "{report:?}",
    );
    let mut stale = entity("a", ComponentValue::Null);
    stale.components.get_mut(&component()).unwrap().version = ComponentSchemaVersion(2);
    assert!(diagnostics(seal(SceneDocument::from_ordered_roots([stale]), &registry)).has_errors(),);
    let mut unknown = entity("a", ComponentValue::Null);
    let record = unknown.components.remove(&component()).unwrap();
    unknown
        .components
        .insert(ComponentTypeId::new("game.Missing"), record);
    let report = diagnostics(seal(
        SceneDocument::from_ordered_roots([unknown]),
        &registry,
    ));
    assert!(has_code(&report, "scene.unknown-component"), "{report:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn unowned_native_definitions_remain_actually_unowned() {
    let (registry, calls) = registry(schema(None), None);
    let compiled = seal(
        SceneDocument::from_ordered_roots([entity("a", ComponentValue::Null)]),
        &registry,
    )
    .unwrap();
    assert!(
        compiled.entities()[0].components()[&component()]
            .schema_owner()
            .is_none(),
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn mixed_definitions_require_the_data_owner_and_flat_data_values() {
    let (native, calls) = registry(schema(None), Some("game.native"));
    let data_component = ComponentTypeId::new("game.DataLink");
    let mut data_schema = schema(None);
    data_schema.id = data_component.clone();
    let owner = PersistentComponentSchemaOwnerId::try_new("game.data").unwrap();
    let data_owner = ComponentSchemaOwnerSnapshot::admit(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![data_schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let bindings = SceneComponentBindings::new(native, [data_owner]).unwrap();
    let make_document = |target, claim| {
        SceneDocument::from_ordered_roots([
            entity("a", ComponentValue::Null),
            SceneEntityRecord::new(id("z")).with_component(
                data_component.clone(),
                SceneComponentRecord {
                    owner: claim,
                    version: ComponentSchemaVersion::ONE,
                    value: value(target),
                    contextual_defaults: BTreeMap::new(),
                },
            ),
        ])
    };
    for (target, claim, accepted) in [
        (ComponentValue::Null, Some(owner.clone()), true),
        (ComponentValue::Null, None, false),
        (authored(), Some(owner.clone()), false),
    ] {
        let document = make_document(target, claim);
        let origins = provenance(&document);
        let result = admit_draft(
            document,
            origins,
            SchemaAuthority::Mixed(&bindings),
            ComponentValueMaterializationLimits::default(),
            None,
            |_, _, _| Ok::<_, Infallible>(()),
        );
        if accepted {
            let compiled = result.unwrap();
            assert_eq!(
                compiled.entities()[1].components()[&data_component].schema_owner(),
                Some(&owner),
            );
        } else {
            assert!(diagnostics(result).has_errors());
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn aggregate_value_limits_are_inclusive_and_exclude_identity_metadata() {
    let (registry, calls) = registry(schema(None), None);
    let document = SceneDocument::from_ordered_roots([
        entity("a-long-source-identity", ComponentValue::Null),
        entity("z-another-long-source-identity", ComponentValue::Null),
    ]);
    // Two Maps, each with the six-byte key "target" and one zero-byte Null leaf.
    for (nodes, bytes, accepted) in [(4, 12, true), (3, 12, false), (4, 11, false)] {
        let result = admit_draft(
            document.clone(),
            provenance(&document),
            SchemaAuthority::Native(&registry),
            ComponentValueMaterializationLimits::new(
                ItemLimit::new(nodes).unwrap(),
                ByteLimit::new(bytes).unwrap(),
                DepthLimit::new(4).unwrap(),
            ),
            None,
            |_, _, _| Ok::<_, Infallible>(()),
        );
        assert_eq!(result.is_ok(), accepted, "{result:?}");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn working_admission_preserves_exact_refusals_at_the_inclusive_boundary() {
    let (registry, calls) = registry(schema(Some(ComponentValue::Null)), Some("game.selected"));
    let document = SceneDocument::from_ordered_roots([entity("a", ComponentValue::Null)]);
    let mut required = 0usize;
    admit_draft(
        document.clone(),
        provenance(&document),
        SchemaAuthority::Native(&registry),
        ComponentValueMaterializationLimits::default(),
        None,
        |_, _, bytes| {
            required = required.checked_add(bytes.unwrap()).unwrap();
            Ok::<_, Infallible>(())
        },
    )
    .unwrap();
    assert!(required > 0);
    for maximum in [required, required - 1] {
        let mut used = 0usize;
        let refusal = "owner allowance refused";
        let result = admit_draft(
            document.clone(),
            provenance(&document),
            SchemaAuthority::Native(&registry),
            ComponentValueMaterializationLimits::default(),
            None,
            |_, _, bytes| {
                let next = bytes
                    .and_then(|bytes| used.checked_add(bytes))
                    .ok_or(refusal)?;
                if next > maximum {
                    return Err(refusal);
                }
                used = next;
                Ok(())
            },
        );
        if maximum == required {
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(used, required);
        } else {
            assert!(matches!(
                result,
                Err(CompiledSceneAdmissionError::Admission(error)) if error == refusal,
            ));
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn duplicate_canonical_addresses_refuse_identical_and_aliased_lookups() {
    let (registry, calls) = registry(schema(None), None);
    let key = prefab_key("source");
    for lookup in ["prefabs/source.ron", "aliases/source.ron"] {
        let document = SceneDocument::new(
            [id("anchor")],
            [
                SceneEntityRecord::new(id("anchor")).with_children([id("p0"), id("p1")]),
                SceneEntityRecord::new(id("p0")),
                SceneEntityRecord::new(id("p1")),
            ],
        );
        let mut origins = provenance(&document);
        origins.insert(
            id("p0"),
            projected(
                "anchor",
                [source_step("prefabs/source.ron", &key, "shared")],
            ),
        );
        origins.insert(
            id("p1"),
            projected("anchor", [source_step(lookup, &key, "shared")]),
        );
        assert_eq!(
            CanonicalSceneEntityAddress::new(&origins[&id("p0")]),
            CanonicalSceneEntityAddress::new(&origins[&id("p1")]),
        );
        let report = diagnostics(seal_with_provenance(document, origins, &registry));
        assert!(
            has_code(&report, "scene.compiled-duplicate-logical-address"),
            "{report:?}",
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn distinct_repeated_nested_sources_anchors_and_step_kinds_are_admitted() {
    let (registry, calls) = registry(schema(None), None);
    let outer = prefab_key("outer");
    let inner = prefab_key("inner");
    let source = |key: &SourceDocumentKey, entity| source_step("prefabs/source.ron", key, entity);
    let origins = [
        (id("left"), SceneEntityProvenance::local(id("left"))),
        (id("right"), SceneEntityProvenance::local(id("right"))),
        (id("shared"), SceneEntityProvenance::local(id("shared"))),
        (id("p0"), projected("left", [source(&outer, "shared")])),
        (id("p1"), projected("right", [source(&outer, "shared")])),
        (id("p2"), projected("left", [source(&inner, "shared")])),
        (
            id("p3"),
            projected(
                "left",
                [source(&outer, "nested-a"), source(&inner, "shared")],
            ),
        ),
        (
            id("p4"),
            projected(
                "left",
                [source(&outer, "nested-b"), source(&inner, "shared")],
            ),
        ),
        (
            id("p5"),
            projected(
                "left",
                [PrefabProjectionStep::InstanceAddition {
                    entity: id("shared"),
                }],
            ),
        ),
        (
            id("p6"),
            projected(
                "left",
                [
                    source(&outer, "nested-a"),
                    PrefabProjectionStep::InstanceAddition {
                        entity: id("shared"),
                    },
                ],
            ),
        ),
        (
            id("p7"),
            projected("left", [source(&outer, "shared"), source(&inner, "shared")]),
        ),
    ]
    .into_iter()
    .collect::<SceneEntityProvenanceMap>();
    let document =
        SceneDocument::from_ordered_roots(origins.keys().cloned().map(SceneEntityRecord::new));
    let compiled = seal_with_provenance(document, origins.clone(), &registry).unwrap();
    assert_eq!(compiled.canonical_provenance(), &origins);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn canonical_address_order_uses_exact_source_keys_and_terminal_ids() {
    let first = prefab_key("first");
    let second = prefab_key("second");
    let left = projected("anchor", [source_step("z.ron", &first, "shared")]);
    let right = projected("anchor", [source_step("a.ron", &second, "shared")]);
    assert_eq!(
        CanonicalSceneEntityAddress::new(&left).cmp(&CanonicalSceneEntityAddress::new(&right)),
        first.cmp(&second),
    );
    let terminal =
        SceneEntityProvenance::projected(id("terminal"), left.projection_path().unwrap().clone());
    assert_eq!(
        CanonicalSceneEntityAddress::new(&left).cmp(&CanonicalSceneEntityAddress::new(&terminal)),
        id("shared").cmp(&id("terminal")),
    );
    let local = SceneEntityProvenance::local(id("shared"));
    assert!(CanonicalSceneEntityAddress::new(&local) < CanonicalSceneEntityAddress::new(&left));
}

#[test]
fn canonical_provenance_requires_valid_prefab_keys_at_every_source_step() {
    let (registry, calls) = registry(schema(None), None);
    let valid = prefab_key("valid");
    let invalid = [
        SourceDocumentKey::named(
            "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
            "scene-source",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
        SourceDocumentKey::stable_asset(
            "00000000-0000-0000-0000-000000000000".parse().unwrap(),
            SourceDocumentKind::Prefab,
        ),
    ];
    for invalid in invalid {
        for keys in [[&invalid, &valid], [&valid, &invalid]] {
            let document = SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(id("anchor")),
                SceneEntityRecord::new(id("projected")),
            ]);
            let mut origins = provenance(&document);
            origins.insert(
                id("projected"),
                projected(
                    "anchor",
                    [
                        source_step("outer.ron", keys[0], "nested"),
                        source_step("inner.ron", keys[1], "shared"),
                    ],
                ),
            );
            let report = diagnostics(seal_with_provenance(document, origins, &registry));
            assert!(has_code(&report, "scene.invalid-provenance"), "{report:?}");
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn canonical_provenance_keeps_associated_document_keys_for_primary_products() {
    use nara_asset::{AssetProductRef, AssetRef, AssetSourcePath, ImportedProductId};

    let (registry, calls) = registry(schema(None), None);
    let source = "00000000-0000-4000-8000-000000000041".parse().unwrap();
    let other = "00000000-0000-4000-8000-000000000042".parse().unwrap();
    let key = SourceDocumentKey::stable_asset(source, SourceDocumentKind::Prefab);
    let other_key = SourceDocumentKey::stable_asset(other, SourceDocumentKind::Prefab);
    let named = prefab_key("associated-source");
    let location = SourceDocumentKey::project_location(
        "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
        AssetSourcePath::new("prefabs/source.ron").unwrap(),
        SourceDocumentKind::Prefab,
    );
    let auxiliary =
        ImportedProductId::parse_subproduct("00000000-0000-4000-8000-000000000043").unwrap();
    for (key, product, accepted) in [
        (key.clone(), ImportedProductId::PRIMARY, true),
        (named.clone(), ImportedProductId::PRIMARY, true),
        (location.clone(), ImportedProductId::PRIMARY, true),
        (other_key, ImportedProductId::PRIMARY, false),
        (key, auxiliary, false),
        (named, auxiliary, false),
        (location, auxiliary, false),
    ] {
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("anchor")),
            SceneEntityRecord::new(id("projected")),
        ]);
        let mut origins = provenance(&document);
        origins.insert(
            id("projected"),
            projected(
                "anchor",
                [PrefabProjectionStep::SourceEntity {
                    source_document: AssetRef::Product(
                        AssetProductRef::new(source, product).unwrap(),
                    ),
                    canonical_source: key,
                    entity: id("shared"),
                }],
            ),
        );
        let expected = origins.clone();
        let result = seal_with_provenance(document, origins, &registry);
        if accepted {
            let compiled = result.unwrap();
            assert_eq!(compiled.canonical_provenance(), &expected);
        } else {
            let report = diagnostics(result);
            assert!(has_code(&report, "scene.invalid-provenance"), "{report:?}");
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn canonical_address_index_admission_precedes_duplicate_detection() {
    let (registry, calls) = registry(schema(None), None);
    let key = prefab_key("source");
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("anchor")),
        SceneEntityRecord::new(id("p0")),
        SceneEntityRecord::new(id("p1")),
    ]);
    let mut origins = provenance(&document);
    for entity in ["p0", "p1"] {
        origins.insert(
            id(entity),
            projected("anchor", [source_step("source.ron", &key, "shared")]),
        );
    }
    let index_bytes =
        origins.len() * (size_of::<CanonicalSceneEntityAddress<'_>>() + 6 * size_of::<usize>());
    let refusal = "canonical index refused";
    let mut requests = 0;
    let result = admit_claims(
        flat_claims(document, origins),
        SchemaAuthority::Native(&registry),
        ComponentValueMaterializationLimits::default(),
        None,
        false,
        |_, _, bytes| {
            requests += 1;
            if requests == 2 {
                assert_eq!(bytes, Some(index_bytes));
                Err(refusal)
            } else {
                Ok(())
            }
        },
    );
    assert!(matches!(
        result,
        Err(CompiledSceneAdmissionError::Admission(error)) if error == refusal,
    ));
    assert_eq!(requests, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn wide_optional_schema_requests_storage_before_reference_traversal() {
    let (document, registry, calls) = wide_optional_fixture();
    let origins = provenance(&document);
    let limits = ComponentValueMaterializationLimits::default();
    let claims = flat_claims(document, origins);
    let mut prefix_bytes = 0usize;
    {
        let mut admit_prefix = |bytes: Option<usize>| {
            prefix_bytes = prefix_bytes.checked_add(bytes.unwrap()).unwrap();
            Ok::<_, Infallible>(())
        };
        admit_validation_scratch(&claims, &mut admit_prefix).unwrap();
        admit_canonical_provenance(&claims, &claims.canonical_provenance, &mut admit_prefix)
            .unwrap();
    }
    let mut raw_bytes = 0usize;
    claims.entities[0].components[&component()]
        .value
        .cost_up_to_with_admission(limits.nodes().get(), limits.bytes().get(), |bytes| {
            raw_bytes = raw_bytes.checked_add(bytes.unwrap()).unwrap();
            Ok::<_, Infallible>(())
        })
        .unwrap()
        .unwrap();
    nara_reflect::__private::validate_native_component_value_references_with_admission(
        &claims.entities[0].components[&component()].value,
        limits,
        |_, _, allocated| {
            raw_bytes = raw_bytes.checked_add(allocated).unwrap();
            Ok::<_, Infallible>(())
        },
    )
    .unwrap();
    let maximum = prefix_bytes.checked_add(raw_bytes).unwrap();
    let refusal = "schema validation refused";
    let mut used = 0usize;
    let mut denied = None;
    let mut observations_after_raw = 0usize;
    let result = admit_claims(
        claims,
        SchemaAuthority::Native(&registry),
        limits,
        None,
        false,
        |_, _, bytes| {
            let bytes = bytes.ok_or(refusal)?;
            let next = used.checked_add(bytes).ok_or(refusal)?;
            if next > maximum {
                denied = Some(bytes);
                return Err(refusal);
            }
            if used > prefix_bytes && bytes == 0 {
                observations_after_raw += 1;
            }
            used = next;
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(CompiledSceneAdmissionError::Admission(error)) if error == refusal,
    ));
    assert_eq!(used, maximum);
    // Allow the complete raw walker, then refuse the Schema index before reference traversal.
    // Its ordinary default planner must have observed the 1024 optional fields first; the old
    // unaccounted value validator reached reference work without those admission observations.
    assert!(observations_after_raw >= 1024);
    assert!(denied.unwrap() >= 1024 * size_of::<usize>());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn wide_optional_schema_with_an_empty_explicit_value_seals_without_codecs() {
    let (document, registry, calls) = wide_optional_fixture();
    let compiled = seal(document, &registry).unwrap();
    assert_eq!(
        compiled.entities()[0].components()[&component()].value(),
        &ComponentValue::Map(BTreeMap::new()),
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn terminal_admission_preserves_reference_work_independently_of_allocated_bytes() {
    let (registry, calls) = registry(schema(None), None);
    let document = SceneDocument::from_ordered_roots([
        entity(
            "a",
            ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("b") }),
        ),
        SceneEntityRecord::new(id("b")),
    ]);
    let mut reference_nodes = 0usize;
    let mut reference_bytes = 0usize;
    admit_draft(
        document.clone(),
        provenance(&document),
        SchemaAuthority::Native(&registry),
        ComponentValueMaterializationLimits::default(),
        None,
        |nodes, bytes, allocation| {
            assert!(allocation.is_some());
            reference_nodes = reference_nodes.checked_add(nodes).unwrap();
            reference_bytes = reference_bytes.checked_add(bytes).unwrap();
            Ok::<_, Infallible>(())
        },
    )
    .unwrap();
    assert!(reference_nodes > 0);
    assert!(reference_bytes > 0);
    for (maximum_nodes, maximum_bytes, accepted) in [
        (reference_nodes, reference_bytes, true),
        (reference_nodes - 1, reference_bytes, false),
        (reference_nodes, reference_bytes - 1, false),
    ] {
        let (mut nodes_used, mut bytes_used) = (0usize, 0usize);
        let result = admit_draft(
            document.clone(),
            provenance(&document),
            SchemaAuthority::Native(&registry),
            ComponentValueMaterializationLimits::default(),
            None,
            |nodes, bytes, allocation| {
                assert!(allocation.is_some());
                nodes_used += nodes;
                bytes_used += bytes;
                if nodes_used > maximum_nodes || bytes_used > maximum_bytes {
                    Err("reference work refused")
                } else {
                    Ok(())
                }
            },
        );
        if accepted {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert!(matches!(
                result,
                Err(CompiledSceneAdmissionError::Admission(
                    "reference work refused"
                ))
            ));
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
