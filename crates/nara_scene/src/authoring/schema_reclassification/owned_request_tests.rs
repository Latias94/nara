use super::*;
use crate::SchemaReclassificationKind::DeclarationExtension;
use crate::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentReadiness, AuthoringDocumentSource,
    PrefabDocumentCandidate, SceneComponentRecord, SceneDocumentCandidate, SceneEntityId,
    SceneEntityRecord, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};
use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind, EntityReference,
    PersistentComponentSchemaOwnerId,
};

const COMPONENT: &str = "test.OwnedReclassification";
const OWNER: &str = "test.owned-reclassification";

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn previous() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap()
}

fn successor_with_kind(kind: ComponentValueKind) -> AuthoringSchemaBasis {
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        kind.clone(),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    if kind == ComponentValueKind::EntityRef {
        field = field.with_capability(ComponentCapability::EntityRef);
    }
    let schema = ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Owned reclassification",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn successor() -> AuthoringSchemaBasis {
    successor_with_kind(ComponentValueKind::U64)
}

fn document_with_value(value: ComponentValue) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("root")).with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", value)]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    )])
}

fn document() -> SceneDocument {
    document_with_value(ComponentValue::U64(7))
}

fn scene(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(
                    AssetRef::path("scenes/owned-reclassification.scene.json").unwrap(),
                ),
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
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/owned-reclassification.prefab.json").unwrap(),
                ),
                basis,
            )
            .unwrap()
            .publish(),
    )
}

// Keep the nominal Scene and Prefab contracts under the same finite cases.
macro_rules! each_source {
    ($document:expr, $basis:expr, |$session:ident, $make:ident, $other:ident, $candidate:ident| $body:block) => {{
        {
            #[allow(unused_mut)]
            let mut $session = scene($document, $basis);
            #[allow(unused_variables)]
            let $make = scene;
            #[allow(unused_variables)]
            let $other = prefab;
            #[allow(dead_code)]
            type $candidate = SceneDocumentCandidate;
            $body
        }
        {
            #[allow(unused_mut)]
            let mut $session = prefab($document, $basis);
            #[allow(unused_variables)]
            let $make = prefab;
            #[allow(unused_variables)]
            let $other = scene;
            #[allow(dead_code)]
            type $candidate = PrefabDocumentCandidate;
            $body
        }
    }};
}

fn add(name: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id(name)),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }])
}

// The debug value includes the complete private scope/footprint; addresses additionally prove
// capture and publication leave the original retained entries in place.
fn history_snapshot(history: &AuthoringHistory) -> (String, Vec<usize>) {
    (
        format!("{history:?}"),
        history
            .undo_stack
            .iter()
            .chain(&history.redo_stack)
            .map(|entry| std::ptr::from_ref(entry) as usize)
            .collect(),
    )
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

fn assert_rejected(report: &ScenePatchReport) {
    assert!(!report.applied);
    assert!(report.inverse.is_none());
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-schema-reclassification-rejected"
        }),
        "{:?}",
        report.diagnostics
    );
}

fn limited(work: usize, retained: usize) -> SchemaReclassificationWork {
    SchemaReclassificationWork::new(SchemaReclassificationLimits::new(
        ByteLimit::new(work).unwrap(),
        ByteLimit::new(retained).unwrap(),
    ))
}

