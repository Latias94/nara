use nara_core::{DiagnosticSeverity, EntityUuid, Vec2, vfs::AssetPath};
use nara_editor_core::{AssetKind, EditorCommand};
use nara_editor_viewport::ViewportStats;
use nara_scene::spatial::Visibility;
use open_gpui::{Context, Entity, IntoElement, Render, Window, div, prelude::*, px, rgb};
use open_gpui_base::{DropPlacement, Row, TreeRow, VirtualListEvent, VirtualTree};

use crate::{theme, workspace::Workspace};

pub struct HierarchyPanel {
    workspace: Entity<Workspace>,
    tree: Entity<VirtualTree<EntityUuid>>,
    revision: u64,
}

impl HierarchyPanel {
    pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        let tree = cx.new(|cx| {
            let mut tree = VirtualTree::tree(px(24.0), cx);
            tree.set_reorder_enabled(true, cx);
            tree.set_indent(px(16.0), cx);
            tree.set_row_renderer(
                |row, state, _, _| {
                    div()
                        .w_full()
                        .px_1()
                        .truncate()
                        .when(state.selected, |div| {
                            div.bg(rgb(theme::SELECTED))
                                .border_l_2()
                                .border_color(rgb(theme::ACCENT))
                        })
                        .child(row.label().clone())
                },
                cx,
            );
            tree
        });
        cx.subscribe(
            &tree,
            |panel, tree, event: &VirtualListEvent<EntityUuid>, cx| {
                if panel.workspace.read(cx).session.gesture_active() {
                    panel.sync(cx);
                    return;
                }
                let active = tree.read(cx).active_key().copied();
                panel.workspace.update(cx, |workspace, cx| match event {
                    VirtualListEvent::SelectionChanged(ids) => {
                        let active = active.filter(|entity| ids.contains(entity)).or_else(|| {
                            workspace
                                .session
                                .selection()
                                .active()
                                .filter(|entity| ids.contains(entity))
                        });
                        let ordered = ids.iter().copied().filter(|entity| Some(*entity) != active);
                        workspace.select(ordered.chain(active), cx);
                    }
                    VirtualListEvent::ReorderRequested(request) => {
                        let parent = if request.placement == DropPlacement::Into {
                            Some(request.target)
                        } else {
                            workspace
                                .session
                                .document()
                                .scene()
                                .entities
                                .iter()
                                .find(|row| row.id == request.target)
                                .and_then(|row| row.parent)
                        };
                        workspace.apply(
                            EditorCommand::Reparent {
                                entities: request.moved.clone(),
                                parent,
                            },
                            cx,
                        );
                    }
                    _ => {}
                });
            },
        )
        .detach();
        cx.observe(&workspace, |panel, _, cx| {
            panel.sync(cx);
            cx.notify();
        })
        .detach();
        let mut panel = Self {
            workspace,
            tree,
            revision: 0,
        };
        panel.sync(cx);
        panel
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        let selected = workspace.session.selection().entities().to_vec();
        let refresh = self.revision != workspace.revision;
        let first = self.revision == 0;
        let rows = refresh.then(|| {
            workspace
                .session
                .document()
                .scene()
                .entities
                .iter()
                .map(|row| TreeRow::new(row.id, row.parent, row.name.clone()))
                .collect::<Vec<_>>()
        });
        self.revision = workspace.revision;
        self.tree.update(cx, |tree, cx| {
            if let Some(rows) = rows {
                let roots: Vec<_> = rows
                    .iter()
                    .filter(|row| row.parent().is_none())
                    .map(|row| *row.key())
                    .collect();
                if tree.set_rows(rows, cx).is_ok() && first {
                    for id in roots {
                        tree.set_expanded(&id, true, cx);
                    }
                }
            }
            let tree_selection = tree.selection();
            if tree_selection.len() != selected.len()
                || !tree_selection
                    .iter()
                    .all(|entity| selected.contains(entity))
            {
                tree.set_selection(selected, cx);
            }
        });
    }
}

impl Render for HierarchyPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session = &self.workspace.read(cx).session;
        let locked = session.gesture_active();
        let has_selection = !session.selection().entities().is_empty();
        let can_edit_selection = has_selection && !locked;
        theme::panel()
            .child(
                theme::toolbar()
                    .child(
                        theme::button("create-entity", "Add")
                            .disabled(locked)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.workspace.update(cx, |workspace, cx| {
                                    workspace.create(Vec2::ZERO, None, None, cx)
                                });
                            })),
                    )
                    .child(
                        theme::button("unparent-entity", "Unparent")
                            .disabled(!can_edit_selection)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.workspace.update(cx, |workspace, cx| {
                                    workspace.apply(
                                        EditorCommand::Reparent {
                                            entities: workspace
                                                .session
                                                .selection()
                                                .entities()
                                                .to_vec(),
                                            parent: None,
                                        },
                                        cx,
                                    );
                                });
                            })),
                    )
                    .child(
                        theme::button("hide-entities", "Hide")
                            .disabled(!can_edit_selection)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.workspace.update(cx, |workspace, cx| {
                                    workspace.set_visibility(Visibility::Hidden, cx)
                                });
                            })),
                    )
                    .child(
                        theme::button("show-entities", "Show")
                            .disabled(!can_edit_selection)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.workspace.update(cx, |workspace, cx| {
                                    workspace.set_visibility(Visibility::Visible, cx)
                                });
                            })),
                    )
                    .child(
                        theme::button("delete-entities", "Delete")
                            .disabled(!can_edit_selection)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.workspace.update(cx, Workspace::delete_selection);
                            })),
                    ),
            )
            .child(div().min_h_0().flex_1().child(self.tree.clone()))
    }
}

