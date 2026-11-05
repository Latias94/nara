//! A consumer-owned frontend over semantic Editor panels and shared text primitives.
//!
//! The native adapter owns placement, input admission, modal arbitration and GPU upload. This
//! module owns three local controls and a finite CPU painter; it has no document or GPU owner.

use std::{
    io,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    project_host::EditorEditPreparationError,
    reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue,
    },
    scene::{AuthoringPropertyEdit, SceneEntityId},
    tooling::{EditorDocumentQueryBasis, PreparedEditorDocumentEdit},
};
use nara_editor::{
    PanelActionDispatch, PanelSubmissionId,
    panels::{EditorPanel, EditorPanelContext},
};
use nara_text::{
    FontFaceId, TextEdit, TextEditor, TextEngine, TextLimits, TextPaintSnapshot, TextRect,
    bundled_font_face,
};

use crate::{COMPONENT, Evidence, FIELD, INITIAL, Result};

pub const LOGICAL_SIZE: [f32; 2] = [400.0, 180.0];
pub const MAX_SCALE: f32 = 4.0;
pub const MAX_DRAFT_BYTES: usize = 512;

static NEXT_INCARNATION: AtomicU64 = AtomicU64::new(1);

fn text_limits() -> TextLimits {
    TextLimits {
        max_text_bytes: MAX_DRAFT_BYTES,
        max_glyphs: MAX_DRAFT_BYTES,
        max_scale_factor: MAX_SCALE,
        ..TextLimits::default()
    }
}

/// Fixed panel-local logical geometry. The native adapter supplies the surface placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub field: TextRect,
    pub review: TextRect,
    pub modal: TextRect,
    pub apply: TextRect,
}

impl Geometry {
    pub const FIXED: Self = Self {
        field: TextRect {
            x0: 12.0,
            y0: 12.0,
            x1: 172.0,
            y1: 52.0,
        },
        review: TextRect {
            x0: 12.0,
            y0: 70.0,
            x1: 172.0,
            y1: 110.0,
        },
        modal: TextRect {
            x0: 190.0,
            y0: 12.0,
            x1: 370.0,
            y1: 152.0,
        },
        apply: TextRect {
            x0: 198.0,
            y0: 36.0,
            x1: 358.0,
            y1: 76.0,
        },
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Field,
    Review,
    Apply,
}

/// A field lifetime, scoped to one panel incarnation. Callers cannot mint tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputToken {
    incarnation: u64,
    generation: u64,
}

/// Inputs already admitted by the native Host, in panel-local logical coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    PointerMove {
        x: f32,
        y: f32,
    },
    PointerDown {
        x: f32,
        y: f32,
    },
    PointerUp {
        x: f32,
        y: f32,
    },
    Focus(Control),
    Activate(Control),
    Text {
        token: InputToken,
        edit: TextEdit,
    },
    /// Native focus loss or route cancellation; the modal itself stays open.
    Blur,
    DismissModal,
}

/// An immutable captured display and its exact local input geometry.
///
/// Clones retain the original text layout, selection, focus lifetime and modal state. Painting
/// this value never reaches a live panel. Local focus, presses and text edits can advance while
/// this capture stays presented; geometric text input separately requires its original layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    incarnation: u64,
    revision: u64,
    geometry: Geometry,
    draft: String,
    text: TextPaintSnapshot,
    origin: [f32; 2],
    caret: Option<TextRect>,
    selection: Vec<TextRect>,
    ime_area: TextRect,
    focused: Option<Control>,
    pressed: Option<Control>,
    input_token: Option<InputToken>,
    popup: bool,
    popup_generation: usize,
}

impl Snapshot {
    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }

    /// An observation counter, not an input-validity fence.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn geometry(&self) -> Geometry {
        self.geometry
    }

    pub fn draft(&self) -> &str {
        &self.draft
    }

    pub fn display_text(&self) -> &str {
        self.text.display_text()
    }

    pub fn text_snapshot(&self) -> &TextPaintSnapshot {
        &self.text
    }

    pub fn field_text_origin(&self) -> [f32; 2] {
        self.origin
    }

    pub fn field_text_clip(&self) -> TextRect {
        inset(self.geometry.field, 8.0, 4.0)
    }

    /// The candidate exclusion area in panel-local logical coordinates, clipped to the field.
    pub fn ime_area(&self) -> Option<TextRect> {
        self.input_token.map(|_| {
            intersect(
                translate(self.ime_area, self.origin),
                self.field_text_clip(),
            )
        })
    }

    pub fn focused(&self) -> Option<Control> {
        self.focused
    }

    pub fn pressed(&self) -> Option<Control> {
        self.pressed
    }

    pub fn input_token(&self) -> Option<InputToken> {
        self.input_token
    }

    pub fn popup_open(&self) -> bool {
        self.popup
    }

    /// Identifies this panel's popup show cycle, including captures made after it closes.
    pub fn popup_generation(&self) -> usize {
        self.popup_generation
    }

    /// While the modal is open, all underlay and outside points have no local control target.
    /// The Host still owns arbitration against other panels and native surfaces.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Control> {
        if self.popup {
            return contains(self.geometry.apply, x, y).then_some(Control::Apply);
        }
        if contains(self.geometry.field, x, y) {
            Some(Control::Field)
        } else if contains(self.geometry.review, x, y) {
            Some(Control::Review)
        } else {
            None
        }
    }
}

