//! Independent public Host conversion, history, persistence and stale-file qualification.
//! Drives real file-backed work without qualifying native presentation or controlled worker races.

use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use nara_asset::AssetRef;
use nara_engine::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_host::{
        EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession, advanced::built_in_schema_providers,
    },
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, PrefabInstancePath,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
};
use nara_tooling::{
    CommittedEditorPrefabConversion, EditorAction, EditorActionBatch, EditorActionOutcome,
    EditorDocumentId, EditorDocumentKind, EditorDocumentMutation, EditorDocumentOpenResult,
    EditorDocumentQueryBasis, EditorDocumentTabModel, EditorEntityQuery, EditorEntityQueryLimits,
    EditorEntityQueryPosition, EditorNavigationAction, EditorPersistenceAction,
    EditorPersistenceRequestResult, EditorPersistenceResult, EditorPersistenceView,
    EditorPrefabConversionAction, EditorPrefabConversionActionOutcome, EditorPrefabConversionId,
    EditorPrefabConversionPhase as Phase, EditorPrefabConversionRejection,
    EditorPrefabConversionResult, EditorPrefabConversionReview, EditorProjectBrowserAction,
    EditorProjectBrowserActionOutcome, EditorPropertyRead, EditorPropertyReadLimits,
    EditorPropertyTarget, EditorPropertyValue, EditorSelectionSet,
};

const SCENE: &str = "scenes/startup.scene.json";
const PREFAB: &str = "prefabs/container.prefab.json";
const BASE: &str = "prefabs/base.prefab.json";
const WAIT: Duration = Duration::from_secs(15);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn payload_type() -> ComponentTypeId {
    ComponentTypeId::new("consumer.HostConversionPayload")
}

fn payload_owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("consumer.host-conversion").unwrap()
}

fn record(entity: SceneEntityId, amount: u64) -> SceneEntityRecord {
    SceneEntityRecord::new(entity).with_component(
        payload_type(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("amount", ComponentValue::U64(amount))]),
        )
        .with_owner(payload_owner()),
    )
}

fn original() -> SceneDocument {
    let mut anchor = record(id("anchor"), 10).with_children([id("local")]);
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("base.prefab.json").unwrap(),
        overrides: None,
    });
    SceneDocument::new([id("anchor")], [anchor, record(id("local"), 20)])
}

fn base(amount: u64) -> PrefabDocument {
    PrefabDocument::from_ordered_roots([
        record(id("leaf-0"), amount),
        record(id("leaf-1"), amount + 1),
    ])
}

struct Project {
    root: tempfile::TempDir,
}

