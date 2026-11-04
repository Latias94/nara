use std::{collections::HashMap, sync::Arc};

use thiserror::Error;

use crate::{FontFaceId, GlyphRasterError, TextEngine, TextLayout, TextLayoutError, TextRect};

/// The complete display source and its immutable editing-layout provenance.
///
/// Only [`crate::TextEditor::paint_snapshot`] publishes these snapshots. They retain the
/// complete positioned layout, including visual order and font selection, without reshaping.
/// A snapshot is input to window preparation, not a render resource or an unchecked layout.
#[derive(Clone, Debug, PartialEq)]
pub struct TextPaintSnapshot {
    inner: Arc<PaintSource>,
}

#[derive(Debug, PartialEq)]
struct PaintSource {
    text: Arc<str>,
    font_ids: Arc<[FontFaceId]>,
    logical_size: f32,
    layout: TextLayout,
}

impl TextPaintSnapshot {
    pub(crate) fn new(
        text: &str,
        font_ids: &[FontFaceId],
        logical_size: f32,
        layout: TextLayout,
    ) -> Self {
        Self {
            inner: Arc::new(PaintSource {
                text: Arc::from(text),
                font_ids: Arc::from(font_ids),
                logical_size,
                layout,
            }),
        }
    }

    #[must_use]
    pub fn display_text(&self) -> &str {
        &self.inner.text
    }

    #[must_use]
    pub fn width(&self) -> f32 {
        self.inner.layout.width()
    }

    #[must_use]
    pub fn height(&self) -> f32 {
        self.inner.layout.height()
    }

    #[must_use]
    pub fn glyph_count(&self) -> usize {
        self.inner.layout.glyphs().len()
    }