fn assert_limit(report: &ScenePatchReport, kind: &str, stage: Option<&str>) {
    assert!(!report.applied);
    assert!(report.inverse.is_none());
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-schema-reclassification-limit"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "budget-kind" && field.display_value() == kind
                })
                && stage.is_none_or(|stage| {
                    diagnostic.fields().iter().any(|field| {
                        field.key().as_str() == "preparation-stage"
                            && field.display_value() == stage
                    })
                })
        }),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn worker_preparation_shares_frozen_source_and_preserves_both_real_history_branches() {
    let previous = previous();
    let next = successor();
    each_source!(&document(), &previous, |session, make, other, Candidate| {
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
        session.source_upgrade_required = true;
        let before = source_snapshot!(session);
        let original_state = session.authority.published_shared().unwrap();
        let revision = session.revision();
        let mut work = SchemaReclassificationWork::default();
        let ticket = session
            .retain_schema_reclassification_source(&mut work)
            .unwrap();
        let request = session
            .capture_schema_reclassification(&next, &DeclarationExtension, &mut work, ticket)
            .unwrap();
        let captured = work.usage();
        assert_source_unchanged!(session, before);
        assert!(Arc::ptr_eq(
            &original_state,
            &session.authority.published_shared().unwrap()
        ));

        // This unscoped worker requires owned Send + 'static inputs and outputs. The live
        // authoring Session stays on this thread, including its native projection authority.
        let (prepared, work) = std::thread::spawn(move || {
            let prepared = request.prepare(&mut work).unwrap();
            (prepared, work)
        })
        .join()
        .unwrap();
        assert!(work.usage().work_bytes > captured.work_bytes);
        let view = prepared.source_view();
        assert!(Arc::ptr_eq(&before.0, &view.shared_document()));
        assert!(std::ptr::eq(view.document(), before.0.as_ref()));
        assert_eq!(view.base_revision(), revision);
        assert_eq!(
            view.base_revision().source_id(),
            initial_content.source_id()
        );
        assert_eq!(view.schema_basis(), next);
        assert_eq!(Some(view.source()), before.3.as_ref());
        assert_eq!(view.file_limits(), original_state.file_limits());
        assert_eq!(view.readiness(), AuthoringDocumentReadiness::Complete);
        assert_eq!(view.unavailable_record_manifest().total_count(), 0);
        assert_eq!(view.diagnostics(), prepared.diagnostics());
        assert_source_unchanged!(session, before);
        assert!(Arc::ptr_eq(
            &original_state,
            &session.authority.published_shared().unwrap()
        ));
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));

        session.commit_schema_reclassification(prepared).unwrap();
        assert_source_unchanged!(session, before);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.authoring_schema_basis(), Some(next.clone()));
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
fn frozen_requests_survive_edit_reopen_undo_and_basis_changes_but_cannot_commit() {
    let previous = previous();
    let next = successor();
    for change in ["edit", "reopen", "undo", "basis"] {
        each_source!(&document(), &previous, |session, make, other, Candidate| {
            assert!(session.apply_authoring_patch(&add("first")).applied);
            let frozen = source_snapshot!(session);
            let revision = session.revision();
            let mut work = SchemaReclassificationWork::default();
            let ticket = session
                .retain_schema_reclassification_source(&mut work)
                .unwrap();
            let request = session
                .capture_schema_reclassification(&next, &DeclarationExtension, &mut work, ticket)
                .unwrap();
            match change {
                "edit" => assert!(session.apply_authoring_patch(&add("later")).applied),
                "reopen" => {
                    let candidate =
                        Candidate::decode_json_str(&session.document().to_json_string().unwrap())
                            .unwrap();
                    session = session.try_reopen_authoring_candidate(candidate).unwrap();
                    assert_eq!(session.document(), frozen.0.as_ref());
                    assert_ne!(session.revision().source_id(), revision.source_id());
                }
                "undo" => {
                    assert!(session.undo_authoring().applied);
                    // Returning to identical content still advances the conflict revision.
                    assert!(session.redo_authoring().applied);
                    assert_eq!(session.document(), frozen.0.as_ref());
                }
                "basis" => {
                    let independently_admitted = self::previous();
                    assert_ne!(independently_admitted, previous);
                    let replacement = session
                        .prepare_schema_reclassification(
                            &independently_admitted,
                            &DeclarationExtension,
                            &mut SchemaReclassificationWork::default(),
                        )
                        .unwrap();
                    session.commit_schema_reclassification(replacement).unwrap();
                    assert_eq!(session.revision(), revision);
                }
                _ => unreachable!(),
            }
            let current = source_snapshot!(session);
            let current_state = session.authority.published_shared().unwrap();
            let prepared = std::thread::spawn(move || request.prepare(&mut work).unwrap())
                .join()
                .unwrap();
            let view = prepared.source_view();
            assert!(Arc::ptr_eq(&frozen.0, &view.shared_document()));
            assert_eq!(view.document(), frozen.0.as_ref());
            assert_eq!(view.base_revision(), revision);
            assert_eq!(view.schema_basis(), next);
            assert_eq!(Some(view.source()), frozen.3.as_ref());
            assert_eq!(view.readiness(), AuthoringDocumentReadiness::Complete);
            assert_source_unchanged!(session, current);
            assert_rejected(
                &session
                    .commit_schema_reclassification(prepared)
                    .unwrap_err(),
            );
            assert_source_unchanged!(session, current);
            assert!(Arc::ptr_eq(
                &current_state,
                &session.authority.published_shared().unwrap()
            ));
        });
    }
}

