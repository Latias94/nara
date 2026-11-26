use std::{mem::size_of, sync::Arc};

#[cfg(any(feature = "project-source", test))]
use nara_asset::AssetSourceKind;
#[cfg(feature = "project-source")]
use nara_asset::ImportedProductId;
use nara_asset::{AssetEvents, AssetProductRef, AssetRef, AssetServer, AssetStates, Assets};
use nara_ecs::{Res, ResMut, World, bevy_ecs::system::SystemState};
use nara_render::image::ImageAsset;
#[cfg(any(feature = "project-source", test))]
use nara_render::material::Material2dDomain;
use nara_render::material::{
    Material2dAsset, Material2dContentReceipt, Material2dPublicationError,
    Material2dPublicationInput, Material2dPublicationMode, PreparedMaterial2dPublications,
};
use nara_scene::advanced::SceneAssetReservations;

use super::{
    HostFault, ProjectSceneResidency, ResidencyInput, binding_changed, failure,
    images::ImagePublications, resources_missing, stock::StockChange,
};
#[cfg(feature = "project-source")]
use crate::{
    project_assets::ProjectDomainAssets,
    project_content::{ProjectContentLease, asset_record_retained_bytes},
};
use crate::{
    runtime_content::{RuntimeContentAsset, RuntimeContentLease},
    scene_retention::ARC_CONTROL_BLOCK_BYTES,
};

#[cfg(test)]
mod tests;

pub(super) enum MaterialMetadataLease {
    #[cfg(feature = "project-source")]
    Source {
        _lease: ProjectContentLease,
    },
    Processed {
        _lease: RuntimeContentLease,
    },
}

#[cfg(feature = "project-source")]
struct SourceMaterialOwner {
    _domains: ProjectDomainAssets,
    _metadata: Arc<MaterialMetadataLease>,
}

struct RuntimeMaterialOwner {
    _acquired: RuntimeContentAsset<Material2dAsset>,
    _metadata: Arc<MaterialMetadataLease>,
}

#[derive(Default)]
pub(super) struct MaterialPublications {
    batch: PreparedMaterial2dPublications,
    _batch_metadata_lease: Option<Arc<MaterialMetadataLease>>,
}

pub(super) fn reserve(
    content: ResidencyInput<'_>,
    change: StockChange<'_>,
) -> Result<Option<Arc<MaterialMetadataLease>>, HostFault> {
    let incoming = content.material_count();
    let current = change
        .current()
        .map_or(0, ProjectSceneResidency::material_count);
    if incoming == 0 && (change.is_additive() || current == 0) {
        return Ok(None);
    }
    let bytes = metadata_bytes(content, incoming, current)?;
    let lease = match content {
        #[cfg(feature = "project-source")]
        ResidencyInput::Source(snapshot) => MaterialMetadataLease::Source {
            _lease: snapshot
                .reserve_consumer_retained(bytes)
                .map_err(|error| HostFault::new(error.diagnostics().clone()))?,
        },
        ResidencyInput::Processed(content) => MaterialMetadataLease::Processed {
            _lease: content.base().reserve_retained(bytes).map_err(|_| {
                failure(
                    "project.scene.residency-budget-exceeded",
                    "Prepared Scene exceeds its original Runtime content metadata budget",
                )
            })?,
        },
    };
    Ok(Some(Arc::new(lease)))
}

