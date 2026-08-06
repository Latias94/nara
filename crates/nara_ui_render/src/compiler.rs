use std::{collections::BTreeMap, sync::Arc};

use nara_asset::AssetServer;
use nara_image::{PreparedImageResource, PreparedImageSnapshot};
use nara_render::{PreparedRenderResources, RenderPrepareStatus, RenderResourceKey, RenderTarget};
use nara_text::{TextEngine, TextEngineCreateError, TextLimits, bundled_font_face};
use nara_ui_kernel::{
    UiFrame, UiImageId, UiPaintPrimitive, UiPhysicalRect, UiRect, UiResolvedBrush,
};
use thiserror::Error;

use crate::{
    UiGlyphAtlasPageSnapshot, UiPacketImage, UiPacketPrimitive, UiPacketRect, UiPacketUv,
    UiPaintDiagnostic, UiPaintPacket, UiPaintPacketIdentity, UiRenderLimits, UiRenderLimitsError,
    atlas::{GlyphAtlas, GlyphSlot},
};

const MAX_DIAGNOSTIC_MESSAGE_BYTES: usize = 1_024;

#[derive(Debug, Default)]
pub struct UiPreparedImages {
    images: BTreeMap<String, PreparedImageSnapshot>,
    ambiguous: BTreeMap<String, (RenderResourceKey, RenderResourceKey)>,
}

impl UiPreparedImages {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: UiImageId, snapshot: PreparedImageSnapshot) {
        self.insert_alias(id.as_str().to_owned(), snapshot);
    }

    pub(crate) fn capture(
        asset_server: Option<&AssetServer>,
        prepared: Option<&PreparedRenderResources<PreparedImageResource>>,
    ) -> Self {
        let (Some(asset_server), Some(prepared)) = (asset_server, prepared) else {
            return Self::default();
        };
        let mut index = Self::default();
        for key in prepared.keys() {
            let Some(record) = prepared.get(key) else {
                continue;
            };
            let resource = match record.status() {
                RenderPrepareStatus::Ready => record.resource(),
                RenderPrepareStatus::Failed(_) => None,
            };
            let Some(resource) = resource else {
                continue;
            };
            let snapshot = PreparedImageSnapshot::new(record.snapshot().clone(), resource.clone());
            if let Some(path) = asset_server.path(key.asset_id()) {
                index.insert_alias(path.to_owned(), snapshot.clone());
                index.insert_alias(format!("path:{path}"), snapshot.clone());
                index.insert_alias(format!("project://{path}"), snapshot.clone());
            }
            if let Some(stable_id) = asset_server.stable_id(key.asset_id()) {
                let stable_id = stable_id.to_string();
                index.insert_alias(stable_id.clone(), snapshot.clone());
                index.insert_alias(format!("stable:{stable_id}"), snapshot.clone());
            }
        }
        index
    }

    fn insert_alias(&mut self, alias: String, snapshot: PreparedImageSnapshot) {
        if self.ambiguous.contains_key(&alias) {
            return;
        }
        match self.images.get(&alias) {
            None => {
                self.images.insert(alias, snapshot);
            }
            Some(existing) if existing.identity() == snapshot.identity() => {}
            Some(existing) => {
                let existing_key = existing.identity().key();
                let replacement_key = snapshot.identity().key();
                self.images.remove(&alias);
                self.ambiguous
                    .insert(alias, (existing_key, replacement_key));
            }
        }
    }

    fn resolve(&self, id: &UiImageId) -> Result<PreparedImageSnapshot, UiPaintPacketBuildError> {
        if let Some((first, second)) = self.ambiguous.get(id.as_str()).copied() {
            return Err(UiPaintPacketBuildError::AmbiguousImage {
                id: id.as_str().to_owned(),
                first,
                second,
            });
        }
        self.images
            .get(id.as_str())
            .cloned()
            .ok_or_else(|| UiPaintPacketBuildError::MissingImage {
                id: id.as_str().to_owned(),
            })
    }
}

