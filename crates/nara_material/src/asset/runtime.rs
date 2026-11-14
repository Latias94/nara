use std::{collections::BTreeMap, mem::size_of};

use nara_asset::{
    AssetProductRef, AssetRef, AssetReferenceDemand, AssetRuntimeError, AssetRuntimeLimits,
    RuntimeAssetCodec,
};
use serde::{Deserialize, Serialize};

use super::{
    MAX_RETAINED_BYTES, MAX_RUNTIME_FILE_BYTES, MAX_SHADER_BYTES, MAX_WORK_BYTES, Material2dAsset,
    MaterialAssetError, validate_defaults,
    wire::{self, DeclarationWire, ParametersWire, Record, byte_limit},
};
use crate::{Material2dGeneration, MaterialParameterValue};

/// Version one of the bounded, deterministic `runtime_material2d` Runtime JSON format.
///
/// This stores the complete Product identity, validated declaration, original user WGSL body,
/// full semantic defaults and only complete texture Product references. Local sequence numbers,
/// handles, native bytes and source observations are absent. Decode compiles the original body
/// once through the stock Naga authority and fabricates no source-read receipts.
///
/// The finite input and output reservations cover JSON preflight, DTOs, stock assembly and Naga
/// work. Retention charges the inline product, generation/declaration and source Arc allocations,
/// all semantic strings, field/texture vectors and conservative full BTreeMap nodes per entry.
/// Shared storage is charged in full. Encoded input and caller-owned Host wrappers are excluded.
/// Retained costing accepts original decoded/imported values; an attached opaque lifetime owner
/// is rejected because its separate acquisition charge belongs to the publishing content Host.
#[derive(Debug, Clone, Copy, Default)]
pub struct Material2dRuntimeCodec;

impl Material2dRuntimeCodec {
    pub const RUNTIME_KIND: &'static str = "runtime_material2d";
    pub const RUNTIME_VERSION: u32 = 1;
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeWire {
    kind: String,
    version: u32,
    product: AssetProductRef,
    declaration: Record<DeclarationWire>,
    user_body: String,
    params: ParametersWire,
    textures: BTreeMap<String, AssetProductRef>,
}

impl RuntimeAssetCodec<Material2dAsset> for Material2dRuntimeCodec {
    const RUNTIME_KIND: &'static str = Self::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = Self::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(
            byte_limit(MAX_RUNTIME_FILE_BYTES),
            byte_limit(MAX_WORK_BYTES),
            byte_limit(MAX_RETAINED_BYTES),
        )
    }

    fn encode_runtime(product: &Material2dAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
        let encode = || {
            validate_defaults(
                product.generation().declaration(),
                product.default_parameters(),
                product.textures(),
            )?;
            if product.generation().shader().user_source().len() > MAX_SHADER_BYTES {
                return Err(MaterialAssetError::Limit);
            }
            let textures = product
                .textures()
                .iter()
                .map(|(id, reference)| match reference {
                    AssetRef::Product(reference) => Ok((id.clone(), *reference)),
                    AssetRef::Path(_) | AssetRef::StableId(_) => {
                        Err(MaterialAssetError::UnresolvedReference)
                    }
                })
                .collect::<Result<_, _>>()?;
            let wire = RuntimeWire {
                kind: Self::RUNTIME_KIND.to_owned(),
                version: Self::RUNTIME_VERSION,
                product: product.product(),
                declaration: Record(DeclarationWire::from_declaration(
                    product.generation().declaration(),
                )),
                user_body: product.generation().shader().user_source().to_owned(),
                params: wire::parameters_to_wire(product.default_parameters()),
                textures,
            };
            wire::encode(&wire, MAX_RUNTIME_FILE_BYTES, true)
        };
        encode().map_err(runtime_error)
    }

    fn decode_runtime(bytes: &[u8]) -> Result<Material2dAsset, AssetRuntimeError> {
        let decode = || {
            let wire: RuntimeWire = wire::decode(bytes, MAX_RUNTIME_FILE_BYTES, true)?;
            if wire.kind != Self::RUNTIME_KIND || wire.version != Self::RUNTIME_VERSION {
                return Err(MaterialAssetError::Format);
            }
            let declaration = wire.declaration.0.into_declaration()?;
            let parameters = wire::parameters_from_wire(wire.params);
            let textures = wire
                .textures
                .into_iter()
                .map(|(id, reference)| (id, AssetRef::Product(reference)))
                .collect();
            validate_defaults(&declaration, &parameters, &textures)?;
            let generation =
                Material2dGeneration::from_shader_source(declaration, &wire.user_body)?;
            let product = Material2dAsset::new(wire.product, generation, parameters, textures)?;
            retained_bytes(&product)?;
            Ok(product)
        };
        decode().map_err(runtime_error)
    }

    fn runtime_retained_bytes(product: &Material2dAsset) -> Result<usize, AssetRuntimeError> {
        retained_bytes(product).map_err(runtime_error)
    }

    fn visit_runtime_dependencies<E>(
        product: &Material2dAsset,
        mut visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        for reference in product.textures().values() {
            if let AssetRef::Product(reference) = reference {
                visitor(*reference, AssetReferenceDemand::Immediate)?;
            }
        }
        Ok(())
    }

    fn contains_runtime_product(product: &Material2dAsset, reference: AssetProductRef) -> bool {
        product.product() == reference
    }
}

pub(super) fn retained_bytes(product: &Material2dAsset) -> Result<usize, MaterialAssetError> {
    let count = || {
        let mut bytes = size_of::<Material2dAsset>()
            .checked_add(product.generation().product_retained_bytes()?)?
            .checked_add(size_of::<super::MaterialDefaultBindings>())?
            .checked_add(2 * size_of::<usize>())?;
        let parameter_node = btree_node_bytes::<String, MaterialParameterValue>()?;
        for id in product.default_parameters().keys() {
            bytes = bytes
                .checked_add(parameter_node)?
                .checked_add(id.capacity())?;
        }
        let texture_node = btree_node_bytes::<String, AssetRef>()?;
        for (id, reference) in product.textures() {
            bytes = bytes
                .checked_add(texture_node)?
                .checked_add(id.capacity())?;
            if let AssetRef::Path(path) = reference {
                bytes = bytes.checked_add(path.as_str().len())?;
            }
        }
        Some(bytes)
    };
    count()
        .filter(|bytes| *bytes <= MAX_RETAINED_BYTES)
        .ok_or(MaterialAssetError::Retention)
}

fn btree_node_bytes<K, V>() -> Option<usize> {
    size_of::<(K, V)>()
        .checked_mul(11)?
        .checked_add(12 * size_of::<usize>())?
        .checked_add(2 * size_of::<usize>())
}

fn runtime_error(error: MaterialAssetError) -> AssetRuntimeError {
    AssetRuntimeError::new(error.diagnostic())
}