pub(super) fn prepare(
    world: &World,
    content: ResidencyInput<'_>,
    change: StockChange<'_>,
    bindings: &mut SceneAssetReservations,
    images: &ImagePublications,
    metadata: Option<Arc<MaterialMetadataLease>>,
) -> Result<MaterialPublications, HostFault> {
    let Some(metadata) = metadata else {
        return Ok(MaterialPublications::default());
    };
    let owned = change
        .current()
        .into_iter()
        .flat_map(ProjectSceneResidency::owned_materials);
    let (server, values, states) = resources(world)?;
    let mode = if change.is_additive() {
        Material2dPublicationMode::Add
    } else {
        Material2dPublicationMode::Replace
    };
    let batch = match content {
        #[cfg(feature = "project-source")]
        ResidencyInput::Source(snapshot) => {
            let collection = snapshot.domain_assets().collection::<Material2dDomain>();
            if collection.is_some_and(|collection| {
                collection.source_records().len() != collection.iter().len()
            }) {
                return Err(binding_changed());
            }
            let resolver = snapshot.asset_resolution();
            let kind = material_kind();
            let inputs = collection
                .into_iter()
                .flat_map(|collection| collection.source_records().zip(collection.iter()))
                .map(|(record, original)| {
                    let product = original.product();
                    let reference = AssetRef::Product(product);
                    if record.source_kind() != &kind
                        || product.source() != record.stable_id()
                        || product.product() != ImportedProductId::PRIMARY
                        || resolver.resolve_ref(&reference).ok() != Some(record)
                    {
                        return Err(binding_changed());
                    }
                    let handle = bindings
                        .reserve(&reference, resolver, &kind)
                        .map_err(|_| binding_changed())?;
                    let owner = Arc::new(SourceMaterialOwner {
                        _domains: snapshot.domain_assets().clone(),
                        _metadata: Arc::clone(&metadata),
                    });
                    let staged = bindings.staged_server();
                    Material2dPublicationInput::resolve(
                        handle,
                        Some(record.path().clone()),
                        original,
                        staged,
                        Arc::clone(&metadata),
                        owner,
                        |reference| {
                            let record = reference
                                .validate_with_database_as(resolver, &AssetSourceKind::Image)
                                .map_err(|_| dependency_missing())?;
                            let product = AssetProductRef::new(
                                record.stable_id(),
                                ImportedProductId::PRIMARY,
                            )
                            .map_err(|_| binding_changed())?;
                            planned_image(images, staged, product)
                        },
                    )
                });
            PreparedMaterial2dPublications::prepare::<HostFault>(
                inputs, owned, mode, server, values, states,
            )
        }
        ResidencyInput::Processed(content) => {
            let inputs = content.materials.iter().map(|acquired| {
                let original = acquired.product();
                if !content.base().same_base(acquired.base())
                    || acquired.reference() != original.product()
                {
                    return Err(binding_changed());
                }
                let handle = bindings
                    .reserve_product(original.product())
                    .map_err(|_| binding_changed())?;
                let owner = Arc::new(RuntimeMaterialOwner {
                    _acquired: acquired.clone(),
                    _metadata: Arc::clone(&metadata),
                });
                let staged = bindings.staged_server();
                Material2dPublicationInput::resolve(
                    handle,
                    None,
                    original,
                    staged,
                    Arc::clone(&metadata),
                    owner,
                    |reference| {
                        let AssetRef::Product(product) = reference else {
                            return Err(dependency_missing());
                        };
                        planned_image(images, staged, *product)
                    },
                )
            });
            PreparedMaterial2dPublications::prepare::<HostFault>(
                inputs, owned, mode, server, values, states,
            )
        }
    }?;
    batch.validate_bindings(bindings.staged_server())?;
    Ok(MaterialPublications {
        batch,
        _batch_metadata_lease: Some(metadata),
    })
}

pub(super) fn retire<'receipt>(
    owned: impl Iterator<Item = &'receipt Material2dContentReceipt> + Clone,
    world: &World,
) -> Result<MaterialPublications, HostFault> {
    if owned.clone().next().is_none() {
        return Ok(MaterialPublications::default());
    }
    let (server, values, states) = resources(world)?;
    let batch = PreparedMaterial2dPublications::prepare(
        std::iter::empty::<Result<_, Material2dPublicationError>>(),
        owned,
        Material2dPublicationMode::Replace,
        server,
        values,
        states,
    )?;
    Ok(MaterialPublications {
        batch,
        _batch_metadata_lease: None,
    })
}

impl MaterialPublications {
    pub(super) fn additive_products(&self) -> Vec<AssetProductRef> {
        self.batch.additive_products()
    }

    pub(super) fn retiring_products(&self) -> impl Iterator<Item = AssetProductRef> + '_ {
        self.batch.retiring_products()
    }

    pub(super) fn validate(&self, world: &World) -> Result<(), HostFault> {
        if self.batch.is_empty() {
            return Ok(());
        }
        let (server, values, states) = resources(world)?;
        self.batch.validate(server, values, states)?;
        Ok(())
    }

    pub(super) fn commit(self, world: &mut World) -> Vec<Material2dContentReceipt> {
        if self.batch.is_empty() {
            return Vec::new();
        }
        let mut access = SystemState::<(
            Res<AssetServer>,
            ResMut<Assets<Material2dAsset>>,
            ResMut<AssetStates>,
            ResMut<AssetEvents>,
        )>::new(world);
        let (server, mut values, mut states, mut events) = access
            .get_mut(world)
            .expect("Material residency resources passed preflight");
        self.batch
            .commit(&server, &mut values, &mut states, &mut events)
            .expect("Scene Material residency passed preflight before Scene commit")
    }
}

