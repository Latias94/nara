mod close;
mod layout_file;

#[cfg(test)]
mod tests;

use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use anyhow::{Context as _, Result};
use open_gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, TitlebarOptions, WeakEntity,
    WindowBounds, WindowOptions, px, size,
};
use open_gpui_docking::{
    DockPane, DockSurface,
    dockspace::{
        model::{
            DockspaceAxis, DockspaceLayout, DockspaceNode, DockspaceRootLayout,
            DockspaceSurfaceLayout, DockspaceView, ItemId, RootId, SurfaceId,
        },
        policy::DockPolicy,
        runtime::{DockspaceDocumentBootstrap, DockspaceDocumentId, DockspaceSession},
    },
    persistence::{
        DockHostDocument, DockLogicalPlacementRect, DockLogicalViewportPlacement,
        DockLogicalWindowPlacement, DockSurfaceKeyBinding, DockWindowPlacementState,
        DockWorkspaceDocument, RestoredDockWorkspace,
    },
};
use open_gpui_wgpu::SharedWgpuContext;

use crate::{
    inspector::InspectorPanel,
    panels::{AssetPanel, ConsolePanel, HierarchyPanel},
    theme,
    viewport::ViewportPanel,
    workspace::Workspace,
};

pub(crate) const ROOT: SurfaceId = SurfaceId::new(1);
const PANES: [&str; 5] = ["hierarchy", "scene", "inspector", "assets", "console"];

pub(crate) struct OpenEditor {
    pub(crate) window: AnyWindowHandle,
    pub(crate) dock: DockSurface,
    pub(crate) workspace: Entity<Workspace>,
    pub(crate) viewport: Entity<ViewportPanel>,
}

pub fn open(
    workspace: Workspace,
    shared: &SharedWgpuContext,
    persist: bool,
    cx: &mut App,
) -> Result<OpenEditor> {
    let project = workspace.project.directory().to_path_buf();
    let title = format!(
        "{} · Nara Editor",
        workspace.project.manifest().project.name
    );
    let layout_path = persist.then(|| project.join(".nara").join("editor-layout.json"));
    let workspace = cx.new(|_| workspace);
    let mut policy = DockPolicy::default();
    policy.set_allow_native_surfaces(cx.viewport_capabilities().platform_viewport_windows);
    policy.set_allow_contained_floating(true);

    let saved = layout_path
        .as_deref()
        .map_or(Ok(None), |path| restore(path, policy.clone()));
    let (session, restored) = match saved {
        Ok(Some(restored)) => (restored.session, Some(restored.host)),
        Ok(None) => (default_session(policy)?, None),
        Err(error) => {
            warn_layout(
                &workspace.downgrade(),
                format!("Layout restored to defaults: {error:#}"),
                cx,
            );
            (default_session(policy)?, None)
        }
    };
    let items = PANES.map(|key| {
        session
            .item_id_for_external_key(key)
            .expect("all workspace keys registered")
    });
    let preview = nara_editor_viewport::EditorViewport::new(shared)?;
    let view = cx.new(|cx| ViewportPanel::new(workspace.clone(), preview, cx));
    let receiver = view.read(cx).receiver();
    let viewport_stats = view.read(cx).stats_entity().clone();
    let inspector = cx.new(|cx| InspectorPanel::new(workspace.clone(), cx));
    let panes = [
        DockPane::new(
            items[0],
            "Hierarchy",
            cx.new(|cx| HierarchyPanel::new(workspace.clone(), cx)),
        ),
        DockPane::new(items[1], "Scene View", view.clone()).with_content_receiver(receiver),
        DockPane::new(items[2], "Inspector", inspector.clone()),
        DockPane::new(
            items[3],
            "Assets",
            cx.new(|cx| AssetPanel::new(workspace.clone(), cx)),
        ),
        DockPane::new(
            items[4],
            "Console",
            cx.new(|cx| ConsolePanel::new(workspace.clone(), viewport_stats, cx)),
        ),
    ];
    let dock = DockSurface::new_managed(session, panes, theme::docking(), cx)?;
    close::keep_required_panes(&dock, workspace.downgrade(), cx)?;
    let main = if let Some(host) = restored {
        let roots = dock.open_restored_native_roots(
            &host,
            ROOT,
            |binding, cx| options(&title, cx, binding.surface == ROOT),
            cx,
        )?;
        roots
            .get(&ROOT)
            .copied()
            .context("The editor root was not restored")?
    } else {
        dock.open_native_root(ROOT, true, options(&title, cx, true), cx)?
    };
    let closing = close::guard_editor_window(
        &dock,
        main,
        workspace.downgrade(),
        inspector.downgrade(),
        layout_path.clone(),
        cx,
    )?;
    if let Some(path) = layout_path {
        persist_on_layout_changes(&dock, path, workspace.downgrade(), closing, cx)?;
    }
    cx.activate(true);
    Ok(OpenEditor {
        window: main,
        dock,
        workspace,
        viewport: view,
    })
}

