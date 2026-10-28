//! Source-content contributions selected by the compiled product recipe.

use std::{
    any::Any,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
};

use nara_asset::{
    AssetImportError, AssetImportSource, AssetMetaIdentityCandidate, SourceAssetImporter,
};
use nara_core::ByteLimit;
use nara_ecs::Resource;

use super::*;

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
    records: Box<[AssetRecord]>,
    products: Box<[T]>,
}

impl<T> ProjectAssetCollection<T> {
    #[must_use]
    pub fn get(&self, reference: &AssetRef) -> Option<&T> {
        self.records
            .iter()
            .position(|record| match reference {
                AssetRef::Path(path) => record.path() == path,
                AssetRef::StableId(id) => record.stable_id() == *id,
            })
            .map(|index| &self.products[index])
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&AssetRecord, &T)> {
        self.records.iter().zip(self.products.iter())
    }
}

pub(super) trait ErasedCollection: Any + Send + Sync {
    fn domain_type(&self) -> std::any::TypeId;
    fn as_any(&self) -> &dyn Any;
    fn records(&self) -> &[AssetRecord];
}

struct DomainCollection<D: SourceAssetImporter>(ProjectAssetCollection<D::Product>);

impl<D: SourceAssetImporter> ErasedCollection for DomainCollection<D> {
    fn domain_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<D>()
    }
    fn as_any(&self) -> &dyn Any {
        &self.0
    }
    fn records(&self) -> &[AssetRecord] {
        &self.0.records
    }
}

/// Immutable, generation-retaining typed resource collections for the committed project Scene.
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
    _lease: ProjectContentLease,
    _generation: ProjectContentGenerationLease,
}

impl ProjectDomainAssets {
    pub(super) fn new(
        collections: Vec<Box<dyn ErasedCollection>>,
        lease: ProjectContentLease,
        generation: ProjectContentGenerationLease,
    ) -> Self {
        Self {
            inner: Arc::new(DomainAssetsInner {
                collections: collections.into_boxed_slice(),
                _lease: lease,
                _generation: generation,
            }),
        }
    }

    #[must_use]
    pub fn collection<D: SourceAssetImporter>(
        &self,
    ) -> Option<&ProjectAssetCollection<D::Product>> {
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
            drop(metadata);
            context.subtract_budget(ProjectContentBudgetKind::WorkBytes, metadata_work.reserved);
            context.subtract_budget(ProjectContentBudgetKind::WorkBytes, work.reserved);
            context.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
            products.push(product);
        }
        Ok(Box::new(DomainCollection::<D>(ProjectAssetCollection {
            records: records.into_boxed_slice(),
            products: products.into_boxed_slice(),
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
                provider.load(self, assets, root, records, &mut cache)
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
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, cache.work_bytes);
        Ok(collections)
    }
}
