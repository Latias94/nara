use std::{collections::VecDeque, num::NonZeroUsize, ops::Range};

use icu_segmenter::GraphemeClusterSegmenter;
use parley::{BoundingBox, editing::PlainEditor};
use thiserror::Error;

use crate::{FontFaceId, TextEngine, TextLayout, TextLayoutError, TextLayoutRequest, TextLimits};

const MAX_HISTORY_ENTRIES: usize = 32;

/// A rectangle in the editor's local logical coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl TextRect {
    fn from_parley(rect: BoundingBox) -> Result<Self, TextEditError> {
        let rect = Self {
            x0: rect.x0 as f32,
            y0: rect.y0 as f32,
            x1: rect.x1 as f32,
            y1: rect.y1 as f32,
        };
        if ![rect.x0, rect.y0, rect.x1, rect.y1]
            .into_iter()
            .all(f32::is_finite)
            || rect.x0 > rect.x1
            || rect.y0 > rect.y1
        {
            return Err(TextLayoutError::NonFiniteResult.into());
        }
        Ok(rect)
    }
}

/// Selection endpoints as UTF-8 byte offsets into [`TextEditor::display_text`].
///
/// The anchor is retained when extending a selection. The underlying editor also retains cursor
/// affinities, so equal byte offsets do not discard the visual side of a bidi boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: usize,
    pub focus: usize,
}

impl TextSelection {
    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }

    #[must_use]
    pub const fn is_collapsed(self) -> bool {
        self.anchor == self.focus
    }
}

/// One bounded local editing action. Points use local logical layout coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum TextEdit {
    Insert(String),
    Backspace,
    Delete,
    MoveLeft {
        extend: bool,
        word: bool,
    },
    MoveRight {
        extend: bool,
        word: bool,
    },
    Home {
        extend: bool,
    },
    End {
        extend: bool,
    },
    SelectAll,
    Point {
        x: f32,
        y: f32,
        extend: bool,
    },
    /// The cursor endpoints are UTF-8 byte offsets relative to `text`.
    /// An empty preedit cancels composition. `None` hides the caret.
    Preedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// Replace the selection captured before composition, or the current selection if not composing.
    Commit(String),
    CancelComposition,
    Undo,
    Redo,
}

/// Changes adopted by one successful operation. Rejected operations publish no changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextEditOutcome {
    pub text_changed: bool,
    pub display_changed: bool,
    pub selection_changed: bool,
    pub composition_changed: bool,
}

