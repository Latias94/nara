use std::time::Instant;

use nara_core::{Color, DocumentId, EntityUuid, Vec2};
use nara_editor_core::{
    EditorGesture, SelectionMode,
    actions::{ActionContext, EditorAction, EditorKeymap},
    play::PlayControl,
    viewport::{
        Gizmo2D, GizmoDrag2D, GizmoGeometry2D, GizmoHandle, GizmoSelection2D, GridSnap2D,
        PanelPoint, PanelSize, ToolMode, ViewportCamera2D, WorldPoint,
    },
};
use nara_editor_gizmo::{OverlayFrame, OverlayGrid, OverlayPrimitive, OverlayStroke};
use nara_editor_viewport::{EditorViewport, RenderOutcome, ViewportError, ViewportStats};
use nara_project::workflow::ProjectTrust;
use open_gpui::{
    Bounds, Context, Div, Entity, FocusHandle, HitboxTag, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, PointerCaptureHandle, Render,
    Subscription, TargetedEvent, Window, WindowId, canvas, div, prelude::*, px, rgb,
};

use crate::{
    panels::ImageDrag,
    theme,
    workspace::{JobStatus, Workspace},
};

pub struct ViewportPanel {
    workspace: Entity<Workspace>,
    preview: EditorViewport,
    stats: Entity<ViewportStats>,
    receiver: HitboxTag,
    focus: FocusHandle,
    focus_subscription: Option<Subscription>,
    capture: Option<(WindowId, PointerCaptureHandle)>,
    camera: Option<ViewportCamera2D>,
    displayed_camera: Option<ViewportCamera2D>,
    bounds: Option<Bounds<Pixels>>,
    scale: f32,
    camera_document: Option<DocumentId>,
    scene_revision: (u64, u64),
    scene_ready: bool,
    displayed_scene_revision: (u64, u64),
    presented_preview: Option<((u64, u64), Instant)>,
    selection: Vec<EntityUuid>,
    tool: ToolMode,
    keys: EditorKeymap,
    dirty: bool,
    cursor: Option<PanelPoint>,
    hover: Hover,
    drag: Option<Drag>,
    drop_preview: Option<WorldPoint>,
    #[cfg(feature = "native-smoke")]
    requested_frames: u32,
    #[cfg(feature = "native-smoke")]
    performance: Option<PerformanceFrames>,
}

#[cfg(feature = "native-smoke")]
#[derive(Clone, Copy, Default)]
pub(crate) struct PerformanceFrames {
    pub(crate) first_submission: Option<Instant>,
    pub(crate) last_submission: Option<Instant>,
    pub(crate) submissions: u64,
    pub(crate) status_updates: u64,
    pub(crate) settled: Option<PerformanceBoundary>,
    publish_status: bool,
}

#[cfg(feature = "native-smoke")]
#[derive(Clone, Copy)]
pub(crate) struct PerformanceBoundary {
    pub(crate) at: Instant,
    pub(crate) gpui_submissions: u64,
}

#[cfg(feature = "native-smoke")]
impl PerformanceBoundary {
    pub(crate) fn from_window(window: &Window) -> Self {
        Self {
            at: Instant::now(),
            gpui_submissions: window.present_submission_count(),
        }
    }
}

enum Drag {
    Pan(PanelPoint),
    Gizmo {
        gesture: GizmoDrag2D,
        owner: EditorGesture,
        selection: GizmoSelection2D,
    },
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Hover {
    entity: Option<EntityUuid>,
    gizmo: Option<GizmoHandle>,
}

impl ViewportPanel {
    pub fn new(
        workspace: Entity<Workspace>,
        preview: EditorViewport,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            workspace,
            preview,
            stats: cx.new(|_| ViewportStats::default()),
            receiver: HitboxTag::new(),
            focus: cx.focus_handle(),
            focus_subscription: None,
            capture: None,
            camera: None,
            displayed_camera: None,
            bounds: None,
            scale: 0.0,
            camera_document: None,
            scene_revision: (0, 0),
            scene_ready: false,
            displayed_scene_revision: (0, 0),
            presented_preview: None,
            selection: Vec::new(),
            tool: ToolMode::Select,
            keys: EditorKeymap::default(),
            dirty: true,
            cursor: None,
            hover: Hover::default(),
            drag: None,
            drop_preview: None,
            #[cfg(feature = "native-smoke")]
            requested_frames: 0,
            #[cfg(feature = "native-smoke")]
            performance: None,
        }
    }

