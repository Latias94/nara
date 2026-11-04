use std::sync::atomic::{AtomicU64, Ordering};

use nara_text::{TextEdit, TextEditError, TextEditOutcome, TextEditor, TextEngine, TextRect};
use nara_ui_kernel::{
    UiBorder, UiBrush, UiColor, UiColorRole, UiDispatchBatch, UiInteraction, UiKey, UiKeyError,
    UiLocalKey, UiNode, UiOverflow, UiPreparedText, UiRect, UiRootKey, UiRoutePhase,
    UiRoutedEventKind, UiSemanticRole, UiSemantics, UiStyle, UiStyleError,
};
use thiserror::Error;

static NEXT_FIELD: AtomicU64 = AtomicU64::new(1);
const TEXT_INSET: f32 = 6.0;

/// One field incarnation and focus lifetime, captured when a Host admits text input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiTextInputToken {
    field: u64,
    focus: u64,
}

/// A single-line field buffer and its local interaction state, without document authority.
///
/// Keep this owner across compatible redraws. Recreating it gives a new live key even for the
/// same semantic property. The caller separately owns that property's identity and Apply action.
pub struct UiTextField {
    key: UiKey,
    text_key: UiLocalKey,
    field: u64,
    label: String,
    editor: TextEditor,
    focused: bool,
    focus: Option<UiTextInputToken>,
    focus_generation: u64,
    enabled: bool,
}

