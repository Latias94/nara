use std::collections::BTreeMap;

use nara_core::{AssetId, Vec4};
use uuid::Uuid;

use super::{
    AlphaMode2d, Material2dBinding, Material2dDeclaration, Material2dGeneration,
    MaterialDeclarationError, MaterialParameterKind, MaterialParameterLayout,
    MaterialParameterValue, MaterialUniformField, MaterialUpdateError,
};

const HEADER: [u8; 8] = *b"NSMT\x01\0\0\0";
const ENCODED_LEN: usize = 40;
#[cfg(feature = "material-authoring")]
const MAX_SOURCE_LEN: usize = 64 * 1024;

/// An authored sprite material using the stock quad interface and one linear RGBA uniform.
/// The shader is a separate asset so multiple materials can share the same source.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "material-authoring", derive(serde::Deserialize))]
#[cfg_attr(
    feature = "material-authoring",
    serde(rename = "SpriteMaterial", deny_unknown_fields)
)]
pub struct SpriteMaterialAsset {
    #[cfg_attr(feature = "material-authoring", serde(deserialize_with = "asset_uuid"))]
    shader: AssetId,
    #[cfg_attr(feature = "material-authoring", serde(default = "white"))]
    color: [f32; 4],
}

impl SpriteMaterialAsset {
    pub fn new(shader: AssetId, color: Vec4) -> Result<Self, SpriteMaterialAssetError> {
        if !color.is_finite() {
            return Err(SpriteMaterialAssetError::NonFiniteColor);
        }
        Ok(Self {
            shader,
            color: color.to_array(),
        })
    }

    #[must_use]
    pub const fn shader(self) -> AssetId {
        self.shader
    }

    #[cfg(feature = "material-authoring")]
    pub fn from_ron(source: &str) -> Result<Self, SpriteMaterialAssetError> {
        if source.len() > MAX_SOURCE_LEN {
            return Err(SpriteMaterialAssetError::SourceTooLarge);
        }
        let asset: Self = ron::from_str(source)?;
        Self::new(asset.shader, Vec4::from_array(asset.color))
    }

    #[must_use]
    pub fn to_cooked(self) -> [u8; ENCODED_LEN] {
        let mut output = [0; ENCODED_LEN];
        output[..8].copy_from_slice(&HEADER);
        output[8..24].copy_from_slice(self.shader.as_uuid().as_bytes());
        for (channel, bytes) in self.color.iter().zip(output[24..].chunks_exact_mut(4)) {
            bytes.copy_from_slice(&channel.to_le_bytes());
        }
        output
    }

    pub fn from_cooked(bytes: &[u8]) -> Result<Self, SpriteMaterialAssetError> {
        let bytes: &[u8; ENCODED_LEN] = bytes
            .try_into()
            .map_err(|_| SpriteMaterialAssetError::InvalidCookedMaterial)?;
        if bytes[..8] != HEADER {
            return Err(SpriteMaterialAssetError::InvalidCookedMaterial);
        }
        let shader = AssetId::try_from_uuid(Uuid::from_bytes(bytes[8..24].try_into().unwrap()))
            .map_err(|_| SpriteMaterialAssetError::InvalidCookedMaterial)?;
        let color = std::array::from_fn(|index| {
            let start = 24 + index * 4;
            f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
        });
        Self::new(shader, Vec4::from_array(color))
    }

    /// Validates a WGSL `material_fragment` and binds this material to the resulting generation.
    /// The fragment can read the linear RGBA multiplier through `material_parameters.color`.
    pub fn bind(self, shader: &str) -> Result<Material2dBinding, SpriteMaterialAssetError> {
        let layout = MaterialParameterLayout::new(
            vec![MaterialUniformField::new(
                "color",
                "color",
                MaterialParameterKind::Vec4,
                0,
            )?],
            16,
        )?;
        let declaration = Material2dDeclaration::new(
            "nara.sprite-material",
            1,
            AlphaMode2d::Blend,
            layout,
            Vec::new(),
        )?;
        Ok(
            Material2dGeneration::from_shader_source(declaration, shader)?.bind_values(
                BTreeMap::from([("color".into(), MaterialParameterValue::Vec4(self.color))]),
                BTreeMap::new(),
            )?,
        )
    }
}

#[cfg(feature = "material-authoring")]
fn white() -> [f32; 4] {
    [1.0; 4]
}

#[cfg(feature = "material-authoring")]
fn asset_uuid<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<AssetId, D::Error> {
    <String as serde::Deserialize>::deserialize(deserializer)?
        .parse()
        .map_err(serde::de::Error::custom)
}

#[derive(Debug, thiserror::Error)]
pub enum SpriteMaterialAssetError {
    #[error("sprite material color must be finite")]
    NonFiniteColor,
    #[error("invalid cooked sprite material header, length, or shader ID")]
    InvalidCookedMaterial,
    #[cfg(feature = "material-authoring")]
    #[error("sprite material source exceeds 64 KiB")]
    SourceTooLarge,
    #[cfg(feature = "material-authoring")]
    #[error("invalid sprite material source: {0}")]
    Ron(#[from] ron::error::SpannedError),
    #[error(transparent)]
    Declaration(#[from] MaterialDeclarationError),
    #[error(transparent)]
    Material(#[from] MaterialUpdateError),
}

#[cfg(all(test, feature = "material-authoring"))]
mod tests {
    use super::*;

    #[test]
    fn sprite_material_uses_fixed_binary_values_and_the_wgsl_interface() {
        let source = r#"SpriteMaterial(shader: "0193a100-1111-7000-8000-000000000001", color: (0.5, 1.0, 2.0, 0.75))"#;
        let material = SpriteMaterialAsset::from_ron(source).unwrap();
        let bytes = material.to_cooked();
        assert_eq!(SpriteMaterialAsset::from_cooked(&bytes).unwrap(), material);
        assert!(SpriteMaterialAsset::from_cooked(&bytes[..39]).is_err());
        let mut nil = bytes;
        nil[8..24].fill(0);
        assert!(SpriteMaterialAsset::from_cooked(&nil).is_err());
        let shader = "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return sampled * tint * material_parameters.color; }";
        let binding = material.bind(shader).unwrap();
        assert_eq!(binding.uniform_bytes().len(), 16);
        assert!(material.bind("invalid WGSL").is_err());
    }
}
