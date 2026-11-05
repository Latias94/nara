use std::time::Instant;

use nara::render::{DisplayMapping2d, DisplayRect};
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::MouseButton,
    window::CursorIcon,
};

use super::{
    Capabilities, EditorClipboard, EditorPanelCapability, EditorPanelImageHost,
    EditorPanelTextInputScope,
};

/// Frontend caret rectangle in the accepted image's target pixels. Zero width is valid.
/// A Nara frontend first applies its own existing content transform to obtain these pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorPanelImeArea {
    pub position: [f32; 2],
    pub size: [f32; 2],
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct LogicalImeArea {
    position: [f64; 2],
    size: [f64; 2],
}

impl LogicalImeArea {
    fn from_target(
        area: EditorPanelImeArea,
        mapping: DisplayMapping2d,
        clip: DisplayRect,
    ) -> Result<Self, EditorPanelServiceError> {
        let extent = [
            mapping.target().width as f64,
            mapping.target().height as f64,
        ];
        if (0..2).any(|axis| {
            !area.position[axis].is_finite()
                || !area.size[axis].is_finite()
                || area.position[axis] < 0.0
                || area.size[axis] < 0.0
                || f64::from(area.position[axis]) + f64::from(area.size[axis]) > extent[axis]
        }) {
            return Err(EditorPanelServiceError::InvalidImeArea);
        }
        let content = mapping.content_rect();
        let scale: [f64; 2] = std::array::from_fn(|axis| {
            (f64::from(content.max[axis]) - f64::from(content.min[axis])) / extent[axis]
        });
        let mut position = std::array::from_fn(|axis| {
            f64::from(content.min[axis]) + f64::from(area.position[axis]) * scale[axis]
        });
        let mut size = std::array::from_fn(|axis| f64::from(area.size[axis]) * scale[axis]);
        if position
            .iter()
            .chain(size.iter())
            .any(|value| !value.is_finite())
        {
            return Err(EditorPanelServiceError::InvalidImeArea);
        }
        for axis in 0..2 {
            let visible_min = f64::from(content.min[axis].max(clip.min[axis]));
            let visible_max = f64::from(content.max[axis].min(clip.max[axis]));
            if size[axis] == 0.0 {
                if position[axis] < visible_min || position[axis] > visible_max {
                    return Err(EditorPanelServiceError::InvalidImeArea);
                }
            } else {
                let min = position[axis].max(visible_min);
                let max = (position[axis] + size[axis]).min(visible_max);
                if min >= max {
                    return Err(EditorPanelServiceError::InvalidImeArea);
                }
                position[axis] = min;
                size[axis] = max - min;
            }
        }
        Ok(Self { position, size })
    }
}

#[cfg(test)]
mod tests {
    use nara::render::{DisplayScale, Extent2d};

    use super::*;

