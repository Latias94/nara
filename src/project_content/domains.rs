//! Typed source content selected by the compiled product recipe.

use std::{
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
};

use nara_asset::{
    AssetImportError, AssetImportLimits, AssetImportSource, AssetMetaIdentityCandidate,
    SourceAssetImporter,
};
use nara_core::ByteLimit;
#[cfg(feature = "project-tooling")]
use nara_material::{Material2dAsset, Material2dDomain};

pub(super) use crate::project_assets::{CollectionStorage, ErasedCollection};
use crate::project_assets::{DomainCollection, validate_product_membership};

use super::*;

#[cfg(test)]
mod collection_tests;

fn import_error(error: AssetImportError<ProjectContentError>) -> ProjectContentError {
    match error {
        AssetImportError::Domain(diagnostic) => ProjectContentError::asset_domain(*diagnostic),
        AssetImportError::Source(error) => error,
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

#[cfg(feature = "project-tooling")]
impl ProjectContentSnapshot {
    /// Queries the admitted Runtime plan's source registrations, including unloaded domains.
    pub(crate) fn selects_source_domain<D: SourceAssetImporter>(&self) -> bool {
        self.inner
            .domain_authority
            .providers
            .iter()
            .any(|provider| {
                provider.domain_type() == std::any::TypeId::of::<D>()
                    && provider.source_kind() == D::SOURCE_KIND
            })
    }
}

fn validate_asset_domain_kind(
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
        sources: &mut SourceCandidates,
        existing: Option<Box<dyn ErasedCollection>>,
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
        sources: &mut SourceCandidates,
        mut existing: Option<Box<dyn ErasedCollection>>,
    ) -> Result<Box<dyn ErasedCollection>, ProjectContentError> {
        let limits = D::limits();
        let overhead = records.iter().try_fold(
            checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    if existing.is_none() {
                        size_of::<DomainCollection<D>>()
                    } else {
                        0
                    },
                    if existing.is_none() {
                        context
                            .asset_scope
                            .prefix()
                            .map_or(Ok(0), |prefix| string_retained_bytes(prefix.as_str()))?
                    } else {
                        0
                    },
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
        let transfer_work = if existing.is_some() {
            Some(context.reserve_work(checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                records.len(),
                size_of::<D::Product>(),
            )?)?)
        } else {
            None
        };
        let mut products = Vec::new();
        products
            .try_reserve_exact(records.len())
            .map_err(|_| allocation_failed("project.content.domain-allocation"))?;
        for record in &records {
            context.subtract_budget(ProjectContentBudgetKind::QueuedJobs, 1);
            context.add_budget(ProjectContentBudgetKind::InFlightJobs, 1)?;
            let (product, source_work) = prepare_domain_source::<D>(
                context,
                assets,
                root,
                record,
                cache,
                limits,
                ContentReadContext::Asset,
            )?;
            if let Some(required) = sources.products.remove(&record.stable_id()) {
                validate_product_membership::<D>(&product, required).map_err(missing_product)?;
            }
            D::visit_source_dependencies(&product, |reference, kind, demand| {
                context.collect_source_reference(assets, reference, Some(kind), demand, sources)
            })?;
            for reservation in source_work {
                context.subtract_budget(ProjectContentBudgetKind::WorkBytes, reservation.reserved);
            }
            context.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
            products.push(product);
        }
        if let Some(mut collection) = existing.take() {
            let previous = collection
                .as_any_mut()
                .downcast_mut::<ProjectAssetCollection<D::Product>>()
                .expect("the selected provider retains its original typed collection");
            let CollectionStorage::Source {
                scope,
                records: previous_records,
                products: previous_products,
                ..
            } = &mut previous.storage
            else {
                unreachable!("source import cannot append to a processed collection");
            };
            assert_eq!(*scope, context.asset_scope);
            let movement = context.reserve_work(checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [
                    checked_plan_product(
                        ProjectContentBudgetKind::WorkBytes,
                        previous_records.capacity(),
                        size_of::<AssetRecord>(),
                    )?,
                    checked_plan_product(
                        ProjectContentBudgetKind::WorkBytes,
                        previous_products.capacity(),
                        size_of::<D::Product>(),
                    )?,
                ],
            )?)?;
            previous_records
                .try_reserve_exact(records.len())
                .map_err(|_| allocation_failed("project.content.domain-allocation"))?;
            previous_products
                .try_reserve_exact(products.len())
                .map_err(|_| allocation_failed("project.content.domain-allocation"))?;
            previous_records.extend(records);
            previous_products.extend(products);
            context.subtract_budget(ProjectContentBudgetKind::WorkBytes, movement.reserved);
            if let Some(reservation) = transfer_work {
                context.subtract_budget(ProjectContentBudgetKind::WorkBytes, reservation.reserved);
            }
            return Ok(collection);
        }
        Ok(Box::new(DomainCollection::<D>(ProjectAssetCollection {
            storage: CollectionStorage::Source {
                scope: context.asset_scope.clone(),
                records,
                products,
                contains_product: D::contains_product,
            },
        })))
    }
}

