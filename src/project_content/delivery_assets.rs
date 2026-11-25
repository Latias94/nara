use std::collections::{BTreeMap, btree_map::Entry};

use crate::fs::{ContentDigest, DirectoryCapability};
use nara_asset::{
    AssetMetaFileLimits, AssetMetaIdentityCandidate, AssetPath, AssetPathScope, AssetProductRef,
    AssetRef, AssetSourceKind, AssetSourcePath, ImportedProductId, ProjectAssetDatabase,
    StableAssetId,
};
use nara_project::ProjectPath;
use nara_render::material::{Material2dAsset, Material2dDomain};
use nara_render::tilemap::TileSetSource;

use super::{
    BTREE_ENTRY_OVERHEAD, ContentReadContext, DeliveryCatalogue, DeliveryImageExpectation,
    LoadContext, ProjectContentBudgetKind as Budget, ProjectContentDeliveryReceipt,
    ProjectContentError, ProjectContentErrorKind, ProjectContentLoader, ProjectContentMemberKind,
    ProjectImageContent, ProjectSettingsCandidate, RuntimePlan, STRING_ALLOCATION_OVERHEAD,
    catalogue::{
        CookedProductSource, ReferenceCatalogue, asset_record_retained_bytes, identity_conflict,
        metadata_source_changed, verify_selected_source,
    },
    checked_plan_sum,
    delivery::{DeliveryVisitError, delivery_member_changed, delivery_member_limit},
    string_retained_bytes,
};

pub(crate) struct DeliveryAssets<'a> {
    assets: Option<DirectoryCapability>,
    context: LoadContext<'a>,
    catalogue: Option<&'a DeliveryCatalogue>,
    asset_root: &'a ProjectPath,
    textures: BTreeMap<StableAssetId, bool>,
}

pub(crate) enum DeliveryAssetDefinition<'a> {
    TileSet(&'a TileSetSource, AssetProductRef),
    Material(&'a Material2dAsset),
}

impl ProjectContentLoader {
    /// Opens one bounded discovery pass when delivery has not already discovered every identity.
    /// Supplemental reads then use the original unique delivery limit for content members.
    pub(crate) fn delivery_assets<'a>(
        &'a self,
        candidate: &'a ProjectSettingsCandidate,
        plan: &'a RuntimePlan,
        receipt: &'a ProjectContentDeliveryReceipt,
        catalogue: Option<&'a DeliveryCatalogue>,
        selected: &dyn Fn(StableAssetId) -> bool,
    ) -> Result<DeliveryAssets<'a>, ProjectContentError> {
        let mut context = LoadContext::new(
            self,
            plan.schema_validation(),
            &plan.asset_domains,
            None,
            Default::default(),
            candidate.manifest_digest(),
            None,
            None,
            false,
            None,
        )?;
        context.add_budget(Budget::OpenHandles, 1)?;
        context.bind_source_roots(candidate)?;
        if catalogue.is_none() {
            context.scan_reference_catalogue(Some(selected))?;
        }
        context.set_budget(Budget::Files, receipt.members().len())?;
        context.supplemental_delivery = Some(receipt);
        Ok(DeliveryAssets {
            assets: None,
            context,
            catalogue,
            asset_root: &candidate.settings().paths.assets,
            textures: BTreeMap::new(),
        })
    }
}

impl<V, D> LoadContext<'_, V, D> {
    pub(super) fn admit_supplemental_read(
        &mut self,
        receipt: &ProjectContentDeliveryReceipt,
        root: &ProjectPath,
        path: &str,
    ) -> Result<Option<ContentDigest>, ProjectContentError> {
        let work = self.reserve_work(checked_plan_sum(
            Budget::WorkBytes,
            [
                root.as_str().len(),
                1,
                path.len(),
                STRING_ALLOCATION_OVERHEAD,
            ],
        )?)?;
        let path = format!("{}/{path}", root.as_str());
        let expected = self
            .content_closure
            .as_ref()
            .expect("supplemental reads own their file observations")
            .member_digest(&path)
            .or_else(|| receipt.member_for_path(&path).map(|member| member.digest()));
        drop(path);
        self.subtract_budget(Budget::WorkBytes, work.reserved);
        if expected.is_none() {
            self.add_budget(Budget::Files, 1)
                .map_err(|_| delivery_member_limit())?;
        }
        Ok(expected)
    }

    fn without_member_file_charge<T, E: From<ProjectContentError>>(
        &mut self,
        visit: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let files = self.budget_value(Budget::Files);
        self.set_budget(Budget::Files, 0)?;
        let result = visit(self);
        self.set_budget(Budget::Files, files)?;
        result
    }
}

