use std::{
    any::TypeId,
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
};

use crate::{
    asset::{
        AssetPathScope, AssetProductRef, AssetRef, AssetReferenceResolver, AssetSourceKind,
        AssetSourcePath, ImportedProductId, SourceAssetBuild, StableAssetId,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildObservation, ArtifactBuildStore,
        ArtifactContentDigest, ArtifactSourceRequest, VerifiedArtifactGroup, build_source,
    },
    core::FormatVersion,
    fs::{DirectoryCapability, RelativePath},
    material::Material2dDomain,
    project_content::{DeliverySnapshotPhase, DeliveryVisitError, SelectedSceneProduct},
    project_host::{
        ProjectContentLoader, ProjectContentMemberKind, ProjectContentSnapshot,
        resolve_product_recipe,
    },
    project_workflow::cooked_content::{CookedContentBuilder, CookedContentRequest},
    tilemap::TileSetDescriptor,
};

use super::{
    PreparedProjectCook, ProjectCookError, ProjectCookRequest, ProjectCookSourceSelection,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Output {
    Scene,
    Image,
    Input,
    Material,
    TileSet,
    Group(ArtifactContentDigest),
}

#[derive(Default)]
struct Observations {
    raw: BTreeMap<AssetSourcePath, Option<ArtifactContentDigest>>,
    processed: Vec<ArtifactBuildObservation>,
}

impl Observations {
    fn raw(
        &mut self,
        builder: &mut CookedContentBuilder,
        path: &str,
        digest: Option<ArtifactContentDigest>,
    ) -> Result<(), ProjectCookError> {
        if let Some(previous) = self.raw.get(path) {
            return if *previous == digest {
                Ok(())
            } else {
                Err(ProjectCookError::Invalid(format!(
                    "source selection changed: {path}"
                )))
            };
        }
        builder.reserve_consumer_metadata(metadata_bytes::<(
            AssetSourcePath,
            Option<ArtifactContentDigest>,
        )>(path.len())?)?;
        let key = AssetSourcePath::new(path)
            .map_err(|error| ProjectCookError::Invalid(error.to_string()))?;
        self.raw.insert(key, digest);
        Ok(())
    }

    fn add(
        &mut self,
        builder: &mut CookedContentBuilder,
        observation: &ArtifactBuildObservation,
    ) -> Result<(), ProjectCookError> {
        match observation {
            ArtifactBuildObservation::Raw { path, digest } => {
                self.raw(builder, path.as_str(), *digest)
            }
            ArtifactBuildObservation::Processed { format, .. } => {
                let bytes = size_of::<ArtifactBuildObservation>()
                    .checked_mul(2)
                    .and_then(|bytes| bytes.checked_add(format.as_str().len()))
                    .ok_or_else(overflow)?;
                builder.reserve_consumer_metadata(bytes)?;
                self.processed.push(observation.clone());
                Ok(())
            }
        }
    }

    fn finish(
        self,
        builder: &mut CookedContentBuilder,
    ) -> Result<Vec<ArtifactBuildObservation>, ProjectCookError> {
        let count = self
            .raw
            .len()
            .checked_add(self.processed.len())
            .ok_or_else(overflow)?;
        builder.reserve_consumer_metadata(
            count
                .checked_mul(size_of::<ArtifactBuildObservation>())
                .ok_or_else(overflow)?,
        )?;
        let mut result = Vec::with_capacity(count);
        result.extend(
            self.raw
                .into_iter()
                .map(|(path, digest)| ArtifactBuildObservation::Raw { path, digest }),
        );
        result.extend(self.processed);
        Ok(result)
    }
}

fn overflow() -> ProjectCookError {
    crate::project_workflow::cooked_content::CookedContentError::Overflow.into()
}

fn metadata_bytes<T>(strings: usize) -> Result<usize, ProjectCookError> {
    size_of::<T>()
        .checked_add(64)
        .and_then(|bytes| bytes.checked_mul(2))
        .and_then(|bytes| bytes.checked_add(strings))
        .ok_or_else(overflow)
}

/// A borrowed complete source generation and its original unpublished assembly.
///
/// Select additional typed build facets with [`Self::add_source_assets`], or supply a complete
/// verified group with [`Self::add_group`]. This does not register another importer or mutate a
/// World. The callback must propagate failed contributions; final closure admission remains with
/// the original cooked-content builder.
pub struct ProjectCookContext<'a> {
    snapshot: &'a ProjectContentSnapshot,
    root: &'a DirectoryCapability,
    store: &'a ArtifactBuildStore,
    implementation: ArtifactContentDigest,
    builder: &'a mut CookedContentBuilder,
    outputs: &'a mut BTreeMap<AssetProductRef, Output>,
    built_sources: &'a mut BTreeMap<StableAssetId, TypeId>,
    observations: &'a mut Observations,
}

