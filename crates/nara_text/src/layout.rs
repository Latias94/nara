use std::ops::Range;

use icu_segmenter::GraphemeClusterSegmenter;
use thiserror::Error;

use crate::FontFaceId;

const MAX_REQUEST_LOGICAL_SIZE: f32 = 256.0;
const MAX_REQUEST_SCALE_FACTOR: f32 = 64.0;
const MAX_REQUEST_WRAP_WIDTH: f32 = 1_000_000.0;
pub const MAX_GRAPHEME_SCALARS: usize = 32;

/// Validated logical font size and optional absolute logical line height.
///
/// `None` uses the actual selected fonts' ascent, descent and leading. An explicit height
/// controls line spacing, not glyph scaling or ink bounds; smaller heights can overlap ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextTypography {
    logical_size: f32,
    line_height: Option<f32>,
}

impl TextTypography {
    pub fn new(logical_size: f32) -> Result<Self, TextLayoutError> {
        if !logical_size.is_finite()
            || logical_size <= 0.0
            || logical_size > MAX_REQUEST_LOGICAL_SIZE
        {
            return Err(TextLayoutError::InvalidLogicalSize {
                value: logical_size,
            });
        }
        Ok(Self {
            logical_size,
            line_height: None,
        })
    }

    pub fn with_line_height(mut self, height: f32) -> Result<Self, TextLayoutError> {
        if !height.is_finite() || height <= 0.0 {
            return Err(TextLayoutError::InvalidLineHeight { value: height });
        }
        self.line_height = Some(height);
        Ok(self)
    }

    #[must_use]
    pub const fn with_auto_line_height(mut self) -> Self {
        self.line_height = None;
        self
    }

    #[must_use]
    pub const fn logical_size(self) -> f32 {
        self.logical_size
    }

    #[must_use]
    pub const fn line_height(self) -> Option<f32> {
        self.line_height
    }

    pub(crate) fn parley_line_height(self) -> parley::LineHeight {
        self.line_height
            .map_or_else(parley::LineHeight::default, parley::LineHeight::Absolute)
    }
}

