use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, Result};
use nara_core::vfs::write_atomic;
use open_gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};
use open_gpui_docking::{
    DockPane, DockSurface,
    dockspace::{
        close::CloseDecision,
        model::{
            DockspaceAxis, DockspaceLayout, DockspaceNode, DockspaceRootLayout,
            DockspaceSurfaceLayout, RootId, SurfaceId,
        },
        policy::DockPolicy,
        runtime::{DockspaceDocumentBootstrap, DockspaceDocumentId, DockspaceSession},
    },
    persistence::{
        DockHostDocument, DockLogicalPlacementRect, DockLogicalViewportPlacement,
        DockLogicalWindowPlacement, DockSurfaceKeyBinding, DockWindowPlacementState,
        DockWorkspaceDocument,
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
    let layout_path = project.join(".nara").join("editor-layout.json");
    let workspace = cx.new(|_| workspace);
    let mut policy = DockPolicy::default();
    policy.set_allow_native_surfaces(cx.viewport_capabilities().platform_viewport_windows);
    policy.set_allow_contained_floating(true);

    let saved = if persist {
        restore(&layout_path, policy.clone())
    } else {
        Ok(None)
    };
    let (session, restored) = match saved {
        Ok(Some(restored)) => (restored.session, Some(restored.host)),
        Ok(None) => (default_session(policy)?, None),
        Err(error) => {
            workspace.update(cx, |workspace, cx| {
                workspace.warn(format!("Layout restored to defaults: {error:#}"), cx)
            });
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
    let panes = [
        DockPane::new(
            items[0],
            "Hierarchy",
            cx.new(|cx| HierarchyPanel::new(workspace.clone(), cx)),
        ),
        DockPane::new(items[1], "Scene View", view.clone()).with_content_receiver(receiver),
        DockPane::new(
            items[2],
            "Inspector",
            cx.new(|cx| InspectorPanel::new(workspace.clone(), cx)),
        ),
        DockPane::new(
            items[3],
            "Assets",
            cx.new(|cx| AssetPanel::new(workspace.clone(), cx)),
        ),
        DockPane::new(
            items[4],
            "Console",
            cx.new(|cx| ConsolePanel::new(workspace.clone(), cx)),
        ),
    ];
    let dock = DockSurface::new_managed(session, panes, theme::docking(), cx)?;
    let weak = workspace.downgrade();
    dock.set_close_handler(
        move |dock, plan, cx| {
            let dirty = weak
                .read_with(cx, |workspace, _| workspace.session.is_dirty())
                .unwrap_or(false);
            let result = dock.update(
                |frame| {
                    let decisions = frame
                        .pending_close_decisions(plan.request())
                        .collect::<Vec<_>>();
                    for decision in decisions {
                        let decision_value = if dirty {
                            CloseDecision::Veto
                        } else {
                            CloseDecision::Allow
                        };
                        frame.resolve_close(plan.request(), decision.token(), decision_value)?;
                    }
                    Ok(())
                },
                cx,
            );
            if dirty || result.is_err() {
                let _ = weak.update(cx, |workspace, cx| {
                    let message = result.err().map_or_else(
                        || "Save the current scene before closing it".into(),
                        |error| error.to_string(),
                    );
                    workspace.warn(message, cx);
                });
            }
        },
        cx,
    )?;

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
    if persist {
        persist_on_layout_changes(&dock, layout_path, workspace.downgrade(), cx)?;
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

fn restore(
    path: &Path,
    policy: DockPolicy,
) -> Result<Option<open_gpui_docking::persistence::RestoredDockWorkspace>> {
    let bytes = match fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        result => result?,
    };
    let restored =
        DockWorkspaceDocument::restore_json(&bytes, policy, |_, key| PANES.contains(&key))?;
    anyhow::ensure!(
        restored
            .session
            .view()
            .surfaces()
            .any(|surface| surface.id() == ROOT),
        "The editor root is missing"
    );
    for key in PANES {
        let item = restored
            .session
            .item_id_for_external_key(key)
            .context("A pane key is missing")?;
        anyhow::ensure!(
            restored.session.view().item(item).is_some(),
            "A saved pane was closed"
        );
    }
    Ok(Some(restored))
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
    workspace: open_gpui::WeakEntity<Workspace>,
    cx: &mut App,
) -> Result<()> {
    use std::{cell::Cell, rc::Rc};
    let sequence = Rc::new(Cell::new(0u64));
    dock.set_commit_handler(
        move |dock, _, cx| {
            let revision = sequence.get().wrapping_add(1);
            sequence.set(revision);
            let sequence = sequence.clone();
            let dock = dock.clone();
            let workspace = workspace.clone();
            let path = path.clone();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(220))
                    .await;
                if sequence.get() != revision {
                    return;
                }
                let _ = workspace.update(cx, |workspace, cx| {
                    if let Err(error) = save_layout(&dock, &path, cx) {
                        workspace.warn(format!("Layout save: {error:#}"), cx);
                    }
                });
            })
            .detach();
        },
        cx,
    )
}

pub(crate) fn save_layout(dock: &DockSurface, path: &Path, cx: &mut App) -> Result<()> {
    let keys = PANES.map(|key| dock.item_id_for_external_key(key));
    if !dock.with_view(|view| {
        keys.iter()
            .all(|key| key.is_some_and(|item| view.item(item).is_some()))
    }) {
        return Ok(());
    }
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
    let document = dock.capture_document(DockHostDocument {
        surfaces,
        migration: None,
    })?;
    fs::create_dir_all(
        path.parent()
            .context("The layout has no parent directory")?,
    )?;
    write_atomic(path, &document.to_json()?)?;
    Ok(())
}