impl ProjectCookContext<'_> {
    #[must_use]
    pub fn snapshot(&self) -> &ProjectContentSnapshot {
        self.snapshot
    }

    /// Builds the actual sources admitted for `D`, preserving its separate build operation.
    ///
    /// The original artifact owner records optional/build-only inputs and complete compound
    /// mappings. Shared sources are built once in this assembly; source-only import values are
    /// not substituted for that build contract.
    pub fn add_source_assets<D>(&mut self) -> Result<(), ProjectCookError>
    where
        D: SourceAssetBuild,
        D::Settings: serde::Serialize,
    {
        let Some(collection) = self.snapshot.domain_assets().collection::<D>() else {
            return Ok(());
        };
        let scope = self.snapshot.asset_resolution().scope();
        let assets = self.open_source_directory(scope)?;
        let domain = TypeId::of::<D>();
        for record in collection.source_records() {
            if let Some(built) = self.built_sources.get(&record.stable_id()) {
                if *built != domain {
                    return Err(ProjectCookError::Invalid(format!(
                        "source {} changed its selected build domain",
                        record.path()
                    )));
                }
                continue;
            }
            self.builder
                .reserve_consumer_metadata(metadata_bytes::<(StableAssetId, TypeId)>(0)?)?;
            let request = self.source_request::<D>(scope, assets.share(), record.path())?;
            let group = build_source::<D>(self.store, &request, |_| Ok(()))?.into_group();
            self.add_verified_group(&group)?;
            self.built_sources.insert(record.stable_id(), domain);
        }
        Ok(())
    }

    /// Adds a verified group for the current typed source selection.
    ///
    /// First admission requires the selected snapshot to contain the source. The build owner
    /// compares its normalized settings, source identity, primary bytes and compiled definition
    /// with the retained seed. The existing group supplies the build observations for publication.
    pub fn add_group<D>(&mut self, group: &VerifiedArtifactGroup) -> Result<(), ProjectCookError>
    where
        D: SourceAssetBuild,
        D::Settings: serde::Serialize,
    {
        let source = group.seed().source;
        let domain = TypeId::of::<D>();
        if self.outputs.get(&source) == Some(&Output::Group(group.digest())) {
            return if self.built_sources.get(&source.source()) == Some(&domain) {
                Ok(())
            } else {
                Err(ProjectCookError::Invalid(format!(
                    "artifact group already belongs to a different source build domain: {source}"
                )))
            };
        }
        let record = self
            .snapshot
            .asset_database()
            .record_for_stable_id(source.source())
            .filter(|record| {
                matches!(record.source_kind(), AssetSourceKind::Other(kind) if kind == D::SOURCE_KIND)
            })
            .filter(|_| self.snapshot.domain_assets().collection::<D>().is_some())
            .ok_or_else(|| {
                ProjectCookError::Invalid(format!(
                    "artifact group has no selected source for {}: {source}",
                    D::SOURCE_KIND,
                ))
            })?;
        let scope = self.snapshot.asset_resolution().scope();
        self.builder
            .reserve_consumer_metadata(metadata_bytes::<(StableAssetId, TypeId)>(0)?)?;
        let directory = self.open_source_directory(scope)?;
        let request = self.source_request::<D>(scope, directory, record.path())?;
        if !request.matches_source_group::<D>(self.store, group)? {
            return Err(ProjectCookError::Invalid(format!(
                "artifact group does not match the selected source build: {}",
                record.path(),
            )));
        }
        self.add_verified_group(group)?;
        self.built_sources.insert(source.source(), domain);
        Ok(())
    }

    fn open_source_directory(
        &self,
        scope: &AssetPathScope,
    ) -> Result<DirectoryCapability, ProjectCookError> {
        match scope.prefix() {
            Some(prefix) => Ok(self
                .root
                .open_directory_exact(&RelativePath::new(prefix.as_str())?)?),
            None => Ok(self.root.share()),
        }
    }

    fn source_request<D>(
        &self,
        scope: &AssetPathScope,
        directory: DirectoryCapability,
        path: &AssetSourcePath,
    ) -> Result<ArtifactSourceRequest, ProjectCookError>
    where
        D: SourceAssetBuild,
        D::Settings: serde::Serialize,
    {
        let local = scope.relativize(path).ok_or_else(|| {
            ProjectCookError::Invalid("source build escaped its admitted asset scope".into())
        })?;
        let revision = FormatVersion::new(D::BUILD_REVISION).ok_or_else(|| {
            ProjectCookError::Invalid("source-build revision must be nonzero".into())
        })?;
        let closure = self.snapshot.content_closure();
        let members = closure.members();
        let digest = |path: &str, kind| {
            members
                .binary_search_by(|member| member.path().as_str().cmp(path))
                .ok()
                .map(|index| &members[index])
                .filter(|member| member.kind() == kind)
                .map(|member| ArtifactContentDigest::from_content(member.digest()))
                .ok_or_else(|| {
                    ProjectCookError::Invalid(format!("source build input is not selected: {path}"))
                })
        };
        let primary = digest(path.as_str(), ProjectContentMemberKind::AssetSource)?;
        let metadata = digest(&path.meta_path(), ProjectContentMemberKind::AssetMetadata)?;
        Ok(ArtifactSourceRequest::new(
            directory,
            scope.clone(),
            local,
            ArtifactBuildDefinition {
                id: D::BUILD_ID.to_owned(),
                revision,
                implementation: self.implementation,
            },
        )
        .with_source_digests(primary, metadata))
    }

    fn add_verified_group(
        &mut self,
        group: &VerifiedArtifactGroup,
    ) -> Result<(), ProjectCookError> {
        let output = Output::Group(group.digest());
        let products = group
            .members()
            .iter()
            .flat_map(|member| member.products.iter());
        let mut present = 0;
        let mut total = 0;
        for product in products {
            total += 1;
            if let Some(previous) = self.outputs.get(product) {
                if *previous != output {
                    return Err(ProjectCookError::Invalid(format!(
                        "cooked Product {product} has conflicting output"
                    )));
                }
                present += 1;
            }
        }
        if total != 0 && total == present {
            return Ok(());
        }
        if present != 0 {
            return Err(ProjectCookError::Invalid(
                "a complete group overlaps an already cooked Product".into(),
            ));
        }
        for product in group.members().iter().flat_map(|member| &member.products) {
            reserve_output(self.builder)?;
        }
        self.builder.add_group(group)?;
        for product in group.members().iter().flat_map(|member| &member.products) {
            self.outputs.insert(*product, output);
        }
        for observation in group.observations() {
            self.observations.add(self.builder, observation)?;
        }
        Ok(())
    }

    fn stock_assets(&mut self) -> Result<(), ProjectCookError> {
        for image in self.snapshot.images() {
            let product = image.image().product().ok_or_else(|| {
                ProjectCookError::Invalid(
                    "a cooked Image requires persistent Product identity".into(),
                )
            })?;
            if self.needs(product, Output::Image)? {
                reserve_output(self.builder)?;
                self.builder.add_image(image.image())?;
                self.outputs.insert(product, Output::Image);
            }
        }
        if let Some(input) = self.snapshot.input_action_map() {
            let path_bytes = input
                .path()
                .as_str()
                .len()
                .checked_add(5)
                .ok_or_else(overflow)?;
            self.builder.admit_consumer_work(
                path_bytes
                    .checked_add(metadata_bytes::<(
                        AssetSourcePath,
                        Option<ArtifactContentDigest>,
                    )>(path_bytes)?)
                    .ok_or_else(overflow)?,
            )?;
            self.observations.raw(
                self.builder,
                &input.path().meta_path(),
                input
                    .metadata_digest()
                    .map(ArtifactContentDigest::from_content),
            )?;
            if self.needs(input.product(), Output::Input)? {
                reserve_output(self.builder)?;
                self.builder.add_input(input.asset())?;
                self.outputs.insert(input.product(), Output::Input);
            }
        }
        let resolver = self.snapshot.asset_resolution();
        for tileset in self.snapshot.tilesets() {
            let product = AssetProductRef::new(tileset.stable_id(), ImportedProductId::PRIMARY)
                .map_err(|error| ProjectCookError::Invalid(error.to_string()))?;
            if !self.needs(product, Output::TileSet)? {
                continue;
            }
            reserve_output(self.builder)?;
            let source = tileset.source();
            let image = resolve_image(resolver, &source.image)?;
            let descriptor = TileSetDescriptor::new(product, image, source.atlas)
                .map(|value| value.with_sampler(source.sampler))
                .map(|value| value.with_alpha_mode(source.alpha_mode))
                .and_then(|value| value.with_tint(source.tint))
                .map_err(|error| ProjectCookError::Invalid(error.to_string()))?;
            self.builder.add_tileset(&descriptor)?;
            self.outputs.insert(product, Output::TileSet);
        }
        if let Some(materials) = self
            .snapshot
            .domain_assets()
            .collection::<Material2dDomain>()
        {
            for material in materials.iter() {
                let product = material.product();
                if !self.needs(product, Output::Material)? {
                    continue;
                }
                reserve_output(self.builder)?;
                let material = material
                    .resolve_texture_references(|reference| resolve_image(resolver, reference))?;
                self.builder.add_material(&material)?;
                self.outputs.insert(product, Output::Material);
            }
        }
        Ok(())
    }

    fn scenes(&mut self) -> Result<(), ProjectCookError> {
        let selected = self.snapshot.selected_scene_product().ok_or_else(|| {
            ProjectCookError::Invalid("cooked Scenes require authored stable metadata".into())
        })?;
        self.builder.declare_scene_root(selected)?;
        if self.needs(selected, Output::Scene)? {
            reserve_output(self.builder)?;
            self.builder.add_scene(
                selected,
                self.snapshot.compiled_candidate(),
                self.snapshot.asset_resolution(),
            )?;
            self.outputs.insert(selected, Output::Scene);
        }
        let resolver = self.snapshot.asset_resolution();
        for scene in self.snapshot.scene_dependencies() {
            let record = resolver
                .database()
                .record_for_path(scene.path())
                .ok_or_else(|| {
                    ProjectCookError::Invalid(format!(
                        "Scene {} lost its admitted identity",
                        scene.path()
                    ))
                })?;
            let product = AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
                .map_err(|error| ProjectCookError::Invalid(error.to_string()))?;
            if self.needs(product, Output::Scene)? {
                reserve_output(self.builder)?;
                self.builder
                    .add_scene(product, scene.compiled_candidate(), resolver)?;
                self.outputs.insert(product, Output::Scene);
            }
        }
        Ok(())
    }

    fn needs(&self, product: AssetProductRef, output: Output) -> Result<bool, ProjectCookError> {
        match self.outputs.get(&product) {
            None => Ok(true),
            Some(previous) if *previous == output => Ok(false),
            Some(_) => Err(ProjectCookError::Invalid(format!(
                "cooked Product {product} changed its selected resource kind"
            ))),
        }
    }
}

