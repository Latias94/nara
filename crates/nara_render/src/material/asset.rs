//! Durable material products and their pure source and Runtime facets.

use std::{collections::BTreeMap, sync::Arc};

use crate::image::ImageAsset;
use nara_asset::{AssetProductRef, AssetRef, Handle};
use nara_core::diagnostic::{DiagnosticCode, ReportDiagnostic as Diagnostic, SafeSummary};

use crate::material::{
    Material2dBinding, Material2dDeclaration, Material2dGeneration, MaterialDeclarationError,
    MaterialParameterValues, MaterialUpdateError,
};

mod runtime;
mod source;
mod wire;

pub use runtime::Material2dRuntimeCodec;
pub use source::{Material2dDomain, Material2dSource};

const MAX_TEXT_BYTES: usize = 256;
const MAX_PATH_BYTES: usize = 4096;
const MAX_SHADER_INPUTS: usize = 32;
const MAX_SHADER_BYTES: usize = 1024 * 1024;
const MAX_SOURCE_FILE_BYTES: usize = 256 * 1024;
const MAX_RUNTIME_FILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_WORK_BYTES: usize = 128 * 1024 * 1024;
const MAX_RETAINED_BYTES: usize = 8 * 1024 * 1024;

/// One durable Product, its exact Shader generation and complete default bindings.
///
/// Publication and residency belong to the caller's original typed asset store. This value has
/// no current-generation lookup, startup-library membership, native resources or I/O authority.
#[derive(Debug, Clone, PartialEq)]
pub struct Material2dAsset {
    product: AssetProductRef,
    generation: Material2dGeneration,
    defaults: Arc<MaterialDefaultBindings>,
}

#[derive(Debug, PartialEq)]
struct MaterialDefaultBindings {
    default_parameters: MaterialParameterValues,
    textures: BTreeMap<String, AssetRef>,
}

impl Material2dAsset {
    fn new(
        product: AssetProductRef,
        generation: Material2dGeneration,
        default_parameters: MaterialParameterValues,
        textures: BTreeMap<String, AssetRef>,
    ) -> Result<Self, MaterialAssetError> {
        validate_defaults(generation.declaration(), &default_parameters, &textures)?;
        Ok(Self {
            product,
            generation,
            defaults: Arc::new(MaterialDefaultBindings {
                default_parameters,
                textures,
            }),
        })
    }

    #[must_use]
    pub const fn product(&self) -> AssetProductRef {
        self.product
    }

    #[must_use]
    pub const fn generation(&self) -> &Material2dGeneration {
        &self.generation
    }

    #[must_use]
    pub fn default_parameters(&self) -> &MaterialParameterValues {
        &self.defaults.default_parameters
    }

    #[must_use]
    pub fn textures(&self) -> &BTreeMap<String, AssetRef> {
        &self.defaults.textures
    }

    /// Models new default-binding storage allocated while resolving texture references.
    ///
    /// Parameter/key strings and map storage are copied; the generation and its acquisition are
    /// shared. The caller reserves this overlap before [`Self::resolve_texture_references`].
    /// Complete Product selections allocate no additional path strings.
    #[must_use]
    pub fn canonicalization_retained_bytes(&self) -> Option<usize> {
        use std::mem::size_of;
        let node = |entry: usize| entry.checked_mul(11)?.checked_add(14 * size_of::<usize>());
        let mut bytes = size_of::<MaterialDefaultBindings>().checked_add(2 * size_of::<usize>())?;
        for id in self.default_parameters().keys() {
            bytes = bytes
                .checked_add(node(size_of::<(
                    String,
                    crate::material::MaterialParameterValue,
                )>())?)?
                .checked_add(id.len())?;
        }
        for id in self.textures().keys() {
            bytes = bytes
                .checked_add(node(size_of::<(String, AssetRef)>())?)?
                .checked_add(id.len())?;
        }
        Some(bytes)
    }

