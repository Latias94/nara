use dear_imgui_rs::{MouseButton, Ui};
use nara::render::{DisplayMapping2d, DisplayRect, DisplayScale, Extent2d};

use super::ImguiViewImage;

pub(super) struct ImguiSceneView {
    uv: DisplayRect,
}

impl Default for ImguiSceneView {
    fn default() -> Self {
        Self {
            uv: DisplayRect::UNIT,
        }
    }
}

pub(super) struct ImguiSceneViewOutput {
    pub desired_extent: Option<[f32; 2]>,
    pub mapping: Option<DisplayMapping2d>,
    pub pick_point: Option<[f32; 2]>,
    pub camera_changed: bool,
}

impl ImguiSceneView {
    pub fn show(
        &mut self,
        ui: &Ui,
        image: Option<ImguiViewImage>,
        presented: bool,
        editing: bool,
    ) -> ImguiSceneViewOutput {
        let before = self.uv;
        if ui.button("Reset View") {
            self.uv = DisplayRect::UNIT;
        }
        ui.same_line();
        ui.text(if presented {
            "Presented"
        } else {
            "Preparing Scene View"
        });
        let min = ui.cursor_screen_pos();
        let available = ui.content_region_avail();
        let size = [available[0].max(1.0), available[1].max(1.0)];
        let scale = super::framebuffer_scale(ui);
        let desired_extent = Some([size[0] * scale[0], size[1] * scale[1]]);
        let panel = DisplayRect {
            min,
            max: [min[0] + size[0], min[1] + size[1]],
        };
        let mapping = image.and_then(|image| {
            DisplayMapping2d::fit(
                Extent2d::new(image.extent[0], image.extent[1])?,
                panel,
                scale[0],
                DisplayScale::Fit,
            )?
            .with_uv(self.uv)
        });
        let mut pick_point = None;
        if let Some((image, mapping)) = image.zip(mapping) {
            let content = mapping.content_rect();
            ui.set_cursor_screen_pos(content.min);
            ui.image_config(
                image.texture,
                [
                    content.max[0] - content.min[0],
                    content.max[1] - content.min[1],
                ],
            )
            .uv0(self.uv.min)
            .uv1(self.uv.max)
            .build();
            if ui.is_item_hovered() {
                let pointer = ui.mouse_pos();
                if presented && editing && ui.is_item_clicked() {
                    pick_point = Some(pointer);
                }
                if ui.is_mouse_down(MouseButton::Middle) {
                    self.pan(ui.io().mouse_delta(), content);
                }
                let wheel = ui.io().mouse_wheel();
                if wheel != 0.0 {
                    self.zoom(pointer, content, wheel);
                }
            }
        } else {
            ui.text("Waiting for a Nara Scene image");
        }
        ui.set_cursor_screen_pos([panel.min[0], panel.max[1]]);
        ui.dummy([0.0, 0.0]);
        ImguiSceneViewOutput {
            desired_extent,
            mapping,
            pick_point,
            camera_changed: self.uv != before,
        }
    }

    fn pan(&mut self, delta: [f32; 2], content: DisplayRect) {
        let size = [
            content.max[0] - content.min[0],
            content.max[1] - content.min[1],
        ];
        if size.iter().any(|size| *size <= 0.0) {
            return;
        }
        let span = [
            self.uv.max[0] - self.uv.min[0],
            self.uv.max[1] - self.uv.min[1],
        ];
        let min =
            std::array::from_fn(|axis| self.uv.min[axis] - delta[axis] / size[axis] * span[axis]);
        self.uv = clamp_uv(min, span);
    }

    fn zoom(&mut self, pointer: [f32; 2], content: DisplayRect, wheel: f32) {
        let anchor: [f32; 2] = std::array::from_fn(|axis| {
            ((pointer[axis] - content.min[axis]) / (content.max[axis] - content.min[axis]))
                .clamp(0.0, 1.0)
        });
        let span: [f32; 2] = std::array::from_fn(|axis| self.uv.max[axis] - self.uv.min[axis]);
        let next: [f32; 2] = span.map(|span| (span * (-wheel * 0.1).exp()).clamp(0.125, 1.0));
        let min = std::array::from_fn(|axis| {
            self.uv.min[axis] + anchor[axis] * (span[axis] - next[axis])
        });
        self.uv = clamp_uv(min, next);
    }
}

fn clamp_uv(min: [f32; 2], span: [f32; 2]) -> DisplayRect {
    let min = std::array::from_fn(|axis| min[axis].clamp(0.0, 1.0 - span[axis]));
    DisplayRect {
        min,
        max: [min[0] + span[0], min[1] + span[1]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_preserves_the_pointer_target_and_pan_stays_inside_the_image() {
        let content = DisplayRect {
            min: [100.0, 200.0],
            max: [900.0, 600.0],
        };
        let target = Extent2d::new(800, 400).unwrap();
        let pointer = [300.0, 500.0];
        let before = DisplayMapping2d::fit(target, content, 1.5, DisplayScale::Fit).unwrap();
        let mut view = ImguiSceneView::default();
        view.zoom(pointer, content, 3.0);
        let after = before.with_uv(view.uv).unwrap();
        let old_target = before.panel_to_target(pointer).unwrap();
        let new_target = after.panel_to_target(pointer).unwrap();
        for axis in 0..2 {
            assert!((old_target[axis] - new_target[axis]).abs() < 0.001);
        }
        view.pan([100000.0, -100000.0], content);
        assert!(DisplayRect::UNIT.contains(view.uv.min));
        assert!(DisplayRect::UNIT.contains(view.uv.max));
        assert_eq!(view.uv.min[0], 0.0);
        assert_eq!(view.uv.max[1], 1.0);
    }
}
