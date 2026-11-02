use std::{collections::HashMap, convert::Infallible, sync::Arc};

use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaVersion, ComponentValue, ComponentValueKind,
    ComponentValueShape, EntityReference,
};

use super::*;
use crate::{
    AuthoringDocumentSource, PrefabAddition, PrefabAuthoringSession, PrefabDocument,
    PrefabDocumentCandidate, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceHop,
    PrefabOverlay, PrefabProjectionStep, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SourceDocumentKey, SourceDocumentKind,
    authoring::prefab_conversion::tests::{
        admit_schema, basis, component, id, instance, original, schema_owner,
    },
};

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "e758bd18-2f06-4736-9752-dcfe038c3289".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn child_placement(parent: &str, child: &str) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        PrefabEntityTarget::source(id(child)),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id(parent),
        ))),
    )
}

fn selected_input(name: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
                expected_base: key("base"),
            }],
            target: AuthoredReferenceTarget::Input(
                nara_identity::ReferenceInputId::new(name).unwrap(),
            ),
        },
    }
}

#[test]
fn query_inventory_is_finite_deduplicated_and_uses_understood_lexical_payloads() {
    let field = ComponentFieldId::new("target");
    let basis = admit_schema(
        ComponentSchema::new(component(), "Queries", ComponentSchemaVersion::ONE)
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
            .with_fields([ComponentFieldSchema::optional_with_default(
                field.clone(),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: id("anchor"),
                }),
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::EntityRef])]),
    );
    let payload = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::Null)]),
    )
    .with_owner(schema_owner())
    .with_contextual_default(
        field,
        ComponentValue::EntityReference(selected_input("hidden")),
    );
    let mut retained = SceneEntityRecord::new(id("retained")).with_component(component(), payload);
    let mut declaration = instance("unrelated");
    declaration.arguments = Some(crate::PrefabReferenceArguments {
        base: key("unrelated"),
        values: [
            (
                nara_identity::ReferenceInputId::new("dormant").unwrap(),
                selected_input("dormant"),
            ),
            (
                nara_identity::ReferenceInputId::new("duplicate").unwrap(),
                selected_input("hidden"),
            ),
            (
                nara_identity::ReferenceInputId::new("own").unwrap(),
                EntityReference::Authored {
                    reference: AuthoredEntityReference::input(
                        nara_identity::ReferenceInputId::new("own").unwrap(),
                    ),
                },
            ),
        ]
        .into(),
    });
    retained.prefab = Some(declaration);
    retained.components.insert(
        nara_reflect::ComponentTypeId::new("test.UnknownQuery"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "unknown",
                ComponentValue::EntityReference(selected_input("opaque")),
            )]),
        ),
    );
    let records = vec![retained];
    let limits = crate::validation::reference_traversal_limits(
        ComponentValueMaterializationLimits::default(),
    );
    let maximum = nara_core::ByteLimit::new(1024 * 1024).unwrap();
    let collected =
        source::collect_source_reference_queries(&records, &id("anchor"), &basis, limits, maximum)
            .unwrap();
    assert_eq!(collected.values.len(), 2);
    for name in ["hidden", "dormant"] {
        let EntityReference::Authored { reference } = selected_input(name) else {
            unreachable!()
        };
        assert!(collected.values.contains(&reference));
    }
    let exact = nara_core::ByteLimit::new(collected.work_bytes).unwrap();
    source::collect_source_reference_queries(&records, &id("anchor"), &basis, limits, exact)
        .unwrap();
    assert!(
        source::collect_source_reference_queries(
            &records,
            &id("anchor"),
            &basis,
            limits,
            nara_core::ByteLimit::new(collected.work_bytes - 1).unwrap()
        )
        .is_err()
    );
}

#[test]
fn query_inventory_accumulates_source_read_nodes_and_bytes_without_input_queries() {
    let basis = basis();
    let reference_target = "a".repeat(4096);
    let records = ["first", "other"].map(|name| {
        SceneEntityRecord::new(id(name)).with_component(component(), reference(&reference_target))
    });
    let limits = crate::validation::reference_traversal_limits(
        ComponentValueMaterializationLimits::default(),
    );
    let maximum = nara_core::ByteLimit::new(1024 * 1024).unwrap();
    let first = source::collect_source_reference_queries(
        &records[..1],
        &id("anchor"),
        &basis,
        limits,
        maximum,
    )
    .unwrap();
    let other = source::collect_source_reference_queries(
        &records[1..],
        &id("anchor"),
        &basis,
        limits,
        maximum,
    )
    .unwrap();
    assert!(first.values.is_empty() && other.values.is_empty());
    assert_eq!(first.reference_work, other.reference_work);
    assert!(first.reference_work.0 > 0 && first.reference_work.1 > 0);
    let combined =
        source::collect_source_reference_queries(&records, &id("anchor"), &basis, limits, maximum)
            .unwrap();
    assert_eq!(
        combined.reference_work,
        (first.reference_work.0 * 2, first.reference_work.1 * 2)
    );
    for bounded in [
        nara_reflect::EntityReferenceTraversalLimits::new(
            nara_core::ItemLimit::new(first.reference_work.0).unwrap(),
            limits.bytes(),
            limits.depth(),
        ),
        nara_reflect::EntityReferenceTraversalLimits::new(
            limits.nodes(),
            nara_core::ByteLimit::new(first.reference_work.1).unwrap(),
            limits.depth(),
        ),
    ] {
        for record in &records {
            source::collect_source_reference_queries(
                std::slice::from_ref(record),
                &id("anchor"),
                &basis,
                bounded,
                maximum,
            )
            .unwrap();
        }
        assert!(
            source::collect_source_reference_queries(
                &records,
                &id("anchor"),
                &basis,
                bounded,
                maximum
            )
            .is_err()
        );
    }
}

fn reference(target: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "target",
            ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(target) }),
        )]),
    )
    .with_owner(schema_owner())
}

