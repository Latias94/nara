//! Dedicated source repair cannot be forged by an otherwise identical structural patch.

use super::*;
use nara_scene::{
    PrefabDeclarationLimits, PrefabEntityTarget, PrefabInstancePath, PrefabOverlay,
    PrefabOverlaySlot, PrefabRevert, SceneEntityId, SourceDocumentKey, SourceDocumentKind,
};
use nara_tooling::{EditorActionRejection, EditorDocumentMutation};

#[test]
fn ui_prefab_revert_uses_source_owner_proof_and_keeps_one_undo() {
    let project = TestProject::with_image("ui-prefab-intent-revert");
    project.configure_authoring_ui("UI Prefab Intent Revert");
    project.write_ui_authoring_documents(None, None);
    let anchor = SceneEntityId::new("repair-root").unwrap();
    let mut overlay = PrefabOverlay::new(SourceDocumentKey::project_location(
        "c60a6da4-b6a7-5028-bc53-39111af1a2ba".parse().unwrap(),
        AssetPath::new("prefabs/ui.prefab.json").unwrap(),
        SourceDocumentKind::Prefab,
    ));
    overlay.suppressions.push(PrefabEntityTarget::source(
        SceneEntityId::new("obsolete").unwrap(),
    ));
    let mut record = SceneEntityRecord::new(anchor.clone())
        .with_component(
            ComponentTypeId::new("nara.ui.UiRoot"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("layer", ComponentValue::String("base".into()))]),
            ),
        )
        .with_component(
            ComponentTypeId::new("nara.ui.UiNode"),
            ui_node_record("flow", "percent", 1.0, "percent", 1.0),
        );
    record.prefab = Some(PrefabInstance {
        source: AssetRef::path("ui.prefab.json").unwrap(),
        overrides: Some(overlay),
    });
    let original = PrefabDocument::from_ordered_roots([record]);
    let path = project.root.join("prefabs/repair-ui.prefab.json");
    fs::write(&path, original.to_json_string().unwrap()).unwrap();
    let disk = fs::read(&path).unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = opened_document(editor.open_indexed_authoring_document(
        editor.project_source_index_snapshot().unwrap(),
        "prefabs/repair-ui.prefab.json",
        crate::project_host::authoring::AuthoringDocumentKind::Prefab,
    ));
    let basis = editor.document_query_basis(document).unwrap();
    let selector = PrefabInstancePath::new(anchor);
    let limits = PrefabDeclarationLimits::default();
    let observed = editor
        .observe_prefab_instance(&basis, &selector, limits)
        .unwrap();
    let action = PrefabRevert::Intent(PrefabOverlaySlot::Suppression { index: 0 });
    let prepared = editor
        .prepare_prefab_revert(&basis, &selector, action, limits)
        .unwrap();
    let stale = editor
        .prepare_prefab_revert(&basis, &selector, action, limits)
        .unwrap();
    assert!(prepared.is_prefab_revert());
    assert!(matches!(
        editor.prepare_authoring_edit(&basis, prepared.patch()),
        Err(editor::EditorEditPreparationError::StructureContractMismatch { .. }),
    ));
    let lookalike = editor
        .workspace()
        .prepare_authoring_edit(&basis, prepared.patch())
        .unwrap();
    assert!(!lookalike.is_prefab_revert());
    assert!(matches!(
        editor.commit_authoring_edit(lookalike),
        EditorActionOutcome::Rejected(EditorActionRejection::StructureContractMismatch { .. }),
    ));
    assert_eq!(
        editor.workspace().document_revision(document),
        Some(basis.revision)
    );
    assert!(matches!(
        editor.commit_authoring_edit(prepared),
        EditorActionOutcome::Workspace(report) if report.applied,
    ));
    let current = editor.document_query_basis(document).unwrap();
    let repaired = editor
        .observe_prefab_instance(&current, &selector, limits)
        .unwrap();
    let instance = repaired.instance.as_ref().unwrap();
    assert_eq!(instance.source, observed.instance.as_ref().unwrap().source);
    assert_eq!(
        instance.overrides.as_ref().unwrap().base,
        observed
            .instance
            .as_ref()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap()
            .base,
    );
    assert!(instance.overrides.as_ref().unwrap().suppressions.is_empty());
    assert_eq!(
        editor
            .workspace()
            .prefab(document)
            .unwrap()
            .session()
            .history_status()
            .undo_depth,
        1
    );

    editor.drive_editor_frame(Duration::ZERO);
    let frame = editor.current_frame();
    let result = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: current.revision,
            })],
        ))
        .unwrap();
    assert!(
        matches!(&result.outcomes()[0], EditorActionOutcome::Workspace(report) if report.applied)
    );
    assert_eq!(
        editor
            .workspace()
            .prefab(document)
            .unwrap()
            .session()
            .document(),
        &original
    );
    assert!(matches!(
        editor.commit_authoring_edit(stale),
        EditorActionOutcome::Workspace(report) if !report.applied,
    ));
    assert_eq!(fs::read(&path).unwrap(), disk);
    let _ = drive_editor_close_to_completion(&mut editor);
}
