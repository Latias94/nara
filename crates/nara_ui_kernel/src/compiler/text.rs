use std::{collections::VecDeque, sync::Arc};

use nara_text::{
    TextEngine, TextLayout, TextLayoutError, TextLayoutRequest, TextPaintSnapshot,
    TextWidthMeasurement, TextWindowError,
};

#[derive(Debug)]
pub(super) enum TextCompileError {
    SourceBytesLimitExceeded { actual: usize, maximum: usize },
    ShapedGlyphLimitExceeded { actual: usize, maximum: usize },
    Layout(TextLayoutError),
    Window(TextWindowError),
}

const MAX_CACHED_TEXT_LAYOUTS: usize = 256;

#[derive(Clone)]
enum CachedText {
    Layout(Arc<TextLayout>),
    Width(TextWidthMeasurement),
}

impl CachedText {
    fn glyph_count(&self) -> usize {
        match self {
            Self::Layout(layout) => layout.glyphs().len(),
            Self::Width(measured) => measured.glyph_count(),
        }
    }
}

pub(super) struct TextLayoutCache {
    entries: VecDeque<(TextLayoutRequest, CachedText)>,
    source_bytes: usize,
    glyphs: usize,
    max_entries: usize,
    max_source_bytes: usize,
    max_glyphs: usize,
}

impl TextLayoutCache {
    pub(super) fn new(max_entries: usize, max_source_bytes: usize, max_glyphs: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            source_bytes: 0,
            glyphs: 0,
            max_entries: max_entries.min(MAX_CACHED_TEXT_LAYOUTS),
            max_source_bytes,
            max_glyphs,
        }
    }

    fn get(&mut self, request: &TextLayoutRequest) -> Option<Arc<TextLayout>> {
        match self.get_value(request, false)? {
            CachedText::Layout(layout) => Some(layout),
            CachedText::Width(_) => None,
        }
    }

    fn get_width(&mut self, request: &TextLayoutRequest) -> Option<TextWidthMeasurement> {
        match self.get_value(request, true)? {
            CachedText::Width(measured) => Some(measured),
            CachedText::Layout(_) => None,
        }
    }

    fn get_value(&mut self, request: &TextLayoutRequest, width: bool) -> Option<CachedText> {
        let index = self.entries.iter().position(|(cached, value)| {
            cached == request && matches!(value, CachedText::Width(_)) == width
        })?;
        let entry = self.entries.remove(index)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }

    fn insert(&mut self, request: TextLayoutRequest, layout: Arc<TextLayout>) {
        self.insert_value(request, CachedText::Layout(layout));
    }

    fn insert_value(&mut self, request: TextLayoutRequest, value: CachedText) {
        let source_bytes = request.text().len();
        let glyphs = value.glyph_count();
        if self.max_entries == 0 || source_bytes > self.max_source_bytes || glyphs > self.max_glyphs
        {
            return;
        }
        while self.entries.len() >= self.max_entries
            || self.source_bytes.saturating_add(source_bytes) > self.max_source_bytes
            || self.glyphs.saturating_add(glyphs) > self.max_glyphs
        {
            let Some((request, value)) = self.entries.pop_front() else {
                break;
            };
            self.source_bytes -= request.text().len();
            self.glyphs -= value.glyph_count();
        }
        self.source_bytes += source_bytes;
        self.glyphs += glyphs;
        self.entries.push_back((request, value));
    }
}

/// Candidate-local source admission and expensive new shaping work.
///
/// Prepared sources pay full bytes once per immutable source identity, separately from ordinary
/// request-cache entries. They perform no new shaping. Each final prepared draw scans/rasterizes
/// at most one receiver-admitted full layout; the candidate's node ceiling bounds draw count.
/// Output glyphs (including repeated draws) are charged by the root/frame output budget.
/// Width observations share the same source/glyph and retained-cache limits, but cannot satisfy
/// a drawable-layout request. The engine's immutable fonts and complete request bind both kinds.
pub(super) struct TextCompileSession {
    cache: TextLayoutCache,
    layouts: Vec<(TextLayoutRequest, Result<Arc<TextLayout>, TextLayoutError>)>,
    widths: Vec<(
        TextLayoutRequest,
        Result<TextWidthMeasurement, TextLayoutError>,
    )>,
    prepared: Vec<(TextPaintSnapshot, Result<(), TextWindowError>)>,
    used_source_bytes: usize,
    max_source_bytes: usize,
    used_shaped_glyphs: usize,
    max_shaped_glyphs: usize,
}

