use std::collections::HashMap;

use guillotiere::{AtlasAllocator, size2};
use nara_ui::text::{
    FontFace, FontFaceError, GlyphRaster, GlyphRasterError, GlyphRasterKey, TextEngine,
    TextEngineCreateError, TextLimits, bundled_font_face,
};
use thiserror::Error;

pub(crate) const PAGE_SIZE: u32 = 1024;
const MAX_PAGES: usize = 4;
const MAX_GLYPHS: usize = 4096;

#[derive(Debug, Error)]
pub(crate) enum GlyphError {
    #[error(transparent)]
    BundledFont(#[from] FontFaceError),
    #[error(transparent)]
    TextEngine(#[from] TextEngineCreateError),
    #[error(transparent)]
    Raster(#[from] GlyphRasterError),
    #[error("HUD glyph exceeds the atlas page size")]
    TooLarge,
    #[error("HUD glyph atlas is full")]
    Full,
}

#[derive(Clone, Copy)]
pub(crate) struct GlyphSlot {
    pub(crate) page: usize,
    pub(crate) bounds: [u32; 4],
    pub(crate) left: i32,
    pub(crate) top: i32,
}

impl GlyphSlot {
    pub(crate) fn uv(self) -> [f32; 4] {
        self.bounds.map(|edge| edge as f32 / PAGE_SIZE as f32)
    }
}

pub(crate) struct GlyphPage {
    allocator: AtlasAllocator,
    pub(crate) pixels: Vec<u8>,
    pub(crate) dirty: Option<[u32; 4]>,
}

impl GlyphPage {
    fn new() -> Self {
        Self {
            allocator: AtlasAllocator::new(size2(PAGE_SIZE as i32, PAGE_SIZE as i32)),
            pixels: vec![0; (PAGE_SIZE * PAGE_SIZE) as usize],
            dirty: None,
        }
    }

    fn write(&mut self, page: usize, raster: &GlyphRaster) -> Option<GlyphSlot> {
        let width = raster.width();
        let height = raster.height();
        let allocation = self
            .allocator
            .allocate(size2(width as i32 + 2, height as i32 + 2))?;
        let min_x = allocation.rectangle.min.x as u32 + 1;
        let min_y = allocation.rectangle.min.y as u32 + 1;
        let bounds = [min_x, min_y, min_x + width, min_y + height];
        for (row, source) in raster.mask().chunks_exact(width as usize).enumerate() {
            let start = (min_y as usize + row) * PAGE_SIZE as usize + min_x as usize;
            self.pixels[start..start + width as usize].copy_from_slice(source);
        }
        self.dirty = Some(match self.dirty {
            Some([left, top, right, bottom]) => [
                left.min(bounds[0]),
                top.min(bounds[1]),
                right.max(bounds[2]),
                bottom.max(bounds[3]),
            ],
            None => bounds,
        });
        Some(GlyphSlot {
            page,
            bounds,
            left: raster.left(),
            top: raster.top(),
        })
    }
}

#[derive(Default)]
pub(crate) struct GlyphAtlas {
    fonts: Vec<FontFace>,
    text: Option<TextEngine>,
    glyphs: HashMap<GlyphRasterKey, Option<GlyphSlot>>,
    pub(crate) pages: Vec<GlyphPage>,
}

impl GlyphAtlas {
    pub(crate) fn set_fonts(&mut self, fonts: &[FontFace]) -> bool {
        if self
            .fonts
            .iter()
            .map(FontFace::id)
            .eq(fonts.iter().map(FontFace::id))
        {
            return false;
        }
        self.fonts = fonts.to_vec();
        self.text = None;
        self.clear();
        true
    }

    pub(crate) fn clear(&mut self) {
        self.glyphs.clear();
        self.pages.clear();
    }

    pub(crate) fn glyph(&mut self, key: GlyphRasterKey) -> Result<Option<GlyphSlot>, GlyphError> {
        if let Some(slot) = self.glyphs.get(&key) {
            return Ok(*slot);
        }
        if self.glyphs.len() == MAX_GLYPHS {
            return Err(GlyphError::Full);
        }
        let engine = match &mut self.text {
            Some(engine) => engine,
            empty @ None => {
                let fonts = if self.fonts.is_empty() {
                    vec![bundled_font_face()?]
                } else {
                    self.fonts.clone()
                };
                empty.insert(TextEngine::with_fonts(fonts, TextLimits::default())?)
            }
        };
        let raster = engine.rasterize(key)?;
        let slot = if raster.width() == 0 || raster.height() == 0 {
            None
        } else {
            Some(self.pack(&raster)?)
        };
        self.glyphs.insert(key, slot);
        Ok(slot)
    }

    fn pack(&mut self, raster: &GlyphRaster) -> Result<GlyphSlot, GlyphError> {
        if raster.width() > PAGE_SIZE - 2 || raster.height() > PAGE_SIZE - 2 {
            return Err(GlyphError::TooLarge);
        }
        for (index, page) in self.pages.iter_mut().enumerate() {
            if let Some(slot) = page.write(index, raster) {
                return Ok(slot);
            }
        }
        if self.pages.len() == MAX_PAGES {
            return Err(GlyphError::Full);
        }
        let index = self.pages.len();
        let mut page = GlyphPage::new();
        let slot = page.write(index, raster).ok_or(GlyphError::TooLarge)?;
        self.pages.push(page);
        Ok(slot)
    }
}
