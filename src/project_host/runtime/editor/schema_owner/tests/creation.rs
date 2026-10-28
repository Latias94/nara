//! Actual metadata publication and recovery keep creation-only changes in the existing owner.

use nara_reflect::ComponentSchemaOwnerChange;

use super::*;

const SUCCESSOR: &str = ".nara/source-tools/artifacts/stats-creation";

fn adopt_initial(fixture: &Fixture, editor: &mut EditorProjectSession) {
    let operation = fixture.review(editor);
    editor.adopt_schema_owner(operation).unwrap();
    assert_eq!(terminal(editor), EditorSchemaOwnerAdoptionOutcome::Adopted);
    editor.acknowledge_schema_owner_adoption(operation).unwrap();
    wait(editor, |editor| !editor.project_source_index_work_pending());
}

fn install_successor(fixture: &Fixture) -> ContentDigest {
    let bundle = AuthoringSchemaBundle::from_json_bytes(&fixture.bytes(BUNDLE)).unwrap();
    let previous = bundle
        .owner(&PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap())
        .unwrap();
    let mut current = ComponentSchemaCatalog::successor_of(previous.current()).unwrap();
    current.components = previous.current().components().to_vec();
    current.components[0].authoring_initial_value =
        Some(ComponentValue::map([("health", ComponentValue::I64(250))]));
    let successor = ComponentSchemaOwnerSnapshot::admit(
        previous.owner_id().clone(),
        current,
        Some(previous.current().clone()),
    )
    .unwrap();
    install_owner(&fixture.root, "stats-creation", &successor)
}

fn review_creation(
    editor: &mut EditorProjectSession,
    receipt: ContentDigest,
) -> EditorSchemaOwnerAdoptionId {
    let operation = editor
        .prepare_installed_schema_owner(
            SUCCESSOR,
            receipt,
            ComponentSchemaOwnerChange::CreationMetadata,
        )
        .unwrap();
    wait(editor, |editor| {
        matches!(
            editor.schema_owner_adoption_view().phase,
            EditorSchemaOwnerAdoptionPhase::Review | EditorSchemaOwnerAdoptionPhase::Terminal,
        )
    });
    let view = editor.schema_owner_adoption_view();
    assert_eq!(
        view.phase,
        EditorSchemaOwnerAdoptionPhase::Review,
        "{view:?}"
    );
    assert_eq!(view.change, ComponentSchemaOwnerChange::CreationMetadata);
    operation
}

fn assert_successor(fixture: &Fixture, editor: &EditorProjectSession, document: EditorDocumentId) {
    let bundle = AuthoringSchemaBundle::from_json_bytes(&fixture.bytes(BUNDLE)).unwrap();
    let owner = bundle
        .owner(&PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap())
        .unwrap();
    assert_eq!(owner.current().generation(), 2);
    assert_eq!(
        owner
            .schema(&component())
            .unwrap()
            .authoring_initial_value(),
        Some(&ComponentValue::map(
            [("health", ComponentValue::I64(250)),]
        ))
    );
    let stored = &editor
        .workspace()
        .scene(document)
        .unwrap()
        .session()
        .document()
        .entities[0]
        .components[&component()];
    assert_eq!(
        stored.value,
        ComponentValue::map([("health", ComponentValue::I64(100))])
    );
}

