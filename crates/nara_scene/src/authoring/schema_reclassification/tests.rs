use std::{convert::Infallible, mem::Discriminant};

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind, EntityReference,
    PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    __private::AuthoringAdapterReceipt,
    AuthoringDocumentReadiness, AuthoringDocumentSource, AuthoringRecordReadiness,
    PrefabComponentIntent, PrefabComponentSetMode, PrefabDocumentCandidate, PrefabEntityTarget,
    PrefabInstance, PrefabOverlay, ResolvedPrefabSource, SceneComponentRecord,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchOperation, ScenePlacement,
    ScenePlacementOwner, SourceDocumentKey, SourceDocumentKind,
    authoring::{
        SceneAuthoringContentState, SceneAuthoringHistoryCost, SceneAuthoringHistoryEntry,
        SceneAuthoringHistoryLimits,
    },
};

const EXISTING: &str = "test.reclassification.Existing";
const RECOVERED: &str = "test.reclassification.Recovered";
const UNKNOWN: &str = "missing.Unrelated";
const EXISTING_OWNER: &str = "test.reclassification.existing";
const RECOVERED_OWNER: &str = "test.reclassification.recovered";

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn owner(name: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(name).unwrap()
}

fn schema(component: &str) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new(component),
        "Reclassification value",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
    .with_authoring_initial_value(ComponentValue::map([("value", ComponentValue::U64(1))]))
}

fn admit(declarations: Vec<(&str, ComponentSchema)>, data_adapters: bool) -> AuthoringSchemaBasis {
    let bundle = AuthoringSchemaBundle::new(declarations.into_iter().map(|(name, schema)| {
        AuthoringSchemaBundleOwner::new(
            owner(name),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )
    }))
    .unwrap();
    let adapters: Vec<_> = if data_adapters {
        bundle
            .owners()
            .iter()
            .map(|owner| {
                AuthoringAdapterReceipt::from_schema_owner(owner, &Default::default()).unwrap()
            })
            .collect()
    } else {
        Vec::new()
    };
    AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap()
}

fn empty_basis() -> AuthoringSchemaBasis {
    admit(Vec::new(), false)
}

fn successor() -> AuthoringSchemaBasis {
    admit(vec![(RECOVERED_OWNER, schema(RECOVERED))], true)
}

fn record(value: u64, declared_owner: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("value", ComponentValue::U64(value))]),
    )
    .with_owner(owner(declared_owner))
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root"))
            .with_component(ComponentTypeId::new(RECOVERED), record(7, RECOVERED_OWNER)),
        SceneEntityRecord::new(id("opaque")).with_component(
            ComponentTypeId::new(UNKNOWN),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve this unrelated payload".into()),
            ),
        ),
    ])
}

fn scene(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/reclassify.json").unwrap()),
                basis,
            )
            .unwrap()
            .publish(),
    )
}

fn prefab(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> PrefabAuthoringSession {
    let document = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("prefabs/reclassify.json").unwrap()),
                basis,
            )
            .unwrap()
            .publish(),
    )
}

// Expand identical owner assertions against both nominal public APIs.
macro_rules! each_source {
    ($document:expr, $basis:expr, |$session:ident| $body:block) => {{
        {
            #[allow(unused_mut)]
            let mut $session = scene($document, $basis);
            $body
        }
        {
            #[allow(unused_mut)]
            let mut $session = prefab($document, $basis);
            $body
        }
    }};
}

#[derive(Debug, PartialEq)]
struct EntrySnapshot {
    address: *const SceneAuthoringHistoryEntry,
    patch: ScenePatchDocument,
    from: SceneAuthoringContentState,
    to: SceneAuthoringContentState,
    cost: SceneAuthoringHistoryCost,
    scope: Discriminant<AuthoringHistoryScope>,
    scope_bytes: usize,
}

#[derive(Debug, PartialEq)]
struct HistorySnapshot {
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
    limits: SceneAuthoringHistoryLimits,
    cost: SceneAuthoringHistoryCost,
    undo: Vec<EntrySnapshot>,
    redo: Vec<EntrySnapshot>,
}

fn history_snapshot(history: &AuthoringHistory) -> HistorySnapshot {
    let entries = |stack: &std::collections::VecDeque<SceneAuthoringHistoryEntry>| {
        stack
            .iter()
            .map(|entry| EntrySnapshot {
                address: std::ptr::from_ref(entry),
                patch: entry.patch.clone(),
                from: entry.from_state,
                to: entry.to_state,
                cost: entry.cost,
                scope: std::mem::discriminant(&entry.scope),
                scope_bytes: entry.scope.retained_bytes(),
            })
            .collect()
    };
    HistorySnapshot {
        revision: history.revision,
        content: history.content_state,
        limits: history.limits,
        cost: history.cost,
        undo: entries(&history.undo_stack),
        redo: entries(&history.redo_stack),
    }
}

macro_rules! source_snapshot {
    ($session:ident) => {
        (
            $session.shared_document(),
            history_snapshot(&$session.history),
            $session.source_upgrade_required(),
            $session.authoring_source().cloned(),
        )
    };
}