fn source() -> SceneDocument {
    let mut document = original();
    let anchor = document
        .entities
        .iter_mut()
        .find(|record| record.id == id("anchor"))
        .unwrap();
    let mut overlay = PrefabOverlay::new(key("base"));
    let mut addition = PrefabAddition::new(id("leaf"));
    let mut link = reference("leaf");
    link.value = ComponentValue::map([(
        "target",
        ComponentValue::EntityReference(EntityReference::Authored {
            reference: AuthoredEntityReference {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
                    expected_base: key("base"),
                }],
                target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                    "root-a",
                ))),
            },
        }),
    )]);
    addition.components.insert(component(), link);
    overlay.additions.push(addition);
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(id("leaf")),
        ScenePlacement::last(ScenePlacementParent::Root),
    ));
    anchor.prefab.as_mut().unwrap().overrides = Some(overlay);
    document
}

struct Sources {
    basis: AuthoringSchemaBasis,
    documents: HashMap<AssetRef, (SourceDocumentKey, Arc<PrefabDocument>)>,
    reads: Vec<AssetRef>,
}

impl Sources {
    fn new(basis: AuthoringSchemaBasis) -> Self {
        let mut nested = crate::SceneEntityRecord::new(id("root-b"));
        nested.prefab = Some(instance("nested"));
        let base = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("root-a")),
                SceneEntityPlacement::root(id("root-b")),
                child_placement("root-a", "leaf"),
            ],
            [
                crate::SceneEntityRecord::new(id("root-a"))
                    .with_component(component(), reference("leaf")),
                crate::SceneEntityRecord::new(id("leaf"))
                    .with_component(component(), reference("root-a")),
                nested,
            ],
        );
        let mut unclaimed = reference("leaf");
        unclaimed.owner = None;
        let nested =
            PrefabDocument::from_ordered_roots([
                crate::SceneEntityRecord::new(id("leaf")).with_component(component(), unclaimed)
            ]);
        Self {
            basis,
            documents: [
                (instance("base").source, (key("base"), Arc::new(base))),
                (instance("nested").source, (key("nested"), Arc::new(nested))),
            ]
            .into(),
            reads: Vec::new(),
        }
    }

    fn load(
        &mut self,
        request: &AssetRef,
        _: &mut SceneCompilationWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, Infallible> {
        self.reload(request)
    }

    fn reload(&mut self, request: &AssetRef) -> Result<Option<ResolvedPrefabSource>, Infallible> {
        self.reads.push(request.clone());
        Ok(self.documents.get(request).map(|(key, document)| {
            ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(document),
                self.basis.clone(),
                key.clone(),
            )
        }))
    }
}

fn open_scene(
    document: &SceneDocument,
    basis: &AuthoringSchemaBasis,
    limits: SceneAuthoringHistoryLimits,
) -> SceneAuthoringSession {
    let published = SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/localize.scene.json").unwrap()),
            basis,
        )
        .unwrap()
        .publish();
    SceneAuthoringSession::from_published_authoring_document_with_history_limits(published, limits)
}

fn mapped<'a>(
    mappings: &'a [LocalizedPrefabEntity],
    local: &str,
    steps: usize,
    addition: bool,
) -> &'a SceneEntityId {
    let mut found = mappings.iter().filter(|mapping| {
        let path = mapping.provenance().projection_path().unwrap();
        mapping.provenance().local_entity_id() == &id(local)
            && path.root_anchor() == &id("anchor")
            && path.steps().len() == steps
            && matches!(
                path.steps().last().unwrap(),
                PrefabProjectionStep::InstanceAddition { .. }
            ) == addition
    });
    let result = found.next().expect("typed mapped source").entity();
    assert!(
        found.next().is_none(),
        "typed source mapping must be unique"
    );
    result
}

fn target(document: &SceneDocument, entity: &SceneEntityId) -> SceneEntityId {
    let record = document
        .entities
        .iter()
        .find(|record| &record.id == entity)
        .unwrap();
    assert!(record.prefab.is_none());
    assert_eq!(
        record.components[&component()].owner(),
        Some(&schema_owner())
    );
    let ComponentValue::Map(fields) = &record.components[&component()].value else {
        panic!("complete component map")
    };
    let ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) = &fields["target"]
    else {
        panic!("declared reference must retain its type")
    };
    entity.clone()
}

#[test]
fn real_selected_composition_localizes_mixed_origins_and_replays_exact_source_data() {
    let basis = basis();
    let document = source();
    let mut session = open_scene(&document, &basis, Default::default());
    let before = session.shared_document();
    let before_state = session.content_state();
    let mut sources = Sources::new(basis.clone());
    let prepared = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap();
    assert_eq!(
        sources.reads,
        [instance("base").source, instance("nested").source]
    );
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.history_usage().records, 0);
    assert_eq!(prepared.entities().len(), 5);
    let root_a = mapped(&prepared.entities(), "root-a", 1, false).clone();
    let root_b = mapped(&prepared.entities(), "root-b", 1, false).clone();
    let leaf = mapped(&prepared.entities(), "leaf", 1, false).clone();
    let nested = mapped(&prepared.entities(), "leaf", 2, false).clone();
    let addition = mapped(&prepared.entities(), "leaf", 1, true).clone();
    let ids = [&root_a, &root_b, &leaf, &nested, &addition]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 5);
    assert!(
        ids.iter()
            .all(|entity| !document.entities.iter().any(|old| &old.id == *entity))
    );
    let reviewed = prepared.document().clone();
    assert_eq!(target(&reviewed, &root_a), leaf);
    assert_eq!(target(&reviewed, &leaf), root_a);
    assert_eq!(target(&reviewed, &nested), nested);
    assert_eq!(target(&reviewed, &addition), root_a);
    for old in &document.entities {
        let retained = reviewed
            .entities
            .iter()
            .find(|record| record.id == old.id)
            .unwrap();
        if old.id == id("anchor") {
            assert_eq!(retained.components, old.components);
            assert!(retained.prefab.is_none());
            for entity in [&root_a, &root_b, &addition] {
                assert!(reviewed.placements.iter().any(|entry| {
                    entry.target == PrefabEntityTarget::source(entity.clone())
                        && entry.placement.parent
                            == ScenePlacementParent::Entity(PrefabEntityTarget::source(id(
                                "anchor",
                            )))
                }));
            }
        } else {
            assert_eq!(retained, old);
        }
    }
    let report = session
        .commit_complete_prefab_conversion_with_loader(prepared, |request| sources.reload(request))
        .unwrap();
    assert!(report.report().applied, "{:?}", report.report().diagnostics);
    assert_eq!(session.history_status().undo_depth, 1);
    assert_eq!(session.document(), &reviewed);
    let rebuilt = open_scene(&reviewed, &basis, Default::default());
    assert_classification_matches(&session, &rebuilt);
    sources.documents.clear();
    let reads = sources.reads.clone();
    let undo = session.undo_authoring();
    assert!(undo.applied, "{:?}", undo.diagnostics);
    assert_eq!(session.document(), &document);
    assert_eq!(session.content_state(), before_state);
    let redo = session.redo_authoring();
    assert!(redo.applied, "{:?}", redo.diagnostics);
    assert_eq!(session.document(), &reviewed);
    assert_eq!(sources.reads, reads, "replay has no dependency loader");
    for encoded in [
        reviewed.to_json_string().unwrap(),
        reviewed.to_ron_string().unwrap(),
    ] {
        let candidate = if encoded.starts_with('{') {
            SceneDocumentCandidate::decode_json_str(&encoded)
        } else {
            SceneDocumentCandidate::decode_ron_str(&encoded)
        }
        .unwrap();
        let published = candidate
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/localize.scene.json").unwrap()),
                &basis,
            )
            .unwrap()
            .publish();
        assert_eq!(published.document(), &reviewed);
    }
}

