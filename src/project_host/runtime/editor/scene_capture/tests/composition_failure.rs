use nara_scene::{
    PrefabDeclarationSource, PrefabEntityTarget, PrefabExpansionFailureKind, PrefabOverlay,
    PrefabOverlaySlot,
};

use super::*;

fn conflicting_anchor(fixture: &Fixture, id: &str, path: &str) -> SceneEntityRecord {
    let mut record = anchor(id, path);
    let mut overlay =
        PrefabOverlay::new(fixture.key(&format!("prefabs/{path}"), SourceDocumentKind::Prefab));
    overlay.suppressions.push(PrefabEntityTarget::source(
        SceneEntityId::new("missing-in-base").unwrap(),
    ));
    record.prefab.as_mut().unwrap().overrides = Some(overlay);
    record
}

fn wait_failure(editor: &mut EditorProjectSession, basis: &EditorDocumentQueryBasis) {
    let deadline = Instant::now() + WAIT;
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        if editor.prefab_composition_failure(basis).unwrap().is_some() {
            assert!(editor.current_editor_preview_content_stamp().is_none());
            assert!(editor.take_editor_preview_candidate().is_none());
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the verified composition failure did not publish: {:?}",
            editor.diagnostics(),
        );
        thread::yield_now();
    }
}

#[test]
fn rejected_composition_is_inspectable_only_for_its_current_scene_without_a_native_stamp() {
    let fixture = Fixture::new();
    let local = leaf(INITIAL_ROTATION).entities.remove(0);
    fs::write(
        fixture.root.join(SECOND),
        SceneDocument::from_ordered_roots([
            local,
            conflicting_anchor(&fixture, "anchor", "outer.prefab.json"),
        ])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let disk = fs::read(fixture.root.join(SECOND)).unwrap();
    fixture.run(|editor| {
        let startup = editor.scene_preview_document.unwrap();
        let startup_basis = editor.document_query_basis(startup).unwrap();
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let basis = editor.document_query_basis(second).unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        wait_failure(editor, &basis);
        let failure = editor.prefab_composition_failure(&basis).unwrap().unwrap();
        assert_eq!(failure.reason(), &PrefabExpansionFailureKind::MissingTarget);
        let location = failure.location().unwrap();
        assert_eq!(location.source(), &PrefabDeclarationSource::Containing);
        assert_eq!(location.path().anchor.as_str(), "anchor");
        assert!(location.path().steps.is_empty());
        assert_eq!(
            failure.slot(),
            Some(PrefabOverlaySlot::Suppression { index: 0 })
        );
        assert!(
            editor
                .prefab_composition_failure(&startup_basis)
                .unwrap()
                .is_none()
        );
        assert_eq!(dirty_count(editor), 0);
        assert_eq!(
            editor.workspace.document_revision(second),
            Some(basis.revision)
        );

        let mut stale_schema = basis.clone();
        stale_schema.schema = None;
        assert_eq!(
            editor.prefab_composition_failure(&stale_schema),
            Err(EditorDocumentQueryRejection::StaleSchema),
        );
        edit_rotation(editor, second, 0.75);
        assert_eq!(
            editor.prefab_composition_failure(&basis),
            Err(EditorDocumentQueryRejection::StaleRevision),
        );
        let edited_basis = editor.document_query_basis(second).unwrap();
        assert!(
            editor
                .prefab_composition_failure(&edited_basis)
                .unwrap()
                .is_none()
        );
        assert!(editor.current_editor_preview_content_stamp().is_none());
        wait_failure(editor, &edited_basis);
        assert_eq!(
            editor
                .prefab_composition_failure(&edited_basis)
                .unwrap()
                .unwrap()
                .location()
                .unwrap()
                .path()
                .anchor
                .as_str(),
            "anchor",
        );
    });
    assert_eq!(fs::read(fixture.root.join(SECOND)).unwrap(), disk);
}

#[test]
fn resolved_prefab_failure_keeps_its_owner_and_is_revoked_by_an_actual_participant_edit() {
    let fixture = Fixture::new();
    fs::write(
        fixture.root.join("prefabs/outer.prefab.json"),
        PrefabDocument::from_ordered_roots([conflicting_anchor(
            &fixture,
            "nested",
            "leaf.prefab.json",
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    fixture.run(|editor| {
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let outer = open_document(
            editor,
            "prefabs/outer.prefab.json",
            AuthoringDocumentKind::Prefab,
        );
        let prefab = open_document(editor, LEAF, AuthoringDocumentKind::Prefab);
        let basis = editor.document_query_basis(second).unwrap();
        let outer_basis = editor.document_query_basis(outer).unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        wait_failure(editor, &basis);
        let capture = editor.scene_capture.captured.as_ref().unwrap();
        assert!(capture.verified);
        assert!(!capture.native_ready);
        assert_eq!(
            capture
                .source_capture
                .as_ref()
                .unwrap()
                .prefabs()
                .iter()
                .filter(|prefab| prefab.session_revision().is_some())
                .count(),
            2,
        );
        let failure = editor.prefab_composition_failure(&basis).unwrap().unwrap();
        let location = failure.location().unwrap();
        assert_eq!(
            location.source(),
            &PrefabDeclarationSource::Prefab(
                fixture.key("prefabs/outer.prefab.json", SourceDocumentKind::Prefab),
            ),
        );
        assert_eq!(location.path().anchor.as_str(), "nested");
        assert_eq!(
            failure.slot(),
            Some(PrefabOverlaySlot::Suppression { index: 0 })
        );
        assert!(
            editor
                .prefab_composition_failure(&outer_basis)
                .unwrap()
                .is_none()
        );

        edit_rotation(editor, prefab, 0.75);
        assert!(editor.prefab_composition_failure(&basis).unwrap().is_none());
        assert!(editor.current_editor_preview_content_stamp().is_none());
        wait_failure(editor, &basis);
        assert_eq!(
            editor.workspace.document_revision(second),
            Some(basis.revision)
        );
        assert_eq!(editor.scene_capture.unsaved_dependencies(), 1);

        let mut changed = fs::read(fixture.root.join("prefabs/outer.prefab.json")).unwrap();
        changed.push(b' ');
        fs::write(fixture.root.join("prefabs/outer.prefab.json"), changed).unwrap();
        editor.scene_capture.validate_at = Some(Instant::now());
        let deadline = Instant::now() + WAIT;
        loop {
            editor.drive_editor_frame(Duration::ZERO);
            if editor.prefab_composition_failure(&basis).unwrap().is_none() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "a changed receipt retained its observation"
            );
            thread::yield_now();
        }
        assert!(editor.current_editor_preview_content_stamp().is_none());
    });
}

#[test]
fn a_missing_prefab_appearing_later_triggers_full_recapture_and_native_recovery() {
    let fixture = Fixture::new();
    let leaf_bytes = fs::read(fixture.root.join(LEAF)).unwrap();
    fs::remove_file(fixture.root.join(LEAF)).unwrap();
    fixture.run(|editor| {
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let basis = editor.document_query_basis(second).unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        wait_failure(editor, &basis);
        let capture = editor.scene_capture.captured.as_ref().unwrap();
        assert!(capture.requires_recapture);
        assert!(!capture.native_ready);
        assert!(editor.scene_capture.validate_at.is_some());
        // Every captured receipt still matches: only an actual dependency demand can see this file.
        capture.validation.clone().validate().unwrap();
        fs::write(fixture.root.join(LEAF), &leaf_bytes).unwrap();
        capture.validation.clone().validate().unwrap();

        let mut gate = CompletionGate::hold_next(editor);
        editor.scene_capture.validate_at = Some(Instant::now());
        editor.drive_editor_frame(Duration::ZERO);
        gate.wait();
        assert!(editor.prefab_composition_failure(&basis).unwrap().is_none());
        assert!(editor.current_editor_preview_content_stamp().is_none());
        assert!(editor.take_editor_preview_candidate().is_none());
        gate.release();
        let candidate = wait_preview(editor, second, None);
        assert_eq!(candidate.revision(), basis.revision);
        assert!(editor.prefab_composition_failure(&basis).unwrap().is_none());
        assert_native_leaf(&fixture, editor, INITIAL_ROTATION);
        assert_eq!(dirty_count(editor), 0);
        assert!(
            !editor
                .scene_capture
                .captured
                .as_ref()
                .unwrap()
                .requires_recapture
        );
    });
    assert_eq!(fs::read(fixture.root.join(LEAF)).unwrap(), leaf_bytes);
}
