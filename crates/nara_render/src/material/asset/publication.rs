use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
    sync::Arc,
};

use nara_asset::{
    AssetEvents, AssetId, AssetMutationBaseline, AssetProductRef, AssetPublicationRevision,
    AssetRef, AssetServer, AssetSlotRevision, AssetSourcePath, AssetStateRevision, AssetStates,
    Assets, Handle, ImportedProductId, LoadState,
};

use super::Material2dAsset;
use crate::material::MaterialParameterValue;

#[cfg(test)]
mod tests;

struct MaterialIdentity {
    source_path: Option<AssetSourcePath>,
    original: Material2dAsset,
    _owner: Arc<dyn Send + Sync>,
}

impl MaterialIdentity {
    fn binding_matches(&self, handle: Handle<Material2dAsset>, server: &AssetServer) -> bool {
        let product = self.original.product();
        server.get_product_handle::<Material2dAsset>(product) == Some(handle)
            && self.source_path.as_ref().is_none_or(|path| {
                product.product() == ImportedProductId::PRIMARY
                    && server.path(handle.id()) == Some(path)
            })
    }
}

/// One admitted Material and its destination in the staged asset catalog.
pub struct Material2dPublicationInput {
    handle: Handle<Material2dAsset>,
    identity: Arc<MaterialIdentity>,
    value: Material2dAsset,
}

impl Material2dPublicationInput {
    /// Resolves the original references through the caller's admitted Image products.
    ///
    /// Source inputs supply their canonical catalog path. Cooked inputs use their complete
    /// Product identity. The receipt keeps the comparison data and its metadata owner. The
    /// published generation and its escaped bindings keep the supplied generation owner.
    pub fn resolve<E: From<Material2dPublicationError>>(
        handle: Handle<Material2dAsset>,
        source_path: Option<AssetSourcePath>,
        original: &Material2dAsset,
        staged_server: &AssetServer,
        receipt_owner: Arc<impl Send + Sync + 'static>,
        generation_owner: Arc<impl Send + Sync + 'static>,
        resolve: impl FnMut(&AssetRef) -> Result<AssetProductRef, E>,
    ) -> Result<Self, E> {
        let value = original.resolve_texture_references(resolve)?;
        let identity = MaterialIdentity {
            source_path,
            original: original.clone(),
            _owner: receipt_owner,
        };
        if !identity.binding_matches(handle, staged_server) {
            return Err(Material2dPublicationError::IdentityMismatch.into());
        }
        Ok(Self {
            handle,
            identity: Arc::new(identity),
            value: value.with_lifetime_owner(generation_owner),
        })
    }
}

/// Evidence of the exact successful Material publication and its input custody.
#[derive(Clone)]
pub struct Material2dContentReceipt {
    handle: Handle<Material2dAsset>,
    identity: Arc<MaterialIdentity>,
    slot: AssetSlotRevision,
    state: AssetStateRevision,
    publication: AssetPublicationRevision,
}

impl Material2dContentReceipt {
    #[must_use]
    pub const fn handle(&self) -> Handle<Material2dAsset> {
        self.handle
    }

    #[must_use]
    pub fn product(&self) -> AssetProductRef {
        self.identity.original.product()
    }

