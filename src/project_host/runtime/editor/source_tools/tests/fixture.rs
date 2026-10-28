use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage, SceneAuthoringRevision, SceneDocument,
    SceneEntityId, SceneEntityRecord,
};
use nara_tasks::TaskReturnObserver;
use nara_tooling::EditorDocumentId;
use serde_json::{Value, json};

use super::*;
use crate::project_host::{
    EditorProjectCloseProgress,
    authoring::{
        AuthoringProjectAdmissionHost, PreparedAuthoringProject, project_authoring_adapters,
    },
    composition::built_in_schema_providers,
};

pub(super) const WAIT: Duration = Duration::from_secs(15);
const POLL_BOUND: Duration = Duration::from_secs(2);
const SCENE: &str = "scenes/startup.scene.json";
static NEXT_PROJECT: AtomicU64 = AtomicU64::new(1);

pub(super) fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.FiniteSourcePayload")
}

pub(super) fn target() -> SceneEntityId {
    SceneEntityId::new("target").unwrap()
}

pub(super) fn catalog_json(
    catalog: &ComponentSchemaCatalog,
    predecessor: Option<&ComponentSchemaCatalog>,
) -> Value {
    serde_json::from_str(
        &catalog
            .to_json_string_with_predecessor(predecessor)
            .unwrap(),
    )
    .unwrap()
}

pub(super) fn proposal() -> Output {
    Output::Result(json!({
        "kind": "proposal",
        "title": "Set the finite payload",
        "writes": [{
            "property": ComponentPropertyPath::new(ComponentFieldId::new("amount")),
            "value": ComponentValue::I64(7),
        }],
    }))
}

pub(in crate::project_host::runtime::editor::source_tools) struct Project {
    pub(in crate::project_host::runtime::editor::source_tools) root: PathBuf,
    pub(in crate::project_host::runtime::editor::source_tools) owner: ComponentSchemaOwnerSnapshot,
}