    pub fn receiver(&self) -> HitboxTag {
        self.receiver
    }

    #[cfg(feature = "native-smoke")]
    pub(crate) fn request_frames(&mut self, frames: u32, cx: &mut Context<Self>) {
        self.performance = None;
        self.requested_frames = frames;
        self.dirty = true;
        cx.notify();
    }

    #[cfg(feature = "native-smoke")]
    pub(crate) fn stats(&self) -> ViewportStats {
        self.preview.stats()
    }

    pub(crate) fn stats_entity(&self) -> &Entity<ViewportStats> {
        &self.stats
    }

    #[cfg(feature = "native-smoke")]
    pub(crate) fn presented_preview(&self) -> Option<((u64, u64), Instant)> {
        self.presented_preview
    }

    #[cfg(feature = "native-smoke")]
    pub(crate) fn request_performance_frames(
        &mut self,
        count: u32,
        publish_status: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        use anyhow::Context as _;

        let bounds = self.bounds.context("The viewport has no painted bounds")?;
        anyhow::ensure!(self.requested_frames == 0, "A viewport capture is active");
        self.requested_frames = count;
        self.performance = Some(PerformanceFrames {
            publish_status,
            ..Default::default()
        });
        cx.on_next_frame(window, move |view, window, cx| {
            view.frame(bounds, window, cx)
        });
        Ok(())
    }

    #[cfg(feature = "native-smoke")]
    pub(crate) fn completed_performance_frames(&self) -> Option<PerformanceFrames> {
        self.performance
            .filter(|measurement| measurement.settled.is_some())
    }

    fn frame(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(size) = PanelSize::new(
            bounds.size.width.to_f64() as f32,
            bounds.size.height.to_f64() as f32,
        ) else {
            return;
        };
        let workspace = self.workspace.read(cx);
        let revision = workspace.preview_revision();
        if self.scene_revision != revision {
            let scene = workspace.session.preview_scene();
            let result = self
                .preview
                .set_prepared_scene(scene, workspace.preview_assets());
            self.scene_revision = revision;
            self.scene_ready = result.is_ok();
            if let Err(error) = result {
                self.dirty = false;
                if matches!(error, ViewportError::UnpreparedAsset(_)) {
                    workspace.request_preview_assets();
                } else {
                    self.report(format!("{error:#}"), cx);
                }
                return;
            }
            if self.camera_document != Some(scene.id) {
                self.camera = None;
                self.camera_document = Some(scene.id);
            }
        }
        if !self.scene_ready {
            return;
        }
        self.selection = workspace.session.selection().entities().to_vec();
        self.bounds = Some(bounds);
        self.scale = window.scale_factor();
        self.dirty = false;
        let camera = if let Some(camera) = &mut self.camera {
            camera.resize(size);
            *camera
        } else {
            let camera = self.fit(size, false);
            self.camera = Some(camera);
            camera
        };
        let gizmo = self.active_gizmo();
        let hover = self.hover_at(&camera, self.cursor, gizmo);
        let overlay = self.overlay(&camera, gizmo, hover);
        let submissions = window.present_submission_count();
        match self.preview.render(window, &camera, &overlay) {
            Ok(RenderOutcome::TargetsBusy) => {
                cx.on_next_frame(window, move |view, window, cx| {
                    view.frame(bounds, window, cx)
                });
            }
            Ok(outcome) => {
                self.displayed_camera = Some(camera);
                self.displayed_scene_revision = revision;
                self.hover = hover;
                if outcome == RenderOutcome::Imported {
                    cx.notify();
                }
                #[cfg(feature = "native-smoke")]
                if self.requested_frames != 0 {
                    let performance = self.performance.as_mut().map(|sample| {
                        let completed = Instant::now();
                        sample.first_submission.get_or_insert(completed);
                        sample.last_submission = Some(completed);
                        sample.submissions += 1;
                        sample.status_updates += u64::from(sample.publish_status);
                        sample.publish_status
                    });
                    if performance == Some(true) {
                        self.publish_stats(cx);
                    }
                    self.requested_frames -= 1;
                    if self.requested_frames > 0 {
                        cx.on_next_frame(window, move |view, window, cx| {
                            view.frame(bounds, window, cx)
                        });
                        return;
                    }
                    if performance.is_some() {
                        cx.on_next_frame(window, |view, window, cx| {
                            view.preview.observe_presentation(window);
                            cx.on_next_frame(window, |view, window, _| {
                                if let Some(sample) = &mut view.performance {
                                    sample.settled = Some(PerformanceBoundary::from_window(window));
                                }
                            });
                        });
                        return;
                    }
                }
                cx.on_next_frame(window, move |view, window, cx| {
                    view.observe_presented_preview(revision, submissions, 4, window, cx);
                });
            }
            Err(error) => self.report(error.to_string(), cx),
        }
    }

