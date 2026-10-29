//! Typed source and processed CPU content selected by the compiled product recipe.

use std::{
    any::Any,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
};

use nara_asset::{
    AssetDomain, AssetImportError, AssetImportSource, AssetMetaIdentityCandidate,
    SourceAssetImporter,
};
use nara_core::ByteLimit;
use nara_ecs::Resource;

use crate::runtime_content::{
    RuntimeContentAsset, RuntimeContentBase, RuntimeContentError, RuntimeContentLease,
};

use super::*;

#[cfg(test)]
mod collection_tests;
#[cfg(feature = "tooling")]
mod editor;
#[cfg(feature = "tooling")]
pub(crate) use editor::{EditorSourceValidation, validate_editor_source};

fn import_error(error: AssetImportError<ProjectContentError>) -> ProjectContentError {
    match error {
        AssetImportError::Domain(diagnostic) => ProjectContentError::asset_domain(*diagnostic),
        AssetImportError::Source(error) => error,
    }
}

/// Typed products and their admitted identities for one selected Scene.
pub struct ProjectAssetCollection<T> {
    storage: CollectionStorage<T>,
}

enum CollectionStorage<T> {
    Source {
        records: Box<[AssetRecord]>,
        products: Box<[T]>,
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
    /// IDs select only primary products. Processed values require an exact admitted Product
    /// mapping; an auxiliary selection never falls back to its primary product.
    /// This query performs no loading, importing, I/O or World mutation.
    #[must_use]
    pub fn get(&self, reference: &AssetRef) -> Option<&T> {
        match &self.storage {
            CollectionStorage::Source {
                records,
                products,
                contains_product,
            } => {
                let index = records.iter().position(|record| match reference {
                    AssetRef::Path(path) => record.path() == path,
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
            CollectionStorage::Source { products, .. } => CollectionIter::Source(products.iter()),
            CollectionStorage::Processed { products, .. } => {
                CollectionIter::Processed(products.iter())
            }
        }
    }

    /// Visits real source records only. Processed collections have no source metadata or aliases.
    pub fn source_records(&self) -> impl ExactSizeIterator<Item = &AssetRecord> {
        self.source_records_slice().iter()
    }

    pub(crate) fn runtime_acquisitions(&self) -> &[RuntimeContentAsset<T>] {
        match &self.storage {
            CollectionStorage::Processed { products, .. } => products,
            CollectionStorage::Source { .. } => &[],
        }
    }

    fn source_records_slice(&self) -> &[AssetRecord] {
        match &self.storage {
            CollectionStorage::Source { records, .. } => records,
            CollectionStorage::Processed { .. } => &[],
        }
    }
}

enum CollectionIter<'a, T> {
    Source(std::slice::Iter<'a, T>),
    Processed(std::slice::Iter<'a, RuntimeContentAsset<T>>),
}

impl<'a, T> Iterator for CollectionIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Source(products) => products.next(),
            Self::Processed(products) => products.next().map(RuntimeContentAsset::product),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Source(products) => products.size_hint(),
            Self::Processed(products) => products.size_hint(),
        }
    }
}

impl<T> ExactSizeIterator for CollectionIter<'_, T> {}

pub(crate) trait ErasedCollection: Any + Send + Sync {
    fn domain_type(&self) -> std::any::TypeId;
    fn as_any(&self) -> &dyn Any;
    fn records(&self) -> &[AssetRecord];
    fn validate_product_selections(
        &self,
        required: &mut RequiredAssetProducts,
    ) -> Result<(), AssetProductRef>;
}

struct DomainCollection<D: AssetDomain>(ProjectAssetCollection<D::Product>);

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
    fn domain_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<D>()
    }
    fn as_any(&self) -> &dyn Any {
        &self.0
    }
    fn records(&self) -> &[AssetRecord] {
        self.0.source_records_slice()
    }
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
    Source {
        _lease: ProjectContentLease,
        _generation: ProjectContentGenerationLease,
    },
    Processed {
        _lease: RuntimeContentLease,
        _base: RuntimeContentBase,
    },
}

impl ProjectDomainAssets {
    pub(crate) fn runtime_metadata_bytes(collections: usize) -> Option<usize> {
        std::mem::size_of::<DomainAssetsInner>()
            .checked_add(crate::scene_retention::ARC_CONTROL_BLOCK_BYTES)?
            .checked_add(collections.checked_mul(std::mem::size_of::<Box<dyn ErasedCollection>>())?)
    }