impl Project {
    fn new() -> Self {
        let project = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let root = project.root.path();
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            "schema_version = 1\n\
             [project]\nname = 'Independent Host conversion'\n\
             source_universe = '8227db48-ec36-52ac-babb-e72ef706e665'\n\
             native_document_identity = 'project_location'\n\
             [paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n\
             [startup]\ndefault_scene = 'startup.scene.json'\n\
             [authoring]\nschema_bundle = 'schema/authoring.bundle.json'\n\
             [runtime]\npreset = 'local-headless'\n\
             [capabilities]\nrequested = ['runtime-2d']\n",
        )
        .unwrap();
        let builtins =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap();
        let schema = ComponentSchema::new(
            payload_type(),
            "Consumer payload without a native binding",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("amount"),
            "Amount",
            ComponentFieldPath::from_fields(["amount"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
        let bundle = AuthoringSchemaBundle::new(
            builtins
                .owners()
                .iter()
                .map(AuthoringSchemaBundleOwner::from)
                .chain([AuthoringSchemaBundleOwner::new(
                    payload_owner(),
                    ComponentSchemaCatalog {
                        components: vec![schema],
                        ..Default::default()
                    },
                    None,
                )]),
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(root.join(SCENE), original().to_json_string().unwrap()).unwrap();
        fs::write(
            root.join(PREFAB),
            PrefabDocument::new(original().roots, original().entities)
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(root.join(BASE), base(100).to_json_string().unwrap()).unwrap();
        project
    }

    fn capability(&self) -> DirectoryCapability {
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(self.root.path())
                .unwrap()
        };
        #[cfg(not(windows))]
        let handle = fs::File::open(self.root.path()).unwrap();
        DirectoryCapability::from_host_handle(
            handle,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }

    fn open(&self) -> EditorProjectSession {
        let mut host = EditorProjectOpenHost::new();
        let mut attempt = host.begin(self.capability(), None);
        let deadline = Instant::now() + WAIT;
        loop {
            match host.advance(attempt) {
                EditorProjectOpenProgress::Opened(mut editor) => {
                    loop {
                        let report = host.poll_shutdown();
                        if report.is_complete() {
                            assert!(!report.has_cleanup_failure(), "{report:?}");
                            break;
                        }
                        assert!(Instant::now() < deadline, "Open Host did not retire");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    editor.drive_editor_frame(Duration::ZERO);
                    return editor;
                }
                EditorProjectOpenProgress::Pending(next) => attempt = next,
                EditorProjectOpenProgress::Rejected(error) => {
                    panic!("public project Open failed: {:?}", error.diagnostics());
                }
            }
            assert!(Instant::now() < deadline, "public project Open timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let frame = editor.current_frame().revision();
    let mut outcomes = editor
        .dispatch_editor_actions(EditorActionBatch::new(frame, [action]))
        .unwrap()
        .into_outcomes()
        .into_iter();
    let outcome = outcomes.next().unwrap();
    assert!(outcomes.next().is_none());
    outcome
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "Host timed out: {:?}; persistence: {:?}",
            editor.prefab_conversion_view(),
            editor.persistence_view(),
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn applied(outcome: EditorActionOutcome) {
    assert!(
        matches!(&outcome, EditorActionOutcome::Workspace(report) if report.applied),
        "{outcome:?}"
    );
}

fn open_source(editor: &mut EditorProjectSession, path: &str) -> EditorDocumentId {
    let document = if path == SCENE {
        editor.current_frame().scene_preview_document().unwrap()
    } else {
        wait(editor, |editor| {
            !editor.current_frame().project_browser().has_pending_work()
        });
        let outcome = dispatch(
            editor,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
                relative_path: path.into(),
            }),
        );
        let EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
            request,
        }) = outcome
        else {
            panic!("public Browser Open refused {path}: {outcome:?}");
        };
        wait(editor, |editor| {
            editor.current_frame().document_open().result().is_some()
        });
        let result = editor.current_frame().document_open().result().unwrap();
        let EditorDocumentOpenResult::Opened {
            request: completed,
            document,
        } = result
        else {
            panic!("public Browser Open failed: {result:?}");
        };
        assert_eq!(completed, request);
        assert!(matches!(
            dispatch(
                editor,
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
                    request,
                }),
            ),
            EditorActionOutcome::ProjectBrowser(
                EditorProjectBrowserActionOutcome::OpenAcknowledged { request: acknowledged }
            ) if acknowledged == request
        ));
        document
    };
    for action in [
        EditorNavigationAction::SetActiveDocument { document },
        EditorNavigationAction::SelectEntity {
            document,
            entity: Some(id("anchor")),
        },
    ] {
        applied(dispatch(editor, EditorAction::Navigate(action)));
        editor.drive_editor_frame(Duration::ZERO);
    }
    document
}

fn basis(editor: &EditorProjectSession, document: EditorDocumentId) -> EditorDocumentQueryBasis {
    let frame = editor.current_frame();
    let tab = frame.workspace().document(document).unwrap();
    let basis = EditorDocumentQueryBasis {
        document,
        revision: tab.revision,
        schema: tab.authoring_schema_basis.clone(),
    };
    assert_eq!(editor.document_query_basis(document).unwrap(), basis);
    basis
}

#[derive(Debug, PartialEq)]
struct SourceState {
    tab: EditorDocumentTabModel,
    selection: EditorSelectionSet,
    tree: Vec<(SceneEntityId, Option<SceneEntityId>)>,
    records: Vec<(SceneEntityId, u64)>,
    instance: Option<PrefabInstance>,
}

fn state(editor: &EditorProjectSession, document: EditorDocumentId) -> SourceState {
    let basis = basis(editor, document);
    let frame = editor.current_frame();
    assert_eq!(frame.workspace().active_document, Some(document));
    let active = frame.workspace().active_scene.as_ref().unwrap();
    assert_eq!(active.document, document);
    let page = editor
        .query_document_entities(EditorEntityQuery {
            position: EditorEntityQueryPosition::Start(basis.clone()),
            limits: EditorEntityQueryLimits::new(16, 16).unwrap(),
        })
        .unwrap();
    assert_eq!(page.basis, basis);
    assert!(page.next.is_none(), "this finite fixture must fit one page");
    let records = page
        .entities
        .into_iter()
        .map(|entity| {
            let observed = editor
                .read_document_property(EditorPropertyRead {
                    target: EditorPropertyTarget {
                        basis: basis.clone(),
                        entity: entity.clone(),
                        component: payload_type(),
                        property: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
                    },
                    limits: EditorPropertyReadLimits::new(4096, 16, 4096).unwrap(),
                })
                .unwrap();
            let EditorPropertyValue::Present(ComponentValue::U64(amount)) = observed.value else {
                panic!("schema-only payload was not retained: {observed:?}");
            };
            (entity, amount)
        })
        .collect();
    let declaration = editor
        .observe_prefab_instance(
            &basis,
            &PrefabInstancePath::new(id("anchor")),
            Default::default(),
        )
        .unwrap();
    assert_eq!(declaration.revision, basis.revision);
    assert_eq!(Some(declaration.schema), basis.schema);
    assert!(declaration.nested_instances.is_empty());
    SourceState {
        tab: frame.workspace().document(document).unwrap().clone(),
        selection: active.selection.clone(),
        tree: active
            .editor
            .inspector
            .entities
            .iter()
            .map(|row| (row.id.clone(), row.parent.clone()))
            .collect(),
        records,
        instance: declaration.instance,
    }
}

fn conversion(
    editor: &mut EditorProjectSession,
    action: EditorPrefabConversionAction,
) -> EditorPrefabConversionActionOutcome {
    match dispatch(editor, EditorAction::PrefabConversion(action)) {
        EditorActionOutcome::PrefabConversion(outcome) => outcome,
        other => panic!("conversion action was not routed to its owner: {other:?}"),
    }
}

fn review(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
) -> (EditorPrefabConversionId, Arc<EditorPrefabConversionReview>) {
    let basis = basis(editor, document);
    let outcome = conversion(
        editor,
        EditorPrefabConversionAction::Prepare {
            basis: basis.clone(),
            anchor: id("anchor"),
        },
    );
    let EditorPrefabConversionActionOutcome::Accepted { operation, phase } = outcome else {
        panic!("public conversion Prepare refused: {outcome:?}");
    };
    assert_eq!(phase, Phase::Preparing);
    assert!(editor.prefab_conversion_view().result.is_none());
    wait(editor, |editor| {
        matches!(
            editor.current_frame().prefab_conversion().phase,
            Phase::Review | Phase::Terminal
        )
    });
    let frame = editor.current_frame();
    let view = frame.prefab_conversion();
    assert_eq!(view.phase, Phase::Review, "{view:?}");
    assert_eq!(view.operation, Some(operation));
    assert_eq!(view.document, Some(document));
    assert_eq!(view.anchor.as_deref(), Some("anchor"));
    assert!(view.result.is_none());
    let page = view.review.clone().unwrap();
    assert_eq!(page.basis, basis);
    assert_eq!(page.total_entities, 2);
    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.offset, 0);
    assert!(page.next_offset.is_none());
    assert_eq!((page.source_count, page.working_copy_count), (1, 0));
    (operation, page)
}

fn commit(
    editor: &mut EditorProjectSession,
    operation: EditorPrefabConversionId,
) -> EditorPrefabConversionResult {
    assert_eq!(
        conversion(editor, EditorPrefabConversionAction::Commit { operation }),
        EditorPrefabConversionActionOutcome::Accepted {
            operation,
            phase: Phase::Validating,
        },
    );
    assert!(editor.prefab_conversion_view().result.is_none());
    wait(editor, |editor| {
        editor.current_frame().prefab_conversion().phase == Phase::Terminal
    });
    let frame = editor.current_frame();
    assert_eq!(frame.prefab_conversion().operation, Some(operation));
    frame.prefab_conversion().result.clone().unwrap()
}

fn acknowledge(editor: &mut EditorProjectSession, operation: EditorPrefabConversionId) {
    assert_eq!(
        conversion(
            editor,
            EditorPrefabConversionAction::Acknowledge { operation }
        ),
        EditorPrefabConversionActionOutcome::Accepted {
            operation,
            phase: Phase::Idle,
        },
    );
    let frame = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(frame.prefab_conversion().phase, Phase::Idle);
    assert!(frame.prefab_conversion().result.is_none());
    assert!(frame.prefab_conversion().operation.is_none());
}

fn replay(editor: &mut EditorProjectSession, document: EditorDocumentId, undo: bool) {
    let base_revision = basis(editor, document).revision;
    let mutation = if undo {
        EditorDocumentMutation::Undo {
            document,
            base_revision,
        }
    } else {
        EditorDocumentMutation::Redo {
            document,
            base_revision,
        }
    };
    applied(dispatch(editor, EditorAction::Mutate(mutation)));
    editor.drive_editor_frame(Duration::ZERO);
}

fn persist(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    reopen: bool,
) -> EditorPersistenceResult {
    let captured = basis(editor, document).revision;
    let action = if reopen {
        EditorPersistenceAction::Reopen {
            target: editor.current_frame().reopen_target(),
        }
    } else {
        EditorPersistenceAction::Save {
            document,
            base_revision: captured,
        }
    };
    let outcome = dispatch(editor, EditorAction::Persistence(action));
    let EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt }) =
        outcome
    else {
        panic!("public persistence request refused: {outcome:?}");
    };
    wait(editor, |editor| {
        matches!(
            editor.current_frame().persistence(),
            EditorPersistenceView::Terminal { attempt: completed, .. } if completed == attempt
        )
    });
    let result = editor.current_frame().persistence().result().unwrap();
    match result {
        EditorPersistenceResult::Saved {
            document: saved,
            revision,
            ..
        } if !reopen => {
            assert_eq!(saved, document);
            assert_eq!(revision, captured);
        }
        EditorPersistenceResult::Opened {
            document: opened,
            revision,
            ..
        } if reopen => {
            assert_eq!(opened, document);
            assert_ne!(revision.source_id(), captured.source_id());
        }
        other => panic!("persistence did not succeed: {other:?}"),
    }
    assert!(matches!(
        dispatch(
            editor,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
        ),
        EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Acknowledged { attempt: acknowledged }
        ) if acknowledged == attempt
    ));
    editor.drive_editor_frame(Duration::ZERO);
    result
}