#[test]
fn nominal_prefab_conversion_cancellation_and_source_reopen_keep_owner_affinity() {
    let basis = basis();
    let scene = source();
    let original = PrefabDocument::new(scene.placements, scene.entities);
    let published = PrefabDocumentCandidate::decode_json_str(&original.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("prefabs/localize.prefab.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let before = session.shared_document();
    let revision = session.revision();
    let mut sources = Sources::new(basis.clone());
    let cancelled = session
        .request_complete_prefab_conversion(revision, &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap();
    drop(cancelled);
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_usage().records, 0);
    let prepared = session
        .request_complete_prefab_conversion(revision, &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap();
    let reviewed = prepared.document().clone();
    assert_eq!(prepared.entities().len(), 5);
    let report = session
        .commit_complete_prefab_conversion_with_loader(prepared, |request| sources.reload(request))
        .unwrap();
    assert!(report.report().applied, "{:?}", report.report().diagnostics);
    let undo = session.undo_authoring();
    assert!(undo.applied, "{:?}", undo.diagnostics);
    assert_eq!(session.document(), &original);
    let redo = session.redo_authoring();
    assert!(redo.applied, "{:?}", redo.diagnostics);
    assert_eq!(session.document(), &reviewed);
    let published = PrefabDocumentCandidate::decode_ron_str(&reviewed.to_ron_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("prefabs/localize.prefab.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    assert_eq!(published.document(), &reviewed);
}

#[test]
fn unavailable_selected_data_and_concrete_loader_failures_preserve_the_source() {
    let basis = basis();
    let mut session = open_scene(&source(), &basis, Default::default());
    let before = session.shared_document();
    let mut sources = Sources::new(basis);
    let (_, nested) = sources
        .documents
        .get_mut(&instance("nested").source)
        .unwrap();
    Arc::make_mut(nested).entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .version = ComponentSchemaVersion(2);
    let error = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap_err();
    let PrefabConversionError::Source(report) = error;
    assert!(!report.applied);
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    #[derive(Debug, PartialEq)]
    struct LoadFailure(u32);
    let error = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |_, _| {
                Err::<Option<ResolvedPrefabSource>, _>(LoadFailure(73))
            })
        })
        .unwrap_err();
    assert!(matches!(
        error,
        PrefabConversionError::Loader(LoadFailure(73))
    ));
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.history_usage().records, 0);
    let no_change = session.undo_authoring();
    assert!(!no_change.applied);
}

#[test]
fn source_copy_limits_and_stale_revision_refuse_before_publication() {
    let basis = basis();
    let defaults = SceneAuthoringHistoryLimits::default();
    let limits = SceneAuthoringHistoryLimits::new(
        defaults.max_records(),
        defaults.max_operations(),
        defaults.max_value_nodes(),
        defaults.max_logical_bytes(),
        defaults.max_entry_operations(),
        defaults.max_entry_value_nodes(),
        128,
    )
    .unwrap();
    let session = open_scene(&source(), &basis, limits);
    let before = session.shared_document();
    let mut sources = Sources::new(basis.clone());
    let error = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap_err();
    assert!(matches!(error, PrefabConversionError::Source(_)));
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.history_usage().records, 0);
    let other = open_scene(&source(), &basis, Default::default());
    sources.reads.clear();
    let error = session
        .request_complete_prefab_conversion(other.revision(), &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap_err();
    assert!(matches!(error, PrefabConversionError::Source(_)));
    assert!(sources.reads.is_empty());
}

#[test]
fn repeated_root_placements_admit_the_full_existing_sibling_id_before_copying() {
    let basis = basis();
    let child = id(&"c".repeat(16 * 1024));
    let mut document = original();
    for record in &mut document.entities {
        if record.id == id("local-child") {
            record.id = child.clone();
        }
    }
    document
        .placements
        .iter_mut()
        .find(|entry| entry.target == PrefabEntityTarget::source(id("local-child")))
        .unwrap()
        .target = PrefabEntityTarget::source(child.clone());
    for index in 0..100 {
        document.placements.push(SceneEntityPlacement::new(
            PrefabEntityTarget {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
                    expected_base: key("base"),
                }],
                entity: PrefabEntityOrigin::SourceEntity(id(&format!("root-{index}"))),
            },
            ScenePlacement::before(
                ScenePlacementParent::Entity(PrefabEntityTarget::source(id("anchor"))),
                PrefabEntityTarget::source(child.clone()),
            ),
        ));
    }
    document.canonicalize();
    let defaults = SceneAuthoringHistoryLimits::default();
    let limits = SceneAuthoringHistoryLimits::new(
        defaults.max_records(),
        defaults.max_operations(),
        defaults.max_value_nodes(),
        defaults.max_logical_bytes(),
        defaults.max_entry_operations(),
        defaults.max_entry_value_nodes(),
        1024 * 1024,
    )
    .unwrap();
    let session = open_scene(&document, &basis, limits);
    let before = session.shared_document();
    let mut sources = Sources::new(basis);
    sources.documents.insert(
        instance("base").source,
        (
            key("base"),
            Arc::new(PrefabDocument::from_ordered_roots((0..100).map(|index| {
                crate::SceneEntityRecord::new(id(&format!("root-{index}")))
            }))),
        ),
    );
    let error = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap_err();
    let PrefabConversionError::Source(report) = error;
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-prefab-conversion-copy-limit"
        }),
        "{:?}",
        report.diagnostics
    );
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.history_usage().records, 0);
}