macro_rules! assert_source_unchanged {
    ($session:ident, $before:ident) => {{
        assert!(Arc::ptr_eq(&$before.0, &$session.shared_document()));
        assert_eq!($session.document(), $before.0.as_ref());
        assert_eq!(history_snapshot(&$session.history), $before.1);
        assert_eq!($session.source_upgrade_required(), $before.2);
        assert_eq!($session.authoring_source(), $before.3.as_ref());
    }};
}

fn add(name: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id(name)),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }])
}

fn work_with_limits(work: usize, retained: usize) -> SchemaReclassificationWork {
    SchemaReclassificationWork::new(SchemaReclassificationLimits::new(
        ByteLimit::new(work).unwrap(),
        ByteLimit::new(retained).unwrap(),
    ))
}

fn has_code(report: &ScenePatchReport, code: &str) -> bool {
    report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code().as_str() == code)
}

fn assert_limit(report: &ScenePatchReport, kind: &str) {
    assert!(!report.applied);
    assert!(report.inverse.is_none());
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-schema-reclassification-limit"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "budget-kind" && field.display_value() == kind
                })
        }),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn recovery_preserves_source_and_both_history_branches_without_rebuilding_entries() {
    let previous = empty_basis();
    let next = successor();
    each_source!(&document(), &previous, |session| {
        assert_eq!(session.authoring_record_facts().unwrap().len(), 2);
        assert!(
            session
                .authoring_record_facts()
                .unwrap()
                .iter()
                .all(|fact| { fact.readiness() == AuthoringRecordReadiness::UnknownSchema })
        );
        let initial = session.document().clone();
        let initial_content = session.content_state();
        assert!(session.apply_authoring_patch(&add("first")).applied);
        let first = session.document().clone();
        let first_content = session.content_state();
        assert!(session.apply_authoring_patch(&add("second")).applied);
        let second = session.document().clone();
        let second_content = session.content_state();
        assert!(session.undo_authoring().applied);
        assert_eq!(session.history_status().undo_depth, 1);
        assert_eq!(session.history_status().redo_depth, 1);
        // Exercise preservation of an already recorded upgrade requirement as well as false.
        session.source_upgrade_required = true;
        let before = source_snapshot!(session);
        let mut work = SchemaReclassificationWork::default();
        let prepared = session
            .prepare_schema_reclassification(&next, &mut work)
            .unwrap();
        assert_eq!(prepared.schema_basis(), next);
        assert_eq!(prepared.base_revision(), session.revision());
        assert_eq!(prepared.readiness(), AuthoringDocumentReadiness::Degraded);
        assert_eq!(prepared.unavailable_record_manifest().total_count(), 1);
        assert!(!prepared.diagnostics().has_errors());
        let reviewed_manifest = prepared.unavailable_record_manifest().clone();
        let reviewed_diagnostics = prepared.diagnostics().clone();
        assert_source_unchanged!(session, before);
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
        assert_eq!(
            session.unavailable_record_manifest().unwrap().total_count(),
            2
        );
        assert!(work.usage().work_bytes > 0);
        session.commit_schema_reclassification(prepared).unwrap();
        assert_source_unchanged!(session, before);
        assert_eq!(session.authoring_schema_basis(), Some(next.clone()));
        assert_eq!(
            session.unavailable_record_manifest(),
            Some(&reviewed_manifest)
        );
        assert_eq!(session.authoring_diagnostics(), Some(&reviewed_diagnostics));
        assert_eq!(
            session.authoring_readiness(),
            Some(AuthoringDocumentReadiness::Degraded)
        );
        assert_eq!(
            session.unavailable_record_manifest().unwrap().total_count(),
            1
        );
        for fact in session.authoring_record_facts().unwrap() {
            if fact.component() == &ComponentTypeId::new(RECOVERED) {
                assert_eq!(fact.readiness(), AuthoringRecordReadiness::Available);
                assert!(!fact.adapter().unwrap().supports_native_preview());
            } else {
                assert_eq!(fact.component(), &ComponentTypeId::new(UNKNOWN));
                assert_eq!(fact.readiness(), AuthoringRecordReadiness::UnknownSchema);
                assert!(fact.adapter().is_none());
            }
        }
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &initial);
        assert_eq!(session.content_state(), initial_content);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &first);
        assert_eq!(session.content_state(), first_content);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &second);
        assert_eq!(session.content_state(), second_content);
    });
}