    /// Shares the exact generation and defaults while retaining a caller-owned acquisition.
    ///
    /// Bindings and separately cloned Shader/input snapshots inherit this owner. The generation
    /// charges its decorated views and owner chain through
    /// [`Material2dGeneration::lifetime_owner_retained_bytes`] before allocation.
    #[must_use]
    pub fn with_lifetime_owner<T: Send + Sync + 'static>(&self, owner: Arc<T>) -> Self {
        Self {
            product: self.product,
            generation: self.generation.with_lifetime_owner(owner),
            defaults: Arc::clone(&self.defaults),
        }
    }

    /// Resolves every durable texture selection while retaining the exact generation.
    ///
    /// The caller owns scope, kind and residency validation. The callback receives the original
    /// borrowed reference and its concrete failure is returned unchanged. No Shader is recompiled.
    pub fn resolve_texture_references<E>(
        &self,
        mut resolve: impl FnMut(&AssetRef) -> Result<AssetProductRef, E>,
    ) -> Result<Self, E> {
        let textures = self
            .textures()
            .iter()
            .map(|(id, reference)| Ok((id.clone(), AssetRef::Product(resolve(reference)?))))
            .collect::<Result<_, E>>()?;
        Ok(Self {
            product: self.product,
            generation: self.generation.clone(),
            defaults: Arc::new(MaterialDefaultBindings {
                default_parameters: self.default_parameters().clone(),
                textures,
            }),
        })
    }

    /// Checks partial parameter overrides and texture IDs without allocating resource handles.
    pub fn validate_binding<'a>(
        &self,
        parameters: &MaterialParameterValues,
        override_ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), MaterialAssetError> {
        let layout = self.generation.declaration().parameters();
        if parameters.len() > layout.fields().len() {
            return Err(MaterialAssetError::BindingValues);
        }
        for (id, value) in parameters {
            bounded_text(id)?;
            let field = layout
                .fields()
                .iter()
                .find(|field| field.id() == id)
                .ok_or_else(|| MaterialDeclarationError::UnknownParameter(id.clone()))?;
            if value.kind() != field.kind() {
                return Err(MaterialDeclarationError::ParameterKindMismatch {
                    id: id.clone(),
                    expected: field.kind(),
                    actual: value.kind(),
                }
                .into());
            }
            value.validate(id)?;
        }
        for (index, id) in override_ids.into_iter().enumerate() {
            if index >= self.textures().len() {
                return Err(MaterialAssetError::TextureFields);
            }
            bounded_text(id)?;
            if !self.textures().contains_key(id) {
                return Err(MaterialAssetError::UnknownTexture(id.to_owned()));
            }
        }
        Ok(())
    }

    /// Merges overrides with complete defaults and binds this exact immutable generation.
    ///
    /// The resolver is called only for textures without an explicit handle override. It supplies
    /// already resident typed handles; this owner performs no loading or current-library lookup.
    pub fn bind_values(
        &self,
        parameters: MaterialParameterValues,
        overrides: BTreeMap<String, Handle<ImageAsset>>,
        mut resolver: impl FnMut(&AssetRef) -> Option<Handle<ImageAsset>>,
    ) -> Result<Material2dBinding, MaterialAssetError> {
        self.validate_binding(&parameters, overrides.keys().map(String::as_str))?;
        let mut merged = self.default_parameters().clone();
        merged.extend(parameters);
        let textures = self
            .textures()
            .iter()
            .map(|(id, reference)| {
                let handle = overrides
                    .get(id)
                    .copied()
                    .or_else(|| resolver(reference))
                    .ok_or_else(|| MaterialAssetError::UnresolvedTexture(id.clone()))?;
                Ok((id.clone(), handle))
            })
            .collect::<Result<_, MaterialAssetError>>()?;
        Ok(self.generation.bind_values(merged, textures)?)
    }
}

fn bounded_text(text: &str) -> Result<(), MaterialAssetError> {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES {
        return Err(MaterialAssetError::Limit);
    }
    Ok(())
}

fn validate_declaration(declaration: &Material2dDeclaration) -> Result<(), MaterialAssetError> {
    bounded_text(declaration.id())?;
    for field in declaration.parameters().fields() {
        bounded_text(field.id())?;
        bounded_text(field.name())?;
    }
    for texture in declaration.textures() {
        bounded_text(texture.id())?;
        bounded_text(texture.name())?;
    }
    Ok(())
}

