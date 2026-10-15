use egui::{
    Align2, Color32, FontId, PointerButton, Pos2, Rect, RichText, Sense, Stroke, TextureId, Ui,
    Vec2,
};
use nara_scene::SceneEntityId;
use nara_tooling::{
    EditorAction, EditorGestureAction, EditorGestureCancelReason, EditorGestureView,
};
use nara_ui::__private::{
    UiAuthoringCanvasProjection, UiAuthoringDirectEdit, UiDirectManipulationCapability,
};

use crate::scene_view::{display_mapping, egui_rect};

use super::{
    EguiUiAuthoringInput, EguiUiAuthoringScope, EguiUiCanvasContent, EguiUiCanvasInput,
    EguiUiCanvasOutput,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiUiCanvasDragKind {
    Move,
    Resize,
}

#[derive(Debug, Clone, PartialEq)]
struct EguiUiCanvasDrag {
    entity: SceneEntityId,
    kind: EguiUiCanvasDragKind,
    transform: [f32; 7],
    latest_delta: [f64; 2],
    applied_delta: [f64; 2],
    last_sent_stamp: Option<nara_tooling::EditorGestureStamp>,
    commit_requested: bool,
}

#[derive(Debug, Clone)]
struct EguiUiCanvasSelection {
    entity: SceneEntityId,
    rect: Rect,
    resize_handle: Option<Rect>,
    move_enabled: bool,
    authored_units_per_point: [f64; 2],
    transform: [f32; 7],
}

#[derive(Debug, Default)]
pub(crate) struct EguiUiCanvasPanel {
    scope: Option<EguiUiAuthoringScope>,
    drag: Option<EguiUiCanvasDrag>,
}

impl EguiUiCanvasPanel {
    pub(crate) fn synchronize(&mut self, input: Option<&EguiUiAuthoringInput<'_>>) {
        let next = input.map(|input| EguiUiAuthoringScope {
            document: input.document(),
            revision: input.revision(),
            image_choice_generation: input.image_choice_generation(),
        });
        if self.scope != next {
            self.scope = next;
            self.drag = None;
        }
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        input: EguiUiCanvasInput,
        authoring: &mut EguiUiAuthoringInput<'_>,
        panel_size: Vec2,
        actions: &mut Vec<EditorAction>,
    ) -> EguiUiCanvasOutput {
        let panel_top = ui.cursor().top();
        ui.horizontal(|ui| {
            ui.heading("UI Canvas");
            ui.label(RichText::new(canvas_state_label(input.content)).weak());
        });
        let header_height = (ui.cursor().top() - panel_top).max(0.0);
        let desired_size =
            Vec2::new(panel_size.x, panel_size.y - header_height).max(Vec2::splat(1.0));
        let (outer_rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        ui.painter()
            .rect_filled(outer_rect, 2.0, Color32::from_rgb(18, 20, 25));
        let mapping = canvas_texture_facts(input.content).and_then(|(_, extent)| {
            display_mapping(
                outer_rect,
                extent,
                ui.ctx().pixels_per_point(),
                nara_render::DisplayScale::Fit,
            )
        });
        let image_rect = mapping.map_or(outer_rect, |mapping| egui_rect(mapping.content_rect()));
        if let Some((texture_id, _)) = canvas_texture_facts(input.content) {
            ui.painter().image(
                texture_id,
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let selection = self.paint_selection(
            ui,
            authoring,
            image_rect,
            input.is_presented() && authoring.direct_editing_enabled(),
        );
        self.handle_gesture(ui, input, authoring, selection.as_ref(), actions);

        let pick_uv = (input.is_presented() && response.clicked_by(PointerButton::Primary))
            .then(|| response.interact_pointer_pos())
            .flatten()
            .filter(|point| image_rect.contains(*point))
            .and_then(|point| mapping?.panel_to_uv([point.x, point.y]));

        if !input.is_presented() || input.notice.is_some() {
            ui.painter().text(
                outer_rect.center(),
                Align2::CENTER_CENTER,
                input
                    .notice
                    .unwrap_or_else(|| canvas_state_label(input.content)),
                FontId::proportional(13.0),
                Color32::from_gray(210),
            );
        }
        EguiUiCanvasOutput {
            desired_size_points: outer_rect.size(),
            pick_uv,
        }
    }

    fn paint_selection(
        &self,
        ui: &Ui,
        authoring: &EguiUiAuthoringInput<'_>,
        image_rect: Rect,
        interactive: bool,
    ) -> Option<EguiUiCanvasSelection> {
        let (Ok(projection), Some(selected)) =
            (authoring.projection(), authoring.selected_entity())
        else {
            return None;
        };
        let UiAuthoringCanvasProjection::Ready(canvas) = projection.canvas() else {
            return None;
        };
        let fact = canvas
            .facts()
            .iter()
            .find(|fact| fact.entity() == selected && fact.visible())?;
        let node = projection
            .hierarchy()
            .iter()
            .find(|node| node.entity() == selected)?;
        let transform = canvas.content_transform();
        let physical = transform.physical_surface_size();
        let content_origin = transform.physical_content_origin();
        let scale = transform.content_scale_factor();
        let rect = fact.rect();
        let min = rect.origin();
        let max = rect.max();
        let uv_min = Vec2::new(
            (content_origin.x() + min.x() * scale) / physical.width(),
            (content_origin.y() + min.y() * scale) / physical.height(),
        );
        let uv_max = Vec2::new(
            (content_origin.x() + max.x() * scale) / physical.width(),
            (content_origin.y() + max.y() * scale) / physical.height(),
        );
        let selected_rect = Rect::from_min_max(
            image_rect.min + uv_min * image_rect.size(),
            image_rect.min + uv_max * image_rect.size(),
        );
        let color = if interactive {
            Color32::from_rgb(75, 160, 255)
        } else {
            Color32::from_gray(130)
        };
        ui.painter().rect_stroke(
            selected_rect,
            0.0,
            Stroke::new(1.5, color),
            egui::StrokeKind::Inside,
        );
        let resize_handle = (node.gizmos().resize_node()
            == UiDirectManipulationCapability::Editable)
            .then(|| Rect::from_center_size(selected_rect.max, Vec2::splat(9.0)));
        if let Some(handle) = resize_handle {
            ui.painter().rect_filled(handle, 1.0, color);
        }
        Some(EguiUiCanvasSelection {
            entity: selected.clone(),
            rect: selected_rect,
            resize_handle,
            move_enabled: node.gizmos().move_node() == UiDirectManipulationCapability::Editable,
            authored_units_per_point: [
                f64::from(physical.width() / image_rect.width() / scale),
                f64::from(physical.height() / image_rect.height() / scale),
            ],
            transform: [
                physical.width(),
                physical.height(),
                content_origin.x(),
                content_origin.y(),
                scale,
                image_rect.width(),
                image_rect.height(),
            ],
        })
    }

    fn handle_gesture(
        &mut self,
        ui: &mut Ui,
        input: EguiUiCanvasInput,
        authoring: &mut EguiUiAuthoringInput<'_>,
        selection: Option<&EguiUiCanvasSelection>,
        actions: &mut Vec<EditorAction>,
    ) {
        if !authoring.direct_editing_enabled() {
            self.drag = None;
            return;
        }
        let move_response = selection
            .filter(|selection| selection.move_enabled)
            .map(|selection| {
                ui.interact(
                    selection.rect,
                    ui.id().with(("ui-canvas-move", selection.entity.as_str())),
                    Sense::drag(),
                )
            });
        let resize_response = selection
            .and_then(|selection| selection.resize_handle.map(|handle| (selection, handle)))
            .map(|(selection, handle)| {
                ui.interact(
                    handle,
                    ui.id()
                        .with(("ui-canvas-resize", selection.entity.as_str())),
                    Sense::drag(),
                )
            });

        if self.drag.is_none() && input.is_presented() && authoring.gesture().is_none() {
            let started = resize_response
                .as_ref()
                .filter(|response| response.drag_started())
                .and_then(|_| selection.map(|selection| (selection, EguiUiCanvasDragKind::Resize)))
                .or_else(|| {
                    move_response
                        .as_ref()
                        .filter(|response| response.drag_started())
                        .and_then(|_| {
                            selection.map(|selection| (selection, EguiUiCanvasDragKind::Move))
                        })
                });
            if let Some((selection, kind)) = started {
                let baseline = canvas_direct_edit(kind, &selection.entity, [0.0, 0.0]);
                if let Ok(baseline) = authoring.lower_direct_edit(&baseline) {
                    actions.push(EditorAction::Gesture(EditorGestureAction::Begin {
                        baseline,
                    }));
                    self.drag = Some(EguiUiCanvasDrag {
                        entity: selection.entity.clone(),
                        kind,
                        transform: selection.transform,
                        latest_delta: [0.0, 0.0],
                        applied_delta: [0.0, 0.0],
                        last_sent_stamp: None,
                        commit_requested: false,
                    });
                }
            }
        }

        let Some(mut drag) = self.drag.take() else {
            return;
        };
        let response = match drag.kind {
            EguiUiCanvasDragKind::Move => move_response.as_ref(),
            EguiUiCanvasDragKind::Resize => resize_response.as_ref(),
        };
        let gesture = authoring
            .gesture()
            .filter(|gesture| {
                let target = gesture.baseline().target();
                target.document() == authoring.document()
                    && target.base_revision() == authoring.revision()
                    && target.entity() == &drag.entity
            })
            .map(EditorGestureView::stamp);
        let exact_selection = selection.filter(|selection| {
            selection.entity == drag.entity && selection.transform == drag.transform
        });
        if !input.keeps_gesture_alive() || exact_selection.is_none() {
            if let Some(gesture) = gesture {
                actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                    gesture: gesture.id(),
                    reason: EditorGestureCancelReason::CanvasTransformChanged,
                }));
            }
            return;
        }
        let exact_selection = exact_selection.expect("the exact selection was checked");
        if let Some(response) = response {
            let delta = response.drag_delta();
            drag.latest_delta = [
                f64::from(delta.x) * exact_selection.authored_units_per_point[0],
                f64::from(delta.y) * exact_selection.authored_units_per_point[1],
            ];
            drag.commit_requested |= response.drag_stopped();
        }

        if ui.input(|state| state.viewport().focused == Some(false)) {
            if let Some(gesture) = gesture {
                actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                    gesture: gesture.id(),
                    reason: EditorGestureCancelReason::FocusLost,
                }));
            }
            return;
        }
        if ui.input(|state| state.key_pressed(egui::Key::Escape)) {
            if let Some(gesture) = gesture {
                actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                    gesture: gesture.id(),
                    reason: EditorGestureCancelReason::Escape,
                }));
            }
            return;
        }
        let Some(gesture) = gesture else {
            self.drag = Some(drag);
            return;
        };
        if !input.is_presented() {
            self.drag = Some(drag);
            return;
        }
        if drag.last_sent_stamp == Some(gesture) {
            self.drag = Some(drag);
            return;
        }

        let delta = [
            drag.latest_delta[0] - drag.applied_delta[0],
            drag.latest_delta[1] - drag.applied_delta[1],
        ];
        let edit = canvas_direct_edit(drag.kind, &drag.entity, delta);
        let Ok(edit) = authoring.lower_direct_edit(&edit) else {
            actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                gesture: gesture.id(),
                reason: EditorGestureCancelReason::InvalidPreview,
            }));
            return;
        };
        if drag.commit_requested {
            actions.push(EditorAction::Gesture(EditorGestureAction::Commit {
                gesture: gesture.id(),
                edit,
            }));
            return;
        }
        if delta != [0.0, 0.0] {
            actions.push(EditorAction::Gesture(EditorGestureAction::Preview {
                gesture: gesture.id(),
                edit,
            }));
            drag.applied_delta = drag.latest_delta;
            drag.last_sent_stamp = Some(gesture);
        }
        self.drag = Some(drag);
    }
}

