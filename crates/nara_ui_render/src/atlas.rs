use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use guillotiere::{AtlasAllocator, size2};
use nara_text::{GlyphRaster, GlyphRasterKey, TextEngine};
use thiserror::Error;

use crate::{UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot};

const DEFAULT_MAX_GLYPH_ENTRIES: usize = 4_096;
const DEFAULT_MAX_ATLAS_PAGES: usize = 4;
const DEFAULT_ATLAS_PAGE_EXTENT: u32 = 1_024;
const DEFAULT_MAX_GLYPH_EXTENT: u32 = 256;
const DEFAULT_MAX_NEW_GLYPHS_PER_PACKET: usize = 256;
const DEFAULT_MAX_UPLOAD_BYTES_PER_PACKET: usize = 1_024 * 1_024;
const DEFAULT_MAX_DIAGNOSTICS_PER_PACKET: usize = 64;

/// Typed bounded policy for CPU glyph-atlas construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiRenderLimits {
    pub max_glyph_entries: usize,
    pub max_atlas_pages: usize,
    pub atlas_page_extent: u32,
    pub max_glyph_extent: u32,
    pub max_new_glyphs_per_packet: usize,
    pub max_upload_bytes_per_packet: usize,
    pub max_diagnostics_per_packet: usize,
}

impl Default for UiRenderLimits {
    fn default() -> Self {
        Self {
            max_glyph_entries: DEFAULT_MAX_GLYPH_ENTRIES,
            max_atlas_pages: DEFAULT_MAX_ATLAS_PAGES,
            atlas_page_extent: DEFAULT_ATLAS_PAGE_EXTENT,
            max_glyph_extent: DEFAULT_MAX_GLYPH_EXTENT,
            max_new_glyphs_per_packet: DEFAULT_MAX_NEW_GLYPHS_PER_PACKET,
            max_upload_bytes_per_packet: DEFAULT_MAX_UPLOAD_BYTES_PER_PACKET,
            max_diagnostics_per_packet: DEFAULT_MAX_DIAGNOSTICS_PER_PACKET,
        }
    }
}

impl UiRenderLimits {
    pub(crate) fn validate(self) -> Result<Self, UiRenderLimitsError> {
        let page_bytes = self
            .atlas_page_extent
            .checked_mul(self.atlas_page_extent)
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(UiRenderLimitsError::InvalidField {
                field: "atlas_page_extent",
            })?;
        if self.max_glyph_entries == 0 {
            return Err(UiRenderLimitsError::InvalidField {
                field: "max_glyph_entries",
            });
        }
        if self.max_atlas_pages == 0 || self.max_atlas_pages > usize::from(u16::MAX) {
            return Err(UiRenderLimitsError::InvalidField {
                field: "max_atlas_pages",
            });
        }
        if self.atlas_page_extent == 0 || self.atlas_page_extent > i32::MAX as u32 {
            return Err(UiRenderLimitsError::InvalidField {
                field: "atlas_page_extent",
            });
        }
        if self.max_glyph_extent == 0 || self.max_glyph_extent > self.atlas_page_extent {
            return Err(UiRenderLimitsError::InvalidField {
                field: "max_glyph_extent",
            });
        }
        if self.max_new_glyphs_per_packet == 0 {
            return Err(UiRenderLimitsError::InvalidField {
                field: "max_new_glyphs_per_packet",
            });
        }
        if self.max_upload_bytes_per_packet < page_bytes {
            return Err(UiRenderLimitsError::InvalidField {
                field: "max_upload_bytes_per_packet",
            });
        }
        if self.max_diagnostics_per_packet == 0 {
            return Err(UiRenderLimitsError::InvalidField {
                field: "max_diagnostics_per_packet",
            });
        }
        Ok(self)
    }