/// Stateful bounded compiler from one immutable kernel frame to one owned paint packet.
pub struct UiPaintPacketBuilder {
    limits: UiRenderLimits,
    text: TextEngine,
    atlas: GlyphAtlas,
}

impl UiPaintPacketBuilder {
    pub fn new(limits: UiRenderLimits) -> Result<Self, UiPaintPacketBuilderCreateError> {
        let limits = limits.validate()?;
        let face = bundled_font_face()
            .map_err(|error| UiPaintPacketBuilderCreateError::BundledFont(error.to_string()))?;
        let text = TextEngine::new(face, TextLimits::default())?;
        Ok(Self {
            limits,
            text,
            atlas: GlyphAtlas::default(),
        })
    }

    pub fn build(
        &mut self,
        frame: Arc<UiFrame>,
        target: RenderTarget,
        images: &UiPreparedImages,
    ) -> Result<Arc<UiPaintPacket>, UiPaintPacketBuildError> {
        let identity = UiPaintPacketIdentity::from_frame(&frame);
        let environment = frame.environment();
        let mut requested_glyphs = Vec::new();
        for primitive in frame.paint() {
            if let UiPaintPrimitive::Text { layout, .. } = primitive {
                requested_glyphs.extend(layout.glyphs().iter().map(|glyph| glyph.key()));
            }
        }
        let atlas_diagnostics = self.atlas.prepare(
            environment.font_revision().get(),
            &requested_glyphs,
            &mut self.text,
            self.limits,
        );
        let mut diagnostics = atlas_diagnostics
            .into_iter()
            .map(|diagnostic| {
                UiPaintDiagnostic::new(diagnostic.code, truncate_message(&diagnostic.message))
            })
            .collect::<Vec<_>>();
        let glyph_pages = self.atlas.snapshots();
        let mut packet_images = Vec::new();
        let mut image_indices = BTreeMap::<UiImageId, u32>::new();
        let mut primitives = Vec::new();

        for primitive in frame.paint() {
            match primitive {
                UiPaintPrimitive::Rect {
                    physical_rect,
                    clip_rect,
                    brush,
                    border,
                    radii,
                    ..
                } => {
                    let Some(rect) = packet_rect(*physical_rect) else {
                        continue;
                    };
                    let Some(clip) = packet_clip(*clip_rect, environment.scale_factor(), rect)
                    else {
                        continue;
                    };
                    let physical_radii = radii
                        .values()
                        .map(|value| value * environment.scale_factor());
                    if let Some(brush) = brush {
                        match brush {
                            UiResolvedBrush::Solid(color) => {
                                primitives.push(UiPacketPrimitive::Solid {
                                    rect,
                                    clip,
                                    color: *color,
                                    corner_radii: physical_radii,
                                });
                            }
                            UiResolvedBrush::Image { image, tint } => {
                                let image_index = match image_indices.get(image).copied() {
                                    Some(index) => index,
                                    None => {
                                        let index =
                                            u32::try_from(packet_images.len()).map_err(|_| {
                                                UiPaintPacketBuildError::ImageLimitExceeded
                                            })?;
                                        packet_images.push(UiPacketImage::new(
                                            image.clone(),
                                            images.resolve(image)?,
                                        ));
                                        image_indices.insert(image.clone(), index);
                                        index
                                    }
                                };
                                primitives.push(UiPacketPrimitive::Image {
                                    rect,
                                    clip,
                                    image_index,
                                    tint: *tint,
                                    uv: UiPacketUv::FULL,
                                    corner_radii: physical_radii,
                                });
                            }
                        }
                    }
                    if let Some(border) = border {
                        append_border(
                            &mut primitives,
                            rect,
                            clip,
                            border.width() * environment.scale_factor(),
                            border.color(),
                        );
                    }
                }
                UiPaintPrimitive::Text {
                    rect,
                    clip_rect,
                    color,
                    layout,
                    ..
                } => {
                    let Some(text_rect) = physical_rect(*rect, environment.scale_factor()) else {
                        continue;
                    };
                    let Some(clip) = packet_clip(*clip_rect, environment.scale_factor(), text_rect)
                    else {
                        continue;
                    };
                    append_glyphs(
                        &mut primitives,
                        *rect,
                        clip,
                        *color,
                        layout,
                        environment.scale_factor(),
                        &self.atlas,
                        &glyph_pages,
                    );
                }
            }
        }

        diagnostics.truncate(self.limits.max_diagnostics_per_packet);
        Ok(Arc::new(UiPaintPacket::new(
            identity,
            target,
            environment.scale_factor(),
            frame,
            primitives,
            packet_images,
            glyph_pages,
            diagnostics,
        )))
    }
}