impl UiTextField {
    pub fn new(
        root: UiRootKey,
        label: impl Into<String>,
        value: &str,
        logical_size: f32,
        engine: &mut TextEngine,
    ) -> Result<Self, UiTextFieldError> {
        let label = label.into();
        UiSemantics::new(UiSemanticRole::TextInput, label.clone())?;
        let editor = TextEditor::new(engine, value, logical_size)?;
        let field = NEXT_FIELD
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| UiTextFieldError::IdentityExhausted)?;
        let key = UiKey::scoped(
            root.clone(),
            UiLocalKey::runtime("nara.text-field", field.to_string())?,
        )?;
        let text_key = UiLocalKey::runtime("nara.text-field.content", field.to_string())?;
        UiKey::scoped(root, text_key.clone())?;
        Ok(Self {
            key,
            text_key,
            field,
            label,
            editor,
            focused: false,
            focus: None,
            focus_generation: 0,
            enabled: true,
        })
    }

    #[must_use]
    pub const fn key(&self) -> &UiKey {
        &self.key
    }

    #[must_use]
    pub const fn editor(&self) -> &TextEditor {
        &self.editor
    }

    #[must_use]
    pub const fn input_token(&self) -> Option<UiTextInputToken> {
        self.focus
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Reconfigures the local editor while retaining this field, focus token and enabled state.
    ///
    /// All editing layouts must fit the original limits under the new fonts. No document edit or
    /// focus transition is generated. Raster/target-scale admission remains separate. The caller
    /// updates its compiler/planner fonts and publishes new geometry before admitting its input.
    pub fn reconfigure_fonts(
        &mut self,
        engine: &mut TextEngine,
    ) -> Result<TextEditOutcome, UiTextFieldError> {
        Ok(self.editor.reconfigure_fonts(engine)?)
    }

    /// Opens one focus lifetime after the Host has established focus for this live field.
    pub fn focus(&mut self) -> Result<UiTextInputToken, UiTextFieldError> {
        if !self.enabled {
            return Err(UiTextFieldError::Disabled);
        }
        if let Some(token) = self.focus {
            return Ok(token);
        }
        self.focus_generation = self
            .focus_generation
            .checked_add(1)
            .ok_or(UiTextFieldError::IdentityExhausted)?;
        let token = UiTextInputToken {
            field: self.field,
            focus: self.focus_generation,
        };
        self.focused = true;
        self.focus = Some(token);
        Ok(token)
    }

    /// Revokes the input lifetime first, then cancels composition without publishing the draft.
    pub fn blur(&mut self, engine: &mut TextEngine) -> Result<(), UiTextFieldError> {
        self.focused = false;
        self.focus = None;
        self.editor.edit(engine, &TextEdit::CancelComposition)?;
        Ok(())
    }

    pub fn set_enabled(
        &mut self,
        enabled: bool,
        engine: &mut TextEngine,
    ) -> Result<(), UiTextFieldError> {
        self.enabled = enabled;
        if !enabled {
            self.focus = None;
            self.editor.edit(engine, &TextEdit::CancelComposition)?;
        } else if self.focused {
            self.focus()?;
        }
        Ok(())
    }

    /// Consumes already admitted focus facts; geometric/platform admission stays with the Host.
    pub fn reduce_focus(
        &mut self,
        dispatch: &UiDispatchBatch,
        engine: &mut TextEngine,
    ) -> Result<(), UiTextFieldError> {
        for event in dispatch.events() {
            if event.phase() != UiRoutePhase::Target || event.target() != &self.key {
                continue;
            }
            if let UiRoutedEventKind::FocusChanged { focused } = event.kind() {
                if *focused {
                    self.focused = true;
                    if self.enabled {
                        self.focus()?;
                    }
                } else {
                    self.blur(engine)?;
                }
            }
        }
        Ok(())
    }

    /// Applies one event only to the exact live field and focus that admitted it.
    pub fn edit(
        &mut self,
        token: UiTextInputToken,
        action: &TextEdit,
        engine: &mut TextEngine,
    ) -> Result<TextEditOutcome, UiTextFieldError> {
        if self.focus != Some(token) {
            return Err(UiTextFieldError::StaleInput);
        }
        if !self.enabled {
            return Err(UiTextFieldError::Disabled);
        }
        Ok(self.editor.edit(engine, action)?)
    }

    pub fn point(
        &mut self,
        token: UiTextInputToken,
        position: nara_ui_kernel::UiPoint,
        viewport_width: f32,
        extend: bool,
        engine: &mut TextEngine,
    ) -> Result<TextEditOutcome, UiTextFieldError> {
        let origin = self.text_origin(viewport_width)?;
        self.edit(
            token,
            &TextEdit::Point {
                x: position.x() - origin,
                y: position.y() - TEXT_INSET,
                extend,
            },
            engine,
        )
    }

    pub fn ime_area(&self, viewport_width: f32) -> Result<UiRect, UiTextFieldError> {
        let offset = self.text_origin(viewport_width)?;
        let area = self.editor.ime_area();
        if let Some(rect) = visible_rect(area, offset, viewport_width) {
            return Ok(rect);
        }
        UiRect::from_origin_size(
            (area.x0 + offset).clamp(TEXT_INSET, viewport_width - TEXT_INSET),
            area.y0 + TEXT_INSET,
            1.0,
            (area.y1 - area.y0).max(1.0),
        )
        .ok_or(UiTextFieldError::InvalidViewport)
    }

    /// Builds the field's clipped contents within the caller's resolved logical width.
    pub fn authored_node(
        &self,
        root: &UiRootKey,
        style: UiStyle,
        viewport_width: f32,
    ) -> Result<UiNode, UiTextFieldError> {
        if self.key.root_key() != root {
            return Err(UiTextFieldError::RootMismatch);
        }
        let inner_width = viewport_width - 2.0 * TEXT_INSET;
        if !inner_width.is_finite() || inner_width <= 0.0 {
            return Err(UiTextFieldError::InvalidViewport);
        }
        let focused = self.focus.is_some();
        let offset = self.text_origin(viewport_width)?;
        let mut children = Vec::new();
        if focused {
            for selection in self.editor.selection_rects() {
                if let Some(rect) = visible_rect(*selection, offset, viewport_width) {
                    children.push(UiNode::new(rect_style(rect)).with_visual(
                        nara_ui_kernel::UiVisual::solid(UiColor::rgba(0.2, 0.45, 0.9, 0.35)),
                    ));
                }
            }
        }
        children.push(
            UiNode::new(UiStyle::absolute(
                offset - TEXT_INSET,
                0.0,
                self.editor.layout().width().max(1.0),
                self.editor
                    .layout()
                    .height()
                    .max(self.editor.logical_size()),
            ))
            .with_key(self.text_key.clone())
            .with_prepared_text(
                UiPreparedText::new(self.editor.paint_snapshot())
                    .with_semantic_text(self.editor.text()),
            ),
        );
        if focused
            && let Some(caret) = self
                .editor
                .caret()
                .and_then(|caret| visible_rect(caret, offset, viewport_width))
        {
            children.push(
                UiNode::new(rect_style(caret))
                    .with_visual(nara_ui_kernel::UiVisual::role(UiColorRole::Focus)),
            );
        }
        let border = UiBorder::new(
            1.0,
            UiBrush::Role(if focused {
                UiColorRole::Focus
            } else {
                UiColorRole::SecondaryText
            }),
        )?;
        Ok(UiNode::new(style.with_overflow(UiOverflow::Clip))
            .with_key(
                self.key
                    .local_key()
                    .expect("a field always has a local key")
                    .clone(),
            )
            .with_visual(
                nara_ui_kernel::UiVisual::role(if self.enabled {
                    UiColorRole::Panel
                } else {
                    UiColorRole::Disabled
                })
                .with_border(border),
            )
            .with_interaction(UiInteraction::focus_target().with_enabled(self.enabled))
            .with_semantics(
                UiSemantics::new(UiSemanticRole::TextInput, self.label.clone())?
                    .with_value(self.editor.text())?,
            )
            .with_child(
                UiNode::new(
                    UiStyle::absolute(
                        TEXT_INSET,
                        TEXT_INSET,
                        inner_width,
                        self.editor
                            .layout()
                            .height()
                            .max(self.editor.logical_size()),
                    )
                    .with_overflow(UiOverflow::Clip),
                )
                .with_children(children),
            ))
    }

    fn text_origin(&self, viewport_width: f32) -> Result<f32, UiTextFieldError> {
        let inner_width = viewport_width - 2.0 * TEXT_INSET;
        if !inner_width.is_finite() || inner_width <= 0.0 {
            return Err(UiTextFieldError::InvalidViewport);
        }
        let scroll = if self.focus.is_some() {
            self.editor
                .caret()
                .map_or(0.0, |caret| (caret.x1 - inner_width).max(0.0))
        } else {
            0.0
        };
        Ok(TEXT_INSET - scroll)
    }
}

