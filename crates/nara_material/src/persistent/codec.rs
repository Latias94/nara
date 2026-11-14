use std::collections::BTreeMap;

use nara_app::{App, Plugin, PluginError, PluginPreflightContext};
use nara_asset::{
    AssetRecord, AssetRef, AssetRefError, AssetServer, AssetSourceKind, AssetSourcePath, Assets,
    Handle,
};
use nara_image::ImageAsset;
use nara_reflect::{
    ComponentApplyContext, ComponentCodecError, ComponentDecodeContext, ComponentRegistryBuilder,
    ComponentRegistryError, ComponentSchemaCatalog, ComponentSchemaProviderSourceError,
    ComponentTypeId, PreparedComponentCandidate,
};

use crate::asset::{Material2dAsset, Material2dDomain};

use super::{Material2d, Material2dRecord, material2d_schema};

/// Installs original material asset storage and the native durable material carrier.
#[derive(Clone, Copy, Debug, Default)]
pub struct Material2dPlugin;

pub const MATERIAL_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.material2d");
pub const MATERIAL_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.material2d.components");
pub const MATERIAL_SCHEMA_OWNER_ID: nara_reflect::ComponentSchemaOwnerId =
    nara_reflect::ComponentSchemaOwnerId::new("nara.material2d.components");
pub const MATERIAL_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        MATERIAL_SCHEMA_OWNER_ID,
        MATERIAL_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new("nara.material2d.components.native", 1),
        material_schema_catalog,
        validate_material_components,
        register_material_components,
    );
const MATERIAL_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("runtime-2d")];
pub const MATERIAL_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(MATERIAL_PLUGIN_ID, nara_app::PluginCategory::Runtime)
        .requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
        .requires_product_capabilities(MATERIAL_PRODUCT_REQUIREMENTS)
        .provides_schema(&[MATERIAL_SCHEMA_PROVIDER_ID]);

impl Plugin for Material2dPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &MATERIAL_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        nara_reflect::validate_schema_provider_for_plugin(
            context,
            MATERIAL_PLUGIN_ID,
            "nara.material2d",
            &MATERIAL_SCHEMA_PROVIDER,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Assets<Material2dAsset>>()?;
        nara_reflect::register_schema_provider_for_plugin(
            app,
            MATERIAL_PLUGIN_ID,
            "nara.material2d",
            &MATERIAL_SCHEMA_PROVIDER,
        )
    }
}

/// Registers one material carrier without selecting assets or material families.
pub fn register_material_components(
    registry: &mut ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    validate_material_components(registry)?;
    registry.register_persistent_component_codec_with_context::<Material2d, _, _>(
        material2d_schema(),
        |value, context| {
            let record = Material2dRecord::from_component_value_with_limits(
                value,
                context.materialization_limits(),
            )?;
            let material = prepare_reference::<Material2dAsset>(
                context,
                record.material(),
                &AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
                "material",
            )?;
            let mut overrides = BTreeMap::new();
            for (id, reference) in record.texture_overrides() {
                overrides.insert(
                    id.clone(),
                    prepare_reference::<ImageAsset>(
                        context,
                        reference,
                        &AssetSourceKind::Image,
                        &format!("texture_overrides.{id}"),
                    )?,
                );
            }
            Ok(PreparedComponentCandidate::with_asset_server(
                move |context| {
                    let mut texture_overrides = BTreeMap::new();
                    for (id, prepared) in overrides {
                        let handle = resolve_prepared_reference(
                            context,
                            prepared,
                            &format!("texture_overrides.{id}"),
                        )?;
                        texture_overrides.insert(id, handle);
                    }
                    Ok(Material2d {
                        handle: resolve_prepared_reference(context, material, "material")?,
                        declaration_id: record.declaration_id().to_owned(),
                        declaration_version: record.declaration_version(),
                        parameters: record.parameters().clone(),
                        texture_overrides,
                    })
                },
            ))
        },
        |material, world, context| {
            let server = world.get_resource::<AssetServer>().ok_or_else(|| {
                ComponentCodecError::Message("AssetServer resource is missing".to_owned())
            })?;
            let material_ref = AssetRef::from_handle_with_policy(
                server,
                material.handle,
                context.asset_path_scope(),
                context.asset_ref_export_policy(),
            )
            .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
            let mut texture_overrides = BTreeMap::new();
            for (id, handle) in &material.texture_overrides {
                let reference = AssetRef::from_handle_with_policy(
                    server,
                    *handle,
                    context.asset_path_scope(),
                    context.asset_ref_export_policy(),
                )
                .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                texture_overrides.insert(id.clone(), reference);
            }
            Material2dRecord {
                material: material_ref,
                declaration_id: material.declaration_id.clone(),
                declaration_version: material.declaration_version,
                parameters: material.parameters.clone(),
                texture_overrides,
            }
            .to_component_value_with_limits(context.materialization_limits())
        },
    )?;
    Ok(())
}

