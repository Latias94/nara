use std::{collections::BTreeMap, sync::Arc};

use nara_asset::{
    AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource, AssetPath,
    AssetProductRef, AssetRef, AssetReferenceDemand, AssetSourceKind, ImportedProductId,
    SourceAssetImporter,
};
use serde::{Deserialize, Serialize};

use super::{
    MAX_PATH_BYTES, MAX_RETAINED_BYTES, MAX_SHADER_BYTES, MAX_SHADER_INPUTS, MAX_SOURCE_FILE_BYTES,
    MAX_WORK_BYTES, Material2dAsset, MaterialAssetError, validate_defaults,
    wire::{self, DeclarationWire, ParametersWire, Record, byte_limit},
};
use crate::material::{Material2dDeclaration, Material2dGeneration, MaterialParameterValues};

/// Validated authoring data for one material, with an explicitly ordered raw WGSL input set.
///
/// Version one is bounded deterministic JSON. It preserves the declaration's actual uniform
/// offsets/size and texture bindings, complete defaults and authored durable texture selections.
/// Source identity comes exclusively from admitted metadata, never from the declaration's name.
#[derive(Debug, Clone, PartialEq)]
pub struct Material2dSource {
    declaration: Material2dDeclaration,
    shader_paths: Vec<AssetPath>,
    default_parameters: MaterialParameterValues,
    textures: BTreeMap<String, AssetRef>,
}

impl Material2dSource {
    pub fn new(
        declaration: Material2dDeclaration,
        shader_paths: Vec<AssetPath>,
        default_parameters: MaterialParameterValues,
        textures: BTreeMap<String, AssetRef>,
    ) -> Result<Self, MaterialAssetError> {
        validate_defaults(&declaration, &default_parameters, &textures)?;
        if shader_paths.is_empty() || shader_paths.len() > MAX_SHADER_INPUTS {
            return Err(MaterialAssetError::ShaderInputs);
        }
        for (index, path) in shader_paths.iter().enumerate() {
            if path.as_str().len() > MAX_PATH_BYTES {
                return Err(MaterialAssetError::Limit);
            }
            if shader_paths[..index].contains(path) {
                return Err(MaterialAssetError::ShaderInputs);
            }
        }
        Ok(Self {
            declaration,
            shader_paths,
            default_parameters,
            textures,
        })
    }

    #[must_use]
    pub const fn declaration(&self) -> &Material2dDeclaration {
        &self.declaration
    }

    #[must_use]
    pub fn shader_paths(&self) -> &[AssetPath] {
        &self.shader_paths
    }

    #[must_use]
    pub const fn default_parameters(&self) -> &MaterialParameterValues {
        &self.default_parameters
    }

    #[must_use]
    pub const fn textures(&self) -> &BTreeMap<String, AssetRef> {
        &self.textures
    }

    pub fn encode(&self) -> Result<Box<[u8]>, MaterialAssetError> {
        let wire = SourceWire {
            kind: Material2dDomain::SOURCE_KIND.to_owned(),
            version: 1,
            declaration: Record(DeclarationWire::from_declaration(&self.declaration)),
            shaders: self.shader_paths.clone(),
            params: wire::parameters_to_wire(&self.default_parameters),
            textures: self.textures.clone(),
        };
        wire::encode(&wire, MAX_SOURCE_FILE_BYTES, false)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MaterialAssetError> {
        let wire: SourceWire = wire::decode(bytes, MAX_SOURCE_FILE_BYTES, false)?;
        if wire.kind != Material2dDomain::SOURCE_KIND || wire.version != 1 {
            return Err(MaterialAssetError::Format);
        }
        Self::new(
            wire.declaration.0.into_declaration()?,
            wire.shaders,
            wire::parameters_from_wire(wire.params),
            wire.textures,
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceWire {
    kind: String,
    version: u32,
    declaration: Record<DeclarationWire>,
    shaders: Vec<AssetPath>,
    params: ParametersWire,
    textures: BTreeMap<String, AssetRef>,
}

/// The material Product family and stateless, capability-scoped source importer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Material2dDomain;

impl Material2dDomain {
    pub const SOURCE_KIND: &'static str = "material2d";
}

impl AssetDomain for Material2dDomain {
    type Product = Material2dAsset;

    fn contains_product(product: &Material2dAsset, id: ImportedProductId) -> bool {
        product.product().product() == id
    }
}

impl SourceAssetImporter for Material2dDomain {
    type Settings = ();

    const SOURCE_KIND: &'static str = Self::SOURCE_KIND;

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            byte_limit(MAX_SOURCE_FILE_BYTES),
            byte_limit(MAX_WORK_BYTES),
            byte_limit(MAX_RETAINED_BYTES),
        )
    }

    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<Material2dAsset, AssetImportError<E>> {
        let metadata = source
            .metadata()
            .admit::<()>()
            .map_err(|_| domain_error(MaterialAssetError::Metadata))?;
        if !matches!(
            metadata.identity().source_kind(),
            AssetSourceKind::Other(kind) if kind == Self::SOURCE_KIND
        ) {
            return Err(domain_error(MaterialAssetError::Metadata));
        }
        let product =
            AssetProductRef::new(metadata.identity().stable_id(), ImportedProductId::PRIMARY)
                .map_err(|_| domain_error(MaterialAssetError::ProductIdentity))?;
        let material = Material2dSource::decode(source.bytes()).map_err(domain_error)?;
        let mut inputs = Vec::with_capacity(material.shader_paths.len());
        let mut remaining = MAX_SHADER_BYTES - material.shader_paths.len();
        for path in &material.shader_paths {
            if remaining == 0 {
                return Err(domain_error(MaterialAssetError::Limit));
            }
            let bytes = source.read_dependency(path, byte_limit(remaining))?;
            remaining = remaining
                .checked_sub(bytes.len())
                .ok_or_else(|| domain_error(MaterialAssetError::Limit))?;
            let body = std::str::from_utf8(&bytes)
                .map_err(|_| domain_error(MaterialAssetError::ShaderEncoding))?;
            inputs.push((path.clone(), Arc::from(body)));
        }
        let generation = Material2dGeneration::from_shader_inputs(material.declaration, inputs)
            .map_err(|error| domain_error(error.into()))?;
        let product = Material2dAsset::new(
            product,
            generation,
            material.default_parameters,
            material.textures,
        )
        .map_err(domain_error)?;
        super::runtime::retained_bytes(&product).map_err(domain_error)?;
        Ok(product)
    }

    fn visit_source_dependencies<E>(
        product: &Material2dAsset,
        mut visitor: impl FnMut(&AssetRef, &AssetSourceKind, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        for reference in product.textures().values() {
            visitor(
                reference,
                &AssetSourceKind::Image,
                AssetReferenceDemand::Immediate,
            )?;
        }
        Ok(())
    }
}

fn domain_error<E>(error: MaterialAssetError) -> AssetImportError<E> {
    AssetImportError::domain(error.diagnostic())
}
