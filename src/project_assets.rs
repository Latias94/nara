//! Typed asset collections selected for a committed Scene.

use std::{
    any::{Any, TypeId},
    fmt::{self, Formatter},
    mem::size_of,
    sync::Arc,
};

#[cfg(feature = "project-source")]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "runtime-2d")]
use nara_asset::AssetRecord;
use nara_asset::{AssetDomain, AssetProductRef, AssetRef, ImportedProductId};
#[cfg(feature = "project-source")]
use nara_asset::{AssetPathScope, StableAssetId};
use nara_ecs::Resource;

#[cfg(feature = "project-source")]
use crate::project_content::ProjectContentLease;
use crate::runtime_content::{
    RuntimeContentAsset, RuntimeContentBase, RuntimeContentError, RuntimeContentLease,
};
use crate::scene_retention::ARC_CONTROL_BLOCK_BYTES;

/// Typed products and their admitted identities for one selected Scene.
pub struct ProjectAssetCollection<T> {
    pub(crate) storage: CollectionStorage<T>,
}

pub(crate) enum CollectionStorage<T> {
    #[cfg(feature = "project-source")]
    Source {
        scope: AssetPathScope,
        records: Vec<AssetRecord>,
        products: Vec<T>,
        // Bound to the selected domain. This trusted query only inspects immutable values;
        // it must not allocate, load, perform I/O, mutate products or publish state.
        contains_product: fn(&T, ImportedProductId) -> bool,
    },
    Processed {
        products: Vec<RuntimeContentAsset<T>>,
        mappings: Vec<(AssetProductRef, usize)>,
    },
}

impl<T> ProjectAssetCollection<T> {
    /// Looks up the stored value containing the exact selected product.
    ///
    /// Source values are checked through their domain's pure membership query after locating
    /// the actual enclosing record. Paths alias only real source primary products, and stable
    /// IDs select only primary products. Path aliases are relative to the collection's admitted
    /// source scope; source record paths remain canonical. Processed values require an exact
    /// admitted Product mapping; an auxiliary selection never falls back to its primary product.
    /// This query performs no loading, importing, I/O or World mutation.
    #[must_use]
    pub fn get(&self, reference: &AssetRef) -> Option<&T> {
        match &self.storage {
            #[cfg(feature = "project-source")]
            CollectionStorage::Source {
                scope,
                records,
                products,
                contains_product,
            } => {
                let index = records.iter().position(|record| match reference {
                    AssetRef::Path(path) => {
                        scope.relative_path(record.path()) == Some(path.as_str())
                    }
                    AssetRef::StableId(id) => record.stable_id() == *id,
                    AssetRef::Product(product) => record.stable_id() == product.source(),
                })?;
                let id = match reference {
                    AssetRef::Product(product) => product.product(),
                    AssetRef::Path(_) | AssetRef::StableId(_) => ImportedProductId::PRIMARY,
                };
                let product = &products[index];
                contains_product(product, id).then_some(product)
            }
            CollectionStorage::Processed { products, mappings } => {
                let reference = match reference {
                    AssetRef::Product(product) => *product,
                    AssetRef::StableId(id) => {
                        AssetProductRef::new(*id, ImportedProductId::PRIMARY).ok()?
                    }
                    AssetRef::Path(_) => return None,
                };
                let index = mappings
                    .binary_search_by_key(&reference, |(product, _)| *product)
                    .ok()?;
                Some(products[mappings[index].1].product())
            }
        }
    }

    /// Visits each stored decoded value once, independently of its aliases or Product mappings.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &T> {
        match &self.storage {
            #[cfg(feature = "project-source")]
            CollectionStorage::Source { products, .. } => CollectionIter::Source(products.iter()),
            CollectionStorage::Processed { products, .. } => {
                CollectionIter::Processed(products.iter())
            }
        }
    }

    /// Visits real source records with canonical locations, independently of local Path aliases.
    /// Processed collections have no source metadata or aliases.
    #[cfg(feature = "runtime-2d")]
    pub fn source_records(&self) -> impl ExactSizeIterator<Item = &AssetRecord> {
        self.source_records_slice().iter()
    }

    #[cfg(feature = "runtime-2d")]
    pub(crate) fn runtime_acquisitions(&self) -> &[RuntimeContentAsset<T>] {
        match &self.storage {
            CollectionStorage::Processed { products, .. } => products,
            #[cfg(feature = "project-source")]
            CollectionStorage::Source { .. } => &[],
        }
    }

    #[cfg(feature = "runtime-2d")]
    fn source_records_slice(&self) -> &[AssetRecord] {
        match &self.storage {
            #[cfg(feature = "project-source")]
            CollectionStorage::Source { records, .. } => records,
            CollectionStorage::Processed { .. } => &[],
        }
    }
}