fn history(state: &SourceState, undo: usize, redo: usize, dirty: bool) {
    assert_eq!(
        (state.tab.history.undo_depth, state.tab.history.redo_depth),
        (undo, redo),
    );
    assert_eq!(state.tab.dirty, dirty);
}

fn expected_document(receipt: &CommittedEditorPrefabConversion) -> SceneDocument {
    assert_eq!(receipt.entities().len(), 2);
    let localized = |name: &str| {
        receipt
            .entities()
            .iter()
            .find(|row| row.provenance().local_entity_id() == &id(name))
            .unwrap()
            .entity()
            .clone()
    };
    let left = localized("leaf-0");
    let right = localized("leaf-1");
    assert_ne!(left, right);
    for entity in [&left, &right] {
        assert_ne!(entity, &id("anchor"));
        assert_ne!(entity, &id("local"));
    }
    SceneDocument::new(
        [id("anchor")],
        [
            record(id("anchor"), 10).with_children([left.clone(), right.clone(), id("local")]),
            record(id("local"), 20),
            record(left, 100),
            record(right, 101),
        ],
    )
}

fn close(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + WAIT;
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
            assert!(!receipt.has_cleanup_failure(), "{receipt:?}");
            return;
        }
        assert!(Instant::now() < deadline, "public Session Close timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn public_host_scene_and_prefab_conversion_survive_history_save_and_real_reopen() {
    for path in [SCENE, PREFAB] {
        let project = Project::new();
        let mut editor = project.open();
        let document = open_source(&mut editor, path);
        let before = state(&editor, document);
        assert_eq!(
            before.tab.kind,
            if path == SCENE {
                EditorDocumentKind::Scene
            } else {
                EditorDocumentKind::Prefab
            },
        );
        history(&before, 0, 0, false);
        assert_eq!(before.instance, original().entities[0].prefab);
        let original_file = fs::read(project.root.path().join(path)).unwrap();
        let dependency_file = fs::read(project.root.path().join(BASE)).unwrap();
        let idle_frame = editor.current_frame();
        let (operation, page) = review(&mut editor, document);
        assert_eq!(state(&editor, document), before);
        assert_eq!(
            fs::read(project.root.path().join(path)).unwrap(),
            original_file,
        );
        assert_eq!(idle_frame.prefab_conversion().phase, Phase::Idle);
        let review_frame = editor.current_frame();

        let result = commit(&mut editor, operation);
        let EditorPrefabConversionResult::Committed {
            operation: completed,
            receipt,
        } = result
        else {
            panic!("public conversion did not commit: {result:?}");
        };
        assert_eq!(completed, operation);
        assert!(receipt.report().applied, "{:?}", receipt.report());
        assert_eq!(receipt.report().document, Some(document));
        assert_eq!(
            receipt
                .entities()
                .iter()
                .map(|row| row.entity())
                .collect::<Vec<_>>(),
            page.rows.iter().map(|row| &row.entity).collect::<Vec<_>>(),
        );
        for (mapped, reviewed) in receipt.entities().iter().zip(&page.rows) {
            assert_eq!(
                expected_review_origin(mapped.provenance()).as_deref(),
                Some(reviewed.origin.as_ref()),
            );
            assert!(!reviewed.origin.contains("SceneEntityProvenance"));
        }
        let after = state(&editor, document);
        let expected = expected_document(&receipt);
        let expected_tree = std::iter::once((id("anchor"), None))
            .chain(
                expected
                    .entities
                    .iter()
                    .find(|row| row.id == id("anchor"))
                    .unwrap()
                    .children
                    .iter()
                    .map(|child| (child.clone(), Some(id("anchor")))),
            )
            .collect::<Vec<_>>();
        assert_eq!(after.tree, expected_tree);
        assert_eq!(
            after.records,
            expected
                .entities
                .iter()
                .map(|row| {
                    let ComponentValue::U64(amount) =
                        row.components[&payload_type()].value.get("amount").unwrap()
                    else {
                        panic!("invalid fixture payload");
                    };
                    (row.id.clone(), *amount)
                })
                .collect::<Vec<_>>(),
        );
        assert!(after.instance.is_none());
        assert_eq!(receipt.report().revision, Some(after.tab.revision));
        assert_ne!(after.tab.revision, before.tab.revision);
        assert_eq!(
            after.tab.revision.source_id(),
            before.tab.revision.source_id(),
        );
        assert_eq!(after.tab.saved_revision, before.tab.saved_revision);
        assert_eq!(after.selection, before.selection);
        history(&after, 1, 0, true);
        assert_eq!(
            fs::read(project.root.path().join(path)).unwrap(),
            original_file,
        );

        replay(&mut editor, document, true);
        let undone = state(&editor, document);
        assert_eq!(undone.tree, before.tree);
        assert_eq!(undone.records, before.records);
        assert_eq!(undone.instance, before.instance);
        assert_eq!(undone.selection, before.selection);
        history(&undone, 0, 1, false);
        replay(&mut editor, document, false);
        let redone = state(&editor, document);
        assert_eq!(redone.tree, after.tree);
        assert_eq!(redone.records, after.records);
        assert_eq!(redone.instance, after.instance);
        assert_eq!(redone.selection, after.selection);
        history(&redone, 1, 0, true);
        acknowledge(&mut editor, operation);

        let saved_result = persist(&mut editor, document, false);
        let saved = fs::read(project.root.path().join(path)).unwrap();
        assert_ne!(saved, original_file);
        let (decoded, encoded_expected) = if path == SCENE {
            (
                SceneDocumentCandidate::decode_json_bytes(&saved)
                    .unwrap()
                    .to_json_string()
                    .unwrap(),
                expected.to_json_string().unwrap(),
            )
        } else {
            (
                PrefabDocumentCandidate::decode_json_bytes(&saved)
                    .unwrap()
                    .to_json_string()
                    .unwrap(),
                PrefabDocument::new(expected.roots, expected.entities)
                    .to_json_string()
                    .unwrap(),
            )
        };
        assert_eq!(decoded, encoded_expected);
        let saved_state = state(&editor, document);
        history(&saved_state, 1, 0, false);
        assert_eq!(saved_state.tab.saved_revision, saved_state.tab.revision);

        // Reopen must replace an unsaved Undo with the actual saved converted source.
        replay(&mut editor, document, true);
        let unsaved = state(&editor, document);
        assert_eq!(unsaved.tree, before.tree);
        assert_eq!(unsaved.records, before.records);
        assert_eq!(unsaved.instance, before.instance);
        history(&unsaved, 0, 1, true);
        assert_eq!(fs::read(project.root.path().join(path)).unwrap(), saved);
        let reopened_result = persist(&mut editor, document, true);
        let (
            EditorPersistenceResult::Saved {
                digest: saved_digest,
                ..
            },
            EditorPersistenceResult::Opened {
                digest: reopened_digest,
                ..
            },
        ) = (saved_result, reopened_result)
        else {
            panic!("the exact Save and Reopen terminals were required");
        };
        assert_eq!(saved_digest, reopened_digest);
        let reopened = state(&editor, document);
        assert_eq!(reopened.tree, after.tree);
        assert_eq!(reopened.records, after.records);
        assert_eq!(reopened.instance, after.instance);
        assert_ne!(
            reopened.tab.revision.source_id(),
            after.tab.revision.source_id(),
        );
        assert_eq!(reopened.tab.saved_revision, reopened.tab.revision);
        history(&reopened, 0, 0, false);
        assert_eq!(fs::read(project.root.path().join(path)).unwrap(), saved);
        assert_eq!(
            fs::read(project.root.path().join(BASE)).unwrap(),
            dependency_file,
        );
        assert_eq!(review_frame.prefab_conversion().phase, Phase::Review);
        assert!(review_frame.prefab_conversion().result.is_none());
        close(&mut editor);
    }
}