    /// Whether two handles retain the same published source, independent of their draw window.
    #[must_use]
    pub fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub(crate) fn layout(&self) -> &TextLayout {
        &self.inner.layout
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TextWindowError {
    #[error("prepared text requires the original complete ordered font set")]
    FontSetMismatch,
    #[error("text window must have finite ordered clip edges and finite placement")]
    InvalidWindow,
    #[error("prepared text has {actual} diagnostics; maximum is {maximum}")]
    DiagnosticLimitExceeded { actual: usize, maximum: usize },
    #[error(transparent)]
    Layout(#[from] TextLayoutError),
    #[error(transparent)]
    Raster(#[from] GlyphRasterError),
}

impl TextEngine {
    /// Admits the whole source under this receiver's policy, even for an empty paint window.
    /// This performs no shaping and does not bypass the complete source's byte/line/glyph limits.
    pub fn validate_paint_snapshot(
        &self,
        snapshot: &TextPaintSnapshot,
        target_scale: f32,
    ) -> Result<(), TextWindowError> {
        if !self.font_ids().eq(snapshot.inner.font_ids.iter().copied()) {
            return Err(TextWindowError::FontSetMismatch);
        }
        if snapshot.display_text().len() > self.limits.max_text_bytes {
            return Err(TextLayoutError::TextBytesExceeded {
                actual: snapshot.display_text().len(),
                maximum: self.limits.max_text_bytes,
            }
            .into());
        }
        if !target_scale.is_finite() || target_scale <= 0.0 {
            return Err(TextLayoutError::InvalidScaleFactor {
                value: target_scale,
            }
            .into());
        }
        if target_scale > self.limits.max_scale_factor {
            return Err(TextLayoutError::ScaleFactorExceeded {
                actual: target_scale,
                maximum: self.limits.max_scale_factor,
            }
            .into());
        }
        let layout = snapshot.layout();
        if layout.lines().len() > self.limits.max_lines {
            return Err(TextLayoutError::LineLimitExceeded {
                actual: layout.lines().len(),
                maximum: self.limits.max_lines,
            }
            .into());
        }
        if layout.glyphs().len() > self.limits.max_glyphs {
            return Err(TextLayoutError::GlyphLimitExceeded {
                actual: layout.glyphs().len(),
                maximum: self.limits.max_glyphs,
            }
            .into());
        }
        if layout.diagnostics().len() > self.limits.max_diagnostics {
            return Err(TextWindowError::DiagnosticLimitExceeded {
                actual: layout.diagnostics().len(),
                maximum: self.limits.max_diagnostics,
            });
        }
        Ok(())
    }

    /// Prepares a clip in local logical coordinates at a zero draw/surface origin.
    ///
    /// Complete glyphs keep their original visual order and coordinates; only their raster keys
    /// are rebuilt for the target scale. The result retains full metrics but owns only visible
    /// glyphs and reindexed runs, with no reference back to the complete source layout.
    pub fn prepare_window(
        &mut self,
        snapshot: &TextPaintSnapshot,
        local_clip: TextRect,
        target_scale: f32,
    ) -> Result<Arc<TextLayout>, TextWindowError> {
        self.prepare_window_at(snapshot, local_clip, target_scale, [0.0; 2], [0.0; 2])
    }

    /// Prepares the same window with exact renderer placement.
    ///
    /// `clip` and `origin` use the target's logical coordinates; `physical_origin` is the physical
    /// content origin. Baselines use `physical_origin + (origin + glyph_position) * target_scale`,
    /// rounded to the nearest integer, just as the UI renderer does. Raster subpixel phases use
    /// those same unsnapped baselines. Clip edges use the same mapping and rounding.
    ///
    /// Each call scans at most the receiver's admitted full glyph count and rasterizes each distinct
    /// key once to obtain actual ink placement, including overhang and zero-advance marks. It
    /// makes no shaping or performance-cache claim. Raster/extent failures publish no window.
    pub fn prepare_window_at(
        &mut self,
        snapshot: &TextPaintSnapshot,
        clip: TextRect,
        target_scale: f32,
        origin: [f32; 2],
        physical_origin: [f32; 2],
    ) -> Result<Arc<TextLayout>, TextWindowError> {
        self.work_stats.paint_window_attempts =
            self.work_stats.paint_window_attempts.saturating_add(1);
        self.validate_paint_snapshot(snapshot, target_scale)?;
        if ![
            clip.x0,
            clip.y0,
            clip.x1,
            clip.y1,
            origin[0],
            origin[1],
            physical_origin[0],
            physical_origin[1],
        ]
        .into_iter()
        .all(f32::is_finite)
            || clip.x0 > clip.x1
            || clip.y0 > clip.y1
        {
            return Err(TextWindowError::InvalidWindow);
        }
        let clip = [
            physical_origin[0] + clip.x0 * target_scale,
            physical_origin[1] + clip.y0 * target_scale,
            physical_origin[0] + clip.x1 * target_scale,
            physical_origin[1] + clip.y1 * target_scale,
        ];
        if !clip.into_iter().all(f32::is_finite) {
            return Err(TextWindowError::InvalidWindow);
        }
        let clip = clip.map(snap);
        let source = snapshot.layout();
        let mut output = TextLayout {
            width: source.width,
            height: source.height,
            lines: source.lines.clone(),
            runs: Vec::new(),
            glyphs: Vec::new(),
            diagnostics: source.diagnostics.clone(),
        };
        // Call-local placement reuse bounds retained metadata by the admitted full glyph count.
        // Masks are dropped immediately; atlas/resource admission remains the renderer's owner.
        let mut footprints = HashMap::new();
        for run in source.runs() {
            let glyph_start = output.glyphs.len();
            for glyph in &source.glyphs()[run.glyph_range()] {
                self.work_stats.paint_window_glyphs_scanned = self
                    .work_stats
                    .paint_window_glyphs_scanned
                    .saturating_add(1);
                let baseline = [
                    physical_origin[0] + (origin[0] + glyph.x()) * target_scale,
                    physical_origin[1] + (origin[1] + glyph.y()) * target_scale,
                ];
                if !baseline.into_iter().all(f32::is_finite) {
                    return Err(TextWindowError::InvalidWindow);
                }
                let mut glyph = glyph.clone();
                glyph.key.physical_size_bits =
                    (snapshot.inner.logical_size * target_scale).to_bits();
                glyph.key.subpixel_x = crate::engine::quarter_pixel(baseline[0]);
                glyph.key.subpixel_y = crate::engine::quarter_pixel(baseline[1]);
                let (left, top, width, height) = match footprints.get(&glyph.key()).copied() {
                    Some(footprint) => footprint,
                    None => {
                        let raster = self.rasterize(glyph.key())?;
                        let footprint =
                            (raster.left(), raster.top(), raster.width(), raster.height());
                        footprints.insert(glyph.key(), footprint);
                        footprint
                    }
                };
                let x0 = snap(baseline[0]).saturating_add(left);
                let y0 = snap(baseline[1]).saturating_sub(top);
                let x1 = x0.saturating_add(width as i32);
                let y1 = y0.saturating_add(height as i32);
                if clip[0] < clip[2]
                    && clip[1] < clip[3]
                    && x0 < x1
                    && y0 < y1
                    && x0 < clip[2]
                    && clip[0] < x1
                    && y0 < clip[3]
                    && clip[1] < y1
                {
                    glyph.run_index = output.runs.len();
                    output.glyphs.push(glyph);
                }
            }
            if output.glyphs.len() != glyph_start {
                let mut run = run.clone();
                run.glyph_range = glyph_start..output.glyphs.len();
                output.runs.push(run);
            }
        }
        Ok(Arc::new(output))
    }
}

fn snap(value: f32) -> i32 {
    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}