fn reserve_output(builder: &mut CookedContentBuilder) -> Result<(), ProjectCookError> {
    builder.reserve_consumer_metadata(metadata_bytes::<(AssetProductRef, Output)>(0)?)?;
    Ok(())
}

/// Cooks the selected source project without reconstructing its identities or recompiling Scenes.
///
/// Stock Images preserve their admitted pixels, color space and extent through the stock Runtime
/// codec. Device-specific image transforms remain explicit separate artifact contributions.
/// Complete source snapshots are borrowed sequentially and released by the original loader.
pub fn prepare_project_cooked_content(
    request: ProjectCookRequest<'_>,
    mut contribute: impl FnMut(&mut ProjectCookContext<'_>) -> Result<(), ProjectCookError>,
) -> Result<PreparedProjectCook, ProjectCookError> {
    catch_unwind(AssertUnwindSafe(|| prepare(request, &mut contribute)))
        .unwrap_or(Err(ProjectCookError::Panicked))
}

fn prepare(
    request: ProjectCookRequest<'_>,
    contribute: &mut impl FnMut(&mut ProjectCookContext<'_>) -> Result<(), ProjectCookError>,
) -> Result<PreparedProjectCook, ProjectCookError> {
    let plan = resolve_product_recipe(request.candidate, request.recipe.clone())
        .map_err(|error| ProjectCookError::Invalid(format!("project composition: {error:?}")))?;
    let loader = ProjectContentLoader::with_limits(
        request.root.share(),
        request.recipe.source_content_limits(),
    )
    .map_err(ProjectCookError::Source)?;
    #[cfg(test)]
    tests::observe_loader_budget(&loader);
    let mut builder = None;
    let mut outputs = BTreeMap::new();
    let mut built_sources = BTreeMap::new();
    let mut observations = Observations::default();
    let scene_product = SelectedSceneProduct::Required;
    let receipt = loader
        .load_delivery_with_snapshots(
            request.candidate,
            &plan,
            scene_product,
            |snapshot, phase| {
                if builder.is_none() {
                    let startup = snapshot.selected_scene_product().ok_or_else(|| {
                        ProjectCookError::Invalid(
                            "cooked startup requires authored stable metadata".into(),
                        )
                    })?;
                    let mut configuration =
                        request.candidate.runtime_configuration().to_configuration();
                    configuration.input.action_map =
                        snapshot.input_action_map().map(|input| input.product());
                    builder = Some(CookedContentBuilder::new(
                        CookedContentRequest {
                            target: request.target.clone(),
                            configuration,
                            startup_scene: startup,
                            scenes: vec![startup],
                            addressable_roots: Vec::new(),
                        },
                        request.recipe.clone(),
                        request.limits,
                    )?);
                }
                if phase == DeliverySnapshotPhase::Startup {
                    return Ok(());
                }
                let mut context = ProjectCookContext {
                    snapshot,
                    root: request.root,
                    store: request.store,
                    implementation: request.implementation,
                    builder: builder.as_mut().expect("startup initialized the assembly"),
                    outputs: &mut outputs,
                    built_sources: &mut built_sources,
                    observations: &mut observations,
                };
                context.stock_assets()?;
                contribute(&mut context)?;
                context.scenes()
            },
        )
        .map_err(|error| match error {
            DeliveryVisitError::Content(error) => ProjectCookError::Source(error),
            DeliveryVisitError::Visit(error) => error,
        })?;
    let mut builder = builder.ok_or_else(|| {
        ProjectCookError::Invalid("source delivery produced no startup Scene".into())
    })?;
    for member in receipt.members() {
        observations.raw(
            &mut builder,
            member.path().as_str(),
            Some(ArtifactContentDigest::from_content(member.digest())),
        )?;
    }
    let observations = observations.finish(&mut builder)?;
    drop((outputs, built_sources));
    let content = builder.finish()?;
    if content
        .base()
        .plan()
        .schema_validation()
        .composition_fingerprint()
        != receipt.schema_fingerprint()
    {
        return Err(ProjectCookError::Invalid(
            "cooked content Schema differs from its original source delivery".into(),
        ));
    }
    let selection = ProjectCookSourceSelection::capture(request.root, receipt, observations)?;
    Ok(PreparedProjectCook { content, selection })
}

fn resolve_image(
    resolver: AssetReferenceResolver<'_>,
    reference: &AssetRef,
) -> Result<AssetProductRef, ProjectCookError> {
    let record = resolver
        .resolve_ref(reference)
        .map_err(|error| ProjectCookError::Invalid(error.to_string()))?;
    if record.source_kind() != &AssetSourceKind::Image {
        return Err(ProjectCookError::Invalid(
            "a texture requires an Image source".into(),
        ));
    }
    match reference {
        AssetRef::Product(product) => Ok(*product),
        AssetRef::Path(_) | AssetRef::StableId(_) => {
            AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
                .map_err(|error| ProjectCookError::Invalid(error.to_string()))
        }
    }
}
