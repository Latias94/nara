//! Durable material authoring, Scene records and the single native material carrier.
//!
//! Parameter payloads use `parameters.<kind>.<semantic-id>`, where the complete kind maps are
//! `f32`, `i32`, `u32`, `vec2`, `vec3` and `vec4`. Each semantic ID belongs to exactly one
//! kind map. Vectors contain a fixed-length list of finite floating-point values.
//! Semantic IDs remain literal map keys, including any dots within an ID.

use std::collections::BTreeMap;

use nara_asset::{AssetRef, Handle};
use nara_ecs::Component;
use nara_image::ImageAsset;
use nara_reflect::{ComponentCodecError, ComponentFieldId};

use crate::{
    Material2dBinding, Material2dDeclaration, MaterialDeclarationError, MaterialParameterValues,
    MaterialParameters, asset::Material2dAsset, declaration::MAX_MATERIAL_TEXTURES,
    parameters::MAX_MATERIAL_PARAMETERS,
};

mod codec;
mod record;
mod schema;

pub use codec::{
    MATERIAL_PLUGIN_DECLARATION, MATERIAL_PLUGIN_ID, MATERIAL_SCHEMA_OWNER_ID,
    MATERIAL_SCHEMA_PROVIDER, MATERIAL_SCHEMA_PROVIDER_ID, Material2dPlugin,
    register_material_components,
};
pub use record::Material2dRecord;
pub use schema::material2d_schema;

/// Authored material state whose handle selects the reusable material asset.
///
/// The complete semantic parameter map belongs to the component. Binding captures
/// one supplied asset generation and never consults the current World or library.
#[derive(Clone, Debug, PartialEq, Component)]
pub struct Material2d {
    handle: Handle<Material2dAsset>,
    declaration_id: String,
    declaration_version: u32,
    parameters: MaterialParameterValues,
    texture_overrides: BTreeMap<String, Handle<ImageAsset>>,
}

impl Material2d {
    /// Validates complete semantic values and the declared texture override IDs.
    pub fn new(
        handle: Handle<Material2dAsset>,
        declaration: &Material2dDeclaration,
        parameters: MaterialParameterValues,
        texture_overrides: BTreeMap<String, Handle<ImageAsset>>,
    ) -> Result<Self, ComponentCodecError> {
        validate_declaration(declaration, &parameters, &texture_overrides)?;
        Ok(Self {
            handle,
            declaration_id: declaration.id().to_owned(),
            declaration_version: declaration.version(),
            parameters,
            texture_overrides,
        })
    }

    /// Projects typed values only when their actual layout matches the declaration.
    pub fn from_parameters<T: MaterialParameters>(
        handle: Handle<Material2dAsset>,
        declaration: &Material2dDeclaration,
        parameters: &T,
        texture_overrides: BTreeMap<String, Handle<ImageAsset>>,
    ) -> Result<Self, ComponentCodecError> {
        if T::parameter_layout().map_err(declaration_error)? != *declaration.parameters() {
            return Err(ComponentCodecError::invalid_field(
                "parameters",
                "typed parameters matching the actual material declaration layout",
            ));
        }
        Self::new(
            handle,
            declaration,
            parameters.parameter_values().map_err(declaration_error)?,
            texture_overrides,
        )
    }

    #[must_use]
    pub const fn handle(&self) -> Handle<Material2dAsset> {
        self.handle
    }

    #[must_use]
    pub fn declaration_id(&self) -> &str {
        &self.declaration_id
    }

    #[must_use]
    pub const fn declaration_version(&self) -> u32 {
        self.declaration_version
    }

    #[must_use]
    pub fn parameters(&self) -> &MaterialParameterValues {
        &self.parameters
    }

    #[must_use]
    pub fn texture_overrides(&self) -> &BTreeMap<String, Handle<ImageAsset>> {
        &self.texture_overrides
    }

    /// Captures the supplied asset with complete authored values and effective textures.
    pub fn bind(
        &self,
        asset: &Material2dAsset,
        resolve_default: impl FnMut(&AssetRef) -> Option<Handle<ImageAsset>>,
    ) -> Result<Material2dBinding, ComponentCodecError> {
        validate_asset_values(
            &self.declaration_id,
            self.declaration_version,
            &self.parameters,
            &self.texture_overrides,
            asset,
        )?;
        asset
            .bind_values(
                self.parameters.clone(),
                self.texture_overrides.clone(),
                resolve_default,
            )
            .map_err(|error| ComponentCodecError::Message(error.to_string()))
    }
}