    pub(super) fn validate_product_selections(
        &self,
        required: &mut RequiredAssetProducts,
    ) -> Result<(), ProjectContentError> {
        catch_unwind(AssertUnwindSafe(|| {
            for collection in &self.inner.collections {
                collection
                    .validate_product_selections(required)
                    .map_err(missing_product)?;
            }
            if let Some(reference) = required.values().find_map(|products| products.first()) {
                return Err(missing_product(*reference));
            }
            Ok(())
        }))
        .map_err(|_| {
            ProjectContentError::single(
                ProjectContentErrorKind::AssetDomain,
                "project.content.domain-product-check-panicked",
                "Prepared asset product membership query panicked",
            )
        })?
    }

    pub(super) fn new(
        collections: Vec<Box<dyn ErasedCollection>>,
        lease: ProjectContentLease,
        generation: ProjectContentGenerationLease,
    ) -> Self {
        Self {
            inner: Arc::new(DomainAssetsInner {
                collections: collections.into_boxed_slice(),
                _custody: DomainAssetsCustody::Source {
                    _lease: lease,
                    _generation: generation,
                },
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
            .filter(|collection| collection.domain_type() == std::any::TypeId::of::<D>())
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

#[derive(Clone, Default)]
pub(crate) struct ProjectAssetDomains {
    providers: Vec<Arc<dyn DomainProvider>>,
}

impl fmt::Debug for ProjectAssetDomains {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entries(self.providers.iter().map(|p| p.source_kind()))
            .finish()
    }
}

impl ProjectAssetDomains {
    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.providers.len() == other.providers.len()
            && self.providers.iter().all(|provider| {
                other.providers.iter().any(|candidate| {
                    provider.source_kind() == candidate.source_kind()
                        && provider.domain_type() == candidate.domain_type()
                })
            })
    }

    pub(crate) const fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub(crate) fn add<D: SourceAssetImporter>(&mut self) -> Result<(), crate::ProductRecipeError> {
        validate_asset_domain_kind(
            D::SOURCE_KIND,
            self.providers
                .iter()
                .any(|provider| provider.source_kind() == D::SOURCE_KIND),
        )?;
        self.providers.push(Arc::new(TypedDomain::<D>(PhantomData)));
        Ok(())
    }

    pub(super) fn supports(&self, kind: &AssetSourceKind) -> bool {
        matches!(kind, AssetSourceKind::Other(kind) if self.providers.iter().any(|provider| provider.source_kind() == kind))
    }
}

pub(crate) fn validate_asset_domain_kind(
    kind: &'static str,
    duplicate: bool,
) -> Result<(), crate::ProductRecipeError> {
    if kind.is_empty() || kind == "tileset" {
        return Err(crate::ProductRecipeError::ReservedAssetDomain { kind });
    }
    if duplicate {
        return Err(crate::ProductRecipeError::DuplicateAssetDomain { kind });
    }
    Ok(())
}

trait DomainProvider: Send + Sync {
    fn source_kind(&self) -> &'static str;
    fn domain_type(&self) -> std::any::TypeId;
    fn load(
        &self,
        context: &mut LoadContext<'_>,
        assets: &DirectoryCapability,
        root: &nara_project::ProjectPath,
        records: Vec<AssetRecord>,
        cache: &mut SourceCache,
        required_products: &mut RequiredAssetProducts,
    ) -> Result<Box<dyn ErasedCollection>, ProjectContentError>;
}

struct TypedDomain<D>(PhantomData<fn() -> D>);

impl<D: SourceAssetImporter> DomainProvider for TypedDomain<D> {
    fn domain_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<D>()
    }
    fn source_kind(&self) -> &'static str {
        D::SOURCE_KIND
    }

    fn load(
        &self,
        context: &mut LoadContext<'_>,
        assets: &DirectoryCapability,
        root: &nara_project::ProjectPath,
        records: Vec<AssetRecord>,
        cache: &mut SourceCache,
        required_products: &mut RequiredAssetProducts,
    ) -> Result<Box<dyn ErasedCollection>, ProjectContentError> {
        let limits = D::limits();
        let overhead = records.iter().try_fold(
            checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    size_of::<DomainCollection<D>>(),
                    checked_plan_product(
                        ProjectContentBudgetKind::RetainedBytes,
                        records.len(),
                        size_of::<D::Product>(),
                    )?,
                ],
            )?,
            |total, record| {
                checked_plan_sum(
                    ProjectContentBudgetKind::RetainedBytes,
                    [total, asset_record_retained_bytes(record)?],
                )
            },
        )?;
        context.add_budget(ProjectContentBudgetKind::RetainedBytes, overhead)?;
        let mut products = Vec::new();
        products
            .try_reserve_exact(records.len())
            .map_err(|_| allocation_failed("project.content.domain-allocation"))?;
        for record in &records {
            context.subtract_budget(ProjectContentBudgetKind::QueuedJobs, 1);
            context.add_budget(ProjectContentBudgetKind::InFlightJobs, 1)?;
            let metadata_path = record.path().meta_path();
            let (bytes, digest) = context.read_file(
                assets,
                &metadata_path,
                nara_asset::AssetMetaFileLimits::default()
                    .encoded_bytes()
                    .get(),
            )?;
            context.verify_catalogued_metadata(record.path(), root, digest)?;
            context.record_member(
                ProjectContentMemberKind::AssetMetadata,
                root,
                &metadata_path,
                digest,
            )?;
            let metadata_work = context.reserve_work(
                nara_asset::AssetMetaFileLimits::default()
                    .encoded_bytes()
                    .get(),
            )?;
            let metadata =
                AssetMetaIdentityCandidate::decode_json_bytes_owned(bytes.into_boxed_slice())
                    .map_err(|_| catalogue::identity_conflict(record.path()))?;
            if AssetRecord::from(metadata.identity().clone()) != *record {
                return Err(catalogue::identity_conflict(record.path()));
            }
            context.add_budget(
                ProjectContentBudgetKind::RetainedBytes,
                limits.retained_bytes().get(),
            )?;
            let work = context.reserve_work(limits.work_bytes().get())?;
            let mut reader = DomainSourceReader {
                context,
                assets,
                root,
                cache,
            };
            let bytes = reader.read_source(record.path(), limits.source_bytes())?;
            let mut read_dependency = |path: &AssetPath, limit| reader.read_dependency(path, limit);
            let product = D::prepare(&mut AssetImportSource::new(
                &metadata,
                bytes,
                &mut read_dependency,
            ))
            .map_err(import_error)?;
            if let Some(required) = required_products.remove(&record.stable_id()) {
                validate_product_membership::<D>(&product, required).map_err(missing_product)?;
            }
            drop(metadata);
            context.subtract_budget(ProjectContentBudgetKind::WorkBytes, metadata_work.reserved);
            context.subtract_budget(ProjectContentBudgetKind::WorkBytes, work.reserved);
            context.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
            products.push(product);
        }
        Ok(Box::new(DomainCollection::<D>(ProjectAssetCollection {
            storage: CollectionStorage::Source {
                records: records.into_boxed_slice(),
                products: products.into_boxed_slice(),
                contains_product: D::contains_product,
            },
        })))
    }
}