/// Retained frontend state. Only `prepare_edit` crosses the original Editor context boundary.
pub struct ConformancePanel {
    serial: usize,
    incarnation: u64,
    revision: u64,
    evidence: Rc<Evidence>,
    text: TextEngine,
    field: TextEditor,
    focus_generation: u64,
    input_token: Option<InputToken>,
    focused: Option<Control>,
    pressed: Option<Control>,
    popup: bool,
    reviews: usize,
}

impl ConformancePanel {
    pub fn new(evidence: Rc<Evidence>) -> Result<Self> {
        let serial = evidence
            .created
            .get()
            .checked_add(1)
            .ok_or_else(|| io::Error::other("panel serial capacity exhausted"))?;
        let incarnation = NEXT_INCARNATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| io::Error::other("panel incarnation capacity exhausted"))?;
        let mut text = TextEngine::new(bundled_font_face()?, text_limits())?;
        let field = TextEditor::new(&mut text, INITIAL, 18.0)?;
        let panel = Self {
            serial,
            incarnation,
            revision: 0,
            evidence: Rc::clone(&evidence),
            text,
            field,
            focus_generation: 0,
            input_token: None,
            focused: None,
            pressed: None,
            popup: false,
            reviews: 0,
        };
        evidence.created.set(serial);
        Ok(panel)
    }

    pub fn serial(&self) -> usize {
        self.serial
    }

    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }

    pub fn draft(&self) -> &str {
        self.field.text()
    }

    pub fn display_text(&self) -> &str {
        self.field.display_text()
    }

    pub fn editor(&self) -> &TextEditor {
        &self.field
    }

    pub fn input_token(&self) -> Option<InputToken> {
        self.input_token
    }

    pub fn focused(&self) -> Option<Control> {
        self.focused
    }

    pub fn popup_open(&self) -> bool {
        self.popup
    }

    pub fn reviews(&self) -> usize {
        self.reviews
    }

    /// Captures state without editing, shaping or issuing semantic work.
    pub fn compose(&self) -> Snapshot {
        let geometry = Geometry::FIXED;
        let clip = inset(geometry.field, 8.0, 4.0);
        let width = clip.x1 - clip.x0;
        let scroll = self
            .field
            .caret()
            .map_or(0.0, |caret| (caret.x1 - width).max(0.0));
        let origin = [
            clip.x0 - scroll,
            clip.y0 + ((clip.y1 - clip.y0 - self.field.layout().height()) * 0.5).max(0.0),
        ];
        Snapshot {
            incarnation: self.incarnation,
            revision: self.revision,
            geometry,
            draft: self.draft().into(),
            text: self.field.paint_snapshot(),
            origin,
            caret: self.field.caret(),
            selection: self.field.selection_rects().to_vec(),
            ime_area: self.field.ime_area(),
            focused: self.focused,
            pressed: self.pressed,
            input_token: self.input_token,
            popup: self.popup,
            popup_generation: self.reviews,
        }
    }

    /// Reduces admitted input against the actual presented capture. Returns only Apply intent.
    ///
    /// Focus, button gestures and token-bound text edits can share one capture between paints.
    /// A different incarnation, geometry or popup cycle refuses control input. Field Point also
    /// requires matching presented text/layout and no active composition. Blur always cancels
    /// this incarnation's local input, including when a popup transition awaits presentation.
    /// Refused input and failed text edits adopt no local state.
    pub fn reduce(&mut self, snapshot: &Snapshot, input: &Input) -> Result<bool> {
        if snapshot.incarnation != self.incarnation {
            return Err(io::Error::other("foreign conformance snapshot").into());
        }
        if !matches!(input, Input::Blur)
            && (snapshot.geometry != Geometry::FIXED
                || snapshot.popup != self.popup
                || snapshot.popup_generation != self.reviews)
        {
            return Err(io::Error::other("incompatible conformance control presentation").into());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| io::Error::other("panel revision capacity exhausted"))?;
        let controls = (
            self.focused,
            self.pressed,
            self.input_token,
            self.popup,
            self.reviews,
        );
        let mut text_changed = false;
        let mut apply = false;
        match input {
            Input::PointerMove { x, y } => validate_point(*x, *y)?,
            Input::PointerDown { x, y } => {
                validate_point(*x, *y)?;
                let target = snapshot.hit_test(*x, *y);
                if target == Some(Control::Field) {
                    self.validate_field_point(snapshot)?;
                    let token = self.field_token()?;
                    text_changed = self
                        .field
                        .edit(
                            &mut self.text,
                            &TextEdit::Point {
                                x: *x - snapshot.origin[0],
                                y: *y - snapshot.origin[1],
                                extend: false,
                            },
                        )?
                        .changed();
                    self.focus_generation = token.generation;
                    self.input_token = Some(token);
                    self.focused = target;
                } else if target.is_some() || !self.popup {
                    self.set_focus(target)?;
                }
                self.pressed = target.filter(|control| *control != Control::Field);
            }
            Input::PointerUp { x, y } => {
                validate_point(*x, *y)?;
                if let Some(control) = self.pressed
                    && snapshot.hit_test(*x, *y) == Some(control)
                {
                    apply = self.activate(control)?;
                }
                self.pressed = None;
            }
            Input::Focus(control) => {
                if self.control_available(*control) {
                    self.set_focus(Some(*control))?;
                    self.pressed = None;
                }
            }
            Input::Activate(control) => {
                apply = self.activate(*control)?;
                self.pressed = None;
            }
            Input::Text { token, edit } => {
                if self.input_token != Some(*token)
                    || self.focused != Some(Control::Field)
                    || self.popup
                {
                    return Err(io::Error::other("stale conformance text input token").into());
                }
                if matches!(edit, TextEdit::Point { .. }) {
                    self.validate_field_point(snapshot)?;
                }
                text_changed = self.field.edit(&mut self.text, edit)?.changed();
            }
            Input::Blur => {
                self.set_focus(None)?;
                self.pressed = None;
            }
            Input::DismissModal => {
                if self.popup {
                    self.set_focus(None)?;
                    self.popup = false;
                    self.pressed = None;
                }
            }
        }
        if text_changed
            || controls
                != (
                    self.focused,
                    self.pressed,
                    self.input_token,
                    self.popup,
                    self.reviews,
                )
        {
            self.revision = revision;
        }
        Ok(apply)
    }

    /// Semantic setup for CPU callers. Native callers use `reduce` after Host admission.
    pub fn focus_for_cpu(&mut self) -> Result<InputToken> {
        self.reduce(&self.compose(), &Input::Focus(Control::Field))?;
        self.input_token
            .ok_or_else(|| io::Error::other("the modal blocks field focus").into())
    }

    pub fn edit(&mut self, token: InputToken, edit: &TextEdit) -> Result<()> {
        self.reduce(
            &self.compose(),
            &Input::Text {
                token,
                edit: edit.clone(),
            },
        )?;
        Ok(())
    }

    pub fn replace_for_cpu(&mut self, value: &str) -> Result<InputToken> {
        let token = self.focus_for_cpu()?;
        self.edit(token, &TextEdit::SelectAll)?;
        self.edit(token, &TextEdit::Commit(value.into()))?;
        Ok(token)
    }

    /// Prepares the original Context's property edit. Dropping it performs no document mutation.
    pub fn prepare_edit(
        &self,
        context: &mut EditorPanelContext<'_, '_>,
        basis: &EditorDocumentQueryBasis,
        entity: SceneEntityId,
    ) -> std::result::Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        context.prepare_authoring_property_edit(
            basis,
            &[AuthoringPropertyEdit {
                entity,
                component: ComponentTypeId::new(COMPONENT),
                component_version: ComponentSchemaVersion::ONE,
                property: ComponentPropertyPath::new(ComponentFieldId::new(FIELD)),
                value: ComponentValue::String(self.draft().into()),
            }],
        )
    }

    fn control_available(&self, control: Control) -> bool {
        match control {
            Control::Field | Control::Review => !self.popup,
            Control::Apply => self.popup,
        }
    }

    fn validate_field_point(&self, snapshot: &Snapshot) -> Result<()> {
        if self.field.is_composing() || snapshot.text != self.field.paint_snapshot() {
            return Err(io::Error::other(
                "field Point requires matching presented text/layout and no active composition",
            )
            .into());
        }
        Ok(())
    }

    fn field_token(&self) -> Result<InputToken> {
        if let Some(token) = self.input_token {
            return Ok(token);
        }
        Ok(InputToken {
            incarnation: self.incarnation,
            generation: self
                .focus_generation
                .checked_add(1)
                .ok_or_else(|| io::Error::other("field focus capacity exhausted"))?,
        })
    }

    fn set_focus(&mut self, target: Option<Control>) -> Result<()> {
        if target == self.focused {
            return Ok(());
        }
        let token = if target == Some(Control::Field) {
            Some(self.field_token()?)
        } else {
            None
        };
        if self.focused == Some(Control::Field) {
            self.field
                .edit(&mut self.text, &TextEdit::CancelComposition)?;
        }
        if let Some(token) = token {
            self.focus_generation = token.generation;
        }
        self.input_token = token;
        self.focused = target;
        Ok(())
    }

    fn activate(&mut self, control: Control) -> Result<bool> {
        if !self.control_available(control) {
            return Ok(false);
        }
        match control {
            Control::Field => self.set_focus(Some(Control::Field))?,
            Control::Review => {
                let reviews = self
                    .reviews
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("review capacity exhausted"))?;
                self.set_focus(Some(Control::Apply))?;
                self.reviews = reviews;
                self.popup = true;
            }
            Control::Apply => {
                self.set_focus(None)?;
                self.popup = false;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl EditorPanel for ConformancePanel {
    fn action_finished(&mut self, id: PanelSubmissionId, dispatch: PanelActionDispatch) {
        self.evidence
            .finished
            .borrow_mut()
            .push((self.serial, id, dispatch));
    }
}

impl Drop for ConformancePanel {
    fn drop(&mut self) {
        self.evidence.dropped.borrow_mut().push(self.serial);
    }
}

/// A bounded software painter using actual retained layout and bundled-font raster provenance.
///
/// Output is top-to-bottom, tightly packed, opaque RGBA8 (four bytes per pixel). Color channels
/// are linear; alpha is always 255, so the bytes also satisfy premultiplied-alpha interpretation.
/// No atlas, persistent image history, native resources or performance claim is involved.
pub struct SoftwarePainter {
    text: TextEngine,
    review: TextPaintSnapshot,
    apply: TextPaintSnapshot,
    title: TextPaintSnapshot,
}

impl SoftwarePainter {
    pub fn new() -> Result<Self> {
        let mut text = TextEngine::new(bundled_font_face()?, text_limits())?;
        let review = TextEditor::new(&mut text, "Review change", 16.0)?.paint_snapshot();
        let apply = TextEditor::new(&mut text, "Apply caption", 16.0)?.paint_snapshot();
        let title = TextEditor::new(&mut text, "Review caption", 14.0)?.paint_snapshot();
        Ok(Self {
            text,
            review,
            apply,
            title,
        })
    }

    pub fn font_face_id(&self) -> FontFaceId {
        self.text.font_face_id()
    }

    /// Round both logical extents to physical pixels; refuse zero-sized or excessive captures.
    pub fn pixel_size(scale: f32) -> Result<[u32; 2]> {
        if !scale.is_finite() || scale <= 0.0 || scale > MAX_SCALE {
            return Err(io::Error::other("conformance scale must be finite and in (0, 4]").into());
        }
        let size = LOGICAL_SIZE.map(|extent| (extent * scale).round() as u32);
        if size.contains(&0) {
            return Err(io::Error::other("conformance scale produces an empty image").into());
        }
        Ok(size)
    }

    /// Repeated paints consume only this capture, never live editing or semantic panel state.
    pub fn paint(&mut self, snapshot: &Snapshot, scale: f32) -> Result<Vec<u8>> {
        let [width, height] = Self::pixel_size(scale)?;
        let mut canvas = Canvas {
            width,
            height,
            scale,
            rgba: vec![0; width as usize * height as usize * 4],
        };
        canvas.fill(
            TextRect {
                x0: 0.0,
                y0: 0.0,
                x1: LOGICAL_SIZE[0],
                y1: LOGICAL_SIZE[1],
            },
            [28, 32, 40],
        );
        let geometry = snapshot.geometry;
        let field_border = if snapshot.focused == Some(Control::Field) {
            [100, 180, 255]
        } else {
            [125, 135, 150]
        };
        canvas.fill(geometry.field, field_border);
        canvas.fill(inset(geometry.field, 1.0, 1.0), [40, 46, 56]);
        let field_clip = snapshot.field_text_clip();
        if snapshot.input_token.is_some() {
            for selection in &snapshot.selection {
                canvas.fill(
                    intersect(translate(*selection, snapshot.origin), field_clip),
                    [47, 84, 122],
                );
            }
        }
        paint_text(
            &mut self.text,
            &mut canvas,
            &snapshot.text,
            field_clip,
            snapshot.origin,
            [240, 243, 248],
        )?;
        if snapshot.input_token.is_some()
            && let Some(caret) = snapshot.caret
        {
            canvas.fill(
                intersect(translate(caret, snapshot.origin), field_clip),
                [255, 220, 115],
            );
        }
        paint_button(
            &mut self.text,
            &mut canvas,
            &self.review,
            geometry.review,
            Control::Review,
            snapshot,
        )?;
        if snapshot.popup {
            canvas.fill(geometry.modal, [155, 170, 190]);
            canvas.fill(inset(geometry.modal, 1.0, 1.0), [52, 60, 76]);
            let title_clip = TextRect {
                x0: 198.0,
                y0: 14.0,
                x1: 362.0,
                y1: 34.0,
            };
            paint_text(
                &mut self.text,
                &mut canvas,
                &self.title,
                title_clip,
                [198.0, 14.0],
                [240, 243, 248],
            )?;
            paint_button(
                &mut self.text,
                &mut canvas,
                &self.apply,
                geometry.apply,
                Control::Apply,
                snapshot,
            )?;
            let preview = TextRect {
                x0: 198.0,
                y0: 88.0,
                x1: 362.0,
                y1: 144.0,
            };
            paint_text(
                &mut self.text,
                &mut canvas,
                &snapshot.text,
                preview,
                [198.0, 88.0],
                [240, 243, 248],
            )?;
        }
        Ok(canvas.rgba)
    }
}

fn paint_button(
    text: &mut TextEngine,
    canvas: &mut Canvas,
    label: &TextPaintSnapshot,
    rect: TextRect,
    control: Control,
    snapshot: &Snapshot,
) -> Result<()> {
    let border = if snapshot.focused == Some(control) {
        [100, 180, 255]
    } else {
        [125, 135, 150]
    };
    let fill = if snapshot.pressed == Some(control) {
        [42, 85, 125]
    } else {
        [48, 105, 154]
    };
    canvas.fill(rect, border);
    canvas.fill(inset(rect, 1.0, 1.0), fill);
    let clip = inset(rect, 6.0, 4.0);
    let origin = [
        clip.x0,
        clip.y0 + ((clip.y1 - clip.y0 - label.height()) * 0.5).max(0.0),
    ];
    paint_text(text, canvas, label, clip, origin, [248, 250, 255])
}

fn paint_text(
    text: &mut TextEngine,
    canvas: &mut Canvas,
    source: &TextPaintSnapshot,
    clip: TextRect,
    origin: [f32; 2],
    color: [u8; 3],
) -> Result<()> {
    let layout = text.prepare_window_at(source, clip, canvas.scale, origin, [0.0; 2])?;
    let [cx0, cy0, cx1, cy1] = canvas.edges(clip);
    for glyph in layout.glyphs() {
        let raster = text.rasterize(glyph.key())?;
        let left = ((origin[0] + glyph.x()) * canvas.scale).round() as i32 + raster.left();
        let top = ((origin[1] + glyph.y()) * canvas.scale).round() as i32 - raster.top();
        let right = (left + raster.width() as i32).min(cx1);
        let bottom = (top + raster.height() as i32).min(cy1);
        for y in top.max(cy0)..bottom {
            for x in left.max(cx0)..right {
                let mask_index = (y - top) as usize * raster.width() as usize + (x - left) as usize;
                let alpha = u32::from(raster.mask()[mask_index]);
                let index = (y as usize * canvas.width as usize + x as usize) * 4;
                for (channel, source) in color.iter().enumerate() {
                    let destination = u32::from(canvas.rgba[index + channel]);
                    canvas.rgba[index + channel] =
                        ((u32::from(*source) * alpha + destination * (255 - alpha) + 127) / 255)
                            as u8;
                }
            }
        }
    }
    Ok(())
}

struct Canvas {
    width: u32,
    height: u32,
    scale: f32,
    rgba: Vec<u8>,
}

impl Canvas {
    fn edges(&self, rect: TextRect) -> [i32; 4] {
        [
            (rect.x0 * self.scale).round().clamp(0.0, self.width as f32) as i32,
            (rect.y0 * self.scale)
                .round()
                .clamp(0.0, self.height as f32) as i32,
            (rect.x1 * self.scale).round().clamp(0.0, self.width as f32) as i32,
            (rect.y1 * self.scale)
                .round()
                .clamp(0.0, self.height as f32) as i32,
        ]
    }

    fn fill(&mut self, rect: TextRect, color: [u8; 3]) {
        let [x0, y0, x1, y1] = self.edges(rect);
        for y in y0..y1 {
            for x in x0..x1 {
                let index = (y as usize * self.width as usize + x as usize) * 4;
                self.rgba[index..index + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
            }
        }
    }
}

fn contains(rect: TextRect, x: f32, y: f32) -> bool {
    x >= rect.x0 && x < rect.x1 && y >= rect.y0 && y < rect.y1
}

fn validate_point(x: f32, y: f32) -> Result<()> {
    if !x.is_finite() || !y.is_finite() {
        return Err(io::Error::other("conformance pointer coordinates must be finite").into());
    }
    Ok(())
}

fn inset(rect: TextRect, x: f32, y: f32) -> TextRect {
    TextRect {
        x0: rect.x0 + x,
        y0: rect.y0 + y,
        x1: rect.x1 - x,
        y1: rect.y1 - y,
    }
}

fn translate(rect: TextRect, origin: [f32; 2]) -> TextRect {
    TextRect {
        x0: rect.x0 + origin[0],
        y0: rect.y0 + origin[1],
        x1: rect.x1 + origin[0],
        y1: rect.y1 + origin[1],
    }
}

fn intersect(rect: TextRect, clip: TextRect) -> TextRect {
    let x0 = rect.x0.clamp(clip.x0, clip.x1);
    let y0 = rect.y0.clamp(clip.y0, clip.y1);
    TextRect {
        x0,
        y0,
        x1: rect.x1.clamp(x0, clip.x1),
        y1: rect.y1.clamp(y0, clip.y1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(panel: &mut ConformancePanel, input: Input) -> bool {
        panel.reduce(&panel.compose(), &input).unwrap()
    }

    fn click(panel: &mut ConformancePanel, x: f32, y: f32) -> bool {
        let presented = panel.compose();
        assert!(
            !panel
                .reduce(&presented, &Input::PointerDown { x, y })
                .unwrap()
        );
        panel
            .reduce(&presented, &Input::PointerUp { x, y })
            .unwrap()
    }

    #[test]
    fn one_presented_capture_admits_complete_gestures_but_not_another_popup_cycle() {
        let evidence = Rc::new(Evidence::default());
        let mut panel = ConformancePanel::new(Rc::clone(&evidence)).unwrap();
        let base = panel.compose();
        assert!(
            !panel
                .reduce(&base, &Input::PointerDown { x: 20.0, y: 80.0 })
                .unwrap()
        );
        assert_eq!(panel.focused(), Some(Control::Review));
        assert_eq!(panel.compose().pressed(), Some(Control::Review));
        assert_ne!(base.revision(), panel.compose().revision());
        assert_eq!(base.focused(), None);
        for (x, y) in [(380.0, 170.0), (20.0, 80.0)] {
            assert!(!panel.reduce(&base, &Input::PointerMove { x, y }).unwrap());
            assert_eq!(panel.compose().pressed(), Some(Control::Review));
        }
        assert!(
            !panel
                .reduce(&base, &Input::PointerUp { x: 20.0, y: 80.0 })
                .unwrap()
        );
        assert!(panel.popup_open());
        assert_eq!(panel.reviews(), 1);
        let opened = panel.compose();
        assert!(
            panel
                .reduce(&base, &Input::Activate(Control::Apply))
                .is_err()
        );
        assert_eq!(opened, panel.compose());

        assert!(!panel.reduce(&base, &Input::Blur).unwrap());
        assert!(panel.popup_open());
        assert_eq!(panel.focused(), None);
        let popup = panel.compose();
        assert!(
            !panel
                .reduce(&popup, &Input::PointerDown { x: 210.0, y: 50.0 })
                .unwrap()
        );
        assert_eq!(panel.focused(), Some(Control::Apply));
        assert!(
            panel
                .reduce(&popup, &Input::PointerUp { x: 210.0, y: 50.0 })
                .unwrap()
        );
        assert!(!panel.popup_open());
        let closed = panel.compose();
        assert!(
            panel
                .reduce(&popup, &Input::Activate(Control::Apply))
                .is_err()
        );
        assert_eq!(closed, panel.compose());

        assert!(
            !panel
                .reduce(&closed, &Input::Activate(Control::Review))
                .unwrap()
        );
        let reopened = panel.compose();
        assert_eq!(popup.popup_open(), reopened.popup_open());
        assert_ne!(popup.popup_generation(), reopened.popup_generation());
        assert!(
            panel
                .reduce(&popup, &Input::Activate(Control::Apply))
                .is_err()
        );
        assert_eq!(reopened, panel.compose());
        assert!(
            !panel
                .reduce(&reopened, &Input::PointerDown { x: 210.0, y: 50.0 })
                .unwrap()
        );
        assert!(
            panel
                .reduce(&reopened, &Input::PointerUp { x: 210.0, y: 50.0 })
                .unwrap()
        );
        assert!(evidence.finished.borrow().is_empty());
    }

    #[test]
    fn one_presented_capture_admits_focus_text_history_and_new_focus_lifetimes() {
        let evidence = Rc::new(Evidence::default());
        let mut panel = ConformancePanel::new(Rc::clone(&evidence)).unwrap();
        let presented = panel.compose();
        assert!(
            !panel
                .reduce(&presented, &Input::Focus(Control::Field))
                .unwrap()
        );
        let token = panel.input_token().unwrap();
        assert!(
            !panel
                .reduce(
                    &presented,
                    &Input::Text {
                        token,
                        edit: TextEdit::SelectAll,
                    }
                )
                .unwrap()
        );
        assert!(
            !panel
                .reduce(
                    &presented,
                    &Input::Text {
                        token,
                        edit: TextEdit::Preedit {
                            text: "preview".into(),
                            cursor: Some((7, 7))
                        },
                    }
                )
                .unwrap()
        );
        assert_eq!(panel.draft(), INITIAL);
        assert_eq!(panel.display_text(), "preview");
        assert!(
            !panel
                .reduce(
                    &presented,
                    &Input::Text {
                        token,
                        edit: TextEdit::Commit("committed".into()),
                    }
                )
                .unwrap()
        );
        assert_eq!(panel.draft(), "committed");
        assert!(
            !panel
                .reduce(
                    &presented,
                    &Input::Text {
                        token,
                        edit: TextEdit::Undo
                    }
                )
                .unwrap()
        );
        assert_eq!(panel.draft(), INITIAL);
        assert!(
            !panel
                .reduce(
                    &presented,
                    &Input::Text {
                        token,
                        edit: TextEdit::Redo
                    }
                )
                .unwrap()
        );
        assert_eq!(panel.draft(), "committed");
        assert_eq!(presented.display_text(), INITIAL);
        assert_eq!(presented.input_token(), None);

        assert!(!panel.reduce(&presented, &Input::Blur).unwrap());
        assert!(
            !panel
                .reduce(&presented, &Input::Focus(Control::Field))
                .unwrap()
        );
        let renewed = panel.input_token().unwrap();
        assert_ne!(token, renewed);
        let focused = panel.compose();
        assert!(
            panel
                .reduce(
                    &presented,
                    &Input::Text {
                        token,
                        edit: TextEdit::Commit("late".into()),
                    }
                )
                .is_err()
        );
        assert_eq!(focused, panel.compose());
        assert!(
            !panel
                .reduce(
                    &presented,
                    &Input::Text {
                        token: renewed,
                        edit: TextEdit::Insert("!".into()),
                    }
                )
                .unwrap()
        );
        assert_eq!(panel.draft(), "committed!");

        let mut other = ConformancePanel::new(Rc::clone(&evidence)).unwrap();
        let other_before = other.compose();
        assert!(
            other
                .reduce(&presented, &Input::Focus(Control::Field))
                .is_err()
        );
        assert!(other.reduce(&presented, &Input::Blur).is_err());
        assert_eq!(other_before, other.compose());
        assert!(evidence.finished.borrow().is_empty());
    }

    #[test]
    fn only_geometric_text_input_waits_for_matching_presented_layout() {
        let mut panel = ConformancePanel::new(Rc::new(Evidence::default())).unwrap();
        let presented = panel.compose();
        assert!(
            !panel
                .reduce(&presented, &Input::PointerDown { x: 20.0, y: 30.0 })
                .unwrap()
        );
        assert!(
            !panel
                .reduce(&presented, &Input::PointerUp { x: 20.0, y: 30.0 })
                .unwrap()
        );
        let token = panel.input_token().unwrap();
        for edit in [
            TextEdit::SelectAll,
            TextEdit::Commit("a long shaped caption ".repeat(10)),
        ] {
            assert!(
                !panel
                    .reduce(&presented, &Input::Text { token, edit })
                    .unwrap()
            );
        }
        let desired = panel.compose();
        assert_ne!(presented.field_text_origin(), desired.field_text_origin());
        assert!(
            panel
                .reduce(&presented, &Input::PointerDown { x: 30.0, y: 30.0 })
                .is_err()
        );
        assert!(
            panel
                .reduce(
                    &presented,
                    &Input::Text {
                        token,
                        edit: TextEdit::Point {
                            x: 10.0,
                            y: 10.0,
                            extend: false
                        },
                    }
                )
                .is_err()
        );
        assert_eq!(desired, panel.compose());

        let presented_text = desired;
        assert!(
            !panel
                .reduce(&presented_text, &Input::PointerDown { x: 30.0, y: 30.0 })
                .unwrap()
        );
        assert!(
            !panel
                .reduce(&presented_text, &Input::PointerUp { x: 30.0, y: 30.0 })
                .unwrap()
        );
        for edit in [
            TextEdit::SelectAll,
            TextEdit::Preedit {
                text: "preview".into(),
                cursor: None,
            },
        ] {
            assert!(
                !panel
                    .reduce(&presented_text, &Input::Text { token, edit })
                    .unwrap()
            );
        }
        let presented_preedit = panel.compose();
        assert!(
            panel
                .reduce(&presented_preedit, &Input::PointerDown { x: 30.0, y: 30.0 })
                .is_err()
        );
        assert_eq!(presented_preedit, panel.compose());
        assert!(panel.editor().is_composing());
        assert!(
            !panel
                .reduce(
                    &presented_preedit,
                    &Input::Text {
                        token,
                        edit: TextEdit::CancelComposition,
                    }
                )
                .unwrap()
        );

        assert!(
            !panel
                .reduce(&presented, &Input::PointerDown { x: 20.0, y: 80.0 })
                .unwrap()
        );
        assert!(
            !panel
                .reduce(&presented, &Input::PointerUp { x: 20.0, y: 80.0 })
                .unwrap()
        );
        assert!(panel.popup_open());
        assert_eq!(panel.reviews(), 1);
    }

    #[test]
    fn focus_lifetimes_composition_and_local_history_remain_consumer_owned() {
        let evidence = Rc::new(Evidence::default());
        let mut panel = ConformancePanel::new(Rc::clone(&evidence)).unwrap();
        let token = panel.focus_for_cpu().unwrap();
        panel.edit(token, &TextEdit::SelectAll).unwrap();
        panel
            .edit(
                token,
                &TextEdit::Preedit {
                    text: "preview".into(),
                    cursor: Some((7, 7)),
                },
            )
            .unwrap();
        let preedit = panel.compose();
        assert_eq!(panel.draft(), INITIAL);
        assert_eq!(preedit.display_text(), "preview");
        panel
            .edit(token, &TextEdit::Commit("caption".into()))
            .unwrap();
        panel.edit(token, &TextEdit::Undo).unwrap();
        assert_eq!(panel.draft(), INITIAL);
        panel.edit(token, &TextEdit::Redo).unwrap();
        assert_eq!(panel.draft(), "caption");
        assert_eq!(preedit.display_text(), "preview");
        assert!(!input(&mut panel, Input::Blur));
        assert!(
            panel
                .edit(token, &TextEdit::Insert("stale".into()))
                .is_err()
        );
        let renewed = panel.focus_for_cpu().unwrap();
        assert_ne!(token, renewed);
        assert!(panel.edit(token, &TextEdit::Commit("late".into())).is_err());
        let serial = panel.serial();
        let mut other = ConformancePanel::new(Rc::clone(&evidence)).unwrap();
        other.focus_for_cpu().unwrap();
        assert!(
            other
                .edit(renewed, &TextEdit::Commit("foreign".into()))
                .is_err()
        );
        drop(panel);
        assert_eq!(*evidence.dropped.borrow(), [serial]);
        assert!(evidence.finished.borrow().is_empty());
    }

    #[test]
    fn modal_blocks_underlay_outside_and_old_text_then_emits_one_apply_intent() {
        let evidence = Rc::new(Evidence::default());
        let mut panel = ConformancePanel::new(Rc::clone(&evidence)).unwrap();
        let token = panel.replace_for_cpu("caption").unwrap();
        panel.edit(token, &TextEdit::SelectAll).unwrap();
        panel
            .edit(
                token,
                &TextEdit::Preedit {
                    text: "uncommitted".into(),
                    cursor: None,
                },
            )
            .unwrap();
        let before = panel.compose();
        assert!(!click(&mut panel, 20.0, 80.0));
        assert!(panel.popup_open());
        assert_eq!(panel.focused(), Some(Control::Apply));
        assert_eq!(panel.display_text(), "caption");
        assert_eq!(panel.reviews(), 1);
        assert!(panel.input_token().is_none());
        assert!(
            panel
                .reduce(&before, &Input::Activate(Control::Review))
                .is_err()
        );
        assert!(panel.edit(token, &TextEdit::Commit("late".into())).is_err());
        for (x, y) in [(20.0, 20.0), (20.0, 80.0), (380.0, 170.0), (192.0, 20.0)] {
            assert_eq!(panel.compose().hit_test(x, y), None);
            assert!(!click(&mut panel, x, y));
            assert!(panel.popup_open());
            assert_eq!(panel.reviews(), 1);
            assert!(panel.input_token().is_none());
        }
        assert!(!input(&mut panel, Input::Activate(Control::Review)));
        assert!(click(&mut panel, 210.0, 50.0));
        assert!(!panel.popup_open());
        assert!(!input(&mut panel, Input::PointerUp { x: 210.0, y: 50.0 }));
        assert!(!input(&mut panel, Input::Activate(Control::Apply)));
        assert_eq!(panel.draft(), "caption");
        assert!(evidence.finished.borrow().is_empty());
    }

    #[test]
    fn refused_input_preserves_capture_selection_composition_and_history() {
        let mut panel = ConformancePanel::new(Rc::new(Evidence::default())).unwrap();
        let token = panel.replace_for_cpu("retained").unwrap();
        panel.edit(token, &TextEdit::SelectAll).unwrap();
        panel
            .edit(
                token,
                &TextEdit::Preedit {
                    text: "display".into(),
                    cursor: Some((7, 7)),
                },
            )
            .unwrap();
        let snapshot = panel.compose();
        let selection = panel.editor().selection();
        assert!(
            panel
                .edit(token, &TextEdit::Commit("x".repeat(MAX_DRAFT_BYTES + 1)))
                .is_err()
        );
        assert_eq!(snapshot, panel.compose());
        assert_eq!(selection, panel.editor().selection());
        assert!(panel.editor().is_composing());
        assert!(
            panel
                .reduce(
                    &snapshot,
                    &Input::PointerDown {
                        x: f32::NAN,
                        y: 20.0
                    }
                )
                .is_err()
        );
        assert_eq!(snapshot, panel.compose());
        panel.edit(token, &TextEdit::CancelComposition).unwrap();
        panel.edit(token, &TextEdit::Undo).unwrap();
        assert_eq!(panel.draft(), INITIAL);
        panel.edit(token, &TextEdit::Redo).unwrap();
        assert_eq!(panel.draft(), "retained");
    }

    #[test]
    fn captured_text_and_popup_produce_clipped_repeatable_real_glyph_pixels() {
        let evidence = Rc::new(Evidence::default());
        let mut panel = ConformancePanel::new(Rc::clone(&evidence)).unwrap();
        let mut painter = SoftwarePainter::new().unwrap();
        assert_eq!(painter.font_face_id(), bundled_font_face().unwrap().id());
        let old = panel.compose();
        let old_pixels = painter.paint(&old, 1.0).unwrap();
        let token = panel.replace_for_cpu("").unwrap();
        input(&mut panel, Input::Blur);
        let empty = panel.compose();
        let empty_pixels = painter.paint(&empty, 1.0).unwrap();
        assert_ne!(old_pixels, empty_pixels);
        let clip = old.field_text_clip();
        for (index, (a, b)) in old_pixels
            .chunks_exact(4)
            .zip(empty_pixels.chunks_exact(4))
            .enumerate()
        {
            if a != b {
                assert!(contains(clip, (index % 400) as f32, (index / 400) as f32));
            }
        }
        assert!(
            panel
                .edit(token, &TextEdit::Commit("stale".into()))
                .is_err()
        );
        panel.replace_for_cpu("visible popup").unwrap();
        input(&mut panel, Input::Activate(Control::Review));
        let popup = panel.compose();
        assert!(popup.popup_open());
        assert_eq!(popup.display_text(), "visible popup");
        for scale in [1.0, 1.25, 2.0] {
            let pixels = painter.paint(&popup, scale).unwrap();
            let [width, height] = SoftwarePainter::pixel_size(scale).unwrap();
            assert_eq!(pixels.len(), width as usize * height as usize * 4);
            assert!(pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
            assert_eq!(pixels, painter.paint(&popup, scale).unwrap());
        }
        let pixels = painter.paint(&popup, 1.0).unwrap();
        for rect in [
            inset(popup.geometry().apply, 6.0, 4.0),
            TextRect {
                x0: 198.0,
                y0: 88.0,
                x1: 362.0,
                y1: 144.0,
            },
        ] {
            assert!(
                (rect.y0 as usize..rect.y1 as usize).any(|y| {
                    (rect.x0 as usize..rect.x1 as usize).any(|x| {
                        let index = (y * 400 + x) * 4;
                        pixels[index] > 180 && pixels[index + 1] > 180 && pixels[index + 2] > 180
                    })
                }),
                "actual label/draft glyph ink must be present",
            );
        }
        assert_eq!(old_pixels, painter.paint(&old, 1.0).unwrap());
        assert_eq!(popup, panel.compose());
        assert!(evidence.finished.borrow().is_empty());
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, MAX_SCALE + 0.1] {
            assert!(painter.paint(&popup, scale).is_err());
        }
    }
}
