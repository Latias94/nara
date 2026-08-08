use std::sync::Arc;

use nara_text::{TextEngine, TextLayout, TextLayoutError, TextLayoutRequest};

#[derive(Debug)]
pub(super) enum TextCompileError {
    SourceBytesLimitExceeded { actual: usize, maximum: usize },
    ShapedGlyphLimitExceeded { actual: usize, maximum: usize },
    Layout(TextLayoutError),
}

/// Candidate-local owner of expensive shaping work and reusable immutable layouts.
pub(super) struct TextCompileSession {
    layouts: Vec<(TextLayoutRequest, Arc<TextLayout>)>,
    used_source_bytes: usize,
    max_source_bytes: usize,
    used_shape_units: usize,
    max_shape_units: usize,
}

impl TextCompileSession {
    pub(super) fn new(max_source_bytes: usize, max_shape_units: usize) -> Self {
        Self {
            layouts: Vec::new(),
            used_source_bytes: 0,
            max_source_bytes,
            used_shape_units: 0,
            max_shape_units,
        }
    }

    pub(super) fn layout(
        &mut self,
        engine: &mut TextEngine,
        request: TextLayoutRequest,
    ) -> Result<Arc<TextLayout>, TextCompileError> {
        if let Some((_, layout)) = self.layouts.iter().find(|(cached, _)| cached == &request) {
            return Ok(Arc::clone(layout));
        }

        let source_bytes = self.used_source_bytes.saturating_add(request.text().len());
        if source_bytes > self.max_source_bytes {
            return Err(TextCompileError::SourceBytesLimitExceeded {
                actual: source_bytes,
                maximum: self.max_source_bytes,
            });
        }

        // Unicode scalar count is a conservative pre-shape work unit. The exact glyph count is
        // checked again below because shaping may expand or contract the source sequence.
        let scalar_units = request.text().chars().count();
        let preflight_shape_units = self.used_shape_units.saturating_add(scalar_units);
        if preflight_shape_units > self.max_shape_units {
            return Err(TextCompileError::ShapedGlyphLimitExceeded {
                actual: preflight_shape_units,
                maximum: self.max_shape_units,
            });
        }

        let remaining_glyphs = self.max_shape_units.saturating_sub(self.used_shape_units);
        let layout = match engine.layout_with_glyph_limit(&request, remaining_glyphs) {
            Ok(layout) => layout,
            Err(TextLayoutError::GlyphLimitExceeded { actual, .. }) => {
                return Err(TextCompileError::ShapedGlyphLimitExceeded {
                    actual: self.used_shape_units.saturating_add(actual),
                    maximum: self.max_shape_units,
                });
            }
            Err(error) => return Err(TextCompileError::Layout(error)),
        };
        let consumed_shape_units = scalar_units.max(layout.glyphs().len());
        let used_shape_units = self.used_shape_units.saturating_add(consumed_shape_units);
        if used_shape_units > self.max_shape_units {
            return Err(TextCompileError::ShapedGlyphLimitExceeded {
                actual: used_shape_units,
                maximum: self.max_shape_units,
            });
        }

        let layout = Arc::new(layout);
        self.used_source_bytes = source_bytes;
        self.used_shape_units = used_shape_units;
        self.layouts.push((request, Arc::clone(&layout)));
        Ok(layout)
    }
}
