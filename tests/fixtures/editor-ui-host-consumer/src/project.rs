//! Real data-only projects and public Application choreography shared by both consumers.

use std::{
    fs,
    ops::{Deref, DerefMut},
    path::Path,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentSchema,
        ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, PersistentComponentSchemaOwnerId,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord},
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorCloseDecision,
        EditorDocumentId, EditorDocumentOpenResult, EditorDocumentQueryBasis, EditorEntityQuery,
        EditorEntityQueryLimits, EditorEntityQueryPosition, EditorFrameModel,
        EditorNavigationAction, EditorPersistenceResult, EditorPersistenceView,
        EditorProjectBrowserAction, EditorProjectBrowserActionOutcome, EditorPropertyRead,
        EditorPropertyReadLimits, EditorPropertyTarget,
    },
};
use nara_editor::{
    EditorApplication, EditorApplicationActionDispatch, EditorApplicationSubmission,
    EditorLaunchOptions,
    panels::EditorPanelHost,
    workspace::{
        EditorPanelDescriptor, EditorPanelInstanceId, EditorPanelMultiplicity,
        EditorPanelPlacement, EditorPanelTarget,
    },
};

use crate::{COMPONENT, DraftPanel, Evidence, FIELD, INITIAL};

pub const SOURCE: &str = "scenes/secondary.scene.json";

pub fn entity(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

pub fn schema() -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("consumer.editor-host").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(COMPONENT),
                    "Caption",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new(FIELD),
                    "Text",
                    ComponentFieldPath::from_fields([FIELD]),
                    ComponentValueKind::String,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
                .with_authoring_initial_value(ComponentValue::map([(
                    FIELD,
                    ComponentValue::String(INITIAL.into()),
                )])),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

pub fn source() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(entity("caption"))
            .with_component(
                ComponentTypeId::new(COMPONENT),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([(FIELD, ComponentValue::String(INITIAL.into()))]),
                )
                .with_owner(
                    PersistentComponentSchemaOwnerId::try_new("consumer.editor-host").unwrap(),
                ),
            )
            .with_component(
                ComponentTypeId::new("consumer.Unavailable"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([(
                        "opaque",
                        ComponentValue::String("preserve opaque payload".into()),
                    )]),
                ),
            ),
        SceneEntityRecord::new(entity("untouched")),
        SceneEntityRecord::new(entity("tail")),
    ])
}

pub struct Project {
    directory: tempfile::TempDir,
}

impl Project {
    pub fn new() -> Self {
        let directory = tempfile::Builder::new()
            .prefix("nara-editor-ui-host-")
            .tempdir()
            .unwrap();
        for path in ["assets", "prefabs", "scenes", "schema"] {
            fs::create_dir(directory.path().join(path)).unwrap();
        }
        fs::write(directory.path().join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = '24ed5e36-1b37-5fa3-9c42-232066b17916'\nnative_document_identity = 'project_location'\nname = 'Independent UI Host'\n[paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n[startup]\ndefault_scene = 'startup.scene.json'\n[authoring]\nstate = 'ready'\nschema_bundle = 'schema/authoring.bundle.json'\n[runtime]\npreset = 'local-headless'\n[capabilities]\nrequested = ['runtime-2d']\n").unwrap();
        fs::write(
            directory.path().join("schema/authoring.bundle.json"),
            schema().to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            directory.path().join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity("observer"))])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            directory.path().join(SOURCE),
            source().to_json_string().unwrap(),
        )
        .unwrap();
        Self { directory }
    }
    pub fn root(&self) -> &Path {
        self.directory.path()
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

/// A test/qualification guard that retires the real Application before deleting its project.
pub struct Application(EditorApplication);

impl Application {
    pub fn open(project: &Project) -> Self {
        let mut application = Self(EditorApplication::new(EditorLaunchOptions::new(
            project.root(),
        )));
        application.native_shell_ready();
        wait(&mut application, |frame| {
            !frame.project_browser().has_pending_work()
        });
        application
    }
    pub fn close(&mut self) -> bool {
        let mut update = self.request_exit();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !update.exit_ready() && Instant::now() < deadline {
            if let Some(frame) = self.current_frame()
                && let Some(prompt) = frame.workspace_intent().decision_prompt()
            {
                dispatch(
                    self,
                    EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard)),
                );
            }
            update = self.advance(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        update.exit_ready()
    }
}

impl Deref for Application {
    type Target = EditorApplication;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Application {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for Application {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub fn current(application: &EditorApplication) -> Arc<EditorFrameModel> {
    application.current_frame().unwrap()
}

pub fn wait(application: &mut EditorApplication, ready: impl Fn(&EditorFrameModel) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(frame) = application.current_frame()
            && ready(&frame)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the public Editor operation did not settle"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn dispatch(
    application: &mut EditorApplication,
    action: EditorAction,
) -> EditorApplicationSubmission {
    let revision = current(application).revision();
    application.submit_and_drive(EditorActionBatch::new(revision, [action]), Duration::ZERO)
}

pub fn open_secondary(application: &mut EditorApplication) -> EditorDocumentId {
    let active = current(application).workspace().active_document.unwrap();
    let opening = dispatch(
        application,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: SOURCE.into(),
        }),
    );
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::Preparing { request },
        )),
    ] = opening.actions()
    else {
        panic!("{opening:?}")
    };
    let request = *request;
    wait(application, |frame| {
        frame.document_open().result().is_some()
    });
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = current(application).document_open().result().unwrap()
    else {
        panic!("public Browser did not open the secondary Scene")
    };
    assert_eq!(request, completed);
    dispatch(
        application,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request }),
    );
    dispatch(
        application,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
    );
    document
}

pub fn register(
    host: &mut EditorPanelHost<DraftPanel>,
    application: &mut EditorApplication,
    evidence: &Rc<Evidence>,
    kind: &str,
) -> EditorPanelInstanceId {
    let observed = Rc::clone(evidence);
    host.register_panel(
        application,
        EditorPanelDescriptor::new(
            kind,
            "Independent caption",
            EditorPanelMultiplicity::Multiple,
            EditorPanelPlacement::Floating,
            true,
        ),
        move || DraftPanel::new(Rc::clone(&observed)).unwrap(),
    )
    .unwrap();
    let instance = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == kind)
        .unwrap()
        .id();
    assert!(
        application
            .panel_workspace_mut()
            .layout_mut()
            .set_target(instance, EditorPanelTarget::Workspace)
    );
    instance
}

pub fn query(basis: &EditorDocumentQueryBasis, rows: usize) -> EditorEntityQuery {
    EditorEntityQuery {
        position: EditorEntityQueryPosition::Start(basis.clone()),
        limits: EditorEntityQueryLimits::new(rows, rows).unwrap(),
    }
}

pub fn property(basis: &EditorDocumentQueryBasis) -> EditorPropertyRead {
    EditorPropertyRead {
        target: EditorPropertyTarget {
            basis: basis.clone(),
            entity: entity("caption"),
            component: ComponentTypeId::new(COMPONENT),
            property: ComponentPropertyPath::new(ComponentFieldId::new(FIELD)),
        },
        limits: EditorPropertyReadLimits::new(1024, 1, 256).unwrap(),
    }
}

pub fn persistence(application: &mut EditorApplication) -> EditorPersistenceResult {
    wait(application, |frame| {
        matches!(frame.persistence(), EditorPersistenceView::Terminal { .. })
    });
    let EditorPersistenceView::Terminal { result, .. } = current(application).persistence() else {
        unreachable!()
    };
    result
}