impl TextEditOutcome {
    #[must_use]
    pub const fn changed(self) -> bool {
        self.text_changed
            || self.display_changed
            || self.selection_changed
            || self.composition_changed
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TextEditError {
    #[error("text editing requires the original ordered font set and resource limits")]
    EngineMismatch,
    #[error(transparent)]
    Layout(#[from] TextLayoutError),
    #[error("single-line text contains unsupported scalar {scalar:?} at byte {byte_index}")]
    InvalidSingleLineText { byte_index: usize, scalar: char },
    #[error("text hit-test point ({x}, {y}) must be finite")]
    InvalidPoint { x: f32, y: f32 },
    #[error("composition cursor byte {offset} is not a UTF-8 boundary in {text_bytes} bytes")]
    InvalidCompositionCursor { offset: usize, text_bytes: usize },
}

#[derive(Clone, Debug, PartialEq)]
struct EditorGeometry {
    selection: TextSelection,
    caret: Option<TextRect>,
    selection_rects: Vec<TextRect>,
    ime_area: TextRect,
}

impl EditorGeometry {
    fn new(editor: &PlainEditor<()>) -> Result<Self, TextEditError> {
        let selection = editor.raw_selection();
        let caret = editor
            .cursor_geometry(1.0)
            .map(TextRect::from_parley)
            .transpose()?;
        let selection_rects = editor
            .selection_geometry()
            .into_iter()
            .map(|(rect, _)| TextRect::from_parley(rect))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            selection: TextSelection {
                anchor: selection.anchor().index(),
                focus: selection.focus().index(),
            },
            caret,
            selection_rects,
            ime_area: TextRect::from_parley(editor.ime_cursor_area())?,
        })
    }
}

#[derive(Clone, Copy)]
enum HistoryChange {
    None,
    Record,
    Undo,
    Redo,
}

/// A bounded single-line draft with Parley-owned cursor, selection and composition behavior.
/// ICU grapheme boundaries constrain navigation, hit testing and deletion across shaped ligatures.
///
/// Text is never applied to a source document. Undo and Redo retain at most 32 prior buffers in
/// total, each admitted under the text engine's byte limit. Preedit retains the exact original
/// editor, including its selection and cursor affinities, without recording a history entry.
/// Commit replaces that original selection once and records at most one local edit.
/// The font set, font order, resource limits and logical size are fixed at construction. Another
/// engine can drive the editor only when its complete font set and limits match exactly.
///
/// Other editing and navigation actions first cancel active composition and then operate on the
/// restored committed state. A caller ending a focus lifetime should send `CancelComposition`.
/// Native input lifetimes and document Apply are responsibilities of the caller.
///
/// Every action is evaluated on a candidate. Text, geometry, composition and history are adopted
/// together only after the engine accepts the complete layout. Clones own independent drafts.
#[derive(Clone, Debug)]
pub struct TextEditor {
    font_ids: Vec<FontFaceId>,
    limits: TextLimits,
    editor: PlainEditor<()>,
    composition_origin: Option<PlainEditor<()>>,
    undo: VecDeque<PlainEditor<()>>,
    redo: Vec<PlainEditor<()>>,
    layout: TextLayout,
    geometry: EditorGeometry,
}

impl TextEditor {
    pub fn new(engine: &mut TextEngine, text: &str, size: f32) -> Result<Self, TextEditError> {
        validate_text(engine, text)?;
        let request = TextLayoutRequest::new(text, size)?;
        engine.validate_request(&request)?;

        let mut editor = PlainEditor::new(size);
        editor.set_text(text);
        engine.configure_editor(&mut editor);
        editor.set_quantize(false);
        let (layout, geometry) = project_editor(engine, &mut editor)?;
        Ok(Self {
            font_ids: engine.font_ids().collect(),
            limits: engine.limits,
            editor,
            composition_origin: None,
            undo: VecDeque::with_capacity(MAX_HISTORY_ENTRIES),
            redo: Vec::with_capacity(MAX_HISTORY_ENTRIES),
            layout,
            geometry,
        })
    }

    pub fn edit(
        &mut self,
        engine: &mut TextEngine,
        action: &TextEdit,
    ) -> Result<TextEditOutcome, TextEditError> {
        if self.limits != engine.limits || !engine.font_ids().eq(self.font_ids.iter().copied()) {
            return Err(TextEditError::EngineMismatch);
        }
        self.validate_action(engine, action)?;

        let composing = matches!(action, TextEdit::Preedit { text, .. } if !text.is_empty());
        let committed = self.committed_editor();
        let (mut candidate, history_change) = match action {
            TextEdit::Undo => match self.undo.back() {
                Some(previous) => (previous.clone(), HistoryChange::Undo),
                None => (committed.clone(), HistoryChange::None),
            },
            TextEdit::Redo => match self.redo.last() {
                Some(next) => (next.clone(), HistoryChange::Redo),
                None => (committed.clone(), HistoryChange::None),
            },
            _ if composing => (self.editor.clone(), HistoryChange::None),
            TextEdit::Insert(_) | TextEdit::Commit(_) | TextEdit::Backspace | TextEdit::Delete => {
                (committed.clone(), HistoryChange::Record)
            }
            _ => (committed.clone(), HistoryChange::None),
        };

        validate_text(engine, candidate.raw_text())?;
        engine.configure_editor(&mut candidate);
        candidate.refresh_layout(&mut engine.font_context, &mut engine.layout_context);
        apply_action(engine, &mut candidate, action);
        let (layout, geometry) = project_editor(engine, &mut candidate)?;

        let outcome = TextEditOutcome {
            text_changed: !composing && self.text() != candidate.raw_text(),
            display_changed: self.display_text() != candidate.raw_text() || self.layout != layout,
            selection_changed: self.editor.raw_selection().anchor()
                != candidate.raw_selection().anchor()
                || self.editor.raw_selection().focus() != candidate.raw_selection().focus()
                || self.geometry.caret != geometry.caret
                || self.geometry.selection_rects != geometry.selection_rects,
            composition_changed: self.editor.raw_compose() != candidate.raw_compose()
                || (composing && self.display_text() != candidate.raw_text()),
        };

        let previous = std::mem::replace(&mut self.editor, candidate);
        self.layout = layout;
        self.geometry = geometry;
        if composing {
            if self.composition_origin.is_none() {
                self.composition_origin = Some(previous);
            }
        } else {
            let previous = self.composition_origin.take().unwrap_or(previous);
            match history_change {
                HistoryChange::Record if outcome.text_changed => {
                    self.redo.clear();
                    if self.undo.len() == MAX_HISTORY_ENTRIES {
                        let _ = self.undo.pop_front();
                    }
                    self.undo.push_back(previous);
                }
                HistoryChange::Undo => {
                    let _ = self.undo.pop_back();
                    self.redo.push(previous);
                }
                HistoryChange::Redo => {
                    let _ = self.redo.pop();
                    self.undo.push_back(previous);
                }
                HistoryChange::None | HistoryChange::Record => {}
            }
        }
        Ok(outcome)
    }