#[test]
fn owner_created_restore_entity_is_proved_as_history_and_remains_replayable() {
    let previous = admit(vec![(EXISTING_OWNER, schema(EXISTING))], true);
    let next = admit(
        vec![
            (EXISTING_OWNER, schema(EXISTING)),
            (RECOVERED_OWNER, schema(RECOVERED)),
        ],
        true,
    );
    let document = SceneDocument::new(
        [id("root"), id("tail")],
        [
            SceneEntityRecord::new(id("root")).with_children([id("child")]),
            SceneEntityRecord::new(id("child"))
                .with_component(ComponentTypeId::new(EXISTING), record(11, EXISTING_OWNER)),
            SceneEntityRecord::new(id("tail")),
        ],
    );
    let remove = ScenePatchDocument::new([ScenePatchOperation::RemoveEntity {
        entity: id("root"),
        from: ScenePlacement::first(ScenePlacementOwner::Root),
    }]);
    each_source!(&document, &previous, |session| {
        let original = session.document().clone();
        let original_content = session.content_state();
        let report = session.apply_authoring_patch(&remove);
        assert!(report.applied, "{:?}", report.diagnostics);
        assert!(matches!(
            &session.history.undo_stack.back().unwrap().patch.operations[0],
            ScenePatchOperation::RestoreEntity { descendants, .. } if descendants.len() == 1
        ));
        let deleted = session.document().clone();
        let before = source_snapshot!(session);
        let prepared = session
            .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
            .unwrap();
        session.commit_schema_reclassification(prepared).unwrap();
        assert_source_unchanged!(session, before);
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &original);
        assert_eq!(session.content_state(), original_content);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &deleted);
    });
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "e758bd18-2f06-4736-9752-dcfe038c3289".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn declaration(malformed: bool) -> PrefabInstance {
    let mut overlay = PrefabOverlay::new(key("reclassification.base"));
    let mut value = record(7, RECOVERED_OWNER);
    if malformed {
        value.value = ComponentValue::map([("value", ComponentValue::String("not a u64".into()))]);
    }
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: PrefabEntityTarget::source(id("inherited")),
        component: ComponentTypeId::new(RECOVERED),
        record: value,
        bindings: Vec::new(),
    });
    PrefabInstance {
        source: AssetRef::path("prefabs/reclassification-base.json").unwrap(),
        overrides: Some(overlay),
    }
}

fn replace_declaration(expected: bool, value: bool) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: id("anchor"),
        expected: Some(declaration(expected)),
        value: Some(declaration(value)),
    }])
}

#[test]
fn dormant_malformed_overlay_history_remains_replayable_on_either_branch() {
    let previous = empty_basis();
    let next = successor();
    for undo_branch in [true, false] {
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(declaration(undo_branch));
        let document = SceneDocument::from_ordered_roots([anchor]);
        each_source!(&document, &previous, |session| {
            let mut malformed = session.document().clone();
            malformed
                .entities
                .iter_mut()
                .find(|entity| entity.id == id("anchor"))
                .unwrap()
                .prefab = Some(declaration(true));
            let malformed_content;
            if undo_branch {
                malformed_content = session.content_state();
                assert!(
                    session
                        .apply_authoring_patch(&replace_declaration(true, false))
                        .applied
                );
                assert_eq!(session.history_status().undo_depth, 1);
            } else {
                assert!(
                    session
                        .apply_authoring_patch(&replace_declaration(false, true))
                        .applied
                );
                malformed_content = session.content_state();
                assert!(
                    session
                        .apply_authoring_patch(&replace_declaration(true, false))
                        .applied
                );
                assert!(session.undo_authoring().applied);
                assert!(session.undo_authoring().applied);
                assert_eq!(session.history_status().undo_depth, 0);
                assert_eq!(session.history_status().redo_depth, 2);
            }
            // Dormant declarations remain source data. Malformed overlay rejection belongs to
            // the Host's actual-demand compilation workflow, not source reclassification.
            let current = SceneDocument::new(
                session.document().roots.clone(),
                session.document().entities.clone(),
            );
            let mut clean = scene(&current, &previous);
            let clean_prepared = clean
                .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
                .unwrap();
            clean
                .commit_schema_reclassification(clean_prepared)
                .unwrap();
            let before = source_snapshot!(session);
            let facts = session.authoring_record_facts().unwrap().to_vec();
            let manifest = session.unavailable_record_manifest().unwrap().clone();
            let repaired = session.document().clone();
            let repaired_content = session.content_state();
            let prepared = session
                .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
                .unwrap();
            let reviewed_manifest = prepared.unavailable_record_manifest().clone();
            let reviewed_diagnostics = prepared.diagnostics().clone();
            assert_source_unchanged!(session, before);
            assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
            assert_eq!(session.authoring_record_facts().unwrap(), facts.as_slice());
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
            session.commit_schema_reclassification(prepared).unwrap();
            assert_source_unchanged!(session, before);
            assert_eq!(session.authoring_schema_basis(), Some(next.clone()));
            assert_eq!(
                session.unavailable_record_manifest(),
                Some(&reviewed_manifest)
            );
            assert_eq!(session.authoring_diagnostics(), Some(&reviewed_diagnostics));
            let exposed = if undo_branch {
                session.undo_authoring()
            } else {
                session.redo_authoring()
            };
            assert!(exposed.applied, "{:?}", exposed.diagnostics);
            assert_eq!(session.document(), &malformed);
            assert_eq!(session.content_state(), malformed_content);
            let restored = if undo_branch {
                session.redo_authoring()
            } else {
                session.undo_authoring()
            };
            assert!(restored.applied, "{:?}", restored.diagnostics);
            assert_eq!(session.document(), &repaired);
            assert_eq!(session.content_state(), repaired_content);
            if !undo_branch {
                // Keep both original retained edits replayable, including the exact repair.
                assert!(session.redo_authoring().applied);
                assert_eq!(session.document(), &malformed);
                assert!(session.redo_authoring().applied);
                assert_eq!(session.document(), &repaired);
            }
            assert_eq!(session.authoring_schema_basis(), Some(next.clone()));
        });
    }
}