pub(super) fn resources(
    world: &World,
) -> Result<(&AssetServer, &Assets<Material2dAsset>, &AssetStates), HostFault> {
    if !world.contains_resource::<AssetEvents>() {
        return Err(resources_missing());
    }
    Ok((
        world
            .get_resource::<AssetServer>()
            .ok_or_else(resources_missing)?,
        world
            .get_resource::<Assets<Material2dAsset>>()
            .ok_or_else(resources_missing)?,
        world
            .get_resource::<AssetStates>()
            .ok_or_else(resources_missing)?,
    ))
}

fn planned_image(
    images: &ImagePublications,
    server: &AssetServer,
    product: AssetProductRef,
) -> Result<AssetProductRef, HostFault> {
    let handle = images
        .handle_for_product(product)
        .ok_or_else(dependency_missing)?;
    if server.get_product_handle::<ImageAsset>(product) == Some(handle) {
        Ok(product)
    } else {
        Err(dependency_missing())
    }
}

fn metadata_bytes(
    content: ResidencyInput<'_>,
    incoming: usize,
    current: usize,
) -> Result<usize, HostFault> {
    let mut bytes = PreparedMaterial2dPublications::checked_metadata_bytes(incoming, current)
        .ok_or_else(metadata_overflow)?;
    add_bytes(
        &mut bytes,
        size_of::<MaterialMetadataLease>() + ARC_CONTROL_BLOCK_BYTES,
    )?;
    match content {
        #[cfg(feature = "project-source")]
        ResidencyInput::Source(snapshot) => {
            if let Some(collection) = snapshot.domain_assets().collection::<Material2dDomain>() {
                let resolver = snapshot.asset_resolution();
                for value in collection.iter() {
                    add_material_bytes(&mut bytes, value, size_of::<SourceMaterialOwner>())?;
                    for reference in value.textures().values() {
                        add_bytes(
                            &mut bytes,
                            resolver
                                .resolution_work_bytes(reference)
                                .ok_or_else(metadata_overflow)?,
                        )?;
                    }
                }
                for record in collection.source_records() {
                    add_bytes(
                        &mut bytes,
                        asset_record_retained_bytes(record)
                            .map_err(|error| HostFault::new(error.diagnostics().clone()))?,
                    )?;
                }
            }
        }
        ResidencyInput::Processed(content) => {
            for acquired in &content.materials {
                add_material_bytes(
                    &mut bytes,
                    acquired.product(),
                    size_of::<RuntimeMaterialOwner>(),
                )?;
            }
        }
    }
    Ok(bytes)
}

fn add_material_bytes(
    bytes: &mut usize,
    value: &Material2dAsset,
    owner: usize,
) -> Result<(), HostFault> {
    add_bytes(bytes, owner + ARC_CONTROL_BLOCK_BYTES)?;
    add_bytes(
        bytes,
        value
            .canonicalization_retained_bytes()
            .ok_or_else(metadata_overflow)?,
    )?;
    add_bytes(
        bytes,
        value
            .generation()
            .lifetime_owner_retained_bytes()
            .ok_or_else(metadata_overflow)?,
    )
}

fn add_bytes(bytes: &mut usize, extra: usize) -> Result<(), HostFault> {
    *bytes = bytes.checked_add(extra).ok_or_else(metadata_overflow)?;
    Ok(())
}

#[cfg(any(feature = "project-source", test))]
fn material_kind() -> AssetSourceKind {
    AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned())
}

impl From<Material2dPublicationError> for HostFault {
    fn from(error: Material2dPublicationError) -> Self {
        match error {
            Material2dPublicationError::IdentityMismatch => binding_changed(),
            Material2dPublicationError::Superseded => failure(
                "project.scene.material-ownership-changed",
                "Scene Material publication is no longer owned by the current level",
            ),
            Material2dPublicationError::VersionExhausted => failure(
                "project.scene.material-version-exhausted",
                "Scene Material publication versions are exhausted",
            ),
        }
    }
}

fn dependency_missing() -> HostFault {
    failure(
        "project.scene.material-texture-unavailable",
        "Prepared Scene Material texture has no matching Image publication",
    )
}

fn metadata_overflow() -> HostFault {
    failure(
        "project.scene.residency-metadata-overflow",
        "Scene residency metadata size overflowed",
    )
}
