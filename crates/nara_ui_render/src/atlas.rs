use std::{collections::HashMap, sync::Arc};

use guillotiere::{AtlasAllocator, size2};
use nara_text::{GlyphRaster, GlyphRasterKey, TextEngine};
use thiserror::Error;

use crate::{UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot, UiPresentationLimits};

#[derive(Clone)]
struct AtlasPage {
    allocator: AtlasAllocator,
    pixels: Arc<Vec<u8>>,
    generation: u64,
}

impl AtlasPage {
    fn try_new(limits: UiPresentationLimits) -> Result<Self, UiAtlasPlanError> {
        let extent = i32::try_from(limits.atlas_page_extent())
            .map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?;
        let byte_len = limits
            .checked_page_byte_len()
            .map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(byte_len)
            .map_err(|_| UiAtlasPlanError::AllocationFailed)?;
        pixels.resize(byte_len, 0);
        Ok(Self {
            allocator: AtlasAllocator::new(size2(extent, extent)),
            pixels: Arc::new(pixels),
            generation: 0,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GlyphSlot {
    Empty,
    Atlas {
        page_index: u16,
        min_x: u32,
        min_y: u32,
        width: u32,
        height: u32,
        left: i32,
        top: i32,
    },
}

#[derive(Clone, Default)]
pub(crate) struct GlyphAtlas {
    font_revision: Option<u64>,
    pages: Vec<AtlasPage>,
    glyphs: HashMap<GlyphRasterKey, GlyphSlot>,
    next_generation: u64,
}

impl GlyphAtlas {
    pub(crate) fn stage(
        &self,
        font_revision: u64,
        requested: &[GlyphRasterKey],
        engine: &mut TextEngine,
        limits: UiPresentationLimits,
    ) -> Result<Self, UiAtlasPlanError> {
        let mut staged = if self.font_revision == Some(font_revision) {
            self.clone()
        } else {
            Self {
                font_revision: Some(font_revision),
                next_generation: 1,
                ..Self::default()
            }
        };
        if staged.next_generation == 0 {
            staged.next_generation = 1;
        }
        validate_atlas_bytes(staged.pages.len(), limits)?;

        let mut dirty_pages = Vec::new();
        let mut newly_rasterized = 0_usize;
        for key in requested.iter().copied() {
            if staged.glyphs.contains_key(&key) {
                continue;
            }
            if staged.glyphs.len() >= limits.max_glyph_entries() {
                return Err(UiAtlasPlanError::GlyphEntryLimitExceeded {
                    limit: limits.max_glyph_entries(),
                });
            }
            if newly_rasterized >= limits.max_new_glyphs_per_plan() {
                return Err(UiAtlasPlanError::NewGlyphLimitExceeded {
                    limit: limits.max_new_glyphs_per_plan(),
                });
            }
            staged
                .glyphs
                .try_reserve(1)
                .map_err(|_| UiAtlasPlanError::AllocationFailed)?;
            let raster = engine
                .rasterize(key)
                .map_err(|error| UiAtlasPlanError::RasterizationFailed(error.to_string()))?;
            newly_rasterized = newly_rasterized
                .checked_add(1)
                .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
            if raster.width() == 0 || raster.height() == 0 {
                staged.glyphs.insert(key, GlyphSlot::Empty);
                continue;
            }
            if raster.width() > limits.max_glyph_extent()
                || raster.height() > limits.max_glyph_extent()
            {
                return Err(UiAtlasPlanError::GlyphExtentLimitExceeded {
                    width: raster.width(),
                    height: raster.height(),
                    limit: limits.max_glyph_extent(),
                });
            }
            let slot = allocate_raster(&mut staged, &raster, limits, &mut dirty_pages)?;
            staged.glyphs.insert(key, slot);
        }

        dirty_pages.sort_unstable();
        for page_index in dirty_pages {
            let page = staged
                .pages
                .get_mut(page_index)
                .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
            page.generation = staged.next_generation;
            staged.next_generation = staged
                .next_generation
                .checked_add(1)
                .ok_or(UiAtlasPlanError::GenerationExhausted)?;
        }
        Ok(staged)
    }

    #[must_use]
    pub(crate) fn slot(&self, key: GlyphRasterKey) -> Option<GlyphSlot> {
        self.glyphs.get(&key).copied()
    }

    pub(crate) fn snapshot(
        &self,
        page_index: u16,
    ) -> Result<UiGlyphAtlasPageSnapshot, UiAtlasPlanError> {
        let page = self
            .pages
            .get(usize::from(page_index))
            .ok_or(UiAtlasPlanError::MissingPage { page_index })?;
        let extent = u32::try_from(page.allocator.size().width)
            .map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?;
        Ok(UiGlyphAtlasPageSnapshot::new(
            UiGlyphAtlasPageKey::new(self.font_revision.unwrap_or(1), page_index, page.generation),
            extent,
            extent,
            Arc::clone(&page.pixels),
        ))
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub(crate) enum UiAtlasPlanError {
    #[error("the glyph cache exceeds its {limit} entry limit")]
    GlyphEntryLimitExceeded { limit: usize },
    #[error("the presentation plan exceeds its {limit} new-glyph limit")]
    NewGlyphLimitExceeded { limit: usize },
    #[error("glyph rasterization failed: {0}")]
    RasterizationFailed(String),
    #[error("glyph raster {width}x{height} exceeds the {limit} pixel extent limit")]
    GlyphExtentLimitExceeded { width: u32, height: u32, limit: u32 },
    #[error("the glyph atlas exceeds its {limit} page limit")]
    AtlasPageLimitExceeded { limit: usize },
    #[error("the glyph atlas exceeds its {limit} byte retention limit")]
    AtlasByteLimitExceeded { limit: usize },
    #[error("the staged glyph atlas exceeds its {limit} byte mutation limit")]
    StagedAtlasByteLimitExceeded { limit: usize },
    #[error("glyph atlas allocation failed within the admitted hard limits")]
    AllocationFailed,
    #[error("glyph atlas arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("glyph atlas generation identity was exhausted")]
    GenerationExhausted,
    #[error("glyph atlas page {page_index} is missing")]
    MissingPage { page_index: u16 },
}

fn allocate_raster(
    atlas: &mut GlyphAtlas,
    raster: &GlyphRaster,
    limits: UiPresentationLimits,
    dirty_pages: &mut Vec<usize>,
) -> Result<GlyphSlot, UiAtlasPlanError> {
    let requested = size2(
        i32::try_from(raster.width()).map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?,
        i32::try_from(raster.height()).map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?,
    );

    for page_index in 0..atlas.pages.len() {
        if let Some(allocation) = atlas.pages[page_index].allocator.allocate(requested) {
            admit_dirty_page(page_index, dirty_pages, limits)?;
            return write_raster(atlas, page_index, allocation.rectangle, raster);
        }
    }

    if atlas.pages.len() >= limits.max_atlas_pages() {
        return Err(UiAtlasPlanError::AtlasPageLimitExceeded {
            limit: limits.max_atlas_pages(),
        });
    }
    let next_page_count = atlas
        .pages
        .len()
        .checked_add(1)
        .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
    validate_atlas_bytes(next_page_count, limits)?;
    let page_index = atlas.pages.len();
    admit_dirty_page(page_index, dirty_pages, limits)?;
    atlas
        .pages
        .try_reserve(1)
        .map_err(|_| UiAtlasPlanError::AllocationFailed)?;
    atlas.pages.push(AtlasPage::try_new(limits)?);
    let allocation = atlas.pages[page_index]
        .allocator
        .allocate(requested)
        .ok_or(UiAtlasPlanError::GlyphExtentLimitExceeded {
            width: raster.width(),
            height: raster.height(),
            limit: limits.max_glyph_extent(),
        })?;
    write_raster(atlas, page_index, allocation.rectangle, raster)
}

fn admit_dirty_page(
    page_index: usize,
    dirty_pages: &mut Vec<usize>,
    limits: UiPresentationLimits,
) -> Result<(), UiAtlasPlanError> {
    if dirty_pages.contains(&page_index) {
        return Ok(());
    }
    let page_count = dirty_pages
        .len()
        .checked_add(1)
        .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
    let staged_bytes = page_count
        .checked_mul(
            limits
                .checked_page_byte_len()
                .map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?,
        )
        .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
    if staged_bytes > limits.max_staged_atlas_bytes() {
        return Err(UiAtlasPlanError::StagedAtlasByteLimitExceeded {
            limit: limits.max_staged_atlas_bytes(),
        });
    }
    dirty_pages
        .try_reserve(1)
        .map_err(|_| UiAtlasPlanError::AllocationFailed)?;
    dirty_pages.push(page_index);
    Ok(())
}

fn validate_atlas_bytes(
    page_count: usize,
    limits: UiPresentationLimits,
) -> Result<(), UiAtlasPlanError> {
    let retained = page_count
        .checked_mul(
            limits
                .checked_page_byte_len()
                .map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?,
        )
        .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
    if retained > limits.max_atlas_bytes() {
        return Err(UiAtlasPlanError::AtlasByteLimitExceeded {
            limit: limits.max_atlas_bytes(),
        });
    }
    Ok(())
}

fn write_raster(
    atlas: &mut GlyphAtlas,
    page_index: usize,
    rectangle: guillotiere::Rectangle,
    raster: &GlyphRaster,
) -> Result<GlyphSlot, UiAtlasPlanError> {
    let page = atlas
        .pages
        .get_mut(page_index)
        .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
    if Arc::strong_count(&page.pixels) != 1 {
        let mut copy = Vec::new();
        copy.try_reserve_exact(page.pixels.len())
            .map_err(|_| UiAtlasPlanError::AllocationFailed)?;
        copy.extend_from_slice(&page.pixels);
        page.pixels = Arc::new(copy);
    }
    let pixels = Arc::get_mut(&mut page.pixels).ok_or(UiAtlasPlanError::AllocationFailed)?;
    let page_width = usize::try_from(page.allocator.size().width)
        .map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?;
    let min_x =
        usize::try_from(rectangle.min.x).map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?;
    let min_y =
        usize::try_from(rectangle.min.y).map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?;
    let glyph_width =
        usize::try_from(raster.width()).map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?;
    let glyph_height =
        usize::try_from(raster.height()).map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?;
    for row in 0..glyph_height {
        let source_start = row
            .checked_mul(glyph_width)
            .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
        let source_end = source_start
            .checked_add(glyph_width)
            .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
        let destination_start = min_y
            .checked_add(row)
            .and_then(|row| row.checked_mul(page_width))
            .and_then(|start| start.checked_add(min_x))
            .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
        let destination_end = destination_start
            .checked_add(glyph_width)
            .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
        let source = raster
            .mask()
            .get(source_start..source_end)
            .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
        let destination = pixels
            .get_mut(destination_start..destination_end)
            .ok_or(UiAtlasPlanError::ArithmeticOverflow)?;
        destination.copy_from_slice(source);
    }
    Ok(GlyphSlot::Atlas {
        page_index: u16::try_from(page_index).map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?,
        min_x: u32::try_from(min_x).map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?,
        min_y: u32::try_from(min_y).map_err(|_| UiAtlasPlanError::ArithmeticOverflow)?,
        width: raster.width(),
        height: raster.height(),
        left: raster.left(),
        top: raster.top(),
    })
}
