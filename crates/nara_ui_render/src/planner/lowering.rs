use std::{collections::HashSet, mem::size_of, sync::Arc};

use nara_render::{RenderResourceSnapshot, RenderTarget};
use nara_text::GlyphRasterKey;
use nara_ui_kernel::{
    UiFrame, UiImageId, UiPaintPrimitive, UiPhysicalRect, UiRect, UiResolvedBrush,
};

use crate::{
    UiGlyphAtlasPageSnapshot, UiPacketBorder, UiPacketFill, UiPacketImage, UiPacketPrimitive,
    UiPacketRect, UiPacketUv, UiPaintPacket, UiPaintPacketIdentity, UiPresentationLimits,
    atlas::{GlyphAtlas, GlyphSlot},
};

use super::{
    UiPresentationContentError,
    images::{UiPreparedImageError, UiPreparedImages},
};

pub(super) struct ContentPreflight {
    requested_glyphs: Vec<GlyphRasterKey>,
}

impl ContentPreflight {
    pub(super) fn requested_glyphs(&self) -> &[GlyphRasterKey] {
        &self.requested_glyphs
    }
}

pub(super) fn preflight_content(
    frame: &UiFrame,
    images: &UiPreparedImages,
    limits: UiPresentationLimits,
) -> Result<ContentPreflight, UiPresentationContentError> {
    let requested_glyphs = collect_referenced_glyphs(frame, limits)?;
    let mut referenced_image_ids = HashSet::new();
    let mut referenced_images = Vec::<RenderResourceSnapshot>::new();
    for primitive in frame.paint() {
        let UiPaintPrimitive::Rect {
            physical_rect,
            brush,
            radii,
            ..
        } = primitive
        else {
            continue;
        };
        if let Some(rect) = packet_rect(*physical_rect) {
            uniform_corner_radius(
                radii.values(),
                frame.environment().scale_factor(),
                rect,
                limits,
            )?;
        }
        let Some(UiResolvedBrush::Image { image, .. }) = brush else {
            continue;
        };
        if referenced_image_ids.contains(image) {
            continue;
        }
        referenced_image_ids
            .try_reserve(1)
            .map_err(|_| UiPresentationContentError::AllocationFailed)?;
        referenced_image_ids.insert(image.clone());
        let snapshot = images.resolve(image).map_err(map_image_error)?;
        if referenced_images
            .iter()
            .any(|identity| identity == snapshot.identity())
        {
            continue;
        }
        if referenced_images.len() >= limits.max_referenced_images() {
            return Err(UiPresentationContentError::ReferencedImageLimitExceeded {
                limit: limits.max_referenced_images(),
            });
        }
        referenced_images
            .try_reserve(1)
            .map_err(|_| UiPresentationContentError::AllocationFailed)?;
        referenced_images.push(snapshot.identity().clone());
    }
    Ok(ContentPreflight { requested_glyphs })
}

fn collect_referenced_glyphs(
    frame: &UiFrame,
    limits: UiPresentationLimits,
) -> Result<Vec<GlyphRasterKey>, UiPresentationContentError> {
    let mut seen = HashSet::new();
    let mut requested = Vec::new();
    for primitive in frame.paint() {
        let UiPaintPrimitive::Text { layout, .. } = primitive else {
            continue;
        };
        for key in layout.glyphs().iter().map(|glyph| glyph.key()) {
            if seen.contains(&key) {
                continue;
            }
            if requested.len() >= limits.max_referenced_glyphs() {
                return Err(UiPresentationContentError::ReferencedGlyphLimitExceeded {
                    limit: limits.max_referenced_glyphs(),
                });
            }
            seen.try_reserve(1)
                .map_err(|_| UiPresentationContentError::AllocationFailed)?;
            requested
                .try_reserve(1)
                .map_err(|_| UiPresentationContentError::AllocationFailed)?;
            seen.insert(key);
            requested.push(key);
        }
    }
    Ok(requested)
}