enum CollectionIter<'a, T> {
    #[cfg(feature = "project-source")]
    Source(std::slice::Iter<'a, T>),
    Processed(std::slice::Iter<'a, RuntimeContentAsset<T>>),
}

impl<'a, T> Iterator for CollectionIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            #[cfg(feature = "project-source")]
            Self::Source(products) => products.next(),
            Self::Processed(products) => products.next().map(RuntimeContentAsset::product),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            #[cfg(feature = "project-source")]
            Self::Source(products) => products.size_hint(),
            Self::Processed(products) => products.size_hint(),
        }
    }
}

impl<T> ExactSizeIterator for CollectionIter<'_, T> {}

#[cfg(feature = "project-source")]
pub(crate) type RequiredAssetProducts = BTreeMap<StableAssetId, BTreeSet<AssetProductRef>>;

pub(crate) trait ErasedCollection: Any + Send + Sync {
    fn domain_type(&self) -> TypeId;
    fn as_any(&self) -> &dyn Any;
    #[cfg(feature = "project-source")]
    fn as_any_mut(&mut self) -> &mut dyn Any;
    #[cfg(feature = "project-source")]
    fn records(&self) -> &[AssetRecord];
    #[cfg(feature = "project-source")]
    fn validate_product_selections(
        &self,
        required: &mut RequiredAssetProducts,
    ) -> Result<(), AssetProductRef>;
}

pub(crate) struct DomainCollection<D: AssetDomain>(pub(crate) ProjectAssetCollection<D::Product>);

/// Retains pre-budgeted acquisitions and their complete verified member mappings.
///
/// The caller supplies one acquisition per decoded member from the selected original base.
/// Sorting is in place and does not allocate. Each mapping must belong to its acquisition's
/// already verified member, and all acquisitions must retain the same exact base. This
/// constructor does not execute a codec or source importer.
pub(crate) fn runtime_collection<D: AssetDomain>(
    products: Vec<RuntimeContentAsset<D::Product>>,
    mut mappings: Vec<(AssetProductRef, usize)>,
) -> Result<Box<dyn ErasedCollection>, RuntimeContentError> {
    if let Some(first) = products.first() {
        for asset in &products[1..] {
            if !first.base().same_base(asset.base()) {
                return Err(RuntimeContentError::ProductIdentityMismatch);
            }
        }
    }
    mappings.sort_unstable_by_key(|(reference, _)| *reference);
    if mappings.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(RuntimeContentError::ProductIdentityMismatch);
    }
    for (reference, index) in &mappings {
        let asset = products
            .get(*index)
            .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
        let member = asset
            .base()
            .member(asset.reference())
            .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
        if !asset
            .base()
            .member(*reference)
            .is_some_and(|mapped| std::ptr::eq(mapped, member))
        {
            return Err(RuntimeContentError::ProductIdentityMismatch);
        }
    }
    Ok(Box::new(DomainCollection::<D>(ProjectAssetCollection {
        storage: CollectionStorage::Processed { products, mappings },
    })))
}

impl<D: AssetDomain> ErasedCollection for DomainCollection<D> {
    fn domain_type(&self) -> TypeId {
        TypeId::of::<D>()
    }
    fn as_any(&self) -> &dyn Any {
        &self.0
    }
    #[cfg(feature = "project-source")]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.0
    }
    #[cfg(feature = "project-source")]
    fn records(&self) -> &[AssetRecord] {
        self.0.source_records_slice()
    }
    #[cfg(feature = "project-source")]
    fn validate_product_selections(
        &self,
        required: &mut RequiredAssetProducts,
    ) -> Result<(), AssetProductRef> {
        for (record, product) in self.0.source_records().zip(self.0.iter()) {
            if let Some(selections) = required.remove(&record.stable_id()) {
                validate_product_membership::<D>(product, selections)?;
            }
        }
        Ok(())
    }
}