#[test]
fn public_add_entity_history_rejects_reclassification_when_undo_reveals_a_dangling_reference() {
    let previous = empty_basis();
    let link_schema = ComponentSchema::new(
        ComponentTypeId::new(RECOVERED),
        "Recovered reference",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::EntityRef,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef)]);
    let next = admit(vec![(RECOVERED_OWNER, link_schema)], true);
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("root"))
        .with_component(
            ComponentTypeId::new(RECOVERED),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "value",
                    ComponentValue::EntityReference(EntityReference::SceneLocal {
                        entity: id("later"),
                    }),
                )]),
            )
            .with_owner(owner(RECOVERED_OWNER)),
        )]);
    each_source!(&document, &previous, |session| {
        assert_eq!(
            session.authoring_readiness(),
            Some(AuthoringDocumentReadiness::Degraded)
        );
        let added = session.apply_authoring_patch(&add("later"));
        assert!(added.applied, "{:?}", added.diagnostics);
        assert!(matches!(
            added.inverse.as_ref().unwrap().operations.as_slice(),
            [ScenePatchOperation::RemoveEntity { entity, .. }] if entity == &id("later")
        ));
        assert_eq!(session.history_status().undo_depth, 1);
        let current = SceneDocument::new(
            session.document().roots.clone(),
            session.document().entities.clone(),
        );
        // The same current bytes pass the complete source proof without retained history.
        let clean_scene = scene(&current, &previous);
        let clean_prefab = prefab(&current, &previous);
        assert_eq!(
            clean_scene
                .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
                .unwrap()
                .readiness(),
            AuthoringDocumentReadiness::Complete
        );
        assert_eq!(
            clean_prefab
                .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
                .unwrap()
                .readiness(),
            AuthoringDocumentReadiness::Complete
        );
        let before = source_snapshot!(session);
        let facts = session.authoring_record_facts().unwrap().to_vec();
        let manifest = session.unavailable_record_manifest().unwrap().clone();
        let report = session
            .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
            .unwrap_err();
        assert!(!report.applied);
        assert!(report.inverse.is_none());
        assert!(
            has_code(&report, "scene.authoring-local-reference-invalid"),
            "{:?}",
            report.diagnostics
        );
        assert_source_unchanged!(session, before);
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
        assert_eq!(session.authoring_record_facts().unwrap(), facts.as_slice());
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
        // Rejection leaves the real history usable under its original unknown-schema basis.
        let undone = session.undo_authoring();
        assert!(undone.applied, "{:?}", undone.diagnostics);
        assert_eq!(session.document().roots, document.roots);
        assert_eq!(session.document().entities, document.entities);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), before.0.as_ref());
        assert_eq!(session.content_state(), before.1.content);
    });
}

#[test]
fn declaration_extension_requires_exact_owner_full_schema_and_adapter_coverage() {
    let previous = admit(vec![(EXISTING_OWNER, schema(EXISTING))], true);
    let changed_default = schema(EXISTING)
        .with_authoring_initial_value(ComponentValue::map([("value", ComponentValue::U64(2))]));
    let invalid = [
        empty_basis(),
        admit(vec![("different.owner", schema(EXISTING))], true),
        admit(vec![(EXISTING_OWNER, changed_default)], true),
        admit(vec![(EXISTING_OWNER, schema(EXISTING))], false),
    ];
    each_source!(&SceneDocument::default(), &previous, |session| {
        let before = source_snapshot!(session);
        for next in &invalid {
            let mut work = SchemaReclassificationWork::default();
            assert!(
                session
                    .prepare_schema_reclassification(next, &mut work)
                    .is_err()
            );
            assert_eq!(work.usage(), SchemaReclassificationUsage::default());
            assert_source_unchanged!(session, before);
            assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
        }
    });
    let unbound = admit(vec![(EXISTING_OWNER, schema(EXISTING))], false);
    each_source!(&SceneDocument::default(), &unbound, |session| {
        assert!(
            session
                .prepare_schema_reclassification(
                    &previous,
                    &mut SchemaReclassificationWork::default()
                )
                .is_err()
        );
    });
}

