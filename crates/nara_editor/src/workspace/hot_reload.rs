use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, Result};
use nara_asset::watch::{AssetWatcher, RELOAD_RETRY_DELAYS, is_transient_reload_error};
use nara_core::{AssetId, vfs::DirAssetSource};
use nara_editor_core::{EditorProject, EditorSession, ProjectError};
use nara_editor_viewport::PreviewAssets;
use open_gpui::{App, BackgroundExecutor, Context, Task, WeakEntity};

use super::Workspace;
use crate::inspector::InspectorPanel;

const PREVIEW: u8 = 1;
const EXTERNAL: u8 = 2;
const DISCOVER: u8 = 4;
const WATCH_INTERVAL: Duration = Duration::from_millis(20);
const ACTIVE_EDIT_INTERVAL: Duration = Duration::from_millis(75);

#[derive(Default)]
pub(super) struct HotReload {
    assets: Arc<PreviewAssets>,
    requests: Arc<AtomicU8>,
    worker: Option<Task<()>>,
    last_error: Option<String>,
}

impl HotReload {
    pub(super) fn request_discovery(&self) -> bool {
        self.requests.fetch_or(DISCOVER, Ordering::Relaxed);
        self.worker.is_some()
    }
}

impl Workspace {
    pub(crate) fn watch_assets(
        &mut self,
        inspector: WeakEntity<InspectorPanel>,
        cx: &mut Context<Self>,
    ) {
        let directory = self.project.asset_directory().to_path_buf();
        let project_root = self.project.directory().to_path_buf();
        let initially_empty = self.project.asset_database().records().next().is_none();
        let requests = Arc::clone(&self.hot_reload.requests);
        self.request_preview_assets();
        self.hot_reload.worker = Some(cx.spawn(async move |workspace, cx| {
            let executor = cx.background_executor().clone();
            let watcher = executor
                .spawn(async move {
                    AssetWatcher::under(&project_root, &directory)
                        .with_context(|| format!("Could not watch '{}'", directory.display()))
                })
                .await;
            let mut watcher = match watcher {
                Ok(watcher) => {
                    if initially_empty {
                        requests.fetch_or(EXTERNAL, Ordering::Relaxed);
                    }
                    Some(watcher)
                }
                Err(error) => {
                    if workspace
                        .update(cx, |workspace, cx| workspace.warn(format!("{error:#}"), cx))
                        .is_err()
                    {
                        return;
                    }
                    None
                }
            };
            loop {
                let poll = executor.clone();
                let signal = Arc::clone(&requests);
                let changes;
                (watcher, changes) = executor
                    .spawn(async move {
                        let changes = wait_for_changes(watcher.as_ref(), &signal, &poll).await;
                        (watcher, changes)
                    })
                    .await;
                let flags = match changes {
                    Ok(flags) => flags,
                    Err(error) => {
                        if workspace
                            .update(cx, |workspace, cx| workspace.warn(format!("{error:#}"), cx))
                            .is_err()
                        {
                            return;
                        }
                        continue;
                    }
                };
                let mut delays = RELOAD_RETRY_DELAYS.into_iter().peekable();
                loop {
                    let input = loop {
                        match workspace.update(cx, |workspace, cx| {
                            if workspace.reload_blocked(&inspector, cx) {
                                return None;
                            }
                            workspace.reload_input().ok()
                        }) {
                            Ok(Some(input)) => break input,
                            Ok(None) => executor.timer(ACTIVE_EDIT_INTERVAL).await,
                            Err(_) => return,
                        }
                    };
                    let guard = input.guard.clone();
                    let prepared = executor.spawn(async move { input.prepare(flags) }).await;
                    let retry = prepared
                        .as_ref()
                        .err()
                        .filter(|error| is_transient_reload_error(error.as_ref()))
                        .and_then(|_| delays.peek().copied());
                    match workspace.update(cx, |workspace, cx| {
                        if workspace.reload_guard() != guard
                            || workspace.reload_blocked(&inspector, cx)
                        {
                            return false;
                        }
                        if retry.is_none() {
                            workspace.publish_reload(prepared, cx);
                        }
                        true
                    }) {
                        Err(_) => return,
                        Ok(false) => continue,
                        Ok(true) => {}
                    }
                    let Some(delay) = retry else { break };
                    delays.next();
                    executor.timer(delay).await;
                }
            }
        }));
    }

    pub(crate) fn preview_assets(&self) -> &PreviewAssets {
        &self.hot_reload.assets
    }

    pub(crate) fn request_preview_assets(&self) {
        self.hot_reload
            .requests
            .fetch_or(PREVIEW, Ordering::Relaxed);
    }

