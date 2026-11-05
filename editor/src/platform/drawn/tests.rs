use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    scene::{SceneAuthoringRevision, SceneEntityId, ScenePlacement},
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId, EditorDocumentKind,
        EditorFrameModel, EditorPersistenceResult, EditorPersistenceView,
    },
};
use nara_ui_kernel::{UiFrame, UiPoint, UiSemanticRole};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, Ime, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

use super::DrawnNative;
use crate::{
    EditorApplicationActionDispatch, EditorLaunchOptions, native::EditorPanelPlatformEvent,
};

mod captures;
mod driver;
mod fixture;
mod lifecycle;
mod ordinary;
mod specialized;
mod styled;

type TestResult<T = ()> = Result<T, String>;

const HIERARCHY: &str = "nara.hierarchy";
const SHELL: &str = "nara.drawn.workspace";
const INSPECTOR: &str = "nara.inspector";
const TIMEOUT: Duration = Duration::from_secs(180);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(15);

#[test]
#[ignore = "requires a Windows desktop, WGPU_BACKEND=dx12 and --no-default-features"]
fn native_drawn_source_workspace_preserves_structure_history_and_persistence() {
    assert!(
        !cfg!(feature = "imgui-reference"),
        "run this independent native acceptance with --no-default-features; the reference frontend must not initialize a context"
    );
    assert_eq!(
        std::env::var("WGPU_BACKEND").as_deref(),
        Ok("dx12"),
        "select DX12 explicitly before starting this native acceptance"
    );
    let project = fixture::Project::new().expect("the data-only project must be created");
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder
        .build()
        .expect("the actual Winit event loop must open");
    let mut application = Acceptance {
        project: &project,
        result: None,
    };
    event_loop
        .run_app(&mut application)
        .expect("the native event loop must complete");
    let presentations = application
        .result
        .expect("the hidden native acceptance must run")
        .expect("the complete drawn source workflow must succeed");
    assert!(presentations > 0);
}

struct Acceptance<'a> {
    project: &'a fixture::Project,
    result: Option<TestResult<u64>>,
}

impl ApplicationHandler for Acceptance<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some((|| {
                let window = event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Nara drawn source native acceptance")
                            .with_visible(false)
                            .with_inner_size(LogicalSize::new(1200.0, 1100.0)),
                    )
                    .map_err(|error| error.to_string())?;
                let native = DrawnNative::new(
                    Arc::new(window),
                    EditorLaunchOptions::new(self.project.root()),
                )?;
                let mut workflow = Workflow {
                    native,
                    project: self.project,
                    deadline: Instant::now() + TIMEOUT,
                    presentations: 0,
                    closed: false,
                    expected_cleanup_errors: Vec::new(),
                };
                let result = workflow.run();
                let cleanup = workflow.close();
                match (result, cleanup) {
                    (Ok(()), Ok(())) => Ok(workflow.presentations),
                    (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
                    (Err(error), Err(cleanup)) => {
                        Err(format!("{error}; native cleanup: {cleanup}"))
                    }
                }
            })());
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

struct Workflow<'a> {
    native: DrawnNative,
    project: &'a fixture::Project,
    deadline: Instant,
    presentations: u64,
    closed: bool,
    expected_cleanup_errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceState {
    document: EditorDocumentId,
    kind: EditorDocumentKind,
    revision: SceneAuthoringRevision,
    rows: BTreeMap<SceneEntityId, Option<ScenePlacement>>,
    undo: usize,
    redo: usize,
    dirty: bool,
}

impl Workflow<'_> {
    fn run(&mut self) -> TestResult {
        self.wait("open the data-only project", |workflow| {
            workflow
                .native
                .application
                .current_frame()
                .is_some_and(|frame| !frame.project_browser().has_pending_work())
        })?;
        self.paint()?;
        self.event(EditorPanelPlatformEvent::Focused(true))?;
        self.prepare_image_fixture()?;
        let scene = self.state()?.document;
        let prefab = self.open_source(fixture::PREFAB)?;
        let preserved = self.open_source(fixture::PRESERVED)?;
        self.switch_document(scene)?;
        self.hierarchy_filter_completion()?;
        self.ordinary(fixture::SCENE, EditorDocumentKind::Scene)?;
        self.inspector_edit_and_restore()?;
        self.switch_document(prefab)?;
        self.ordinary(fixture::PREFAB, EditorDocumentKind::Prefab)?;
        self.switch_document(preserved)?;
        self.preserve_unknown()?;
        let ui = self.open_source(fixture::UI)?;
        self.projection_replacement(scene, ui)
            .map_err(|error| format!("Projection replacement: {error}"))?;
        self.specialized_ui()
            .map_err(|error| format!("Specialized UI: {error}"))?;
        self.switch_document(preserved)?;
        self.dirty_close_and_graphics_recovery()
            .map_err(|error| format!("Dirty close and graphics recovery: {error}"))?;
        self.switch_document(scene)?;
        self.replace_hierarchy_incarnation()?;
        self.close_with_preedit_and_pending_candidate()
    }
}