#[test]
fn dependency_capture_refuses_before_retention_when_loader_reserves_the_remaining_allowance() {
    let basis = basis();
    let session = open_scene(&source(), &basis, Default::default());
    let before = session.shared_document();
    let mut sources = Sources::new(basis);
    let request = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .unwrap();
    let error = request
        .prepare_with_loader(None, |request, working| {
            // All remaining allowance belongs to this loader; a new dependency cannot fit.
            working.restrict_working_bytes(
                nara_core::ByteLimit::new(working.checked_work_bytes().unwrap()).unwrap(),
            );
            sources.load(request, working)
        })
        .unwrap_err();
    let PrefabConversionError::Source(report) = error;
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-prefab-conversion-source-budget-exceeded"
        }),
        "{:?}",
        report.diagnostics
    );
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.history_usage().records, 0);
}

#[test]
fn long_declared_reference_paths_share_the_source_preparation_work_allowance() {
    let storage = "r".repeat(64 * 1024);
    let mut field = nara_reflect::ComponentFieldSchema::required(
        ComponentFieldId::new("refs"),
        "References",
        ComponentFieldPath::from_fields([storage.as_str()]),
        ComponentValueKind::List,
    )
    .with_capabilities([
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::EntityRef,
    ]);
    field.shape = ComponentValueShape::list(ComponentValueShape::EntityRef);
    let basis = admit_schema(
        ComponentSchema::new(component(), "Path work", ComponentSchemaVersion::ONE)
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
            .with_fields([field]),
    );
    let defaults = SceneAuthoringHistoryLimits::default();
    let limits = SceneAuthoringHistoryLimits::new(
        defaults.max_records(),
        defaults.max_operations(),
        defaults.max_value_nodes(),
        defaults.max_logical_bytes(),
        defaults.max_entry_operations(),
        defaults.max_entry_value_nodes(),
        1024 * 1024,
    )
    .unwrap();
    let session = open_scene(&original(), &basis, limits);
    let before = session.shared_document();
    let value = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            storage.as_str(),
            ComponentValue::List(vec![
                ComponentValue::EntityReference(
                    EntityReference::SceneLocal { entity: id("root") }
                );
                32
            ]),
        )]),
    )
    .with_owner(schema_owner());
    let mut sources = Sources::new(basis);
    sources.documents.insert(
        instance("base").source,
        (
            key("base"),
            Arc::new(PrefabDocument::from_ordered_roots([
                crate::SceneEntityRecord::new(id("root")).with_component(component(), value),
            ])),
        ),
    );
    let error = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap_err();
    let PrefabConversionError::Source(report) = error;
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-prefab-conversion-copy-limit"
        }),
        "{:?}",
        report.diagnostics
    );
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.history_usage().records, 0);
}

#[test]
fn empty_selected_compilation_still_prepares_and_replays_one_conversion() {
    let basis = basis();
    let document = original();
    let mut session = open_scene(&document, &basis, Default::default());
    let mut sources = Sources::new(basis);
    sources.documents.insert(
        instance("base").source,
        (key("base"), Arc::new(PrefabDocument::default())),
    );
    let prepared = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap();
    assert!(prepared.entities().is_empty());
    assert_eq!(sources.reads, [instance("base").source]);
    let reviewed = prepared.document().clone();
    let commit = session
        .commit_complete_prefab_conversion_with_loader(prepared, |request| sources.reload(request))
        .unwrap();
    assert!(commit.report().applied, "{:?}", commit.report().diagnostics);
    assert_eq!(session.history_status().undo_depth, 1);
    let undo = session.undo_authoring();
    assert!(undo.applied, "{:?}", undo.diagnostics);
    assert_eq!(session.document(), &document);
    let redo = session.redo_authoring();
    assert!(redo.applied, "{:?}", redo.diagnostics);
    assert_eq!(session.document(), &reviewed);
}

#[test]
fn materialized_persistent_default_keeps_its_namespace_and_identity() {
    use nara_identity::{
        PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
    };
    let persistent = EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("test.external").unwrap(),
            PersistentRuntimeId::parse_str("a51d6a7c-3376-4de6-b846-7737431d6085").unwrap(),
        ),
    };
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueKind::EntityRef,
    )
    .with_capabilities([
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::EntityRef,
    ]);
    field.default_value = Some(ComponentValue::EntityReference(persistent.clone()));
    let basis = admit_schema(
        ComponentSchema::new(component(), "Default", ComponentSchemaVersion::ONE)
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
            .with_fields([field]),
    );
    let session = open_scene(&original(), &basis, Default::default());
    let mut sources = Sources::new(basis);
    let raw = PrefabDocument::from_ordered_roots([crate::SceneEntityRecord::new(id("root"))
        .with_component(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::Map(Default::default()),
            )
            .with_owner(schema_owner()),
        )]);
    sources.documents.insert(
        instance("base").source,
        (key("base"), Arc::new(raw.clone())),
    );
    let prepared = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .map_err(PrefabConversionError::Source)
        .and_then(|request| {
            request.prepare_with_loader(None, |request, work| sources.load(request, work))
        })
        .unwrap();
    let created = prepared.entities()[0].entity();
    let record = prepared
        .document()
        .entities
        .iter()
        .find(|record| &record.id == created)
        .unwrap();
    assert_eq!(
        record.components[&component()].value,
        ComponentValue::map([("target", ComponentValue::EntityReference(persistent)),])
    );
    assert_eq!(sources.documents[&instance("base").source].1.as_ref(), &raw);
}

