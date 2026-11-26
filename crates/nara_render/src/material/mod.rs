//! Shared sampling/alpha policy and admitted 2D material declarations, values and Shader artifacts.

#[cfg(feature = "material-asset")]
mod asset;
#[cfg(feature = "material")]
mod declaration;
#[cfg(feature = "material")]
mod library;
#[cfg(feature = "material")]
mod parameters;
#[cfg(feature = "persistent")]
mod persistent;
#[cfg(feature = "material")]
mod shader;

#[cfg(feature = "material-asset")]
pub use asset::{
    Material2dAsset, Material2dContentReceipt, Material2dDomain, Material2dPublicationError,
    Material2dPublicationInput, Material2dPublicationMode, Material2dRuntimeCodec,
    Material2dSource, MaterialAssetError, PreparedMaterial2dPublications,
};
#[cfg(feature = "persistent")]
pub use persistent::{
    MATERIAL_PLUGIN_DECLARATION, MATERIAL_PLUGIN_ID, MATERIAL_SCHEMA_OWNER_ID,
    MATERIAL_SCHEMA_PROVIDER, MATERIAL_SCHEMA_PROVIDER_ID, Material2d, Material2dPlugin,
    Material2dRecord, material2d_schema, register_material_components,
};

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
#[cfg(feature = "material")]
pub use nara_material_derive::MaterialParameters;
#[cfg(feature = "material")]
pub use parameters::{
    MaterialParameterKind, MaterialParameterLayout, MaterialParameterValue,
    MaterialParameterValues, MaterialParameters,
};
#[cfg(feature = "material")]
pub use shader::{
    __STOCK_QUAD_SHADER_SOURCE, __STOCK_VIEW_UNIFORM_BYTES, MaterialShaderArtifact,
    MaterialShaderError,
};

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
pub enum FilterMode {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SamplerDescriptor {
    pub min_filter: FilterMode,
    pub mag_filter: FilterMode,
    pub mipmap_filter: FilterMode,
    pub address_mode_u: AddressMode,
    pub address_mode_v: AddressMode,
}

impl SamplerDescriptor {
    pub const LINEAR_CLAMP: Self = Self {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
    };

    pub const NEAREST_CLAMP: Self = Self {
        min_filter: FilterMode::Nearest,
        mag_filter: FilterMode::Nearest,
        mipmap_filter: FilterMode::Nearest,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
    };
}

impl Default for SamplerDescriptor {
    fn default() -> Self {
        Self::LINEAR_CLAMP
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AlphaMode2d {
    Opaque,
    Blend,
}

impl Default for AlphaMode2d {
    fn default() -> Self {
        Self::Blend
    }
}

pub mod prelude {
    pub use crate::material::{AddressMode, AlphaMode2d, FilterMode, SamplerDescriptor};
    #[cfg(feature = "persistent")]
    pub use crate::material::{Material2d, Material2dPlugin, Material2dRecord};
    #[cfg(feature = "material-asset")]
    pub use crate::material::{
        Material2dAsset, Material2dDomain, Material2dRuntimeCodec, Material2dSource,
        MaterialAssetError,
    };
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
