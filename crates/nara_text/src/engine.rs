use std::collections::BTreeSet;

use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, LayoutContext, PositionedLayoutItem,
    StyleProperty, TextWrapMode,
    fontique::{Blob, Collection, CollectionOptions, SourceCache},
};
use swash::{CacheKey, FontRef, scale::ScaleContext};
use thiserror::Error;

use crate::{
    FontFace, GlyphRasterKey, PositionedGlyph, TextAlignment, TextDiagnostic, TextDirection,
    TextLayout, TextLayoutError, TextLayoutRequest, TextLimits, TextLine, TextRun,
};

pub struct TextEngine {
    pub(crate) face: FontFace,
    pub(crate) limits: TextLimits,
    font_context: FontContext,
    layout_context: LayoutContext<()>,
    pub(crate) scale_context: ScaleContext,
    pub(crate) swash_offset: u32,
    pub(crate) swash_key: CacheKey,
}

impl TextEngine {
    pub fn new(face: FontFace, limits: TextLimits) -> Result<Self, TextEngineCreateError> {
        if let Some(field) = limits.invalid_field() {
            return Err(TextEngineCreateError::InvalidLimit { field });
        }

        let swash_font = FontRef::from_index(face.bytes(), face.face_index() as usize)
            .ok_or(TextEngineCreateError::RasterFontUnavailable)?;
        let swash_offset = swash_font.offset;
        let swash_key = swash_font.key;

        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let registered = collection.register_fonts(Blob::new(face.shared_bytes()), None);
        if registered.is_empty() || collection.family_id(face.family_name()).is_none() {
            return Err(TextEngineCreateError::FontRegistrationFailed);
        }
        let font_context = FontContext {
            collection,
            source_cache: SourceCache::default(),
        };

        Ok(Self {
            face,
            limits,
            font_context,
            layout_context: LayoutContext::new(),
            scale_context: ScaleContext::new(),
            swash_offset,
            swash_key,
        })
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
        self.validate_request(request)?;
        let max_glyphs = max_glyphs.min(self.limits.max_glyphs);
        let diagnostics = missing_glyph_diagnostics(&self.face, request.text(), self.limits);
        let family_name = self.face.family_name();
        let mut builder =
            self.layout_context
                .ranged_builder(&mut self.font_context, request.text(), 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(family_name)));
        builder.push_default(StyleProperty::FontSize(request.logical_size()));
        builder.push_default(StyleProperty::TextWrapMode(
            if request.wrap_width().is_some() {
                TextWrapMode::Wrap
            } else {
                TextWrapMode::NoWrap
            },
        ));

        let mut parley_layout = builder.build(request.text());
        parley_layout.break_all_lines(request.wrap_width());
        parley_layout.align(
            parley_alignment(request.alignment()),
            AlignmentOptions::default(),
        );

        if parley_layout.len() > self.limits.max_lines {
            return Err(TextLayoutError::LineLimitExceeded {
                actual: parley_layout.len(),
                maximum: self.limits.max_lines,
            });
        }
        if !parley_layout.width().is_finite() || !parley_layout.height().is_finite() {
            return Err(TextLayoutError::NonFiniteResult);
        }

        let mut lines = Vec::with_capacity(parley_layout.len());
        let mut runs = Vec::new();
        let mut glyphs = Vec::new();
        for (line_index, line) in parley_layout.lines().enumerate() {
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
                            face_id: self.face.id(),
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
            width: parley_layout.width(),
            height: parley_layout.height(),
            lines,
            runs,
            glyphs,
            diagnostics,
        })
    }

    fn validate_request(&self, request: &TextLayoutRequest) -> Result<(), TextLayoutError> {
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
        Ok(())
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TextEngineCreateError {
    #[error("text limit `{field}` is outside the supported bounded policy")]
    InvalidLimit { field: &'static str },
    #[error("admitted font could not be registered with the shaping engine")]
    FontRegistrationFailed,
    #[error("admitted font could not be opened by the rasterizer")]
    RasterFontUnavailable,
}

fn missing_glyph_diagnostics(
    face: &FontFace,
    text: &str,
    limits: TextLimits,
) -> Vec<TextDiagnostic> {
    let Ok(parsed) = ttf_parser::Face::parse(face.bytes(), face.face_index()) else {
        return Vec::new();
    };
    let missing = text
        .chars()
        .filter(|scalar| !scalar.is_control() && !scalar.is_whitespace())
        .filter(|scalar| parsed.glyph_index(*scalar).is_none())
        .collect::<BTreeSet<_>>();
    if missing.len() <= limits.max_diagnostics {
        return missing
            .into_iter()
            .map(|scalar| TextDiagnostic::MissingGlyph { scalar })
            .collect();
    }

    let retained = limits.max_diagnostics.saturating_sub(1);
    let omitted = missing.len() - retained;
    let mut diagnostics = missing
        .into_iter()
        .take(retained)
        .map(|scalar| TextDiagnostic::MissingGlyph { scalar })
        .collect::<Vec<_>>();
    diagnostics.push(TextDiagnostic::DiagnosticsTruncated { omitted });
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

fn quarter_pixel(value: f32) -> u8 {
    (value.rem_euclid(1.0) * 4.0).floor() as u8
}