fn default_session(policy: DockPolicy) -> Result<DockspaceSession> {
    let mut keys = DockspaceDocumentBootstrap::new(DockspaceDocumentId::from_bytes(
        *uuid::Uuid::new_v4().as_bytes(),
    ));
    let [hierarchy, scene, inspector, assets, console] = PANES
        .map(|key| keys.ensure_item(key))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .expect("five editor panes");
    let tool_strip = DockspaceNode::tabs([assets, console]);
    let middle = DockspaceNode::split(
        DockspaceAxis::Vertical,
        [
            (DockspaceNode::central_tabs([scene]), 0.75),
            (tool_strip, 0.25),
        ],
    )?;
    let body = DockspaceNode::split(
        DockspaceAxis::Horizontal,
        [
            (DockspaceNode::tabs([hierarchy]), 0.19),
            (middle, 0.55),
            (DockspaceNode::tabs([inspector]), 0.26),
        ],
    )?;
    let layout = DockspaceLayout::new([DockspaceSurfaceLayout::new(
        ROOT,
        DockspaceRootLayout::new(RootId::new(1), body),
    )])?;
    Ok(DockspaceSession::from_persistent_layout(
        layout, policy, keys,
    )?)
}

fn restore(path: &Path, policy: DockPolicy) -> Result<Option<RestoredDockWorkspace>> {
    let Some(bytes) = layout_file::read(path)? else {
        return Ok(None);
    };
    (|| -> Result<_> {
        let restored =
            DockWorkspaceDocument::restore_json(&bytes, policy, |_, key| PANES.contains(&key))?;
        require_full_layout(&restored.session.view(), |key| {
            restored.session.item_id_for_external_key(key)
        })?;
        Ok(Some(restored))
    })()
    .with_context(|| format!("Invalid editor layout at {}", path.display()))
}

fn options(title: &str, cx: &App, main: bool) -> WindowOptions {
    let dimensions = if main {
        size(px(1460.0), px(940.0))
    } else {
        size(px(560.0), px(720.0))
    };
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None, dimensions, cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(title.to_owned().into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn persist_on_layout_changes(
    dock: &DockSurface,
    path: PathBuf,
    workspace: WeakEntity<Workspace>,
    close_state: Rc<Cell<close::RootCloseState>>,
    cx: &mut App,
) -> Result<()> {
    let sequence = Rc::new(Cell::new(0u64));
    dock.set_commit_handler(
        move |dock, _, cx| {
            if close_state.get() == close::RootCloseState::Closing {
                return;
            }
            let revision = sequence.get().wrapping_add(1);
            sequence.set(revision);
            let sequence = sequence.clone();
            let dock = dock.clone();
            let workspace = workspace.clone();
            let path = path.clone();
            let close_state = close_state.clone();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(220))
                    .await;
                if close_state.get() == close::RootCloseState::Closing || sequence.get() != revision
                {
                    return;
                }
                cx.update(|cx| persist_layout(&dock, &path, &workspace, cx));
            })
            .detach();
        },
        cx,
    )
}