    #[test]
    fn ime_exclusion_is_clipped_after_fractional_mapping_without_relocating_caret() {
        let mapping = DisplayMapping2d::fit(
            Extent2d::new(200, 100).unwrap(),
            DisplayRect {
                min: [10.25, 20.5],
                max: [310.25, 170.5],
            },
            1.5,
            DisplayScale::Fit,
        )
        .unwrap();
        let clip = DisplayRect {
            min: [10.25, 35.5],
            max: [160.25, 155.5],
        };
        let convert = |position, size| {
            LogicalImeArea::from_target(EditorPanelImeArea { position, size }, mapping, clip)
        };
        let crossing = convert([90.0, 0.0], [11.0, 100.0]).unwrap();
        assert_eq!(crossing.position, [145.25, 35.5]);
        assert_eq!(crossing.size, [15.0, 120.0]);
        let edge = convert([100.0, 20.0], [0.0, 16.0]).unwrap();
        assert_eq!(edge.position, [160.25, 50.5]);
        assert_eq!(edge.size, [0.0, 24.0]);
        for (position, size) in [
            ([150.0, 20.0], [0.0, 16.0]),
            ([100.25, 20.0], [0.0, 16.0]),
            ([110.0, 20.0], [10.0, 16.0]),
            ([50.0, 0.0], [10.0, 0.0]),
        ] {
            assert!(matches!(
                convert(position, size),
                Err(EditorPanelServiceError::InvalidImeArea)
            ));
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EditorPanelServiceError {
    #[error("the attachment did not declare {0:?}")]
    CapabilityNotDeclared(EditorPanelCapability),
    #[error("native services require an active accepted endpoint, not a cancelled delivery")]
    InactiveEndpoint,
    #[error("text input and IME services require the current window keyboard owner")]
    NotKeyboardOwner,
    #[error("this delivery belongs to a retired provider-local text-input scope")]
    StaleTextInputScope,
    #[error("cursor access requires the current hovered or captured pointer owner")]
    NotPointerOwner,
    #[error("pointer capture can only be requested from the matching Pressed delivery")]
    CaptureRequiresPress,
    #[error("the endpoint does not own pointer capture")]
    NotCaptureOwner,
    #[error("the original captured button must be released before another button can capture")]
    CaptureButtonConflict,
    #[error("the IME caret must be finite, nonnegative and visible inside the image clip")]
    InvalidImeArea,
    #[error("native clipboard access failed: {0}")]
    Clipboard(String),
}

#[derive(Clone, Copy)]
pub(super) enum CaptureRequest {
    Capture(MouseButton),
    Release,
}

#[derive(Default)]
pub(super) struct ServiceRequests {
    pub capture: Option<CaptureRequest>,
    pub text_input: Option<Option<EditorPanelTextInputScope>>,
    pub ime_area: Option<LogicalImeArea>,
    pub cursor: Option<CursorIcon>,
    pub redraw: bool,
    pub clear_deadline: bool,
    pub deadline: Option<Instant>,
}

/// Finite native services scoped to one original accepted input delivery.
///
/// Every mutating request reports undeclared capabilities or incorrect ownership explicitly.
/// Clipboard calls preserve the original shared provider's errors; no fallback is substituted.
/// Successful requests take effect before the next chronological callback in the same dispatch.
pub struct EditorPanelServices<'a> {
    pub(super) clipboard: &'a EditorClipboard,
    pub(super) capabilities: Capabilities,
    pub(super) active: bool,
    pub(super) keyboard_owner: bool,
    pub(super) pointer_owner: bool,
    pub(super) capture_owner: bool,
    pub(super) captured_button: Option<MouseButton>,
    pub(super) pressed: Option<MouseButton>,
    pub(super) ime_ready: bool,
    pub(super) current_text_scope: Option<EditorPanelTextInputScope>,
    pub(super) delivery_text_scope: Option<EditorPanelTextInputScope>,
    pub(super) mapping: DisplayMapping2d,
    pub(super) clip: DisplayRect,
    pub(super) requests: ServiceRequests,
}

impl EditorPanelServices<'_> {
    fn require(&self, capability: EditorPanelCapability) -> Result<(), EditorPanelServiceError> {
        if !self.capabilities.has(capability) {
            return Err(EditorPanelServiceError::CapabilityNotDeclared(capability));
        }
        if !self.active {
            return Err(EditorPanelServiceError::InactiveEndpoint);
        }
        Ok(())
    }

    fn require_keyboard(&self) -> Result<(), EditorPanelServiceError> {
        self.require(EditorPanelCapability::TextInput)?;
        if !self.keyboard_owner {
            return Err(EditorPanelServiceError::NotKeyboardOwner);
        }
        if self.delivery_text_scope.is_some() && self.delivery_text_scope != self.current_text_scope
        {
            return Err(EditorPanelServiceError::StaleTextInputScope);
        }
        Ok(())
    }

    pub fn read_clipboard(&self) -> Result<String, EditorPanelServiceError> {
        self.require(EditorPanelCapability::Clipboard)?;
        self.clipboard
            .read()
            .map_err(EditorPanelServiceError::Clipboard)
    }

    pub fn write_clipboard(&self, text: &str) -> Result<(), EditorPanelServiceError> {
        self.require(EditorPanelCapability::Clipboard)?;
        self.clipboard
            .write(text)
            .map_err(EditorPanelServiceError::Clipboard)
    }

    /// Captures only this delivery's pressed button. Other button releases cannot end it.
    pub fn request_pointer_capture(
        &mut self,
        button: MouseButton,
    ) -> Result<(), EditorPanelServiceError> {
        self.require(EditorPanelCapability::Pointer)?;
        if self.pressed != Some(button) {
            return Err(EditorPanelServiceError::CaptureRequiresPress);
        }
        if self
            .captured_button
            .is_some_and(|captured| captured != button)
        {
            return Err(EditorPanelServiceError::CaptureButtonConflict);
        }
        self.requests.capture = Some(CaptureRequest::Capture(button));
        Ok(())
    }

    pub fn release_pointer_capture(&mut self) -> Result<(), EditorPanelServiceError> {
        self.require(EditorPanelCapability::Pointer)?;
        if !self.capture_owner {
            return Err(EditorPanelServiceError::NotCaptureOwner);
        }
        self.requests.capture = Some(CaptureRequest::Release);
        Ok(())
    }

    /// Requests this provider-local text-focus lifetime for the current keyboard owner. `None`
    /// disables input. A different scope resets IME readiness even inside the same panel; an
    /// already enabled composition must acknowledge Disabled before the successor can enable.
    pub fn set_text_input(
        &mut self,
        scope: Option<EditorPanelTextInputScope>,
    ) -> Result<(), EditorPanelServiceError> {
        self.require_keyboard()?;
        self.requests.text_input = Some(scope);
        Ok(())
    }

    pub fn ime_ready(&self) -> Result<bool, EditorPanelServiceError> {
        self.require_keyboard()?;
        let changing = self
            .requests
            .text_input
            .as_ref()
            .is_some_and(|scope| scope != &self.current_text_scope);
        Ok(self.ime_ready && !changing)
    }

    /// Stores a target-pixel caret/exclusion area through the accepted display mapping. Nonzero
    /// areas are clipped to visible content; invisible areas and outside zero-width carets refuse.
    /// Native caret publication waits for IME readiness, and is reset on owner retirement.
    pub fn set_ime_area(
        &mut self,
        area: EditorPanelImeArea,
    ) -> Result<(), EditorPanelServiceError> {
        self.require_keyboard()?;
        self.requests.ime_area = Some(LogicalImeArea::from_target(area, self.mapping, self.clip)?);
        Ok(())
    }

    pub fn set_cursor(&mut self, cursor: CursorIcon) -> Result<(), EditorPanelServiceError> {
        self.require(EditorPanelCapability::Cursor)?;
        if !self.pointer_owner {
            return Err(EditorPanelServiceError::NotPointerOwner);
        }
        self.requests.cursor = Some(cursor);
        Ok(())
    }

    /// Coalesces immediate redraw demand until the next successful stage begins a paint attempt.
    pub fn request_redraw(&mut self) -> Result<(), EditorPanelServiceError> {
        self.require(EditorPanelCapability::Refresh)?;
        self.requests.redraw = true;
        Ok(())
    }

    /// Retains the earliest requested deadline for this original live panel. No static polling.
    pub fn request_redraw_at(&mut self, deadline: Instant) -> Result<(), EditorPanelServiceError> {
        self.require(EditorPanelCapability::Refresh)?;
        self.requests.deadline = Some(
            self.requests
                .deadline
                .map_or(deadline, |previous| previous.min(deadline)),
        );
        Ok(())
    }

    pub fn clear_redraw_deadline(&mut self) -> Result<(), EditorPanelServiceError> {
        self.require(EditorPanelCapability::Refresh)?;
        self.requests.clear_deadline = true;
        self.requests.deadline = None;
        Ok(())
    }
}

impl<S> EditorPanelImageHost<S> {
    pub(super) fn reset_ime(&mut self) {
        if self.ime.published_area.is_some() {
            self.window
                .set_ime_cursor_area(LogicalPosition::new(0.0, 0.0), LogicalSize::new(0.0, 0.0));
        }
        if self.ime.allowed || self.ime.ready {
            if self.ime.ready {
                self.ime.waiting_disabled = true;
                self.ime.retiring_scope = self.ime.scope_owner.clone().zip(self.ime.scope.clone());
            }
            self.window.set_ime_allowed(false);
        }
        self.ime.scope = None;
        self.ime.scope_owner = None;
        self.ime.allowed = false;
        self.ime.ready = false;
        self.ime.area = None;
        self.ime.published_area = None;
    }