    fn matches_slot(
        &self,
        server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> bool {
        self.identity.binding_matches(self.handle, server)
            && values
                .get(self.handle)
                .is_some_and(|value| value.product() == self.product())
            && values.slot_revision(self.handle) == self.slot
            && states.published_slot_revision(self.handle.id()) == Some(&self.slot)
            && states
                .state(self.handle.id())
                .is_some_and(|state| state.usable_version().is_some())
    }

    fn matches(
        &self,
        server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> bool {
        states.state_revision(self.handle.id()) == self.state
            && self.matches_slot(server, values, states)
    }

    /// Checks ownership of the live publication, allowing a failed reload to keep its last good
    /// publication eligible for retirement or replacement.
    fn validate_retirement(
        &self,
        server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> Result<(), Material2dPublicationError> {
        let id = self.handle.id();
        if self.matches_slot(server, values, states)
            && states.publication_revision(id) == self.publication
            && (states.state_revision(id) == self.state
                || states
                    .state(id)
                    .is_some_and(|state| matches!(state.load_state(), LoadState::Failed { .. })))
        {
            Ok(())
        } else {
            Err(Material2dPublicationError::Superseded)
        }
    }
}

/// How a Material transition treats the supplied receipts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material2dPublicationMode {
    /// Publish the incoming destinations and retire the other supplied receipts.
    Replace,
    /// Share identical publications and leave all other existing publications in place.
    Add,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Material2dPublicationError {
    #[error("material publication has a conflicting destination or product identity")]
    IdentityMismatch,
    #[error("material publication no longer owns its asset binding or value/state association")]
    Superseded,
    #[error("material publication version is exhausted")]
    VersionExhausted,
}

struct Publication {
    input: Material2dPublicationInput,
    baseline: AssetMutationBaseline<Material2dAsset>,
    predecessor: Option<Arc<MaterialIdentity>>,
}

/// A finite batch of Material publications, shared uses and retirements prepared without writes.
#[derive(Default)]
#[must_use]
pub struct PreparedMaterial2dPublications {
    publish: BTreeMap<AssetProductRef, Publication>,
    shared: Vec<Material2dContentReceipt>,
    retire: Vec<Material2dContentReceipt>,
}

impl PreparedMaterial2dPublications {
    /// Models batch rows, indexes and overlapping receipt storage during preparation and commit.
    /// Callers also budget cloned source paths, resolved bindings and their retained input owners.
    #[must_use]
    pub fn checked_metadata_bytes(inputs: usize, receipts: usize) -> Option<usize> {
        const ENTRY_OVERHEAD: usize = 6 * size_of::<usize>();
        const RECEIPT: usize = size_of::<Material2dContentReceipt>();
        let input = size_of::<Publication>()
            + size_of::<MaterialIdentity>()
            + 4 * size_of::<usize>()
            + 3 * RECEIPT
            + size_of::<AssetId>()
            + size_of::<AssetProductRef>()
            + 3 * ENTRY_OVERHEAD;
        let current =
            2 * RECEIPT + size_of::<(AssetId, &Material2dContentReceipt)>() + ENTRY_OVERHEAD;
        inputs
            .checked_mul(input)?
            .checked_add(receipts.checked_mul(current)?)
    }

    /// Checks each input against the live store before requesting the next input.
    ///
    /// An input can reserve its binding in the caller's private catalog as it is pulled. Call
    /// [`Self::validate_bindings`] on the resulting catalog before publishing another resource.
    pub fn prepare<'receipt, E: From<Material2dPublicationError>>(
        inputs: impl IntoIterator<Item = Result<Material2dPublicationInput, E>>,
        owned: impl Iterator<Item = &'receipt Material2dContentReceipt> + Clone,
        mode: Material2dPublicationMode,
        live_server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> Result<Self, E> {
        let mut batch = Self::default();
        let mut owned_by_id = BTreeMap::new();
        for receipt in owned.clone() {
            if owned_by_id.insert(receipt.handle.id(), receipt).is_some() {
                return Err(Material2dPublicationError::IdentityMismatch.into());
            }
            if mode == Material2dPublicationMode::Replace {
                receipt.validate_retirement(live_server, values, states)?;
            }
        }
        let mut next_ids = BTreeSet::new();
        for input in inputs {
            let input = input?;
            let id = input.handle.id();
            let product = input.value.product();
            if !next_ids.insert(id) || batch.publish.contains_key(&product) {
                return Err(Material2dPublicationError::IdentityMismatch.into());
            }
            let incumbent = owned_by_id
                .get(&id)
                .copied()
                .filter(|receipt| receipt.product() == product);
            if mode == Material2dPublicationMode::Add
                && let Some(receipt) = incumbent
            {
                if receipt.identity.source_path != input.identity.source_path
                    || !receipt.matches(live_server, values, states)
                    || !same_material(&receipt.identity.original, &input.identity.original)
                    || values
                        .get(input.handle)
                        .is_none_or(|resident| resident.textures() != input.value.textures())
                {
                    return Err(Material2dPublicationError::Superseded.into());
                }
                batch.shared.push(receipt.clone());
                continue;
            }
            if incumbent.is_none()
                && (values.get(input.handle).is_some()
                    || states
                        .state(id)
                        .is_some_and(|state| !matches!(state.load_state(), LoadState::Removed)))
            {
                return Err(Material2dPublicationError::Superseded.into());
            }
            validate_next_version(id, states)?;
            let baseline = values
                .prepare_mutation(input.handle, states)
                .map_err(|_| Material2dPublicationError::Superseded)?;
            batch.publish.insert(
                product,
                Publication {
                    input,
                    baseline,
                    predecessor: incumbent.map(|receipt| Arc::clone(&receipt.identity)),
                },
            );
        }
        if mode == Material2dPublicationMode::Replace {
            for receipt in owned.filter(|receipt| !next_ids.contains(&receipt.handle.id())) {
                validate_next_version(receipt.handle.id(), states)?;
                batch.retire.push(receipt.clone());
            }
        }
        Ok(batch)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.publish.is_empty() && self.shared.is_empty() && self.retire.is_empty()
    }

    /// Returns the Products used by this batch, including shared publications.
    #[must_use]
    pub fn additive_products(&self) -> Vec<AssetProductRef> {
        self.publish
            .keys()
            .copied()
            .chain(self.shared.iter().map(Material2dContentReceipt::product))
            .collect()
    }

    pub fn retiring_products(&self) -> impl Iterator<Item = AssetProductRef> + '_ {
        self.retire.iter().map(Material2dContentReceipt::product)
    }

    /// Checks the bindings in the catalog that will be installed for publication.
    pub fn validate_bindings(
        &self,
        server: &AssetServer,
    ) -> Result<(), Material2dPublicationError> {
        let changed = self.publish.values().any(|item| {
            !item
                .input
                .identity
                .binding_matches(item.input.handle, server)
        }) || self
            .shared
            .iter()
            .chain(&self.retire)
            .any(|receipt| !receipt.identity.binding_matches(receipt.handle, server));
        if changed {
            Err(Material2dPublicationError::Superseded)
        } else {
            Ok(())
        }
    }

    /// Rechecks the touched store slots and owned publications before the catalog is installed.
    pub fn validate(
        &self,
        live_server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> Result<(), Material2dPublicationError> {
        self.validate_slots(live_server, values, states)?;
        if self.publish.values().any(|item| {
            item.predecessor
                .as_ref()
                .is_some_and(|identity| !identity.binding_matches(item.input.handle, live_server))
        }) {
            return Err(Material2dPublicationError::Superseded);
        }
        Ok(())
    }

    fn validate_slots(
        &self,
        server: &AssetServer,
        values: &Assets<Material2dAsset>,
        states: &AssetStates,
    ) -> Result<(), Material2dPublicationError> {
        for item in self.publish.values() {
            item.baseline
                .validate(values, states)
                .map_err(|_| Material2dPublicationError::Superseded)?;
        }
        for receipt in &self.retire {
            receipt.validate_retirement(server, values, states)?;
            validate_next_version(receipt.handle.id(), states)?;
        }
        if self
            .shared
            .iter()
            .any(|receipt| !receipt.matches(server, values, states))
        {
            return Err(Material2dPublicationError::Superseded);
        }
        Ok(())
    }

    /// Checks the entire batch against the installed catalog before the first mutation.
    /// Returns only new publications. Shared Materials keep their original receipts.
    pub fn commit(
        self,
        server: &AssetServer,
        values: &mut Assets<Material2dAsset>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<Vec<Material2dContentReceipt>, Material2dPublicationError> {
        self.validate_slots(server, values, states)?;
        self.validate_bindings(server)?;
        let mut receipts = Vec::with_capacity(self.publish.len());
        for item in self.publish.into_values() {
            let Material2dPublicationInput {
                handle,
                identity,
                value,
            } = item.input;
            item.baseline
                .commit_loaded(value, values, states, events, None, None)
                .expect("preflighted Material publication remains valid in this disjoint batch");
            receipts.push(Material2dContentReceipt {
                handle,
                identity,
                slot: values.slot_revision(handle),
                state: states.state_revision(handle.id()),
                publication: states.publication_revision(handle.id()),
            });
        }
        for receipt in self.retire {
            values
                .remove_with_state(receipt.handle, states, events)
                .expect("preflighted Material retirement remains valid in this disjoint batch");
        }
        Ok(receipts)
    }
}

fn validate_next_version(
    id: AssetId,
    states: &AssetStates,
) -> Result<(), Material2dPublicationError> {
    states
        .next_version(id)
        .map(|_| ())
        .map_err(|_| Material2dPublicationError::VersionExhausted)
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
    use MaterialParameterValue::{F32, I32, U32, Vec2, Vec3, Vec4};

    match (left, right) {
        (F32(left), F32(right)) => left.to_bits() == right.to_bits(),
        (I32(left), I32(right)) => left == right,
        (U32(left), U32(right)) => left == right,
        (Vec2(left), Vec2(right)) => left.map(f32::to_bits) == right.map(f32::to_bits),
        (Vec3(left), Vec3(right)) => left.map(f32::to_bits) == right.map(f32::to_bits),
        (Vec4(left), Vec4(right)) => left.map(f32::to_bits) == right.map(f32::to_bits),
        _ => false,
    }
}
