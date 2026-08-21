use std::{
    env,
    ffi::OsStr,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::tooling::{
    EditorDocumentId, EditorDocumentKind, EditorProjectBrowserAction,
    EditorProjectBrowserBudgetUsage, EditorProjectBrowserDiagnosticPhase,
    EditorProjectBrowserDiagnosticView, EditorProjectBrowserEntry, EditorProjectBrowserLocalAction,
    EditorProjectBrowserStatus, EditorProjectSourceKind, EditorProjectSourceOpenCapability,
};
use winit::window::WindowId;

use crate::{EditorApplication, EditorApplicationState, EditorLaunchOptions};

use super::{EditorRunError, NativeEditorCanaryAdvance};

mod contract;
mod fixture;

use contract::{
    canary_state_error, project_browser_action, ready_status_label, require_browser_baseline,
    require_browser_entry, require_browser_matches_baseline, require_current_frame,
    require_document, require_document_id, require_filtered_row, require_last_good_snapshot,
    require_open_documents, require_path_filter, require_project_browser_sidebar_presentation,
    require_ready_browser_snapshot, require_rendered_state, require_safe_lineage_failure,
    require_selected_path, require_workspace_contract,
};
use fixture::{
    GuardedProject, canary_io_error, canary_prepare_error, canary_refresh_mutation_error,
};

const GENERIC_OPEN_CANARY_ENV: &str = "NARA_EDITOR_GENERIC_OPEN_CANARY";
const GENERIC_OPEN_CANARY_DEADLINE: Duration = Duration::from_secs(35);
const PROJECT_MANIFEST_PATH: &str = "nara.toml";
const AUTHORING_BUNDLE_PATH: &str = "catalogs/authoring.bundle.json";
const STARTUP_SCENE_PATH: &str = "worlds/entry.scene.json";
const EXTERNAL_SCENE_PATH: &str = "worlds/external.scene.json";
const EXTERNAL_PREFAB_PATH: &str = "blueprints/external.prefab.json";
const EXTERNAL_SCENE_FILTER: &str = "external.scene";
const EXTERNAL_PREFAB_FILTER: &str = "external.prefab";
const SCRIPTS_ROOT_PATH: &str = "scripts";
const RETAINED_SCRIPTS_ROOT_PATH: &str = "scripts-before-refresh";
const LINEAGE_CHANGED_CODE: &str = "project.source-index.directory-lineage-changed";
const TEMP_ROOT_PREFIX: &str = "nara-editor-generic-open-canary-";
const SOURCE_MEMBER_BYTE_LIMIT: u64 = 16 * 1024 * 1024;

pub(super) fn requested() -> bool {
    env::var_os(GENERIC_OPEN_CANARY_ENV).as_deref() == Some(OsStr::new("1"))
}

#[derive(Debug, Clone)]
enum EditorGenericOpenCanaryStage {
    ProjectManager,
    Recovery,
    AwaitingInitialBrowser,
    AwaitingProjectBrowserPresentation(BrowserBaseline),
    AwaitingSceneFilter(BrowserBaseline),
    AwaitingSceneSelection(BrowserBaseline),
    AwaitingSceneOpen(BrowserBaseline),
    AwaitingPrefabFilter(SceneDocumentState),
    AwaitingPrefabSelection(SceneDocumentState),
    AwaitingPrefabOpen(SceneDocumentState),
    AwaitingEquivalentRefresh(OpenDocumentsState),
    AwaitingFailureRevalidation(RefreshedBrowserState),
    AwaitingLastGoodFailure(RefreshedBrowserState),
    Complete(EditorGenericOpenCanaryEvidence),
}

#[derive(Debug, Clone)]
struct BrowserBaseline {
    status: EditorProjectBrowserStatus,
    generation: u64,
    entries: Arc<[EditorProjectBrowserEntry]>,
    diagnostics: Arc<[EditorProjectBrowserDiagnosticView]>,
    usage: EditorProjectBrowserBudgetUsage,
}

#[derive(Debug, Clone)]
struct SceneDocumentState {
    browser: BrowserBaseline,
    scene_document: EditorDocumentId,
}

#[derive(Debug, Clone)]
struct OpenDocumentsState {
    browser: BrowserBaseline,
    scene_document: EditorDocumentId,
    prefab_document: EditorDocumentId,
}

#[derive(Debug, Clone)]
struct RefreshedBrowserState {
    documents: OpenDocumentsState,
    status: EditorProjectBrowserStatus,
    generation: u64,
    diagnostics: Arc<[EditorProjectBrowserDiagnosticView]>,
    usage: EditorProjectBrowserBudgetUsage,
}

#[derive(Debug, Clone, Copy)]
struct EditorGenericOpenCanaryEvidence {
    initial_status: EditorProjectBrowserStatus,
    initial_generation: u64,
    refreshed_status: EditorProjectBrowserStatus,
    refreshed_generation: u64,
    entry_count: usize,
    scene_document: EditorDocumentId,
    prefab_document: EditorDocumentId,
}

pub(super) struct EditorGenericOpenCanaryHost {
    guarded_project: GuardedProject,
    stage: EditorGenericOpenCanaryStage,
    deadline: Instant,
    window_id: Option<WindowId>,
}

impl EditorGenericOpenCanaryHost {
    pub(super) fn new(
        options: EditorLaunchOptions,
    ) -> Result<(EditorApplication, Self), EditorRunError> {
        let source_root = options.project_root().ok_or_else(|| {
            EditorRunError::usage(format!(
                "{GENERIC_OPEN_CANARY_ENV}=1 requires one source project root"
            ))
        })?;
        let guarded_project = GuardedProject::prepare(source_root).map_err(canary_prepare_error)?;
        let launch = EditorLaunchOptions::new(guarded_project.root())
            .with_optional_profile(options.profile().map(str::to_owned));
        Ok((
            EditorApplication::new(launch),
            Self {
                guarded_project,
                stage: EditorGenericOpenCanaryStage::ProjectManager,
                deadline: Instant::now() + GENERIC_OPEN_CANARY_DEADLINE,
                window_id: None,
            },
        ))
    }

    pub(super) fn advance_project_shell(
        &mut self,
        rendered_state: EditorApplicationState,
        application: &mut EditorApplication,
        window_id: WindowId,
    ) -> Result<bool, EditorRunError> {
        self.require_before_deadline(application.state())?;
        self.observe_window(window_id, application.state())?;
        let stage = self.stage.clone();
        match stage.clone() {
            EditorGenericOpenCanaryStage::ProjectManager => {
                require_rendered_state(
                    rendered_state,
                    EditorApplicationState::ProjectManager,
                    &stage,
                )?;
                self.require_before_deadline(application.state())?;
                let update = application.native_shell_ready();
                if !update.actions_observed()
                    || application.state() != EditorApplicationState::OpeningInitial
                {
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the initial project request was not armed after Project Manager presentation",
                    ));
                }
                self.require_before_deadline(application.state())?;
                self.stage = EditorGenericOpenCanaryStage::Recovery;
                Ok(true)
            }
            EditorGenericOpenCanaryStage::Recovery => match rendered_state {
                EditorApplicationState::OpeningInitial => Ok(false),
                EditorApplicationState::Recovery => {
                    self.guarded_project
                        .restore_authoring_bundle()
                        .map_err(canary_io_error)?;
                    self.require_before_deadline(application.state())?;
                    if !application.retry_project_open()
                        || application.state() != EditorApplicationState::OpeningInitial
                    {
                        return Err(canary_state_error(
                            &stage,
                            application.state(),
                            "Recovery did not accept the guarded Retry",
                        ));
                    }
                    self.require_before_deadline(application.state())?;
                    self.stage = EditorGenericOpenCanaryStage::AwaitingInitialBrowser;
                    Ok(true)
                }
                state => Err(canary_state_error(
                    &stage,
                    state,
                    "the missing authoring bundle did not reach Recovery",
                )),
            },
            EditorGenericOpenCanaryStage::AwaitingInitialBrowser => match rendered_state {
                EditorApplicationState::OpeningInitial => Ok(false),
                EditorApplicationState::Recovery => Err(canary_state_error(
                    &stage,
                    rendered_state,
                    "the repaired authoring bundle was rejected",
                )),
                state => Err(canary_state_error(
                    &stage,
                    state,
                    "the repaired project left the bounded initial-open trajectory",
                )),
            },
            EditorGenericOpenCanaryStage::AwaitingProjectBrowserPresentation(_)
            | EditorGenericOpenCanaryStage::AwaitingSceneFilter(_)
            | EditorGenericOpenCanaryStage::AwaitingSceneSelection(_)
            | EditorGenericOpenCanaryStage::AwaitingSceneOpen(_)
            | EditorGenericOpenCanaryStage::AwaitingPrefabFilter(_)
            | EditorGenericOpenCanaryStage::AwaitingPrefabSelection(_)
            | EditorGenericOpenCanaryStage::AwaitingPrefabOpen(_)
            | EditorGenericOpenCanaryStage::AwaitingEquivalentRefresh(_)
            | EditorGenericOpenCanaryStage::AwaitingFailureRevalidation(_)
            | EditorGenericOpenCanaryStage::AwaitingLastGoodFailure(_)
            | EditorGenericOpenCanaryStage::Complete(_) => Err(canary_state_error(
                &stage,
                rendered_state,
                "the canary returned to the Project Shell after publishing its workspace",
            )),
        }
    }

    pub(super) fn advance_workspace(
        &mut self,
        rendered_state: EditorApplicationState,
        application: &mut EditorApplication,
        window_id: WindowId,
    ) -> Result<NativeEditorCanaryAdvance, EditorRunError> {
        self.require_before_deadline(application.state())?;
        self.observe_window(window_id, application.state())?;
        let stage = self.stage.clone();
        require_rendered_state(
            rendered_state,
            EditorApplicationState::AuthoringDegraded,
            &stage,
        )?;
        require_workspace_contract(application, &stage)?;
        self.require_before_deadline(application.state())?;
        match stage.clone() {
            EditorGenericOpenCanaryStage::ProjectManager
            | EditorGenericOpenCanaryStage::Recovery => Err(canary_state_error(
                &stage,
                rendered_state,
                "a workspace was published before the guarded repair completed",
            )),
            EditorGenericOpenCanaryStage::AwaitingInitialBrowser => {
                let frame = require_current_frame(application, &stage)?;
                let browser = frame.project_browser();
                if browser.has_pending_work() {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                let snapshot =
                    require_ready_browser_snapshot(browser, application.state(), &stage)?;
                require_browser_entry(
                    snapshot,
                    EXTERNAL_SCENE_PATH,
                    EditorProjectSourceKind::Scene,
                    EditorProjectSourceOpenCapability::Scene,
                    application.state(),
                    &stage,
                )?;
                require_browser_entry(
                    snapshot,
                    EXTERNAL_PREFAB_PATH,
                    EditorProjectSourceKind::Prefab,
                    EditorProjectSourceOpenCapability::Prefab,
                    application.state(),
                    &stage,
                )?;
                let baseline = BrowserBaseline {
                    status: browser.status(),
                    generation: snapshot.generation(),
                    entries: Arc::from(snapshot.entries()),
                    diagnostics: Arc::from(snapshot.diagnostics()),
                    usage: snapshot.usage(),
                };
                drop(frame);
                if !application.select_project_browser_sidebar() {
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the native Editor did not switch from the default Scene sidebar to Project",
                    ));
                }
                self.stage =
                    EditorGenericOpenCanaryStage::AwaitingProjectBrowserPresentation(baseline);
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorGenericOpenCanaryStage::AwaitingProjectBrowserPresentation(baseline) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                require_browser_baseline(application, &baseline, &stage)?;
                if !application.apply_project_browser_local_action(
                    EditorProjectBrowserLocalAction::SetPathFilter {
                        value: Arc::from(EXTERNAL_SCENE_FILTER),
                    },
                ) {
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the Project Browser did not accept its UI-neutral Scene path filter",
                    ));
                }
                self.stage = EditorGenericOpenCanaryStage::AwaitingSceneFilter(baseline);
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorGenericOpenCanaryStage::AwaitingSceneFilter(baseline) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                let frame = require_current_frame(application, &stage)?;
                let browser = frame.project_browser();
                require_path_filter(browser, EXTERNAL_SCENE_FILTER, application.state(), &stage)?;
                if browser.has_pending_work() {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                require_browser_matches_baseline(browser, &baseline, application.state(), &stage)?;
                require_filtered_row(
                    browser,
                    EXTERNAL_SCENE_PATH,
                    EditorProjectSourceKind::Scene,
                    application.state(),
                    &stage,
                )?;
                drop(frame);
                if !application.apply_project_browser_local_action(
                    EditorProjectBrowserLocalAction::Select {
                        relative_path: Some(Arc::from(EXTERNAL_SCENE_PATH)),
                    },
                ) {
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the filtered Project Browser did not select the closure-external Scene",
                    ));
                }
                self.stage = EditorGenericOpenCanaryStage::AwaitingSceneSelection(baseline);
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorGenericOpenCanaryStage::AwaitingSceneSelection(baseline) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                let frame = require_current_frame(application, &stage)?;
                let browser = frame.project_browser();
                require_selected_path(browser, EXTERNAL_SCENE_PATH, application.state(), &stage)?;
                require_browser_matches_baseline(browser, &baseline, application.state(), &stage)?;
                self.stage = EditorGenericOpenCanaryStage::AwaitingSceneOpen(baseline);
                Ok(project_browser_action(EditorProjectBrowserAction::Open {
                    relative_path: Arc::from(EXTERNAL_SCENE_PATH),
                }))
            }
            EditorGenericOpenCanaryStage::AwaitingSceneOpen(baseline) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                let frame = require_current_frame(application, &stage)?;
                let scene_document = require_document(
                    &frame,
                    EXTERNAL_SCENE_PATH,
                    EditorDocumentKind::Scene,
                    true,
                    application.state(),
                    &stage,
                )?;
                require_browser_matches_baseline(
                    frame.project_browser(),
                    &baseline,
                    application.state(),
                    &stage,
                )?;
                drop(frame);
                if !application.apply_project_browser_local_action(
                    EditorProjectBrowserLocalAction::SetPathFilter {
                        value: Arc::from(EXTERNAL_PREFAB_FILTER),
                    },
                ) {
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the Project Browser did not accept its UI-neutral Prefab path filter",
                    ));
                }
                self.stage =
                    EditorGenericOpenCanaryStage::AwaitingPrefabFilter(SceneDocumentState {
                        browser: baseline,
                        scene_document,
                    });
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorGenericOpenCanaryStage::AwaitingPrefabFilter(scene) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                let frame = require_current_frame(application, &stage)?;
                let browser = frame.project_browser();
                require_path_filter(browser, EXTERNAL_PREFAB_FILTER, application.state(), &stage)?;
                if browser.has_pending_work() {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                require_browser_matches_baseline(
                    browser,
                    &scene.browser,
                    application.state(),
                    &stage,
                )?;
                require_filtered_row(
                    browser,
                    EXTERNAL_PREFAB_PATH,
                    EditorProjectSourceKind::Prefab,
                    application.state(),
                    &stage,
                )?;
                require_document_id(
                    &frame,
                    scene.scene_document,
                    EXTERNAL_SCENE_PATH,
                    EditorDocumentKind::Scene,
                    true,
                    application.state(),
                    &stage,
                )?;
                drop(frame);
                if !application.apply_project_browser_local_action(
                    EditorProjectBrowserLocalAction::Select {
                        relative_path: Some(Arc::from(EXTERNAL_PREFAB_PATH)),
                    },
                ) {
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the filtered Project Browser did not select the closure-external Prefab",
                    ));
                }
                self.stage = EditorGenericOpenCanaryStage::AwaitingPrefabSelection(scene);
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorGenericOpenCanaryStage::AwaitingPrefabSelection(scene) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                let frame = require_current_frame(application, &stage)?;
                let browser = frame.project_browser();
                require_selected_path(browser, EXTERNAL_PREFAB_PATH, application.state(), &stage)?;
                require_browser_matches_baseline(
                    browser,
                    &scene.browser,
                    application.state(),
                    &stage,
                )?;
                self.stage = EditorGenericOpenCanaryStage::AwaitingPrefabOpen(scene);
                Ok(project_browser_action(EditorProjectBrowserAction::Open {
                    relative_path: Arc::from(EXTERNAL_PREFAB_PATH),
                }))
            }
            EditorGenericOpenCanaryStage::AwaitingPrefabOpen(scene) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                let frame = require_current_frame(application, &stage)?;
                require_document_id(
                    &frame,
                    scene.scene_document,
                    EXTERNAL_SCENE_PATH,
                    EditorDocumentKind::Scene,
                    false,
                    application.state(),
                    &stage,
                )?;
                let prefab_document = require_document(
                    &frame,
                    EXTERNAL_PREFAB_PATH,
                    EditorDocumentKind::Prefab,
                    true,
                    application.state(),
                    &stage,
                )?;
                require_browser_matches_baseline(
                    frame.project_browser(),
                    &scene.browser,
                    application.state(),
                    &stage,
                )?;
                let documents = OpenDocumentsState {
                    browser: scene.browser,
                    scene_document: scene.scene_document,
                    prefab_document,
                };
                self.stage = EditorGenericOpenCanaryStage::AwaitingEquivalentRefresh(documents);
                Ok(project_browser_action(EditorProjectBrowserAction::Refresh))
            }
            EditorGenericOpenCanaryStage::AwaitingEquivalentRefresh(documents) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                let frame = require_current_frame(application, &stage)?;
                let browser = frame.project_browser();
                if browser.has_pending_work() {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                let snapshot =
                    require_ready_browser_snapshot(browser, application.state(), &stage)?;
                if snapshot.generation() <= documents.browser.generation
                    || snapshot.entries() != documents.browser.entries.as_ref()
                    || snapshot.diagnostics() != documents.browser.diagnostics.as_ref()
                    || snapshot.usage() != documents.browser.usage
                    || browser.status() != documents.browser.status
                {
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the equivalent Project Browser Refresh did not publish a newer generation with identical snapshot facts",
                    ));
                }
                require_open_documents(&frame, &documents, application.state(), &stage)?;
                let refreshed = RefreshedBrowserState {
                    documents,
                    status: browser.status(),
                    generation: snapshot.generation(),
                    diagnostics: Arc::from(snapshot.diagnostics()),
                    usage: snapshot.usage(),
                };
                self.stage = EditorGenericOpenCanaryStage::AwaitingFailureRevalidation(refreshed);
                Ok(project_browser_action(EditorProjectBrowserAction::Refresh))
            }
            EditorGenericOpenCanaryStage::AwaitingFailureRevalidation(refreshed) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                let frame = require_current_frame(application, &stage)?;
                let browser = frame.project_browser();
                require_last_good_snapshot(browser, &refreshed, application.state(), &stage)?;
                if browser.status() == EditorProjectBrowserStatus::Unavailable {
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the failure Refresh became unavailable before the scripts root was replaced",
                    ));
                }
                let Some(progress) = browser.scan_progress() else {
                    if browser.has_pending_work() {
                        return Ok(NativeEditorCanaryAdvance::idle());
                    }
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the failure Refresh stopped before lineage revalidation",
                    ));
                };
                if progress.phase() != EditorProjectBrowserDiagnosticPhase::LineageRevalidation {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                drop(frame);
                self.guarded_project
                    .replace_scripts_root()
                    .map_err(canary_refresh_mutation_error)?;
                self.stage = EditorGenericOpenCanaryStage::AwaitingLastGoodFailure(refreshed);
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            EditorGenericOpenCanaryStage::AwaitingLastGoodFailure(refreshed) => {
                require_project_browser_sidebar_presentation(application, &stage)?;
                let frame = require_current_frame(application, &stage)?;
                let browser = frame.project_browser();
                if browser.has_pending_work() {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                if browser.status() != EditorProjectBrowserStatus::Unavailable {
                    return Err(canary_state_error(
                        &stage,
                        application.state(),
                        "the scripts-root replacement did not make the Refresh unavailable",
                    ));
                }
                require_last_good_snapshot(browser, &refreshed, application.state(), &stage)?;
                require_safe_lineage_failure(
                    browser.failure(),
                    self.guarded_project.root(),
                    application.state(),
                    &stage,
                )?;
                require_open_documents(&frame, &refreshed.documents, application.state(), &stage)?;
                let evidence = EditorGenericOpenCanaryEvidence {
                    initial_status: refreshed.documents.browser.status,
                    initial_generation: refreshed.documents.browser.generation,
                    refreshed_status: refreshed.status,
                    refreshed_generation: refreshed.generation,
                    entry_count: refreshed.documents.browser.entries.len(),
                    scene_document: refreshed.documents.scene_document,
                    prefab_document: refreshed.documents.prefab_document,
                };
                self.stage = EditorGenericOpenCanaryStage::Complete(evidence);
                Ok(NativeEditorCanaryAdvance::exit())
            }
            EditorGenericOpenCanaryStage::Complete(_) => Ok(NativeEditorCanaryAdvance::idle()),
        }
    }

    pub(super) const fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn timeout_error(
        &self,
        application_state: EditorApplicationState,
    ) -> EditorRunError {
        EditorRunError::Platform(format!(
            "the generic Open/Recovery canary timed out in {:?}; application state={application_state:?}",
            self.stage
        ))
    }

    pub(super) fn finish(self) -> Result<(), EditorRunError> {
        let evidence = match self.stage {
            EditorGenericOpenCanaryStage::Complete(evidence) => evidence,
            stage => {
                return Err(EditorRunError::Platform(format!(
                    "the native Editor exited before the generic Open/Recovery canary completed ({stage:?})"
                )));
            }
        };
        println!(
            "{{\"kind\":\"nara_editor_generic_open_canary\",\"version\":2,\"window\":\"single\",\"project_manager\":\"presented\",\"recovery\":\"presented\",\"workspace\":\"authoring_degraded\",\"project_browser\":{{\"sidebar\":\"project\",\"initial_status\":\"{}\",\"initial_generation\":{},\"entry_count\":{},\"scene\":{{\"filter\":\"{}\",\"path\":\"{}\",\"document\":{}}},\"prefab\":{{\"filter\":\"{}\",\"path\":\"{}\",\"document\":{},\"scene_retained\":true}},\"refresh\":{{\"status\":\"{}\",\"generation\":{},\"generation_monotonic\":true,\"entries_equal\":true}},\"last_good\":{{\"status\":\"unavailable\",\"generation\":{},\"retained\":true,\"failure_code\":\"{}\",\"phase\":\"lineage_revalidation\",\"relative_source\":\"{}\",\"diagnostic_safe\":true}}}},\"runtime\":\"external_host_unavailable\",\"project_content\":\"not_installed\",\"play\":\"empty\",\"result\":\"ok\"}}",
            ready_status_label(evidence.initial_status),
            evidence.initial_generation,
            evidence.entry_count,
            EXTERNAL_SCENE_FILTER,
            EXTERNAL_SCENE_PATH,
            evidence.scene_document.raw(),
            EXTERNAL_PREFAB_FILTER,
            EXTERNAL_PREFAB_PATH,
            evidence.prefab_document.raw(),
            ready_status_label(evidence.refreshed_status),
            evidence.refreshed_generation,
            evidence.refreshed_generation,
            LINEAGE_CHANGED_CODE,
            SCRIPTS_ROOT_PATH,
        );
        Ok(())
    }

    fn require_before_deadline(
        &self,
        application_state: EditorApplicationState,
    ) -> Result<(), EditorRunError> {
        if Instant::now() < self.deadline {
            return Ok(());
        }
        Err(self.timeout_error(application_state))
    }

    fn observe_window(
        &mut self,
        window_id: WindowId,
        application_state: EditorApplicationState,
    ) -> Result<(), EditorRunError> {
        match self.window_id {
            None => {
                self.window_id = Some(window_id);
                Ok(())
            }
            Some(expected) if expected == window_id => Ok(()),
            Some(expected) => Err(EditorRunError::Platform(format!(
                "the generic Open/Recovery canary changed native windows; canary stage={:?}, application state={application_state:?}, expected window={expected:?}, presented window={window_id:?}",
                self.stage
            ))),
        }
    }
}
