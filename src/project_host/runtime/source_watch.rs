//! Optional native observation feeding the original hosted source Scene owner.

use std::path::{Path, PathBuf};

use crate::fs::DirectoryCapability;
use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginProductCapability, PluginShutdownContext, PluginShutdownObligationId,
    RuntimeCloseParticipantId, RuntimeFaultDetail, StartupStage,
};
use nara_asset::watch::{
    AssetWatchEventQueue, AssetWatchLifecycleStats, AssetWatchPath, AssetWatchQueueLimits,
    AssetWatchQueueStats, AssetWatchRuntime, AssetWatchRuntimeObserver, AssetWatchTranslator,
    AssetWatcher,
};
use nara_asset::{AssetPathScope, AssetSourceChangeKind, AssetSourceRoot};
use nara_ecs::{Mut, Res, Resource, World, schedule::IntoScheduleConfigs};

use super::{
    HostFault, ProjectScenes,
    scene_requests::{input_reload, poll_project_scene_requests},
};
use crate::project_host::{
    ProjectContentLoader, ProjectSettingsCandidate, open_explicit_product_project_root,
};

pub const PROJECT_SCENE_WATCH_PLUGIN_ID: PluginId = PluginId::new("nara.project.scene-watch");
const OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.project.scene-watch.native");
const PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.project.scene-watch.native");
const MAX_COVERAGE_PATHS: usize = 128;
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PROJECT_SCENE_WATCH_PLUGIN_ID, PluginCategory::Asset)
        .requires_plugins(&[
            crate::startup_scene::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
            nara_app::tasks::TASK_PLUGIN_ID,
        ])
        .conflicts(&[nara_asset::watch::ASSET_WATCH_PLUGIN_ID])
        .requires_product_capabilities(&[
            PluginProductCapability::new("runtime-2d"),
            PluginProductCapability::new("asset-watch"),
        ])
        .shutdown_obligations(&[OBLIGATION]);

/// Native source coverage for a hosted Scene, independent of generic asset reloading.
///
/// Add this configured plugin to the product recipe. The explicit root must identify the same
/// directory as the source loader. Construction and recipe resolution acquire no native watcher;
/// actual source Host admission binds coverage before requesting initial reconciliation.
/// Games still inspect, overlay, commit and acknowledge the original Scene requests themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSceneWatchPlugin {
    root: PathBuf,
    limits: AssetWatchQueueLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProjectSceneWatchConfigurationError {
    #[error("Scene source watching requires an explicit absolute project root")]
    AbsoluteRootRequired,
}

impl ProjectSceneWatchPlugin {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, ProjectSceneWatchConfigurationError> {
        let root = root.into();
        if !root.is_absolute() {
            return Err(ProjectSceneWatchConfigurationError::AbsoluteRootRequired);
        }
        Ok(Self {
            root,
            limits: AssetWatchQueueLimits::default(),
        })
    }

    #[must_use]
    pub const fn with_queue_limits(mut self, limits: AssetWatchQueueLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Returns the canonical recipe bytes for this root and its queue limits.
    ///
    /// Pass these bytes to [`crate::ProductRecipe::add_configured_plugin`] with a factory that
    /// clones this configuration. Encoding preserves the native path and does not open it.
    #[must_use]
    pub fn configuration_bytes(&self) -> Vec<u8> {
        let mut output = Vec::new();
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            output.push(0);
            let bytes = self.root.as_os_str().encode_wide().count() as u64 * 2;
            output.extend_from_slice(&bytes.to_le_bytes());
            for unit in self.root.as_os_str().encode_wide() {
                output.extend_from_slice(&unit.to_le_bytes());
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            output.push(1);
            let bytes = self.root.as_os_str().as_bytes();
            output.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            output.extend_from_slice(bytes);
        }
        #[cfg(not(any(windows, unix)))]
        {
            output.push(2);
            let bytes = self.root.as_os_str().as_encoded_bytes();
            output.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            output.extend_from_slice(bytes);
        }
        output.extend_from_slice(&(self.limits.events().get() as u64).to_le_bytes());
        output.extend_from_slice(&(self.limits.retained_bytes().get() as u64).to_le_bytes());
        output
    }
}

impl Plugin for ProjectSceneWatchPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let native = AssetWatchRuntime::register(app, OBLIGATION, PARTICIPANT)?;
        app.insert_resource(SourceWatchDriver {
            configuration: self.clone(),
            native,
            binding: None,
            current: None,
            rescan_pending: false,
            retry_requested: false,
            restarts: 0,
            last_queue: AssetWatchEventQueue::with_limits(self.limits).stats(),
            error: None,
        })?
        .add_systems(StartupStage::Runtime, require_source_binding)?
        .add_systems(
            CoreStage::TaskUpdate,
            poll_source_watch.before(poll_project_scene_requests),
        )?;
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        context.world_mut().remove_resource::<SourceWatchDriver>();
        Ok(())
    }
}