fn expected_review_origin(provenance: &nara_scene::SceneEntityProvenance) -> Option<String> {
    let path = provenance.projection_path()?;
    let mut segments = vec![path.root_anchor().to_string()];
    for step in path.steps() {
        match step {
            nara_scene::PrefabProjectionStep::SourceEntity {
                canonical_source,
                entity,
                ..
            } => {
                let nara_scene::SourceDocumentOrigin::ProjectLocation { path, .. } =
                    canonical_source.origin()
                else {
                    return None;
                };
                segments.push(format!("{path}:{entity}"));
            }
            nara_scene::PrefabProjectionStep::InstanceAddition { entity } => {
                segments.push(format!("added:{entity}"));
            }
        }
    }
    Some(segments.join(" / "))
}

#[test]
fn public_host_rejects_dependency_file_changed_after_review_without_source_or_history_changes() {
    for path in [SCENE, PREFAB] {
        let project = Project::new();
        let mut editor = project.open();
        let document = open_source(&mut editor, path);
        let before = state(&editor, document);
        history(&before, 0, 0, false);
        let original_file = fs::read(project.root.path().join(path)).unwrap();
        let original_dependency = fs::read(project.root.path().join(BASE)).unwrap();
        let (operation, _) = review(&mut editor, document);
        assert_eq!(state(&editor, document), before);

        let changed_dependency = base(900).to_json_string().unwrap().into_bytes();
        assert_ne!(changed_dependency, original_dependency);
        assert_eq!(changed_dependency.len(), original_dependency.len());
        fs::write(project.root.path().join(BASE), &changed_dependency).unwrap();
        let result = commit(&mut editor, operation);
        let terminal_frame = editor.current_frame();
        let EditorPrefabConversionResult::Failed {
            operation: completed,
            reason,
            diagnostics,
        } = result
        else {
            panic!("stale dependency was not rejected: {result:?}");
        };
        assert_eq!(completed, operation);
        assert_eq!(reason, EditorPrefabConversionRejection::ValidationFailed);
        assert!(diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(state(&editor, document), before);
        assert_eq!(
            fs::read(project.root.path().join(path)).unwrap(),
            original_file,
        );
        assert_eq!(
            fs::read(project.root.path().join(BASE)).unwrap(),
            changed_dependency,
        );
        for _ in 0..2 {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(
                frame.prefab_conversion(),
                terminal_frame.prefab_conversion()
            );
            assert_eq!(state(&editor, document), before);
        }
        acknowledge(&mut editor, operation);
        assert_eq!(state(&editor, document), before);
        close(&mut editor);
    }
}