/// Immutable typed resource collections retaining the committed Scene's original content owner.
///
/// File-backed Hosts install this resource before gameplay startup and replace it only after a
/// successful Scene transaction. Clones keep the exact products and their budget reservation alive.
/// Plain CPU collections retain no Scene source and do not reserve a decoded Scene generation.
/// Products containing retained image/material views preserve those views' own custody.
/// Heterogeneous storage is private; no public erased store or mutable World callback is exposed.
#[derive(Clone, Resource)]
pub struct ProjectDomainAssets {
    inner: Arc<DomainAssetsInner>,
}

struct DomainAssetsInner {
    collections: Box<[Box<dyn ErasedCollection>]>,
    // Drop values and indices before releasing their metadata reservation or content owner.
    _custody: DomainAssetsCustody,
}

enum DomainAssetsCustody {
    #[cfg(feature = "project-source")]
    Source { _lease: ProjectContentLease },
    Processed {
        _lease: RuntimeContentLease,
        _base: RuntimeContentBase,
    },
}

impl ProjectDomainAssets {
    pub(crate) fn contains_additive_unsupported_collections(&self) -> bool {
        #[cfg(feature = "runtime-2d")]
        {
            self.inner.collections.iter().any(|collection| {
                let domain = collection.domain_type();
                domain != TypeId::of::<nara_render::image::ImageDomain>()
                    && domain != TypeId::of::<nara_render::material::Material2dDomain>()
                    && domain != TypeId::of::<nara_render::tilemap::TileSetDomain>()
            })
        }
        #[cfg(not(feature = "runtime-2d"))]
        {
            !self.inner.collections.is_empty()
        }
    }

    pub(crate) fn runtime_metadata_bytes(collections: usize) -> Option<usize> {
        size_of::<DomainAssetsInner>()
            .checked_add(ARC_CONTROL_BLOCK_BYTES)?
            .checked_add(collections.checked_mul(size_of::<Box<dyn ErasedCollection>>())?)
    }

    #[cfg(feature = "project-source")]
    pub(crate) fn validate_product_selections(
        &self,
        required: &mut RequiredAssetProducts,
    ) -> Result<(), AssetProductRef> {
        for collection in &self.inner.collections {
            collection.validate_product_selections(required)?;
        }
        if let Some(reference) = required.values().find_map(|products| products.first()) {
            return Err(*reference);
        }
        Ok(())
    }

    #[cfg(feature = "project-source")]
    pub(crate) fn new(
        collections: Vec<Box<dyn ErasedCollection>>,
        lease: ProjectContentLease,
    ) -> Self {
        Self {
            inner: Arc::new(DomainAssetsInner {
                collections: collections.into_boxed_slice(),
                _custody: DomainAssetsCustody::Source { _lease: lease },
            }),
        }
    }

    pub(crate) fn from_runtime(
        collections: Vec<Box<dyn ErasedCollection>>,
        base: RuntimeContentBase,
        lease: RuntimeContentLease,
    ) -> Self {
        Self {
            inner: Arc::new(DomainAssetsInner {
                collections: collections.into_boxed_slice(),
                _custody: DomainAssetsCustody::Processed {
                    _lease: lease,
                    _base: base,
                },
            }),
        }
    }

    #[must_use]
    pub fn collection<D: AssetDomain>(&self) -> Option<&ProjectAssetCollection<D::Product>> {
        self.inner
            .collections
            .iter()
            .filter(|collection| collection.domain_type() == TypeId::of::<D>())
            .find_map(|collection| {
                collection
                    .as_any()
                    .downcast_ref::<ProjectAssetCollection<D::Product>>()
            })
    }
}

impl fmt::Debug for ProjectDomainAssets {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectDomainAssets")
            .field("collection_count", &self.inner.collections.len())
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "project-source")]
pub(crate) fn validate_product_membership<D: AssetDomain>(
    product: &D::Product,
    required: BTreeSet<AssetProductRef>,
) -> Result<(), AssetProductRef> {
    for reference in required {
        if !D::contains_product(product, reference.product()) {
            return Err(reference);
        }
    }
    Ok(())
}