#[test]
fn edit_and_schema_candidates_reject_stale_revision_and_independently_admitted_equal_basis() {
    let previous = empty_basis();
    let equal_previous = empty_basis();
    assert_ne!(previous, equal_previous);
    let next = successor();
    each_source!(&document(), &previous, |session| {
        let stale = session
            .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
            .unwrap();
        assert!(session.apply_authoring_patch(&add("newer")).applied);
        let before = source_snapshot!(session);
        assert!(session.commit_schema_reclassification(stale).is_err());
        assert_source_unchanged!(session, before);

        let old_edit = session
            .prepare_authoring_edit(session.revision(), &add("old-edit"))
            .unwrap();
        let old_schema = session
            .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
            .unwrap();
        // Equal metadata admitted independently changes the exact basis without changing revision.
        let replacement = session
            .prepare_schema_reclassification(
                &equal_previous,
                &mut SchemaReclassificationWork::default(),
            )
            .unwrap();
        session.commit_schema_reclassification(replacement).unwrap();
        assert_source_unchanged!(session, before);
        assert_eq!(
            session.authoring_schema_basis(),
            Some(equal_previous.clone())
        );
        assert!(session.commit_schema_reclassification(old_schema).is_err());
        let rejected = session.commit_authoring_edit(old_edit);
        assert!(!rejected.applied);
        assert_source_unchanged!(session, before);
    });
}

#[test]
fn duplicate_candidates_and_candidates_from_another_source_cannot_publish() {
    let previous = empty_basis();
    let next = successor();
    each_source!(&document(), &previous, |session| {
        let first = session
            .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
            .unwrap();
        let duplicate = session
            .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
            .unwrap();
        let before = source_snapshot!(session);
        session.commit_schema_reclassification(first).unwrap();
        assert!(session.commit_schema_reclassification(duplicate).is_err());
        assert_source_unchanged!(session, before);
        assert_eq!(session.authoring_schema_basis(), Some(next.clone()));
    });
    let mut first = scene(&document(), &previous);
    let mut second = scene(&document(), &previous);
    let foreign = first
        .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
        .unwrap();
    let before = source_snapshot!(second);
    assert!(second.commit_schema_reclassification(foreign).is_err());
    assert_source_unchanged!(second, before);
    let mut first_prefab = prefab(&document(), &previous);
    let mut second_prefab = prefab(&document(), &previous);
    let foreign = first_prefab
        .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
        .unwrap();
    let before = source_snapshot!(second_prefab);
    assert!(
        second_prefab
            .commit_schema_reclassification(foreign)
            .is_err()
    );
    assert_source_unchanged!(second_prefab, before);
    // A fresh source with identical bytes still owns an independent revision lineage.
    let stale = first
        .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
        .unwrap();
    first = first
        .try_reopen_authoring_candidate(
            SceneDocumentCandidate::decode_json_str(&document().to_json_string().unwrap()).unwrap(),
        )
        .unwrap();
    let before = source_snapshot!(first);
    assert!(first.commit_schema_reclassification(stale).is_err());
    assert_source_unchanged!(first, before);
    let stale = first_prefab
        .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
        .unwrap();
    first_prefab = first_prefab
        .try_reopen_authoring_candidate(
            PrefabDocumentCandidate::decode_json_str(
                &PrefabDocument::new(document().roots, document().entities)
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let before = source_snapshot!(first_prefab);
    assert!(first_prefab.commit_schema_reclassification(stale).is_err());
    assert_source_unchanged!(first_prefab, before);
}

#[test]
fn successful_recovery_invalidates_old_edits_even_when_source_revision_stays_equal() {
    let previous = empty_basis();
    let next = successor();
    each_source!(&document(), &previous, |session| {
        let edit = session
            .prepare_authoring_edit(session.revision(), &add("stale-edit"))
            .unwrap();
        let before = source_snapshot!(session);
        let prepared = session
            .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
            .unwrap();
        session.commit_schema_reclassification(prepared).unwrap();
        assert!(!session.commit_authoring_edit(edit).applied);
        assert_source_unchanged!(session, before);
        assert_eq!(session.authoring_schema_basis(), Some(next.clone()));
    });
}

#[test]
fn many_tiny_edits_charge_cumulative_work_separately_from_peak_retention() {
    let previous = empty_basis();
    let next = successor();
    let mut document = document();
    let opaque = document
        .entities
        .iter_mut()
        .find(|record| record.id == id("opaque"))
        .unwrap();
    opaque
        .components
        .get_mut(&ComponentTypeId::new(UNKNOWN))
        .unwrap()
        .value = ComponentValue::String("x".repeat(24 * 1024));
    each_source!(&document, &previous, |session| {
        for index in 0..16 {
            let report = session.apply_authoring_patch(&add(&format!("tiny-{index}")));
            assert!(report.applied, "{:?}", report.diagnostics);
        }
        let before = source_snapshot!(session);
        let mut measured = SchemaReclassificationWork::default();
        drop(
            session
                .prepare_schema_reclassification(&next, &mut measured)
                .unwrap(),
        );
        let usage = measured.usage();
        assert!(usage.work_bytes > usage.peak_retained_bytes, "{usage:?}");
        let mut exact = work_with_limits(usage.work_bytes, usage.peak_retained_bytes);
        drop(
            session
                .prepare_schema_reclassification(&next, &mut exact)
                .unwrap(),
        );
        assert_eq!(exact.usage(), usage);
        let mut work_limited = work_with_limits(usage.work_bytes - 1, usage.peak_retained_bytes);
        assert_limit(
            &session
                .prepare_schema_reclassification(&next, &mut work_limited)
                .unwrap_err(),
            "work",
        );
        let mut peak_limited = work_with_limits(usage.work_bytes, usage.peak_retained_bytes - 1);
        assert_limit(
            &session
                .prepare_schema_reclassification(&next, &mut peak_limited)
                .unwrap_err(),
            "retained",
        );
        assert_source_unchanged!(session, before);
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
    });
}

#[test]
fn absent_string_field_history_replays_large_generated_inverses_with_a_finite_work_limit() {
    const PAYLOAD_BYTES: usize = 64 * 1024;
    let text_schema = ComponentSchema::new(
        ComponentTypeId::new(EXISTING),
        "Optional text",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::optional(
        ComponentFieldId::new("text"),
        "Text",
        ComponentFieldPath::from_fields(["text"]),
        ComponentValueKind::String,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let previous = admit(vec![(EXISTING_OWNER, text_schema.clone())], true);
    let next = admit(
        vec![
            (EXISTING_OWNER, text_schema),
            (RECOVERED_OWNER, schema(RECOVERED)),
        ],
        true,
    );
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("root"))
        .with_component(
            ComponentTypeId::new(EXISTING),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            )
            .with_owner(owner(EXISTING_OWNER)),
        )
        .with_component(ComponentTypeId::new(RECOVERED), record(7, RECOVERED_OWNER))]);
    let set = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("root"),
        component: ComponentTypeId::new(EXISTING),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("text"),
        value: ComponentValue::String("x".repeat(PAYLOAD_BYTES)),
    }]);
    for redo_branch in [false, true] {
        each_source!(&document, &previous, |session| {
            let absent = session.document().clone();
            let absent_content = session.content_state();
            let report = session.apply_authoring_patch(&set);
            assert!(report.applied, "{:?}", report.diagnostics);
            assert!(matches!(
                session
                    .history
                    .undo_stack
                    .back()
                    .unwrap()
                    .patch
                    .operations
                    .as_slice(),
                [ScenePatchOperation::RemoveField { .. }]
            ));
            let present = session.document().clone();
            let present_content = session.content_state();
            if redo_branch {
                let report = session.undo_authoring();
                assert!(report.applied, "{:?}", report.diagnostics);
                assert!(matches!(
                    session.history.redo_stack.back().unwrap().patch.operations.as_slice(),
                    [ScenePatchOperation::SetField { value: ComponentValue::String(value), .. }]
                        if value.len() == PAYLOAD_BYTES
                ));
            }
            let before = source_snapshot!(session);
            let facts = session.authoring_record_facts().unwrap().to_vec();
            let manifest = session.unavailable_record_manifest().unwrap().clone();
            let mut measured = SchemaReclassificationWork::default();
            drop(
                session
                    .prepare_schema_reclassification(&next, &mut measured)
                    .unwrap(),
            );
            let usage = measured.usage();
            let mut limited = work_with_limits(usage.work_bytes - 1, usage.peak_retained_bytes);
            let report = session
                .prepare_schema_reclassification(&next, &mut limited)
                .unwrap_err();
            assert_limit(&report, "work");
            assert_source_unchanged!(session, before);
            assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
            assert_eq!(session.authoring_record_facts().unwrap(), facts.as_slice());
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
            let mut exact = work_with_limits(usage.work_bytes, usage.peak_retained_bytes);
            let prepared = session
                .prepare_schema_reclassification(&next, &mut exact)
                .unwrap();
            assert_eq!(prepared.readiness(), AuthoringDocumentReadiness::Complete);
            assert_eq!(prepared.unavailable_record_manifest().total_count(), 0);
            assert!(!prepared.diagnostics().has_errors());
            session.commit_schema_reclassification(prepared).unwrap();
            assert_source_unchanged!(session, before);
            if redo_branch {
                assert!(session.redo_authoring().applied);
                assert_eq!(session.document(), &present);
                assert_eq!(session.content_state(), present_content);
                assert!(session.undo_authoring().applied);
                assert_eq!(session.document(), &absent);
                assert_eq!(session.content_state(), absent_content);
            } else {
                assert!(session.undo_authoring().applied);
                assert_eq!(session.document(), &absent);
                assert_eq!(session.content_state(), absent_content);
                assert!(session.redo_authoring().applied);
                assert_eq!(session.document(), &present);
                assert_eq!(session.content_state(), present_content);
            }
        });
    }
}