fn material_schema_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![material2d_schema()],
        ..ComponentSchemaCatalog::default()
    })
}

fn validate_material_components(
    registry: &ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    registry.validate_component_registration::<Material2d>(&ComponentTypeId::new("nara.material2d"))
}

enum PreparedReference<T> {
    Resolved(Handle<T>),
    Path(AssetSourcePath),
    Record(AssetRecord),
}

fn prepare_reference<T: 'static>(
    context: &mut ComponentDecodeContext<'_>,
    reference: &AssetRef,
    source_kind: &AssetSourceKind,
    field: &str,
) -> Result<PreparedReference<T>, ComponentCodecError> {
    if context.asset_resolution().is_some()
        && let Some(product) = reference.as_product()
        && !reference.is_primary()
    {
        return Err(reference_error(
            field,
            reference,
            AssetRefError::UnsupportedProductSelection(product),
        ));
    }
    if let AssetRef::Path(path) = reference {
        let bytes = context
            .asset_path_scope()
            .prefix()
            .map_or(0, |prefix| prefix.as_str().len().saturating_add(1))
            .saturating_add(path.as_str().len());
        validate_path_size(bytes, context)?;
    }
    if let Some(result) = context.resolve_asset_ref_with_kind::<T>(reference, source_kind) {
        return result
            .map(PreparedReference::Resolved)
            .map_err(|error| reference_error(field, reference, error));
    }
    if let Some(resolver) = context.asset_resolution() {
        let record = reference
            .validate_with_database_as(resolver, source_kind)
            .map_err(|error| reference_error(field, reference, error))?;
        validate_path_size(record.path().as_str().len(), context)?;
        return Ok(PreparedReference::Record(record.clone()));
    }
    match reference {
        AssetRef::Path(path) => Ok(PreparedReference::Path(
            context.asset_path_scope().resolve(path),
        )),
        AssetRef::StableId(id) => Err(reference_error(
            field,
            reference,
            AssetRefError::MissingProjectDatabase(*id),
        )),
        AssetRef::Product(product) => Err(reference_error(
            field,
            reference,
            AssetRefError::MissingProjectDatabase(product.source()),
        )),
    }
}

fn validate_path_size(
    bytes: usize,
    context: &ComponentDecodeContext<'_>,
) -> Result<(), ComponentCodecError> {
    let maximum = context.materialization_limits().bytes();
    if bytes > maximum {
        return Err(ComponentCodecError::MaterializationByteLimit {
            observed: bytes,
            maximum,
        });
    }
    Ok(())
}

fn resolve_prepared_reference<T: 'static>(
    context: &mut ComponentApplyContext,
    reference: PreparedReference<T>,
    field: &str,
) -> Result<Handle<T>, ComponentCodecError> {
    match reference {
        PreparedReference::Resolved(handle) => Ok(handle),
        PreparedReference::Path(path) => context.reserve_asset_path(&path).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(field, path.as_str(), error.to_string())
        }),
        PreparedReference::Record(record) => {
            context.reserve_asset_record(&record).map_err(|error| {
                ComponentCodecError::invalid_asset_ref(
                    field,
                    record.path().as_str(),
                    error.to_string(),
                )
            })
        }
    }
}

fn reference_error(field: &str, reference: &AssetRef, error: AssetRefError) -> ComponentCodecError {
    ComponentCodecError::invalid_asset_ref(field, reference.to_string(), error.to_string())
}