/// Watching reports native coverage; it does not imply that the game committed reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSceneWatchState {
    Unbound,
    Watching,
    RetryRequired,
}

/// Read-only observation of native coverage and pending Scene-ingress work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSceneWatchStatus {
    pub state: ProjectSceneWatchState,
    pub rescan_pending: bool,
    pub queue: AssetWatchQueueStats,
    pub native: AssetWatchLifecycleStats,
    pub restarts: u64,
    pub error: Option<String>,
}

struct Binding {
    root: AssetSourceRoot,
    capability: DirectoryCapability,
    desired: Vec<AssetWatchPath>,
}

struct Coverage {
    queue: AssetWatchEventQueue,
    _watcher: AssetWatcher,
}

#[derive(Resource)]
pub(crate) struct SourceWatchDriver {
    configuration: ProjectSceneWatchPlugin,
    native: AssetWatchRuntime,
    binding: Option<Binding>,
    current: Option<Coverage>,
    rescan_pending: bool,
    retry_requested: bool,
    restarts: u64,
    last_queue: AssetWatchQueueStats,
    error: Option<String>,
}

impl SourceWatchDriver {
    pub(crate) fn status(&self) -> ProjectSceneWatchStatus {
        ProjectSceneWatchStatus {
            state: if self.binding.is_none() {
                ProjectSceneWatchState::Unbound
            } else if self.current.is_some() {
                ProjectSceneWatchState::Watching
            } else {
                ProjectSceneWatchState::RetryRequired
            },
            rescan_pending: self.rescan_pending,
            queue: self
                .current
                .as_ref()
                .map_or(self.last_queue, |current| current.queue.stats()),
            native: self.native.stats(),
            restarts: self.restarts,
            error: self.error.clone(),
        }
    }

    pub(crate) fn retirement(&self) -> AssetWatchRuntimeObserver {
        self.native.observer()
    }

    pub(crate) fn retry(&mut self) -> bool {
        if self.binding.is_none() || self.current.is_some() || self.retry_requested {
            return false;
        }
        self.retry_requested = true;
        true
    }

    fn replace_coverage(&mut self) -> Result<(), String> {
        let binding = self.binding.as_ref().ok_or("source watcher is unbound")?;
        let reopened = open_explicit_product_project_root(binding.root.root())
            .map_err(|error| error.to_string())?;
        if !binding
            .capability
            .same_object(&reopened)
            .map_err(|error| error.to_string())?
        {
            return Err("source watch root no longer identifies the admitted directory".into());
        }
        let paths = coverage_paths(binding.root.root(), &binding.desired)?;
        let queue = AssetWatchEventQueue::with_limits(self.configuration.limits);
        let watcher = self
            .native
            .watch_paths(&binding.root, &paths, queue.sender())
            .map_err(|error| error.to_string())?;
        self.current = Some(Coverage {
            queue,
            _watcher: watcher,
        });
        self.rescan_pending = true;
        self.error = None;
        Ok(())
    }

    fn recover(&mut self) {
        self.retry_requested = false;
        match self.replace_coverage() {
            Ok(()) => self.restarts = self.restarts.saturating_add(1),
            Err(error) => {
                self.current = None;
                self.rescan_pending = true;
                self.error = Some(error);
            }
        }
    }