impl Project {
    pub(in crate::project_host::runtime::editor::source_tools) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_source_tool_session_{}_{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed),
        ));
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            "schema_version = 1\n\
             [project]\nname = 'Finite source tool Session'\n\
             source_universe = '72b32f54-73d6-5c54-a560-0298797f6c90'\n\
             native_document_identity = 'project_location'\n\
             [paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n\
             [startup]\ndefault_scene = 'startup.scene.json'\n\
             [authoring]\nschema_bundle = 'schema/authoring.bundle.json'\n\
             [runtime]\npreset = 'local-headless'\n\
             [capabilities]\nrequested = ['runtime-2d']\n",
        )
        .unwrap();
        let schema = ComponentSchema::new(
            component(),
            "Finite source payload",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("amount"),
            "Amount",
            ComponentFieldPath::from_fields(["amount"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
        .with_authoring_initial_value(ComponentValue::map([("amount", ComponentValue::I64(1))]));
        let owner = ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("test.finite-source-payload").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let builtins =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap();
        let bundle = AuthoringSchemaBundle::new(
            builtins
                .owners()
                .iter()
                .map(AuthoringSchemaBundleOwner::from)
                .chain([AuthoringSchemaBundleOwner::from(&owner)]),
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(SCENE),
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(target())])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        Self { root, owner }
    }

    pub(super) fn description(&self) -> Output {
        Output::Result(json!({
            "kind": "description",
            "catalog": catalog_json(self.owner.current(), self.owner.predecessor()),
        }))
    }

    pub(super) fn facet_description(&self) -> Output {
        Output::Result(json!({
            "kind": "description",
            "catalog": catalog_json(self.owner.current(), self.owner.predecessor()),
            "integer_property": {
                "field": "amount",
                "parameter": "bonus",
                "label": "Bonus",
                "initial": 1,
            },
        }))
    }

    pub(in crate::project_host::runtime::editor::source_tools) fn tool(
        &self,
    ) -> EditorSourceToolDefinition {
        self.tool_with(self.description(), proposal(), Duration::from_secs(10))
    }

    pub(super) fn tool_with(
        &self,
        describe: Output,
        invoke: Output,
        timeout: Duration,
    ) -> EditorSourceToolDefinition {
        EditorSourceToolDefinition::new(
            child::command(&self.root, describe, invoke, timeout),
            self.owner.clone(),
            component(),
        )
        .unwrap()
    }

    pub(in crate::project_host::runtime::editor::source_tools) fn open(
        &self,
    ) -> EditorProjectSession {
        // Match the real metadata authoring fixture: admit bundle, adapters and startup closure.
        #[cfg(windows)]
        let directory = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(unix)]
        let directory = fs::File::open(&self.root).unwrap();
        let capability = DirectoryCapability::from_host_handle(
            directory,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(capability, None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapters = project_authoring_adapters(schema.bundle()).unwrap();
        let receipt = PreparedAuthoringProject::new(schema, adapters)
            .admit_startup_closure()
            .unwrap()
            .publish();
        let mut editor = EditorProjectSession::open(receipt, None).unwrap();
        wait(&mut editor, |editor| {
            !editor.project_source_index_work_pending()
        });
        editor
    }

    pub(in crate::project_host::runtime::editor::source_tools) fn run(
        &self,
        test: impl FnOnce(&mut EditorProjectSession),
    ) {
        let mut editor = self.open();
        let result = catch_unwind(AssertUnwindSafe(|| test(&mut editor)));
        let deadline = Instant::now() + WAIT;
        loop {
            match editor.advance_close() {
                EditorProjectCloseProgress::Complete(receipt) => {
                    assert!(receipt.task_shutdown().is_complete());
                    assert!(!receipt.has_cleanup_failure());
                    break;
                }
                progress => assert!(
                    Instant::now() < deadline,
                    "source-tool Session failed to close: {progress:?}"
                ),
            }
            thread::sleep(Duration::from_millis(1));
        }
        if let Err(error) = result {
            resume_unwind(error);
        }
    }

    pub(super) fn persisted(&self) -> Vec<u8> {
        fs::read(self.root.join(SCENE)).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let (Ok(root), Ok(temporary)) = (
            self.root.canonicalize(),
            std::env::temp_dir().canonicalize(),
        ) else {
            return;
        };
        if root.parent() == Some(temporary.as_path())
            && root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_source_tool_session_")
        {
            let _ = fs::remove_dir_all(root);
        }
    }
}

pub(super) fn document(editor: &EditorProjectSession) -> EditorDocumentId {
    editor.workspace.active_document().unwrap()
}

#[derive(Debug, PartialEq)]
pub(super) struct Snapshot {
    pub(super) source: SceneDocument,
    revision: SceneAuthoringRevision,
    pub(super) history: SceneAuthoringHistoryStatus,
    usage: SceneAuthoringHistoryUsage,
    dirty: bool,
}

pub(super) fn snapshot(editor: &EditorProjectSession) -> Snapshot {
    let slot = editor.workspace.scene(document(editor)).unwrap();
    Snapshot {
        source: slot.session().document().clone(),
        revision: slot.revision(),
        history: slot.session().history_status(),
        usage: slot.session().history_usage(),
        dirty: slot.is_dirty(),
    }
}

pub(in crate::project_host::runtime::editor::source_tools) fn wait(
    editor: &mut EditorProjectSession,
    ready: impl Fn(&EditorProjectSession) -> bool,
) {
    let deadline = Instant::now() + WAIT;
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "source-tool owner did not settle: {:?}",
            editor.source_tool_view(),
        );
        editor.drive_editor_frame(Duration::ZERO);
        thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn activate(
    editor: &mut EditorProjectSession,
    project: &Project,
) -> EditorSourceToolActivationId {
    let attempt = editor.enable_source_tool(project.tool()).unwrap();
    wait(editor, |editor| !editor.source_tool_view().probing);
    let view = editor.source_tool_view();
    assert_eq!(view.attempt.as_ref(), Some(&attempt));
    let active = view.active.as_ref().expect("the finite tool must activate");
    assert!(view.attempt_error.is_none(), "{view:?}");
    active.id().clone()
}

pub(super) fn attach(
    editor: &mut EditorProjectSession,
    activation: &EditorSourceToolActivationId,
) -> EditorSourceToolRequestId {
    let basis = editor.document_query_basis(document(editor)).unwrap();
    editor
        .request_source_tool(
            activation,
            &basis,
            EditorSourceToolIntent::Attach { entity: target() },
            BTreeMap::new(),
        )
        .unwrap()
}

#[derive(Clone, Copy, Debug)]
pub(super) enum GatePoint {
    BeforeSpawn,
    BeforeIoReturn,
}

pub(super) struct HeldProcess {
    gate: Arc<process::ProcessTestGate>,
    release: mpsc::Sender<()>,
    watchdog: Option<JoinHandle<()>>,
}

impl HeldProcess {
    pub(super) fn arm(editor: &mut EditorProjectSession, point: GatePoint) -> Self {
        let gate = Arc::new(process::ProcessTestGate::default());
        editor.source_tools.next_gates = match point {
            GatePoint::BeforeSpawn => process::ProcessTestGates {
                before_spawn: Some(Arc::clone(&gate)),
                before_io_return: None,
            },
            GatePoint::BeforeIoReturn => process::ProcessTestGates {
                before_spawn: None,
                before_io_return: Some(Arc::clone(&gate)),
            },
        };
        let (release, receiver) = mpsc::channel();
        let watchdog_gate = Arc::clone(&gate);
        let watchdog = thread::spawn(move || {
            // A blocking-poll regression fails its latency assertion instead of hanging nextest.
            let _ = receiver.recv_timeout(WAIT * 2);
            watchdog_gate.release.store(true, Ordering::Release);
        });
        Self {
            gate,
            release,
            watchdog: Some(watchdog),
        }
    }

    pub(super) fn wait_until_entered(&self) {
        let deadline = Instant::now() + WAIT;
        while !self.gate.entered.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "the finite child did not reach its gate"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    pub(super) fn release(&self) {
        self.gate.release.store(true, Ordering::Release);
        let _ = self.release.send(());
    }

    pub(super) fn assert_pending(&self, returned: &TaskReturnObserver) {
        assert!(!self.gate.release.load(Ordering::Acquire));
        assert_eq!(returned.state(), TaskReturnState::Pending);
    }

    pub(super) fn poll<T>(&self, poll: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let result = poll();
        assert!(
            started.elapsed() < POLL_BOUND,
            "owner poll blocked on finite-child cleanup"
        );
        result
    }
}

impl Drop for HeldProcess {
    fn drop(&mut self) {
        self.release();
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
    }
}