    fn observe_presented_preview(
        &mut self,
        revision: (u64, u64),
        submissions: u64,
        retries: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview.observe_presentation(window);
        if self.displayed_scene_revision == revision
            && self
                .presented_preview
                .is_none_or(|(previous, _)| previous != revision)
        {
            if window.present_submission_count() > submissions {
                self.presented_preview = Some((revision, Instant::now()));
            } else if retries != 0 {
                cx.on_next_frame(window, move |view, window, cx| {
                    view.observe_presented_preview(revision, submissions, retries - 1, window, cx);
                });
            }
        }
        self.publish_stats(cx);
    }

    fn publish_stats(&self, cx: &mut Context<Self>) {
        let stats = self.preview.stats();
        self.stats.update(cx, |previous, cx| {
            if *previous != stats {
                *previous = stats;
                cx.notify();
            }
        });
    }

    fn fit(&self, size: PanelSize, selection_only: bool) -> ViewportCamera2D {
        let selection = if selection_only {
            &self.selection[..]
        } else {
            &[]
        };
        let bounds = self.preview.scene_bounds(selection).or_else(|| {
            if selection.is_empty() {
                None
            } else {
                self.preview.scene_bounds(&[])
            }
        });
        let (center, extent) = bounds.map_or(
            (Vec2::ZERO, Vec2::new(740.0, 400.0).as_dvec2()),
            |[min, max]| {
                let min = min.as_dvec2();
                let max = max.as_dvec2();
                (((min + max) * 0.5).as_vec2(), (max - min) + 64.0)
            },
        );
        let zoom = (size.as_vec2().as_dvec2() / extent).min_element() as f32;
        ViewportCamera2D::new(
            size,
            WorldPoint(center),
            zoom.max(ViewportCamera2D::MIN_ZOOM),
        )
        .expect("finite scene bounds and panel size")
    }