#[test]
fn shared_work_remembers_dropped_candidates_and_limits_the_complete_source_set() {
    let previous = empty_basis();
    let next = successor();
    let first = scene(&document(), &previous);
    let second = prefab(&document(), &previous);
    let first_before = source_snapshot!(first);
    let second_before = source_snapshot!(second);
    let mut shared = SchemaReclassificationWork::default();
    drop(
        first
            .prepare_schema_reclassification(&next, &mut shared)
            .unwrap(),
    );
    let first_usage = shared.usage();
    drop(
        second
            .prepare_schema_reclassification(&next, &mut shared)
            .unwrap(),
    );
    let combined = shared.usage();
    let mut independent = SchemaReclassificationWork::default();
    drop(
        second
            .prepare_schema_reclassification(&next, &mut independent)
            .unwrap(),
    );
    assert_eq!(
        combined.work_bytes,
        first_usage.work_bytes + independent.usage().work_bytes
    );
    assert!(combined.peak_retained_bytes > first_usage.peak_retained_bytes);
    let mut limited = work_with_limits(combined.work_bytes - 1, combined.peak_retained_bytes);
    drop(
        first
            .prepare_schema_reclassification(&next, &mut limited)
            .unwrap(),
    );
    assert_limit(
        &second
            .prepare_schema_reclassification(&next, &mut limited)
            .unwrap_err(),
        "work",
    );
    assert_source_unchanged!(first, first_before);
    assert_source_unchanged!(second, second_before);
    assert_eq!(first.authoring_schema_basis(), Some(previous.clone()));
    assert_eq!(second.authoring_schema_basis(), Some(previous));
}

