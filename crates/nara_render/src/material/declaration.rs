//! Validated material identity and the bounded stock 2D binding contract.

use std::collections::BTreeSet;

use crate::material::AlphaMode2d;
use crate::material::parameters::{
    MaterialParameterKind, MaterialParameterLayout, MaterialParameters,
};

pub use crate::material::parameters::MaterialUniformField;

/// The bind group reserved for a material's uniform and additional textures.
pub const MATERIAL_BIND_GROUP: u32 = 1;
/// The material parameter uniform's binding within [`MATERIAL_BIND_GROUP`].
pub const MATERIAL_UNIFORM_BINDING: u32 = 0;
/// The maximum number of additional sampled 2D textures in a material.
pub const MAX_MATERIAL_TEXTURES: usize = 8;

/// A stable texture identity paired with its actual WGSL global name and binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialTextureDeclaration {
    id: String,
    name: String,
    binding: u32,
}

impl MaterialTextureDeclaration {
    /// Declares a texture at binding 1 through 8 of the material bind group.
    ///
    /// `id` is persistent semantic identity; `name` is the shader symbol. Renaming
    /// the shader symbol does not change the texture's semantic identity.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        binding: u32,
    ) -> Result<Self, MaterialDeclarationError> {
        let id = id.into();
        let name = name.into();
        validate_id(&id, "texture")?;
        validate_name(&name, "texture")?;
        if !(1..=MAX_MATERIAL_TEXTURES as u32).contains(&binding) {
            return Err(MaterialDeclarationError::InvalidTextureBinding { id, binding });
        }
        Ok(Self { id, name, binding })
    }

    /// Returns the persistent semantic identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the actual WGSL global name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the binding within [`MATERIAL_BIND_GROUP`].
    #[must_use]
    pub const fn binding(&self) -> u32 {
        self.binding
    }
}

/// A material family's semantic declaration and concrete uniform/texture ABI.
///
/// Construction checks bounded declaration data. Admission must additionally join
/// this complete layout with the actual Naga shader interface before GPU use.
/// The same constructor accepts layouts from Rust parameters or a shader author.
/// The engine-owned group 2 View uniform is outside this authored declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Material2dDeclaration {
    id: String,
    version: u32,
    alpha_mode: AlphaMode2d,
    parameters: MaterialParameterLayout,
    textures: Vec<MaterialTextureDeclaration>,
    semantic_digest: [u8; 32],
    binding_digest: [u8; 32],
}

