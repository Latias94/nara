use std::{collections::HashMap, convert::Infallible, sync::Arc};

use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaVersion, ComponentValue, ComponentValueKind,
    ComponentValueShape, EntityReference,
};

use super::*;
use crate::{
    AuthoringDocumentSource, PrefabAddition, PrefabAuthoringSession, PrefabDocument,
    PrefabDocumentCandidate, PrefabEntityTarget, PrefabOverlay, PrefabPlacement,
    PrefabPlacementParent, PrefabPlacementSlot, PrefabProjectionStep, PrefabReferenceBinding,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SourceDocumentKey, SourceDocumentKind,
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
    addition.components.insert(component(), reference("leaf"));
    addition.bindings.insert(
        component(),
        vec![PrefabReferenceBinding {
            slot: ComponentFieldPath::from_fields(["target"]),
            expected: id("leaf"),
            target: PrefabEntityTarget::source(id("root-a")),
        }],
    );
    overlay.additions.push(addition);
    overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("leaf")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
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
            [id("root-a"), id("root-b")],
            [
                crate::SceneEntityRecord::new(id("root-a"))
                    .with_children([id("leaf")])
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
        _: &mut PrefabExpansionWorkingSet,
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
            assert_eq!(
                retained.children,
                [
                    root_a.clone(),
                    root_b.clone(),
                    addition.clone(),
                    id("local-child")
                ]
            );
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
    let original = PrefabDocument::new(scene.roots, scene.entities);
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
    let child = id(&"c".repeat(256 * 1024));
    let mut document = original();
    for record in &mut document.entities {
        if record.id == id("anchor") {
            record.children = vec![child.clone()];
        } else if record.id == id("local-child") {
            record.id = child.clone();
        }
    }
    document.canonicalize();
    let session = open_scene(&document, &basis, Default::default());
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