fn prepare_domain_source<D: SourceAssetImporter>(
    context: &mut LoadContext<'_>,
    assets: &DirectoryCapability,
    root: &nara_project::ProjectPath,
    record: &AssetRecord,
    cache: &mut SourceCache,
    limits: AssetImportLimits,
    read: ContentReadContext<'_>,
) -> Result<(D::Product, [WorkReservation; 3]), ProjectContentError> {
    let local_path = context
        .asset_scope
        .relative_path(record.path())
        .ok_or_else(|| source_scope_mismatch(record.path()))?;
    // Cover the local source alias, sidecar spelling and temporary bound identity
    // before allocating any of them. Concrete reads stay under the Assets capability.
    let path_work = context.reserve_work(checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            string_retained_bytes(local_path)?,
            string_retained_bytes(local_path)?,
            ".meta".len(),
            asset_record_retained_bytes(record)?,
        ],
    )?)?;
    let path = context
        .asset_scope
        .relativize(record.path())
        .ok_or_else(|| source_scope_mismatch(record.path()))?;
    let metadata_path = path.meta_path();
    let metadata_limit = AssetMetaFileLimits::default().encoded_bytes().get();
    context.observe_source_read(Some(root), &metadata_path)?;
    let (bytes, digest) = context.read_file_inner(assets, &metadata_path, metadata_limit, read)?;
    context.verify_catalogued_metadata(record.path(), root, digest)?;
    context.record_member(
        ProjectContentMemberKind::AssetMetadata,
        root,
        &metadata_path,
        digest,
    )?;
    let metadata_work = context.reserve_work(metadata_limit)?;
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes_owned(bytes.into_boxed_slice())
        .map_err(|_| catalogue::identity_conflict(record.path()))?;
    if metadata.identity().path() != &path {
        return Err(catalogue::identity_conflict(record.path()));
    }
    if metadata
        .identity()
        .clone()
        .into_record(&context.asset_scope)
        != *record
    {
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
    let bytes = reader.read_source(&path, limits.source_bytes())?;
    let mut read_dependency = |path: &AssetPath, limit| reader.read_dependency(path, limit);
    let product = D::prepare(&mut AssetImportSource::new(
        &metadata,
        bytes,
        &mut read_dependency,
    ))
    .map_err(import_error)?;
    Ok((product, [path_work, metadata_work, work]))
}

fn source_scope_mismatch(path: &AssetSourcePath) -> ProjectContentError {
    ProjectContentError::with_identifier(
        ProjectContentErrorKind::AssetMetaMismatch,
        "project.content.domain-source-scope-mismatch",
        "Asset source is outside the admitted Assets scope",
        "asset",
        path.as_str(),
    )
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
        self.context
            .observe_source_read(Some(self.root), path.as_str())?;
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
        let (bytes, digest) = self.context.read_file_inner(
            self.assets,
            path.as_str(),
            limit.get(),
            ContentReadContext::ExactProjectMember {
                source_root: self.root,
            },
        )?;
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
            .observe_source_read(Some(self.root), path.as_str())?;
        self.context
            .add_budget(ProjectContentBudgetKind::DependencyEdges, 1)?;
        self.read_source(path, limit)
    }
}