fn validate_reference(reference: &AssetRef) -> Result<(), MaterialAssetError> {
    match reference {
        AssetRef::Path(path) if path.as_str().len() > MAX_PATH_BYTES => {
            Err(MaterialAssetError::Limit)
        }
        AssetRef::StableId(id) if id.as_uuid().is_nil() => Err(MaterialAssetError::ProductIdentity),
        AssetRef::Path(_) | AssetRef::StableId(_) | AssetRef::Product(_) => Ok(()),
    }
}

fn validate_defaults(
    declaration: &Material2dDeclaration,
    parameters: &MaterialParameterValues,
    textures: &BTreeMap<String, AssetRef>,
) -> Result<(), MaterialAssetError> {
    validate_declaration(declaration)?;
    if parameters.len() > crate::material::parameters::MAX_MATERIAL_PARAMETERS {
        return Err(MaterialAssetError::Limit);
    }
    for id in parameters.keys() {
        bounded_text(id)?;
    }
    declaration.parameters().validate_values(parameters)?;
    if textures.len() != declaration.textures().len()
        || declaration
            .textures()
            .iter()
            .any(|texture| !textures.contains_key(texture.id()))
    {
        return Err(MaterialAssetError::TextureFields);
    }
    for reference in textures.values() {
        validate_reference(reference)?;
    }
    Ok(())
}

/// A bounded typed rejection of durable material data or an exact binding attempt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MaterialAssetError {
    #[error(transparent)]
    Declaration(#[from] MaterialDeclarationError),
    #[error(transparent)]
    Generation(#[from] MaterialUpdateError),
    #[error("material data exceeds its finite format limits")]
    Limit,
    #[error("material JSON has an invalid, ambiguous or unsupported shape")]
    Wire,
    #[error("material format kind or version is unsupported")]
    Format,
    #[error("material source metadata requires kind material2d and unit settings")]
    Metadata,
    #[error("material requires a complete non-nil Product identity")]
    ProductIdentity,
    #[error("material requires a nonempty unique ordered Shader path set")]
    ShaderInputs,
    #[error("material Shader input is not UTF-8")]
    ShaderEncoding,
    #[error("material parameter overrides do not match the declaration")]
    BindingValues,
    #[error("material defaults require exactly the declared texture IDs")]
    TextureFields,
    #[error("unknown material texture ID {0}")]
    UnknownTexture(String),
    #[error("material texture {0} has no resident Image handle")]
    UnresolvedTexture(String),
    #[error("Runtime material textures require complete resolved Product references")]
    UnresolvedReference,
    #[error("material retained allocations exceed the reserved product model")]
    Retention,
}

impl MaterialAssetError {
    fn diagnostic(&self) -> Diagnostic {
        let (code, summary) = match self {
            Self::Declaration(_) | Self::BindingValues => (
                "material.values",
                "Material declaration or parameter values are invalid",
            ),
            Self::Generation(_) | Self::ShaderInputs | Self::ShaderEncoding => (
                "material.shader",
                "Material Shader source or its compiled interface is invalid",
            ),
            Self::Limit | Self::Retention => (
                "material.limit",
                "Material data exceeds the finite format or retention limits",
            ),
            Self::Wire | Self::Format => (
                "material.format",
                "Material data has an invalid or unsupported format",
            ),
            Self::Metadata | Self::ProductIdentity => (
                "material.identity",
                "Material metadata or complete Product identity is invalid",
            ),
            Self::TextureFields
            | Self::UnknownTexture(_)
            | Self::UnresolvedTexture(_)
            | Self::UnresolvedReference => (
                "material.textures",
                "Material texture references do not satisfy the declared bindings",
            ),
        };
        Diagnostic::error(
            DiagnosticCode::new(code).expect("fixed valid material diagnostic code"),
            SafeSummary::new(summary).expect("fixed safe material diagnostic summary"),
        )
    }
}

#[cfg(test)]
mod tests;
