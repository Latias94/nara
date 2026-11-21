use std::path::Path;

use nara::scene::{SceneFileLimits, SceneFormatError};

use super::*;

const AUDIT_ROWS: u64 = 39_500;
const DEFAULT_ROWS: u64 = 86;
const FILE_LIMIT: usize = 16 * 1024 * 1024;
const INITIAL_AMOUNT: u64 = 7;
const EDITED_AMOUNT: u64 = 8;

#[test]
fn a_registered_panel_round_trips_ron_scene_edits_beyond_the_json_limit() {
    round_trip_ron(
        EditorDocumentKind::Scene,
        "scenes/large-inventory.scene.ron",
    );
}

#[test]
fn a_registered_panel_round_trips_ron_prefab_edits_beyond_the_json_limit() {
    round_trip_ron(
        EditorDocumentKind::Prefab,
        "prefabs/large-inventory.prefab.ron",
    );
}

#[test]
fn a_registered_panel_round_trips_an_omitted_scene_default_edit() {
    round_trip_omitted_default(
        EditorDocumentKind::Scene,
        "scenes/default-inventory.scene.json",
    );
}

#[test]
fn a_registered_panel_round_trips_an_omitted_prefab_default_edit() {
    round_trip_omitted_default(
        EditorDocumentKind::Prefab,
        "prefabs/default-inventory.prefab.json",
    );
}

fn inventory_rows(count: u64, last_balance: u64) -> ComponentValue {
    let row = |key, balance| {
        ComponentValue::map([
            ("row_id", ComponentValue::U64(key)),
            ("balance", ComponentValue::U64(balance)),
        ])
    };
    ComponentValue::List(
        (1..count)
            .map(|key| row(key, key))
            .chain([row(count, last_balance)])
            .collect(),
    )
}

fn inventory_entity(amount: u64, rows: Option<ComponentValue>) -> SceneEntityRecord {
    let mut fields = vec![("amount", ComponentValue::U64(amount))];
    if let Some(rows) = rows {
        fields.push(("inventory", rows));
    }
    SceneEntityRecord::new(id("anchor")).with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::map(fields))
            .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    )
}