impl DeliveryAssets<'_> {
    pub(crate) fn discover_sources(
        &mut self,
        selected: &dyn Fn(StableAssetId) -> bool,
    ) -> Result<(), ProjectContentError> {
        if self.catalogue.is_none() {
            self.context.without_member_file_charge(|context| {
                context.scan_reference_catalogue(Some(selected))
            })?;
        }
        Ok(())
    }

    /// Replays the same delivery while retaining supplemental observations and allocations.
    /// Each sequential traversal carries its own file and directory work counters.
    pub(crate) fn during_delivery_replay<E>(
        &mut self,
        replay: impl FnOnce() -> Result<(), DeliveryVisitError<E>>,
    ) -> Result<(), DeliveryVisitError<E>> {
        let counters = [
            Budget::Files,
            Budget::DirectoryEntries,
            Budget::DirectoryDepth,
        ];
        let prior = counters.map(|kind| (kind, self.context.budget_value(kind)));
        let budget = self.context.budget_mut();
        budget
            .set_many(&counters.map(|kind| (kind, 0)))
            .map_err(ProjectContentError::budget)?;
        let result = replay();
        budget
            .set_many(&prior)
            .map_err(ProjectContentError::budget)?;
        result
    }

    pub(crate) fn verify_source(
        &self,
        id: StableAssetId,
        source: &CookedProductSource,
    ) -> Result<(), ProjectContentError> {
        verify_selected_source(
            self.database(),
            self.reference_catalogue(),
            id,
            &source.path,
            source.metadata,
        )
    }

    /// Reads the current facts for a demanded, explicitly contributed build source.
    pub(crate) fn with_build_source<E>(
        &mut self,
        id: StableAssetId,
        kind: &str,
        source_bytes: usize,
        visit: impl FnOnce(
            &DirectoryCapability,
            &AssetPathScope,
            &AssetSourcePath,
            ContentDigest,
            ContentDigest,
        ) -> Result<(), E>,
    ) -> Result<bool, DeliveryVisitError<E>> {
        let Some(record) = self.database().record_for_stable_id(id) else {
            return Ok(false);
        };
        if !matches!(record.source_kind(), AssetSourceKind::Other(actual) if actual.as_str() == kind)
        {
            return Ok(false);
        }
        let Some(relative) = self.context.asset_scope.relative_path(record.path()) else {
            return Ok(false);
        };
        let Some(metadata) = self
            .reference_catalogue()
            .and_then(|catalogue| catalogue.metadata.get(record.path()))
            .map(|metadata| metadata.digest)
        else {
            return Ok(false);
        };
        let work_bytes = checked_plan_sum(
            Budget::WorkBytes,
            [
                string_retained_bytes(record.path().as_str())?,
                string_retained_bytes(relative)?,
                string_retained_bytes(relative)?,
                ".meta".len(),
            ],
        )?;
        let work = self.context.reserve_work(work_bytes)?;
        let path = self
            .database()
            .record_for_stable_id(id)
            .expect("reserving work keeps the selected source")
            .path()
            .clone();
        let local = self
            .context
            .asset_scope
            .relativize(&path)
            .ok_or_else(|| identity_conflict(&path))?;
        let metadata_path = local.meta_path();
        if self.assets.is_none() {
            self.assets = Some(self.context.open_directory(self.asset_root)?);
        }
        let assets = self
            .assets
            .as_ref()
            .expect("delivery assets directory is open");
        let mut read = |name: &str, limit, kind| {
            let (bytes, digest) = self.context.read_file_inner(
                assets,
                name,
                limit,
                ContentReadContext::ExactProjectMember {
                    source_root: self.asset_root,
                },
            )?;
            drop(bytes);
            self.context
                .record_member(kind, self.asset_root, name, digest)?;
            Ok::<_, ProjectContentError>(digest)
        };
        let observed = read(
            &metadata_path,
            AssetMetaFileLimits::default().encoded_bytes().get(),
            ProjectContentMemberKind::AssetMetadata,
        )?;
        if observed != metadata {
            return Err(metadata_source_changed(path.as_str()).into());
        }
        let primary = read(
            local.as_str(),
            source_bytes,
            ProjectContentMemberKind::AssetSource,
        )?;
        let result = visit(assets, &self.context.asset_scope, &path, primary, metadata);
        drop((path, local, metadata_path));
        self.context
            .subtract_budget(Budget::WorkBytes, work.reserved);
        result.map_err(DeliveryVisitError::Visit)?;
        Ok(true)
    }

    /// Acquires a demanded stock definition and borrows it until the existing encoder returns.
    pub(crate) fn with_definition<T, E>(
        &mut self,
        product: AssetProductRef,
        visit: impl FnOnce(DeliveryAssetDefinition<'_>, &AssetSourcePath, ContentDigest) -> Result<T, E>,
    ) -> Result<Option<T>, DeliveryVisitError<E>> {
        if product.product() != ImportedProductId::PRIMARY {
            return Ok(None);
        }
        let Some(record) = self.database().record_for_stable_id(product.source()) else {
            return Ok(None);
        };
        let AssetSourceKind::Other(kind) = record.source_kind() else {
            return Ok(None);
        };
        let tileset = match kind.as_str() {
            "tileset" => true,
            Material2dDomain::SOURCE_KIND => false,
            _ => return Ok(None),
        };
        let Some(relative) = self.context.asset_scope.relative_path(record.path()) else {
            return Ok(None);
        };
        let Some(metadata) = self
            .reference_catalogue()
            .and_then(|catalogue| catalogue.metadata.get(record.path()))
            .map(|metadata| metadata.digest)
        else {
            return Ok(None);
        };
        let work = self.context.reserve_work(checked_plan_sum(
            Budget::WorkBytes,
            [
                asset_record_retained_bytes(record)?,
                string_retained_bytes(relative)?,
                string_retained_bytes(record.path().as_str())?,
                ".meta".len(),
            ],
        )?)?;
        let record = self
            .database()
            .record_for_stable_id(product.source())
            .expect("reserving work keeps the selected source")
            .clone();
        let path = self
            .context
            .asset_scope
            .relativize(record.path())
            .expect("the selected source is inside the asset scope");
        let retained = self.context.budget_value(Budget::RetainedBytes);
        if self.assets.is_none() {
            self.assets = Some(self.context.open_directory(self.asset_root)?);
        }
        let assets = self.assets.as_ref().expect("assets directory is open");
        self.context.add_budget(Budget::InFlightJobs, 1)?;
        let definition = if tileset {
            let (observed, source, digest, _, record_work) = self.context.read_tileset_definition(
                assets,
                self.asset_root,
                &path,
                ContentReadContext::ExactProjectMember {
                    source_root: self.asset_root,
                },
            )?;
            if observed != record || digest != metadata {
                return Err(delivery_member_changed().into());
            }
            let image = self.resolve_texture(&source.image)?;
            let result = visit(
                DeliveryAssetDefinition::TileSet(&source, image),
                record.path(),
                metadata,
            );
            drop((observed, source));
            self.context
                .subtract_budget(Budget::WorkBytes, record_work.reserved);
            Some(result)
        } else if let Some(source) =
            self.context
                .read_delivery_material(assets, self.asset_root, &record)?
        {
            if self
                .context
                .content_closure
                .as_ref()
                .expect("stock reads retain their source observations")
                .member_digest(&record.path().meta_path())
                != Some(metadata)
            {
                return Err(delivery_member_changed().into());
            }
            let resolved =
                source.resolve_texture_references(|reference| self.resolve_texture(reference))?;
            Some(visit(
                DeliveryAssetDefinition::Material(&resolved),
                record.path(),
                metadata,
            ))
        } else {
            None
        };
        drop((record, path));
        self.context.subtract_budget(Budget::InFlightJobs, 1);
        self.context
            .subtract_budget(Budget::WorkBytes, work.reserved);
        self.context.set_budget(Budget::RetainedBytes, retained)?;
        definition.transpose().map_err(DeliveryVisitError::Visit)
    }

    /// Reports the original Image source for every selected stock texture, even when its output
    /// was already supplied. A filtered pass checks only identities missing from the first pass.
    pub(crate) fn verify_textures<E>(
        &mut self,
        mut visit: impl FnMut(StableAssetId, &AssetSourcePath, ContentDigest) -> Result<(), E>,
    ) -> Result<(), DeliveryVisitError<E>> {
        if self.textures.values().any(|&requires_scan| requires_scan) {
            self.context.without_member_file_charge(|context| {
                context.scan_reference_catalogue(Some(&|id| self.textures.get(&id) == Some(&true)))
            })?;
        }
        for &id in self.textures.keys() {
            let product = AssetProductRef::new(id, ImportedProductId::PRIMARY)
                .map_err(|_| unresolved_texture())?;
            let (path, _, metadata) = self.location(product).ok_or_else(unresolved_texture)?;
            visit(id, path, metadata).map_err(DeliveryVisitError::Visit)?;
        }
        Ok(())
    }

    pub(crate) fn observed_files(&self) -> impl Iterator<Item = (&str, ContentDigest)> {
        self.context
            .content_closure
            .as_ref()
            .expect("supplemental reads own their file observations")
            .supplemental_files()
    }

    fn resolve_texture(
        &mut self,
        reference: &AssetRef,
    ) -> Result<AssetProductRef, ProjectContentError> {
        self.context.add_budget(Budget::DependencyEdges, 1)?;
        if !reference.is_primary() {
            return Err(unresolved_texture());
        }
        let (id, new_path_identity) = match reference {
            AssetRef::Product(product) => (product.source(), false),
            AssetRef::StableId(id) => (*id, false),
            AssetRef::Path(path) => {
                let bytes = self
                    .database()
                    .resolver(&self.context.asset_scope)
                    .path_resolution_work_bytes(path)
                    .ok_or_else(unresolved_texture)?;
                let work = self.context.reserve_work(bytes)?;
                let record = self
                    .database()
                    .resolver(&self.context.asset_scope)
                    .record_for_path(path);
                let identity = match record {
                    Some(record) if record.source_kind() == &AssetSourceKind::Image => {
                        (record.stable_id(), false)
                    }
                    Some(_) => return Err(unresolved_texture()),
                    None if self.catalogue.is_none() => (self.read_texture_identity(path)?, true),
                    None => return Err(unresolved_texture()),
                };
                self.context
                    .subtract_budget(Budget::WorkBytes, work.reserved);
                identity
            }
        };
        let requires_scan = match self.database().record_for_stable_id(id) {
            Some(record)
                if record.source_kind() == &AssetSourceKind::Image
                    && self
                        .context
                        .asset_scope
                        .relative_path(record.path())
                        .is_some() =>
            {
                new_path_identity
            }
            None if self.catalogue.is_none() => true,
            _ => return Err(unresolved_texture()),
        };
        if let Entry::Vacant(entry) = self.textures.entry(id) {
            self.context.add_budget(
                Budget::WorkBytes,
                size_of::<(StableAssetId, bool)>() + BTREE_ENTRY_OVERHEAD,
            )?;
            entry.insert(requires_scan);
        }
        AssetProductRef::new(id, ImportedProductId::PRIMARY).map_err(|_| unresolved_texture())
    }

    fn read_texture_identity(
        &mut self,
        path: &AssetPath,
    ) -> Result<StableAssetId, ProjectContentError> {
        let ceiling = AssetMetaFileLimits::default().encoded_bytes().get();
        let scratch = self.context.reserve_work(checked_plan_sum(
            Budget::WorkBytes,
            [
                ceiling,
                string_retained_bytes(path.as_str())?,
                string_retained_bytes(path.as_str())?,
                ".meta".len(),
            ],
        )?)?;
        let metadata_path = path.meta_path();
        let (bytes, digest) = self.context.read_file_inner(
            self.assets
                .as_ref()
                .expect("stock definitions open the assets directory"),
            &metadata_path,
            ceiling,
            ContentReadContext::ExactProjectMember {
                source_root: self.asset_root,
            },
        )?;
        let identity =
            AssetMetaIdentityCandidate::decode_json_bytes_owned(bytes.into_boxed_slice())
                .map_err(|_| identity_conflict(path))?
                .into_identity();
        if identity.path() != path || identity.source_kind() != &AssetSourceKind::Image {
            return Err(identity_conflict(path));
        }
        let id = identity.stable_id();
        self.context.record_member(
            ProjectContentMemberKind::AssetMetadata,
            self.asset_root,
            &metadata_path,
            digest,
        )?;
        self.context
            .index_metadata_identity(path.clone(), 0, digest, identity)?;
        drop(metadata_path);
        self.context
            .subtract_budget(Budget::WorkBytes, scratch.reserved);
        Ok(id)
    }

    /// Supplies a demanded stock Image selected by stable identity in the catalogue.
    /// The decoded data retires when the visitor returns.
    pub(crate) fn with_image<E>(
        &mut self,
        product: AssetProductRef,
        visit: impl FnOnce(&ProjectImageContent) -> Result<(), E>,
    ) -> Result<bool, DeliveryVisitError<E>> {
        let Some((canonical, relative, _)) = self.location(product) else {
            return Ok(false);
        };
        let work_bytes = checked_plan_sum(
            Budget::WorkBytes,
            [
                string_retained_bytes(relative)?,
                canonical.as_str().len(),
                5,
                STRING_ALLOCATION_OVERHEAD,
            ],
        )?;
        let work = self.context.reserve_work(work_bytes)?;
        let (_, relative, metadata) = self
            .location(product)
            .expect("reserving work does not change the admitted sources");
        let path = AssetPath::new(relative).map_err(|_| {
            ProjectContentError::with_identifier(
                ProjectContentErrorKind::InvalidLogicalPath,
                "project.delivery.image-path-invalid",
                "Demanded Image has an invalid local asset path",
                "asset",
                relative,
            )
        })?;
        if self.assets.is_none() {
            self.assets = Some(self.context.open_directory(self.asset_root)?);
        }
        let artifact_before = self.context.budget_value(Budget::ArtifactBytes);
        let retained_before = self.context.budget_value(Budget::RetainedBytes);
        self.context.add_budget(Budget::InFlightJobs, 1)?;
        let loaded = self.context.load_image(
            self.assets
                .as_ref()
                .expect("the assets directory was opened"),
            self.asset_root,
            &path,
            Some(DeliveryImageExpectation { metadata }),
        );
        self.context.subtract_budget(Budget::InFlightJobs, 1);
        let image = loaded?;
        let result = visit(&image);
        drop((image, path));
        self.context
            .subtract_budget(Budget::WorkBytes, work.reserved);
        self.context
            .budget_mut()
            .set_many(&[
                (Budget::ArtifactBytes, artifact_before),
                (Budget::RetainedBytes, retained_before),
            ])
            .map_err(ProjectContentError::budget)?;
        result.map_err(DeliveryVisitError::Visit)?;
        Ok(true)
    }

    fn database(&self) -> &ProjectAssetDatabase {
        self.catalogue
            .map_or(&self.context.asset_database, |catalogue| {
                &catalogue.asset_database
            })
    }

    fn reference_catalogue(&self) -> Option<&ReferenceCatalogue> {
        match self.catalogue {
            Some(catalogue) => Some(&catalogue.catalogue),
            None => self.context.catalogue.as_ref(),
        }
    }

    fn location(
        &self,
        product: AssetProductRef,
    ) -> Option<(&AssetSourcePath, &str, ContentDigest)> {
        let catalogue = self.reference_catalogue()?;
        let record = self.database().record_for_stable_id(product.source())?;
        if record.source_kind() != &AssetSourceKind::Image {
            return None;
        }
        let relative = self.context.asset_scope.relative_path(record.path())?;
        let metadata = catalogue.metadata.get(record.path())?;
        Some((record.path(), relative, metadata.digest))
    }
}

fn unresolved_texture() -> ProjectContentError {
    ProjectContentError::single(
        ProjectContentErrorKind::AssetMetaMismatch,
        "project.delivery.stock-texture-unresolved",
        "Cooked stock texture needs a primary Image source",
    )
}
