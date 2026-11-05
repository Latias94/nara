use std::{borrow::Cow, collections::BTreeSet, ops::Range};

use parley::{
    Alignment, AlignmentOptions, FontContext, FontData, FontFamily, FontFamilyName, Layout,
    LayoutContext, PositionedLayoutItem, StyleProperty, TextWrapMode,
    editing::PlainEditor,
    fontique::{Blob, Collection, CollectionOptions, FontInfoOverride, SourceCache},
};
use swash::{CacheKey, FontRef, scale::ScaleContext};
use thiserror::Error;

use crate::{
    FontFace, FontFaceId, GlyphRasterKey, PositionedGlyph, TextAlignment, TextDiagnostic,
    TextDirection, TextLayout, TextLayoutError, TextLayoutRequest, TextLimits, TextLine, TextRun,
    TextTypography,
    face::{MAX_FONT_FACES, MAX_TOTAL_FONT_BYTES},
};

/// Cumulative work observed by one [`TextEngine`], with every counter saturating at [`u64::MAX`].
///
/// These counters describe entered operations and produced work, including rejected results.
/// They do not affect admission, resource budgets, or publication of layout, raster, or editor state.
/// They are not timings, retained memory measurements, or permission to perform further work.
///
/// Layout counters cover only [`TextEngine::layout`] and [`TextEngine::layout_with_glyph_limit`].
/// [`crate::TextEditor`] construction, edits, and projection use Parley directly and are excluded,
/// as are font setup and validation-only calls. Window preparation includes its nested raster work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextWorkStats {
    /// Direct layout calls, including request validation failures. Delegation counts only once.
    pub layout_attempts: u64,
    /// UTF-8 source bytes supplied to direct layout calls, including refused requests.
    /// This is submitted input size, not a count of bytes scanned or retained.
    pub layout_source_bytes: u64,
    /// Glyphs exposed by complete line-broken Parley layouts before Nara result admission.
    /// Includes layouts refused during projection; request validation failures contribute none.
    pub shaped_glyphs: u64,
    /// Raster calls, including rejected keys and calls made while preparing paint windows.
    pub raster_attempts: u64,
    /// Image bytes produced by the rasterizer before content, extent, and mask admission.
    /// Includes discarded or refused images; absent images contribute no bytes.
    pub raster_output_bytes: u64,
    /// Window preparation calls, including source, placement, and raster failures.
    /// Delegation from `prepare_window` to `prepare_window_at` counts only once.
    pub paint_window_attempts: u64,
    /// Source glyphs entered by the window scan, including hidden glyphs, footprint reuse,
    /// and the glyph causing a failure. A failure stops counting at that glyph.
    pub paint_window_glyphs_scanned: u64,
}

pub struct TextEngine {
    primary_face_id: FontFaceId,
    fonts: Vec<AdmittedFont>,
    font_families: FontFamily<'static>,
    pub(crate) limits: TextLimits,
    pub(crate) font_context: FontContext,
    pub(crate) layout_context: LayoutContext<()>,
    scale_context: ScaleContext,
    pub(crate) work_stats: TextWorkStats,
}

struct AdmittedFont {
    face: FontFace,
    font_data: FontData,
    swash_offset: u32,
    swash_key: CacheKey,
}

impl TextEngine {
    pub fn new(face: FontFace, limits: TextLimits) -> Result<Self, TextEngineCreateError> {
        Self::with_fonts(vec![face], limits)
    }