impl Material2dDeclaration {
    /// Validates a material family, its nonzero version and texture declarations.
    pub fn new(
        id: impl Into<String>,
        version: u32,
        alpha_mode: AlphaMode2d,
        parameters: MaterialParameterLayout,
        textures: Vec<MaterialTextureDeclaration>,
    ) -> Result<Self, MaterialDeclarationError> {
        let id = id.into();
        validate_id(&id, "material")?;
        if version == 0 {
            return Err(MaterialDeclarationError::InvalidVersion(version));
        }
        if textures.len() > MAX_MATERIAL_TEXTURES {
            return Err(MaterialDeclarationError::TooManyTextures {
                count: textures.len(),
                max: MAX_MATERIAL_TEXTURES,
            });
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        for texture in &textures {
            if !ids.insert(texture.id()) {
                return Err(MaterialDeclarationError::DuplicateTextureId(
                    texture.id.clone(),
                ));
            }
            if !names.insert(texture.name()) {
                return Err(MaterialDeclarationError::DuplicateTextureName(
                    texture.name.clone(),
                ));
            }
            if !bindings.insert(texture.binding) {
                return Err(MaterialDeclarationError::DuplicateTextureBinding(
                    texture.binding,
                ));
            }
        }
        let mut declaration = Self {
            id,
            version,
            alpha_mode,
            parameters,
            textures,
            semantic_digest: [0; 32],
            binding_digest: [0; 32],
        };
        declaration.semantic_digest = declaration.compute_semantic_digest();
        declaration.binding_digest = declaration.compute_binding_digest();
        Ok(declaration)
    }

    /// Builds the same declaration using a typed parameter layout from encase.
    pub fn from_parameters<T: MaterialParameters>(
        id: impl Into<String>,
        version: u32,
        alpha_mode: AlphaMode2d,
        textures: Vec<MaterialTextureDeclaration>,
    ) -> Result<Self, MaterialDeclarationError> {
        Self::new(id, version, alpha_mode, T::parameter_layout()?, textures)
    }

    /// Returns the persistent material family identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the author's explicit semantic schema version.
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Returns the draw's alpha behavior.
    #[must_use]
    pub const fn alpha_mode(&self) -> AlphaMode2d {
        self.alpha_mode
    }

    /// Returns the concrete uniform layout.
    #[must_use]
    pub const fn parameters(&self) -> &MaterialParameterLayout {
        &self.parameters
    }

    /// Returns the declared textures in author-supplied order.
    #[must_use]
    pub fn textures(&self) -> &[MaterialTextureDeclaration] {
        &self.textures
    }

    /// Fingerprints material identity, version, alpha and stable parameter/texture IDs.
    ///
    /// Fields are sorted by semantic ID. Shader names, field order, byte offsets,
    /// texture bindings and Rust type identity are excluded. A shader-symbol rename
    /// or ABI relocation therefore preserves this digest when semantics are unchanged.
    /// The digest is computed once during construction.
    #[must_use]
    pub const fn semantic_digest(&self) -> [u8; 32] {
        self.semantic_digest
    }

    /// Fingerprints actual uniform size, field names/types/offsets and texture bindings.
    ///
    /// The material group and uniform binding are included. Semantic IDs, version,
    /// alpha and shader source are excluded. A pipeline key must separately include
    /// alpha behavior and the compiled shader fingerprint. The digest is computed
    /// once during construction. The engine-owned View layout belongs to the
    /// compiled Shader interface digest; captured View/time values are excluded
    /// from both digests and do not create pipeline variants.
    #[must_use]
    pub const fn binding_digest(&self) -> [u8; 32] {
        self.binding_digest
    }

    fn compute_semantic_digest(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"nara.material.semantic.v1\0");
        hash_string(&mut hasher, &self.id);
        hasher.update(&self.version.to_le_bytes());
        hasher.update(&[match self.alpha_mode {
            AlphaMode2d::Opaque => 0,
            AlphaMode2d::Blend => 1,
        }]);
        let mut fields: Vec<_> = self.parameters.fields().iter().collect();
        fields.sort_unstable_by(|left, right| left.id().cmp(right.id()));
        hasher.update(&(fields.len() as u32).to_le_bytes());
        for field in fields {
            hash_string(&mut hasher, field.id());
            hash_kind(&mut hasher, field.kind());
        }
        let mut textures: Vec<_> = self.textures.iter().collect();
        textures.sort_unstable_by(|left, right| left.id().cmp(right.id()));
        hasher.update(&(textures.len() as u32).to_le_bytes());
        for texture in textures {
            hash_string(&mut hasher, texture.id());
        }
        *hasher.finalize().as_bytes()
    }

    fn compute_binding_digest(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"nara.material.binding.v1\0");
        hasher.update(&MATERIAL_BIND_GROUP.to_le_bytes());
        hasher.update(&MATERIAL_UNIFORM_BINDING.to_le_bytes());
        hasher.update(&self.parameters.byte_size().to_le_bytes());
        let mut fields: Vec<_> = self.parameters.fields().iter().collect();
        fields.sort_unstable_by_key(|field| field.offset());
        hasher.update(&(fields.len() as u32).to_le_bytes());
        for field in fields {
            hash_string(&mut hasher, field.name());
            hash_kind(&mut hasher, field.kind());
            hasher.update(&field.offset().to_le_bytes());
        }
        let mut textures: Vec<_> = self.textures.iter().collect();
        textures.sort_unstable_by_key(|texture| texture.binding());
        hasher.update(&(textures.len() as u32).to_le_bytes());
        for texture in textures {
            hash_string(&mut hasher, texture.name());
            hasher.update(&texture.binding().to_le_bytes());
        }
        *hasher.finalize().as_bytes()
    }
}

fn hash_string(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn hash_kind(hasher: &mut blake3::Hasher, kind: MaterialParameterKind) {
    hasher.update(&[match kind {
        MaterialParameterKind::F32 => 0,
        MaterialParameterKind::I32 => 1,
        MaterialParameterKind::U32 => 2,
        MaterialParameterKind::Vec2 => 3,
        MaterialParameterKind::Vec3 => 4,
        MaterialParameterKind::Vec4 => 5,
    }]);
}

pub(crate) fn validate_id(id: &str, target: &'static str) -> Result<(), MaterialDeclarationError> {
    if id.trim().is_empty() {
        return Err(MaterialDeclarationError::EmptyId { target });
    }
    if id.len() > 128
        || !id.as_bytes()[0].is_ascii_alphanumeric()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(MaterialDeclarationError::InvalidId { target });
    }
    Ok(())
}

