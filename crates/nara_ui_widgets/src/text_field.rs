use std::{
    cell::RefCell,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_text::{
    TextEdit, TextEditError, TextEditOutcome, TextEditor, TextEngine, TextRect, TextTypography,
};
use nara_ui_kernel::{
    UiAvailableSpace, UiBorder, UiBrush, UiColor, UiColorRole, UiCustomLayout, UiDimension,
    UiDispatchBatch, UiEdges, UiInteraction, UiKey, UiKeyError, UiLayout, UiLayoutChildren,
    UiLayoutConstraints, UiLayoutError, UiLayoutMeasurement, UiLayoutOutput, UiLocalKey, UiNode,
    UiOverflow, UiPoint, UiPreparedText, UiRect, UiRootKey, UiRoutePhase, UiRoutedEventKind,
    UiSemanticRole, UiSemantics, UiSize, UiStyle, UiStyleError, UiVisual,
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
    layout: RefCell<Option<FieldLayouts>>,
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
            layout: RefCell::new(None),
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

    /// Places the cursor using a point relative to the field's outer origin.
    /// `viewport_width` must be the actual presented outer width used for painting, not the
    /// preferred intrinsic width supplied to [`Self::authored_node`].
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

    /// Returns the candidate exclusion area relative to the field's outer origin.
    /// Use the actual presented outer width so IME, pointer input and paint share one viewport.
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
    /// Supply the actual presented outer width when correlating painting, input or IME geometry.
    pub fn layout_measurement(
        &self,
        viewport_width: f32,
    ) -> Result<UiLayoutMeasurement, UiTextFieldError> {
        Ok(self.geometry(viewport_width)?.measurement)
    }

    /// Captures immutable field contents for layout at the final allocated logical width.
    ///
    /// `viewport_width` is the preferred intrinsic outer width for auto/max-content sizing.
    /// Min-content sizing reserves padding and positive content width. A definite allocation
    /// determines the viewport, horizontal scroll, selection and caret, independently of that
    /// preference. Painting and geometric input must use the same actual presented outer width;
    /// pass it to [`Self::point`], [`Self::ime_area`] and [`Self::layout_measurement`].
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
        let metrics = self.metrics()?;
        let geometry = metrics.geometry(viewport_width)?;
        let focused = self.focus.is_some();
        let description = FieldLayout {
            metrics,
            preferred_width: viewport_width,
        };
        let viewport_description = ViewportLayout {
            metrics,
            preferred_width: geometry.viewport.size().width(),
            text_width: self.editor.layout().width().max(1.0),
            selection: if focused {
                self.editor.selection_rects().to_vec()
            } else {
                Vec::new()
            },
            caret: focused.then(|| self.editor.caret()).flatten(),
        };
        let (layout, viewport_layout) = {
            let mut cached = self.layout.borrow_mut();
            match &*cached {
                Some(previous)
                    if previous.field == description
                        && previous.viewport == viewport_description =>
                {
                    (previous.layout.clone(), previous.viewport_layout.clone())
                }
                _ => {
                    let layout = UiCustomLayout::new(description);
                    let viewport_layout = UiCustomLayout::new(viewport_description.clone());
                    *cached = Some(FieldLayouts {
                        field: description,
                        viewport: viewport_description.clone(),
                        layout: layout.clone(),
                        viewport_layout: viewport_layout.clone(),
                    });
                    (layout, viewport_layout)
                }
            }
        };
        let minimum_height = geometry.measurement.size().height();
        let style = match style.height() {
            UiDimension::Px(height) if height.is_finite() && height >= 0.0 => {
                style.with_height(UiDimension::Px(height.max(minimum_height)))
            }
            _ => style,
        };
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
        for _ in &viewport_description.selection {
            children.push(
                UiNode::new(UiStyle::flex())
                    .with_visual(UiVisual::default().with_background(selection_brush.clone())),
            );
        }
        children.push(
            UiNode::new(UiStyle::flex())
                .with_key(self.text_key.clone())
                .with_prepared_text(
                    UiPreparedText::new(self.editor.paint_snapshot())
                        .with_draw_offset(UiPoint::new(0.0, -metrics.block_min))
                        .with_brush(foreground)
                        .with_semantic_text(self.editor.text()),
                ),
        );
        if viewport_description.caret.is_some() {
            children.push(
                UiNode::new(UiStyle::flex())
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
        .with_custom_layout(layout)
        .with_child(
            UiNode::new(UiStyle::flex().with_overflow(UiOverflow::Clip))
                .with_custom_layout(viewport_layout)
                .with_children(children),
        ))
    }

    fn geometry(&self, viewport_width: f32) -> Result<FieldGeometry, UiTextFieldError> {
        self.metrics()?.geometry(viewport_width)
    }

    fn metrics(&self) -> Result<FieldMetrics, UiTextFieldError> {
        let padding = match &self.appearance {
            Some(appearance) => appearance.padding,
            None => UiEdges::all(TEXT_INSET)?,
        };
        let (block_min, block_max) = self.editor.layout().lines().iter().fold(
            (0.0_f32, self.editor.layout().height()),
            |(min, max), line| (min.min(line.block_min()), max.max(line.block_max())),
        );
        Ok(FieldMetrics {
            padding,
            block_min,
            block_height: block_max - block_min,
            baseline: self
                .editor
                .layout()
                .lines()
                .first()
                .map(|line| line.baseline()),
            scroll_edge: self
                .focus
                .and_then(|_| self.editor.caret())
                .map_or(0.0, |caret| caret.x1),
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
struct FieldMetrics {
    padding: UiEdges,
    block_min: f32,
    block_height: f32,
    baseline: Option<f32>,
    scroll_edge: f32,
}

impl FieldMetrics {
    fn scroll(self, inner_width: f32) -> f32 {
        (self.scroll_edge - inner_width).max(0.0)
    }

    fn geometry(self, viewport_width: f32) -> Result<FieldGeometry, UiTextFieldError> {
        let padding = self.padding;
        let inner_width = viewport_width - padding.left() - padding.right();
        if !inner_width.is_finite() || inner_width <= 0.0 {
            return Err(UiTextFieldError::InvalidViewport);
        }
        let viewport = UiRect::from_origin_size(
            padding.left(),
            padding.top(),
            inner_width,
            self.block_height,
        )
        .filter(|rect| !rect.size().is_empty())
        .ok_or(UiTextFieldError::InvalidViewport)?;
        let text_origin = UiPoint::new(
            padding.left() - self.scroll(inner_width),
            padding.top() - self.block_min,
        );
        if !text_origin.y().is_finite() || !viewport.max().y().is_finite() {
            return Err(UiTextFieldError::InvalidViewport);
        }
        let outer_size = UiSize::new(viewport_width, viewport.max().y() + padding.bottom())
            .ok_or(UiTextFieldError::InvalidViewport)?;
        let baseline = self.baseline.map(|baseline| text_origin.y() + baseline);
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

#[derive(Clone, Copy, PartialEq)]
struct FieldLayout {
    metrics: FieldMetrics,
    preferred_width: f32,
}

impl UiLayout for FieldLayout {
    fn layout(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if children.len() != 1 {
            return Err(UiLayoutError::InvalidGeometry);
        }
        let minimum_width = self.metrics.padding.left() + self.metrics.padding.right() + 1.0;
        let width = match constraints.width {
            UiAvailableSpace::Definite(width) => width,
            UiAvailableSpace::MinContent => minimum_width,
            UiAvailableSpace::MaxContent => self.preferred_width.max(minimum_width),
        };
        let geometry = self
            .metrics
            .geometry(width)
            .map_err(|_| UiLayoutError::InvalidGeometry)?;
        children.place(0, geometry.viewport)?;
        let output = UiLayoutOutput::new(geometry.measurement.size());
        match geometry.measurement.first_baseline() {
            Some(baseline) => output.with_first_baseline(baseline),
            None => Ok(output),
        }
    }
}

#[derive(Clone, PartialEq)]
struct ViewportLayout {
    metrics: FieldMetrics,
    preferred_width: f32,
    text_width: f32,
    selection: Vec<TextRect>,
    caret: Option<TextRect>,
}

impl UiLayout for ViewportLayout {
    fn layout(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if children.len() != self.selection.len() + 1 + usize::from(self.caret.is_some()) {
            return Err(UiLayoutError::InvalidGeometry);
        }
        let width = match constraints.width {
            UiAvailableSpace::Definite(width) => width,
            UiAvailableSpace::MinContent => 1.0,
            UiAvailableSpace::MaxContent => self.preferred_width,
        };
        if !width.is_finite() || width <= 0.0 {
            return Err(UiLayoutError::InvalidGeometry);
        }
        let size =
            UiSize::new(width, self.metrics.block_height).ok_or(UiLayoutError::InvalidGeometry)?;
        let geometry = FieldGeometry {
            viewport: UiRect::new(UiPoint::default(), size)
                .ok_or(UiLayoutError::InvalidGeometry)?,
            text_origin: UiPoint::new(-self.metrics.scroll(width), -self.metrics.block_min),
            measurement: UiLayoutMeasurement::new(
                size,
                self.metrics
                    .baseline
                    .map(|baseline| baseline - self.metrics.block_min),
            )?,
        };
        for (index, selection) in self.selection.iter().enumerate() {
            place_visible_rect(children, index, *selection, &geometry)?;
        }
        children.place(
            self.selection.len(),
            UiRect::from_origin_size(
                geometry.text_origin.x(),
                0.0,
                self.text_width,
                size.height(),
            )
            .ok_or(UiLayoutError::InvalidGeometry)?,
        )?;
        if let Some(caret) = self.caret {
            place_visible_rect(children, self.selection.len() + 1, caret, &geometry)?;
        }
        let output = UiLayoutOutput::new(size);
        match geometry.measurement.first_baseline() {
            Some(baseline) => output.with_first_baseline(baseline),
            None => Ok(output),
        }
    }
}

struct FieldLayouts {
    field: FieldLayout,
    viewport: ViewportLayout,
    layout: UiCustomLayout,
    viewport_layout: UiCustomLayout,
}

fn place_visible_rect(
    children: &mut UiLayoutChildren<'_>,
    index: usize,
    rect: TextRect,
    geometry: &FieldGeometry,
) -> Result<(), UiLayoutError> {
    match visible_rect(rect, geometry) {
        Some(rect) => children.place(index, rect),
        None => children.hide(index),
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
