use std::{
    any::TypeId,
    collections::{BTreeMap, btree_map::Entry},
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
    image::ImageAsset,
    input::ActionMapAsset,
    material::{Material2dAsset, Material2dDomain},
    project_content::{DeliverySnapshotPhase, DeliveryVisitError, SelectedSceneProduct},
    project_host::{
        ProjectContentLoader, ProjectContentMemberKind, ProjectContentSnapshot,
        ProjectTileSetContent, resolve_product_recipe,
    },
    project_workflow::cooked_content::{CookedContentBuilder, CookedContentRequest},
    runtime_content::RuntimeContentDependencyRole,
    scene::CompiledSceneCandidate,
    tilemap::TileSetDescriptor,
};

use super::{
    PreparedProjectCook, ProjectCookError, ProjectCookRequest, ProjectCookSourceSelection,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StockKind {
    Scene,
    Image,
    Input,
    Material,
    TileSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StockState {
    Available,
    Demanded,
    Cooked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Output {
    /// A mandatory target whose provider has yet to be seen.
    Demanded,
    Stock(StockKind, StockState),
    Group(ArtifactContentDigest),
}

#[derive(Clone, Copy)]
enum StockAsset<'a> {
    Scene(&'a CompiledSceneCandidate),
    Image(&'a ImageAsset),
    Input(&'a ActionMapAsset),
    Material(&'a Material2dAsset),
    TileSet(&'a ProjectTileSetContent),
}

impl StockAsset<'_> {
    fn kind(self) -> StockKind {
        match self {
            Self::Scene(_) => StockKind::Scene,
            Self::Image(_) => StockKind::Image,
            Self::Input(_) => StockKind::Input,
            Self::Material(_) => StockKind::Material,
            Self::TileSet(_) => StockKind::TileSet,
        }
    }
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

impl<'a> ProjectCookContext<'a> {
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
            match self.outputs.get(product) {
                None | Some(Output::Demanded) => {}
                Some(previous) if *previous == output => present += 1,
                Some(_) => {
                    return Err(ProjectCookError::Invalid(format!(
                        "cooked Product {product} has conflicting output"
                    )));
                }
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
            if !self.outputs.contains_key(product) {
                reserve_output(self.builder)?;
            }
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

    fn select_stock(
        &mut self,
        inspected_members: &mut usize,
        prepare: impl FnOnce(&mut Self) -> Result<(), ProjectCookError>,
    ) -> Result<(), ProjectCookError> {
        let snapshot = self.snapshot;
        let materials = snapshot.domain_assets().collection::<Material2dDomain>();
        let count = snapshot
            .images()
            .len()
            .checked_add(snapshot.tilesets().len())
            .and_then(|count| count.checked_add(snapshot.scene_dependencies().len()))
            .and_then(|count| count.checked_add(materials.map_or(0, |values| values.iter().len())))
            .ok_or_else(overflow)?;
        let bytes = count
            .checked_mul(size_of::<(AssetProductRef, StockAsset<'_>)>())
            .ok_or_else(overflow)?;
        self.builder.reserve_consumer_work(bytes)?;
        let mut available = Vec::with_capacity(count);
        for image in snapshot.images() {
            let product = image.image().product().ok_or_else(|| {
                ProjectCookError::Invalid(
                    "a cooked Image requires persistent Product identity".into(),
                )
            })?;
            available.push((product, StockAsset::Image(image.image())));
        }
        for tileset in snapshot.tilesets() {
            let product = AssetProductRef::new(tileset.stable_id(), ImportedProductId::PRIMARY)
                .map_err(|error| ProjectCookError::Invalid(error.to_string()))?;
            available.push((product, StockAsset::TileSet(tileset)));
        }
        if let Some(materials) = materials {
            for material in materials.iter() {
                available.push((material.product(), StockAsset::Material(material)));
            }
        }
        let database = snapshot.asset_database();
        for scene in snapshot.scene_dependencies() {
            let record = database.record_for_path(scene.path()).ok_or_else(|| {
                ProjectCookError::Invalid(format!(
                    "Scene {} lost its admitted identity",
                    scene.path()
                ))
            })?;
            let product = AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
                .map_err(|error| ProjectCookError::Invalid(error.to_string()))?;
            available.push((product, StockAsset::Scene(scene.compiled_candidate())));
        }
        for &(product, asset) in &available {
            self.register_stock(product, asset.kind())?;
        }
        available.sort_unstable_by_key(|(product, _)| *product);
        let result =
            prepare(self).and_then(|()| self.cook_demanded_stock(inspected_members, &available));
        drop(available);
        self.builder.release_consumer_work(bytes);
        result
    }

    fn cook_demanded_stock(
        &mut self,
        inspected_members: &mut usize,
        available: &[(AssetProductRef, StockAsset<'_>)],
    ) -> Result<(), ProjectCookError> {
        for &(product, asset) in available {
            if matches!(
                self.outputs.get(&product),
                Some(Output::Stock(_, StockState::Demanded))
            ) {
                self.cook_stock(product, asset)?;
            }
        }

        while let Some(count) = self
            .builder
            .pending_dependencies(*inspected_members)
            .map(|dependencies| dependencies.len())
        {
            for index in 0..count {
                let dependency = self
                    .builder
                    .pending_dependencies(*inspected_members)
                    .expect("pending members retain their assembly index")[index];
                let product = dependency.product();
                if dependency.role() == RuntimeContentDependencyRole::Soft
                    || matches!(
                        self.outputs.get(&product),
                        Some(Output::Stock(_, StockState::Cooked) | Output::Group(_))
                    )
                {
                    continue;
                }
                if let Ok(slot) = available.binary_search_by_key(&product, |(product, _)| *product)
                {
                    self.cook_stock(product, available[slot].1)?;
                } else {
                    match self.outputs.entry(product) {
                        Entry::Vacant(entry) => {
                            reserve_output(self.builder)?;
                            entry.insert(Output::Demanded);
                        }
                        Entry::Occupied(mut entry) => {
                            if let Output::Stock(_, state) = entry.get_mut() {
                                *state = StockState::Demanded;
                            }
                        }
                    }
                }
            }
            *inspected_members += 1;
        }
        Ok(())
    }

    fn input(&mut self) -> Result<(), ProjectCookError> {
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
            self.cook_stock(input.product(), StockAsset::Input(input.asset()))?;
        }
        Ok(())
    }

    fn scene_root(&mut self) -> Result<(), ProjectCookError> {
        let selected = self.snapshot.selected_scene_product().ok_or_else(|| {
            ProjectCookError::Invalid("cooked Scenes require authored stable metadata".into())
        })?;
        self.builder.declare_scene_root(selected)?;
        self.cook_stock(
            selected,
            StockAsset::Scene(self.snapshot.compiled_candidate()),
        )
    }

    fn cook_stock(
        &mut self,
        product: AssetProductRef,
        asset: StockAsset<'_>,
    ) -> Result<(), ProjectCookError> {
        let kind = asset.kind();
        if !self.register_stock(product, kind)? {
            return Ok(());
        }
        let resolver = self.snapshot.asset_resolution();
        match asset {
            StockAsset::Scene(scene) => self.builder.add_scene(product, scene, resolver)?,
            StockAsset::Image(image) => self.builder.add_image(image)?,
            StockAsset::Input(input) => self.builder.add_input(input)?,
            StockAsset::Material(material) => {
                let material = material
                    .resolve_texture_references(|reference| resolve_image(resolver, reference))?;
                self.builder.add_material(&material)?;
            }
            StockAsset::TileSet(tileset) => {
                let source = tileset.source();
                let image = resolve_image(resolver, &source.image)?;
                let descriptor = TileSetDescriptor::new(product, image, source.atlas)
                    .map(|value| value.with_sampler(source.sampler))
                    .map(|value| value.with_alpha_mode(source.alpha_mode))
                    .and_then(|value| value.with_tint(source.tint))
                    .map_err(|error| ProjectCookError::Invalid(error.to_string()))?;
                self.builder.add_tileset(&descriptor)?;
            }
        }
        self.outputs
            .insert(product, Output::Stock(kind, StockState::Cooked));
        Ok(())
    }

    fn register_stock(
        &mut self,
        product: AssetProductRef,
        kind: StockKind,
    ) -> Result<bool, ProjectCookError> {
        match self.outputs.entry(product) {
            Entry::Vacant(entry) => {
                reserve_output(self.builder)?;
                entry.insert(Output::Stock(kind, StockState::Available));
                Ok(true)
            }
            Entry::Occupied(mut entry) => match *entry.get() {
                Output::Demanded => {
                    entry.insert(Output::Stock(kind, StockState::Demanded));
                    Ok(true)
                }
                Output::Stock(previous, state) if previous == kind => {
                    Ok(state != StockState::Cooked)
                }
                _ => Err(ProjectCookError::Invalid(format!(
                    "cooked Product {product} changed its selected resource kind"
                ))),
            },
        }
    }
}

fn reserve_output(builder: &mut CookedContentBuilder) -> Result<(), ProjectCookError> {
    builder.reserve_consumer_metadata(metadata_bytes::<(AssetProductRef, Output)>(0)?)?;
    Ok(())
}

/// Cooks the selected source project without reconstructing its identities or recompiling Scenes.
///
/// Stock payloads are selected through delivery roots and the assembled Runtime dependencies.
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
    let mut inspected_members = 0;
    let scene_product = SelectedSceneProduct::Required;
    let delivery_error = |error| match error {
        DeliveryVisitError::Content(error) => ProjectCookError::Source(error),
        DeliveryVisitError::Visit(error) => error,
    };
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
                context.input()?;
                context.select_stock(&mut inspected_members, |context| {
                    contribute(context)?;
                    context.scene_root()
                })
            },
        )
        .map_err(delivery_error)?;
    let mut builder = builder.ok_or_else(|| {
        ProjectCookError::Invalid("source delivery produced no startup Scene".into())
    })?;
    let needs_replay = |outputs: &BTreeMap<AssetProductRef, Output>| {
        outputs
            .values()
            .any(|output| matches!(output, Output::Stock(_, StockState::Demanded)))
    };
    let receipt = if needs_replay(&outputs) {
        let expected_delivery = receipt.delivery_digest();
        let expected_startup = receipt.startup_content().content_closure().stamp();
        drop(receipt);
        let replay = loader
            .load_delivery_with_snapshots(
                request.candidate,
                &plan,
                scene_product,
                |snapshot, phase| {
                    if phase != DeliverySnapshotPhase::Complete || !needs_replay(&outputs) {
                        return Ok(());
                    }
                    ProjectCookContext {
                        snapshot,
                        root: request.root,
                        store: request.store,
                        implementation: request.implementation,
                        builder: &mut builder,
                        outputs: &mut outputs,
                        built_sources: &mut built_sources,
                        observations: &mut observations,
                    }
                    .select_stock(&mut inspected_members, |_| Ok(()))
                },
            )
            .map_err(delivery_error)?;
        if replay.delivery_digest() != expected_delivery
            || replay.startup_content().content_closure().stamp() != expected_startup
        {
            return Err(ProjectCookError::Invalid(
                "source delivery changed while selecting cooked dependencies".into(),
            ));
        }
        replay
    } else {
        receipt
    };
    let output_bytes = metadata_bytes::<(AssetProductRef, Output)>(0)?
        .checked_mul(outputs.len())
        .ok_or_else(overflow)?;
    let source_index_bytes = metadata_bytes::<(StableAssetId, TypeId)>(0)?
        .checked_mul(built_sources.len())
        .ok_or_else(overflow)?;
    drop((outputs, built_sources));
    builder.release_consumer_metadata(
        output_bytes
            .checked_add(source_index_bytes)
            .ok_or_else(overflow)?,
    );
    for member in receipt.members() {
        observations.raw(
            &mut builder,
            member.path().as_str(),
            Some(ArtifactContentDigest::from_content(member.digest())),
        )?;
    }
    let observations = observations.finish(&mut builder)?;
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