#[test]
fn tiny_limits_and_discarded_candidates_never_change_semantic_owner_state() {
    let previous = empty_basis();
    let next = successor();
    each_source!(&document(), &previous, |session| {
        assert!(session.apply_authoring_patch(&add("retained")).applied);
        let before = source_snapshot!(session);
        let facts = session.authoring_record_facts().unwrap().to_vec();
        let manifest = session.unavailable_record_manifest().unwrap().clone();
        let mut tiny_work = work_with_limits(1, 256 * 1024 * 1024);
        assert_limit(
            &session
                .prepare_schema_reclassification(&next, &mut tiny_work)
                .unwrap_err(),
            "work",
        );
        let mut tiny_peak = work_with_limits(512 * 1024 * 1024, 1);
        assert_limit(
            &session
                .prepare_schema_reclassification(&next, &mut tiny_peak)
                .unwrap_err(),
            "retained",
        );
        drop(
            session
                .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
                .unwrap(),
        );
        assert_source_unchanged!(session, before);
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
        assert_eq!(session.authoring_record_facts().unwrap(), facts.as_slice());
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    });
}

#[test]
fn retained_edges_must_keep_their_original_cost_on_both_stacks() {
    let previous = empty_basis();
    let next = successor();
    for undo_branch in [true, false] {
        each_source!(&document(), &previous, |session| {
            for name in ["first", "second", "third"] {
                assert!(session.apply_authoring_patch(&add(name)).applied);
            }
            assert!(session.undo_authoring().applied);
            let stack = if undo_branch {
                &mut session.history.undo_stack
            } else {
                &mut session.history.redo_stack
            };
            // Fault injection changes only the recorded allowance, never a conversion footprint.
            stack.front_mut().unwrap().cost = SceneAuthoringHistoryCost::default();
            let before = source_snapshot!(session);
            let report = session
                .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
                .unwrap_err();
            assert!(
                has_code(&report, "scene.authoring-history-cost-changed"),
                "{:?}",
                report.diagnostics
            );
            assert_source_unchanged!(session, before);
            assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
        });
    }
}

#[test]
fn proof_runs_every_real_edge_in_both_directions_and_checks_exact_source_and_inverse() {
    let previous = empty_basis();
    let mut session = scene(&document(), &previous);
    for name in ["first", "second", "third"] {
        assert!(session.apply_authoring_patch(&add(name)).applied);
    }
    assert!(session.undo_authoring().applied);
    assert_eq!(session.history_status().undo_depth, 2);
    assert_eq!(session.history_status().redo_depth, 1);
    let before = source_snapshot!(session);
    let state = session.authority.published().unwrap();
    for fault in 0..3 {
        let calls = std::cell::Cell::new(0);
        let mut work = SchemaReclassificationWork::default();
        let result = prove_history(
            &session.history,
            session.document(),
            state,
            &mut work,
            scene_bytes,
            |document, state, patch, scope, admit| {
                let index = calls.get();
                calls.set(index + 1);
                let mut prepared = prepare_scene_step(document, state, patch, scope, Some(admit))?;
                // Inject corruption only after the actual owner kernel executed the reverse edge.
                if index == 1 {
                    match fault {
                        1 => prepared.document.roots.reverse(),
                        2 => prepared.report.inverse = Some(ScenePatchDocument::default()),
                        _ => {}
                    }
                }
                Ok(ProofStep {
                    document: prepared.document,
                    state: prepared.state,
                    report: prepared.report,
                })
            },
        );
        if fault == 0 {
            result.unwrap();
            assert_eq!(calls.get(), 2 * session.history_usage().records);
        } else {
            let report = result.unwrap_err();
            assert!(has_code(
                &report,
                "scene.authoring-schema-reclassification-rejected"
            ));
            assert_eq!(calls.get(), 2);
        }
        assert_source_unchanged!(session, before);
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
    }
}