#[derive(Clone)]
pub struct ImageDrag(pub AssetPath);

struct ImageDragLabel(String);

impl Render for ImageDragLabel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .bg(rgb(theme::RAISED))
            .text_color(rgb(theme::TEXT))
            .border_1()
            .border_color(rgb(theme::ACCENT))
            .child(self.0.clone())
    }
}

pub struct AssetPanel(Entity<Workspace>);

impl AssetPanel {
    pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self(workspace)
    }
}

impl Render for AssetPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.0.read(cx);
        let selected = workspace.selected_asset.clone();
        let entries = workspace.assets.entries().to_vec();
        let toolbar = theme::toolbar()
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .child(workspace.project.manifest().paths.assets.clone()),
            )
            .child(
                theme::button("rescan-assets", "Refresh").on_click(cx.listener(
                    |panel, _, _, cx| {
                        panel.0.update(cx, Workspace::refresh_assets);
                    },
                )),
            );
        theme::panel().child(toolbar).child(
            div()
                .id("asset-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(entries.into_iter().map(|entry| {
                    let path = entry.path;
                    let label = path.to_string();
                    let kind = entry.kind;
                    let active = selected.as_ref() == Some(&path);
                    let open = path.clone();
                    div()
                        .id(label.clone())
                        .when(kind == AssetKind::Image, |div| {
                            div.on_drag(ImageDrag(path), |drag, _, _, cx| {
                                cx.new(|_| ImageDragLabel(drag.0.to_string()))
                            })
                        })
                        .child(
                            Row::new(
                                "asset",
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().w(px(46.0)).text_color(rgb(theme::MUTED)).child(
                                        match kind {
                                            AssetKind::Scene => "Scene",
                                            AssetKind::Prefab => "Prefab",
                                            AssetKind::Image => "PNG",
                                            AssetKind::File => "File",
                                        },
                                    ))
                                    .child(div().truncate().child(label)),
                            )
                            .h(px(24.0))
                            .w_full()
                            .px_2()
                            .selected(active)
                            .when(active, |row| row.bg(rgb(theme::SELECTED)))
                            .on_click(cx.listener(
                                move |panel, _, _, cx| {
                                    panel.0.update(cx, |workspace, cx| {
                                        workspace.open_asset(&open, cx)
                                    });
                                },
                            )),
                        )
                })),
        )
    }
}

pub struct ConsolePanel {
    workspace: Entity<Workspace>,
    viewport_status: Entity<ViewportStatus>,
}

impl ConsolePanel {
    pub fn new(
        workspace: Entity<Workspace>,
        viewport_stats: Entity<ViewportStats>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        let viewport_status = cx.new(|cx| {
            cx.observe(&viewport_stats, |_, _, cx| cx.notify()).detach();
            ViewportStatus(viewport_stats)
        });
        Self {
            workspace,
            viewport_status,
        }
    }
}

impl Render for ConsolePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let lines = workspace
            .console
            .entries()
            .iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>();
        theme::panel()
            .child(theme::toolbar().child(self.viewport_status.clone()).child(
                theme::button("clear-console", "Clear").on_click(cx.listener(|panel, _, _, cx| {
                    panel.workspace.update(cx, |workspace, cx| {
                        workspace.console.clear();
                        cx.notify();
                    });
                })),
            ))
            .child(
                div()
                    .id("console-log")
                    .min_h_0()
                    .flex_1()
                    .overflow_y_scroll()
                    .children(lines.into_iter().map(|line| {
                        let color = match line.severity {
                            DiagnosticSeverity::Error => theme::ERROR,
                            DiagnosticSeverity::Warning => theme::WARNING,
                            DiagnosticSeverity::Info => theme::MUTED,
                        };
                        let message = if let Some(location) = line.location {
                            let position = location
                                .line_column
                                .map(|(line, column)| format!("{line}:{column}"))
                                .unwrap_or_default();
                            format!("{} {} {}", location.path.display(), position, line.message)
                        } else {
                            line.message
                        };
                        div()
                            .px_2()
                            .py_1()
                            .text_color(rgb(color))
                            .border_b_1()
                            .border_color(rgb(theme::BORDER))
                            .child(message)
                    })),
            )
    }
}

struct ViewportStatus(Entity<ViewportStats>);

impl Render for ViewportStatus {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stats = self.0.read(cx);
        div().flex_1().truncate().child(format!(
            "{} frames · {} retained · {} sprites · {} textures / {} buffers last frame",
            stats.rendered_frames,
            stats.retained_presentations,
            stats.extraction.extracted_sprites,
            stats.last_work.created_texture_calls,
            stats.last_work.created_buffer_calls,
        ))
    }
}
