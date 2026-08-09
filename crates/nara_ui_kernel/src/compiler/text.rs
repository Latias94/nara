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
    layouts: Vec<(TextLayoutRequest, Result<Arc<TextLayout>, TextLayoutError>)>,
    used_source_bytes: usize,
    max_source_bytes: usize,
    used_shaped_glyphs: usize,
    max_shaped_glyphs: usize,
}

impl TextCompileSession {
    pub(super) fn new(max_source_bytes: usize, max_shaped_glyphs: usize) -> Self {
        Self {
            layouts: Vec::new(),
            used_source_bytes: 0,
            max_source_bytes,
            used_shaped_glyphs: 0,
            max_shaped_glyphs,
        }
    }

    pub(super) fn layout(
        &mut self,
        engine: &mut TextEngine,
        request: TextLayoutRequest,
    ) -> Result<Arc<TextLayout>, TextCompileError> {
        if let Some((_, cached_layout)) = self
            .layouts
            .iter()
            .find(|(cached_request, _)| cached_request == &request)
        {
            return match cached_layout {
                Ok(layout) => Ok(Arc::clone(layout)),
                Err(error) => Err(TextCompileError::Layout(error.clone())),
            };
        }

        // Reserve input work before asking the shaping engine to do anything. A deterministic
        // layout failure still consumes this candidate-wide work budget; only an exact cached
        // request can avoid paying for the same failed operation twice.
        let attempted_source_bytes = self.used_source_bytes.saturating_add(request.text().len());
        if attempted_source_bytes > self.max_source_bytes {
            return Err(TextCompileError::SourceBytesLimitExceeded {
                actual: attempted_source_bytes,
                maximum: self.max_source_bytes,
            });
        }
        self.used_source_bytes = attempted_source_bytes;

        let remaining_glyphs = self
            .max_shaped_glyphs
            .saturating_sub(self.used_shaped_glyphs);
        let layout = match engine.layout_with_glyph_limit(&request, remaining_glyphs) {
            Ok(layout) => layout,
            Err(TextLayoutError::GlyphLimitExceeded { actual, .. }) => {
                return Err(TextCompileError::ShapedGlyphLimitExceeded {
                    actual: self.used_shaped_glyphs.saturating_add(actual),
                    maximum: self.max_shaped_glyphs,
                });
            }
            Err(error) => {
                self.layouts.push((request, Err(error.clone())));
                return Err(TextCompileError::Layout(error));
            }
        };
        let used_shaped_glyphs = self
            .used_shaped_glyphs
            .saturating_add(layout.glyphs().len());

        let layout = Arc::new(layout);
        self.used_shaped_glyphs = used_shaped_glyphs;
        self.layouts.push((request, Ok(Arc::clone(&layout))));
        Ok(layout)
    }
}
