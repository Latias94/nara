//! Finite source-authoring proof through the public, window-free Editor owner.

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use nara_asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId};
use nara_engine::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    image::ImageImportSettings,
    project_host::{
        EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession,
    },
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentPropertyPath,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
};
use nara_scene::{
    AuthoringPropertyEdit, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, ScenePatchDocument, ScenePatchOperation,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorAuthoringProjection,
    EditorAuthoringQuery, EditorAuthoringQueryLimits, EditorAuthoringQueryPosition,
    EditorAuthoringQueryTarget, EditorAuthoringRow, EditorDocumentId, EditorDocumentMutation,
    EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorPropertyRead, EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
};

const MOVER: &str = "trial.behavior.Mover";
const SENSOR: &str = "trial.behavior.Sensor";
const LABEL: &str = "trial.behavior.Label";
const NAME: &str = "nara.scene.Name";
const TIMEOUT: Duration = Duration::from_secs(30);

/// Exercises the fixed behavior source under its admitted catalog, independently of execution.
/// Both formats must return the same complete document after physical Editor cleanup.
pub fn edit_save_reopen(
    owner: &ComponentSchemaOwnerSnapshot,
    original: &SceneDocument,
) -> SceneDocument {
    assert_eq!(owner.owner_id().as_str(), "trial.behaviors");
    let label_schema = owner.schema(&component(LABEL)).unwrap();
    let initial_label = record(original, "label", LABEL);
    assert_eq!(
        label_schema.authoring_initial_value(),
        Some(&initial_label.value),
        "the external Label catalog must supply its actual Attach template",
    );
    assert_eq!(
        label_schema
            .fields()
            .iter()
            .find(|field| field.id == ComponentFieldId::new("text"))
            .unwrap()
            .path,
        nara_reflect::ComponentFieldPath::from_fields(["wire_text"]),
    );
    let bundle = schema_bundle(owner);

    let mut expected = original.clone();
    for (entity, kind, wire, value) in [
        ("actor", MOVER, "amount", ComponentValue::f64(7.0).unwrap()),
        (
            "label",
            LABEL,
            "wire_text",
            ComponentValue::String("Edited label".into()),
        ),
    ] {
        let record = expected
            .entities
            .iter_mut()
            .find(|record| record.id == id(entity))
            .unwrap()
            .components
            .get_mut(&component(kind))
            .unwrap();
        let ComponentValue::Map(fields) = &mut record.value else {
            panic!("the fixed behavior record must be a Map");
        };
        assert!(fields.insert(wire.into(), value).is_some());
    }

    let mut output = None;
    for extension in ["json", "ron"] {
        let project = Project::new(&bundle, original, extension);
        let original_bytes = fs::read(project.scene_path()).unwrap();
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        assert_history(&editor, document, 0, 0);
        assert_siblings(&editor, document, original);

        let mover = AuthoringPropertyEdit {
            entity: id("actor"),
            component: component(MOVER),
            component_version: ComponentSchemaVersion::ONE,
            property: property("amount"),
            value: ComponentValue::f64(7.0).unwrap(),
        };
        let revision = editor.document_query_basis(document).unwrap().revision;
        let prepared = editor
            .prepare_authoring_property_edit(document, revision, &[mover])
            .unwrap();
        applied(editor.commit_authoring_edit(prepared));
        editor.drive_editor_frame(Duration::ZERO);
        assert_value(
            &editor,
            document,
            "actor",
            MOVER,
            "amount",
            ComponentValue::f64(7.0).unwrap(),
        );
        replay(&mut editor, document, false);
        assert_value(
            &editor,
            document,
            "actor",
            MOVER,
            "amount",
            record(original, "actor", MOVER)
                .value
                .field("amount")
                .unwrap()
                .clone(),
        );
        replay(&mut editor, document, true);
        assert_value(
            &editor,
            document,
            "actor",
            MOVER,
            "amount",
            ComponentValue::f64(7.0).unwrap(),
        );
        assert_siblings(&editor, document, original);

        let revision = editor.document_query_basis(document).unwrap().revision;
        let remove = ScenePatchDocument::new([ScenePatchOperation::RemoveComponent {
            entity: id("label"),
            component: component(LABEL),
        }]);
        let prepared = editor
            .prepare_authoring_edit(document, revision, &remove)
            .unwrap();
        applied(editor.commit_authoring_edit(prepared));
        editor.drive_editor_frame(Duration::ZERO);
        assert_label_present(&editor, document, false);
        replay(&mut editor, document, false);
        assert_label_present(&editor, document, true);
        assert_value(
            &editor,
            document,
            "label",
            LABEL,
            "text",
            initial_label.value.field("wire_text").unwrap().clone(),
        );
        replay(&mut editor, document, true);
        assert_label_present(&editor, document, false);
        assert_siblings(&editor, document, original);

        let revision = editor.document_query_basis(document).unwrap().revision;
        let mut draft = editor
            .begin_authoring_construction(document, revision)
            .unwrap();
        draft.add_component(id("label"), component(LABEL)).unwrap();
        let prepared = editor
            .prepare_authoring_construction(document, &draft)
            .unwrap();
        assert!(matches!(
            prepared.patch().operations.as_slice(),
            [ScenePatchOperation::AddComponent { entity, component: kind, value }]
                if entity == &id("label") && kind == &component(LABEL) && value == initial_label
        ));
        let committed = editor.commit_authoring_construction(prepared).unwrap();
        assert!(committed.report().applied, "{:?}", committed.report());
        editor.drive_editor_frame(Duration::ZERO);
        assert_label_present(&editor, document, true);
        assert_value(
            &editor,
            document,
            "label",
            LABEL,
            "text",
            initial_label.value.field("wire_text").unwrap().clone(),
        );
        replay(&mut editor, document, false);
        assert_label_present(&editor, document, false);
        replay(&mut editor, document, true);
        assert_label_present(&editor, document, true);
        assert_siblings(&editor, document, original);

        let revision = editor.document_query_basis(document).unwrap().revision;
        let label = AuthoringPropertyEdit {
            entity: id("label"),
            component: component(LABEL),
            component_version: ComponentSchemaVersion::ONE,
            property: property("text"),
            value: ComponentValue::String("Edited label".into()),
        };
        let prepared = editor
            .prepare_authoring_property_edit(document, revision, &[label])
            .unwrap();
        applied(editor.commit_authoring_edit(prepared));
        editor.drive_editor_frame(Duration::ZERO);
        assert_value(
            &editor,
            document,
            "label",
            LABEL,
            "text",
            ComponentValue::String("Edited label".into()),
        );
        replay(&mut editor, document, false);
        assert_value(
            &editor,
            document,
            "label",
            LABEL,
            "text",
            initial_label.value.field("wire_text").unwrap().clone(),
        );
        replay(&mut editor, document, true);
        assert_value(
            &editor,
            document,
            "label",
            LABEL,
            "text",
            ComponentValue::String("Edited label".into()),
        );
        assert_siblings(&editor, document, original);
        assert_history(&editor, document, 4, 0);
        assert_eq!(fs::read(project.scene_path()).unwrap(), original_bytes);

        save(&mut editor, document);
        let saved = project.read();
        assert_eq!(saved, expected, "Save must preserve every source sibling");
        close(&mut editor);
        drop(editor);

        // Reopen through a new Host and Session, rather than replaying a cached source snapshot.
        let saved_bytes = fs::read(project.scene_path()).unwrap();
        let mut reopened = project.open();
        let document = reopened
            .current_frame()
            .workspace()
            .active_document
            .unwrap();
        assert_history(&reopened, document, 0, 0);
        assert_siblings(&reopened, document, original);
        assert_value(
            &reopened,
            document,
            "actor",
            MOVER,
            "amount",
            ComponentValue::f64(7.0).unwrap(),
        );
        assert_value(
            &reopened,
            document,
            "label",
            LABEL,
            "text",
            ComponentValue::String("Edited label".into()),
        );
        let result = project.read();
        assert_eq!(result, saved);
        assert!(record(&result, "label", LABEL).value.field("text").is_err());
        assert_eq!(fs::read(project.scene_path()).unwrap(), saved_bytes);
        close(&mut reopened);
        drop(reopened);
        if let Some(previous) = &output {
            assert_eq!(&result, previous, "JSON and RON authoring must agree");
        }
        output = Some(result);
    }
    output.unwrap()
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn property(value: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new(value))
}