#[test]
fn retained_tickets_and_owned_requests_require_their_original_allowance_source_and_kind() {
    let previous = previous();
    let next = successor();
    each_source!(&document(), &previous, |session, make, other, Candidate| {
        let before = source_snapshot!(session);
        let mut work = SchemaReclassificationWork::default();
        let mut foreign_work = SchemaReclassificationWork::default();
        let ticket = session
            .retain_schema_reclassification_source(&mut work)
            .unwrap();
        let usage = work.usage();
        assert_rejected(
            &session
                .capture_schema_reclassification(
                    &next,
                    &DeclarationExtension,
                    &mut foreign_work,
                    ticket,
                )
                .unwrap_err(),
        );
        assert_eq!(foreign_work.usage(), SchemaReclassificationUsage::default());
        assert_eq!(work.usage(), usage);

        let foreign = make(&document(), &previous);
        assert_eq!(foreign.document(), session.document());
        let ticket = session
            .retain_schema_reclassification_source(&mut work)
            .unwrap();
        let usage = work.usage();
        assert_rejected(
            &foreign
                .capture_schema_reclassification(&next, &DeclarationExtension, &mut work, ticket)
                .unwrap_err(),
        );
        assert_eq!(work.usage(), usage);
        let opposite_kind = other(&document(), &previous);
        let ticket = session
            .retain_schema_reclassification_source(&mut work)
            .unwrap();
        let usage = work.usage();
        assert_rejected(
            &opposite_kind
                .capture_schema_reclassification(&next, &DeclarationExtension, &mut work, ticket)
                .unwrap_err(),
        );
        assert_eq!(work.usage(), usage);

        let ticket = session
            .retain_schema_reclassification_source(&mut work)
            .unwrap();
        let request = session
            .capture_schema_reclassification(&next, &DeclarationExtension, &mut work, ticket)
            .unwrap();
        let usage = work.usage();
        assert_rejected(&request.prepare(&mut foreign_work).unwrap_err());
        assert_eq!(foreign_work.usage(), SchemaReclassificationUsage::default());
        assert_eq!(work.usage(), usage);
        assert_source_unchanged!(session, before);
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));

        let ticket = session
            .retain_schema_reclassification_source(&mut work)
            .unwrap();
        assert!(session.apply_authoring_patch(&add("new-revision")).applied);
        let current = source_snapshot!(session);
        let usage = work.usage();
        assert_rejected(
            &session
                .capture_schema_reclassification(&next, &DeclarationExtension, &mut work, ticket)
                .unwrap_err(),
        );
        assert_eq!(work.usage(), usage);
        assert_source_unchanged!(session, current);
    });
}

#[test]
fn observed_capture_and_proof_budgets_refuse_before_history_copy_or_publication() {
    let previous = previous();
    let next = successor();
    each_source!(&document(), &previous, |session, make, other, Candidate| {
        assert!(session.apply_authoring_patch(&add("first")).applied);
        assert!(session.apply_authoring_patch(&add("second")).applied);
        assert!(session.undo_authoring().applied);
        let before = source_snapshot!(session);
        let original_state = session.authority.published_shared().unwrap();
        let mut measured = SchemaReclassificationWork::default();
        let ticket = session
            .retain_schema_reclassification_source(&mut measured)
            .unwrap();
        let retained = measured.usage();
        let request = session
            .capture_schema_reclassification(&next, &DeclarationExtension, &mut measured, ticket)
            .unwrap();
        let captured = measured.usage();
        drop(request.prepare(&mut measured).unwrap());
        let complete = measured.usage();
        assert!(captured.work_bytes > retained.work_bytes);
        assert!(captured.peak_retained_bytes > retained.peak_retained_bytes);
        assert!(complete.work_bytes > captured.work_bytes);

        // Calibrate from the real owner path rather than duplicating any cost formula.
        for kind in ["work", "retained"] {
            let mut work = if kind == "work" {
                limited(captured.work_bytes - 1, complete.peak_retained_bytes)
            } else {
                limited(complete.work_bytes, captured.peak_retained_bytes - 1)
            };
            let ticket = session
                .retain_schema_reclassification_source(&mut work)
                .unwrap();
            let usage = work.usage();
            let report = session
                .capture_schema_reclassification(&next, &DeclarationExtension, &mut work, ticket)
                .unwrap_err();
            assert_limit(&report, kind, Some("history-input-copy"));
            assert_eq!(work.usage(), usage);
            assert_source_unchanged!(session, before);
        }
        let mut short = limited(complete.work_bytes - 1, complete.peak_retained_bytes);
        let ticket = session
            .retain_schema_reclassification_source(&mut short)
            .unwrap();
        let request = session
            .capture_schema_reclassification(&next, &DeclarationExtension, &mut short, ticket)
            .unwrap();
        assert_limit(&request.prepare(&mut short).unwrap_err(), "work", None);
        let mut exact = limited(complete.work_bytes, complete.peak_retained_bytes);
        let ticket = session
            .retain_schema_reclassification_source(&mut exact)
            .unwrap();
        let request = session
            .capture_schema_reclassification(&next, &DeclarationExtension, &mut exact, ticket)
            .unwrap();
        let prepared = request.prepare(&mut exact).unwrap();
        assert_eq!(exact.usage(), complete);
        assert_eq!(prepared.readiness(), AuthoringDocumentReadiness::Complete);
        drop(prepared);
        assert_source_unchanged!(session, before);
        assert!(Arc::ptr_eq(
            &original_state,
            &session.authority.published_shared().unwrap()
        ));
    });
}

