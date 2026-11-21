use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
    sync::Arc,
};

use nara_asset::{
    AssetEvents, AssetId, AssetMutationBaseline, AssetProductRef, AssetPublicationRevision,
    AssetRecord, AssetRef, AssetServer, AssetSlotRevision, AssetSourceKind, AssetStateRevision,
    AssetStates, Assets, Handle, ImportedProductId, LoadState,
};
use nara_ecs::World;
use nara_image::ImageAsset;
use nara_material::{Material2dAsset, Material2dDomain, MaterialParameterValue};
use nara_scene::advanced::SceneAssetReservations;

use super::{
    HostFault, ProjectSceneResidency, ResidencyInput, failure, images::ImagePublications,
    resources_missing,
};
use crate::{
    project_assets::ProjectDomainAssets,
    project_content::{ProjectContentLease, asset_record_retained_bytes},
    runtime_content::{RuntimeContentAsset, RuntimeContentLease},
    scene_retention::{ARC_CONTROL_BLOCK_BYTES, BTREE_ENTRY_OVERHEAD},
};

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct OwnedMaterial {
    pub(super) handle: Handle<Material2dAsset>,
    pub(super) product: AssetProductRef,
    identity: Arc<MaterialIdentity>,
    slot: AssetSlotRevision,
    state: AssetStateRevision,
    publication: AssetPublicationRevision,
}

struct MaterialIdentity {
    record: Option<AssetRecord>,
    original: Material2dAsset,
    _metadata: Arc<MaterialMetadataLease>,
}

