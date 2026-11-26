use std::collections::{HashMap, hash_map::Entry};

use nara_asset::image::{ImageColorSpace, ImageContentHash, ImageFormat};

use crate::gpu::{
    RenderPassContext, WgpuRenderError,
    pass::{sampler_layout_entry, texture_layout_entry},
};
use crate::hud::{
    atlas::{GlyphAtlas, PAGE_SIZE},
    paint::{Geometry, MAX_IMAGES, Texture},
};

pub(super) struct Textures {
    pub(super) layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: Option<SampledTexture>,
    glyphs: Vec<SampledTexture>,
    images: HashMap<ImageContentHash, (SampledTexture, u64)>,
    frame: u64,
}

impl Textures {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nara_hud_texture"),
            entries: &[texture_layout_entry(0), sampler_layout_entry(1)],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nara_hud_texture"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            layout,
            sampler,
            white: None,
            glyphs: Vec::new(),
            images: HashMap::new(),
            frame: 0,
        }
    }

    pub(super) fn clear_glyphs(&mut self) {
        self.glyphs.clear();
    }

    pub(super) fn prepare(
        &mut self,
        ctx: &mut RenderPassContext<'_>,
        geometry: &Geometry,
        atlas: &mut GlyphAtlas,
    ) -> Result<(), WgpuRenderError> {
        if self.white.is_none()
            && geometry
                .draws
                .iter()
                .any(|draw| draw.texture == Texture::White)
        {
            let white = SampledTexture::new(
                ctx,
                &self.layout,
                &self.sampler,
                [1, 1],
                wgpu::TextureFormat::Rgba8Unorm,
            )?;
            white.write(ctx, [0, 0, 1, 1], 4, &[255; 4]);
            self.white = Some(white);
        }

        if geometry.glyph_count != 0 {
            for (index, page) in atlas.pages.iter_mut().enumerate() {
                if let Some(texture) = self.glyphs.get(index) {
                    if let Some(bounds @ [left, top, right, bottom]) = page.dirty.take() {
                        let start = (top * PAGE_SIZE + left) as usize;
                        let end = ((bottom - 1) * PAGE_SIZE + right) as usize;
                        texture.write(ctx, bounds, PAGE_SIZE, &page.pixels[start..end]);
                    }
                } else {
                    let texture = SampledTexture::new(
                        ctx,
                        &self.layout,
                        &self.sampler,
                        [PAGE_SIZE; 2],
                        wgpu::TextureFormat::R8Unorm,
                    )?;
                    texture.write(ctx, [0, 0, PAGE_SIZE, PAGE_SIZE], PAGE_SIZE, &page.pixels);
                    page.dirty = None;
                    self.glyphs.push(texture);
                }
            }
        }

        self.frame = self.frame.saturating_add(1);
        let mut spares = MAX_IMAGES.saturating_sub(geometry.images.len());
        self.images.retain(|hash, (_, used)| {
            if geometry.images.contains_key(hash) {
                return true;
            }
            if self.frame.saturating_sub(*used) > 2 || spares == 0 {
                return false;
            }
            spares -= 1;
            true
        });
        for (hash, image) in &geometry.images {
            let entry = match self.images.entry(*hash) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().1 = self.frame;
                    continue;
                }
                Entry::Vacant(entry) => entry,
            };
            let format = match (image.format(), image.color_space()) {
                (ImageFormat::Rgba8, ImageColorSpace::Linear) => wgpu::TextureFormat::Rgba8Unorm,
                (ImageFormat::Rgba8, ImageColorSpace::Srgb) => wgpu::TextureFormat::Rgba8UnormSrgb,
            };
            let extent = image.extent();
            let texture = SampledTexture::new(
                ctx,
                &self.layout,
                &self.sampler,
                [extent.width, extent.height],
                format,
            )?;
            texture.write(
                ctx,
                [0, 0, extent.width, extent.height],
                extent.width * 4,
                image.pixels(),
            );
            entry.insert((texture, self.frame));
        }
        Ok(())
    }

    pub(super) fn binding(&self, texture: Texture) -> Result<&wgpu::BindGroup, WgpuRenderError> {
        match texture {
            Texture::White => self.white.as_ref(),
            Texture::Glyph(page) => self.glyphs.get(page),
            Texture::Image(hash) => self.images.get(&hash).map(|(texture, _)| texture),
        }
        .map(|texture| &texture.binding)
        .ok_or_else(|| WgpuRenderError::Validation("missing HUD texture binding".into()))
    }
}

struct SampledTexture {
    texture: wgpu::Texture,
    binding: wgpu::BindGroup,
}

impl SampledTexture {
    fn new(
        ctx: &mut RenderPassContext<'_>,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        [width, height]: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> Result<Self, WgpuRenderError> {
        let maximum = ctx.device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > maximum || height > maximum {
            return Err(WgpuRenderError::Capacity("HUD texture dimensions"));
        }
        let texture = ctx.create_sampled_texture("nara_hud_texture", extent(width, height), format);
        let view = texture.create_view(&Default::default());
        let binding = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nara_hud_texture"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Ok(Self { texture, binding })
    }

    fn write(
        &self,
        ctx: &mut RenderPassContext<'_>,
        [left, top, right, bottom]: [u32; 4],
        stride: u32,
        pixels: &[u8],
    ) {
        ctx.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: left,
                    y: top,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: None,
            },
            extent(right - left, bottom - top),
        );
    }
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}