#[derive(Default)]
struct SourceCache {
    sources: BTreeMap<AssetPath, Arc<[u8]>>,
    work_bytes: usize,
}

struct DomainSourceReader<'a, 'host> {
    context: &'a mut LoadContext<'host>,
    assets: &'a DirectoryCapability,
    root: &'a nara_project::ProjectPath,
    cache: &'a mut SourceCache,
}

impl DomainSourceReader<'_, '_> {
    fn read_source(
        &mut self,
        path: &AssetPath,
        limit: ByteLimit,
    ) -> Result<Arc<[u8]>, ProjectContentError> {
        if let Some(bytes) = self.cache.sources.get(path) {
            if bytes.len() > limit.get() {
                return Err(ProjectContentError::single(
                    ProjectContentErrorKind::AssetDomain,
                    "project.content.domain-source-limit",
                    "Asset source exceeds the domain input limit",
                ));
            }
            return Ok(Arc::clone(bytes));
        }
        let overhead = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                size_of::<(AssetPath, Arc<[u8]>)>(),
                BTREE_ENTRY_OVERHEAD,
                ARC_CONTROL_BLOCK_BYTES,
                path.as_str().len(),
            ],
        )?;
        self.context
            .add_budget(ProjectContentBudgetKind::WorkBytes, overhead)?;
        self.cache.work_bytes = self
            .cache
            .work_bytes
            .checked_add(overhead)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let (bytes, digest) = self
            .context
            .read_file(self.assets, path.as_str(), limit.get())?;
        self.context.record_member(
            ProjectContentMemberKind::AssetSource,
            self.root,
            path.as_str(),
            digest,
        )?;
        let conversion = self.context.reserve_work(bytes.len())?;
        let bytes: Arc<[u8]> = bytes.into();
        self.context
            .subtract_budget(ProjectContentBudgetKind::WorkBytes, conversion.reserved);
        self.cache.sources.insert(path.clone(), Arc::clone(&bytes));
        Ok(bytes)
    }
}