pub(crate) fn validate_name(
    name: &str,
    target: &'static str,
) -> Result<(), MaterialDeclarationError> {
    if name.trim().is_empty() {
        return Err(MaterialDeclarationError::EmptyName { target });
    }
    Ok(())
}

/// An inspectable failure to declare, reconstruct or encode material parameters.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MaterialDeclarationError {
    /// A persistent identity was empty or whitespace-only.
    #[error("{target} requires a nonempty semantic id")]
    EmptyId { target: &'static str },
    /// A semantic identity cannot be represented by the shared persistent field contract.
    #[error(
        "{target} semantic id must be at most 128 ASCII bytes, start with an alphanumeric character and contain only alphanumerics, '.', '_' or '-'"
    )]
    InvalidId { target: &'static str },
    /// A shader symbol was empty or whitespace-only.
    #[error("{target} requires a nonempty shader name")]
    EmptyName { target: &'static str },
    /// Semantic declaration versions start at one.
    #[error("material declaration version must be greater than zero, received {0}")]
    InvalidVersion(u32),
    /// A uniform has no fields or exceeds the supported field count.
    #[error("material uniform has {count} fields; expected 1 through {max}")]
    InvalidParameterCount { count: usize, max: usize },
    /// A uniform has no bytes or exceeds the supported byte size.
    #[error("material uniform has {size} bytes; expected 1 through {max}")]
    InvalidUniformByteSize { size: u64, max: u32 },
    /// Two uniform fields have the same semantic identity.
    #[error("duplicate material parameter id {0}")]
    DuplicateParameterId(String),
    /// Two uniform fields have the same shader member name.
    #[error("duplicate material parameter shader name {0}")]
    DuplicateParameterName(String),
    /// A field's byte offset violates its actual encase alignment.
    #[error("material parameter {id} offset {offset} is not aligned to {alignment} bytes")]
    MisalignedParameter {
        id: String,
        offset: u32,
        alignment: u64,
    },
    /// A field extends beyond the uniform's byte range.
    #[error(
        "material parameter {id} at {offset} with size {size} exceeds uniform size {byte_size}"
    )]
    ParameterOutOfBounds {
        id: String,
        offset: u32,
        size: u64,
        byte_size: u32,
    },
    /// Two fields write intersecting byte ranges.
    #[error("material parameters {first} and {second} overlap")]
    OverlappingParameters { first: String, second: String },
    /// The uniform size violates the maximum actual field alignment.
    #[error("material uniform size {byte_size} is not aligned to {alignment} bytes")]
    MisalignedUniformSize { byte_size: u32, alignment: u64 },
    /// A value map lacks a declared semantic field.
    #[error("missing material parameter {0}")]
    MissingParameter(String),
    /// A value map contains a field absent from the declaration.
    #[error("unknown material parameter {0}")]
    UnknownParameter(String),
    /// A semantic value has the wrong scalar/vector type.
    #[error("material parameter {id} expects {expected:?}, received {actual:?}")]
    ParameterKindMismatch {
        id: String,
        expected: MaterialParameterKind,
        actual: MaterialParameterKind,
    },
    /// A scalar or vector component is NaN or infinite.
    #[error("material parameter {0} must contain only finite values")]
    NonFiniteParameter(String),
    /// An encase metadata offset cannot be represented by the public layout.
    #[error("material parameter byte offset {0} exceeds the u32 range")]
    UniformOffsetOverflow(u64),
    /// The real uniform encoder rejected a write.
    #[error("material uniform encoding failed: {0}")]
    UniformEncoding(String),
    /// A material declares too many additional textures.
    #[error("material has {count} textures; at most {max} are supported")]
    TooManyTextures { count: usize, max: usize },
    /// A texture uses the uniform binding or exceeds the texture binding range.
    #[error("material texture {id} binding {binding} must be in 1 through 8")]
    InvalidTextureBinding { id: String, binding: u32 },
    /// Two texture slots have the same semantic identity.
    #[error("duplicate material texture id {0}")]
    DuplicateTextureId(String),
    /// Two texture slots have the same shader global name.
    #[error("duplicate material texture shader name {0}")]
    DuplicateTextureName(String),
    /// Two textures occupy the same binding.
    #[error("duplicate material texture binding {0}")]
    DuplicateTextureBinding(u32),
}

#[cfg(test)]
mod tests;