fn record<'a>(document: &'a SceneDocument, entity: &str, kind: &str) -> &'a SceneComponentRecord {
    &document
        .entities
        .iter()
        .find(|record| record.id == id(entity))
        .unwrap()
        .components[&component(kind)]
}

pub(super) fn assert_value(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
    entity: &str,
    kind: &str,
    field: &str,
    value: ComponentValue,
) {
    let observed = editor
        .read_document_property(EditorPropertyRead {
            target: EditorPropertyTarget {
                basis: editor.document_query_basis(document).unwrap(),
                entity: id(entity),
                component: component(kind),
                property: property(field),
            },
            limits: EditorPropertyReadLimits::new(4096, 256, 16 * 1024).unwrap(),
        })
        .unwrap();
    assert_eq!(observed.value, EditorPropertyValue::Present(value));
}

fn assert_siblings(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
    source: &SceneDocument,
) {
    for entity in ["actor", "friend", "label"] {
        assert_value(
            editor,
            document,
            entity,
            NAME,
            "value",
            record(source, entity, NAME).value.clone(),
        );
    }
    for field in ["images", "targets"] {
        assert_value(
            editor,
            document,
            "actor",
            SENSOR,
            field,
            record(source, "actor", SENSOR)
                .value
                .field(field)
                .unwrap()
                .clone(),
        );
    }
}