pub(crate) fn validate_graphemes(text: &str) -> Result<(), TextLayoutError> {
    let mut boundaries = GraphemeClusterSegmenter::new().segment_str(text);
    let mut start = boundaries.next().unwrap_or(0);
    for end in boundaries {
        let actual = text[start..end].chars().count();
        if actual > MAX_GRAPHEME_SCALARS {
            return Err(TextLayoutError::GraphemeLimitExceeded {
                byte_index: start,
                actual,
                maximum: MAX_GRAPHEME_SCALARS,
            });
        }
        start = end;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlignment {
    #[default]
    Start,
    Center,
    End,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextDiagnostic {
    MissingGlyph { scalar: char },
    MissingGlyphSequence { text_range: Range<usize> },
    DiagnosticsTruncated { omitted: usize },
}

#[derive(Clone, Debug, PartialEq)]
enum TextWrapping {
    Natural,
    Width(f32),
    MinContent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayoutRequest {
    text: String,
    typography: TextTypography,
    wrapping: TextWrapping,
    alignment: TextAlignment,
    scale_factor: f32,
}

impl TextLayoutRequest {
    pub fn new(text: impl Into<String>, logical_size: f32) -> Result<Self, TextLayoutError> {
        Ok(Self::from_typography(
            text,
            TextTypography::new(logical_size)?,
        ))
    }

    /// Constructs a request from an already validated typography value.
    #[must_use]
    pub fn from_typography(text: impl Into<String>, typography: TextTypography) -> Self {
        Self {
            text: text.into(),
            typography,
            wrapping: TextWrapping::Natural,
            alignment: TextAlignment::Start,
            scale_factor: 1.0,
        }
    }

    #[must_use]
    pub const fn with_typography(mut self, typography: TextTypography) -> Self {
        self.typography = typography;
        self
    }

    pub fn with_wrap_width(mut self, width: f32) -> Result<Self, TextLayoutError> {
        if !width.is_finite() || width <= 0.0 || width > MAX_REQUEST_WRAP_WIDTH {
            return Err(TextLayoutError::InvalidWrapWidth { value: width });
        }
        self.wrapping = TextWrapping::Width(width);
        Ok(self)
    }

    /// Wraps at the minimum content width calculated from the complete shaped text.
    #[must_use]
    pub fn with_min_content(mut self) -> Self {
        self.wrapping = TextWrapping::MinContent;
        self
    }

    #[must_use]
    pub const fn with_alignment(mut self, alignment: TextAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn with_scale_factor(mut self, scale_factor: f32) -> Result<Self, TextLayoutError> {
        if !scale_factor.is_finite()
            || scale_factor <= 0.0
            || scale_factor > MAX_REQUEST_SCALE_FACTOR
        {
            return Err(TextLayoutError::InvalidScaleFactor {
                value: scale_factor,
            });
        }
        self.scale_factor = scale_factor;
        Ok(self)
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn logical_size(&self) -> f32 {
        self.typography.logical_size()
    }

    #[must_use]
    pub const fn typography(&self) -> TextTypography {
        self.typography
    }

    /// Returns the explicit finite wrap width, or `None` for natural or min-content layout.
    #[must_use]
    pub const fn wrap_width(&self) -> Option<f32> {
        match self.wrapping {
            TextWrapping::Width(width) => Some(width),
            TextWrapping::Natural | TextWrapping::MinContent => None,
        }
    }

    #[must_use]
    pub const fn is_min_content(&self) -> bool {
        matches!(self.wrapping, TextWrapping::MinContent)
    }

    #[must_use]
    pub const fn alignment(&self) -> TextAlignment {
        self.alignment
    }

    #[must_use]
    pub const fn scale_factor(&self) -> f32 {
        self.scale_factor
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphRasterKey {
    pub(crate) face_id: FontFaceId,
    pub(crate) glyph_id: u16,
    pub(crate) physical_size_bits: u32,
    pub(crate) subpixel_x: u8,
    pub(crate) subpixel_y: u8,
}

impl GlyphRasterKey {
    #[must_use]
    pub const fn face_id(self) -> FontFaceId {
        self.face_id
    }

    #[must_use]
    pub const fn glyph_id(self) -> u16 {
        self.glyph_id
    }

    #[must_use]
    pub const fn physical_size(self) -> f32 {
        f32::from_bits(self.physical_size_bits)
    }

    pub(crate) fn subpixel_offset(self) -> (f32, f32) {
        (
            f32::from(self.subpixel_x) * 0.25,
            f32::from(self.subpixel_y) * 0.25,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) advance: f32,
    pub(crate) run_index: usize,
    pub(crate) key: GlyphRasterKey,
}

impl PositionedGlyph {
    #[must_use]
    pub const fn x(&self) -> f32 {
        self.x
    }

    #[must_use]
    pub const fn y(&self) -> f32 {
        self.y
    }

    #[must_use]
    pub const fn advance(&self) -> f32 {
        self.advance
    }

    #[must_use]
    pub const fn run_index(&self) -> usize {
        self.run_index
    }

    #[must_use]
    pub const fn key(&self) -> GlyphRasterKey {
        self.key
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub(crate) line_index: usize,
    pub(crate) text_range: Range<usize>,
    pub(crate) glyph_range: Range<usize>,
    pub(crate) direction: TextDirection,
}

impl TextRun {
    #[must_use]
    pub const fn line_index(&self) -> usize {
        self.line_index
    }

    #[must_use]
    pub fn text_range(&self) -> Range<usize> {
        self.text_range.clone()
    }

    #[must_use]
    pub fn glyph_range(&self) -> Range<usize> {
        self.glyph_range.clone()
    }

    #[must_use]
    pub const fn direction(&self) -> TextDirection {
        self.direction
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub(crate) text_range: Range<usize>,
    pub(crate) baseline: f32,
    pub(crate) block_min: f32,
    pub(crate) block_max: f32,
    pub(crate) advance: f32,
}

impl TextLine {
    #[must_use]
    pub fn text_range(&self) -> Range<usize> {
        self.text_range.clone()
    }

    #[must_use]
    pub const fn baseline(&self) -> f32 {
        self.baseline
    }

    #[must_use]
    pub const fn block_min(&self) -> f32 {
        self.block_min
    }

    #[must_use]
    pub const fn block_max(&self) -> f32 {
        self.block_max
    }

    #[must_use]
    pub const fn advance(&self) -> f32 {
        self.advance
    }
}

/// Immutable owned output of one bounded layout operation.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub(crate) typography: TextTypography,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) lines: Vec<TextLine>,
    pub(crate) runs: Vec<TextRun>,
    pub(crate) glyphs: Vec<PositionedGlyph>,
    pub(crate) diagnostics: Vec<TextDiagnostic>,
}

impl TextLayout {
    #[must_use]
    pub const fn typography(&self) -> TextTypography {
        self.typography
    }

    #[must_use]
    pub const fn width(&self) -> f32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> f32 {
        self.height
    }

    #[must_use]
    pub fn lines(&self) -> &[TextLine] {
        &self.lines
    }

    #[must_use]
    pub fn runs(&self) -> &[TextRun] {
        &self.runs
    }

    #[must_use]
    pub fn glyphs(&self) -> &[PositionedGlyph] {
        &self.glyphs
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[TextDiagnostic] {
        &self.diagnostics
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TextLayoutError {
    #[error("logical text size {value} is invalid")]
    InvalidLogicalSize { value: f32 },
    #[error("logical text line height {value} must be finite and positive")]
    InvalidLineHeight { value: f32 },
    #[error("text scale factor {value} is invalid")]
    InvalidScaleFactor { value: f32 },
    #[error("text wrap width {value} is invalid")]
    InvalidWrapWidth { value: f32 },
    #[error("text has {actual} bytes; maximum is {maximum}")]
    TextBytesExceeded { actual: usize, maximum: usize },
    #[error("grapheme at byte {byte_index} has {actual} scalars; maximum is {maximum}")]
    GraphemeLimitExceeded {
        byte_index: usize,
        actual: usize,
        maximum: usize,
    },
    #[error("layout has {actual} lines; maximum is {maximum}")]
    LineLimitExceeded { actual: usize, maximum: usize },
    #[error("layout has at least {actual} glyphs; maximum is {maximum}")]
    GlyphLimitExceeded { actual: usize, maximum: usize },
    #[error("text scale factor {actual} exceeds maximum {maximum}")]
    ScaleFactorExceeded { actual: f32, maximum: f32 },
    #[error("shaped glyph identifier {glyph_id} cannot be represented")]
    GlyphIdOutOfRange { glyph_id: u32 },
    #[error("text layout selected a font face outside the admitted font set")]
    UnadmittedFontFace,
    #[error("text layout produced non-finite metrics")]
    NonFiniteResult,
}
