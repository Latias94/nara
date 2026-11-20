use nara_asset::{AssetPath, AssetProductRef, AssetSourceKind, AssetSourcePath, ImportedProductId};
use nara_fs::{ContentDigest, DirectoryCapability};
use nara_project::ProjectPath;

use super::{
    DeliveryImageExpectation, LoadContext, ProjectContentBudgetKind as Budget,
    ProjectContentDeliveryReceipt, ProjectContentError, ProjectContentErrorKind,
    ProjectContentLoader, ProjectContentMemberKind, ProjectContentMemberReceipt,
    ProjectImageContent, ProjectSettingsCandidate, RuntimePlan, STRING_ALLOCATION_OVERHEAD,
    checked_plan_sum,
    delivery::{
        DeliverySources, DeliveryVisitError, delivery_member_changed, delivery_member_limit,
    },
    string_retained_bytes,
};

pub(crate) struct DeliveryImages<'a> {
    assets: Option<DirectoryCapability>,
    context: LoadContext<'a>,
    sources: &'a DeliverySources,
    receipt: &'a ProjectContentDeliveryReceipt,
    asset_root: &'a ProjectPath,
    files: usize,
}

impl ProjectContentLoader {
    pub(crate) fn delivery_images<'a>(
        &'a self,
        candidate: &'a ProjectSettingsCandidate,
        plan: &'a RuntimePlan,
        sources: &'a DeliverySources,
        receipt: &'a ProjectContentDeliveryReceipt,
    ) -> Result<DeliveryImages<'a>, ProjectContentError> {
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
        Ok(DeliveryImages {
            assets: None,
            context,
            sources,
            receipt,
            asset_root: &candidate.settings().paths.assets,
            files: receipt.members().len(),
        })
    }
}

impl DeliveryImages<'_> {
    /// Supplies a demanded stock Image from observed delivery sources or the lazy catalogue.
    /// The decoded data retires when the visitor returns.
    pub(crate) fn with_image<E>(
        &mut self,
        product: AssetProductRef,
        visit: impl FnOnce(&ProjectImageContent, ContentDigest, ContentDigest) -> Result<(), E>,
    ) -> Result<bool, DeliveryVisitError<E>> {
        if product.product() != ImportedProductId::PRIMARY {
            return Ok(false);
        }
        if self.observed_image(product).is_none() && self.sources.catalogue.is_none() {
            self.context.ensure_reference_catalogue()?;
        }
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
        let (canonical, relative, catalogued_metadata) = self
            .location(product)
            .expect("reserving work does not change the admitted sources");
        let original_source = self.receipt.member_for_path(canonical.as_str());
        let original_metadata = self.receipt.member_for_path(&canonical.meta_path());
        let files = checked_plan_sum(
            Budget::Files,
            [
                self.files,
                usize::from(original_source.is_none()),
                usize::from(original_metadata.is_none()),
            ],
        )?;
        if files > self.context.loader.limits.limit(Budget::Files) {
            return Err(delivery_member_limit().into());
        }
        let original_metadata = original_metadata.map(ProjectContentMemberReceipt::digest);
        if let (Some(original), Some(catalogued)) = (original_metadata, catalogued_metadata)
            && original != catalogued
        {
            return Err(delivery_member_changed().into());
        }
        let metadata_digest = original_metadata
            .or(catalogued_metadata)
            .ok_or_else(delivery_member_changed)?;
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
            Some(DeliveryImageExpectation {
                metadata: metadata_digest,
                source: original_source.map(ProjectContentMemberReceipt::digest),
            }),
        );
        self.context.subtract_budget(Budget::InFlightJobs, 1);
        let (image, source_digest) = loaded?;
        let result = visit(&image, source_digest, metadata_digest);
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
        self.files = files;
        Ok(true)
    }

    fn observed_image(&self, product: AssetProductRef) -> Option<&AssetSourcePath> {
        let identity = self.sources.identities.get(&product.source())?;
        if identity.is_image
            && self
                .receipt
                .member_for_path(identity.path.as_str())
                .is_some_and(|member| member.kind() == ProjectContentMemberKind::AssetSource)
        {
            Some(&identity.path)
        } else {
            None
        }
    }

    fn location(
        &self,
        product: AssetProductRef,
    ) -> Option<(&AssetSourcePath, &str, Option<ContentDigest>)> {
        if let Some(path) = self.observed_image(product) {
            return Some((path, self.context.asset_scope.relative_path(path)?, None));
        }
        let (database, catalogue) = match &self.sources.catalogue {
            Some(catalogue) => (&catalogue.asset_database, &catalogue.catalogue),
            None => (
                &self.context.asset_database,
                self.context.catalogue.as_ref()?,
            ),
        };
        let record = database.record_for_stable_id(product.source())?;
        if record.source_kind() != &AssetSourceKind::Image {
            return None;
        }
        let relative = self.context.asset_scope.relative_path(record.path())?;
        let metadata = catalogue.metadata.get(record.path())?;
        Some((record.path(), relative, Some(metadata.digest)))
    }
}