fn assert_label_present(editor: &EditorProjectSession, document: EditorDocumentId, present: bool) {
    let page = editor
        .query_document_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                basis: editor.document_query_basis(document).unwrap(),
                entity: id("label"),
                projection: EditorAuthoringProjection::Components,
            }),
            limits: EditorAuthoringQueryLimits::new(16, 16, 4096).unwrap(),
        })
        .unwrap();
    assert!(page.next.is_none());
    assert_eq!(
        page.rows.iter().any(|row| matches!(row,
            EditorAuthoringRow::Component { component: kind, .. } if kind == &component(LABEL))),
        present,
    );
}

pub(super) fn assert_history(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
    undo: usize,
    redo: usize,
) {
    let frame = editor.current_frame();
    let history = &frame.workspace().document(document).unwrap().history;
    assert_eq!(history.undo_depth, undo);
    assert_eq!(history.redo_depth, redo);
}

pub(super) fn applied(outcome: EditorActionOutcome) {
    assert!(
        matches!(outcome, EditorActionOutcome::Workspace(ref report) if report.applied),
        "{outcome:?}",
    );
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let revision = editor.current_frame().revision();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(revision, [action]))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    report.into_outcomes().into_iter().next().unwrap()
}

pub(super) fn replay(editor: &mut EditorProjectSession, document: EditorDocumentId, redo: bool) {
    let base_revision = editor.document_query_basis(document).unwrap().revision;
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
    applied(dispatch(editor, EditorAction::Mutate(mutation)));
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !ready(editor) {
        assert!(Instant::now() < deadline, "Editor operation did not settle");
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn close(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
            assert!(!receipt.has_cleanup_failure(), "{receipt:?}");
            return;
        }
        assert!(Instant::now() < deadline, "Editor close did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn schema_bundle(owner: &ComponentSchemaOwnerSnapshot) -> AuthoringSchemaBundle {
    let native = AuthoringSchemaBundle::from_trusted_provider_definitions([
        SCENE_COMPONENTS_SCHEMA_PROVIDER,
    ])
    .unwrap();
    AuthoringSchemaBundle::new(
        native
            .owners()
            .iter()
            .map(AuthoringSchemaBundleOwner::from)
            .chain([AuthoringSchemaBundleOwner::from(owner)]),
    )
    .unwrap()
}

pub(super) fn save(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let revision = editor.document_query_basis(document).unwrap().revision;
    let attempt = match dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: revision,
        }),
    ) {
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt }) => {
            attempt
        }
        outcome => panic!("Save was not accepted: {outcome:?}"),
    };
    wait(editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    assert_eq!(
        editor.current_frame().persistence().attempt(),
        Some(attempt)
    );
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Saved { document: saved, revision: saved_revision, .. })
            if saved == document && saved_revision == revision
    ));
    dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
}

