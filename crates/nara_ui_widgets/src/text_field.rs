use std::sync::atomic::{AtomicU64, Ordering};

use nara_text::{
    TextEdit, TextEditError, TextEditOutcome, TextEditor, TextEngine, TextRect, TextTypography,
};
use nara_ui_kernel::{
    UiBorder, UiBrush, UiColor, UiColorRole, UiCustomLayout, UiDimension, UiDispatchBatch, UiEdges,
    UiInteraction, UiKey, UiKeyError, UiLayout, UiLayoutChildren, UiLayoutConstraints,
    UiLayoutError, UiLayoutMeasurement, UiLayoutOutput, UiLocalKey, UiNode, UiOverflow, UiPoint,
    UiPreparedText, UiRect, UiRootKey, UiRoutePhase, UiRoutedEventKind, UiSemanticRole,
    UiSemantics, UiSize, UiStyle, UiStyleError, UiVisual,
};
use thiserror::Error;

static NEXT_FIELD: AtomicU64 = AtomicU64::new(1);
const TEXT_INSET: f32 = 6.0;

/// Renderer-neutral, caller-resolved appearance adopted by one live field.
///
/// `visual` contains the background, border and corner radii. The caller resolves focused and
/// disabled variants before adoption. Padding belongs here so painting, hit testing and IME
/// exclusion share it. The outer layout style controls placement and size, not field padding.
#[derive(Clone, Debug, PartialEq)]
pub struct UiTextFieldAppearance {
    pub padding: UiEdges,
    pub foreground: UiBrush,
    pub visual: UiVisual,
    pub selection: UiBrush,
    pub caret: UiBrush,
}

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
    appearance: Option<UiTextFieldAppearance>,
}

impl UiTextField {
    pub fn new(
        root: UiRootKey,
        label: impl Into<String>,
        value: &str,
        logical_size: f32,
        engine: &mut TextEngine,
    ) -> Result<Self, UiTextFieldError> {
        let typography = TextTypography::new(logical_size).map_err(TextEditError::from)?;
        Self::with_typography(root, label, value, typography, engine)
    }