#[test]
fn contextual_defaults_survive_nested_conversion_reopen_and_exact_history() {
    let local = |name: &str| {
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(name) })
    };
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueKind::EntityRef,
        local("root"),
    )
    .with_capabilities([
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::EntityRef,
    ]);
    let basis = admit_schema(
        ComponentSchema::new(component(), "Contextual", ComponentSchemaVersion::ONE)
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
            .with_fields([field]),
    );
    for masked in [false, true] {
        let payload = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            if masked {
                ComponentValue::map([("target", local("leaf"))])
            } else {
                ComponentValue::Map(Default::default())
            },
        )
        .with_owner(schema_owner())
        .with_contextual_default(ComponentFieldId::new("target"), local("root"));
        let mut bridge = crate::SceneEntityRecord::new(id("bridge"));
        bridge.prefab = Some(instance("nested"));
        let base = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("root")),
                child_placement("root", "leaf"),
                child_placement("root", "bridge"),
            ],
            [
                crate::SceneEntityRecord::new(id("root"))
                    .with_component(component(), payload.clone()),
                crate::SceneEntityRecord::new(id("leaf")),
                bridge,
            ],
        );
        let nested = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("root")),
                child_placement("root", "leaf"),
            ],
            [
                crate::SceneEntityRecord::new(id("root")).with_component(component(), payload),
                crate::SceneEntityRecord::new(id("leaf")),
            ],
        );
        let mut sources = Sources::new(basis.clone());
        sources.documents = [
            (instance("base").source, (key("base"), Arc::new(base))),
            (instance("nested").source, (key("nested"), Arc::new(nested))),
        ]
        .into();
        let mut document = original();
        document
            .placements
            .push(SceneEntityPlacement::root(id("root")));
        document
            .entities
            .push(crate::SceneEntityRecord::new(id("root")));
        document.canonicalize();
        let mut session = open_scene(&document, &basis, Default::default());
        let prepared = session
            .request_complete_prefab_conversion(
                session.revision(),
                &id("anchor"),
                Default::default(),
            )
            .unwrap()
            .prepare_with_loader(None, |request, work| sources.load(request, work))
            .unwrap();
        for depth in [1, 2] {
            let root = mapped(prepared.entities(), "root", depth, false);
            let leaf = mapped(prepared.entities(), "leaf", depth, false);
            let record = prepared
                .document()
                .entities
                .iter()
                .find(|record| &record.id == root)
                .unwrap();
            let payload = &record.components[&component()];
            assert_eq!(
                payload.contextual_defaults[&ComponentFieldId::new("target")],
                local(root.as_str())
            );
            assert_eq!(
                target(prepared.document(), root),
                if masked { leaf.clone() } else { root.clone() }
            );
        }
        let reviewed = prepared.document().clone();
        let rebuilt = open_scene(&reviewed, &basis, Default::default());
        session
            .commit_complete_prefab_conversion_with_loader(prepared, |request| {
                sources.reload(request)
            })
            .unwrap();
        assert_classification_matches(&session, &rebuilt);
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &document);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &reviewed);
        let reopened = SceneDocumentCandidate::decode_ron_str(&reviewed.to_ron_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/localize.scene.ron").unwrap()),
                &basis,
            )
            .unwrap()
            .publish();
        assert_eq!(reopened.document(), &reviewed);
    }
}

fn copy_stage(report: &ScenePatchReport, stage: &str) -> bool {
    report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.authoring-prefab-conversion-copy-limit"
            && diagnostic
                .fields()
                .iter()
                .any(|field| field.key().as_str() == "copy-stage" && field.display_value() == stage)
    })
}

fn assert_classification_matches(actual: &SceneAuthoringSession, expected: &SceneAuthoringSession) {
    assert_eq!(
        actual.authoring_record_facts(),
        expected.authoring_record_facts()
    );
    assert_eq!(
        actual.unavailable_record_manifest(),
        expected.unavailable_record_manifest()
    );
    assert_eq!(actual.authoring_readiness(), expected.authoring_readiness());
    assert_eq!(
        actual.authoring_diagnostics(),
        expected.authoring_diagnostics()
    );
    let actual = actual.authority.published().unwrap();
    let expected = expected.authority.published().unwrap();
    assert_eq!(
        actual
            .schema_declared_asset_references()
            .collect::<Vec<_>>(),
        expected
            .schema_declared_asset_references()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        actual.checked_retained_bytes(),
        expected.checked_retained_bytes()
    );
}

#[test]
fn remaining_conversion_allowance_refuses_large_unrelated_root_before_document_copy() {
    let basis = basis();
    let mut document = original();
    document
        .entities
        .iter_mut()
        .find(|record| record.id == id("other"))
        .unwrap()
        .components
        .insert(
            nara_reflect::ComponentTypeId::new("test.LargeUnavailable"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::List(
                    (0..4)
                        .map(|_| ComponentValue::String("x".repeat(512 * 1024)))
                        .collect(),
                ),
            ),
        );
    let scene = open_scene(&document, &basis, Default::default());
    let published = PrefabDocumentCandidate::decode_json_str(
        &PrefabDocument::new(document.placements.clone(), document.entities.clone())
            .to_json_string()
            .unwrap(),
    )
    .unwrap()
    .admit_authoring_with_basis(
        AuthoringDocumentSource::new(AssetRef::path("prefabs/guard.prefab.json").unwrap()),
        &basis,
    )
    .unwrap()
    .publish();
    let prefab = PrefabAuthoringSession::from_published_authoring_document(published);
    let resolved = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::default()),
        basis,
        key("base"),
    );
    super::super::work::take_copies();
    let mut request = scene
        .request_complete_prefab_conversion(scene.revision(), &id("anchor"), Default::default())
        .unwrap();
    request.restrict_working_bytes(nara_core::ByteLimit::new(256 * 1024).unwrap());
    let error = request
        .prepare_with_loader(None, |_, _| Ok::<_, Infallible>(Some(resolved.clone())))
        .unwrap_err();
    let PrefabConversionError::Source(report) = error;
    assert!(
        copy_stage(&report, "complete-document"),
        "{:?}",
        report.diagnostics
    );
    assert!(
        super::super::work::take_copies().is_empty(),
        "the guard precedes the complete root clone"
    );

    let mut request = prefab
        .request_complete_prefab_conversion(prefab.revision(), &id("anchor"), Default::default())
        .unwrap();
    request.restrict_working_bytes(nara_core::ByteLimit::new(256 * 1024).unwrap());
    let error = request
        .prepare_with_loader(None, |_, _| Ok::<_, Infallible>(Some(resolved.clone())))
        .unwrap_err();
    let PrefabConversionError::Source(report) = error;
    assert!(
        copy_stage(&report, "complete-document"),
        "{:?}",
        report.diagnostics
    );
    assert!(
        super::super::work::take_copies().is_empty(),
        "Prefab preparation cannot instantiate before admission"
    );
    assert_eq!(scene.document(), &document);
    assert_eq!(scene.history_usage().records, 0);
    assert_eq!(prefab.history_usage().records, 0);
}