pub(super) struct Project {
    directory: tempfile::TempDir,
    extension: &'static str,
}

impl Project {
    pub(super) fn new(
        bundle: &AuthoringSchemaBundle,
        scene: &SceneDocument,
        extension: &'static str,
    ) -> Self {
        let project = Self {
            directory: tempfile::tempdir().unwrap(),
            extension,
        };
        for directory in ["assets/textures", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(project.root().join(directory)).unwrap();
        }
        fs::write(
            project.root().join("nara.toml"),
            format!(
                concat!(
                    "schema_version = 1\n[project]\n",
                    "source_universe = 'f1390a84-11df-49b0-a901-d7a045141039'\n",
                    "native_document_identity = 'project_location'\nname = 'Behavior language trial'\n",
                    "[paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n",
                    "[startup]\ndefault_scene = 'startup.scene.{}'\n",
                    "[authoring]\nschema_bundle = 'schema/authoring.bundle.json'\n",
                    "[runtime]\npreset = 'local-headless'\n",
                    "[capabilities]\nrequested = ['runtime-2d']\n",
                ),
                extension,
            ),
        )
        .unwrap();
        fs::write(
            project.root().join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.scene_path(),
            if extension == "ron" {
                scene.to_ron_string().unwrap()
            } else {
                scene.to_json_string().unwrap()
            },
        )
        .unwrap();
        fs::write(
            project.root().join("assets/textures/sensor.png"),
            include_bytes!("../../../images/valid-rgba-1x1.png"),
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("b1a29038-b6ee-44b9-aa80-15be0560ed4d").unwrap(),
                AssetPath::new("textures/sensor.png").unwrap(),
                AssetSourceKind::Image,
            ),
            ImageImportSettings::default(),
        );
        fs::write(
            project.root().join("assets/textures/sensor.png.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
        project
    }

    pub(super) fn root(&self) -> &Path {
        self.directory.path()
    }

    pub(super) fn scene_path(&self) -> std::path::PathBuf {
        self.root()
            .join(format!("scenes/startup.scene.{}", self.extension))
    }

    pub(super) fn read(&self) -> SceneDocument {
        let bytes = fs::read(self.scene_path()).unwrap();
        let candidate = if self.extension == "ron" {
            SceneDocumentCandidate::decode_ron_bytes(&bytes)
        } else {
            SceneDocumentCandidate::decode_json_bytes(&bytes)
        }
        .unwrap();
        candidate.into_composition_source().unwrap()
    }

    pub(super) fn open(&self) -> EditorProjectSession {
        #[cfg(windows)]
        let directory = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(self.root())
                .unwrap()
        };
        #[cfg(not(windows))]
        let directory = fs::File::open(self.root()).unwrap();
        let capability = DirectoryCapability::from_host_handle(
            directory,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        let mut host = EditorProjectOpenHost::new();
        let mut attempt = host.begin(capability, None);
        let deadline = Instant::now() + TIMEOUT;
        let session = loop {
            match host.advance(attempt) {
                EditorProjectOpenProgress::Opened(editor) => break editor,
                EditorProjectOpenProgress::Pending(next) => attempt = next,
                EditorProjectOpenProgress::Rejected(failure) => {
                    panic!("{:?}", failure.diagnostics());
                }
            }
            assert!(Instant::now() < deadline, "Editor Open did not settle");
            std::thread::sleep(Duration::from_millis(1));
        };
        // Open preparation belongs to its Host, independently of the returned Session.
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let report = host.poll_shutdown();
            if report.is_complete() {
                assert!(!report.has_cleanup_failure(), "{report:?}");
                break;
            }
            assert!(Instant::now() < deadline, "Editor Open Host did not close");
            std::thread::sleep(Duration::from_millis(1));
        }
        session
    }
}