#[test]
fn invalid_successors_and_native_sources_refuse_without_additional_usage() {
    let previous = successor();
    let invalid = [
        self::previous(),
        successor_with_kind(ComponentValueKind::String),
    ];
    each_source!(&document(), &previous, |session, make, other, Candidate| {
        let before = source_snapshot!(session);
        let original_state = session.authority.published_shared().unwrap();
        for next in &invalid {
            let mut work = SchemaReclassificationWork::default();
            let ticket = session
                .retain_schema_reclassification_source(&mut work)
                .unwrap();
            let usage = work.usage();
            assert!(
                session
                    .capture_schema_reclassification(next, &DeclarationExtension, &mut work, ticket)
                    .is_err()
            );
            assert_eq!(work.usage(), usage);
            let mut fresh = SchemaReclassificationWork::default();
            assert!(
                session
                    .prepare_schema_reclassification(next, &DeclarationExtension, &mut fresh)
                    .is_err()
            );
            assert_eq!(fresh.usage(), SchemaReclassificationUsage::default());
            assert_source_unchanged!(session, before);
            assert!(Arc::ptr_eq(
                &original_state,
                &session.authority.published_shared().unwrap()
            ));
        }
    });
    macro_rules! native_refuses {
        ($native:expr) => {{
            let native = $native;
            let mut work = SchemaReclassificationWork::default();
            assert!(
                native
                    .retain_schema_reclassification_source(&mut work)
                    .is_err()
            );
            assert_eq!(work.usage(), SchemaReclassificationUsage::default());
        }};
    }
    native_refuses!(SceneAuthoringSession::new(document()));
    native_refuses!(PrefabAuthoringSession::new(PrefabDocument::new(
        document().roots,
        document().entities,
    )));
}

#[test]
fn frozen_history_still_rejects_a_dangling_undo_after_reopen_clears_live_history() {
    let previous = previous();
    let next = successor_with_kind(ComponentValueKind::EntityRef);
    let document = document_with_value(ComponentValue::EntityReference(
        EntityReference::SceneLocal {
            entity: id("later"),
        },
    ));
    each_source!(&document, &previous, |session, make, other, Candidate| {
        assert!(session.apply_authoring_patch(&add("later")).applied);
        assert_eq!(session.history_status().undo_depth, 1);
        let frozen_revision = session.revision();
        let mut work = SchemaReclassificationWork::default();
        let ticket = session
            .retain_schema_reclassification_source(&mut work)
            .unwrap();
        let request = session
            .capture_schema_reclassification(&next, &DeclarationExtension, &mut work, ticket)
            .unwrap();
        let candidate =
            Candidate::decode_json_str(&session.document().to_json_string().unwrap()).unwrap();
        session = session.try_reopen_authoring_candidate(candidate).unwrap();
        assert_ne!(session.revision().source_id(), frozen_revision.source_id());
        assert_eq!(session.history_status().undo_depth, 0);
        assert_eq!(session.history_status().redo_depth, 0);
        let before = source_snapshot!(session);
        let original_state = session.authority.published_shared().unwrap();

        // Identical current bytes without the old Undo edge are valid. The frozen request
        // must nevertheless prove that original edge, where Undo removes the reference target.
        let mut clean_work = SchemaReclassificationWork::default();
        let ticket = session
            .retain_schema_reclassification_source(&mut clean_work)
            .unwrap();
        let clean = session
            .capture_schema_reclassification(&next, &DeclarationExtension, &mut clean_work, ticket)
            .unwrap();
        assert_eq!(
            clean.prepare(&mut clean_work).unwrap().readiness(),
            AuthoringDocumentReadiness::Complete
        );
        let report = std::thread::spawn(move || request.prepare(&mut work).unwrap_err())
            .join()
            .unwrap();
        assert!(!report.applied);
        assert!(report.inverse.is_none());
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.authoring-local-reference-invalid"
            }),
            "{:?}",
            report.diagnostics
        );
        assert_source_unchanged!(session, before);
        assert!(Arc::ptr_eq(
            &original_state,
            &session.authority.published_shared().unwrap()
        ));
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
    });
}