pub(super) fn lower_packet(
    frame: Arc<UiFrame>,
    target: RenderTarget,
    images: &UiPreparedImages,
    atlas: &GlyphAtlas,
    limits: UiPresentationLimits,
) -> Result<Arc<UiPaintPacket>, UiPresentationContentError> {
    let identity = UiPaintPacketIdentity::from_frame(&frame);
    let environment = frame.environment();
    let mut packet_images = Vec::<UiPacketImage>::new();
    let mut image_indices = Vec::<(UiImageId, u32)>::new();
    let mut glyph_pages = Vec::new();
    let mut glyph_page_indices = Vec::<(u16, u16)>::new();
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
                let Some(clip) = packet_clip(*clip_rect, environment.scale_factor(), rect) else {
                    continue;
                };
                if brush.is_none() && border.is_none() {
                    continue;
                }
                let corner_radius = uniform_corner_radius(
                    radii.values(),
                    environment.scale_factor(),
                    rect,
                    limits,
                )?;
                let fill = match brush {
                    Some(UiResolvedBrush::Solid(color)) => {
                        Some(UiPacketFill::Solid { color: *color })
                    }
                    Some(UiResolvedBrush::Image { image, tint }) => {
                        let image_index = match image_indices
                            .iter()
                            .find_map(|(candidate, index)| (candidate == image).then_some(*index))
                        {
                            Some(index) => index,
                            None => {
                                image_indices
                                    .try_reserve(1)
                                    .map_err(|_| UiPresentationContentError::AllocationFailed)?;
                                let snapshot = images.resolve(image).map_err(map_image_error)?;
                                let index = match packet_images.iter().position(|candidate| {
                                    candidate.snapshot().identity() == snapshot.identity()
                                }) {
                                    Some(index) => u32::try_from(index).map_err(|_| {
                                        UiPresentationContentError::ImageIndexRangeExceeded
                                    })?,
                                    None => {
                                        if packet_images.len() >= limits.max_referenced_images() {
                                            return Err(
                                                UiPresentationContentError::ReferencedImageLimitExceeded {
                                                    limit: limits.max_referenced_images(),
                                                },
                                            );
                                        }
                                        packet_images.try_reserve(1).map_err(|_| {
                                            UiPresentationContentError::AllocationFailed
                                        })?;
                                        let index =
                                            u32::try_from(packet_images.len()).map_err(|_| {
                                                UiPresentationContentError::ImageIndexRangeExceeded
                                            })?;
                                        packet_images
                                            .push(UiPacketImage::new(image.clone(), snapshot));
                                        index
                                    }
                                };
                                image_indices.push((image.clone(), index));
                                index
                            }
                        };
                        Some(UiPacketFill::Image {
                            image_index,
                            tint: *tint,
                            uv: UiPacketUv::FULL,
                        })
                    }
                    None => None,
                };
                let border = border.map(|border| {
                    let physical_width = border.width() * environment.scale_factor();
                    let maximum = (rect.width().min(rect.height()) as f32) * 0.5;
                    UiPacketBorder::new(border.color(), physical_width.min(maximum))
                });
                push_primitive(
                    &mut primitives,
                    UiPacketPrimitive::Rect {
                        rect,
                        clip,
                        fill,
                        border,
                        corner_radius,
                    },
                    limits,
                )?;
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
                    &mut glyph_pages,
                    &mut glyph_page_indices,
                    layout,
                    GlyphAppendContext {
                        text_rect: *rect,
                        clip,
                        color: *color,
                        scale_factor: environment.scale_factor(),
                        atlas,
                        limits,
                    },
                )?;
            }
        }
    }

    validate_retained_bytes(&primitives, &packet_images, &glyph_pages, limits)?;
    Ok(Arc::new(UiPaintPacket::new(
        identity,
        target,
        environment.scale_factor(),
        frame,
        primitives,
        packet_images,
        glyph_pages,
    )))
}

fn map_image_error(error: UiPreparedImageError) -> UiPresentationContentError {
    match error {
        UiPreparedImageError::Missing { id } => UiPresentationContentError::MissingImage { id },
        UiPreparedImageError::Ambiguous { id, first, second } => {
            UiPresentationContentError::AmbiguousImage { id, first, second }
        }
        UiPreparedImageError::InvalidKind { id, key } => {
            UiPresentationContentError::InvalidImageResourceKind { id, key }
        }
    }
}

struct GlyphAppendContext<'a> {
    text_rect: UiRect,
    clip: Option<UiPacketRect>,
    color: nara_ui_kernel::UiColor,
    scale_factor: f32,
    atlas: &'a GlyphAtlas,
    limits: UiPresentationLimits,
}