fn canvas_direct_edit(
    kind: EguiUiCanvasDragKind,
    entity: &SceneEntityId,
    delta: [f64; 2],
) -> UiAuthoringDirectEdit {
    match kind {
        EguiUiCanvasDragKind::Move => UiAuthoringDirectEdit::MoveBy {
            entity: entity.clone(),
            delta,
        },
        EguiUiCanvasDragKind::Resize => UiAuthoringDirectEdit::ResizeBy {
            entity: entity.clone(),
            delta,
        },
    }
}

fn canvas_state_label(content: EguiUiCanvasContent) -> &'static str {
    match content {
        EguiUiCanvasContent::Presented { .. } => "Ready",
        EguiUiCanvasContent::Previewing { .. } => "Updating",
        EguiUiCanvasContent::Resizing => "Resizing",
        EguiUiCanvasContent::Unavailable => "Unavailable",
    }
}

fn canvas_texture_facts(content: EguiUiCanvasContent) -> Option<(TextureId, [u32; 2])> {
    match content {
        EguiUiCanvasContent::Presented { texture_id, extent }
        | EguiUiCanvasContent::Previewing { texture_id, extent } => Some((texture_id, extent)),
        EguiUiCanvasContent::Resizing | EguiUiCanvasContent::Unavailable => None,
    }
}