#[test]
fn all_sources_bind_before_infallible_commits_and_a_later_bind_failure_publishes_nothing() {
    let previous = empty_basis();
    let next = successor();
    let mut first = scene(&document(), &previous);
    let mut second = prefab(&document(), &previous);
    let mut third = scene(&document(), &previous);
    let mut work = SchemaReclassificationWork::default();
    let first_candidate = first
        .prepare_schema_reclassification(&next, &mut work)
        .unwrap();
    let second_candidate = second
        .prepare_schema_reclassification(&next, &mut work)
        .unwrap();
    let third_candidate = third
        .prepare_schema_reclassification(&next, &mut work)
        .unwrap();
    assert!(third.apply_authoring_patch(&add("newer")).applied);
    let first_before = source_snapshot!(first);
    let second_before = source_snapshot!(second);
    let third_before = source_snapshot!(third);
    {
        let first_guard = first.bind_schema_reclassification(first_candidate).unwrap();
        let second_guard = second
            .bind_schema_reclassification(second_candidate)
            .unwrap();
        assert!(third.bind_schema_reclassification(third_candidate).is_err());
        drop((first_guard, second_guard));
    }
    assert_source_unchanged!(first, first_before);
    assert_source_unchanged!(second, second_before);
    assert_source_unchanged!(third, third_before);
    assert_eq!(first.authoring_schema_basis(), Some(previous.clone()));
    assert_eq!(second.authoring_schema_basis(), Some(previous.clone()));
    assert_eq!(third.authoring_schema_basis(), Some(previous));
    let mut work = SchemaReclassificationWork::default();
    let first_candidate = first
        .prepare_schema_reclassification(&next, &mut work)
        .unwrap();
    let second_candidate = second
        .prepare_schema_reclassification(&next, &mut work)
        .unwrap();
    let third_candidate = third
        .prepare_schema_reclassification(&next, &mut work)
        .unwrap();
    let first_guard = first.bind_schema_reclassification(first_candidate).unwrap();
    let second_guard = second
        .bind_schema_reclassification(second_candidate)
        .unwrap();
    let third_guard = third.bind_schema_reclassification(third_candidate).unwrap();
    // A whole-set owner reaches this point with exclusive guards and no remaining fallible commit.
    first_guard.commit();
    second_guard.commit();
    third_guard.commit();
    assert_source_unchanged!(first, first_before);
    assert_source_unchanged!(second, second_before);
    assert_source_unchanged!(third, third_before);
    assert_eq!(first.authoring_schema_basis(), Some(next.clone()));
    assert_eq!(second.authoring_schema_basis(), Some(next.clone()));
    assert_eq!(third.authoring_schema_basis(), Some(next));
}

#[test]
fn real_complete_prefab_conversion_history_reclassifies_without_fabricated_footprints() {
    let previous = empty_basis();
    let next = successor();
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    let request_source = AssetRef::path("prefabs/conversion-base.json").unwrap();
    anchor.prefab = Some(PrefabInstance {
        source: request_source.clone(),
        overrides: None,
    });
    let document = SceneDocument::from_ordered_roots([anchor]);
    let dependency = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("leaf")),
    ]));
    let load = |request: &AssetRef| -> Result<Option<ResolvedPrefabSource>, Infallible> {
        assert_eq!(request, &request_source);
        Ok(Some(ResolvedPrefabSource::bound_authoring_source(
            Arc::clone(&dependency),
            previous.clone(),
            key("conversion-base"),
        )))
    };
    for redo_branch in [false, true] {
        each_source!(&document, &previous, |session| {
            let original = session.document().clone();
            let prepared = session
                .request_complete_prefab_conversion(
                    session.revision(),
                    &id("anchor"),
                    Default::default(),
                )
                .unwrap()
                .prepare_with_loader(None, |request, _| load(request))
                .unwrap();
            let reviewed = prepared.document().clone();
            session
                .commit_complete_prefab_conversion_with_loader(prepared, load)
                .unwrap();
            assert!(matches!(
                &session.history.undo_stack.back().unwrap().scope,
                AuthoringHistoryScope::PrefabConversion(_)
            ));
            if redo_branch {
                assert!(session.undo_authoring().applied);
            }
            let before = source_snapshot!(session);
            let prepared = session
                .prepare_schema_reclassification(&next, &mut SchemaReclassificationWork::default())
                .unwrap();
            session.commit_schema_reclassification(prepared).unwrap();
            assert_source_unchanged!(session, before);
            if redo_branch {
                assert!(session.redo_authoring().applied);
                assert_eq!(session.document(), &reviewed);
                assert!(session.undo_authoring().applied);
                assert_eq!(session.document(), &original);
            } else {
                assert!(session.undo_authoring().applied);
                assert_eq!(session.document(), &original);
                assert!(session.redo_authoring().applied);
                assert_eq!(session.document(), &reviewed);
            }
        });
    }
}

#[test]
fn native_sessions_have_no_reclassification_authority() {
    let next = successor();
    let native = SceneAuthoringSession::new(document());
    let mut work = SchemaReclassificationWork::default();
    assert!(
        native
            .prepare_schema_reclassification(&next, &mut work)
            .is_err()
    );
    assert_eq!(work.usage(), SchemaReclassificationUsage::default());
    let native =
        PrefabAuthoringSession::new(PrefabDocument::new(document().roots, document().entities));
    assert!(
        native
            .prepare_schema_reclassification(&next, &mut work)
            .is_err()
    );
    assert_eq!(work.usage(), SchemaReclassificationUsage::default());
}