impl OwnedMaterial {
    fn matches(
        &self,
        server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> bool {
        states.state_revision(self.handle.id()) == self.state
            && self.matches_slot(server, values, states)
    }

    fn matches_slot(
        &self,
        server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> bool {
        values
            .get(self.handle)
            .is_some_and(|value| value.product() == self.product)
            && binding_matches(
                self.handle,
                self.product,
                self.identity.record.as_ref(),
                server,
            )
            && values.slot_revision(self.handle) == self.slot
            && states.published_slot_revision(self.handle.id()) == Some(&self.slot)
            && states
                .state(self.handle.id())
                .is_some_and(|state| state.usable_version().is_some())
            && values.prepare_mutation(self.handle, states).is_ok()
    }

    fn matches_retirement(
        &self,
        server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> bool {
        self.matches_slot(server, values, states)
            && states.publication_revision(self.handle.id()) == self.publication
            && (states.state_revision(self.handle.id()) == self.state
                || states
                    .state(self.handle.id())
                    .is_some_and(|state| matches!(state.load_state(), LoadState::Failed { .. })))
    }
}

#[derive(Default)]
pub(super) struct MaterialResidency {
    owned: Vec<OwnedMaterial>,
}

impl MaterialResidency {
    pub(super) fn is_empty(&self) -> bool {
        self.owned.is_empty()
    }

    pub(super) fn append(&mut self, mut other: Self) {
        self.owned.append(&mut other.owned);
    }

    pub(super) fn retain_products(&mut self, mut keep: impl FnMut(AssetProductRef) -> bool) {
        self.owned.retain(|material| keep(material.product));
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &OwnedMaterial> + Clone {
        self.owned.iter()
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
    identity: Arc<MaterialIdentity>,
    value: Material2dAsset,
    baseline: AssetMutationBaseline<Material2dAsset>,
}

pub(super) struct MaterialPublications {
    publish: BTreeMap<AssetProductRef, MaterialPublication>,
    retire: Vec<OwnedMaterial>,
    shared: Vec<OwnedMaterial>,
    custody: Option<MaterialCustody>,
}

#[derive(Clone, Copy)]
pub(super) enum MaterialChange<'a> {
    Replace(Option<&'a ProjectSceneResidency>),
    Add(&'a ProjectSceneResidency),
}

impl<'a> MaterialChange<'a> {
    fn owned(self) -> impl Iterator<Item = &'a OwnedMaterial> + Clone {
        let current = match self {
            Self::Replace(current) => current,
            Self::Add(current) => Some(current),
        };
        current
            .into_iter()
            .flat_map(ProjectSceneResidency::owned_materials)
    }
}

impl MaterialPublications {
    pub(super) fn reserve(
        content: ResidencyInput<'_>,
        change: MaterialChange<'_>,
    ) -> Result<Option<MaterialCustody>, HostFault> {
        if content.material_count() == 0
            && (matches!(change, MaterialChange::Add(_)) || change.owned().next().is_none())
        {
            return Ok(None);
        }
        let bytes = metadata_bytes(content, change)?;
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
        self.publish.is_empty() && self.retire.is_empty() && self.shared.is_empty()
    }

    pub(super) fn products(&self) -> Vec<AssetProductRef> {
        self.publish
            .keys()
            .copied()
            .chain(self.shared.iter().map(|material| material.product))
            .collect()
    }

    pub(super) fn retiring_products(&self) -> impl Iterator<Item = AssetProductRef> + '_ {
        self.retire.iter().map(|material| material.product)
    }

    pub(super) fn prepare(
        world: &World,
        content: ResidencyInput<'_>,
        change: MaterialChange<'_>,
        bindings: &mut SceneAssetReservations,
        images: &ImagePublications,
        custody: Option<MaterialCustody>,
    ) -> Result<Self, HostFault> {
        let owned = change.owned();
        let additive = matches!(change, MaterialChange::Add(_));
        let mut prepared = Self {
            publish: BTreeMap::new(),
            retire: Vec::new(),
            shared: Vec::new(),
            custody,
        };
        if content.material_count() == 0 && (additive || owned.clone().next().is_none()) {
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
        if !additive
            && owned
                .clone()
                .any(|material| !material.matches_retirement(server, values, states))
        {
            return Err(superseded());
        }
        let old = owned
            .clone()
            .map(|material| (material.handle.id(), material))
            .collect::<BTreeMap<_, _>>();
        let mut next_ids = BTreeSet::new();
        let mut insert = |handle: Handle<Material2dAsset>,
                          record: Option<AssetRecord>,
                          original: &Material2dAsset,
                          value: Material2dAsset| {
            let product = value.product();
            if prepared.publish.contains_key(&product) || !next_ids.insert(handle.id()) {
                return Err(identity_conflict());
            }
            let ours = old.get(&handle.id()).filter(|material| {
                material.product == product && (!additive || material.identity.record == record)
            });
            if additive && let Some(material) = ours {
                if !material.matches(server, values, states)
                    || !same_material(&material.identity.original, original)
                    || values
                        .get(handle)
                        .is_none_or(|resident| !same_material(resident, &value))
                {
                    return Err(superseded());
                }
                prepared.shared.push((*material).clone());
                return Ok(());
            }
            if ours.is_none()
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
                    identity: Arc::new(MaterialIdentity {
                        record,
                        original: original.clone(),
                        _metadata: Arc::clone(&metadata),
                    }),
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
                    for (record, original) in collection.source_records().zip(collection.iter()) {
                        let product = original.product();
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
                        let value = original.resolve_texture_references(|reference| {
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
                        insert(handle, Some(record.clone()), original, value)?;
                    }
                }
                ResidencyInput::Processed(content) => {
                    for acquired in &content.materials {
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
                        insert(handle, None, acquired.product(), value)?;
                    }
                }
            }
        }
        if !additive {
            prepared.retire = Self::retire(
                owned.filter(|material| !next_ids.contains(&material.handle.id())),
                world,
            )?
            .retire;
        }
        Ok(prepared)
    }

    pub(super) fn retire<'a>(
        owned: impl IntoIterator<Item = &'a OwnedMaterial>,
        world: &World,
    ) -> Result<Self, HostFault> {
        let mut retire = Vec::new();
        for material in owned {
            if !world.contains_resource::<AssetEvents>() {
                return Err(resources_missing());
            }
            let server = world
                .get_resource::<AssetServer>()
                .ok_or_else(resources_missing)?;
            let values = world
                .get_resource::<Assets<Material2dAsset>>()
                .ok_or_else(resources_missing)?;
            let states = world
                .get_resource::<AssetStates>()
                .ok_or_else(resources_missing)?;
            if !material.matches_retirement(server, values, states) {
                return Err(superseded());
            }
            states
                .next_version(material.handle.id())
                .map_err(|_| version_exhausted())?;
            retire.push(material.clone());
        }
        Ok(Self {
            publish: BTreeMap::new(),
            retire,
            shared: Vec::new(),
            custody: None,
        })
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
            .any(|material| !material.matches_retirement(server, values, states))
            || self
                .shared
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
                item.identity.record.as_ref(),
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
                identity: item.identity,
                slot: values.slot_revision(item.handle),
                state: states.state_revision(item.handle.id()),
                publication: states.publication_revision(item.handle.id()),
            });
        }
        for material in self.retire {
            assert!(
                values
                    .remove_with_state(material.handle, states, events)
                    .expect("Scene Material retirement version was preflighted")
                    .is_some()
            );
        }
        MaterialResidency { owned }
    }
}

fn validate_image(
    images: &ImagePublications,
    server: &AssetServer,
    handle: Handle<ImageAsset>,
    product: AssetProductRef,
) -> Result<(), HostFault> {
    let expected = images
        .handle_for_product(product)
        .ok_or_else(dependency_missing)?;
    if handle != expected
        || server.get_handle::<ImageAsset>(handle.id()) != Some(handle)
        || server.product(handle.id()) != Some(product)
    {
        return Err(dependency_missing());
    }
    Ok(())
}

fn same_material(original: &Material2dAsset, incoming: &Material2dAsset) -> bool {
    original.product() == incoming.product()
        && original.generation().declaration() == incoming.generation().declaration()
        && original.generation().shader() == incoming.generation().shader()
        && original.generation().inputs() == incoming.generation().inputs()
        && original.default_parameters().len() == incoming.default_parameters().len()
        && original
            .default_parameters()
            .iter()
            .zip(incoming.default_parameters())
            .all(|((left_id, left), (right_id, right))| {
                left_id == right_id && same_parameter(*left, *right)
            })
        && original.textures() == incoming.textures()
}

fn same_parameter(left: MaterialParameterValue, right: MaterialParameterValue) -> bool {
    match (left, right) {
        (MaterialParameterValue::F32(left), MaterialParameterValue::F32(right)) => {
            left.to_bits() == right.to_bits()
        }
        (MaterialParameterValue::I32(left), MaterialParameterValue::I32(right)) => left == right,
        (MaterialParameterValue::U32(left), MaterialParameterValue::U32(right)) => left == right,
        (MaterialParameterValue::Vec2(left), MaterialParameterValue::Vec2(right)) => {
            left.map(f32::to_bits) == right.map(f32::to_bits)
        }
        (MaterialParameterValue::Vec3(left), MaterialParameterValue::Vec3(right)) => {
            left.map(f32::to_bits) == right.map(f32::to_bits)
        }
        (MaterialParameterValue::Vec4(left), MaterialParameterValue::Vec4(right)) => {
            left.map(f32::to_bits) == right.map(f32::to_bits)
        }
        _ => false,
    }
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
    change: MaterialChange<'_>,
) -> Result<usize, HostFault> {
    let mut bytes =
        size_of::<MaterialCustody>() + size_of::<MaterialMetadataLease>() + ARC_CONTROL_BLOCK_BYTES;
    let (source, processed) = match content {
        ResidencyInput::Source(snapshot) => (
            snapshot.domain_assets().collection::<Material2dDomain>(),
            &[][..],
        ),
        ResidencyInput::Processed(content) => (None, content.materials.as_slice()),
    };
    for value in source
        .into_iter()
        .flat_map(|collection| collection.iter())
        .chain(processed.iter().map(RuntimeContentAsset::product))
    {
        let canonical_bytes = value
            .canonicalization_retained_bytes()
            .ok_or_else(metadata_overflow)?;
        add_bytes(&mut bytes, canonical_bytes)?;
        add_bytes(
            &mut bytes,
            size_of::<MaterialPublication>()
                + size_of::<MaterialIdentity>()
                + ARC_CONTROL_BLOCK_BYTES
                + 3 * size_of::<OwnedMaterial>()
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
        match content {
            ResidencyInput::Source(snapshot) => {
                for reference in value.textures().values() {
                    let work = snapshot
                        .asset_resolution()
                        .resolution_work_bytes(reference)
                        .ok_or_else(metadata_overflow)?;
                    add_bytes(&mut bytes, work)?;
                    add_bytes(&mut bytes, work)?;
                }
            }
            ResidencyInput::Processed(_) => {
                let growth = AssetServer::checked_product_binding_retained_bytes(None)
                    .ok_or_else(metadata_overflow)?;
                add_bytes(&mut bytes, growth)?;
                add_bytes(&mut bytes, growth)?;
            }
        }
    }
    for record in source
        .into_iter()
        .flat_map(|collection| collection.source_records())
    {
        add_bytes(
            &mut bytes,
            asset_record_retained_bytes(record)
                .map_err(|error| HostFault::new(error.diagnostics().clone()))?,
        )?;
    }
    for _material in change.owned() {
        add_bytes(
            &mut bytes,
            2 * size_of::<OwnedMaterial>()
                + size_of::<(AssetId, &OwnedMaterial)>()
                + BTREE_ENTRY_OVERHEAD,
        )?;
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