impl Default for UiPaintPacketBuilder {
    fn default() -> Self {
        Self::new(UiRenderLimits::default())
            .expect("the default UI render limits and bundled font are valid")
    }
}

#[derive(Debug, Error)]
pub enum UiPaintPacketBuilderCreateError {
    #[error(transparent)]
    InvalidLimits(#[from] UiRenderLimitsError),
    #[error("bundled UI font could not be admitted: {0}")]
    BundledFont(String),
    #[error(transparent)]
    TextEngine(#[from] TextEngineCreateError),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiPaintPacketBuildError {
    #[error("UI image `{id}` has no exact prepared image snapshot")]
    MissingImage { id: String },
    #[error("UI image `{id}` resolves ambiguously to {first:?} and {second:?}")]
    AmbiguousImage {
        id: String,
        first: RenderResourceKey,
        second: RenderResourceKey,
    },
    #[error("UI paint packet exceeds the supported image-index range")]
    ImageLimitExceeded,
}

fn append_glyphs(
    primitives: &mut Vec<UiPacketPrimitive>,
    text_rect: UiRect,
    clip: Option<UiPacketRect>,
    color: nara_ui_kernel::UiColor,
    layout: &nara_text::TextLayout,
    scale_factor: f32,
    atlas: &GlyphAtlas,
    pages: &[UiGlyphAtlasPageSnapshot],
) {
    let origin = text_rect.origin();
    for glyph in layout.glyphs() {
        let Some(slot) = atlas.slot(glyph.key()) else {
            continue;
        };
        let GlyphSlot::Atlas {
            page_index,
            min_x,
            min_y,
            width,
            height,
            left,
            top,
        } = slot
        else {
            continue;
        };
        let baseline_x = snap_coordinate(origin.x() + glyph.x(), scale_factor);
        let baseline_y = snap_coordinate(origin.y() + glyph.y(), scale_factor);
        let Some(rect) = UiPacketRect::new(
            baseline_x.saturating_add(left),
            baseline_y.saturating_sub(top),
            baseline_x
                .saturating_add(left)
                .saturating_add(i32::try_from(width).unwrap_or(i32::MAX)),
            baseline_y
                .saturating_sub(top)
                .saturating_add(i32::try_from(height).unwrap_or(i32::MAX)),
        ) else {
            continue;
        };
        let Some(page) = pages.get(usize::from(page_index)) else {
            continue;
        };
        let page_width = page.width() as f32;
        let page_height = page.height() as f32;
        let Some(clip) = primitive_clip(clip, rect) else {
            continue;
        };
        primitives.push(UiPacketPrimitive::Glyph {
            rect,
            clip,
            page_index,
            color,
            uv: UiPacketUv::new(
                [min_x as f32 / page_width, min_y as f32 / page_height],
                [width as f32 / page_width, height as f32 / page_height],
            ),
        });
    }
}

fn append_border(
    primitives: &mut Vec<UiPacketPrimitive>,
    rect: UiPacketRect,
    clip: Option<UiPacketRect>,
    physical_width: f32,
    color: nara_ui_kernel::UiColor,
) {
    let width = physical_width.round().clamp(1.0, i32::MAX as f32) as i32;
    let inner_left = rect.min_x().saturating_add(width).min(rect.max_x());
    let inner_right = rect.max_x().saturating_sub(width).max(rect.min_x());
    let inner_top = rect.min_y().saturating_add(width).min(rect.max_y());
    let inner_bottom = rect.max_y().saturating_sub(width).max(rect.min_y());
    let candidates = [
        UiPacketRect::new(rect.min_x(), rect.min_y(), rect.max_x(), inner_top),
        UiPacketRect::new(rect.min_x(), inner_bottom, rect.max_x(), rect.max_y()),
        UiPacketRect::new(rect.min_x(), inner_top, inner_left, inner_bottom),
        UiPacketRect::new(inner_right, inner_top, rect.max_x(), inner_bottom),
    ];
    primitives.extend(candidates.into_iter().flatten().filter_map(|border_rect| {
        let clip = primitive_clip(clip, border_rect)?;
        Some(UiPacketPrimitive::Solid {
            rect: border_rect,
            clip,
            color,
            corner_radii: [0.0; 4],
        })
    }));
}

fn packet_rect(rect: UiPhysicalRect) -> Option<UiPacketRect> {
    UiPacketRect::new(rect.min_x(), rect.min_y(), rect.max_x(), rect.max_y())
}

fn physical_rect(rect: UiRect, scale_factor: f32) -> Option<UiPacketRect> {
    let origin = rect.origin();
    let max = rect.max();
    UiPacketRect::new(
        snap_coordinate(origin.x(), scale_factor),
        snap_coordinate(origin.y(), scale_factor),
        snap_coordinate(max.x(), scale_factor),
        snap_coordinate(max.y(), scale_factor),
    )
}

fn packet_clip(
    clip: Option<UiRect>,
    scale_factor: f32,
    primitive_rect: UiPacketRect,
) -> Option<Option<UiPacketRect>> {
    let Some(clip) = clip else {
        return Some(None);
    };
    physical_rect(clip, scale_factor)
        .and_then(|clip| clip.intersect(primitive_rect))
        .map(Some)
}

fn primitive_clip(
    clip: Option<UiPacketRect>,
    primitive_rect: UiPacketRect,
) -> Option<Option<UiPacketRect>> {
    match clip {
        Some(clip) => clip.intersect(primitive_rect).map(Some),
        None => Some(None),
    }
}

fn snap_coordinate(value: f32, scale_factor: f32) -> i32 {
    (value * scale_factor)
        .round()
        .clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_clip_distinguishes_unclipped_from_fully_clipped() {
        let primitive = UiPacketRect::new(10, 10, 20, 20).unwrap();
        let outside = UiPacketRect::new(0, 0, 5, 5).unwrap();

        assert_eq!(primitive_clip(None, primitive), Some(None));
        assert_eq!(primitive_clip(Some(outside), primitive), None);
    }

    #[test]
    fn border_segments_outside_the_visible_clip_are_omitted() {
        let rect = UiPacketRect::new(0, 0, 10, 10).unwrap();
        let center_clip = UiPacketRect::new(4, 4, 6, 6).unwrap();
        let mut primitives = Vec::new();

        append_border(
            &mut primitives,
            rect,
            Some(center_clip),
            1.0,
            nara_ui_kernel::UiColor::rgba(1.0, 1.0, 1.0, 1.0),
        );

        assert!(primitives.is_empty());
    }
}

fn truncate_message(message: &str) -> String {
    if message.len() <= MAX_DIAGNOSTIC_MESSAGE_BYTES {
        return message.to_owned();
    }
    let mut end = MAX_DIAGNOSTIC_MESSAGE_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_owned()
}