    fn report(&self, message: String, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.warn(message, cx));
    }

    fn local(&self, position: Point<Pixels>) -> Option<PanelPoint> {
        let offset = position - self.bounds?.origin;
        Some(PanelPoint(Vec2::new(
            offset.x.to_f64() as f32,
            offset.y.to_f64() as f32,
        )))
    }

    fn active_gizmo(&self) -> Option<Gizmo2D> {
        let affine = self.preview.world_affine(*self.selection.last()?)?;
        let axis = affine.matrix2.x_axis;
        Gizmo2D::new(WorldPoint(affine.translation), axis.y.atan2(axis.x))
    }

    fn hover_at(
        &self,
        camera: &ViewportCamera2D,
        cursor: Option<PanelPoint>,
        gizmo: Option<Gizmo2D>,
    ) -> Hover {
        let Some(cursor) = cursor.filter(|&cursor| camera.contains(cursor)) else {
            return Hover::default();
        };
        Hover {
            entity: camera
                .pick(cursor, self.preview.pick_candidates())
                .filter(|entity| !self.selection.contains(entity)),
            gizmo: gizmo.and_then(|gizmo| gizmo.hit_test(camera, self.tool, cursor)),
        }
    }

    fn overlay(
        &self,
        camera: &ViewportCamera2D,
        gizmo: Option<Gizmo2D>,
        hover: Hover,
    ) -> OverlayFrame {
        let width = self.scale.min(3.0).max(1.0);
        let mut overlay = OverlayFrame {
            grid: Some(OverlayGrid::default()),
            ..Default::default()
        };
        self.preview.append_colliders(
            &mut overlay,
            OverlayStroke::new(Color::rgba(0.22, 0.63, 0.49, 0.35), width),
        );
        if let Some(hovered) = hover.entity
            && let Some(quad) = self.preview.selection_quad(hovered)
        {
            overlay.primitives.push(OverlayPrimitive::Rect {
                corners: quad.world_corners().map(|corner| corner.0),
                stroke: OverlayStroke::new(Color::rgba(0.45, 0.66, 0.71, 0.6), width),
            });
        }
        for &selected in &self.selection {
            if let Some(quad) = self.preview.selection_quad(selected) {
                overlay.primitives.push(OverlayPrimitive::Rect {
                    corners: quad.world_corners().map(|corner| corner.0),
                    stroke: OverlayStroke::new(Color::rgba(0.12, 0.62, 0.54, 1.0), width * 2.0),
                });
            }
        }
        if let Some(geometry) = gizmo.and_then(|gizmo| gizmo.geometry(camera, self.tool)) {
            add_gizmo(&mut overlay.primitives, geometry, hover.gizmo, width);
        }
        if let Some(center) = self.drop_preview {
            let extent = Vec2::splat(16.0);
            overlay.primitives.push(OverlayPrimitive::rectangle(
                center.0 - extent,
                center.0 + extent,
                OverlayStroke::new(Color::rgba(0.12, 0.62, 0.54, 0.9), width * 2.0),
            ));
        }
        overlay
    }

    fn pressed(
        &mut self,
        targeted: &TargetedEvent<MouseDownEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_some() {
            return;
        }
        let Ok(position) = targeted.target_local_position() else {
            return;
        };
        let MouseDownEvent {
            button, modifiers, ..
        } = *targeted.window_event();
        let cursor = panel_point(position);
        let Some(camera) = self.displayed_camera else {
            return;
        };
        let Some((_, capture)) = self.capture else {
            return;
        };
        window.focus(&self.focus, cx);
        if button == MouseButton::Middle || (button == MouseButton::Left && modifiers.alt) {
            if window.capture_pointer(&capture, button).is_ok() {
                self.drag = Some(Drag::Pan(cursor));
                cx.stop_propagation();
            }
            return;
        }
        let workspace = self.workspace.read(cx);
        if workspace.session.gesture_active()
            || self.displayed_scene_revision != workspace.preview_revision()
            || self.selection != workspace.session.selection().entities()
        {
            return;
        }
        if let Some(gizmo) = self.active_gizmo()
            && let Some(gesture) = gizmo.begin_drag(&camera, self.tool, cursor)
        {
            let selection = match GizmoSelection2D::capture(&workspace.session, gizmo, |entity| {
                self.preview.parent_affine(entity)
            }) {
                Ok(Some(selection)) => selection,
                Ok(None) => {
                    self.report("The selection has an invalid parent transform".into(), cx);
                    return;
                }
                Err(error) => {
                    self.report(error.to_string(), cx);
                    return;
                }
            };
            if window.capture_pointer(&capture, button).is_err() {
                return;
            }
            if let Some(owner) = self.workspace.update(cx, Workspace::begin_gesture) {
                self.drag = Some(Drag::Gizmo {
                    gesture,
                    owner,
                    selection,
                });
                cx.stop_propagation();
            } else {
                let _ = window.release_pointer(&capture);
            }
            return;
        }
        if self.tool == ToolMode::TileBrush
            && let Some(point) = camera.panel_to_world(cursor)
            && let Some(point) = GridSnap2D::new(32.0).and_then(|snap| snap.snap_point(point))
        {
            self.workspace.update(cx, |workspace, cx| {
                if let Some(path) = workspace.selected_asset.clone().filter(|path| {
                    nara_editor_core::AssetKind::of(path) == nara_editor_core::AssetKind::Image
                }) {
                    workspace.create(point.0, Some(path), None, cx);
                } else {
                    workspace.warn("Select a PNG in Assets before painting", cx);
                }
            });
            return;
        }
        let entity = camera.pick(cursor, self.preview.pick_candidates());
        self.workspace.update(cx, |workspace, cx| {
            if let Some(entity) = entity {
                let mode = if modifiers.secondary(cx) || modifiers.shift {
                    SelectionMode::Toggle
                } else {
                    SelectionMode::Replace
                };
                workspace.session.selection_mut().select(entity, mode, &[]);
            } else if !modifiers.shift && !modifiers.secondary(cx) {
                workspace.session.selection_mut().clear();
            }
            cx.notify();
        });
    }

    fn moved(&mut self, point: PanelPoint, shift: bool, cx: &mut Context<Self>) {
        self.cursor = Some(point);
        let mut changed = false;
        if let Some(drag) = &mut self.drag {
            match drag {
                Drag::Pan(last) => {
                    if let Some(camera) = &mut self.camera
                        && point != *last
                    {
                        changed = camera.pan(point.0 - last.0).is_some();
                    }
                    *last = point;
                }
                Drag::Gizmo {
                    gesture,
                    owner,
                    selection,
                } => {
                    let delta = if shift {
                        gesture.update_snapped(point, GridSnap2D::new(32.0).unwrap())
                    } else {
                        gesture.update(point)
                    };
                    if let Some(delta) = delta {
                        changed = self.workspace.update(cx, |workspace, cx| {
                            match selection.command(delta) {
                                Ok(command) => workspace.apply_gesture(owner, command, cx),
                                Err(error) => {
                                    workspace.warn(error.to_string(), cx);
                                    false
                                }
                            }
                        });
                    }
                }
            }
        }
        let hover = self.displayed_camera.map_or(Hover::default(), |camera| {
            self.hover_at(&camera, self.cursor, self.active_gizmo())
        });
        if changed || hover != self.hover {
            self.hover = hover;
            self.dirty = true;
            cx.notify();
        }
    }

    fn finish_drag(&mut self, cancel: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(drag) = self.drag.take() {
            if let Some((_, capture)) = self.capture {
                let _ = window.release_pointer(&capture);
            }
            if let Drag::Gizmo { owner, .. } = drag {
                self.workspace.update(cx, |workspace, cx| {
                    if cancel {
                        workspace.cancel_gesture(&owner, cx);
                    } else {
                        workspace.end_gesture(&owner, cx);
                    }
                });
            }
            self.dirty = true;
            cx.notify();
        }
    }

    fn keyboard(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let Some(action) = self.keys.resolve(
            ActionContext::Viewport,
            &key.key,
            key.modifiers.secondary(cx),
            key.modifiers.shift,
        ) else {
            return;
        };
        cx.stop_propagation();
        match action {
            EditorAction::Cancel => self.finish_drag(true, window, cx),
            EditorAction::Tool(mode) => {
                self.finish_drag(true, window, cx);
                self.tool = mode;
            }
            EditorAction::FrameSelection => {
                if self.drag.is_none()
                    && let Some(camera) = self.camera
                {
                    self.camera = Some(self.fit(camera.panel_size(), true));
                }
            }
            EditorAction::Save => self.workspace.update(cx, Workspace::save),
            EditorAction::Undo => self.workspace.update(cx, Workspace::undo),
            EditorAction::Redo => self.workspace.update(cx, Workspace::redo),
            EditorAction::Delete => self.workspace.update(cx, Workspace::delete_selection),
        }
        self.dirty = true;
        cx.notify();
    }

    fn toolbar(&self, cx: &Context<Self>) -> Div {
        let workspace = self.workspace.read(cx);
        let status = workspace.status;
        let pending = workspace.control_pending();
        let editing = workspace.session.gesture_active();
        let trust = workspace.trust();
        let header = workspace
            .session
            .document()
            .path()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled Scene");
        let title = if workspace.session.is_dirty() {
            format!("{header} •")
        } else {
            header.to_owned()
        };
        let mode_label = match self.tool {
            ToolMode::Select => "Select",
            ToolMode::Move => "Move",
            ToolMode::Rotate => "Rotate",
            ToolMode::Scale => "Scale",
            ToolMode::TileBrush => "Tile Brush",
        };
        let modes = [
            ("select", "Q", ToolMode::Select),
            ("move", "W", ToolMode::Move),
            ("rotate", "E", ToolMode::Rotate),
            ("scale", "R", ToolMode::Scale),
            ("brush", "B", ToolMode::TileBrush),
        ];
        let doc = theme::toolbar()
            .child(div().flex_1().min_w_0().truncate().child(title))
            .child(
                theme::button("save", "Save").disabled(editing).on_click(
                    cx.listener(|view, _, _, cx| view.workspace.update(cx, Workspace::save)),
                ),
            )
            .child(
                theme::button("undo", "Undo")
                    .disabled(!workspace.session.can_undo())
                    .on_click(
                        cx.listener(|view, _, _, cx| view.workspace.update(cx, Workspace::undo)),
                    ),
            )
            .child(
                theme::button("redo", "Redo")
                    .disabled(!workspace.session.can_redo())
                    .on_click(
                        cx.listener(|view, _, _, cx| view.workspace.update(cx, Workspace::redo)),
                    ),
            )
            .when(trust == ProjectTrust::Untrusted, |div| {
                div.child(
                    theme::button("trust", "Trust Project").on_click(cx.listener(
                        |view, _, _, cx| view.workspace.update(cx, Workspace::trust_project),
                    )),
                )
            })
            .child(
                theme::button("schema", "Refresh Schema")
                    .disabled(status != JobStatus::Idle || trust != ProjectTrust::Trusted)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.workspace.update(cx, Workspace::refresh_schema)
                    })),
            );
        let transport = theme::toolbar()
            .children(modes.map(|(id, label, mode)| {
                theme::button(id, label)
                    .bg(rgb(if mode == self.tool {
                        theme::SELECTED
                    } else {
                        theme::HOVER
                    }))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.finish_drag(true, window, cx);
                        view.tool = mode;
                        view.dirty = true;
                        cx.notify();
                    }))
            }))
            .child(div().text_color(rgb(theme::MUTED)).child(mode_label))
            .child(div().flex_1().min_w_0())
            .when(status != JobStatus::Idle, |div| {
                div.child(div_label(if workspace.steps == 0 {
                    status.label().to_owned()
                } else {
                    format!("{} · step {}", status.label(), workspace.steps)
                }))
            })
            .child(
                theme::button("play", "Play")
                    .disabled(
                        editing || status != JobStatus::Idle || trust != ProjectTrust::Trusted,
                    )
                    .on_click(
                        cx.listener(|view, _, _, cx| view.workspace.update(cx, Workspace::play)),
                    ),
            )
            .child(
                theme::button(
                    "pause",
                    if status == JobStatus::Paused {
                        "Resume"
                    } else {
                        "Pause"
                    },
                )
                .disabled(pending || !matches!(status, JobStatus::Running | JobStatus::Paused))
                .on_click(cx.listener(|view, _, _, cx| {
                    view.workspace.update(cx, |workspace, cx| {
                        workspace.control(
                            if workspace.status == JobStatus::Paused {
                                PlayControl::Resume
                            } else {
                                PlayControl::Pause
                            },
                            cx,
                        )
                    });
                })),
            )
            .child(
                theme::button("step", "Step")
                    .disabled(pending || status != JobStatus::Paused)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.workspace
                            .update(cx, |workspace, cx| workspace.control(PlayControl::Step, cx))
                    })),
            )
            .child(
                theme::button("stop", "Stop")
                    .disabled(matches!(status, JobStatus::Idle | JobStatus::Stopping))
                    .on_click(
                        cx.listener(|view, _, _, cx| view.workspace.update(cx, Workspace::stop)),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(doc)
            .child(transport)
    }
}