fn append_glyphs(
    primitives: &mut Vec<UiPacketPrimitive>,
    pages: &mut Vec<UiGlyphAtlasPageSnapshot>,
    page_indices: &mut Vec<(u16, u16)>,
    layout: &nara_text::TextLayout,
    context: GlyphAppendContext<'_>,
) -> Result<(), UiPresentationContentError> {
    let origin = context.text_rect.origin();
    for glyph in layout.glyphs() {
        let Some(slot) = context.atlas.slot(glyph.key()) else {
            return Err(UiPresentationContentError::ArithmeticOverflow);
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
        let baseline_x = snap_coordinate(origin.x() + glyph.x(), context.scale_factor);
        let baseline_y = snap_coordinate(origin.y() + glyph.y(), context.scale_factor);
        let width_i32 =
            i32::try_from(width).map_err(|_| UiPresentationContentError::ArithmeticOverflow)?;
        let height_i32 =
            i32::try_from(height).map_err(|_| UiPresentationContentError::ArithmeticOverflow)?;
        let Some(rect) = UiPacketRect::new(
            baseline_x.saturating_add(left),
            baseline_y.saturating_sub(top),
            baseline_x.saturating_add(left).saturating_add(width_i32),
            baseline_y.saturating_sub(top).saturating_add(height_i32),
        ) else {
            continue;
        };
        let Some(clip) = primitive_clip(context.clip, rect) else {
            continue;
        };
        let packet_page_index = match page_indices
            .iter()
            .find_map(|(candidate, index)| (*candidate == page_index).then_some(*index))
        {
            Some(index) => index,
            None => {
                pages
                    .try_reserve(1)
                    .map_err(|_| UiPresentationContentError::AllocationFailed)?;
                page_indices
                    .try_reserve(1)
                    .map_err(|_| UiPresentationContentError::AllocationFailed)?;
                let index = u16::try_from(pages.len())
                    .map_err(|_| UiPresentationContentError::ArithmeticOverflow)?;
                pages.push(
                    context
                        .atlas
                        .snapshot(page_index)
                        .map_err(UiPresentationContentError::from_atlas)?,
                );
                page_indices.push((page_index, index));
                index
            }
        };
        let page = pages
            .get(usize::from(packet_page_index))
            .ok_or(UiPresentationContentError::ArithmeticOverflow)?;
        let page_width = page.width() as f32;
        let page_height = page.height() as f32;
        push_primitive(
            primitives,
            UiPacketPrimitive::Glyph {
                rect,
                clip,
                page_index: packet_page_index,
                color: context.color,
                uv: UiPacketUv::from_top_left(
                    [min_x as f32 / page_width, min_y as f32 / page_height],
                    [width as f32 / page_width, height as f32 / page_height],
                ),
            },
            context.limits,
        )?;
    }
    Ok(())
}

fn push_primitive(
    primitives: &mut Vec<UiPacketPrimitive>,
    primitive: UiPacketPrimitive,
    limits: UiPresentationLimits,
) -> Result<(), UiPresentationContentError> {
    if primitives.len() >= limits.max_expanded_primitives() {
        return Err(UiPresentationContentError::PrimitiveLimitExceeded {
            limit: limits.max_expanded_primitives(),
        });
    }
    primitives
        .try_reserve(1)
        .map_err(|_| UiPresentationContentError::AllocationFailed)?;
    primitives.push(primitive);
    Ok(())
}

fn uniform_corner_radius(
    radii: [f32; 4],
    scale_factor: f32,
    rect: UiPacketRect,
    limits: UiPresentationLimits,
) -> Result<f32, UiPresentationContentError> {
    if radii[1..].iter().any(|radius| *radius != radii[0]) {
        return Err(UiPresentationContentError::NonUniformCornerRadius);
    }
    let physical = radii[0] * scale_factor;
    if !physical.is_finite() || physical < 0.0 {
        return Err(UiPresentationContentError::NonUniformCornerRadius);
    }
    if physical > limits.max_uniform_corner_radius() {
        return Err(UiPresentationContentError::CornerRadiusLimitExceeded {
            actual: physical.ceil().min(u32::MAX as f32) as u32,
            limit: limits
                .max_uniform_corner_radius()
                .ceil()
                .min(u32::MAX as f32) as u32,
        });
    }
    let geometric_maximum = (rect.width().min(rect.height()) as f32) * 0.5;
    Ok(physical.min(geometric_maximum))
}

fn validate_retained_bytes(
    primitives: &[UiPacketPrimitive],
    images: &[UiPacketImage],
    pages: &[UiGlyphAtlasPageSnapshot],
    limits: UiPresentationLimits,
) -> Result<(), UiPresentationContentError> {
    let mut total = checked_slice_bytes::<UiPacketPrimitive>(primitives.len())?;
    total = checked_add(total, checked_slice_bytes::<UiPacketImage>(images.len())?)?;
    total = checked_add(
        total,
        checked_slice_bytes::<UiGlyphAtlasPageSnapshot>(pages.len())?,
    )?;
    for image in images {
        total = checked_add(total, image.snapshot().resource().pixel_len())?;
    }
    for page in pages {
        total = checked_add(total, page.pixels().len())?;
    }
    if total > limits.max_packet_retained_bytes() {
        return Err(UiPresentationContentError::PacketRetentionLimitExceeded {
            limit: limits.max_packet_retained_bytes(),
        });
    }
    Ok(())
}

fn checked_slice_bytes<T>(len: usize) -> Result<usize, UiPresentationContentError> {
    len.checked_mul(size_of::<T>())
        .ok_or(UiPresentationContentError::ArithmeticOverflow)
}

fn checked_add(left: usize, right: usize) -> Result<usize, UiPresentationContentError> {
    left.checked_add(right)
        .ok_or(UiPresentationContentError::ArithmeticOverflow)
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
}