    #[must_use]
    pub fn logical_size(&self) -> f32 {
        self.editor.get_font_size()
    }

    /// The complete committed buffer, including the original selection during preedit.
    #[must_use]
    pub fn text(&self) -> &str {
        self.committed_editor().raw_text()
    }

    #[must_use]
    pub fn display_text(&self) -> &str {
        self.editor.raw_text()
    }

    /// The last successfully adopted immutable layout. No engine access or shaping is required.
    #[must_use]
    pub const fn layout(&self) -> &TextLayout {
        &self.layout
    }

    /// The same immutable layout returned by [`Self::layout`].
    #[must_use]
    pub const fn snapshot(&self) -> &TextLayout {
        &self.layout
    }

    #[must_use]
    pub const fn selection(&self) -> TextSelection {
        self.geometry.selection
    }

    #[must_use]
    pub fn selection_bytes(&self) -> Range<usize> {
        self.selection().range()
    }

    #[must_use]
    pub const fn caret(&self) -> Option<TextRect> {
        self.geometry.caret
    }

    #[must_use]
    pub fn selection_rects(&self) -> &[TextRect] {
        &self.geometry.selection_rects
    }

    /// Layout-derived candidate exclusion area; native placement is the caller's responsibility.
    #[must_use]
    pub const fn ime_area(&self) -> TextRect {
        self.geometry.ime_area
    }

    #[must_use]
    pub fn composition_range(&self) -> Option<Range<usize>> {
        self.editor.raw_compose().clone()
    }