    pub(super) fn publish_ime_area(&mut self) {
        if self.ime.ready {
            if let Some(area) = self.ime.area {
                if self.ime.published_area != Some(area) {
                    self.window.set_ime_cursor_area(
                        LogicalPosition::new(area.position[0], area.position[1]),
                        LogicalSize::new(area.size[0], area.size[1]),
                    );
                    self.ime.published_area = Some(area);
                }
            }
        }
    }

    pub(super) fn enable_requested_ime(&mut self) {
        if self.ime.scope.is_some()
            && !self.ime.allowed
            && !self.ime.waiting_disabled
            && self.focused
            && self.keyboard.is_some()
            && !self.closed
        {
            self.window.set_ime_allowed(true);
            self.ime.allowed = true;
        }
    }

    pub(super) fn apply_services(
        &mut self,
        image: &super::EditorPanelImage<S>,
        requests: ServiceRequests,
    ) {
        if let Some(capture) = requests.capture {
            match capture {
                CaptureRequest::Capture(button) => {
                    self.capture = Some(super::PointerCapture {
                        image: image.clone(),
                        button,
                    });
                }
                CaptureRequest::Release => {
                    self.capture = None;
                }
            }
        }
        if let Some(scope) = requests.text_input {
            if scope != self.ime.scope || self.ime.scope_owner.as_ref() != Some(&image.token) {
                self.reset_ime();
                self.ime.scope = scope;
                self.ime.scope_owner = self.ime.scope.as_ref().map(|_| image.token.clone());
            }
            self.enable_requested_ime();
        }
        if let Some(area) = requests.ime_area {
            self.ime.area = Some(area);
            self.publish_ime_area();
        }
        if let Some(cursor) = requests.cursor {
            if cursor != self.cursor {
                self.window.set_cursor(cursor);
                self.cursor = cursor;
            }
        }
        if requests.redraw {
            self.request_redraw();
        }
        if let Some(entry) = self
            .attachments
            .iter_mut()
            .find(|entry| entry.token == image.token)
        {
            if requests.clear_deadline {
                entry.deadline = None;
            }
            if let Some(deadline) = requests.deadline {
                entry.deadline = Some(
                    entry
                        .deadline
                        .map_or(deadline, |previous| previous.min(deadline)),
                );
            }
        }
    }
}