impl DomainSourceReader<'_, '_> {
    fn read_dependency(
        &mut self,
        path: &AssetPath,
        limit: ByteLimit,
    ) -> Result<Arc<[u8]>, ProjectContentError> {
        self.context
            .add_budget(ProjectContentBudgetKind::DependencyEdges, 1)?;
        self.read_source(path, limit)
    }
}

impl LoadContext<'_> {
    pub(super) fn load_domains(
        &mut self,
        assets: &DirectoryCapability,
        root: &nara_project::ProjectPath,
        paths: BTreeSet<AssetPath>,
        required_products: &mut RequiredAssetProducts,
    ) -> Result<Vec<Box<dyn ErasedCollection>>, ProjectContentError> {
        let providers = self.domains;
        let mut collections = Vec::new();
        self.add_budget(
            ProjectContentBudgetKind::RetainedBytes,
            checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    size_of::<DomainAssetsInner>(),
                    ARC_CONTROL_BLOCK_BYTES,
                    checked_plan_product(
                        ProjectContentBudgetKind::RetainedBytes,
                        providers.providers.len(),
                        size_of::<Box<dyn ErasedCollection>>()
                            + size_of::<Arc<dyn DomainProvider>>(),
                    )?,
                ],
            )?,
        )?;
        collections
            .try_reserve_exact(providers.providers.len())
            .map_err(|_| allocation_failed("project.content.domain-allocation"))?;
        let mut cache = SourceCache::default();
        for provider in &providers.providers {
            let mut selected_records = paths
                .iter()
                .filter_map(|path| self.asset_database.record_for_path(path))
                .filter(|record| {
                    matches!(record.source_kind(), AssetSourceKind::Other(kind) if kind == provider.source_kind())
                });
            let (count, records_work) =
                selected_records.try_fold((0usize, 0usize), |(count, total), record| {
                    Ok::<_, ProjectContentError>((
                        count + 1,
                        checked_plan_sum(
                            ProjectContentBudgetKind::WorkBytes,
                            [total, asset_record_retained_bytes(record)?],
                        )?,
                    ))
                })?;
            if count == 0 {
                continue;
            }
            let records_reservation = self.reserve_work(records_work)?;
            let mut records = Vec::new();
            records
                .try_reserve_exact(count)
                .map_err(|_| allocation_failed("project.content.domain-allocation"))?;
            records.extend(
                paths
                    .iter()
                    .filter_map(|path| self.asset_database.record_for_path(path))
                    .filter(|record| {
                        matches!(record.source_kind(), AssetSourceKind::Other(kind) if kind == provider.source_kind())
                    })
                    .cloned(),
            );
            let collection = catch_unwind(AssertUnwindSafe(|| {
                provider.load(self, assets, root, records, &mut cache, required_products)
            }))
            .map_err(|_| {
                ProjectContentError::single(
                    ProjectContentErrorKind::AssetDomain,
                    "project.content.domain-panicked",
                    "Asset domain preparation panicked",
                )
            })??;
            collections.push(collection);
            self.subtract_budget(
                ProjectContentBudgetKind::WorkBytes,
                records_reservation.reserved,
            );
        }
        if let Some(reference) = required_products
            .values()
            .find_map(|products| products.first())
        {
            return Err(missing_product(*reference));
        }
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, cache.work_bytes);
        Ok(collections)
    }
}

fn validate_product_membership<D: AssetDomain>(
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

fn missing_product(reference: AssetProductRef) -> ProjectContentError {
    ProjectContentError::with_identifier(
        ProjectContentErrorKind::AssetReference,
        "project.content.asset-product-missing",
        "Prepared asset does not contain a required product",
        "asset-product",
        &AssetRef::Product(reference).to_string(),
    )
}
