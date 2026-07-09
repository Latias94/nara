//! Backend-neutral material descriptors shared by sprite, tilemap, UI, and future render domains.

use nara_asset::AssetRef;
use nara_core::Color;

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

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Material2dDescriptor {
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub image: Option<AssetRef>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: Color,
}

impl Material2dDescriptor {
    #[must_use]
    pub fn from_color(tint: Color) -> Self {
        Self {
            image: None,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint,
        }
    }

    #[must_use]
    pub fn from_image(image: AssetRef) -> Self {
        Self {
            image: Some(image),
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint: Color::WHITE,
        }
    }

    #[must_use]
    pub const fn with_sampler(mut self, sampler: SamplerDescriptor) -> Self {
        self.sampler = sampler;
        self
    }

    #[must_use]
    pub const fn with_alpha_mode(mut self, alpha_mode: AlphaMode2d) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }

    #[must_use]
    pub const fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }

    #[must_use]
    pub fn key(&self) -> Material2dKey {
        material2d_descriptor_key(self)
    }
}

impl Default for Material2dDescriptor {
    fn default() -> Self {
        Self::from_color(Color::WHITE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Material2dKey([u8; 32]);

impl Material2dKey {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[must_use]
pub fn material2d_descriptor_key(descriptor: &Material2dDescriptor) -> Material2dKey {
    let mut hasher = blake3::Hasher::new();
    write_image_ref(&mut hasher, descriptor.image.as_ref());
    write_sampler(&mut hasher, descriptor.sampler);
    hasher.update(&[alpha_mode_tag(descriptor.alpha_mode)]);
    write_color(&mut hasher, descriptor.tint);
    Material2dKey::from_bytes(*hasher.finalize().as_bytes())
}

fn write_image_ref(hasher: &mut blake3::Hasher, image: Option<&AssetRef>) {
    match image {
        None => {
            hasher.update(&[0]);
        }
        Some(AssetRef::Path(path)) => {
            hasher.update(&[1]);
            write_str(hasher, path.as_str());
        }
        Some(AssetRef::StableId(id)) => {
            hasher.update(&[2]);
            write_str(hasher, &id.to_string());
        }
    };
}

fn write_sampler(hasher: &mut blake3::Hasher, sampler: SamplerDescriptor) {
    hasher.update(&[
        filter_mode_tag(sampler.min_filter),
        filter_mode_tag(sampler.mag_filter),
        filter_mode_tag(sampler.mipmap_filter),
        address_mode_tag(sampler.address_mode_u),
        address_mode_tag(sampler.address_mode_v),
    ]);
}

fn write_color(hasher: &mut blake3::Hasher, color: Color) {
    hasher.update(&color.r.to_le_bytes());
    hasher.update(&color.g.to_le_bytes());
    hasher.update(&color.b.to_le_bytes());
    hasher.update(&color.a.to_le_bytes());
}

fn write_str(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn filter_mode_tag(filter_mode: FilterMode) -> u8 {
    match filter_mode {
        FilterMode::Nearest => 1,
        FilterMode::Linear => 2,
    }
}

fn address_mode_tag(address_mode: AddressMode) -> u8 {
    match address_mode {
        AddressMode::ClampToEdge => 1,
        AddressMode::Repeat => 2,
        AddressMode::MirrorRepeat => 3,
    }
}

fn alpha_mode_tag(alpha_mode: AlphaMode2d) -> u8 {
    match alpha_mode {
        AlphaMode2d::Opaque => 1,
        AlphaMode2d::Blend => 2,
    }
}

pub mod prelude {
    pub use crate::{
        AddressMode, AlphaMode2d, FilterMode, Material2dDescriptor, Material2dKey,
        SamplerDescriptor, material2d_descriptor_key,
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

    #[test]
    fn material_key_changes_when_material_intent_changes() {
        let base = Material2dDescriptor::from_image(
            AssetRef::path("textures/player.png").expect("test path is valid"),
        );

        assert_ne!(
            base.key(),
            base.clone()
                .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
                .key()
        );
        assert_ne!(
            base.key(),
            base.clone().with_alpha_mode(AlphaMode2d::Opaque).key()
        );
        assert_ne!(base.key(), base.clone().with_tint(Color::BLACK).key());
        assert_ne!(
            base.key(),
            Material2dDescriptor::from_image(
                AssetRef::path("textures/enemy.png").expect("test path is valid"),
            )
            .key()
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn material_descriptor_roundtrips_through_json() {
        let descriptor = Material2dDescriptor::from_image(
            AssetRef::path("textures/player.png").expect("test path is valid"),
        )
        .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
        .with_alpha_mode(AlphaMode2d::Opaque)
        .with_tint(Color::rgba(0.25, 0.5, 0.75, 1.0));

        let json = serde_json::to_string(&descriptor).unwrap();
        let decoded: Material2dDescriptor = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, descriptor);
    }
}
