#[path = "support/prefab_extraction.rs"]
mod support;

use std::{collections::BTreeMap, time::Duration};

use nara_core::ItemLimit;
use nara_engine::project_host::{
    AuthoringSourceOperationLimits, EditorEditPreparationError, EditorPrefabExtractionOutcome,
    EditorPrefabExtractionPhase, EditorPrefabExtractionRequest,
};
use nara_reflect::{ComponentFieldId, ComponentPropertyPath, ComponentValue};
use nara_scene::{
    PrefabComponentIntent, PrefabEntityOrigin, PrefabInstanceBinding, PrefabInstancePath,
    PrefabOverlaySlot, PrefabPropertyIntent, PrefabRevert, SceneAuthoringHistoryStatus,
};
use nara_tooling::{
    EditorAction, EditorActionOutcome, EditorDocumentQueryRejection, EditorPrefabConversionAction,
    EditorPrefabConversionActionOutcome, EditorPrefabConversionPhase, EditorPrefabConversionResult,
    EditorPrefabExtractionDisposition,
};

use support::*;

fn row<'a>(scene: &'a [EntityObservation], path: &str) -> &'a EntityObservation {
    let mut rows = scene.iter().filter(|row| row.path == path);
    let row = rows
        .next()
        .unwrap_or_else(|| panic!("missing actual occurrence {path}"));
    assert!(
        rows.next().is_none(),
        "merged or duplicated occurrence {path}"
    );
    row
}

