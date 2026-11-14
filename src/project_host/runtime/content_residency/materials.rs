use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
    sync::Arc,
};

use nara_asset::{
    AssetEvents, AssetId, AssetMutationBaseline, AssetProductRef, AssetRecord, AssetRef,
    AssetServer, AssetSourceKind, AssetStates, Assets, Handle, ImportedProductId, LoadState,
    RuntimeAssetCodec,
};
use nara_ecs::World;
use nara_image::ImageAsset;
use nara_material::{Material2dAsset, Material2dDomain, Material2dRuntimeCodec};
use nara_scene::advanced::SceneAssetReservations;

use super::{HostFault, ResidencyInput, failure, images::ImagePublications, resources_missing};
use crate::{
    project_content::{ProjectContentLease, ProjectDomainAssets, asset_record_retained_bytes},
    runtime_content::{RuntimeContentAsset, RuntimeContentLease},
    scene_retention::{ARC_CONTROL_BLOCK_BYTES, BTREE_ENTRY_OVERHEAD},
};

#[cfg(test)]
mod tests;

#[derive(Clone)]
struct OwnedMaterial {
    handle: Handle<Material2dAsset>,
    product: AssetProductRef,
    record: Option<AssetRecord>,
    baseline: AssetMutationBaseline<Material2dAsset>,
}