#[test]
fn creation_successor_preserves_dirty_source_history_and_binds_a_fresh_basis() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        adopt_initial(&fixture, editor);
        let receipt = install_successor(&fixture);
        let document = startup(editor);
        name_edit(editor, document, "kept-undo");
        name_edit(editor, document, "kept-redo");
        let base_revision = editor.document_query_basis(document).unwrap().revision;
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision
                })
                .applied
        );
        let inactive = second(editor);
        name_edit(editor, inactive, "inactive-dirty");
        let before = source_state(editor, document);
        let inactive_before = source_state(editor, inactive);
        let old_basis = editor.document_query_basis(document).unwrap();
        let disk = fixture.bytes(BUNDLE);
        let operation = review_creation(editor, receipt);
        assert_eq!(fixture.bytes(BUNDLE), disk);
        assert_eq!(source_state(editor, document), before);
        editor.adopt_schema_owner(operation).unwrap();
        assert_eq!(terminal(editor), EditorSchemaOwnerAdoptionOutcome::Adopted);
        assert_eq!(source_state(editor, document), before);
        assert_eq!(source_state(editor, inactive), inactive_before);
        assert!(Arc::ptr_eq(
            &editor
                .workspace()
                .scene(document)
                .unwrap()
                .session()
                .shared_document(),
            &before.document
        ));
        assert_ne!(
            editor.document_query_basis(document).unwrap().schema,
            old_basis.schema
        );
        assert_successor(&fixture, editor, document);
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        let base_revision = editor.document_query_basis(document).unwrap().revision;
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Redo {
                    document,
                    base_revision
                })
                .applied
        );
        assert_eq!(
            editor
                .workspace()
                .scene(document)
                .unwrap()
                .session()
                .history_status()
                .undo_depth,
            2
        );
    });
}

#[test]
fn creation_publication_cancel_retains_the_new_disk_owner_and_reconciles_with_original_intent() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        adopt_initial(&fixture, editor);
        let receipt = install_successor(&fixture);
        let document = startup(editor);
        name_edit(editor, document, "preserved-cancelled-evolution");
        let before = source_state(editor, document);
        let old_basis = editor.document_query_basis(document).unwrap();
        let operation = review_creation(editor, receipt);
        let mut held = HeldPublication::arm(editor);
        editor.adopt_schema_owner(operation).unwrap();
        let returned = publication_returned(editor);
        held.wait_until_entered();
        held.assert_pending(editor, &returned);
        held.poll(|| editor.cancel_schema_owner_adoption(operation))
            .unwrap();
        assert_eq!(source_state(editor, document), before);
        held.release();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption
        );
        assert_eq!(editor.document_query_basis(document).unwrap(), old_basis);
        let bytes = fixture.bytes(BUNDLE);
        let reconciliation = editor.reconcile_schema_owner_adoption(operation).unwrap();
        wait(editor, |editor| {
            matches!(
                editor.schema_owner_adoption_view().phase,
                EditorSchemaOwnerAdoptionPhase::Review | EditorSchemaOwnerAdoptionPhase::Terminal
            )
        });
        assert_eq!(
            editor.schema_owner_adoption_view().phase,
            EditorSchemaOwnerAdoptionPhase::Review
        );
        assert_eq!(
            editor.schema_owner_adoption_view().change,
            ComponentSchemaOwnerChange::CreationMetadata
        );
        editor.adopt_schema_owner(reconciliation).unwrap();
        assert_eq!(terminal(editor), EditorSchemaOwnerAdoptionOutcome::Adopted);
        assert_eq!(fixture.bytes(BUNDLE), bytes);
        assert_eq!(source_state(editor, document), before);
        assert_successor(&fixture, editor, document);
    });
}

#[test]
fn creation_review_fences_source_edits_until_cancel_and_never_writes_the_successor() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        adopt_initial(&fixture, editor);
        let receipt = install_successor(&fixture);
        let document = startup(editor);
        let original = fixture.bytes(BUNDLE);
        let operation = review_creation(editor, receipt);
        let before = source_state(editor, document);
        let report =
            editor.apply_workspace_command(EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
                document,
                base_revision: before.revision,
                edit: AuthoringFieldEdit::single(
                    entity(),
                    name_component(),
                    ComponentSchemaVersion::ONE,
                    ComponentFieldId::new("value"),
                    ComponentValue::String("refused-during-review".into()),
                )
                .unwrap(),
            });
        assert!(!report.applied);
        assert_eq!(source_state(editor, document), before);
        editor.cancel_schema_owner_adoption(operation).unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Cancelled
        );
        name_edit(editor, document, "allowed-after-cancel");
        assert_eq!(fixture.bytes(BUNDLE), original);
    });
}