fn visible_rect(rect: TextRect, offset: f32, viewport_width: f32) -> Option<UiRect> {
    let rect = UiRect::from_origin_size(
        rect.x0 + offset,
        rect.y0 + TEXT_INSET,
        rect.x1 - rect.x0,
        rect.y1 - rect.y0,
    )?;
    rect.intersect(UiRect::from_origin_size(
        TEXT_INSET,
        0.0,
        viewport_width - 2.0 * TEXT_INSET,
        16384.0,
    )?)
}

fn rect_style(rect: UiRect) -> UiStyle {
    UiStyle::absolute(
        rect.origin().x() - TEXT_INSET,
        rect.origin().y() - TEXT_INSET,
        rect.size().width(),
        rect.size().height(),
    )
}

#[derive(Debug, Error)]
pub enum UiTextFieldError {
    #[error(transparent)]
    Text(#[from] TextEditError),
    #[error(transparent)]
    Key(#[from] UiKeyError),
    #[error(transparent)]
    Style(#[from] UiStyleError),
    #[error(transparent)]
    Authoring(#[from] nara_ui_kernel::UiAuthoringError),
    #[error("text field identity is exhausted")]
    IdentityExhausted,
    #[error("text input belongs to a retired field or focus lifetime")]
    StaleInput,
    #[error("text field input is disabled")]
    Disabled,
    #[error("text field viewport must have a finite positive content width")]
    InvalidViewport,
    #[error("text field belongs to another UI root")]
    RootMismatch,
}