impl Render for ViewportPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_id = window.window_handle().window_id();
        if self.capture.is_none_or(|(owner, _)| owner != window_id) {
            self.finish_drag(true, window, cx);
            self.capture = Some((window_id, window.new_pointer_capture_handle()));
            self.focus_subscription = Some(cx.on_blur(&self.focus, window, |view, window, cx| {
                view.finish_drag(true, window, cx);
            }));
            self.dirty = true;
        }
        let (_, capture) = self.capture.unwrap();
        let workspace = self.workspace.read(cx);
        let render = self.dirty
            || workspace.preview_revision() != self.scene_revision
            || workspace.session.selection().entities() != self.selection;
        let previous = self.bounds;
        let scale = self.scale;
        let weak = cx.weak_entity();
        let cancel = weak.clone();
        let drawing = self.preview.texture(window).cloned();
        let painted = canvas(
            move |bounds, window, _| {
                if render || previous != Some(bounds) || scale != window.scale_factor() {
                    let render = weak.clone();
                    window.on_next_frame(move |window, cx| {
                        let _ = render.update(cx, |view, cx| view.frame(bounds, window, cx));
                    });
                }
            },
            move |bounds, (), window, _| {
                window.on_pointer_cancel(move |_, phase, window, cx| {
                    if phase == open_gpui::DispatchPhase::Bubble {
                        let _ = cancel.update(cx, |view, cx| view.finish_drag(true, window, cx));
                    }
                });
                if let Some(texture) = &drawing {
                    let _ = window.paint_image_texture(bounds, Default::default(), texture);
                }
            },
        )
        .absolute()
        .size_full();

        let surface = div()
            .id("scene-view-surface")
            .relative()
            .min_h_0()
            .min_w_0()
            .flex_1()
            .overflow_hidden()
            .bg(rgb(theme::CANVAS))
            .track_focus(&self.focus)
            .track_pointer_capture(&capture)
            .on_key_down(cx.listener(Self::keyboard))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pressed))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::pressed))
            .on_mouse_move(
                cx.listener(|view, event: &TargetedEvent<MouseMoveEvent>, _, cx| {
                    if let Ok(position) = event.target_local_position() {
                        view.moved(
                            panel_point(position),
                            event.window_event().modifiers.shift,
                            cx,
                        );
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, event: &TargetedEvent<MouseUpEvent>, window, cx| {
                    if view.drag.is_some()
                        && let Ok(position) = event.target_local_position()
                    {
                        view.moved(
                            panel_point(position),
                            event.window_event().modifiers.shift,
                            cx,
                        );
                    }
                    view.finish_drag(false, window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|view, _, window, cx| view.finish_drag(false, window, cx)),
            )
            .on_raw_scroll_wheel(cx.listener(|view, event: &open_gpui::ScrollEvent, _, cx| {
                if view.drag.is_some() {
                    return;
                }
                let Some(point) = view.local(event.position) else {
                    return;
                };
                let zoom = (event.delta.pixel_delta(px(20.0)).y.to_f64() as f32 * 0.006)
                    .clamp(-1.0, 1.0)
                    .exp();
                if let Some(camera) = &mut view.camera {
                    camera.zoom_at(point, zoom);
                }
                view.dirty = true;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                if !*hovered && view.drag.is_none() {
                    view.cursor = None;
                    let had_preview = view.drop_preview.take().is_some();
                    if view.hover != Hover::default() || had_preview {
                        view.hover = Hover::default();
                        view.dirty = true;
                        cx.notify();
                    }
                }
            }))
            .on_drag_move(cx.listener(
                |view, event: &open_gpui::DragMoveEvent<ImageDrag>, _, cx| {
                    view.drop_preview = event.target_local_position().ok().and_then(|point| {
                        view.displayed_camera?.panel_to_world(panel_point(point))
                    });
                    view.dirty = true;
                    cx.notify();
                },
            ))
            .on_drop(
                cx.listener(|view, drop: &open_gpui::DropEvent<ImageDrag>, _, cx| {
                    let revision = view.workspace.read(cx).preview_revision();
                    let point = drop
                        .pointer()
                        .target_local_position()
                        .ok()
                        .and_then(|point| {
                            view.displayed_camera?.panel_to_world(panel_point(point))
                        });
                    if let Some(point) = point
                        && view.displayed_scene_revision == revision
                    {
                        let id = drop.value().id;
                        view.workspace
                            .update(cx, |workspace, cx| workspace.create_image(point.0, id, cx));
                    }
                    view.drop_preview = None;
                    view.dirty = true;
                    cx.notify();
                }),
            )
            .child(painted);
        theme::panel()
            .hitbox_tag(self.receiver)
            .child(self.toolbar(cx))
            .child(surface)
    }
}

fn div_label(text: String) -> Div {
    div()
        .text_size(px(12.0))
        .text_color(rgb(theme::MUTED))
        .truncate()
        .child(text)
}

fn panel_point(position: Point<Pixels>) -> PanelPoint {
    PanelPoint(Vec2::new(
        position.x.to_f64() as f32,
        position.y.to_f64() as f32,
    ))
}

fn add_gizmo(
    primitives: &mut Vec<OverlayPrimitive>,
    geometry: GizmoGeometry2D,
    hover: Option<GizmoHandle>,
    width: f32,
) {
    let ink = |handle, color| {
        OverlayStroke::new(color, width * if hover == Some(handle) { 3.0 } else { 2.0 })
    };
    match geometry {
        GizmoGeometry2D::Rotate(ring) => primitives.push(OverlayPrimitive::Circle {
            center: ring.pivot.0,
            radius: ring.radius,
            stroke: ink(GizmoHandle::Rotation, Color::rgba(0.19, 0.65, 0.57, 1.0)),
        }),
        GizmoGeometry2D::Move(axes) | GizmoGeometry2D::Scale(axes) => {
            let axis = [
                (
                    axes.x_end.0,
                    ink(GizmoHandle::X, Color::rgba(0.82, 0.24, 0.28, 1.0)),
                ),
                (
                    axes.y_end.0,
                    ink(GizmoHandle::Y, Color::rgba(0.31, 0.70, 0.29, 1.0)),
                ),
            ];
            for (end, stroke) in axis {
                primitives.push(OverlayPrimitive::Line {
                    from: axes.pivot.0,
                    to: end,
                    stroke,
                });
                match geometry {
                    GizmoGeometry2D::Move(_) => primitives.push(OverlayPrimitive::Circle {
                        center: end,
                        radius: axes.handle_radius,
                        stroke,
                    }),
                    _ => primitives.push(OverlayPrimitive::rectangle(
                        end - Vec2::splat(axes.handle_radius),
                        end + Vec2::splat(axes.handle_radius),
                        stroke,
                    )),
                }
            }
            primitives.push(OverlayPrimitive::Circle {
                center: axes.pivot.0,
                radius: axes.center_radius,
                stroke: ink(GizmoHandle::Free, Color::rgba(0.79, 0.87, 0.91, 0.95)),
            });
        }
    }
}