#[test]
fn last_loader_restriction_governs_complete_root_and_classification_copies() {
    let basis = basis();
    let mut document = original();
    // Exceed the accept-only certificate's depth while remaining valid source data. This
    // exercises the budgeted publication buffer rather than its allocation-free fast path.
    let payload = (0..14).fold(
        ComponentValue::List(
            (0..4)
                .map(|_| ComponentValue::String("x".repeat(512 * 1024)))
                .collect(),
        ),
        |value, _| ComponentValue::List(vec![value]),
    );
    document
        .entities
        .iter_mut()
        .find(|record| record.id == id("other"))
        .unwrap()
        .components
        .insert(
            nara_reflect::ComponentTypeId::new("test.LargeUnavailable"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, payload),
        );
    let session = open_scene(&document, &basis, Default::default());
    let prefab = PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_str(
            &PrefabDocument::new(document.placements.clone(), document.entities.clone())
                .to_json_string()
                .unwrap(),
        )
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("prefabs/guard.prefab.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish(),
    );
    let resolved = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::default()),
        basis,
        key("base"),
    );
    for (limit, stage, copies) in [
        (256 * 1024, "complete-document", Vec::new()),
        (
            3 * 1024 * 1024,
            "publication-buffer",
            vec!["complete-document"],
        ),
        (
            7 * 1024 * 1024,
            "classification-snapshot",
            vec!["complete-document"],
        ),
    ] {
        super::super::work::take_copies();
        let mut reads = 0;
        let error = session
            .request_complete_prefab_conversion(
                session.revision(),
                &id("anchor"),
                Default::default(),
            )
            .unwrap()
            .prepare_with_loader(None, |_, working| {
                reads += 1;
                working.restrict_working_bytes(nara_core::ByteLimit::new(limit).unwrap());
                Ok::<_, Infallible>(Some(resolved.clone()))
            })
            .unwrap_err();
        let PrefabConversionError::Source(report) = error;
        assert!(copy_stage(&report, stage), "{:?}", report.diagnostics);
        assert_eq!(reads, 1);
        assert_eq!(
            super::super::work::take_copies(),
            copies,
            "no copy after its owning guard refuses"
        );
        assert_eq!(session.document(), &document);
        assert_eq!(session.history_usage().records, 0);
    }
    for (limit, stage, copies) in [
        (256 * 1024, "complete-document", Vec::new()),
        (
            3 * 1024 * 1024,
            "classification-snapshot",
            vec!["complete-document"],
        ),
        (
            7 * 1024 * 1024,
            "publication-buffer",
            vec!["complete-document", "classification-snapshot"],
        ),
    ] {
        super::super::work::take_copies();
        let error = prefab
            .request_complete_prefab_conversion(
                prefab.revision(),
                &id("anchor"),
                Default::default(),
            )
            .unwrap()
            .prepare_with_loader(None, |_, working| {
                working.restrict_working_bytes(nara_core::ByteLimit::new(limit).unwrap());
                Ok::<_, Infallible>(Some(resolved.clone()))
            })
            .unwrap_err();
        let PrefabConversionError::Source(report) = error;
        assert!(copy_stage(&report, stage), "{:?}", report.diagnostics);
        assert_eq!(super::super::work::take_copies(), copies);
        assert_eq!(prefab.history_usage().records, 0);
    }
}

#[test]
fn unchanged_missing_large_defaults_are_shared_without_schema_or_value_materialization() {
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("payload"),
        "Payload",
        ComponentFieldPath::from_fields(["payload"]),
        ComponentValueKind::String,
        ComponentValue::String("x".repeat(2 * 1024 * 1024)),
    )
    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect]);
    let basis = admit_schema(
        ComponentSchema::new(component(), "Default", ComponentSchemaVersion::ONE)
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
            .with_fields([field]),
    );
    let mut document = original();
    document
        .entities
        .iter_mut()
        .find(|record| record.id == id("other"))
        .unwrap()
        .components
        .insert(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::Map(Default::default()),
            )
            .with_owner(schema_owner()),
        );
    let mut session = open_scene(&document, &basis, Default::default());
    let resolved = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::default()),
        basis.clone(),
        key("base"),
    );
    let mut request = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .unwrap();
    request.restrict_working_bytes(nara_core::ByteLimit::new(256 * 1024).unwrap());
    let prepared = request
        .prepare_with_loader(None, |_, _| Ok::<_, Infallible>(Some(resolved.clone())))
        .unwrap();
    assert!(
        prepared
            .document()
            .entities
            .iter()
            .find(|record| record.id == id("other"))
            .unwrap()
            .components[&component()]
            .value
            .get("payload")
            .is_none()
    );
    let rebuilt = open_scene(prepared.document(), &basis, Default::default());
    session
        .commit_complete_prefab_conversion_with_loader(prepared, |_| {
            Ok::<_, Infallible>(Some(resolved.clone()))
        })
        .unwrap();
    assert_eq!(
        session.authoring_record_facts(),
        rebuilt.authoring_record_facts()
    );
    assert_eq!(
        session.unavailable_record_manifest(),
        rebuilt.unavailable_record_manifest()
    );
    assert_eq!(
        session.authoring_diagnostics(),
        rebuilt.authoring_diagnostics()
    );
}

