use std::ops::Range;

use thiserror::Error;

use crate::FontFaceId;

const MAX_REQUEST_LOGICAL_SIZE: f32 = 256.0;
const MAX_REQUEST_SCALE_FACTOR: f32 = 64.0;
const MAX_REQUEST_WRAP_WIDTH: f32 = 1_000_000.0;

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
    DiagnosticsTruncated { omitted: usize },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayoutRequest {
    text: String,
    logical_size: f32,
    wrap_width: Option<f32>,
    alignment: TextAlignment,
    scale_factor: f32,
}

impl TextLayoutRequest {
    pub fn new(text: impl Into<String>, logical_size: f32) -> Result<Self, TextLayoutError> {
        if !logical_size.is_finite()
            || logical_size <= 0.0
            || logical_size > MAX_REQUEST_LOGICAL_SIZE
        {
            return Err(TextLayoutError::InvalidLogicalSize {
                value: logical_size,
            });
        }
        Ok(Self {
            text: text.into(),
            logical_size,
            wrap_width: None,
            alignment: TextAlignment::Start,
            scale_factor: 1.0,
        })
    }

    pub fn with_wrap_width(mut self, width: f32) -> Result<Self, TextLayoutError> {
        if !width.is_finite() || width <= 0.0 || width > MAX_REQUEST_WRAP_WIDTH {
            return Err(TextLayoutError::InvalidWrapWidth { value: width });
        }
        self.wrap_width = Some(width);
        Ok(self)
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
        self.logical_size
    }

    #[must_use]
    pub const fn wrap_width(&self) -> Option<f32> {
        self.wrap_width
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
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) lines: Vec<TextLine>,
    pub(crate) runs: Vec<TextRun>,
    pub(crate) glyphs: Vec<PositionedGlyph>,
    pub(crate) diagnostics: Vec<TextDiagnostic>,
}

impl TextLayout {
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
    #[error("text scale factor {value} is invalid")]
    InvalidScaleFactor { value: f32 },
    #[error("text wrap width {value} is invalid")]
    InvalidWrapWidth { value: f32 },
    #[error("text has {actual} bytes; maximum is {maximum}")]
    TextBytesExceeded { actual: usize, maximum: usize },
    #[error("layout has {actual} lines; maximum is {maximum}")]
    LineLimitExceeded { actual: usize, maximum: usize },
    #[error("layout has at least {actual} glyphs; maximum is {maximum}")]
    GlyphLimitExceeded { actual: usize, maximum: usize },
    #[error("text scale factor {actual} exceeds maximum {maximum}")]
    ScaleFactorExceeded { actual: f32, maximum: f32 },
    #[error("shaped glyph identifier {glyph_id} cannot be represented")]
    GlyphIdOutOfRange { glyph_id: u32 },
    #[error("text layout produced non-finite metrics")]
    NonFiniteResult,
}