    pub(crate) fn page_byte_len(self) -> usize {
        usize::try_from(self.atlas_page_extent * self.atlas_page_extent)
            .expect("validated atlas page extent fits usize")
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiRenderLimitsError {
    #[error("UI render limit `{field}` is outside the supported bounded policy")]
    InvalidField { field: &'static str },
}

#[derive(Clone)]
struct AtlasPage {
    allocator: AtlasAllocator,
    pixels: Arc<Vec<u8>>,
    generation: u64,
}

impl AtlasPage {
    fn new(limits: UiRenderLimits) -> Self {
        let extent =
            i32::try_from(limits.atlas_page_extent).expect("validated atlas page extent fits i32");
        Self {
            allocator: AtlasAllocator::new(size2(extent, extent)),
            pixels: Arc::new(vec![0; limits.page_byte_len()]),
            generation: 0,
        }
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

#[derive(Clone, Debug)]
pub(crate) struct AtlasDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

#[derive(Clone)]
pub(crate) struct GlyphAtlas {
    font_revision: Option<u64>,
    pages: Vec<AtlasPage>,
    glyphs: HashMap<GlyphRasterKey, GlyphSlot>,
    next_generation: u64,
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self {
            font_revision: None,
            pages: Vec::new(),
            glyphs: HashMap::new(),
            next_generation: 1,
        }
    }
}

impl GlyphAtlas {
    pub(crate) fn prepare(
        &mut self,
        font_revision: u64,
        requested: &[GlyphRasterKey],
        engine: &mut TextEngine,
        limits: UiRenderLimits,
    ) -> Vec<AtlasDiagnostic> {
        let mut staged = if self.font_revision == Some(font_revision) {
            self.clone()
        } else {
            Self {
                font_revision: Some(font_revision),
                ..Self::default()
            }
        };
        let mut diagnostics = Vec::new();
        let mut seen = HashSet::with_capacity(requested.len());
        let mut dirty_pages = BTreeSet::new();
        let mut newly_rasterized = 0_usize;
        let mut admission_closed = false;

        for key in requested.iter().copied() {
            if !seen.insert(key) || staged.glyphs.contains_key(&key) {
                continue;
            }
            if admission_closed {
                continue;
            }
            if staged.glyphs.len() == limits.max_glyph_entries {
                push_diagnostic(
                    &mut diagnostics,
                    limits,
                    "glyph-cache-saturated",
                    "the bounded glyph cache is saturated; the affected glyph was omitted",
                );
                continue;
            }
            if newly_rasterized == limits.max_new_glyphs_per_packet {
                push_diagnostic(
                    &mut diagnostics,
                    limits,
                    "glyph-raster-deferred",
                    "the per-packet glyph raster budget was exhausted; remaining new glyphs were deferred",
                );
                admission_closed = true;
                continue;
            }

            let raster = match engine.rasterize(key) {
                Ok(raster) => raster,
                Err(error) => {
                    push_diagnostic(
                        &mut diagnostics,
                        limits,
                        "glyph-raster-rejected",
                        &format!("glyph rasterization failed: {error}"),
                    );
                    continue;
                }
            };
            newly_rasterized = newly_rasterized.saturating_add(1);
            if raster.width() == 0 || raster.height() == 0 {
                staged.glyphs.insert(key, GlyphSlot::Empty);
                continue;
            }
            if raster.width() > limits.max_glyph_extent || raster.height() > limits.max_glyph_extent
            {
                push_diagnostic(
                    &mut diagnostics,
                    limits,
                    "glyph-extent-rejected",
                    &format!(
                        "glyph raster {}x{} exceeds the {} pixel extent limit",
                        raster.width(),
                        raster.height(),
                        limits.max_glyph_extent
                    ),
                );
                continue;
            }

            match allocate_raster(&mut staged, &raster, limits, &mut dirty_pages) {
                RasterAdmission::Admitted(slot) => {
                    staged.glyphs.insert(key, slot);
                }
                RasterAdmission::Deferred => {
                    push_diagnostic(
                        &mut diagnostics,
                        limits,
                        "glyph-upload-deferred",
                        "the per-packet atlas upload budget was exhausted; remaining new glyphs were deferred",
                    );
                    admission_closed = true;
                }
                RasterAdmission::Saturated => {
                    push_diagnostic(
                        &mut diagnostics,
                        limits,
                        "glyph-atlas-saturated",
                        "all bounded glyph-atlas pages are saturated; the affected glyph was omitted",
                    );
                }
            }
        }

        for page_index in dirty_pages {
            let page = &mut staged.pages[page_index];
            page.generation = staged.next_generation;
            staged.next_generation = staged.next_generation.saturating_add(1).max(1);
        }
        *self = staged;
        diagnostics
    }

    #[must_use]
    pub(crate) fn slot(&self, key: GlyphRasterKey) -> Option<GlyphSlot> {
        self.glyphs.get(&key).copied()
    }

    pub(crate) fn snapshots(&self) -> Vec<UiGlyphAtlasPageSnapshot> {
        let font_revision = self.font_revision.unwrap_or(1);
        self.pages
            .iter()
            .enumerate()
            .map(|(page_index, page)| {
                UiGlyphAtlasPageSnapshot::new(
                    UiGlyphAtlasPageKey::new(
                        font_revision,
                        u16::try_from(page_index).expect("page limit validated against u16"),
                        page.generation,
                    ),
                    page_extent(&page.allocator),
                    page_extent(&page.allocator),
                    Arc::clone(&page.pixels),
                )
            })
            .collect()
    }
}

enum RasterAdmission {
    Admitted(GlyphSlot),
    Deferred,
    Saturated,
}

fn allocate_raster(
    atlas: &mut GlyphAtlas,
    raster: &GlyphRaster,
    limits: UiRenderLimits,
    dirty_pages: &mut BTreeSet<usize>,
) -> RasterAdmission {
    let page_bytes = limits.page_byte_len();
    let upload_bytes = dirty_pages.len().saturating_mul(page_bytes);
    let requested = size2(
        i32::try_from(raster.width()).expect("glyph extent limit fits i32"),
        i32::try_from(raster.height()).expect("glyph extent limit fits i32"),
    );

    for page_index in dirty_pages.iter().copied().collect::<Vec<_>>() {
        if let Some(allocation) = atlas.pages[page_index].allocator.allocate(requested) {
            return write_raster(atlas, page_index, allocation.rectangle, raster);
        }
    }

    if upload_bytes.saturating_add(page_bytes) > limits.max_upload_bytes_per_packet {
        return RasterAdmission::Deferred;
    }

    for page_index in 0..atlas.pages.len() {
        if dirty_pages.contains(&page_index) {
            continue;
        }
        if let Some(allocation) = atlas.pages[page_index].allocator.allocate(requested) {
            dirty_pages.insert(page_index);
            return write_raster(atlas, page_index, allocation.rectangle, raster);
        }
    }

    if atlas.pages.len() == limits.max_atlas_pages {
        return RasterAdmission::Saturated;
    }

    let page_index = atlas.pages.len();
    atlas.pages.push(AtlasPage::new(limits));
    let allocation = atlas.pages[page_index]
        .allocator
        .allocate(requested)
        .expect("validated glyph fits an empty atlas page");
    dirty_pages.insert(page_index);
    write_raster(atlas, page_index, allocation.rectangle, raster)
}

fn write_raster(
    atlas: &mut GlyphAtlas,
    page_index: usize,
    rectangle: guillotiere::Rectangle,
    raster: &GlyphRaster,
) -> RasterAdmission {
    let page = &mut atlas.pages[page_index];
    let page_width =
        usize::try_from(page.allocator.size().width).expect("validated page width fits usize");
    let min_x = usize::try_from(rectangle.min.x).expect("allocator returns non-negative x");
    let min_y = usize::try_from(rectangle.min.y).expect("allocator returns non-negative y");
    let glyph_width = usize::try_from(raster.width()).expect("glyph width fits usize");
    let glyph_height = usize::try_from(raster.height()).expect("glyph height fits usize");
    let pixels = Arc::make_mut(&mut page.pixels);
    for row in 0..glyph_height {
        let source_start = row * glyph_width;
        let destination_start = (min_y + row) * page_width + min_x;
        pixels[destination_start..destination_start + glyph_width]
            .copy_from_slice(&raster.mask()[source_start..source_start + glyph_width]);
    }
    RasterAdmission::Admitted(GlyphSlot::Atlas {
        page_index: u16::try_from(page_index).expect("page limit validated against u16"),
        min_x: u32::try_from(min_x).expect("page coordinate fits u32"),
        min_y: u32::try_from(min_y).expect("page coordinate fits u32"),
        width: raster.width(),
        height: raster.height(),
        left: raster.left(),
        top: raster.top(),
    })
}

fn page_extent(allocator: &AtlasAllocator) -> u32 {
    u32::try_from(allocator.size().width).expect("validated page extent fits u32")
}

fn push_diagnostic(
    diagnostics: &mut Vec<AtlasDiagnostic>,
    limits: UiRenderLimits,
    code: &'static str,
    message: &str,
) {
    if diagnostics.len() == limits.max_diagnostics_per_packet {
        return;
    }
    diagnostics.push(AtlasDiagnostic {
        code,
        message: message.to_owned(),
    });
}
