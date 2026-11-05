//! Explicit immutable appearance inputs and reusable control recipes.

use nara_text::TextTypography;
use nara_ui_kernel::{
    UiBorder, UiBrush, UiColor, UiCornerRadii, UiDimension, UiEdges, UiNode, UiOverflow, UiRootKey,
    UiStyle, UiStyleError, UiText, UiTextAlignment, UiTextOverflow, UiThemePalette, UiVisual,
};
use thiserror::Error;

use crate::{UiButton, UiButtonState, UiTextFieldAppearance, UiWidgetError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiDensity {
    Compact,
    #[default]
    Normal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiControlSize {
    Small,
    #[default]
    Regular,
    Large,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiButtonVariant {
    Primary,
    #[default]
    Secondary,
    Quiet,
    Destructive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiTextRole {
    #[default]
    Body,
    Caption,
    Title,
}

/// A library palette. Additional libraries can retain their own roles beside this value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiAppearancePalette {
    pub window: UiColor,
    pub panel: UiColor,
    pub raised: UiColor,
    pub input: UiColor,
    pub border: UiColor,
    pub text: UiColor,
    pub muted_text: UiColor,
    pub disabled_text: UiColor,
    pub accent: UiColor,
    pub on_accent: UiColor,
    pub selection: UiColor,
    pub focus: UiColor,
    pub danger: UiColor,
    pub danger_text: UiColor,
    pub positive: UiColor,
    pub hover: UiColor,
    pub pressed: UiColor,
}

impl UiAppearancePalette {
    fn validate(self) -> Result<(), UiAppearanceError> {
        for color in [
            self.window,
            self.panel,
            self.raised,
            self.input,
            self.border,
            self.text,
            self.muted_text,
            self.disabled_text,
            self.accent,
            self.on_accent,
            self.selection,
            self.focus,
            self.danger,
            self.danger_text,
            self.positive,
            self.hover,
            self.pressed,
        ] {
            if color
                .components()
                .into_iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
            {
                return Err(UiAppearanceError::InvalidPalette);
            }
        }
        Ok(())
    }

    #[must_use]
    pub const fn kernel_palette(self) -> UiThemePalette {
        UiThemePalette {
            surface: self.window,
            panel: self.panel,
            primary_text: self.text,
            secondary_text: self.muted_text,
            accent: self.accent,
            positive: self.positive,
            warning: UiColor::rgba(0.65, 0.35, 0.06, 1.0),
            danger: self.danger,
            disabled: self.disabled_text,
            focus: self.focus,
        }
    }
}

impl Default for UiAppearancePalette {
    fn default() -> Self {
        Self {
            window: UiColor::rgba(0.025, 0.030, 0.040, 1.0),
            panel: UiColor::rgba(0.040, 0.047, 0.060, 1.0),
            raised: UiColor::rgba(0.065, 0.075, 0.095, 1.0),
            input: UiColor::rgba(0.018, 0.022, 0.032, 1.0),
            border: UiColor::rgba(0.120, 0.140, 0.175, 1.0),
            text: UiColor::rgba(0.90, 0.92, 0.96, 1.0),
            muted_text: UiColor::rgba(0.43, 0.48, 0.56, 1.0),
            disabled_text: UiColor::rgba(0.23, 0.27, 0.33, 1.0),
            accent: UiColor::rgba(0.025, 0.110, 0.320, 1.0),
            on_accent: UiColor::rgba(0.96, 0.98, 1.0, 1.0),
            selection: UiColor::rgba(0.035, 0.090, 0.180, 1.0),
            focus: UiColor::rgba(0.30, 0.55, 1.0, 1.0),
            danger: UiColor::rgba(0.40, 0.025, 0.045, 1.0),
            danger_text: UiColor::rgba(0.95, 0.32, 0.38, 1.0),
            positive: UiColor::rgba(0.08, 0.45, 0.20, 1.0),
            hover: UiColor::rgba(0.085, 0.105, 0.140, 1.0),
            pressed: UiColor::rgba(0.025, 0.055, 0.110, 1.0),
        }
    }
}

/// The effective value at a caller's current scope, independent of state and native scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiAppearance {
    palette: UiAppearancePalette,
    density: UiDensity,
    text_scale: f32,
}

impl Default for UiAppearance {
    fn default() -> Self {
        Self {
            palette: UiAppearancePalette::default(),
            density: UiDensity::Normal,
            text_scale: 1.0,
        }
    }
}

impl UiAppearance {
    pub fn new(
        palette: UiAppearancePalette,
        density: UiDensity,
        text_scale: f32,
    ) -> Result<Self, UiAppearanceError> {
        palette.validate()?;
        if !text_scale.is_finite() || !(0.5..=4.0).contains(&text_scale) {
            return Err(UiAppearanceError::InvalidTextScale { value: text_scale });
        }
        Ok(Self {
            palette,
            density,
            text_scale,
        })
    }

    #[must_use]
    pub const fn palette(self) -> UiAppearancePalette {
        self.palette
    }

    #[must_use]
    pub const fn density(self) -> UiDensity {
        self.density
    }

    #[must_use]
    pub const fn text_scale(self) -> f32 {
        self.text_scale
    }

    #[must_use]
    pub const fn with_density(mut self, density: UiDensity) -> Self {
        self.density = density;
        self
    }

    pub fn with_text_scale(self, text_scale: f32) -> Result<Self, UiAppearanceError> {
        Self::new(self.palette, self.density, text_scale)
    }

    pub fn with_palette(self, palette: UiAppearancePalette) -> Result<Self, UiAppearanceError> {
        Self::new(palette, self.density, self.text_scale)
    }

    #[must_use]
    pub const fn gap(self) -> f32 {
        match self.density {
            UiDensity::Compact => 4.0,
            UiDensity::Normal => 6.0,
        }
    }

    #[must_use]
    pub const fn panel_inset(self) -> f32 {
        match self.density {
            UiDensity::Compact => 8.0,
            UiDensity::Normal => 12.0,
        }
    }

    pub fn typography(self, role: UiTextRole) -> Result<TextTypography, UiAppearanceError> {
        let (size, height) = match role {
            UiTextRole::Body => (14.0, 20.0),
            UiTextRole::Caption => (12.0, 18.0),
            UiTextRole::Title => (16.0, 24.0),
        };
        Ok(TextTypography::new(size * self.text_scale)?
            .with_line_height(height * self.text_scale)?)
    }

    pub fn text(
        self,
        content: impl Into<String>,
        role: UiTextRole,
        muted: bool,
    ) -> Result<UiText, UiAppearanceError> {
        let typography = self.typography(role)?;
        Ok(UiText::new(content, typography.logical_size())?
            .with_typography(typography)
            .with_brush(UiBrush::Solid(if muted {
                self.palette.muted_text
            } else {
                self.palette.text
            })))
    }

    pub fn control_metrics(
        self,
        size: UiControlSize,
    ) -> Result<UiControlMetrics, UiAppearanceError> {
        let compact = self.density == UiDensity::Compact;
        let base_padding = if compact { 2.0 } else { 4.0 };
        let (minimum_height, vertical): (f32, f32) = match size {
            UiControlSize::Small => (
                if compact { 22.0 } else { 24.0 },
                (base_padding - 2.0_f32).max(0.0),
            ),
            UiControlSize::Regular => (if compact { 24.0 } else { 28.0 }, base_padding),
            UiControlSize::Large => (if compact { 30.0 } else { 34.0 }, base_padding + 3.0),
        };
        let typography = self.typography(UiTextRole::Body)?;
        let line_height = typography
            .line_height()
            .expect("control typography has a line height");
        let height = minimum_height.max(line_height + 2.0 * vertical);
        Ok(UiControlMetrics {
            typography,
            height,
            padding: UiEdges::symmetric(
                if compact { 8.0 } else { 10.0 },
                (height - line_height) * 0.5,
            )?,
            radius: 4.0,
            stroke: 1.0,
        })
    }

    pub fn field(
        self,
        focused: bool,
        enabled: bool,
        invalid: bool,
    ) -> Result<UiTextFieldAppearance, UiAppearanceError> {
        let metrics = self.control_metrics(UiControlSize::Regular)?;
        let colors = self.palette;
        let border = if focused {
            colors.focus
        } else if invalid {
            colors.danger
        } else {
            colors.border
        };
        Ok(UiTextFieldAppearance {
            padding: metrics.padding,
            foreground: UiBrush::Solid(if enabled {
                colors.text
            } else {
                colors.disabled_text
            }),
            visual: UiVisual::solid(if invalid {
                mix_color(colors.input, colors.danger, 0.35)
            } else {
                colors.input
            })
            .with_border(UiBorder::new(metrics.stroke, UiBrush::Solid(border))?)
            .with_radii(UiCornerRadii::all(metrics.radius)?),
            selection: UiBrush::Solid(colors.selection),
            caret: UiBrush::Solid(colors.focus),
        })
    }

    pub fn button(
        self,
        button: &UiButton,
        state: UiButtonState,
        variant: UiButtonVariant,
        size: UiControlSize,
        selected: bool,
    ) -> Result<UiButtonStyle, UiAppearanceError> {
        let metrics = self.control_metrics(size)?;
        let colors = self.palette;
        let enabled = button.enabled();
        let emphasis = matches!(
            variant,
            UiButtonVariant::Primary | UiButtonVariant::Destructive
        );
        let mut fill = match variant {
            UiButtonVariant::Primary => colors.accent,
            UiButtonVariant::Secondary => colors.raised,
            UiButtonVariant::Quiet => colors.panel,
            UiButtonVariant::Destructive => colors.danger,
        };
        if selected {
            fill = colors.selection;
        }
        if !enabled {
            fill = if selected {
                mix_color(colors.selection, colors.panel, 0.6)
            } else {
                colors.raised
            };
        }
        if enabled && (state.hovered() || state.pressed()) {
            let interaction = if state.pressed() {
                colors.pressed
            } else {
                colors.hover
            };
            fill = if selected {
                mix_color(colors.selection, interaction, 0.35)
            } else {
                interaction
            };
        }
        let text = if !enabled {
            colors.disabled_text
        } else if emphasis && !selected && !state.hovered() {
            colors.on_accent
        } else {
            colors.text
        };
        let border = if enabled && state.focused() {
            colors.focus
        } else if selected {
            colors.accent
        } else if variant == UiButtonVariant::Quiet {
            fill
        } else {
            colors.border
        };
        Ok(UiButtonStyle {
            metrics,
            visual: UiVisual::solid(fill)
                .with_border(UiBorder::new(metrics.stroke, UiBrush::Solid(border))?)
                .with_radii(UiCornerRadii::all(metrics.radius)?),
            text: UiBrush::Solid(text),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiControlMetrics {
    typography: TextTypography,
    height: f32,
    padding: UiEdges,
    radius: f32,
    stroke: f32,
}

impl UiControlMetrics {
    pub fn new(
        typography: TextTypography,
        height: f32,
        padding: UiEdges,
        radius: f32,
        stroke: f32,
    ) -> Result<Self, UiAppearanceError> {
        let line_height = typography
            .line_height()
            .ok_or(UiAppearanceError::AutomaticControlMetrics)?;
        if !height.is_finite() || height < line_height + padding.top() + padding.bottom() {
            return Err(UiAppearanceError::InvalidControlHeight { value: height });
        }
        UiCornerRadii::all(radius)?;
        UiBorder::new(stroke, UiBrush::Solid(UiColor::rgba(0.0, 0.0, 0.0, 0.0)))?;
        Ok(Self {
            typography,
            height,
            padding,
            radius,
            stroke,
        })
    }

    #[must_use]
    pub const fn typography(self) -> TextTypography {
        self.typography
    }

    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }

    #[must_use]
    pub const fn padding(self) -> UiEdges {
        self.padding
    }

    #[must_use]
    pub const fn radius(self) -> f32 {
        self.radius
    }

    #[must_use]
    pub const fn stroke(self) -> f32 {
        self.stroke
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiButtonStyle {
    metrics: UiControlMetrics,
    visual: UiVisual,
    text: UiBrush,
}

impl UiButtonStyle {
    #[must_use]
    pub const fn metrics(&self) -> UiControlMetrics {
        self.metrics
    }

    #[must_use]
    pub const fn visual(&self) -> &UiVisual {
        &self.visual
    }

    #[must_use]
    pub const fn text_brush(&self) -> &UiBrush {
        &self.text
    }

    pub fn with_metrics(mut self, metrics: UiControlMetrics) -> Result<Self, UiAppearanceError> {
        self.visual = self.visual.with_radii(UiCornerRadii::all(metrics.radius)?);
        if let Some(border) = self.visual.border() {
            let brush = border.brush().clone();
            self.visual = self
                .visual
                .with_border(UiBorder::new(metrics.stroke, brush)?);
        }
        self.metrics = metrics;
        Ok(self)
    }

    #[must_use]
    pub fn with_visual(mut self, visual: UiVisual) -> Self {
        self.visual = visual;
        self
    }

    #[must_use]
    pub fn with_text_brush(mut self, brush: UiBrush) -> Self {
        self.text = brush;
        self
    }

    pub fn authored_node(
        &self,
        button: &UiButton,
        root: &UiRootKey,
        mut style: UiStyle,
    ) -> Result<UiNode, UiAppearanceError> {
        if style.height() == UiDimension::Auto {
            style = style.with_height(UiDimension::Px(self.metrics.height));
        }
        let text = UiText::new(button.label(), self.metrics.typography.logical_size())?
            .with_typography(self.metrics.typography)
            .with_brush(self.text.clone())
            .with_alignment(UiTextAlignment::Center)
            .with_overflow(UiTextOverflow::Clip);
        Ok(button
            .authored_node(
                root,
                style
                    .with_padding(self.metrics.padding)
                    .with_overflow(UiOverflow::Clip),
                self.visual.clone(),
                self.text.clone(),
                self.metrics.typography.logical_size(),
            )?
            .with_text(text))
    }
}

#[derive(Debug, Error)]
pub enum UiAppearanceError {
    #[error("appearance palette channels must be finite and within 0..=1")]
    InvalidPalette,
    #[error("text scale {value} must be finite and within 0.5..=4")]
    InvalidTextScale { value: f32 },
    #[error("control height {value} must contain its typography and vertical padding")]
    InvalidControlHeight { value: f32 },
    #[error(
        "control metrics require an explicit line height; automatic font metrics need actual child measurement"
    )]
    AutomaticControlMetrics,
    #[error(transparent)]
    Text(#[from] nara_text::TextLayoutError),
    #[error(transparent)]
    Style(#[from] UiStyleError),
    #[error(transparent)]
    Widget(#[from] UiWidgetError),
}

fn mix_color(background: UiColor, foreground: UiColor, amount: f32) -> UiColor {
    let background = background.components();
    let foreground = foreground.components();
    let channels = std::array::from_fn::<_, 4, _>(|index| {
        background[index] + (foreground[index] - background[index]) * amount
    });
    UiColor::rgba(channels[0], channels[1], channels[2], channels[3])
}