impl OwnedMaterial {
    fn matches(
        &self,
        server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> bool {
        self.baseline.validate(values, states).is_ok()
            && values
                .get(self.handle)
                .is_some_and(|value| value.product() == self.product)
            && binding_matches(self.handle, self.product, self.record.as_ref(), server)
            && states.published_slot_revision(self.handle.id())
                == Some(&values.slot_revision(self.handle))
            && states
                .state(self.handle.id())
                .is_some_and(|state| state.usable_version().is_some())
    }
}

#[derive(Default)]
pub(super) struct MaterialResidency {
    owned: Vec<OwnedMaterial>,
    _custody: Option<MaterialCustody>,
}

impl MaterialResidency {
    pub(super) fn is_empty(&self) -> bool {
        self.owned.is_empty()
    }
}

enum MaterialMetadataLease {
    Source { _lease: ProjectContentLease },
    Processed { _lease: RuntimeContentLease },
}

struct SourceMaterialOwner {
    _domains: ProjectDomainAssets,
    _metadata: Arc<MaterialMetadataLease>,
}

struct RuntimeMaterialOwner {
    _acquired: RuntimeContentAsset<Material2dAsset>,
    _metadata: Arc<MaterialMetadataLease>,
}

pub(super) struct MaterialCustody {
    _domains: ProjectDomainAssets,
    lease: Arc<MaterialMetadataLease>,
}

struct MaterialPublication {
    handle: Handle<Material2dAsset>,
    product: AssetProductRef,
    record: Option<AssetRecord>,
    value: Material2dAsset,
    baseline: AssetMutationBaseline<Material2dAsset>,
}

pub(super) struct MaterialPublications {
    publish: BTreeMap<AssetProductRef, MaterialPublication>,
    retire: Vec<OwnedMaterial>,
    custody: Option<MaterialCustody>,
}

impl MaterialPublications {
    pub(super) fn reserve(
        content: ResidencyInput<'_>,
        current: Option<&MaterialResidency>,
    ) -> Result<Option<MaterialCustody>, HostFault> {
        if content.material_count() == 0 && current.is_none_or(MaterialResidency::is_empty) {
            return Ok(None);
        }
        let bytes = metadata_bytes(content, current)?;
        let lease = match content {
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
        Ok(Some(MaterialCustody {
            _domains: content.domain_assets().clone(),
            lease: Arc::new(lease),
        }))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.publish.is_empty() && self.retire.is_empty()
    }

    pub(super) fn prepare(
        world: &World,
        content: ResidencyInput<'_>,
        current: Option<&MaterialResidency>,
        bindings: &mut SceneAssetReservations,
        images: &ImagePublications,
        custody: Option<MaterialCustody>,
    ) -> Result<Self, HostFault> {
        let owned = current.map_or(&[][..], |current| current.owned.as_slice());
        let mut prepared = Self {
            publish: BTreeMap::new(),
            retire: Vec::with_capacity(owned.len()),
            custody,
        };
        if content.material_count() == 0 && owned.is_empty() {
            return Ok(prepared);
        }
        let metadata = Arc::clone(
            &prepared
                .custody
                .as_ref()
                .ok_or_else(resources_missing)?
                .lease,
        );
        let server = world
            .get_resource::<AssetServer>()
            .ok_or_else(resources_missing)?;
        let values = world
            .get_resource::<Assets<Material2dAsset>>()
            .ok_or_else(resources_missing)?;
        let states = world
            .get_resource::<AssetStates>()
            .ok_or_else(resources_missing)?;
        if owned
            .iter()
            .any(|material| !material.matches(server, values, states))
        {
            return Err(superseded());
        }
        let old = owned
            .iter()
            .map(|material| (material.handle.id(), material))
            .collect::<BTreeMap<_, _>>();
        let mut next_ids = BTreeSet::new();
        let mut insert = |handle: Handle<Material2dAsset>,
                          record: Option<AssetRecord>,
                          value: Material2dAsset| {
            let product = value.product();
            if prepared.publish.contains_key(&product) || !next_ids.insert(handle.id()) {
                return Err(identity_conflict());
            }
            let ours = old
                .get(&handle.id())
                .is_some_and(|material| material.product == product && material.record == record);
            if !ours
                && (values.get(handle).is_some()
                    || states
                        .state(handle.id())
                        .is_some_and(|state| !matches!(state.load_state(), LoadState::Removed)))
            {
                return Err(superseded());
            }
            states
                .next_version(handle.id())
                .map_err(|_| version_exhausted())?;
            let baseline = values
                .prepare_mutation(handle, states)
                .map_err(|_| superseded())?;
            prepared.publish.insert(
                product,
                MaterialPublication {
                    handle,
                    product,
                    record,
                    value,
                    baseline,
                },
            );
            Ok(())
        };
        if let Some(collection) = content.domain_assets().collection::<Material2dDomain>() {
            match content {
                ResidencyInput::Source(snapshot) => {
                    if collection.source_records().len() != collection.iter().len() {
                        return Err(identity_conflict());
                    }
                    let resolver = snapshot.asset_resolution();
                    for (record, value) in collection.source_records().zip(collection.iter()) {
                        let product = value.product();
                        if record.source_kind() != &material_kind()
                            || product.source() != record.stable_id()
                            || product.product() != ImportedProductId::PRIMARY
                            || resolver.resolve_ref(&AssetRef::Product(product)).ok()
                                != Some(record)
                        {
                            return Err(identity_conflict());
                        }
                        let handle = bindings
                            .reserve::<Material2dAsset>(
                                &AssetRef::Product(product),
                                resolver,
                                &material_kind(),
                            )
                            .map_err(|_| identity_conflict())?;
                        let value = value.resolve_texture_references(|reference| {
                            let record = resolver
                                .resolve_ref(reference)
                                .map_err(|_| dependency_missing())?;
                            if record.source_kind() != &AssetSourceKind::Image {
                                return Err(dependency_missing());
                            }
                            let product = match reference {
                                AssetRef::Product(product) => *product,
                                AssetRef::Path(_) | AssetRef::StableId(_) => AssetProductRef::new(
                                    record.stable_id(),
                                    ImportedProductId::PRIMARY,
                                )
                                .map_err(|_| identity_conflict())?,
                            };
                            let handle = bindings
                                .reserve::<ImageAsset>(reference, resolver, &AssetSourceKind::Image)
                                .map_err(|_| dependency_missing())?;
                            validate_image(images, bindings.staged_server(), handle, product)?;
                            Ok(product)
                        })?;
                        if !binding_matches(handle, product, Some(record), bindings.staged_server())
                        {
                            return Err(identity_conflict());
                        }
                        let value = value.with_lifetime_owner(Arc::new(SourceMaterialOwner {
                            _domains: content.domain_assets().clone(),
                            _metadata: Arc::clone(&metadata),
                        }));
                        insert(handle, Some(record.clone()), value)?;
                    }
                }
                ResidencyInput::Processed(content) => {
                    if collection.source_records().len() != 0
                        || collection.runtime_acquisitions().len() != collection.iter().len()
                    {
                        return Err(identity_conflict());
                    }
                    for acquired in collection.runtime_acquisitions() {
                        let value = acquired.product();
                        let product = value.product();
                        if !content.base().same_base(acquired.base())
                            || acquired.reference() != product
                        {
                            return Err(identity_conflict());
                        }
                        let handle = bindings
                            .reserve_product::<Material2dAsset>(product)
                            .map_err(|_| identity_conflict())?;
                        let value = value.resolve_texture_references(|reference| {
                            let AssetRef::Product(product) = reference else {
                                return Err(dependency_missing());
                            };
                            let handle = bindings
                                .reserve_product::<ImageAsset>(*product)
                                .map_err(|_| dependency_missing())?;
                            validate_image(images, bindings.staged_server(), handle, *product)?;
                            Ok(*product)
                        })?;
                        if !binding_matches(handle, product, None, bindings.staged_server()) {
                            return Err(identity_conflict());
                        }
                        let value = value.with_lifetime_owner(Arc::new(RuntimeMaterialOwner {
                            _acquired: acquired.clone(),
                            _metadata: Arc::clone(&metadata),
                        }));
                        insert(handle, None, value)?;
                    }
                }
            }
        }
        for material in owned {
            if !next_ids.contains(&material.handle.id()) {
                states
                    .next_version(material.handle.id())
                    .map_err(|_| version_exhausted())?;
                prepared.retire.push(material.clone());
            }
        }
        Ok(prepared)
    }

    pub(super) fn validate(&self, world: &World) -> Result<(), HostFault> {
        if self.is_empty() {
            return Ok(());
        }
        self.validate_slots(
            world
                .get_resource::<AssetServer>()
                .ok_or_else(resources_missing)?,
            world
                .get_resource::<Assets<Material2dAsset>>()
                .ok_or_else(resources_missing)?,
            world
                .get_resource::<AssetStates>()
                .ok_or_else(resources_missing)?,
        )
    }

    fn validate_slots(
        &self,
        server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> Result<(), HostFault> {
        for item in self.publish.values() {
            item.baseline
                .validate(values, states)
                .map_err(|_| superseded())?;
        }
        if self
            .retire
            .iter()
            .any(|material| !material.matches(server, values, states))
        {
            return Err(superseded());
        }
        Ok(())
    }

    pub(super) fn commit(
        self,
        server: &AssetServer,
        values: &mut Assets<Material2dAsset>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> MaterialResidency {
        self.validate_slots(server, values, states)
            .expect("Scene Material publication retained its admitted value/state association");
        for item in self.publish.values() {
            assert!(binding_matches(
                item.handle,
                item.product,
                item.record.as_ref(),
                server
            ));
        }
        let mut owned = Vec::with_capacity(self.publish.len());
        for item in self.publish.into_values() {
            item.baseline
                .commit_loaded(item.value, values, states, events, None, None)
                .expect("Scene Material publication version was preflighted");
            owned.push(OwnedMaterial {
                handle: item.handle,
                product: item.product,
                record: item.record,
                baseline: values
                    .prepare_mutation(item.handle, states)
                    .expect("Scene Material publication retains its original stores"),
            });
        }
        for material in self.retire {
            assert!(
                material
                    .baseline
                    .remove(values, states, events)
                    .expect("Scene Material retirement version was preflighted")
                    .is_some()
            );
        }
        MaterialResidency {
            owned,
            _custody: self.custody,
        }
    }
}

fn validate_image(
    images: &ImagePublications,
    server: &AssetServer,
    handle: Handle<ImageAsset>,
    product: AssetProductRef,
) -> Result<(), HostFault> {
    let (expected, image) = images
        .image_for_product(product)
        .ok_or_else(dependency_missing)?;
    if handle != expected
        || image.product() != Some(product)
        || server.get_handle::<ImageAsset>(handle.id()) != Some(handle)
        || server.product(handle.id()) != Some(product)
    {
        return Err(dependency_missing());
    }
    Ok(())
}

fn binding_matches(
    handle: Handle<Material2dAsset>,
    product: AssetProductRef,
    record: Option<&AssetRecord>,
    server: &AssetServer,
) -> bool {
    server.get_handle::<Material2dAsset>(handle.id()) == Some(handle)
        && server.product(handle.id()) == Some(product)
        && record.is_none_or(|record| {
            record.source_kind() == &material_kind()
                && product.source() == record.stable_id()
                && product.product() == ImportedProductId::PRIMARY
                && server.path(handle.id()) == Some(record.path())
        })
}

fn metadata_bytes(
    content: ResidencyInput<'_>,
    current: Option<&MaterialResidency>,
) -> Result<usize, HostFault> {
    let mut bytes =
        size_of::<MaterialCustody>() + size_of::<MaterialMetadataLease>() + ARC_CONTROL_BLOCK_BYTES;
    if let Some(collection) = content.domain_assets().collection::<Material2dDomain>() {
        for value in collection.iter() {
            let canonical_bytes = value
                .canonicalization_retained_bytes()
                .ok_or_else(metadata_overflow)?;
            add_bytes(&mut bytes, canonical_bytes)?;
            add_bytes(
                &mut bytes,
                size_of::<MaterialPublication>()
                    + size_of::<OwnedMaterial>()
                    + size_of::<AssetId>()
                    + 2 * size_of::<AssetProductRef>()
                    + 3 * BTREE_ENTRY_OVERHEAD,
            )?;
            let owner_bytes = match content {
                ResidencyInput::Source(_) => size_of::<SourceMaterialOwner>(),
                ResidencyInput::Processed(_) => size_of::<RuntimeMaterialOwner>(),
            };
            add_bytes(&mut bytes, owner_bytes + ARC_CONTROL_BLOCK_BYTES)?;
            add_bytes(
                &mut bytes,
                value
                    .generation()
                    .lifetime_owner_retained_bytes()
                    .ok_or_else(metadata_overflow)?,
            )?;
            for reference in value.textures().values() {
                if let ResidencyInput::Source(snapshot) = content {
                    let work = snapshot
                        .asset_resolution()
                        .resolution_work_bytes(reference)
                        .ok_or_else(metadata_overflow)?;
                    add_bytes(&mut bytes, work)?;
                    add_bytes(&mut bytes, work)?;
                }
            }
            if matches!(content, ResidencyInput::Processed(_)) {
                let growth = AssetServer::checked_product_binding_retained_bytes(None)
                    .ok_or_else(metadata_overflow)?;
                add_bytes(&mut bytes, growth)?;
                add_bytes(&mut bytes, growth)?;
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
    if let Some(current) = current {
        for material in &current.owned {
            add_bytes(
                &mut bytes,
                size_of::<OwnedMaterial>()
                    + size_of::<(AssetId, &OwnedMaterial)>()
                    + BTREE_ENTRY_OVERHEAD,
            )?;
            if let Some(record) = &material.record {
                add_bytes(
                    &mut bytes,
                    asset_record_retained_bytes(record)
                        .map_err(|error| HostFault::new(error.diagnostics().clone()))?,
                )?;
            }
        }
    }
    Ok(bytes)
}

fn add_bytes(bytes: &mut usize, extra: usize) -> Result<(), HostFault> {
    *bytes = bytes.checked_add(extra).ok_or_else(metadata_overflow)?;
    Ok(())
}

fn material_kind() -> AssetSourceKind {
    AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned())
}

fn identity_conflict() -> HostFault {
    super::binding_changed()
}

fn superseded() -> HostFault {
    failure(
        "project.scene.material-ownership-changed",
        "Scene Material publication is no longer owned by the current level",
    )
}

fn dependency_missing() -> HostFault {
    failure(
        "project.scene.material-texture-unavailable",
        "Prepared Scene Material texture has no matching Image publication",
    )
}

fn version_exhausted() -> HostFault {
    failure(
        "project.scene.material-version-exhausted",
        "Scene Material publication versions are exhausted",
    )
}

fn metadata_overflow() -> HostFault {
    failure(
        "project.scene.residency-metadata-overflow",
        "Scene residency metadata size overflowed",
    )
}
