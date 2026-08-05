use nara_text::{TextAlignment, TextLayoutError, TextLayoutRequest};
use thiserror::Error;

use crate::{UiImageId, UiResolvedBrush};

/// A monotonically assigned adapter revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiRevision(u64);

impl UiRevision {
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A logical length accepted by the first Flex/Absolute vocabulary.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum UiDimension {
    #[default]
    Auto,
    Px(f32),
    /// Fraction of the containing logical extent in the closed range `0.0..=1.0`.
    Percent(f32),
}

impl UiDimension {
    pub fn px(value: f32) -> Result<Self, UiStyleError> {
        validate_non_negative("dimension", value)?;
        Ok(Self::Px(value))
    }

    pub fn percent(value: f32) -> Result<Self, UiStyleError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(UiStyleError::InvalidPercent { value });
        }
        Ok(Self::Percent(value))
    }

    #[must_use]
    pub const fn auto() -> Self {
        Self::Auto
    }

    pub(crate) fn validate(self) -> Result<(), UiStyleError> {
        match self {
            Self::Auto => Ok(()),
            Self::Px(value) => validate_non_negative("dimension", value),
            Self::Percent(value) => {
                if value.is_finite() && (0.0..=1.0).contains(&value) {
                    Ok(())
                } else {
                    Err(UiStyleError::InvalidPercent { value })
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiPositionMode {
    #[default]
    Flow,
    Absolute,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiFlexDirection {
    Row,
    #[default]
    Column,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiAlign {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiJustify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiOverflow {
    #[default]
    Visible,
    Clip,
}

/// Logical box edges in left, right, top, bottom order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiEdges {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl UiEdges {
    pub fn all(value: f32) -> Result<Self, UiStyleError> {
        Self::new(value, value, value, value)
    }

    pub fn symmetric(horizontal: f32, vertical: f32) -> Result<Self, UiStyleError> {
        Self::new(horizontal, horizontal, vertical, vertical)
    }

    pub fn new(left: f32, right: f32, top: f32, bottom: f32) -> Result<Self, UiStyleError> {
        for (field, value) in [
            ("padding.left", left),
            ("padding.right", right),
            ("padding.top", top),
            ("padding.bottom", bottom),
        ] {
            validate_non_negative(field, value)?;
        }
        Ok(Self {
            left,
            right,
            top,
            bottom,
        })
    }

    #[must_use]
    pub const fn left(self) -> f32 {
        self.left
    }

    #[must_use]
    pub const fn right(self) -> f32 {
        self.right
    }

    #[must_use]
    pub const fn top(self) -> f32 {
        self.top
    }

    #[must_use]
    pub const fn bottom(self) -> f32 {
        self.bottom
    }

    fn validate(self) -> Result<(), UiStyleError> {
        Self::new(self.left, self.right, self.top, self.bottom).map(|_| ())
    }
}

/// Closed first-pass style vocabulary lowered privately to Taffy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiStyle {
    pub(crate) position_mode: UiPositionMode,
    pub(crate) width: UiDimension,
    pub(crate) height: UiDimension,
    pub(crate) left: UiDimension,
    pub(crate) top: UiDimension,
    pub(crate) flex_direction: UiFlexDirection,
    pub(crate) align_items: UiAlign,
    pub(crate) justify_content: UiJustify,
    pub(crate) gap: f32,
    pub(crate) padding: UiEdges,
    pub(crate) flex_grow: f32,
    pub(crate) flex_shrink: f32,
    pub(crate) z_index: i32,
    pub(crate) visible: bool,
    pub(crate) overflow: UiOverflow,
    pub(crate) stacking_context: bool,
}

impl UiStyle {
    #[must_use]
    pub const fn flex() -> Self {
        Self {
            position_mode: UiPositionMode::Flow,
            width: UiDimension::Auto,
            height: UiDimension::Auto,
            left: UiDimension::Auto,
            top: UiDimension::Auto,
            flex_direction: UiFlexDirection::Column,
            align_items: UiAlign::Stretch,
            justify_content: UiJustify::Start,
            gap: 0.0,
            padding: UiEdges {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            },
            flex_grow: 0.0,
            flex_shrink: 1.0,
            z_index: 0,
            visible: true,
            overflow: UiOverflow::Visible,
            stacking_context: false,
        }
    }

    #[must_use]
    pub const fn fill() -> Self {
        Self::flex()
            .with_width(UiDimension::Percent(1.0))
            .with_height(UiDimension::Percent(1.0))
    }

    #[must_use]
    pub const fn absolute(left: f32, top: f32, width: f32, height: f32) -> Self {
        Self::flex()
            .with_position_mode(UiPositionMode::Absolute)
            .with_left(UiDimension::Px(left))
            .with_top(UiDimension::Px(top))
            .with_width(UiDimension::Px(width))
            .with_height(UiDimension::Px(height))
    }

    #[must_use]
    pub const fn with_position_mode(mut self, value: UiPositionMode) -> Self {
        self.position_mode = value;
        self
    }

    #[must_use]
    pub const fn with_width(mut self, value: UiDimension) -> Self {
        self.width = value;
        self
    }

    #[must_use]
    pub const fn with_height(mut self, value: UiDimension) -> Self {
        self.height = value;
        self
    }

    #[must_use]
    pub const fn with_left(mut self, value: UiDimension) -> Self {
        self.left = value;
        self
    }

    #[must_use]
    pub const fn with_top(mut self, value: UiDimension) -> Self {
        self.top = value;
        self
    }

    #[must_use]
    pub const fn with_flex_direction(mut self, value: UiFlexDirection) -> Self {
        self.flex_direction = value;
        self
    }

    #[must_use]
    pub const fn with_align_items(mut self, value: UiAlign) -> Self {
        self.align_items = value;
        self
    }

    #[must_use]
    pub const fn with_justify_content(mut self, value: UiJustify) -> Self {
        self.justify_content = value;
        self
    }

    #[must_use]
    pub const fn with_gap(mut self, value: f32) -> Self {
        self.gap = value;
        self
    }

    #[must_use]
    pub const fn with_padding(mut self, value: UiEdges) -> Self {
        self.padding = value;
        self
    }

    #[must_use]
    pub const fn with_flex_grow(mut self, value: f32) -> Self {
        self.flex_grow = value;
        self
    }

    #[must_use]
    pub const fn with_flex_shrink(mut self, value: f32) -> Self {
        self.flex_shrink = value;
        self
    }

    #[must_use]
    pub const fn with_z_index(mut self, value: i32) -> Self {
        self.z_index = value;
        self
    }

    #[must_use]
    pub const fn with_visible(mut self, value: bool) -> Self {
        self.visible = value;
        self
    }

    #[must_use]
    pub const fn with_overflow(mut self, value: UiOverflow) -> Self {
        self.overflow = value;
        self
    }

    #[must_use]
    pub const fn with_stacking_context(mut self, value: bool) -> Self {
        self.stacking_context = value;
        self
    }

    pub(crate) fn validate(self) -> Result<(), UiStyleError> {
        self.width.validate()?;
        self.height.validate()?;
        self.left.validate()?;
        self.top.validate()?;
        validate_non_negative("gap", self.gap)?;
        self.padding.validate()?;
        validate_non_negative("flex_grow", self.flex_grow)?;
        validate_non_negative("flex_shrink", self.flex_shrink)?;
        if !(-1_000_000..=1_000_000).contains(&self.z_index) {
            return Err(UiStyleError::ZIndexOutOfRange {
                value: self.z_index,
            });
        }
        Ok(())
    }
}

impl Default for UiStyle {
    fn default() -> Self {
        Self::flex()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiColor {
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
}

impl UiColor {
    #[must_use]
    pub const fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    #[must_use]
    pub const fn components(self) -> [f32; 4] {
        [self.red, self.green, self.blue, self.alpha]
    }

    fn validate(self) -> Result<(), UiStyleError> {
        if self
            .components()
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
        {
            Ok(())
        } else {
            Err(UiStyleError::InvalidColor)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiColorRole {
    Surface,
    Panel,
    PrimaryText,
    SecondaryText,
    Accent,
    Positive,
    Warning,
    Danger,
    Disabled,
    Focus,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiBrush {
    Solid(UiColor),
    Role(UiColorRole),
    Image { image: UiImageId, tint: UiColor },
}

impl UiBrush {
    fn validate(&self) -> Result<(), UiStyleError> {
        match self {
            Self::Solid(color) => color.validate(),
            Self::Role(_) => Ok(()),
            Self::Image { tint, .. } => tint.validate(),
        }
    }

    pub(crate) fn resolve(&self, theme: UiTheme) -> UiResolvedBrush {
        match self {
            Self::Solid(color) => UiResolvedBrush::Solid(*color),
            Self::Role(role) => UiResolvedBrush::Solid(theme.palette.color(*role)),
            Self::Image { image, tint } => UiResolvedBrush::Image {
                image: image.clone(),
                tint: *tint,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiBorder {
    width: f32,
    brush: UiBrush,
}

impl UiBorder {
    pub fn new(width: f32, brush: UiBrush) -> Result<Self, UiStyleError> {
        validate_non_negative("border.width", width)?;
        brush.validate()?;
        if matches!(&brush, UiBrush::Image { .. }) {
            return Err(UiStyleError::ImageBorderUnsupported);
        }
        Ok(Self { width, brush })
    }

    #[must_use]
    pub const fn width(&self) -> f32 {
        self.width
    }

    #[must_use]
    pub const fn brush(&self) -> &UiBrush {
        &self.brush
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiCornerRadii {
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
}

impl UiCornerRadii {
    pub fn all(value: f32) -> Result<Self, UiStyleError> {
        validate_non_negative("corner_radius", value)?;
        Ok(Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        })
    }

    #[must_use]
    pub const fn values(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiVisual {
    pub(crate) background: Option<UiBrush>,
    pub(crate) border: Option<UiBorder>,
    pub(crate) radii: UiCornerRadii,
}

impl UiVisual {
    #[must_use]
    pub fn solid(color: UiColor) -> Self {
        Self {
            background: Some(UiBrush::Solid(color)),
            border: None,
            radii: UiCornerRadii {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
        }
    }

    #[must_use]
    pub fn role(role: UiColorRole) -> Self {
        Self {
            background: Some(UiBrush::Role(role)),
            border: None,
            radii: UiCornerRadii {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
        }
    }

    #[must_use]
    pub fn image(image: UiImageId, tint: UiColor) -> Self {
        Self {
            background: Some(UiBrush::Image { image, tint }),
            border: None,
            radii: UiCornerRadii::default(),
        }
    }

    #[must_use]
    pub fn with_background(mut self, brush: UiBrush) -> Self {
        self.background = Some(brush);
        self
    }

    #[must_use]
    pub fn with_border(mut self, border: UiBorder) -> Self {
        self.border = Some(border);
        self
    }

    #[must_use]
    pub fn with_radii(mut self, radii: UiCornerRadii) -> Self {
        self.radii = radii;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), UiStyleError> {
        if let Some(background) = &self.background {
            background.validate()?;
        }
        if let Some(border) = &self.border {
            validate_non_negative("border.width", border.width)?;
            border.brush.validate()?;
            if matches!(&border.brush, UiBrush::Image { .. }) {
                return Err(UiStyleError::ImageBorderUnsupported);
            }
        }
        for radius in self.radii.values() {
            validate_non_negative("corner_radius", radius)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiTextAlignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiTextOverflow {
    #[default]
    Wrap,
    Clip,
}

/// Bounded display text. Editable text and rich spans are intentionally outside this contract.
#[derive(Clone, Debug, PartialEq)]
pub struct UiText {
    pub(crate) content: String,
    pub(crate) logical_size: f32,
    pub(crate) brush: UiBrush,
    pub(crate) alignment: UiTextAlignment,
    pub(crate) overflow: UiTextOverflow,
    pub(crate) semantic_text: Option<String>,
}

impl UiText {
    pub fn new(content: impl Into<String>, logical_size: f32) -> Result<Self, UiStyleError> {
        if !logical_size.is_finite() || logical_size <= 0.0 || logical_size > 256.0 {
            return Err(UiStyleError::InvalidTextSize {
                value: logical_size,
            });
        }
        Ok(Self {
            content: content.into(),
            logical_size,
            brush: UiBrush::Role(UiColorRole::PrimaryText),
            alignment: UiTextAlignment::Start,
            overflow: UiTextOverflow::Wrap,
            semantic_text: None,
        })
    }

    #[must_use]
    pub fn with_brush(mut self, brush: UiBrush) -> Self {
        self.brush = brush;
        self
    }

    #[must_use]
    pub const fn with_alignment(mut self, alignment: UiTextAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    #[must_use]
    pub const fn with_overflow(mut self, overflow: UiTextOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    #[must_use]
    pub fn with_semantic_text(mut self, text: impl Into<String>) -> Self {
        self.semantic_text = Some(text.into());
        self
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[must_use]
    pub const fn logical_size(&self) -> f32 {
        self.logical_size
    }

    pub(crate) fn request(
        &self,
        wrap_width: Option<f32>,
        scale_factor: f32,
    ) -> Result<TextLayoutRequest, TextLayoutError> {
        let alignment = match self.alignment {
            UiTextAlignment::Start => TextAlignment::Start,
            UiTextAlignment::Center => TextAlignment::Center,
            UiTextAlignment::End => TextAlignment::End,
        };
        let mut request = TextLayoutRequest::new(self.content.clone(), self.logical_size)?
            .with_alignment(alignment)
            .with_scale_factor(scale_factor)?;
        if self.overflow == UiTextOverflow::Wrap
            && let Some(width) = wrap_width.filter(|width| *width > 0.0)
        {
            request = request.with_wrap_width(width)?;
        }
        Ok(request)
    }

    pub(crate) fn validate(&self) -> Result<(), UiStyleError> {
        if !self.logical_size.is_finite() || self.logical_size <= 0.0 || self.logical_size > 256.0 {
            return Err(UiStyleError::InvalidTextSize {
                value: self.logical_size,
            });
        }
        self.brush.validate()?;
        if matches!(&self.brush, UiBrush::Image { .. }) {
            return Err(UiStyleError::ImageTextUnsupported);
        }
        if self
            .semantic_text
            .as_ref()
            .is_some_and(|text| text.len() > 16 * 1024)
        {
            return Err(UiStyleError::SemanticTextTooLong);
        }
        Ok(())
    }
}

/// Typed colors for the first-party theme roles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiThemePalette {
    pub surface: UiColor,
    pub panel: UiColor,
    pub primary_text: UiColor,
    pub secondary_text: UiColor,
    pub accent: UiColor,
    pub positive: UiColor,
    pub warning: UiColor,
    pub danger: UiColor,
    pub disabled: UiColor,
    pub focus: UiColor,
}

impl UiThemePalette {
    fn color(self, role: UiColorRole) -> UiColor {
        match role {
            UiColorRole::Surface => self.surface,
            UiColorRole::Panel => self.panel,
            UiColorRole::PrimaryText => self.primary_text,
            UiColorRole::SecondaryText => self.secondary_text,
            UiColorRole::Accent => self.accent,
            UiColorRole::Positive => self.positive,
            UiColorRole::Warning => self.warning,
            UiColorRole::Danger => self.danger,
            UiColorRole::Disabled => self.disabled,
            UiColorRole::Focus => self.focus,
        }
    }

    fn validate(self) -> Result<(), UiStyleError> {
        for color in [
            self.surface,
            self.panel,
            self.primary_text,
            self.secondary_text,
            self.accent,
            self.positive,
            self.warning,
            self.danger,
            self.disabled,
            self.focus,
        ] {
            color.validate()?;
        }
        Ok(())
    }
}

impl Default for UiThemePalette {
    fn default() -> Self {
        Self {
            surface: UiColor::rgba(0.04, 0.05, 0.06, 1.0),
            panel: UiColor::rgba(0.10, 0.12, 0.14, 1.0),
            primary_text: UiColor::rgba(0.94, 0.96, 0.97, 1.0),
            secondary_text: UiColor::rgba(0.66, 0.72, 0.75, 1.0),
            accent: UiColor::rgba(0.10, 0.72, 0.82, 1.0),
            positive: UiColor::rgba(0.19, 0.72, 0.38, 1.0),
            warning: UiColor::rgba(0.93, 0.66, 0.18, 1.0),
            danger: UiColor::rgba(0.88, 0.25, 0.23, 1.0),
            disabled: UiColor::rgba(0.40, 0.43, 0.45, 1.0),
            focus: UiColor::rgba(0.95, 0.82, 0.31, 1.0),
        }
    }
}

/// Immutable revisioned theme snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTheme {
    revision: UiRevision,
    pub(crate) palette: UiThemePalette,
}

impl UiTheme {
    pub fn new(revision: u64, palette: UiThemePalette) -> Result<Self, UiStyleError> {
        let revision = UiRevision::new(revision).ok_or(UiStyleError::ZeroRevision)?;
        palette.validate()?;
        Ok(Self { revision, palette })
    }

    #[must_use]
    pub const fn revision(self) -> UiRevision {
        self.revision
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            revision: UiRevision(1),
            palette: UiThemePalette::default(),
        }
    }
}

fn validate_non_negative(field: &'static str, value: f32) -> Result<(), UiStyleError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(UiStyleError::InvalidFiniteValue { field, value })
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum UiStyleError {
    #[error("UI style field {field} has invalid finite value {value}")]
    InvalidFiniteValue { field: &'static str, value: f32 },
    #[error("UI percentage {value} is outside 0.0..=1.0")]
    InvalidPercent { value: f32 },
    #[error("UI z-index {value} is outside the supported range")]
    ZIndexOutOfRange { value: i32 },
    #[error("UI color contains a non-finite or out-of-range channel")]
    InvalidColor,
    #[error("UI text size {value} is outside (0, 256]")]
    InvalidTextSize { value: f32 },
    #[error("UI semantic text exceeds 16 KiB")]
    SemanticTextTooLong,
    #[error("UI image brushes are not supported for borders")]
    ImageBorderUnsupported,
    #[error("UI image brushes are not supported for text")]
    ImageTextUnsupported,
    #[error("UI revision must be non-zero")]
    ZeroRevision,
}