impl LoadContext<'_> {
    /// Reads a stock material and its raw source inputs for Cook. The caller holds the
    /// retained reservation until the material is dropped.
    #[cfg(feature = "project-tooling")]
    pub(super) fn read_delivery_material(
        &mut self,
        assets: &DirectoryCapability,
        root: &nara_project::ProjectPath,
        record: &AssetRecord,
    ) -> Result<Option<Material2dAsset>, ProjectContentError> {
        let AssetSourceKind::Other(kind) = record.source_kind() else {
            return Ok(None);
        };
        if !self.domains.providers.iter().any(|provider| {
            provider.source_kind() == kind
                && provider.domain_type() == std::any::TypeId::of::<Material2dDomain>()
        }) {
            return Ok(None);
        }

        let mut cache = SourceCache::default();
        let prepared = prepare_domain_source::<Material2dDomain>(
            self,
            assets,
            root,
            record,
            &mut cache,
            Material2dDomain::limits(),
            ContentReadContext::ExactProjectMember { source_root: root },
        );
        let cache_work = cache.work_bytes;
        drop(cache);
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, cache_work);
        let (material, source_work) = prepared?;
        for reservation in source_work {
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, reservation.reserved);
        }
        Ok(Some(material))
    }

    pub(super) fn prepare_domain_collections(
        &mut self,
    ) -> Result<Vec<Box<dyn ErasedCollection>>, ProjectContentError> {
        let kind = ProjectContentBudgetKind::RetainedBytes;
        let provider_count = self.domains.providers.len();
        let collection_bytes = ProjectDomainAssets::runtime_metadata_bytes(provider_count)
            .ok_or_else(|| overflow_budget(kind))?;
        let provider_bytes =
            checked_plan_product(kind, provider_count, size_of::<Arc<dyn DomainProvider>>())?;
        let mut collections = Vec::new();
        self.add_budget(
            kind,
            checked_plan_sum(kind, [collection_bytes, provider_bytes])?,
        )?;
        collections
            .try_reserve_exact(provider_count)
            .map_err(|_| allocation_failed("project.content.domain-allocation"))?;
        Ok(collections)
    }

    pub(super) fn load_domains(
        &mut self,
        assets: &DirectoryCapability,
        root: &nara_project::ProjectPath,
        sources: &mut SourceCandidates,
        collections: &mut Vec<Box<dyn ErasedCollection>>,
    ) -> Result<(), ProjectContentError> {
        let providers = self.domains;
        let mut cache = SourceCache::default();
        loop {
            let mut progressed = false;
            for provider in &providers.providers {
                let existing_index = collections
                    .iter()
                    .position(|collection| collection.domain_type() == provider.domain_type());
                let loaded = existing_index
                    .map(|index| collections[index].records())
                    .unwrap_or(&[]);
                let lookup_work = sources.domains.iter().try_fold(0usize, |maximum, path| {
                    self.asset_resolution()
                        .path_resolution_work_bytes(path)
                        .map(|bytes| maximum.max(bytes))
                        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))
                })?;
                let lookup_reservation = self.reserve_work(lookup_work)?;
                let resolution = self.asset_resolution();
                let mut selected_records = sources.domains
                .iter()
                .filter_map(|path| resolution.record_for_path(path))
                .filter(|record| {
                    matches!(record.source_kind(), AssetSourceKind::Other(kind) if kind == provider.source_kind())
                })
                .filter(|record| !loaded.iter().any(|previous| previous.stable_id() == record.stable_id()));
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
                    self.subtract_budget(
                        ProjectContentBudgetKind::WorkBytes,
                        lookup_reservation.reserved,
                    );
                    continue;
                }
                let records_reservation = self.reserve_work(records_work)?;
                let mut records = Vec::new();
                records
                    .try_reserve_exact(count)
                    .map_err(|_| allocation_failed("project.content.domain-allocation"))?;
                let resolution = self.asset_resolution();
                records.extend(
                sources.domains
                    .iter()
                    .filter_map(|path| resolution.record_for_path(path))
                    .filter(|record| {
                        matches!(record.source_kind(), AssetSourceKind::Other(kind) if kind == provider.source_kind())
                    })
                    .filter(|record| !loaded.iter().any(|previous| previous.stable_id() == record.stable_id()))
                    .cloned(),
            );
                self.subtract_budget(
                    ProjectContentBudgetKind::WorkBytes,
                    lookup_reservation.reserved,
                );
                let existing = existing_index.map(|index| collections.remove(index));
                let collection = catch_unwind(AssertUnwindSafe(|| {
                    provider.load(self, assets, root, records, &mut cache, sources, existing)
                }))
                .map_err(|_| {
                    ProjectContentError::single(
                        ProjectContentErrorKind::AssetDomain,
                        "project.content.domain-panicked",
                        "Asset domain preparation panicked",
                    )
                })??;
                if let Some(index) = existing_index {
                    collections.insert(index, collection);
                } else {
                    collections.push(collection);
                }
                progressed = true;
                self.subtract_budget(
                    ProjectContentBudgetKind::WorkBytes,
                    records_reservation.reserved,
                );
            }
            if !progressed {
                break;
            }
        }
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, cache.work_bytes);
        Ok(())
    }

    pub(super) fn finish_domain_dependencies(
        &mut self,
        collections: &[Box<dyn ErasedCollection>],
        sources: &mut SourceCandidates,
    ) -> Result<(), ProjectContentError> {
        for collection in collections {
            collection
                .validate_product_selections(&mut sources.products)
                .map_err(missing_product)?;
        }
        if let Some(reference) = sources
            .products
            .values()
            .find_map(|products| products.first())
        {
            return Err(missing_product(*reference));
        }
        let paths = std::mem::take(&mut sources.domains);
        let queued_path_work = paths.iter().try_fold(0usize, |total, path| {
            checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [total, queued_source_path_work_bytes(path)?],
            )
        })?;
        drop(paths);
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, queued_path_work);
        Ok(())
    }
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