    #[must_use]
    pub fn is_composing(&self) -> bool {
        self.composition_origin.is_some()
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn committed_editor(&self) -> &PlainEditor<()> {
        self.composition_origin.as_ref().unwrap_or(&self.editor)
    }

    fn validate_action(&self, engine: &TextEngine, action: &TextEdit) -> Result<(), TextEditError> {
        match action {
            TextEdit::Insert(text) | TextEdit::Commit(text) => {
                validate_text(engine, text)?;
                let committed = self.committed_editor();
                validate_replacement(
                    engine,
                    committed.raw_text(),
                    committed.raw_selection().text_range(),
                    text,
                )?;
            }
            TextEdit::Preedit { text, cursor } => {
                validate_text(engine, text)?;
                if let Some((anchor, focus)) = cursor {
                    for offset in [*anchor, *focus] {
                        if !text.is_char_boundary(offset) {
                            return Err(TextEditError::InvalidCompositionCursor {
                                offset,
                                text_bytes: text.len(),
                            });
                        }
                    }
                }
                if !text.is_empty() {
                    let replaced = self
                        .editor
                        .raw_compose()
                        .clone()
                        .unwrap_or_else(|| self.editor.raw_selection().text_range());
                    validate_replacement(engine, self.display_text(), replaced, text)?;
                }
            }
            TextEdit::Backspace | TextEdit::Delete => {
                let committed = self.committed_editor();
                let mut replaced = committed.raw_selection().text_range();
                if replaced.is_empty() {
                    let boundaries = GraphemeClusterSegmenter::new()
                        .segment_str(committed.raw_text())
                        .collect::<Vec<_>>();
                    if matches!(action, TextEdit::Backspace) {
                        replaced.start = boundaries
                            .iter()
                            .rev()
                            .copied()
                            .find(|&boundary| boundary < replaced.start)
                            .unwrap_or(replaced.start);
                    } else {
                        replaced.end = boundaries
                            .iter()
                            .copied()
                            .find(|&boundary| boundary > replaced.end)
                            .unwrap_or(replaced.end);
                    }
                }
                validate_replacement(engine, committed.raw_text(), replaced, "")?;
            }
            TextEdit::Point { x, y, .. } if !x.is_finite() || !y.is_finite() => {
                return Err(TextEditError::InvalidPoint { x: *x, y: *y });
            }
            _ => {}
        }
        Ok(())
    }
}

fn validate_text(engine: &TextEngine, text: &str) -> Result<(), TextEditError> {
    if text.len() > engine.limits.max_text_bytes {
        return Err(TextLayoutError::TextBytesExceeded {
            actual: text.len(),
            maximum: engine.limits.max_text_bytes,
        }
        .into());
    }
    for (byte_index, scalar) in text.char_indices() {
        if scalar.is_control() || matches!(scalar, '\u{2028}' | '\u{2029}') {
            return Err(TextEditError::InvalidSingleLineText { byte_index, scalar });
        }
    }
    crate::layout::validate_graphemes(text)?;
    Ok(())
}

fn validate_replacement(
    engine: &TextEngine,
    text: &str,
    replaced: Range<usize>,
    inserted: &str,
) -> Result<(), TextEditError> {
    let actual = text.len() - replaced.len() + inserted.len();
    if actual > engine.limits.max_text_bytes {
        return Err(TextLayoutError::TextBytesExceeded {
            actual,
            maximum: engine.limits.max_text_bytes,
        }
        .into());
    }
    let mut candidate = String::with_capacity(actual);
    candidate.push_str(&text[..replaced.start]);
    candidate.push_str(inserted);
    candidate.push_str(&text[replaced.end..]);
    crate::layout::validate_graphemes(&candidate)?;
    Ok(())
}

fn apply_action(engine: &mut TextEngine, editor: &mut PlainEditor<()>, action: &TextEdit) {
    let boundaries = GraphemeClusterSegmenter::new()
        .segment_str(editor.raw_text())
        .collect::<Vec<_>>();
    let selection = editor.raw_selection();
    let collapsed = selection.is_collapsed();
    let focus = selection.focus().index();
    let mut driver = editor.driver(&mut engine.font_context, &mut engine.layout_context);
    match action {
        TextEdit::Insert(text) | TextEdit::Commit(text) if text.is_empty() && collapsed => {}
        TextEdit::Insert(text) | TextEdit::Commit(text) => {
            driver.insert_or_replace_selection(text);
        }
        TextEdit::Backspace if collapsed => {
            if let Some(previous) = boundaries.iter().rev().find(|&&boundary| boundary < focus) {
                driver.delete_bytes_before_selection(NonZeroUsize::new(focus - previous).unwrap());
            }
        }
        TextEdit::Delete if collapsed => {
            if let Some(next) = boundaries.iter().find(|&&boundary| boundary > focus) {
                driver.delete_bytes_after_selection(NonZeroUsize::new(next - focus).unwrap());
            }
        }
        TextEdit::Backspace | TextEdit::Delete => driver.delete_selection(),
        TextEdit::MoveLeft { extend, word } => match (*extend, *word) {
            (false, false) => driver.move_left(),
            (false, true) => driver.move_word_left(),
            (true, false) => driver.select_left(),
            (true, true) => driver.select_word_left(),
        },
        TextEdit::MoveRight { extend, word } => match (*extend, *word) {
            (false, false) => driver.move_right(),
            (false, true) => driver.move_word_right(),
            (true, false) => driver.select_right(),
            (true, true) => driver.select_word_right(),
        },
        TextEdit::Home { extend: false } => driver.move_to_line_start(),
        TextEdit::Home { extend: true } => driver.select_to_line_start(),
        TextEdit::End { extend: false } => driver.move_to_line_end(),
        TextEdit::End { extend: true } => driver.select_to_line_end(),
        TextEdit::SelectAll => driver.select_all(),
        TextEdit::Point {
            x,
            y,
            extend: false,
        } => driver.move_to_point(*x, *y),
        TextEdit::Point { x, y, extend: true } => driver.extend_selection_to_point(*x, *y),
        TextEdit::Preedit { text, cursor } if !text.is_empty() => driver.set_compose(text, *cursor),
        TextEdit::Preedit { .. }
        | TextEdit::CancelComposition
        | TextEdit::Undo
        | TextEdit::Redo => {}
    }
    match action {
        TextEdit::MoveLeft { extend, .. } => {
            move_to_grapheme_boundary(engine, editor, &boundaries, false, *extend)
        }
        TextEdit::MoveRight { extend, .. } => {
            move_to_grapheme_boundary(engine, editor, &boundaries, true, *extend)
        }
        TextEdit::Point { x, extend, .. }
            if boundaries
                .binary_search(&editor.raw_selection().focus().index())
                .is_err() =>
        {
            let mut left = editor.clone();
            move_to_grapheme_boundary(engine, &mut left, &boundaries, false, *extend);
            let mut right = editor.clone();
            move_to_grapheme_boundary(engine, &mut right, &boundaries, true, *extend);
            let distance = |candidate: &PlainEditor<()>| {
                candidate
                    .cursor_geometry(1.0)
                    .map_or(f64::INFINITY, |rect| (rect.x0 - f64::from(*x)).abs())
            };
            *editor = if distance(&left) <= distance(&right) {
                left
            } else {
                right
            };
        }
        TextEdit::Insert(_) | TextEdit::Commit(_) | TextEdit::Backspace | TextEdit::Delete => {
            let focus = editor.raw_selection().focus().index();
            let boundaries = GraphemeClusterSegmenter::new()
                .segment_str(editor.raw_text())
                .collect::<Vec<_>>();
            if boundaries.binary_search(&focus).is_err() {
                let next = boundaries
                    .iter()
                    .copied()
                    .find(|&boundary| boundary > focus)
                    .unwrap_or(editor.raw_text().len());
                let mut left = editor.clone();
                move_to_grapheme_boundary(engine, &mut left, &boundaries, false, false);
                if left.raw_selection().focus().index() == next {
                    *editor = left;
                } else {
                    move_to_grapheme_boundary(engine, editor, &boundaries, true, false);
                }
            }
        }
        _ => {}
    }
}

fn move_to_grapheme_boundary(
    engine: &mut TextEngine,
    editor: &mut PlainEditor<()>,
    boundaries: &[usize],
    right: bool,
    extend: bool,
) {
    for _ in 0..=editor.raw_text().len() {
        if boundaries
            .binary_search(&editor.raw_selection().focus().index())
            .is_ok()
        {
            break;
        }
        let mut driver = editor.driver(&mut engine.font_context, &mut engine.layout_context);
        match (right, extend) {
            (false, false) => driver.move_left(),
            (false, true) => driver.select_left(),
            (true, false) => driver.move_right(),
            (true, true) => driver.select_right(),
        }
    }
}

fn project_editor(
    engine: &mut TextEngine,
    editor: &mut PlainEditor<()>,
) -> Result<(TextLayout, EditorGeometry), TextEditError> {
    let request = TextLayoutRequest::new(editor.raw_text(), editor.get_font_size())?;
    engine.validate_request(&request)?;
    let parley_layout = editor.layout(&mut engine.font_context, &mut engine.layout_context);
    let layout = engine.project_layout(parley_layout, &request)?;
    let geometry = EditorGeometry::new(editor)?;
    Ok((layout, geometry))
}