#[test]
fn classification_reuse_removes_only_selected_overlay_inventory_and_preserves_default_assets() {
    use crate::{PrefabComponentIntent, PrefabPropertyClaim, PrefabPropertyIntent};
    use nara_reflect::ComponentPropertyPath;
    let default = AssetRef::path("textures/default.png").unwrap();
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("image"),
        "Image",
        ComponentFieldPath::from_fields(["image"]),
        ComponentValueKind::AssetRef,
        crate::patch::asset_ref_value(&default),
    )
    .with_capabilities([
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::AssetRef,
    ]);
    let basis = admit_schema(
        ComponentSchema::new(component(), "Image", ComponentSchemaVersion::ONE)
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
            .with_fields([field]),
    );
    let mut document = original();
    for (anchor, path, base) in [
        ("anchor", "textures/converted.png", "base"),
        ("other", "textures/unrelated.png", "unrelated"),
    ] {
        let entity = document
            .entities
            .iter_mut()
            .find(|record| record.id == id(anchor))
            .unwrap();
        let mut overlay = PrefabOverlay::new(key(base));
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("leaf")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("image")),
                intent: PrefabPropertyIntent::Set(crate::patch::asset_ref_value(
                    &AssetRef::path(path).unwrap(),
                )),
            }],
        });
        entity.prefab.as_mut().unwrap().overrides = Some(overlay);
    }
    document
        .entities
        .iter_mut()
        .find(|record| record.id == id("other"))
        .unwrap()
        .components
        .insert(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::Map(Default::default()),
            )
            .with_owner(schema_owner()),
        );
    let mut session = open_scene(&document, &basis, Default::default());
    let resolved = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::from_ordered_roots([
            crate::SceneEntityRecord::new(id("leaf")).with_component(
                component(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::Map(Default::default()),
                )
                .with_owner(schema_owner()),
            ),
        ])),
        basis.clone(),
        key("base"),
    );
    let prepared = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |_, _| Ok::<_, Infallible>(Some(resolved.clone())))
        .unwrap();
    let rebuilt = open_scene(prepared.document(), &basis, Default::default());
    session
        .commit_complete_prefab_conversion_with_loader(prepared, |_| {
            Ok::<_, Infallible>(Some(resolved.clone()))
        })
        .unwrap();
    assert_classification_matches(&session, &rebuilt);
    let references = session
        .authority
        .published()
        .unwrap()
        .schema_declared_asset_references()
        .map(|reference| reference.asset_ref().clone())
        .collect::<Vec<_>>();
    assert_eq!(references.len(), 3);
    assert!(references.contains(&default));
    assert!(references.contains(&AssetRef::path("textures/converted.png").unwrap()));
    assert!(references.contains(&AssetRef::path("textures/unrelated.png").unwrap()));
}

#[test]
fn routed_contextual_default_survives_unselected_sibling_property_intent() {
    use nara_identity::ReferenceInputId;
    use nara_reflect::ComponentPropertyPath;

    use crate::{
        PrefabComponentIntent, PrefabPropertyClaim, PrefabPropertyIntent, PrefabReferenceArguments,
    };

    let u = ReferenceInputId::new("u").unwrap();
    let v = ReferenceInputId::new("v").unwrap();
    let field = ComponentFieldId::new("target");
    let input_default = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::input(u.clone()),
    });
    let socket = ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: id("socket"),
    });
    let basis = admit_schema(
        ComponentSchema::new(component(), "Routed default", ComponentSchemaVersion::ONE)
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
            .with_fields([ComponentFieldSchema::optional_with_default(
                field.clone(),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueShape::optional(ComponentValueShape::EntityRef),
                input_default.clone(),
            )
            .with_capabilities([
                ComponentCapability::Scene,
                ComponentCapability::Inspect,
                ComponentCapability::EntityRef,
            ])]),
    );
    let mut q = PrefabDocument::from_ordered_roots([crate::SceneEntityRecord::new(id("q-root"))
        .with_component(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("target", ComponentValue::Null)]),
            )
            .with_owner(schema_owner())
            .with_contextual_default(field.clone(), input_default),
        )]);
    q.inputs.insert(u.clone());
    let mut r = PrefabDocument::from_ordered_roots([crate::SceneEntityRecord::new(id("r-root"))
        .with_component(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("target", ComponentValue::Null)]),
            )
            .with_owner(schema_owner())
            .with_contextual_default(
                field.clone(),
                ComponentValue::EntityReference(EntityReference::Authored {
                    reference: AuthoredEntityReference::input(v.clone()),
                }),
            ),
        )]);
    r.inputs.insert(v.clone());
    let q = Arc::new(q);
    let r = Arc::new(r);

    for properties in [false, true] {
        let mut a = crate::SceneEntityRecord::new(id("a"));
        let mut selected = instance("q");
        selected.arguments = Some(PrefabReferenceArguments {
            base: key("q"),
            values: [(
                u.clone(),
                EntityReference::Authored {
                    reference: AuthoredEntityReference {
                        through: vec![PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::SourceEntity(id("b")),
                            expected_base: key("r"),
                        }],
                        target: AuthoredReferenceTarget::Input(v.clone()),
                    },
                },
            )]
            .into(),
        });
        a.prefab = Some(selected);
        let mut b = crate::SceneEntityRecord::new(id("b"));
        let mut sibling = instance("r");
        sibling.arguments = Some(PrefabReferenceArguments {
            base: key("r"),
            values: [(
                v.clone(),
                EntityReference::SceneLocal {
                    entity: id("socket"),
                },
            )]
            .into(),
        });
        if properties {
            let mut overlay = PrefabOverlay::new(key("r"));
            overlay.components.push(PrefabComponentIntent::Properties {
                target: PrefabEntityTarget::source(id("r-root")),
                component: component(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(field.clone()),
                    intent: PrefabPropertyIntent::Set(socket.clone()),
                }],
            });
            sibling.overrides = Some(overlay);
        }
        b.prefab = Some(sibling);
        let document = SceneDocument::from_ordered_roots([
            a,
            b.clone(),
            crate::SceneEntityRecord::new(id("socket")),
        ]);
        let mut sources = Sources::new(basis.clone());
        sources.documents = [
            (instance("q").source, (key("q"), Arc::clone(&q))),
            (instance("r").source, (key("r"), Arc::clone(&r))),
        ]
        .into();
        let mut session = open_scene(&document, &basis, Default::default());
        let prepared = session
            .request_complete_prefab_conversion(session.revision(), &id("a"), Default::default())
            .unwrap()
            .prepare_with_loader(None, |request, work| sources.load(request, work))
            .unwrap();
        assert_eq!(
            prepared.entities().len(),
            1,
            "R contributes no selected values"
        );
        let localized = &prepared.entities()[0];
        assert_eq!(localized.provenance().local_entity_id(), &id("q-root"));
        let record = prepared
            .document()
            .entities
            .iter()
            .find(|record| &record.id == localized.entity())
            .unwrap();
        let payload = &record.components[&component()];
        assert_eq!(payload.value.get("target"), Some(&ComponentValue::Null));
        assert_eq!(payload.contextual_defaults.len(), 1);
        assert_eq!(payload.contextual_defaults[&field], socket);
        let reviewed = prepared.document().clone();
        assert_eq!(
            reviewed.entities.iter().find(|record| record.id == id("b")),
            Some(&b)
        );
        let rebuilt = open_scene(&reviewed, &basis, Default::default());
        let committed = session
            .commit_complete_prefab_conversion_with_loader(prepared, |request| {
                sources.reload(request)
            })
            .unwrap();
        assert!(
            committed.report().applied,
            "{:?}",
            committed.report().diagnostics
        );
        assert_eq!(session.document(), &reviewed);
        assert_classification_matches(&session, &rebuilt);
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &document);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &reviewed);
        assert_classification_matches(&session, &rebuilt);
    }
}