    /// Admits an immutable ordered font set; the first face is the primary face.
    ///
    /// The byte budget counts each selected face's full container, including shared containers.
    /// Shaping only considers these explicit faces and never discovers system fonts.
    pub fn with_fonts(
        fonts: Vec<FontFace>,
        limits: TextLimits,
    ) -> Result<Self, TextEngineCreateError> {
        if let Some(field) = limits.invalid_field() {
            return Err(TextEngineCreateError::InvalidLimit { field });
        }
        let primary_face_id = fonts
            .first()
            .ok_or(TextEngineCreateError::EmptyFontSet)?
            .id();
        if fonts.len() > MAX_FONT_FACES {
            return Err(TextEngineCreateError::FontCountExceeded {
                actual: fonts.len(),
                maximum: MAX_FONT_FACES,
            });
        }
        let total_bytes = fonts.iter().map(FontFace::byte_len).sum();
        if total_bytes > MAX_TOTAL_FONT_BYTES {
            return Err(TextEngineCreateError::FontBytesExceeded {
                actual: total_bytes,
                maximum: MAX_TOTAL_FONT_BYTES,
            });
        }
        let mut face_ids = BTreeSet::new();
        for face in &fonts {
            if !face_ids.insert(face.id()) {
                return Err(TextEngineCreateError::DuplicateFontFace { face_id: face.id() });
            }
        }

        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let mut admitted_fonts = Vec::with_capacity(fonts.len());
        let mut font_families = Vec::with_capacity(fonts.len());
        for (font_index, face) in fonts.into_iter().enumerate() {
            let swash_font = FontRef::from_index(face.bytes(), face.face_index() as usize)
                .ok_or(TextEngineCreateError::RasterFontUnavailable { face_id: face.id() })?;
            let swash_offset = swash_font.offset;
            let swash_key = swash_font.key;
            let blob = shaping_blob(&face)?;
            let family_name = format!("nara-admitted-font-{font_index}");
            let registered = collection.register_fonts(
                blob.clone(),
                Some(FontInfoOverride {
                    family_name: Some(&family_name),
                    width: Some(Default::default()),
                    style: Some(Default::default()),
                    weight: Some(Default::default()),
                    ..FontInfoOverride::default()
                }),
            );
            if registered.len() != 1
                || registered[0].1.len() != 1
                || registered[0].1[0].index() != 0
            {
                return Err(TextEngineCreateError::FontRegistrationFailed { face_id: face.id() });
            }
            font_families.push(FontFamilyName::Named(Cow::Owned(family_name)));
            admitted_fonts.push(AdmittedFont {
                face,
                font_data: FontData::new(blob, 0),
                swash_offset,
                swash_key,
            });
        }
        let font_context = FontContext {
            collection,
            source_cache: SourceCache::default(),
        };

        Ok(Self {
            primary_face_id,
            fonts: admitted_fonts,
            font_families: FontFamily::List(Cow::Owned(font_families)),
            limits,
            font_context,
            layout_context: LayoutContext::new(),
            scale_context: ScaleContext::new(),
            work_stats: TextWorkStats::default(),
        })
    }

    /// Returns an independent cumulative work snapshot without changing engine state.
    ///
    /// See [`TextWorkStats`] for counting phases and the excluded Parley editor paths.
    #[must_use]
    pub const fn work_stats(&self) -> TextWorkStats {
        self.work_stats
    }

    #[must_use]
    pub const fn font_face_id(&self) -> FontFaceId {
        self.primary_face_id
    }

    #[must_use]
    pub fn contains_font(&self, face_id: FontFaceId) -> bool {
        self.fonts.iter().any(|font| font.face.id() == face_id)
    }

