//! Explicit program-local texture sources for original captured stock materials.

#[cfg(feature = "sprite-render")]
use crate::material::Material2dBinding;
#[cfg(feature = "sprite-render")]
use std::hash::{Hash, Hasher};

#[cfg(feature = "sprite-render")]
use crate::gpu::{AccessRole, WgpuProgramTexture};
use crate::gpu::{
    TextureInterpretation, TextureTransfer, WgpuProgramRead, WgpuProgramTextureDescriptor,
    WgpuRenderProgramError,
};

/// A typed resource read retained without changing its original semantic identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WgpuMaterialTextureSource {
    read: WgpuProgramRead,
    descriptor: WgpuProgramTextureDescriptor,
}

impl WgpuMaterialTextureSource {
    #[cfg(feature = "sprite-render")]
    fn new<S: 'static>(source: &WgpuProgramTexture<S>) -> Result<Self, WgpuRenderProgramError> {
        validate_source(source.descriptor())?;
        Ok(Self {
            read: source.read(AccessRole::Sampled),
            descriptor: source.descriptor().clone(),
        })
    }

    pub(crate) fn read(&self) -> WgpuProgramRead {
        self.read.clone()
    }

    #[cfg(feature = "sprite-render")]
    pub(crate) const fn descriptor(&self) -> &WgpuProgramTextureDescriptor {
        &self.descriptor
    }

    pub(crate) fn validate(&self) -> Result<(), WgpuRenderProgramError> {
        validate_source(&self.descriptor)
    }
}

fn validate_source(
    descriptor: &WgpuProgramTextureDescriptor,
) -> Result<(), WgpuRenderProgramError> {
    let native = &descriptor.native;
    if native.dimension != wgpu::TextureDimension::D2
        || native.size.width == 0
        || native.size.height == 0
        || native.size.depth_or_array_layers != 1
        || native.mip_level_count != 1
        || native.sample_count != 1
        || !native.view_formats.is_empty()
        || !native.usage.contains(wgpu::TextureUsages::TEXTURE_BINDING)
        || native.format.block_dimensions() != (1, 1)
        || !native.format.required_features().is_empty()
        || native
            .format
            .sample_type(None, Some(wgpu::Features::empty()))
            != Some(wgpu::TextureSampleType::Float { filterable: true })
    {
        return Err(WgpuRenderProgramError::feature(
            "material program textures require a baseline single-mip, single-sample, filterable 2D sampled texture",
        ));
    }
    let transfer = if native.format.has_srgb_suffix() {
        TextureTransfer::Srgb
    } else {
        TextureTransfer::Linear
    };
    if !matches!(
        descriptor.interpretation,
        TextureInterpretation::Color { transfer: actual, .. } if actual == transfer
    ) {
        return Err(WgpuRenderProgramError::feature(
            "material program textures require color interpretation matching their native sampling format",
        ));
    }
    Ok(())
}

/// Replaces one declared texture of one exact captured material for a stock world operation.
///
/// The source is a candidate-local typed version or an original Host-issued imported output.
/// The operation automatically declares the sampled read; a captured raw view cannot replace it.
/// Matching uses the original generation, exact uniform bytes and complete original Image-handle
/// map. Every matching batch occurrence uses this input. Unknown, duplicate and unused selections
/// reject; no mutable material registry or fake asset handle is created.
///
/// Native texture sampling performs its declared transfer conversion. Alpha interpretation is
/// exposed by [`Self::descriptor`] and is not automatically converted; the material Shader must
/// interpret it correctly and return the existing straight-alpha output. The original Sprite's
/// sampler remains in effect. Sources may have a different extent from the world target.
#[cfg(feature = "sprite-render")]
#[derive(Clone, Debug)]
pub struct WgpuMaterialTextureBinding {
    material: Material2dBinding,
    binding: u32,
    source: WgpuMaterialTextureSource,
}

#[cfg(feature = "sprite-render")]
pub(crate) struct MaterialSelection<'a>(pub(crate) &'a Material2dBinding);

#[cfg(feature = "sprite-render")]
impl PartialEq for MaterialSelection<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.generation() == other.0.generation()
            && self.0.uniform_bytes() == other.0.uniform_bytes()
            && self.0.textures() == other.0.textures()
    }
}

#[cfg(feature = "sprite-render")]
impl Eq for MaterialSelection<'_> {}

#[cfg(feature = "sprite-render")]
impl Hash for MaterialSelection<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.generation().hash(state);
        self.0.uniform_bytes().hash(state);
        self.0.textures().hash(state);
    }
}

#[cfg(feature = "sprite-render")]
impl WgpuMaterialTextureBinding {
    pub fn new<S: 'static>(
        material: &Material2dBinding,
        texture_id: &str,
        source: &WgpuProgramTexture<S>,
    ) -> Result<Self, WgpuRenderProgramError> {
        let texture = material
            .generation()
            .declaration()
            .textures()
            .iter()
            .find(|texture| texture.id() == texture_id)
            .ok_or_else(|| {
                WgpuRenderProgramError::feature("unknown material texture semantic ID")
            })?;
        Ok(Self {
            material: material.clone(),
            binding: texture.binding(),
            source: WgpuMaterialTextureSource::new(source)?,
        })
    }

    /// Original source extent, native format, transfer and alpha interpretation.
    #[must_use]
    pub const fn descriptor(&self) -> &WgpuProgramTextureDescriptor {
        self.source.descriptor()
    }

    pub(crate) fn selection(&self) -> MaterialSelection<'_> {
        MaterialSelection(&self.material)
    }

    pub(crate) const fn binding(&self) -> u32 {
        self.binding
    }

    pub(crate) fn source(&self) -> WgpuMaterialTextureSource {
        self.source.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_depth_cannot_be_used_as_an_ordinary_material_image() {
        let mut descriptor = WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            interpretation: TextureInterpretation::Depth,
        };
        assert!(validate_source(&descriptor).is_err());
        descriptor.native.format = wgpu::TextureFormat::Rgba8Unorm;
        assert!(validate_source(&descriptor).is_err());
        descriptor.interpretation = TextureInterpretation::Color {
            transfer: TextureTransfer::Linear,
            alpha: crate::gpu::TextureAlpha::Straight,
        };
        assert!(validate_source(&descriptor).is_ok());
    }
}