fn declaration_error(error: MaterialDeclarationError) -> ComponentCodecError {
    ComponentCodecError::Message(error.to_string())
}

fn validate_field_id(id: &str, field: &str) -> Result<ComponentFieldId, ComponentCodecError> {
    ComponentFieldId::try_new(id).map_err(|error| {
        ComponentCodecError::invalid_field(field, format!("a stable field ID: {error}"))
    })
}

fn validate_declaration<T>(
    declaration: &Material2dDeclaration,
    parameters: &MaterialParameterValues,
    overrides: &BTreeMap<String, T>,
) -> Result<(), ComponentCodecError> {
    validate_field_id(declaration.id(), "declaration_id")?;
    for field in declaration.parameters().fields() {
        validate_field_id(field.id(), "parameters")?;
    }
    for texture in declaration.textures() {
        validate_field_id(texture.id(), "texture_overrides")?;
    }
    validate_authored_values(
        declaration.id(),
        declaration.version(),
        parameters,
        overrides,
    )?;
    declaration
        .parameters()
        .validate_values(parameters)
        .map_err(declaration_error)?;
    for id in overrides.keys() {
        if !declaration
            .textures()
            .iter()
            .any(|texture| texture.id() == id)
        {
            return Err(ComponentCodecError::invalid_field(
                format!("texture_overrides.{id}"),
                "a texture ID in the material declaration",
            ));
        }
    }
    Ok(())
}

fn validate_authored_values<T>(
    declaration_id: &str,
    declaration_version: u32,
    parameters: &MaterialParameterValues,
    overrides: &BTreeMap<String, T>,
) -> Result<(), ComponentCodecError> {
    validate_field_id(declaration_id, "declaration_id")?;
    if declaration_version == 0 {
        return Err(ComponentCodecError::invalid_field(
            "declaration_version",
            "a nonzero u32",
        ));
    }
    if parameters.is_empty() || parameters.len() > MAX_MATERIAL_PARAMETERS {
        return Err(ComponentCodecError::invalid_field(
            "parameters",
            format!("1 through {MAX_MATERIAL_PARAMETERS} semantic parameters"),
        ));
    }
    for (id, value) in parameters {
        validate_field_id(id, "parameters")?;
        value.validate(id).map_err(declaration_error)?;
    }
    if overrides.len() > MAX_MATERIAL_TEXTURES {
        return Err(ComponentCodecError::invalid_field(
            "texture_overrides",
            format!("at most {MAX_MATERIAL_TEXTURES} texture overrides"),
        ));
    }
    for id in overrides.keys() {
        validate_field_id(id, "texture_overrides")?;
    }
    Ok(())
}

fn validate_asset_values<T>(
    declaration_id: &str,
    declaration_version: u32,
    parameters: &MaterialParameterValues,
    overrides: &BTreeMap<String, T>,
    asset: &Material2dAsset,
) -> Result<(), ComponentCodecError> {
    let declaration = asset.generation().declaration();
    if declaration_id != declaration.id() {
        return Err(ComponentCodecError::invalid_field(
            "declaration_id",
            format!("the actual material declaration ID {}", declaration.id()),
        ));
    }
    if declaration_version != declaration.version() {
        return Err(ComponentCodecError::invalid_field(
            "declaration_version",
            format!(
                "the actual material declaration version {}",
                declaration.version()
            ),
        ));
    }
    validate_declaration(declaration, parameters, overrides)?;
    for id in overrides.keys() {
        if !asset.textures().contains_key(id) {
            return Err(ComponentCodecError::invalid_field(
                format!("texture_overrides.{id}"),
                "a texture ID in the actual material asset",
            ));
        }
    }
    if asset.textures().len() != declaration.textures().len()
        || declaration
            .textures()
            .iter()
            .any(|texture| !asset.textures().contains_key(texture.id()))
    {
        return Err(ComponentCodecError::invalid_field(
            "texture_overrides",
            "complete default textures matching the actual material declaration",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