    pub(crate) fn font_ids(&self) -> impl Iterator<Item = FontFaceId> + '_ {
        self.fonts.iter().map(|font| font.face.id())
    }

    pub fn layout(&mut self, request: &TextLayoutRequest) -> Result<TextLayout, TextLayoutError> {
        self.layout_with_glyph_limit(request, self.limits.max_glyphs)
    }

    /// Shapes one request while respecting both the engine policy and a caller-owned allowance.
    ///
    /// The effective allowance is never wider than the limits used to construct this engine. A
    /// caller may use this narrower limit to compose several layout operations under one shared
    /// budget without creating a second text engine.
    pub fn layout_with_glyph_limit(
        &mut self,
        request: &TextLayoutRequest,
        max_glyphs: usize,
    ) -> Result<TextLayout, TextLayoutError> {
        self.work_stats.layout_attempts = self.work_stats.layout_attempts.saturating_add(1);
        self.work_stats.layout_source_bytes = self
            .work_stats
            .layout_source_bytes
            .saturating_add(request.text().len() as u64);
        self.validate_request(request)?;
        let mut builder =
            self.layout_context
                .ranged_builder(&mut self.font_context, request.text(), 1.0, false);
        builder.push_default(StyleProperty::FontFamily(self.font_families.clone()));
        builder.push_default(StyleProperty::FontSize(request.logical_size()));
        builder.push_default(StyleProperty::LineHeight(
            request.typography().parley_line_height(),
        ));
        builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::Wrap));

        let mut parley_layout = builder.build(request.text());
        let wrap_width = if request.is_min_content() {
            let min_width = parley_layout.calculate_content_widths().min;
            if !min_width.is_finite() || min_width < 0.0 {
                return Err(TextLayoutError::NonFiniteResult);
            }
            Some(min_width)
        } else {
            request.wrap_width()
        };
        parley_layout.break_all_lines(wrap_width);
        parley_layout.align(
            parley_alignment(request.alignment()),
            AlignmentOptions::default(),
        );
        for line in parley_layout.lines() {
            for run in line.runs() {
                for cluster in run.clusters() {
                    self.work_stats.shaped_glyphs = self
                        .work_stats
                        .shaped_glyphs
                        .saturating_add(cluster.glyphs().count() as u64);
                }
            }
        }

        self.project_layout_with_glyph_limit(&parley_layout, request, max_glyphs)
    }

    pub(crate) fn configure_editor(&self, editor: &mut PlainEditor<()>) {
        editor.set_scale(1.0);
        editor.set_quantize(false);
        let styles = editor.edit_styles();
        styles.insert(StyleProperty::FontFamily(self.font_families.clone()));
        styles.insert(StyleProperty::TextWrapMode(TextWrapMode::Wrap));
    }

    pub(crate) fn configure_editor_typography(
        &self,
        editor: &mut PlainEditor<()>,
        typography: TextTypography,
    ) {
        self.configure_editor(editor);
        let styles = editor.edit_styles();
        styles.insert(StyleProperty::FontSize(typography.logical_size()));
        styles.insert(StyleProperty::LineHeight(typography.parley_line_height()));
    }

    pub(crate) fn project_layout(
        &self,
        layout: &Layout<()>,
        request: &TextLayoutRequest,
    ) -> Result<TextLayout, TextLayoutError> {
        self.project_layout_with_glyph_limit(layout, request, self.limits.max_glyphs)
    }

    fn project_layout_with_glyph_limit(
        &self,
        layout: &Layout<()>,
        request: &TextLayoutRequest,
        max_glyphs: usize,
    ) -> Result<TextLayout, TextLayoutError> {
        self.validate_request(request)?;
        let max_glyphs = max_glyphs.min(self.limits.max_glyphs);
        if layout.len() > self.limits.max_lines {
            return Err(TextLayoutError::LineLimitExceeded {
                actual: layout.len(),
                maximum: self.limits.max_lines,
            });
        }
        if !layout.width().is_finite() || !layout.height().is_finite() {
            return Err(TextLayoutError::NonFiniteResult);
        }

        let mut lines = Vec::with_capacity(layout.len());
        let mut runs = Vec::new();
        let mut glyphs = Vec::new();
        let mut missing = BTreeSet::new();
        let mut missing_sequences = BTreeSet::new();
        for (line_index, line) in layout.lines().enumerate() {
            let metrics = line.metrics();
            if ![
                metrics.baseline,
                metrics.block_min_coord,
                metrics.block_max_coord,
                metrics.advance,
            ]
            .into_iter()
            .all(f32::is_finite)
            {
                return Err(TextLayoutError::NonFiniteResult);
            }
            lines.push(TextLine {
                text_range: line.text_range(),
                baseline: metrics.baseline,
                block_min: metrics.block_min_coord,
                block_max: metrics.block_max_coord,
                advance: metrics.advance,
            });
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let face = &self
                    .fonts
                    .iter()
                    .find(|font| font.font_data == *run.font())
                    .ok_or(TextLayoutError::UnadmittedFontFace)?
                    .face;
                let face_id = face.id();
                let mut group: Option<(Range<usize>, bool)> = None;
                for cluster in run.visual_clusters() {
                    if !cluster.is_ligature_continuation()
                        && let Some((range, true)) = group.take()
                    {
                        record_missing_glyphs(
                            &mut missing,
                            &mut missing_sequences,
                            request.text(),
                            range,
                        );
                    }
                    let entry = group.get_or_insert((cluster.text_range(), false));
                    entry.0.start = entry.0.start.min(cluster.text_range().start);
                    entry.0.end = entry.0.end.max(cluster.text_range().end);
                    entry.1 |= cluster.glyphs().any(|glyph| glyph.id == 0);
                }
                if let Some((range, true)) = group {
                    record_missing_glyphs(
                        &mut missing,
                        &mut missing_sequences,
                        request.text(),
                        range,
                    );
                }
                let run_index = runs.len();
                let glyph_start = glyphs.len();
                let physical_size = run.font_size() * request.scale_factor();
                for glyph in glyph_run.positioned_glyphs() {
                    if glyphs.len() == max_glyphs {
                        return Err(TextLayoutError::GlyphLimitExceeded {
                            actual: glyphs.len() + 1,
                            maximum: max_glyphs,
                        });
                    }
                    let glyph_id = u16::try_from(glyph.id)
                        .map_err(|_| TextLayoutError::GlyphIdOutOfRange { glyph_id: glyph.id })?;
                    if ![glyph.x, glyph.y, glyph.advance, physical_size]
                        .into_iter()
                        .all(f32::is_finite)
                    {
                        return Err(TextLayoutError::NonFiniteResult);
                    }
                    glyphs.push(PositionedGlyph {
                        x: glyph.x,
                        y: glyph.y,
                        advance: glyph.advance,
                        run_index,
                        key: GlyphRasterKey {
                            face_id,
                            glyph_id,
                            physical_size_bits: physical_size.to_bits(),
                            subpixel_x: quarter_pixel(glyph.x * request.scale_factor()),
                            subpixel_y: quarter_pixel(glyph.y * request.scale_factor()),
                        },
                    });
                }
                runs.push(TextRun {
                    line_index,
                    text_range: run.text_range(),
                    glyph_range: glyph_start..glyphs.len(),
                    direction: if run.is_rtl() {
                        TextDirection::RightToLeft
                    } else {
                        TextDirection::LeftToRight
                    },
                });
            }
        }

        Ok(TextLayout {
            typography: request.typography(),
            width: layout.width(),
            height: layout.height(),
            lines,
            runs,
            glyphs,
            diagnostics: missing_glyph_diagnostics(missing, missing_sequences, self.limits),
        })
    }

    pub(crate) fn validate_request(
        &self,
        request: &TextLayoutRequest,
    ) -> Result<(), TextLayoutError> {
        if request.text().len() > self.limits.max_text_bytes {
            return Err(TextLayoutError::TextBytesExceeded {
                actual: request.text().len(),
                maximum: self.limits.max_text_bytes,
            });
        }
        if request.scale_factor() > self.limits.max_scale_factor {
            return Err(TextLayoutError::ScaleFactorExceeded {
                actual: request.scale_factor(),
                maximum: self.limits.max_scale_factor,
            });
        }
        crate::layout::validate_graphemes(request.text())?;
        Ok(())
    }

    pub(crate) fn raster_context(
        &mut self,
        face_id: FontFaceId,
    ) -> Option<(FontRef<'_>, &mut ScaleContext)> {
        let font = self.fonts.iter().find(|font| font.face.id() == face_id)?;
        Some((
            FontRef {
                data: font.face.bytes(),
                offset: font.swash_offset,
                key: font.swash_key,
            },
            &mut self.scale_context,
        ))
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TextEngineCreateError {
    #[error("text limit `{field}` is outside the supported bounded policy")]
    InvalidLimit { field: &'static str },
    #[error("the explicit font set is empty")]
    EmptyFontSet,
    #[error("font set has {actual} faces; maximum is {maximum}")]
    FontCountExceeded { actual: usize, maximum: usize },
    #[error("font set has {actual} container bytes; maximum is {maximum}")]
    FontBytesExceeded { actual: usize, maximum: usize },
    #[error("font face {face_id:?} is admitted more than once")]
    DuplicateFontFace { face_id: FontFaceId },
    #[error("admitted font {face_id:?} could not be registered with the shaping engine")]
    FontRegistrationFailed { face_id: FontFaceId },
    #[error("admitted font {face_id:?} could not be opened by the rasterizer")]
    RasterFontUnavailable { face_id: FontFaceId },
}

fn shaping_blob(face: &FontFace) -> Result<Blob<u8>, TextEngineCreateError> {
    if ttf_parser::fonts_in_collection(face.bytes()).is_none() {
        return Ok(Blob::new(face.shared_bytes()));
    }
    let offset_start = usize::try_from(face.face_index())
        .ok()
        .and_then(|index| index.checked_mul(4))
        .and_then(|offset| offset.checked_add(12))
        .ok_or(TextEngineCreateError::FontRegistrationFailed { face_id: face.id() })?;
    let face_offset = face
        .bytes()
        .get(offset_start..offset_start + 4)
        .ok_or(TextEngineCreateError::FontRegistrationFailed { face_id: face.id() })?;
    let mut bytes = face.bytes().to_vec();
    bytes[4..8].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    bytes[8..12].copy_from_slice(&1_u32.to_be_bytes());
    bytes[12..16].copy_from_slice(face_offset);
    Ok(Blob::from(bytes))
}

fn record_missing_glyphs(
    missing: &mut BTreeSet<char>,
    sequences: &mut BTreeSet<(usize, usize)>,
    text: &str,
    range: Range<usize>,
) {
    let mut scalars = text[range.clone()].chars();
    let Some(scalar) = scalars.next() else {
        return;
    };
    if scalars.next().is_some() {
        sequences.insert((range.start, range.end));
    } else if !scalar.is_control() && !scalar.is_whitespace() {
        missing.insert(scalar);
    }
}

fn missing_glyph_diagnostics(
    missing: BTreeSet<char>,
    sequences: BTreeSet<(usize, usize)>,
    limits: TextLimits,
) -> Vec<TextDiagnostic> {
    let total = missing.len() + sequences.len();
    let retained = if total > limits.max_diagnostics {
        limits.max_diagnostics.saturating_sub(1)
    } else {
        total
    };
    let mut diagnostics = missing
        .into_iter()
        .map(|scalar| TextDiagnostic::MissingGlyph { scalar })
        .chain(
            sequences
                .into_iter()
                .map(|(start, end)| TextDiagnostic::MissingGlyphSequence {
                    text_range: start..end,
                }),
        )
        .take(retained)
        .collect::<Vec<_>>();
    if total > retained {
        diagnostics.push(TextDiagnostic::DiagnosticsTruncated {
            omitted: total - retained,
        });
    }
    diagnostics
}

const fn parley_alignment(alignment: TextAlignment) -> Alignment {
    match alignment {
        TextAlignment::Start => Alignment::Start,
        TextAlignment::Center => Alignment::Center,
        TextAlignment::End => Alignment::End,
        TextAlignment::Left => Alignment::Left,
        TextAlignment::Right => Alignment::Right,
    }
}

pub(crate) fn quarter_pixel(value: f32) -> u8 {
    (value.rem_euclid(1.0) * 4.0).floor() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundled_font_face;

    #[test]
    fn variable_fonts_use_the_explicit_faces_default_instance_without_style_synthesis() {
        let face = FontFace::from_bytes(
            include_bytes!("../tests/fixtures/MonaSans-VariableFont.ttf").to_vec(),
            0,
        )
        .unwrap();
        let mut engine = TextEngine::new(face, TextLimits::default()).unwrap();
        let mut editor = PlainEditor::new(24.0);
        engine.configure_editor(&mut editor);
        editor.set_text("Nara");
        let layout = editor.layout(&mut engine.font_context, &mut engine.layout_context);
        let mut runs = 0;
        for line in layout.lines() {
            for run in line.runs() {
                runs += 1;
                assert!(
                    run.normalized_coords()
                        .iter()
                        .all(|coordinate| *coordinate == 0)
                );
                assert!(run.synthesis().variation_settings().is_empty());
                assert!(!run.synthesis().embolden());
                assert_eq!(run.synthesis().skew(), None);
            }
        }
        assert!(runs > 0);
        let request = TextLayoutRequest::new("Nara", 24.0).unwrap();
        let projected = engine.project_layout(layout, &request).unwrap();
        assert_eq!(projected, engine.layout(&request).unwrap());
        assert!(projected.glyphs().iter().all(|glyph| {
            engine
                .rasterize(glyph.key())
                .unwrap()
                .mask()
                .iter()
                .any(|coverage| *coverage != 0)
        }));
    }

    #[test]
    fn editor_projection_matches_direct_layout_for_explicit_fonts_and_wrapping() {
        let fallback = FontFace::from_bytes(
            include_bytes!("../tests/fixtures/FiraMono-Medium.ttf").to_vec(),
            0,
        )
        .unwrap();
        let primary = bundled_font_face().unwrap();
        let expected_faces = [primary.id(), fallback.id()];
        let mut engine =
            TextEngine::with_fonts(vec![primary, fallback], TextLimits::default()).unwrap();
        assert_eq!(engine.font_ids().collect::<Vec<_>>(), expected_faces);
        for width in [None, Some(72.0)] {
            let mut request = TextLayoutRequest::new("A\u{2229} one two three\nfour", 20.0)
                .unwrap()
                .with_alignment(TextAlignment::Center)
                .with_scale_factor(1.5)
                .unwrap();
            if let Some(width) = width {
                request = request.with_wrap_width(width).unwrap();
            }
            let mut editor = PlainEditor::new(request.logical_size());
            engine.configure_editor(&mut editor);
            editor.set_text(request.text());
            editor.set_width(width);
            editor.set_alignment(Alignment::Center);
            let layout = editor.layout(&mut engine.font_context, &mut engine.layout_context);
            let projected = engine.project_layout(layout, &request).unwrap();
            assert_eq!(projected, engine.layout(&request).unwrap());
        }
    }

    #[test]
    fn projection_rejects_layouts_from_an_unadmitted_font_context() {
        let engine = TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
        let fallback = FontFace::from_bytes(
            include_bytes!("../tests/fixtures/FiraMono-Medium.ttf").to_vec(),
            0,
        )
        .unwrap();
        let mut foreign_engine = TextEngine::new(fallback, TextLimits::default()).unwrap();
        let request = TextLayoutRequest::new("A", 20.0).unwrap();
        let mut editor = PlainEditor::new(request.logical_size());
        foreign_engine.configure_editor(&mut editor);
        editor.set_text(request.text());
        let layout = editor.layout(
            &mut foreign_engine.font_context,
            &mut foreign_engine.layout_context,
        );
        assert_eq!(
            engine.project_layout(layout, &request),
            Err(TextLayoutError::UnadmittedFontFace)
        );
    }

    #[test]
    fn editor_projection_keeps_line_and_glyph_limits_and_recovers_after_rejection() {
        let limits = TextLimits {
            max_lines: 1,
            max_glyphs: 1,
            ..TextLimits::default()
        };
        let fallback = FontFace::from_bytes(
            include_bytes!("../tests/fixtures/FiraMono-Medium.ttf").to_vec(),
            0,
        )
        .unwrap();
        let mut engine =
            TextEngine::with_fonts(vec![bundled_font_face().unwrap(), fallback], limits).unwrap();
        let mut editor = PlainEditor::new(20.0);
        engine.configure_editor(&mut editor);
        for (text, expected) in [
            (
                "A\u{2229}",
                TextLayoutError::GlyphLimitExceeded {
                    actual: 2,
                    maximum: 1,
                },
            ),
            (
                "A\n\u{2229}",
                TextLayoutError::LineLimitExceeded {
                    actual: 2,
                    maximum: 1,
                },
            ),
        ] {
            editor.set_text(text);
            let request = TextLayoutRequest::new(text, 20.0).unwrap();
            let layout = editor.layout(&mut engine.font_context, &mut engine.layout_context);
            assert_eq!(engine.project_layout(layout, &request), Err(expected));
        }
        editor.set_text("A");
        let request = TextLayoutRequest::new("A", 20.0).unwrap();
        let layout = editor.layout(&mut engine.font_context, &mut engine.layout_context);
        assert_eq!(
            engine
                .project_layout(layout, &request)
                .unwrap()
                .glyphs()
                .len(),
            1
        );
    }
}