fn inventory_schema(default: Option<ComponentValue>) -> AuthoringSchemaBundle {
    let field = ComponentFieldId::new("rows");
    let path = ComponentFieldPath::from_fields(["inventory"]);
    let shape = ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([
            ComponentShapeMember::new("row_id", ComponentValueKind::U64)
                .with_id(ComponentFieldId::new("id")),
            ComponentShapeMember::new("balance", ComponentValueKind::U64)
                .with_id(ComponentFieldId::new("balance")),
        ]),
        ComponentFieldId::new("id"),
    );
    let rows = match default {
        Some(value) => {
            ComponentFieldSchema::optional_with_default(field, "Inventory", path, shape, value)
        }
        None => ComponentFieldSchema::required(field, "Inventory", path, shape),
    };
    schema_bundle([
        rows.with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("amount"),
            "Amount",
            ComponentFieldPath::from_fields(["amount"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn ron_source(kind: EditorDocumentKind, amount: u64, last_balance: u64) -> String {
    let entities = [inventory_entity(
        amount,
        Some(inventory_rows(AUDIT_ROWS, last_balance)),
    )];
    let (ron, json) = match kind {
        EditorDocumentKind::Scene => {
            let document = SceneDocument::from_ordered_roots(entities);
            (document.to_ron_string().unwrap(), document.to_json_string())
        }
        EditorDocumentKind::Prefab => {
            let document = PrefabDocument::from_ordered_roots(entities);
            (document.to_ron_string().unwrap(), document.to_json_string())
        }
        EditorDocumentKind::Extension(_) => unreachable!(),
    };
    assert!(ron.len() < FILE_LIMIT, "RON has {} bytes", ron.len());
    let rejected = json
        .err()
        .expect("the explicit JSON encoder must refuse the declaration");
    assert!(
        matches!(
            rejected,
            SceneFormatError::EncodedBytesExceeded { maximum: FILE_LIMIT, observed }
                if observed > FILE_LIMIT
        ),
        "expected the encoded byte limit, got {rejected:?}"
    );
    ron
}

fn json_source(kind: EditorDocumentKind, rows: Option<ComponentValue>) -> String {
    let entities = [inventory_entity(INITIAL_AMOUNT, rows)];
    match kind {
        EditorDocumentKind::Scene => SceneDocument::from_ordered_roots(entities).to_json_string(),
        EditorDocumentKind::Prefab => PrefabDocument::from_ordered_roots(entities).to_json_string(),
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
    .unwrap()
}

fn inventory_read(basis: &EditorDocumentQueryBasis, row: Option<u64>) -> EditorPropertyRead {
    let property = match row {
        Some(key) => ComponentPropertyPath::new(ComponentFieldId::new("rows")).with_steps([
            ComponentPropertyStep::ListItem {
                by: ComponentFieldId::new("id"),
                key: ComponentPropertyKey::U64(key),
            },
            ComponentPropertyStep::Member(ComponentFieldId::new("balance")),
        ]),
        None => ComponentPropertyPath::new(ComponentFieldId::new("amount")),
    };
    selected_read(basis, property)
}

fn prepare_numeric_edit(
    context: &EditorPanelContext<'_, '_>,
    request: EditorPropertyRead,
    before: EditorPropertyValue,
    after: u64,
) -> PreparedEditorDocumentEdit {
    let observed = context.read_document_property(request.clone()).unwrap();
    assert_eq!(observed.target, request.target);
    assert_eq!(observed.value, before);
    let value = ComponentValue::U64(after);
    let prepared = context
        .prepare_authoring_property_edit(
            &request.target.basis,
            &[property_edit(&request, value.clone())],
        )
        .unwrap();
    assert_selected_scalar(
        &prepared.preview().read_property(request.clone()).unwrap(),
        &request,
        value,
    );
    prepared
}

fn assert_ron_values(
    app: &EditorApplication,
    document: EditorDocumentId,
    amount: u64,
    balance: u64,
) {
    let captured = basis(app, document);
    for (row, value) in [(None, amount), (Some(AUDIT_ROWS), balance)] {
        let request = inventory_read(&captured, row);
        assert_selected_scalar(
            &app.read_document_property(request.clone()).unwrap(),
            &request,
            ComponentValue::U64(value),
        );
    }
}

fn assert_disk(path: &Path, expected: &str) {
    let bytes = fs::read(path).unwrap();
    assert!(
        bytes.as_slice() == expected.as_bytes(),
        "{} differs from the expected declaration ({} actual bytes, {} expected)",
        path.display(),
        bytes.len(),
        expected.len()
    );
}

fn save(
    app: &mut EditorApplication,
    document: EditorDocumentId,
    path: &Path,
    expected: &str,
) -> EditorDocumentDigest {
    let base_revision = basis(app, document).revision;
    let result = persist(
        app,
        EditorPersistenceAction::Save {
            document,
            base_revision,
        },
    );
    let EditorPersistenceResult::Saved {
        document: saved,
        revision,
        digest,
    } = result
    else {
        panic!("the edited source must save: {result:?}");
    };
    assert_eq!(saved, document);
    assert_eq!(revision, base_revision);
    assert_eq!(digest.encoded_bytes(), expected.len() as u64);
    let current = frame(app);
    let tab = current.workspace().document(document).unwrap();
    assert_eq!(tab.saved_revision, revision);
    assert!(!tab.dirty);
    assert_disk(path, expected);
    digest
}

fn reopen(app: &mut EditorApplication, document: EditorDocumentId, digest: EditorDocumentDigest) {
    let original = basis(app, document).revision;
    let result = persist(
        app,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: original,
            },
        },
    );
    let current = frame(app);
    let tab = current.workspace().document(document).unwrap();
    assert_eq!(
        result,
        EditorPersistenceResult::Opened {
            document,
            revision: tab.revision,
            digest,
        }
    );
    assert_ne!(tab.revision.source_id(), original.source_id());
    assert_eq!(tab.saved_revision, tab.revision);
    assert!(!tab.dirty);
    assert_eq!((tab.history.undo_depth, tab.history.redo_depth), (0, 0));
}

fn replay_history(app: &mut EditorApplication, document: EditorDocumentId, redo: bool) {
    let base_revision = basis(app, document).revision;
    let mutation = if redo {
        EditorDocumentMutation::Redo {
            document,
            base_revision,
        }
    } else {
        EditorDocumentMutation::Undo {
            document,
            base_revision,
        }
    };
    let result = submit(app, EditorAction::Mutate(mutation));
    let current = frame(app);
    let tab = current.workspace().document(document).unwrap();
    assert!(
        matches!(
            result.actions(),
            [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
                if report.applied && report.document == Some(document)
                    && report.revision == Some(tab.revision)
        ),
        "history replay must publish its candidate: {result:?}"
    );
    assert_ne!(tab.revision, base_revision);
}

fn round_trip_ron(kind: EditorDocumentKind, source: &str) {
    assert_eq!(SceneFileLimits::default().encoded_bytes().get(), FILE_LIMIT);
    let project = text_project::TextProject::new();
    let original = ron_source(kind, INITIAL_AMOUNT, AUDIT_ROWS);
    let (mut app, document, startup) =
        open_nonactive_source_text(&project, kind, source, inventory_schema(None), &original);
    let path = project.root.join(source);
    let initial = frame(&app);
    let (mut host, panel) = register_panel(&mut app);
    assert_ron_values(&app, document, INITIAL_AMOUNT, AUDIT_ROWS);

    review_and_commit_keyed_edit(&mut app, &mut host, panel, document, |context, captured| {
        prepare_numeric_edit(
            context,
            inventory_read(captured, None),
            EditorPropertyValue::Present(ComponentValue::U64(INITIAL_AMOUNT)),
            EDITED_AMOUNT,
        )
    });
    assert_disk(&path, &original);
    assert_ron_values(&app, document, EDITED_AMOUNT, AUDIT_ROWS);
    let scalar_only = ron_source(kind, EDITED_AMOUNT, AUDIT_ROWS);
    assert_eq!(scalar_only.len(), original.len());
    save(&mut app, document, &path, &scalar_only);

    review_and_commit_keyed_edit(&mut app, &mut host, panel, document, |context, captured| {
        prepare_numeric_edit(
            context,
            inventory_read(captured, Some(AUDIT_ROWS)),
            EditorPropertyValue::Present(ComponentValue::U64(AUDIT_ROWS)),
            AUDIT_ROWS + 1,
        )
    });
    assert_disk(&path, &scalar_only);
    drop(scalar_only);
    assert_ron_values(&app, document, EDITED_AMOUNT, AUDIT_ROWS + 1);
    let edited = ron_source(kind, EDITED_AMOUNT, AUDIT_ROWS + 1);
    let digest = save(&mut app, document, &path, &edited);

    replay_history(&mut app, document, false);
    assert_ron_values(&app, document, EDITED_AMOUNT, AUDIT_ROWS);
    replay_history(&mut app, document, false);
    assert_ron_values(&app, document, INITIAL_AMOUNT, AUDIT_ROWS);
    assert_disk(&path, &edited);
    save(&mut app, document, &path, &original);
    replay_history(&mut app, document, true);
    assert_ron_values(&app, document, EDITED_AMOUNT, AUDIT_ROWS);
    replay_history(&mut app, document, true);
    assert_ron_values(&app, document, EDITED_AMOUNT, AUDIT_ROWS + 1);
    assert_disk(&path, &original);
    drop(original);
    assert_eq!(save(&mut app, document, &path, &edited), digest);

    reopen(&mut app, document, digest);
    assert_ron_values(&app, document, EDITED_AMOUNT, AUDIT_ROWS + 1);
    assert_disk(&path, &edited);
    review_and_commit_keyed_edit(&mut app, &mut host, panel, document, |context, captured| {
        prepare_numeric_edit(
            context,
            inventory_read(captured, Some(AUDIT_ROWS)),
            EditorPropertyValue::Present(ComponentValue::U64(AUDIT_ROWS + 1)),
            AUDIT_ROWS + 2,
        )
    });
    assert_ron_values(&app, document, EDITED_AMOUNT, AUDIT_ROWS + 2);
    assert_disk(&path, &edited);
    drop(edited);
    let final_source = ron_source(kind, EDITED_AMOUNT, AUDIT_ROWS + 2);
    save(&mut app, document, &path, &final_source);
    assert_navigation(&app, &initial, document);
    assert_disk(&project.root.join(STARTUP), &startup);
    assert!(app.close(), "the RON project did not physically retire");
}

fn round_trip_omitted_default(kind: EditorDocumentKind, source: &str) {
    let project = text_project::TextProject::new();
    let original = json_source(kind, None);
    let schema = inventory_schema(Some(inventory_rows(DEFAULT_ROWS, DEFAULT_ROWS)));
    let (mut app, document, startup) =
        open_nonactive_source_text(&project, kind, source, schema, &original);
    let path = project.root.join(source);
    let initial = frame(&app);
    let (mut host, panel) = register_panel(&mut app);
    let defaulted = EditorPropertyValue::Defaulted(ComponentValue::U64(DEFAULT_ROWS));
    let replacement = DEFAULT_ROWS - 1;
    let present = EditorPropertyValue::Present(ComponentValue::U64(replacement));
    let assert_value = |app: &EditorApplication, expected: &EditorPropertyValue| {
        let request = inventory_read(&basis(app, document), Some(DEFAULT_ROWS));
        let observed = app.read_document_property(request.clone()).unwrap();
        assert_eq!(observed.target, request.target);
        assert_eq!(&observed.value, expected);
    };

    review_and_commit_keyed_edit(&mut app, &mut host, panel, document, |context, captured| {
        prepare_numeric_edit(
            context,
            inventory_read(captured, Some(DEFAULT_ROWS)),
            defaulted.clone(),
            replacement,
        )
    });
    assert_value(&app, &present);
    assert_disk(&path, &original);
    let edited = json_source(kind, Some(inventory_rows(DEFAULT_ROWS, replacement)));
    let digest = save(&mut app, document, &path, &edited);

    replay_history(&mut app, document, false);
    assert_value(&app, &defaulted);
    assert_disk(&path, &edited);
    save(&mut app, document, &path, &original);
    replay_history(&mut app, document, true);
    assert_value(&app, &present);
    assert_disk(&path, &original);
    assert_eq!(save(&mut app, document, &path, &edited), digest);

    replay_history(&mut app, document, false);
    let omitted_digest = save(&mut app, document, &path, &original);
    reopen(&mut app, document, omitted_digest);
    assert_value(&app, &defaulted);
    assert_disk(&path, &original);

    review_and_commit_keyed_edit(&mut app, &mut host, panel, document, |context, captured| {
        prepare_numeric_edit(
            context,
            inventory_read(captured, Some(DEFAULT_ROWS)),
            defaulted.clone(),
            replacement,
        )
    });
    assert_eq!(save(&mut app, document, &path, &edited), digest);
    reopen(&mut app, document, digest);
    assert_value(&app, &present);
    assert_disk(&path, &edited);
    assert_navigation(&app, &initial, document);
    assert_disk(&project.root.join(STARTUP), &startup);
    assert!(
        app.close(),
        "the default inventory project did not physically retire"
    );
}