    fn reload_blocked(&self, inspector: &WeakEntity<InspectorPanel>, cx: &App) -> bool {
        self.session.gesture_active()
            || inspector
                .read_with(cx, |panel, cx| !panel.drafts(cx).is_empty())
                .unwrap_or(false)
    }

    fn reload_guard(&self) -> ReloadGuard {
        ReloadGuard {
            scene: self.revision,
            index: self.asset_scan_revision,
            active_asset: self.opened_asset,
            dirty: self.session.is_dirty(),
            path: self.session.document().path().map(PathBuf::from),
        }
    }

    fn reload_input(&self) -> Result<ReloadInput, nara_editor_core::EditorError> {
        Ok(ReloadInput {
            guard: self.reload_guard(),
            project: self.project.clone(),
            session: self.session.snapshot_for_background()?,
            assets: Arc::clone(&self.hot_reload.assets),
        })
    }

    fn publish_reload(&mut self, prepared: Result<PreparedReload>, cx: &mut Context<Self>) {
        let mut prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let error = format!("Asset reload kept the current preview: {error:#}");
                if self.hot_reload.last_error.as_ref() != Some(&error) {
                    self.hot_reload.last_error = Some(error.clone());
                    self.warn(error, cx);
                }
                return;
            }
        };
        self.hot_reload.last_error = None;
        let index_changed = prepared.project.asset_database() != self.project.asset_database();
        let assets_changed = !self.preview_assets().same_content(&prepared.assets);
        let path_changed = prepared.session.document().path() != self.session.document().path();
        if index_changed {
            let selected = self.selected_asset_id();
            self.project = prepared.project;
            self.update_asset_browser(selected);
        }
        if prepared.scene_changed || path_changed {
            prepared.session.keep_selection_from(&self.session);
            self.session = prepared.session;
        }
        self.hot_reload.assets = Arc::new(prepared.assets);
        if prepared.scene_changed {
            self.revision = self.revision.wrapping_add(1);
        }
        if assets_changed {
            self.preview_assets_revision = self.preview_assets_revision.wrapping_add(1);
        }
        if index_changed || assets_changed || prepared.scene_changed || path_changed {
            cx.notify();
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ReloadGuard {
    scene: u64,
    index: u64,
    active_asset: Option<AssetId>,
    dirty: bool,
    path: Option<PathBuf>,
}

struct ReloadInput {
    guard: ReloadGuard,
    project: EditorProject,
    session: EditorSession,
    assets: Arc<PreviewAssets>,
}

struct PreparedReload {
    project: EditorProject,
    session: EditorSession,
    scene_changed: bool,
    assets: PreviewAssets,
}

impl ReloadInput {
    fn prepare(mut self, flags: u8) -> Result<PreparedReload> {
        let mut scene_changed = false;
        if flags & (EXTERNAL | DISCOVER) != 0 {
            if flags & DISCOVER != 0 {
                self.project
                    .refresh_assets()
                    .map_err(project_reload_error)?;
            } else {
                self.project.rescan_assets().map_err(project_reload_error)?;
            }
            let schemas = self.session.registry().catalog().components();
            let disk = self
                .guard
                .active_asset
                .map(|id| {
                    self.project
                        .reload_document(id, schemas)
                        .map_err(project_reload_error)
                        .with_context(|| format!("Could not reload scene asset {id}"))
                })
                .transpose()?;
            let source = disk
                .as_ref()
                .filter(|_| !self.guard.dirty)
                .map_or(self.session.document().scene(), |disk| disk.scene());
            let prefabs = self
                .project
                .reload_prefabs(source, schemas)
                .map_err(project_reload_error)?;
            scene_changed = self.session.reload_sources(disk, prefabs)?;
        }
        let source = DirAssetSource::new(self.project.directory())?;
        let assets = PreviewAssets::load(
            self.session.preview_scene(),
            &source,
            self.project.asset_database(),
            &self.assets,
        )?;
        Ok(PreparedReload {
            project: self.project,
            session: self.session,
            scene_changed,
            assets,
        })
    }
}

fn project_reload_error(error: ProjectError) -> anyhow::Error {
    match error {
        ProjectError::Assets(error) => error.into(),
        ProjectError::Io(error) => error.into(),
        error => error.into(),
    }
}

async fn wait_for_changes(
    watcher: Option<&AssetWatcher>,
    requests: &AtomicU8,
    executor: &BackgroundExecutor,
) -> Result<u8> {
    loop {
        let external = watcher
            .map(AssetWatcher::take_changed)
            .transpose()
            .context("Could not refresh the asset watch")?
            .unwrap_or(false);
        let flags = requests.swap(0, Ordering::Relaxed) | if external { EXTERNAL } else { 0 };
        if flags != 0 {
            return Ok(flags);
        }
        executor.timer(WATCH_INTERVAL).await;
    }
}

#[cfg(all(test, windows))]
mod tests;