fn persist_layout(
    dock: &DockSurface,
    path: &Path,
    workspace: &WeakEntity<Workspace>,
    cx: &mut App,
) {
    if let Err(error) = save_layout(dock, path, cx) {
        warn_layout(workspace, format!("Layout save: {error:#}"), cx);
    }
}

fn warn_layout(workspace: &WeakEntity<Workspace>, message: String, cx: &mut App) {
    eprintln!("{message}");
    let _ = workspace.update(cx, |workspace, cx| workspace.warn(message, cx));
}

pub(crate) fn save_layout(dock: &DockSurface, path: &Path, cx: &mut App) -> Result<()> {
    let document = capture_layout(dock, cx)
        .with_context(|| format!("Could not capture editor layout for {}", path.display()))?;
    layout_file::write(path, &document.to_json()?)
}

fn capture_layout(dock: &DockSurface, cx: &mut App) -> Result<DockWorkspaceDocument> {
    dock.with_view(|view| require_full_layout(&view, |key| dock.item_id_for_external_key(key)))?;
    let inspection = dock.inspection()?;
    let windows = cx.windows();
    let surfaces = inspection
        .surfaces
        .iter()
        .map(|surface| {
            let logical_placement = inspection.mounts.iter().find_map(|mount| {
                if !mount
                    .regions
                    .iter()
                    .any(|region| region.surface == surface.surface)
                {
                    return None;
                }
                let window = windows
                    .iter()
                    .find(|window| window.window_id().as_u64() == mount.window_id)?;
                window
                    .update(cx, |_, window, _| {
                        let bounds = window.window_bounds();
                        let state = match bounds {
                            WindowBounds::Windowed(_) => DockWindowPlacementState::Windowed,
                            WindowBounds::Maximized(_) => DockWindowPlacementState::Maximized,
                            WindowBounds::Fullscreen(_) => DockWindowPlacementState::Fullscreen,
                        };
                        let bounds = bounds.get_bounds();
                        DockLogicalViewportPlacement {
                            display_id: None,
                            window_bounds: Some(DockLogicalWindowPlacement {
                                state,
                                bounds: DockLogicalPlacementRect {
                                    x: bounds.origin.x.to_f64() as f32,
                                    y: bounds.origin.y.to_f64() as f32,
                                    width: bounds.size.width.to_f64() as f32,
                                    height: bounds.size.height.to_f64() as f32,
                                },
                            }),
                            host_bounds: None,
                        }
                    })
                    .ok()
            });
            DockSurfaceKeyBinding {
                space_key: format!("editor-{}", surface.surface.get()),
                surface: surface.surface,
                logical_placement,
                central_policy: None,
            }
        })
        .collect();
    Ok(dock.capture_document(DockHostDocument {
        surfaces,
        migration: None,
    })?)
}

fn require_full_layout(
    view: &DockspaceView<'_>,
    item_id: impl Fn(&str) -> Option<ItemId>,
) -> Result<()> {
    anyhow::ensure!(view.surface(ROOT).is_some(), "The editor root is missing");
    let mut pane_surfaces = [ROOT; PANES.len()];
    for (key, surface) in PANES.into_iter().zip(&mut pane_surfaces) {
        let pane = item_id(key)
            .filter(|item| !view.item_is_hidden(*item))
            .and_then(|item| view.item(item))
            .with_context(|| format!("Required editor pane {key:?} is missing or hidden"))?;
        *surface = pane.surface();
    }
    for surface in view.surfaces().map(|surface| surface.id()) {
        anyhow::ensure!(
            surface == ROOT || pane_surfaces.contains(&surface),
            "Floating editor window {surface:?} has no required pane"
        );
    }
    Ok(())
}