    pub fn with_typography(
        root: UiRootKey,
        label: impl Into<String>,
        value: &str,
        typography: TextTypography,
        engine: &mut TextEngine,
    ) -> Result<Self, UiTextFieldError> {
        let label = label.into();
        UiSemantics::new(UiSemanticRole::TextInput, label.clone())?;
        let editor = TextEditor::with_typography(engine, value, typography)?;
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
            appearance: None,
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

    #[must_use]
    pub const fn appearance(&self) -> Option<&UiTextFieldAppearance> {
        self.appearance.as_ref()
    }

    /// Adopts the same appearance for future painting and geometric input.
    /// Brush validation remains part of ordinary authoring-snapshot admission.
    pub fn set_appearance(&mut self, appearance: UiTextFieldAppearance) {
        self.appearance = Some(appearance);
    }

    /// Atomically adopts typography and ordered fonts without replacing the field or focus token.
    pub fn reconfigure(
        &mut self,
        engine: &mut TextEngine,
        typography: TextTypography,
    ) -> Result<TextEditOutcome, UiTextFieldError> {
        Ok(self.editor.reconfigure(engine, typography)?)
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
        let geometry = self.geometry(viewport_width)?;
        self.edit(
            token,
            &TextEdit::Point {
                x: position.x() - geometry.text_origin.x(),
                y: position.y() - geometry.text_origin.y(),
                extend,
            },
            engine,
        )
    }

    pub fn ime_area(&self, viewport_width: f32) -> Result<UiRect, UiTextFieldError> {
        let geometry = self.geometry(viewport_width)?;
        let area = self.editor.ime_area();
        if let Some(rect) = visible_rect(area, &geometry) {
            return Ok(rect);
        }
        let min = geometry.viewport.origin();
        let max = geometry.viewport.max();
        let y0 = (area.y0 + geometry.text_origin.y()).clamp(min.y(), max.y());
        let y1 = (area.y1 + geometry.text_origin.y()).clamp(min.y(), max.y());
        UiRect::from_origin_size(
            (area.x0 + geometry.text_origin.x()).clamp(min.x(), max.x()),
            y0,
            0.0,
            y1 - y0,
        )
        .ok_or(UiTextFieldError::InvalidViewport)
    }

    /// Returns the minimum outer box and its actual first baseline under the adopted padding.
    /// This reads the retained editing layout without shaping or rasterizing.
    pub fn layout_measurement(
        &self,
        viewport_width: f32,
    ) -> Result<UiLayoutMeasurement, UiTextFieldError> {
        Ok(self.geometry(viewport_width)?.measurement)
    }

    /// Builds the field's clipped contents within the caller's resolved logical width.
    ///
    /// The vertical window contains the actual line blocks, including negative block origins
    /// from short line heights. Auto height measures that window plus padding; pixel heights
    /// reserve at least that minimum. Percentage heights and exact parent allocations must
    /// accommodate [`Self::layout_measurement`].
    pub fn authored_node(
        &self,
        root: &UiRootKey,
        style: UiStyle,
        viewport_width: f32,
    ) -> Result<UiNode, UiTextFieldError> {
        if self.key.root_key() != root {
            return Err(UiTextFieldError::RootMismatch);
        }
        let geometry = self.geometry(viewport_width)?;
        let viewport = geometry.viewport;
        let minimum_height = geometry.measurement.size().height();
        let style = match style.height() {
            UiDimension::Px(height) if height.is_finite() && height >= 0.0 => {
                style.with_height(UiDimension::Px(height.max(minimum_height)))
            }
            _ => style,
        };
        let focused = self.focus.is_some();
        let foreground = self.appearance.as_ref().map_or_else(
            || UiBrush::Role(UiColorRole::PrimaryText),
            |appearance| appearance.foreground.clone(),
        );
        let selection_brush = self.appearance.as_ref().map_or_else(
            || UiBrush::Solid(UiColor::rgba(0.2, 0.45, 0.9, 0.35)),
            |appearance| appearance.selection.clone(),
        );
        let caret_brush = self.appearance.as_ref().map_or_else(
            || UiBrush::Role(UiColorRole::Focus),
            |appearance| appearance.caret.clone(),
        );
        let mut children = Vec::new();
        if focused {
            for selection in self.editor.selection_rects() {
                if let Some(rect) = visible_rect(*selection, &geometry) {
                    children.push(
                        UiNode::new(rect_style(rect, viewport.origin())).with_visual(
                            UiVisual::default().with_background(selection_brush.clone()),
                        ),
                    );
                }
            }
        }
        children.push(
            UiNode::new(UiStyle::absolute(
                geometry.text_origin.x() - viewport.origin().x(),
                0.0,
                self.editor.layout().width().max(1.0),
                viewport.size().height(),
            ))
            .with_key(self.text_key.clone())
            .with_prepared_text(
                UiPreparedText::new(self.editor.paint_snapshot())
                    .with_draw_offset(UiPoint::new(
                        0.0,
                        geometry.text_origin.y() - viewport.origin().y(),
                    ))
                    .with_brush(foreground)
                    .with_semantic_text(self.editor.text()),
            ),
        );
        if focused
            && let Some(caret) = self
                .editor
                .caret()
                .and_then(|caret| visible_rect(caret, &geometry))
        {
            children.push(
                UiNode::new(rect_style(caret, viewport.origin()))
                    .with_visual(UiVisual::default().with_background(caret_brush)),
            );
        }
        let visual = if let Some(appearance) = &self.appearance {
            appearance.visual.clone()
        } else {
            UiVisual::role(if self.enabled {
                UiColorRole::Panel
            } else {
                UiColorRole::Disabled
            })
            .with_border(UiBorder::new(
                1.0,
                UiBrush::Role(if focused {
                    UiColorRole::Focus
                } else {
                    UiColorRole::SecondaryText
                }),
            )?)
        };
        Ok(UiNode::new(
            style
                .with_padding(UiEdges::default())
                .with_overflow(UiOverflow::Clip),
        )
        .with_key(
            self.key
                .local_key()
                .expect("a field always has a local key")
                .clone(),
        )
        .with_visual(visual)
        .with_interaction(UiInteraction::focus_target().with_enabled(self.enabled))
        .with_semantics(
            UiSemantics::new(UiSemanticRole::TextInput, self.label.clone())?
                .with_value(self.editor.text())?,
        )
        .with_custom_layout(UiCustomLayout::new(FieldLayout {
            viewport,
            measurement: geometry.measurement,
        }))
        .with_child(
            UiNode::new(rect_style(viewport, UiPoint::default()).with_overflow(UiOverflow::Clip))
                .with_children(children),
        ))
    }

    fn geometry(&self, viewport_width: f32) -> Result<FieldGeometry, UiTextFieldError> {
        let padding = match &self.appearance {
            Some(appearance) => appearance.padding,
            None => UiEdges::all(TEXT_INSET)?,
        };
        let inner_width = viewport_width - padding.left() - padding.right();
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
        let (block_min, block_max) = self.editor.layout().lines().iter().fold(
            (0.0_f32, self.editor.layout().height()),
            |(min, max), line| (min.min(line.block_min()), max.max(line.block_max())),
        );
        let viewport = UiRect::from_origin_size(
            padding.left(),
            padding.top(),
            inner_width,
            block_max - block_min,
        )
        .filter(|rect| !rect.size().is_empty())
        .ok_or(UiTextFieldError::InvalidViewport)?;
        let text_origin = UiPoint::new(padding.left() - scroll, padding.top() - block_min);
        if !text_origin.y().is_finite() || !viewport.max().y().is_finite() {
            return Err(UiTextFieldError::InvalidViewport);
        }
        let outer_size = UiSize::new(viewport_width, viewport.max().y() + padding.bottom())
            .ok_or(UiTextFieldError::InvalidViewport)?;
        let baseline = self
            .editor
            .layout()
            .lines()
            .first()
            .map(|line| text_origin.y() + line.baseline());
        let measurement = UiLayoutMeasurement::new(outer_size, baseline)
            .map_err(|_| UiTextFieldError::InvalidViewport)?;
        Ok(FieldGeometry {
            viewport,
            text_origin,
            measurement,
        })
    }
}

struct FieldGeometry {
    viewport: UiRect,
    text_origin: UiPoint,
    measurement: UiLayoutMeasurement,
}

struct FieldLayout {
    viewport: UiRect,
    measurement: UiLayoutMeasurement,
}

impl UiLayout for FieldLayout {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if children.len() != 1 {
            return Err(UiLayoutError::InvalidGeometry);
        }
        children.place(0, self.viewport)?;
        let output = UiLayoutOutput::new(self.measurement.size());
        match self.measurement.first_baseline() {
            Some(baseline) => output.with_first_baseline(baseline),
            None => Ok(output),
        }
    }
}

fn visible_rect(rect: TextRect, geometry: &FieldGeometry) -> Option<UiRect> {
    let rect = UiRect::from_origin_size(
        rect.x0 + geometry.text_origin.x(),
        rect.y0 + geometry.text_origin.y(),
        rect.x1 - rect.x0,
        rect.y1 - rect.y0,
    )?;
    rect.intersect(geometry.viewport)
}

fn rect_style(rect: UiRect, origin: UiPoint) -> UiStyle {
    UiStyle::absolute(
        rect.origin().x() - origin.x(),
        rect.origin().y() - origin.y(),
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
    #[error("text field viewport must have finite positive content bounds")]
    InvalidViewport,
    #[error("text field belongs to another UI root")]
    RootMismatch,
}