    fn poll(&mut self, world: &mut World) {
        if self.retry_requested {
            self.recover();
        }
        if let Some(current) = &mut self.current {
            let drain = current.queue.drain();
            self.last_queue = current.queue.stats();
            let root = &self.binding.as_ref().unwrap().root;
            let mut uncertain = drain.rescan_required();
            for event in drain.events() {
                match AssetWatchTranslator.translate_event(root, event) {
                    Ok(changes) => {
                        for change in changes {
                            if input_reload::queue(world, Some(&change)) {
                                continue;
                            }
                            if change.kind() == AssetSourceChangeKind::MetaModified
                                || change.raw_metadata_changed()
                            {
                                self.rescan_pending = true;
                            } else if let Err(error) = ProjectScenes::scope(world, |scenes| {
                                scenes.notify_source_change(&change)
                            }) {
                                self.rescan_pending = true;
                                self.error = Some(error.to_string());
                            }
                        }
                    }
                    Err(_) => uncertain = true,
                }
            }
            if uncertain {
                self.recover();
            }
        }
        if self.rescan_pending && self.current.is_some() {
            match ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()) {
                Ok(()) => {
                    input_reload::queue(world, None);
                    self.rescan_pending = false;
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }
}

pub(super) fn bind_source_watch(
    world: &mut World,
    candidate: &ProjectSettingsCandidate,
    loader: &ProjectContentLoader,
) -> Result<(), HostFault> {
    let Some(mut driver) = world.get_resource_mut::<SourceWatchDriver>() else {
        return Ok(());
    };
    let fail = |code, summary| HostFault::new(super::single_error(code, summary));
    if driver.binding.is_some() {
        return Err(fail(
            "project.scene.watch-already-bound",
            "Source watcher is already bound",
        ));
    }
    let capability =
        open_explicit_product_project_root(&driver.configuration.root).map_err(|_| {
            fail(
                "project.scene.watch-root-unavailable",
                "Source watch root could not be opened",
            )
        })?;
    if !capability
        .same_object(&loader.project_root())
        .map_err(|_| {
            fail(
                "project.scene.watch-root-unavailable",
                "Source watch root could not be compared",
            )
        })?
    {
        return Err(fail(
            "project.scene.watch-root-mismatch",
            "Source watch root differs from the admitted project",
        ));
    }
    let native_root = driver.configuration.root.canonicalize().map_err(|_| {
        fail(
            "project.scene.watch-root-unavailable",
            "Source watch root could not be resolved",
        )
    })?;
    let settings = candidate.settings();
    let mut desired = vec![
        AssetWatchPath::new(native_root.clone(), false),
        AssetWatchPath::new(native_root.join(settings.paths.assets.as_str()), true),
        AssetWatchPath::new(native_root.join(settings.paths.scenes.as_str()), true),
        AssetWatchPath::new(native_root.join(settings.paths.prefabs.as_str()), true),
    ];
    for path in [
        settings.authoring_schema_bundle.as_ref(),
        settings.input.action_map.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        let path = native_root.join(path.as_str());
        desired.push(AssetWatchPath::new(
            path.parent().unwrap().to_owned(),
            false,
        ));
    }
    driver.binding = Some(Binding {
        root: AssetSourceRoot::new(native_root, AssetPathScope::ROOT),
        capability,
        desired,
    });
    driver.replace_coverage().map_err(|error| {
        driver.error = Some(error);
        fail(
            "project.scene.watch-start-failed",
            "Native Scene source coverage could not be established",
        )
    })
}

fn coverage_paths(root: &Path, desired: &[AssetWatchPath]) -> Result<Vec<AssetWatchPath>, String> {
    let mut paths: Vec<AssetWatchPath> = Vec::with_capacity(desired.len());
    for selected in desired {
        let mut path = selected.path();
        let mut recursive = selected.recursive();
        while !path.try_exists().map_err(|error| error.to_string())? {
            path = path
                .parent()
                .ok_or("source watch directory has no parent")?;
            recursive = false;
        }
        if !path.starts_with(root) {
            return Err("source watch directory lies outside the admitted project".into());
        }
        let mut ancestors = path
            .ancestors()
            .take_while(|ancestor| ancestor.starts_with(root))
            .collect::<Vec<_>>();
        ancestors.reverse();
        for ancestor in ancestors {
            let recursive = ancestor == path && recursive;
            if paths.iter().any(|existing| {
                existing.path() == ancestor && (!recursive || existing.recursive())
                    || existing.recursive() && ancestor.starts_with(existing.path())
            }) {
                continue;
            }
            paths.retain(|existing| {
                !(existing.path() == ancestor || recursive && existing.path().starts_with(ancestor))
            });
            paths.push(AssetWatchPath::new(ancestor.to_owned(), recursive));
        }
    }
    if paths.len() > MAX_COVERAGE_PATHS {
        return Err(format!(
            "Scene source coverage exceeds {MAX_COVERAGE_PATHS} native paths"
        ));
    }
    Ok(paths)
}

fn require_source_binding(
    driver: Res<SourceWatchDriver>,
) -> Result<(), nara_ecs::bevy_ecs::error::BevyError> {
    if driver.binding.is_some() && driver.current.is_some() {
        return Ok(());
    }
    Err(RuntimeFaultDetail::new(
        "project.scene.watch-source-host-required",
        "Native Scene watching requires bound source Host admission",
        "nara.project.scene-watch",
    )
    .expect("source-authored diagnostic metadata is valid")
    .into_bevy_error())
}

fn poll_source_watch(world: &mut World) -> Result<(), nara_ecs::bevy_ecs::error::BevyError> {
    if !world.contains_resource::<SourceWatchDriver>() {
        return Ok(());
    }
    if crate::ecs::transaction::validate_resource_scope::<SourceWatchDriver>(world).is_err() {
        let mut driver = world.resource_mut::<SourceWatchDriver>();
        driver.current = None;
        driver.rescan_pending = true;
        driver.error = Some("Scene source watcher lifecycle scope was refused".into());
        return Err(RuntimeFaultDetail::new(
            "project.scene.watch-lifecycle-unsupported",
            "Scene source observation cannot safely access its lifecycle owner",
            "nara.project.scene-watch",
        )
        .expect("source-authored diagnostic metadata is valid")
        .into_bevy_error());
    }
    world.resource_scope(|world, mut driver: Mut<SourceWatchDriver>| driver.poll(world));
    Ok(())
}

#[cfg(test)]
mod tests;