#[test]
fn mixed_contextual_default_updates_inventory_and_mapped_target_after_ordinary_reset() {
    use nara_core::{ByteLimit, ItemLimit};
    use nara_reflect::{ComponentPropertyPath, ComponentPropertyReadBudget, ComponentShapeMember};

    use crate::AuthoringPropertyValue;

    let hidden = AssetRef::path("textures/hidden.png").unwrap();
    let visible = AssetRef::path("textures/visible.png").unwrap();
    let field = ComponentFieldId::new("link");
    let mixed = |entity: &SceneEntityId, image: &AssetRef| {
        ComponentValue::map([
            (
                "target",
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: entity.clone(),
                }),
            ),
            ("image", crate::patch::asset_ref_value(image)),
        ])
    };
    let hidden_default = mixed(&id("root"), &hidden);
    let basis = admit_schema(
        ComponentSchema::new(component(), "Mixed default", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::optional_with_default(
                field.clone(),
                "Link",
                ComponentFieldPath::from_fields(["link"]),
                ComponentValueShape::record_members([
                    ComponentShapeMember::new("target", ComponentValueShape::EntityRef)
                        .with_id(ComponentFieldId::new("target")),
                    ComponentShapeMember::new("image", ComponentValueKind::AssetRef)
                        .with_id(ComponentFieldId::new("image")),
                ]),
                hidden_default.clone(),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef)
            .with_capability(ComponentCapability::AssetRef)]),
    );
    let base = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("root")),
            child_placement("root", "leaf"),
        ],
        [
            crate::SceneEntityRecord::new(id("root")).with_component(
                component(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("link", mixed(&id("leaf"), &visible))]),
                )
                .with_owner(schema_owner())
                .with_contextual_default(field.clone(), hidden_default),
            ),
            crate::SceneEntityRecord::new(id("leaf")),
        ],
    );
    let mut sources = Sources::new(basis.clone());
    sources.documents = [(instance("base").source, (key("base"), Arc::new(base)))].into();
    let mut anchor = crate::SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(instance("base"));
    let document = SceneDocument::from_ordered_roots([
        anchor,
        crate::SceneEntityRecord::new(id("root")),
        crate::SceneEntityRecord::new(id("leaf")),
    ]);
    let mut session = open_scene(&document, &basis, Default::default());
    let prepared = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |request, work| sources.load(request, work))
        .unwrap();
    let root = mapped(prepared.entities(), "root", 1, false).clone();
    let leaf = mapped(prepared.entities(), "leaf", 1, false).clone();
    assert_ne!(root, id("root"));
    assert_ne!(leaf, id("leaf"));
    let expected_default = mixed(&root, &hidden);
    let reviewed = prepared.document().clone();
    let payload = &reviewed
        .entities
        .iter()
        .find(|record| record.id == root)
        .unwrap()
        .components[&component()];
    assert_eq!(payload.value.get("link"), Some(&mixed(&leaf, &visible)));
    assert_eq!(payload.contextual_defaults[&field], expected_default);
    let rebuilt = open_scene(&reviewed, &basis, Default::default());
    let committed = session
        .commit_complete_prefab_conversion_with_loader(prepared, |request| sources.reload(request))
        .unwrap();
    assert!(
        committed.report().applied,
        "{:?}",
        committed.report().diagnostics
    );
    assert_eq!(session.document(), &reviewed);
    assert_classification_matches(&session, &rebuilt);
    let inventory = |session: &SceneAuthoringSession| {
        session
            .authority
            .published()
            .unwrap()
            .schema_declared_asset_references()
            .map(|reference| reference.asset_ref().clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(inventory(&session), [visible.clone()]);

    let reset = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
        entity: root.clone(),
        component: component(),
        component_version: ComponentSchemaVersion::ONE,
        field: field.clone(),
    }]);
    let report = session.apply_authoring_patch(&reset);
    assert!(report.applied, "{:?}", report.diagnostics);
    let absent = session.document().clone();
    let payload = &absent
        .entities
        .iter()
        .find(|record| record.id == root)
        .unwrap()
        .components[&component()];
    assert!(payload.value.get("link").is_none());
    assert_eq!(payload.contextual_defaults[&field], expected_default);
    assert_eq!(
        session
            .authoring_entity_view(&root)
            .unwrap()
            .component(&component())
            .unwrap()
            .property_value(
                &ComponentPropertyPath::new(field),
                &mut ComponentPropertyReadBudget::new(
                    ItemLimit::new(64).unwrap(),
                    ByteLimit::new(1024).unwrap(),
                ),
            )
            .unwrap(),
        AuthoringPropertyValue::Defaulted(&expected_default),
    );
    assert_eq!(inventory(&session), [hidden.clone()]);
    let reset_rebuilt = open_scene(&absent, &basis, Default::default());
    assert_classification_matches(&session, &reset_rebuilt);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &reviewed);
    assert_eq!(inventory(&session), [visible.clone()]);
    assert_classification_matches(&session, &rebuilt);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &document);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &reviewed);
    assert_eq!(inventory(&session), [visible]);
    assert_classification_matches(&session, &rebuilt);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &absent);
    assert_eq!(inventory(&session), [hidden]);
    assert_classification_matches(&session, &reset_rebuilt);
}
