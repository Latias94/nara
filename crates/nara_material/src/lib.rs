//! Shared sampling and alpha semantics, plus one bounded 2D fragment-material extension.

use nara_asset::Handle;
use nara_ecs::{Component, Resource};
use nara_image::ImageAsset;

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

/// The only custom fragment input currently supported by the stock 2D renderer.
///
/// The engine supplies the sampled primary image, linear tint and UV to
/// `fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32>`.
/// Custom WGSL may read `material_parameters: vec4<f32>` and
/// `material_texture: texture_2d<f32>`. It must return linear RGB and straight alpha.
/// Shader source is trusted Rust executable content and changes require a restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Material2dInput {
    pub parameter_vectors: u32,
    pub additional_textures: u32,
}

impl Material2dInput {
    pub const SPRITE_FRAGMENT_V1: Self = Self {
        parameter_vectors: 1,
        additional_textures: 1,
    };
}

/// A trusted startup declaration for the bounded sprite/tile fragment extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Material2dDefinition {
    pub id: &'static str,
    pub fragment_shader: &'static str,
    pub input: Material2dInput,
    pub alpha_mode: AlphaMode2d,
}

/// Immutable startup declarations. Instances retain the admitted definition by value.
#[derive(Debug, Clone, Resource)]
pub struct Material2dDefinitions {
    definitions: &'static [Material2dDefinition],
}

impl Material2dDefinitions {
    pub fn new(definitions: &'static [Material2dDefinition]) -> Result<Self, Material2dError> {
        for (index, definition) in definitions.iter().enumerate() {
            if definition.id.is_empty() || definition.fragment_shader.trim().is_empty() {
                return Err(Material2dError::EmptyDefinition);
            }
            if definition.input != Material2dInput::SPRITE_FRAGMENT_V1 {
                return Err(Material2dError::UnsupportedInput {
                    id: definition.id,
                    requested: definition.input,
                });
            }
            if definitions[..index]
                .iter()
                .any(|other| other.id == definition.id)
            {
                return Err(Material2dError::DuplicateDefinition(definition.id));
            }
        }
        Ok(Self { definitions })
    }

    /// Selects an admitted shader and validates its one parameter vector.
    /// The second image is a normal process-local asset reference; preparation captures its revision.
    pub fn instantiate(
        &self,
        id: &str,
        parameters: [f32; 4],
        texture: Handle<ImageAsset>,
    ) -> Result<Material2dInstance, Material2dError> {
        let definition = self
            .definitions
            .iter()
            .find(|definition| definition.id == id)
            .ok_or_else(|| Material2dError::UnknownDefinition(id.to_owned()))?;
        if !parameters.iter().all(|value| value.is_finite()) {
            return Err(Material2dError::NonFiniteParameters);
        }
        Ok(Material2dInstance {
            definition: *definition,
            parameters,
            texture,
        })
    }
}

/// Transient shading attached to a Sprite or Tilemap entity.
///
/// The definition owns the draw's alpha mode; the Sprite/TileSet still owns its primary
/// image, sampler and tint. This component is deliberately outside durable Scene codecs.
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Material2dInstance {
    definition: Material2dDefinition,
    parameters: [f32; 4],
    texture: Handle<ImageAsset>,
}

impl Material2dInstance {
    #[must_use]
    pub const fn definition(self) -> Material2dDefinition {
        self.definition
    }

    #[must_use]
    pub const fn parameters(self) -> [f32; 4] {
        self.parameters
    }

    #[must_use]
    pub const fn texture(self) -> Handle<ImageAsset> {
        self.texture
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Material2dError {
    #[error("material definitions require a nonempty id and shader")]
    EmptyDefinition,
    #[error("material definition {0} is registered more than once")]
    DuplicateDefinition(&'static str),
    #[error(
        "material {id} requests unsupported fragment inputs {requested:?}; supported inputs are one vec4 and one additional 2D texture"
    )]
    UnsupportedInput {
        id: &'static str,
        requested: Material2dInput,
    },
    #[error("material {0} is not registered at startup")]
    UnknownDefinition(String),
    #[error("material parameters must be finite")]
    NonFiniteParameters,
}

pub mod prelude {
    pub use crate::{
        AddressMode, AlphaMode2d, FilterMode, Material2dDefinition, Material2dDefinitions,
        Material2dError, Material2dInput, Material2dInstance, SamplerDescriptor,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFINITION: Material2dDefinition = Material2dDefinition {
        id: "test.palette",
        fragment_shader: "fn material_fragment() {}",
        input: Material2dInput::SPRITE_FRAGMENT_V1,
        alpha_mode: AlphaMode2d::Blend,
    };
    static DEFINITIONS: [Material2dDefinition; 1] = [DEFINITION];

    #[test]
    fn sampler_default_is_linear_clamp() {
        assert_eq!(
            SamplerDescriptor::default(),
            SamplerDescriptor::LINEAR_CLAMP
        );
    }

    #[test]
    fn instances_select_registered_shader_parameters_texture_and_alpha() {
        let definitions = Material2dDefinitions::new(&DEFINITIONS).unwrap();
        let texture = nara_asset::allocate_anonymous_handle().unwrap();
        let instance = definitions
            .instantiate("test.palette", [0.5, 1.0, 2.0, 3.0], texture)
            .unwrap();
        assert_eq!(instance.definition(), DEFINITION);
        assert_eq!(instance.parameters(), [0.5, 1.0, 2.0, 3.0]);
        assert_eq!(instance.texture(), texture);
        assert!(matches!(
            definitions.instantiate("absent", [0.0; 4], texture),
            Err(Material2dError::UnknownDefinition(_))
        ));
        assert_eq!(
            definitions.instantiate("test.palette", [f32::NAN; 4], texture),
            Err(Material2dError::NonFiniteParameters)
        );
    }

    #[test]
    fn startup_rejects_unsupported_inputs_and_duplicate_ids() {
        static UNSUPPORTED: [Material2dDefinition; 1] = [Material2dDefinition {
            input: Material2dInput {
                additional_textures: 2,
                ..Material2dInput::SPRITE_FRAGMENT_V1
            },
            ..DEFINITION
        }];
        static DUPLICATES: [Material2dDefinition; 2] = [DEFINITION, DEFINITION];
        assert!(matches!(
            Material2dDefinitions::new(&UNSUPPORTED),
            Err(Material2dError::UnsupportedInput { .. })
        ));
        assert!(matches!(
            Material2dDefinitions::new(&DUPLICATES),
            Err(Material2dError::DuplicateDefinition("test.palette"))
        ));
    }
}