#[test]
fn public_extraction_adopts_real_outputs_preserves_selective_history_and_reopens_inheritance() {
    let project = Project::new();
    let before = project.snapshot(STARTUP);
    assert_eq!(before.len(), 25);
    assert_eq!(project.snapshot(DORMANT), before);
    assert_eq!(
        row(&before, "left/trunk/branch").children,
        [
            "left/trunk/branch/child",
            "left/trunk/branch/twig",
            "left/trunk/branch/badge",
        ]
    );
    assert_eq!(
        row(&before, "watcher").references,
        [
            "left/trunk/branch/child",
            "right/trunk/branch/child",
            "wrapped/inner/trunk/branch/child",
            "child",
            "left/trunk/branch",
        ]
    );
    assert_eq!(row(&before, "left/trunk/branch/child").amount, 77);
    assert_eq!(row(&before, "right/trunk/branch/child").amount, 30);
    assert_eq!(row(&before, "wrapped/inner/trunk/branch/child").amount, 30);
    assert_eq!(row(&before, "child").amount, 100);
    assert_eq!(
        row(&before, "right/trunk/branch").references,
        ["right/trunk/branch/child", "right/outside"]
    );
    assert_eq!(
        row(&before, "right/outside").references,
        ["right/trunk/branch", "right/trunk/branch/child"]
    );
    assert_eq!(
        row(&before, "right/trunk/branch/child").references,
        [
            "right/trunk/branch",
            "right/outside",
            "right/trunk/branch/twig/leaf",
        ]
    );
    let original: BTreeMap<_, _> = SOURCES
        .into_iter()
        .chain([BUNDLE, "nara.toml", "prefabs/actor.prefab.json.meta"])
        .map(|path| (path, project.bytes(path)))
        .collect();
    let mut propagated = before.clone();
    for path in [
        "right/trunk/branch/child",
        "wrapped/inner/trunk/branch/child",
    ] {
        propagated
            .iter_mut()
            .find(|row| row.path == path)
            .unwrap()
            .amount = 31;
    }
    let mut reverted = propagated.clone();
    reverted
        .iter_mut()
        .find(|row| row.path == "left/trunk/branch/child")
        .unwrap()
        .amount = 31;

    project.run(|editor| {
        let startup = editor.current_frame().scene_preview_document().unwrap();
        let selected = open_source(editor, P);
        let inactive = open_source(editor, INACTIVE);
        let guard = open_source(editor, GUARD);
        for (document, first, second) in [(inactive, 15, 25), (guard, 16, 26)] {
            edit_amount(editor, document, "target", first);
            edit_amount(editor, document, "target", second);
            replay(editor, document, true);
            assert_eq!(amount(editor, document, "target"), first);
            assert_eq!(
                tab(editor, document).history,
                SceneAuthoringHistoryStatus {
                    undo_depth: 1,
                    redo_depth: 1
                }
            );
            assert!(tab(editor, document).dirty);
        }
        activate(editor, selected);
        let documents = [startup, selected, inactive, guard];
        let captured = documents.map(|document| tab(editor, document));
        let old_selected_basis = editor.document_query_basis(selected).unwrap();
        let schema = old_selected_basis.schema.clone().unwrap();
        let mut request =
            EditorPrefabExtractionRequest::new(selected, id("branch"), target_metadata());
        request.save_current.push(inactive);
        let limits = AuthoringSourceOperationLimits::default()
            .with_open_handles(ItemLimit::new(128).unwrap());
        let operation = editor
            .begin_prefab_extraction_with_limits(request, limits)
            .unwrap();
        assert!(editor.prefab_extraction_view().writes_fenced);
        wait(editor, |editor| {
            !editor.prefab_extraction_view().work_pending
        });
        let view = editor.prefab_extraction_view();
        assert_eq!(
            view.phase,
            EditorPrefabExtractionPhase::Review,
            "{view:?}; {:?}",
            editor.prefab_extraction_diagnostics()
        );
        assert_eq!(view.operation, Some(operation));
        assert_eq!(view.limits, limits);
        assert!(view.can_revise() && view.can_cancel());
        assert!(!editor.current_frame().authoring_operation_work_pending());
        let review = editor.prefab_extraction_review(operation).unwrap();
        assert_eq!(review.source_count(), SOURCES.len());
        let dispositions: BTreeMap<_, _> = review.documents().collect();
        assert_eq!(dispositions.len(), 4);
        for (document, disposition) in [
            (startup, EditorPrefabExtractionDisposition::ReplaceSource),
            (selected, EditorPrefabExtractionDisposition::ReplaceSource),
            (inactive, EditorPrefabExtractionDisposition::SaveCurrent),
            (guard, EditorPrefabExtractionDisposition::Guard),
        ] {
            assert_eq!(dispositions[&document], disposition);
        }
        let outputs: BTreeMap<_, _> = review
            .outputs()
            .map(|output| (output.project_path().to_owned(), output.bytes().to_vec()))
            .collect();
        let mut expected_paths = vec![STARTUP, INACTIVE, P, WRAPPER, CLOSED, DORMANT, Q, Q_META];
        expected_paths.sort_unstable();
        assert_eq!(
            outputs.keys().map(String::as_str).collect::<Vec<_>>(),
            expected_paths
        );
        assert_eq!(documents.map(|document| tab(editor, document)), captured);
        for (path, bytes) in &original {
            assert_eq!(&project.bytes(path), bytes, "Review wrote {path}");
        }
        assert!(!project.root.path().join(Q).exists());
        assert!(!project.root.path().join(Q_META).exists());

        editor.confirm_prefab_extraction(operation).unwrap();
        wait(editor, |editor| {
            !editor.prefab_extraction_view().work_pending
        });
        let view = editor.prefab_extraction_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(
            view.phase,
            EditorPrefabExtractionPhase::Terminal,
            "{view:?}; {:?}",
            editor.prefab_extraction_diagnostics()
        );
        assert_eq!(view.outcome, Some(EditorPrefabExtractionOutcome::Adopted));
        assert_eq!(view.refusal, None);
        assert!(!view.writes_fenced && !view.can_cancel() && !view.can_retry());
        editor.drive_editor_frame(Duration::ZERO);
        for (path, bytes) in &outputs {
            assert_eq!(&project.bytes(path), bytes, "{path}");
        }
        for path in [
            GUARD,
            LEAF,
            BUNDLE,
            "nara.toml",
            "prefabs/actor.prefab.json.meta",
        ] {
            assert_eq!(project.bytes(path), original[path], "{path}");
        }
        assert_eq!(project.snapshot(STARTUP), before);
        assert_eq!(project.snapshot(DORMANT), before);
        assert_eq!(
            editor.current_frame().workspace().active_document,
            Some(selected)
        );
        assert_eq!(
            editor
                .current_frame()
                .workspace()
                .documents
                .iter()
                .map(|tab| tab.document)
                .collect::<Vec<_>>(),
            documents
        );
        assert_eq!(tab(editor, guard), captured[3]);
        assert_eq!(amount(editor, guard, "target"), 16);
        let saved_current = tab(editor, inactive);
        assert_eq!(saved_current.revision, captured[2].revision);
        assert_eq!(saved_current.history, captured[2].history);
        assert_eq!(
            saved_current.authoring_schema_basis,
            captured[2].authoring_schema_basis
        );
        assert_eq!(saved_current.external_reload, captured[2].external_reload);
        assert_eq!(saved_current.saved_revision, captured[2].revision);
        assert!(!saved_current.dirty);
        for (document, original) in [(startup, &captured[0]), (selected, &captured[1])] {
            let installed = tab(editor, document);
            assert_eq!(installed.document, original.document);
            assert_ne!(
                installed.revision.source_id(),
                original.revision.source_id()
            );
            assert_eq!(installed.authoring_schema_basis.as_ref(), Some(&schema));
            assert_eq!(installed.revision, installed.saved_revision);
            assert_eq!(installed.history, SceneAuthoringHistoryStatus::default());
            assert!(!installed.dirty);
        }
        assert!(matches!(
            editor.validate_authoring_query_basis(&old_selected_basis),
            Err(EditorEditPreparationError::QueryBasis(
                EditorDocumentQueryRejection::StaleIncarnation
            ))
        ));
        let extracted = project.prefab(Q);
        assert_eq!(
            extracted
                .entities
                .iter()
                .map(|entity| entity.id.clone())
                .collect::<Vec<_>>(),
            [id("branch"), id("child"), id("twig")]
        );
        assert!(
            extracted
                .entities
                .iter()
                .find(|entity| entity.id == id("twig"))
                .unwrap()
                .prefab
                .is_some()
        );
        let containing = project.prefab(P);
        assert_eq!(
            containing
                .entities
                .iter()
                .map(|entity| entity.id.clone())
                .collect::<Vec<_>>(),
            [id("branch"), id("outside"), id("trunk")]
        );
        let branch = containing
            .entities
            .iter()
            .find(|entity| entity.id == id("branch"))
            .unwrap();
        assert!(branch.components.is_empty());
        let binding = branch.prefab.as_ref().unwrap();
        assert!(matches!(
            &binding.binding,
            PrefabInstanceBinding::Root { base, root }
                if base == &key(Q)
                    && root.through.is_empty()
                    && root.entity == PrefabEntityOrigin::SourceEntity(id("branch"))
        ));
        assert!(
            binding
                .arguments
                .as_ref()
                .is_some_and(|arguments| !arguments.values.is_empty())
        );

        for (document, expected_current, expected_redo) in [(inactive, 15, 25), (guard, 16, 26)] {
            let original = tab(editor, document);
            replay(editor, document, false);
            assert_eq!(amount(editor, document, "target"), expected_redo);
            assert_eq!(
                tab(editor, document).revision.source_id(),
                original.revision.source_id()
            );
            replay(editor, document, true);
            assert_eq!(amount(editor, document, "target"), expected_current);
            assert_eq!(tab(editor, document).history, original.history);
            assert_eq!(
                tab(editor, document).saved_revision,
                original.saved_revision
            );
            assert_eq!(tab(editor, document).dirty, original.dirty);
        }
        replay(editor, inactive, true);
        assert_eq!(amount(editor, inactive, "target"), 5);
        assert!(tab(editor, inactive).dirty);
        replay(editor, inactive, false);
        assert_eq!(amount(editor, inactive, "target"), 15);
        assert!(!tab(editor, inactive).dirty);
        for (document, entity, original) in [(selected, "outside", 50), (startup, "child", 100)] {
            let installed = tab(editor, document);
            edit_amount(editor, document, entity, original + 1);
            assert!(tab(editor, document).dirty);
            replay(editor, document, true);
            assert_eq!(amount(editor, document, entity), original);
            assert!(!tab(editor, document).dirty);
            replay(editor, document, false);
            assert_eq!(amount(editor, document, entity), original + 1);
            replay(editor, document, true);
            assert_eq!(
                tab(editor, document).revision.source_id(),
                installed.revision.source_id()
            );
        }

        let extracted_document = open_source(editor, Q);
        assert_eq!(amount(editor, extracted_document, "child"), 30);
        edit_amount(editor, extracted_document, "child", 31);
        save(editor, extracted_document);
        assert_eq!(project.snapshot(STARTUP), propagated);
        assert_eq!(project.snapshot(DORMANT), propagated);
        let basis = editor.document_query_basis(startup).unwrap();
        let path = PrefabInstancePath::new(id("left"));
        let observation = editor
            .observe_prefab_instance(&basis, &path, Default::default())
            .unwrap();
        let overlay = observation.instance.unwrap().overrides.unwrap();
        assert_eq!(overlay.additions.len(), 1);
        assert_eq!(overlay.additions[0].id, id("badge"));
        let mut pins = overlay.components.iter().enumerate().flat_map(
            |(component_index, intent)| match intent {
                PrefabComponentIntent::Properties {
                    component, claims, ..
                } if component == &payload_type() => claims
                    .iter()
                    .enumerate()
                    .filter_map(move |(claim_index, claim)| {
                        (claim.path == ComponentPropertyPath::new(ComponentFieldId::new("amount"))
                            && claim.intent == PrefabPropertyIntent::Set(ComponentValue::U64(77)))
                        .then_some(PrefabOverlaySlot::Property {
                            component_index,
                            claim_index,
                        })
                    })
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            },
        );
        let pin = pins
            .next()
            .expect("extraction preserves the original property pin");
        assert!(pins.next().is_none());
        let prepared = editor
            .prepare_prefab_revert(&basis, &path, PrefabRevert::Intent(pin), Default::default())
            .unwrap();
        applied(editor.commit_authoring_edit(prepared));
        editor.drive_editor_frame(Duration::ZERO);
        save(editor, startup);
        assert_eq!(project.snapshot(STARTUP), reverted);
        assert_eq!(project.snapshot(DORMANT), propagated);
        assert_eq!(project.bytes(GUARD), original[GUARD]);
    });

    project.run(|editor| {
        let startup = editor.current_frame().scene_preview_document().unwrap();
        assert!(!tab(editor, startup).dirty);
        assert_eq!(
            tab(editor, startup).history,
            SceneAuthoringHistoryStatus::default()
        );
        assert_eq!(project.snapshot(STARTUP), reverted);
        assert_eq!(project.snapshot(DORMANT), propagated);
        for (path, entity, expected) in [
            (Q, "child", 31),
            (P, "outside", 50),
            (INACTIVE, "target", 15),
            (GUARD, "target", 6),
        ] {
            let document = open_source(editor, path);
            assert_eq!(amount(editor, document, entity), expected);
            assert!(!tab(editor, document).dirty);
            assert_eq!(
                tab(editor, document).history,
                SceneAuthoringHistoryStatus::default()
            );
        }
        let original_source = project.bytes(STARTUP);
        let unchanged_prefabs = [P, Q].map(|path| project.bytes(path));
        let basis = editor.document_query_basis(startup).unwrap();
        let prepared = dispatch(editor, EditorAction::PrefabConversion(
            EditorPrefabConversionAction::Prepare { basis, anchor: id("left") }
        ));
        let EditorActionOutcome::PrefabConversion(EditorPrefabConversionActionOutcome::Accepted { operation, .. }) = prepared else {
            panic!("the extracted instance must admit complete conversion: {prepared:?}");
        };
        wait(editor, |editor| matches!(editor.current_frame().prefab_conversion().phase, EditorPrefabConversionPhase::Review | EditorPrefabConversionPhase::Terminal));
        assert_eq!(editor.current_frame().prefab_conversion().phase, EditorPrefabConversionPhase::Review);
        let committed = dispatch(editor, EditorAction::PrefabConversion(EditorPrefabConversionAction::Commit { operation }));
        assert!(matches!(committed, EditorActionOutcome::PrefabConversion(EditorPrefabConversionActionOutcome::Accepted { .. })));
        wait(editor, |editor| editor.current_frame().prefab_conversion().phase == EditorPrefabConversionPhase::Terminal);
        assert!(matches!(editor.current_frame().prefab_conversion().result.as_ref(), Some(EditorPrefabConversionResult::Committed { operation: actual, .. }) if *actual == operation));
        let acknowledged = dispatch(editor, EditorAction::PrefabConversion(EditorPrefabConversionAction::Acknowledge { operation }));
        assert!(matches!(acknowledged, EditorActionOutcome::PrefabConversion(EditorPrefabConversionActionOutcome::Accepted { phase: EditorPrefabConversionPhase::Idle, .. })));
        save(editor, startup);
        let converted_source = project.bytes(STARTUP);
        assert_ne!(converted_source, original_source);
        let decoded = nara_scene::SceneDocumentCandidate::decode_json_bytes(&converted_source).unwrap().into_composition_source().unwrap();
        assert!(decoded.entities.iter().find(|entity| entity.id == id("left")).unwrap().prefab.is_none());
        for anchor in ["right", "wrapped"] {
            assert!(decoded.entities.iter().find(|entity| entity.id == id(anchor)).unwrap().prefab.is_some());
        }
        assert_eq!(project.snapshot(STARTUP), reverted);
        assert_eq!(project.snapshot(DORMANT), propagated);
        replay(editor, startup, true);
        save(editor, startup);
        assert_eq!(project.bytes(STARTUP), original_source);
        replay(editor, startup, false);
        save(editor, startup);
        assert_eq!(project.bytes(STARTUP), converted_source);
        assert_eq!([P, Q].map(|path| project.bytes(path)), unchanged_prefabs);
    });

    project.run(|editor| {
        let startup = editor.current_frame().scene_preview_document().unwrap();
        assert_eq!(project.snapshot(STARTUP), reverted);
        assert_eq!(project.snapshot(DORMANT), propagated);
        assert_eq!(
            tab(editor, startup).history,
            SceneAuthoringHistoryStatus::default()
        );
        assert!(!tab(editor, startup).dirty);
        let source = nara_scene::SceneDocumentCandidate::decode_json_bytes(&project.bytes(STARTUP))
            .unwrap()
            .into_composition_source()
            .unwrap();
        assert!(
            source
                .entities
                .iter()
                .find(|entity| entity.id == id("left"))
                .unwrap()
                .prefab
                .is_none()
        );
    });
}
