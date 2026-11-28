//! Shared sampling/alpha policy and admitted 2D material declarations, values and Shader artifacts.

#[cfg(feature = "material")]
mod declaration;
#[cfg(feature = "material")]
mod library;
#[cfg(feature = "material")]
mod parameters;
#[cfg(feature = "runtime")]
mod runtime_loader;
#[cfg(feature = "material")]
mod shader;
#[cfg(feature = "material")]
mod sprite_asset;

#[cfg(feature = "material")]
pub use declaration::{
    Material2dDeclaration, MaterialDeclarationError, MaterialTextureDeclaration,
    MaterialUniformField,
};
#[cfg(feature = "material")]
pub use glam::{Vec2, Vec3, Vec4};
#[cfg(feature = "material")]
pub use library::{
    Material2dBinding, Material2dGeneration, Material2dLibrary, MaterialShaderInput,
    MaterialShaderInputObservation, MaterialUpdateError, PreparedMaterial2dUpdate,
};
pub use nara_asset::image::{AddressMode, FilterMode, SamplerDescriptor};
#[cfg(feature = "material")]
pub use nara_material_derive::MaterialParameters;
#[cfg(feature = "material")]
pub use parameters::{
    MaterialParameterKind, MaterialParameterLayout, MaterialParameterValue,
    MaterialParameterValues, MaterialParameters,
};
#[cfg(any(feature = "sprite-render", feature = "sprite-source-hydration"))]
pub(crate) use runtime_loader::register_sprite_material_loader;
#[cfg(feature = "runtime")]
pub use runtime_loader::{
    SpriteMaterialBindingAsset, SpriteMaterialLoader, SpriteMaterialLoaderError,
};
#[cfg(feature = "material")]
pub use shader::{
    __STOCK_QUAD_SHADER_SOURCE, __STOCK_VIEW_UNIFORM_BYTES, MaterialShaderArtifact,
    MaterialShaderError,
};
#[cfg(feature = "material")]
pub use sprite_asset::{SpriteMaterialAsset, SpriteMaterialAssetError};

#[doc(hidden)]
#[cfg(feature = "material")]
pub mod __macro_support {
    pub use crate::material::declaration::*;
    pub use crate::material::parameters::*;
    pub use encase;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AlphaMode2d {
    Opaque,
    Blend,
    /// Adds source RGB weighted by its alpha. Target alpha uses source-over coverage.
    Additive,
}

impl Default for AlphaMode2d {
    fn default() -> Self {
        Self::Blend
    }
}

pub mod prelude {
    pub use crate::material::{AddressMode, AlphaMode2d, FilterMode, SamplerDescriptor};
    #[cfg(feature = "material")]
    pub use crate::material::{
        Material2dBinding, Material2dDeclaration, Material2dGeneration, Material2dLibrary,
        MaterialDeclarationError, MaterialParameterKind, MaterialParameterLayout,
        MaterialParameterValue, MaterialParameterValues, MaterialParameters,
        MaterialShaderArtifact, MaterialShaderError, MaterialTextureDeclaration,
        MaterialUniformField, MaterialUpdateError, PreparedMaterial2dUpdate,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_default_is_linear_clamp() {
        assert_eq!(
            SamplerDescriptor::default(),
            SamplerDescriptor::LINEAR_CLAMP
        );
    }
}