impl TextCompileSession {
    pub(super) fn new(
        cache: TextLayoutCache,
        max_source_bytes: usize,
        max_shaped_glyphs: usize,
    ) -> Self {
        Self {
            cache,
            layouts: Vec::new(),
            widths: Vec::new(),
            prepared: Vec::new(),
            used_source_bytes: 0,
            max_source_bytes,
            used_shaped_glyphs: 0,
            max_shaped_glyphs,
        }
    }

    pub(super) fn into_cache(self) -> TextLayoutCache {
        self.cache
    }

    pub(super) fn admit_prepared(
        &mut self,
        engine: &TextEngine,
        snapshot: &TextPaintSnapshot,
        scale_factor: f32,
    ) -> Result<(), TextCompileError> {
        if let Some((_, result)) = self
            .prepared
            .iter()
            .find(|(cached, _)| cached.same_source(snapshot))
        {
            return result.clone().map_err(TextCompileError::Window);
        }
        let actual = self
            .used_source_bytes
            .saturating_add(snapshot.display_text().len());
        if actual > self.max_source_bytes {
            return Err(TextCompileError::SourceBytesLimitExceeded {
                actual,
                maximum: self.max_source_bytes,
            });
        }
        self.used_source_bytes = actual;
        let result = engine.validate_paint_snapshot(snapshot, scale_factor);
        self.prepared.push((snapshot.clone(), result.clone()));
        result.map_err(TextCompileError::Window)
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
        let layout = if let Some(layout) = self.cache.get(&request) {
            if layout.glyphs().len() > remaining_glyphs {
                return Err(TextCompileError::ShapedGlyphLimitExceeded {
                    actual: self.max_shaped_glyphs.saturating_add(1),
                    maximum: self.max_shaped_glyphs,
                });
            }
            layout
        } else {
            let layout = match engine.layout_with_glyph_limit(&request, remaining_glyphs) {
                Ok(layout) => Arc::new(layout),
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
            self.cache.insert(request.clone(), Arc::clone(&layout));
            layout
        };
        let used_shaped_glyphs = self
            .used_shaped_glyphs
            .saturating_add(layout.glyphs().len());

        self.used_shaped_glyphs = used_shaped_glyphs;
        self.layouts.push((request, Ok(Arc::clone(&layout))));
        Ok(layout)
    }

    pub(super) fn measure_for_width(
        &mut self,
        engine: &mut TextEngine,
        request: TextLayoutRequest,
    ) -> Result<TextWidthMeasurement, TextCompileError> {
        if let Some((_, result)) = self.widths.iter().find(|(cached, _)| cached == &request) {
            return result.clone().map_err(TextCompileError::Layout);
        }
        let actual = self.used_source_bytes.saturating_add(request.text().len());
        if actual > self.max_source_bytes {
            return Err(TextCompileError::SourceBytesLimitExceeded {
                actual,
                maximum: self.max_source_bytes,
            });
        }
        self.used_source_bytes = actual;
        let remaining = self
            .max_shaped_glyphs
            .saturating_sub(self.used_shaped_glyphs);
        let measured = if let Some(measured) = self.cache.get_width(&request) {
            if measured.glyph_count() > remaining {
                return Err(TextCompileError::ShapedGlyphLimitExceeded {
                    actual: self.max_shaped_glyphs.saturating_add(1),
                    maximum: self.max_shaped_glyphs,
                });
            }
            measured
        } else {
            let measured = match engine.measure_for_width_with_glyph_limit(&request, remaining) {
                Ok(measured) => measured,
                Err(TextLayoutError::GlyphLimitExceeded { actual, .. }) => {
                    return Err(TextCompileError::ShapedGlyphLimitExceeded {
                        actual: self.used_shaped_glyphs.saturating_add(actual),
                        maximum: self.max_shaped_glyphs,
                    });
                }
                Err(error) => {
                    self.widths.push((request, Err(error.clone())));
                    return Err(TextCompileError::Layout(error));
                }
            };
            self.cache
                .insert_value(request.clone(), CachedText::Width(measured));
            measured
        };
        self.used_shaped_glyphs = self
            .used_shaped_glyphs
            .saturating_add(measured.glyph_count());
        self.widths.push((request, Ok(measured)));
        Ok(measured)
    }
}
