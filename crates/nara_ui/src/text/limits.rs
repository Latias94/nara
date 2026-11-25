/// Resource limits enforced before text results are published.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLimits {
    pub max_text_bytes: usize,
    pub max_lines: usize,
    pub max_glyphs: usize,
    pub max_diagnostics: usize,
    pub max_glyph_extent: u32,
    pub max_scale_factor: f32,
}

const HARD_MAX_TEXT_BYTES: usize = 16 * 1024;
const HARD_MAX_LINES: usize = 256;
const HARD_MAX_GLYPHS: usize = 4_096;
const HARD_MAX_DIAGNOSTICS: usize = 64;
const HARD_MAX_GLYPH_EXTENT: u32 = 256;
const HARD_MAX_SCALE_FACTOR: f32 = 8.0;

impl Default for TextLimits {
    fn default() -> Self {
        Self {
            max_text_bytes: HARD_MAX_TEXT_BYTES,
            max_lines: HARD_MAX_LINES,
            max_glyphs: HARD_MAX_GLYPHS,
            max_diagnostics: HARD_MAX_DIAGNOSTICS,
            max_glyph_extent: HARD_MAX_GLYPH_EXTENT,
            max_scale_factor: HARD_MAX_SCALE_FACTOR,
        }
    }
}

impl TextLimits {
    pub(crate) fn invalid_field(self) -> Option<&'static str> {
        if self.max_text_bytes == 0 || self.max_text_bytes > HARD_MAX_TEXT_BYTES {
            Some("max_text_bytes")
        } else if self.max_lines == 0 || self.max_lines > HARD_MAX_LINES {
            Some("max_lines")
        } else if self.max_glyphs == 0 || self.max_glyphs > HARD_MAX_GLYPHS {
            Some("max_glyphs")
        } else if self.max_diagnostics == 0 || self.max_diagnostics > HARD_MAX_DIAGNOSTICS {
            Some("max_diagnostics")
        } else if self.max_glyph_extent == 0 || self.max_glyph_extent > HARD_MAX_GLYPH_EXTENT {
            Some("max_glyph_extent")
        } else if !self.max_scale_factor.is_finite()
            || self.max_scale_factor <= 0.0
            || self.max_scale_factor > HARD_MAX_SCALE_FACTOR
        {
            Some("max_scale_factor")
        } else {
            None
        }
    }
}
