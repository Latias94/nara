//! Binds project Cook assembly to the executable's selected runtime composition.

#[cfg(test)]
mod tests;

use std::mem::size_of;

use nara_asset::{
    AssetDomain, AssetProductRef, AssetRef, AssetReferenceDemand, AssetReferenceResolver,
    RuntimeAssetCodec, artifact::VerifiedArtifactGroup, pack::CapturedPack,
};
use nara_core::ByteLimit;
use nara_input::{ActionMapAsset, ActionMapDomain, ActionMapRuntimeCodec};
use nara_project::cook::{
    CookedContentAssembly, CookedContentAssemblyLimits, CookedContentFingerprints,
};
pub use nara_project::cook::{CookedContentAssemblyError, CookedContentRequest};
use nara_render::image::{ImageAsset, ImageDomain, ImageRuntimeCodec};
use nara_render::material::{Material2dAsset, Material2dDomain, Material2dRuntimeCodec};
use nara_render::tilemap::{TileSetDescriptor, TileSetDomain, TileSetRuntimeCodec};
use nara_scene::CompiledSceneCandidate;

use crate::{
    ProductRecipe,
    project_host::{RuntimeConfigurationPlan, resolve_runtime_configuration},
    runtime_content::{
        RuntimeContentBase, RuntimeContentCatalogError, RuntimeContentDependency,
        RuntimeContentError, RuntimeContentExpectation, RuntimeContentLimits,
    },
};

/// Logical assembly bounds, in addition to each production owner's frozen limits.
///
/// Retention counts this owner's live request, exact member tables/bytes and complete retained
/// groups, then the prepared captured base and expectation. Work bounds their overlap with new
/// tables, group copies and conservative Scene/codec/catalog/deep-validation promises. Tables
/// request exact capacity; lengths describe logical ownership, not allocator capacity or RSS.
/// Borrowed source candidates and CPU inputs remain their caller's custody. Compiled recipe,
/// plugin and Schema composition retain their existing owner limits, as in Runtime admission.
/// Deep validation reserves the Runtime retained/work ceilings together before decoding; callers
/// can select smaller Runtime limits when qualifying a smaller assembly allowance.
#[derive(Debug, Clone, Copy)]
pub struct CookedContentLimits {
    pub runtime: RuntimeContentLimits,
    pub work_bytes: ByteLimit,
    pub retained_bytes: ByteLimit,
}

impl Default for CookedContentLimits {
    fn default() -> Self {
        Self {
            runtime: RuntimeContentLimits::default(),
            work_bytes: ByteLimit::new(2 * 1024 * 1024 * 1024).expect("nonzero assembly limit"),
            retained_bytes: ByteLimit::new(1024 * 1024 * 1024).expect("nonzero assembly limit"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CookedContentError {
    #[error("cooked content was rejected: {0}")]
    Runtime(#[from] RuntimeContentError),
    #[error(transparent)]
    Assembly(CookedContentAssemblyError),
}

impl From<RuntimeContentCatalogError> for CookedContentError {
    fn from(value: RuntimeContentCatalogError) -> Self {
        RuntimeContentError::Catalog(value).into()
    }
}

impl From<CookedContentAssemblyError> for CookedContentError {
    fn from(value: CookedContentAssemblyError) -> Self {
        match value {
            CookedContentAssemblyError::Catalog(error) => error.into(),
            CookedContentAssemblyError::Scene(error) => RuntimeContentError::Scene(error).into(),
            CookedContentAssemblyError::SceneReference(error) => {
                RuntimeContentError::SceneReference(error).into()
            }
            CookedContentAssemblyError::Asset(error) => RuntimeContentError::Asset(error).into(),
            CookedContentAssemblyError::InvalidCodec => RuntimeContentError::CodecMismatch.into(),
            error => Self::Assembly(error),
        }
    }
}

/// Move-only unpublished assembly using one original executable configuration plan.
///
/// Each `add_*` is transactional. `finish` consumes the builder and requires the complete
/// selected delivery closure to pass the executable's production runtime checks.
#[derive(Debug)]
pub struct CookedContentBuilder {
    assembly: CookedContentAssembly,
    recipe: ProductRecipe,
    plan: RuntimeConfigurationPlan,
    runtime: RuntimeContentLimits,
}

impl CookedContentBuilder {
    pub fn new(
        request: CookedContentRequest,
        recipe: ProductRecipe,
        limits: CookedContentLimits,
    ) -> Result<Self, CookedContentError> {
        let runtime = limits.runtime;
        let assembly = CookedContentAssembly::new(
            request,
            CookedContentAssemblyLimits {
                catalog: runtime.catalog(),
                scene: runtime.scene(),
                member_encoded_bytes: runtime.member_encoded_bytes(),
                total_encoded_bytes: runtime.total_encoded_bytes(),
                runtime_retained_bytes: runtime.retained_bytes(),
                runtime_work_bytes: runtime.work_bytes(),
                work_bytes: limits.work_bytes,
                retained_bytes: limits.retained_bytes,
            },
            size_of::<Self>() - size_of::<CookedContentAssembly>(),
        )?;
        let configuration = assembly
            .configuration()
            .resolve()
            .map_err(RuntimeContentError::Configuration)?;
        let plan = resolve_runtime_configuration(&configuration, recipe.clone())
            .map_err(RuntimeContentError::Composition)?;
        Ok(Self {
            assembly,
            recipe,
            plan,
            runtime,
        })
    }

    /// Declares an admitted Scene root during private sequential source traversal.
    pub(crate) fn declare_scene_root(
        &mut self,
        product: AssetProductRef,
    ) -> Result<(), CookedContentError> {
        Ok(self.assembly.declare_scene_root(product)?)
    }

    /// Borrows the actual dependencies in assembly order, which is stable until `finish`.
    pub(crate) fn pending_dependencies(&self, index: usize) -> Option<&[RuntimeContentDependency]> {
        self.assembly.pending_dependencies(index)
    }

    pub fn add_scene(
        &mut self,
        product: AssetProductRef,
        compiled: &CompiledSceneCandidate,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> Result<(), CookedContentError> {
        self.add_scene_with_component_omissions(product, compiled, asset_resolution, &[])
    }

    pub(crate) fn add_scene_with_component_omissions(
        &mut self,
        product: AssetProductRef,
        compiled: &CompiledSceneCandidate,
        asset_resolution: AssetReferenceResolver<'_>,
        omitted: &[nara_reflect::ComponentTypeId],
    ) -> Result<(), CookedContentError> {
        Ok(self.assembly.add_scene(
            product,
            compiled,
            self.plan.schema_validation().bindings(),
            asset_resolution,
            omitted,
        )?)
    }

    /// Retains the original complete group through capture and deep validation during `finish`.
    /// Member names from the generated store never become Runtime member paths.
    pub fn add_group(&mut self, group: &VerifiedArtifactGroup) -> Result<(), CookedContentError> {
        let codecs = self.plan.runtime_codecs();
        self.assembly.add_group(group, |member| {
            codecs
                .validate_member(member)
                .map_err(CookedContentError::from)
        })
    }

    pub fn add_input(&mut self, input: &ActionMapAsset) -> Result<(), CookedContentError> {
        self.add_cpu::<ActionMapDomain, ActionMapRuntimeCodec>(input.product(), input, &[])
    }

    /// Adds the Image's exact Product through the selected production Runtime codec.
    /// Anonymous Images cannot become cooked members.
    pub fn add_image(&mut self, image: &ImageAsset) -> Result<(), CookedContentError> {
        let product = image
            .product()
            .ok_or(CookedContentAssemblyError::IncompleteProductReference)?;
        self.add_cpu::<ImageDomain, ImageRuntimeCodec>(product, image, &[])
    }

    pub fn add_tileset(&mut self, tileset: &TileSetDescriptor) -> Result<(), CookedContentError> {
        self.add_cpu::<TileSetDomain, TileSetRuntimeCodec>(
            tileset.product(),
            tileset,
            &[(tileset.image_product(), AssetReferenceDemand::Immediate)],
        )
    }

    /// Adds one validated material with canonical complete texture Product references.
    ///
    /// The caller resolves source references through its admitted catalogue before this call.
    /// Texture products must also belong to the finished content; this operation performs no
    /// source loading, publication or native preparation.
    pub fn add_material(&mut self, material: &Material2dAsset) -> Result<(), CookedContentError> {
        let mut dependencies = [(material.product(), AssetReferenceDemand::Immediate); 8];
        let mut count = 0;
        for reference in material.textures().values() {
            let AssetRef::Product(product) = reference else {
                return Err(CookedContentAssemblyError::IncompleteProductReference.into());
            };
            if dependencies[..count]
                .iter()
                .any(|(previous, _)| previous == product)
            {
                continue;
            }
            let Some(slot) = dependencies.get_mut(count) else {
                return Err(CookedContentAssemblyError::Overflow.into());
            };
            *slot = (*product, AssetReferenceDemand::Immediate);
            count += 1;
        }
        self.add_cpu::<Material2dDomain, Material2dRuntimeCodec>(
            material.product(),
            material,
            &dependencies[..count],
        )
    }

    fn add_cpu<D, C>(
        &mut self,
        product: AssetProductRef,
        value: &D::Product,
        dependencies: &[(AssetProductRef, AssetReferenceDemand)],
    ) -> Result<(), CookedContentError>
    where
        D: AssetDomain,
        C: RuntimeAssetCodec<D::Product>,
    {
        let codecs = self.plan.runtime_codecs();
        self.assembly
            .add_asset::<D, C, _>(product, value, dependencies, |member| {
                codecs
                    .selected_limits::<D, C>(member)
                    .map_err(CookedContentError::from)
            })
    }

    pub(crate) fn reserve_consumer_metadata(
        &mut self,
        bytes: usize,
    ) -> Result<(), CookedContentError> {
        Ok(self.assembly.reserve_consumer_metadata(bytes)?)
    }

    pub(crate) fn admit_consumer_work(&self, bytes: usize) -> Result<(), CookedContentError> {
        Ok(self.assembly.admit_consumer_work(bytes)?)
    }

    pub(crate) fn release_consumer_metadata(&mut self, bytes: usize) {
        self.assembly
            .release_consumer_metadata(bytes)
            .expect("caller releases reserved Cook metadata");
    }

    pub(crate) fn reserve_consumer_work(&mut self, bytes: usize) -> Result<(), CookedContentError> {
        Ok(self.assembly.reserve_consumer_work(bytes)?)
    }

    pub(crate) fn release_consumer_work(&mut self, bytes: usize) {
        self.assembly
            .release_consumer_work(bytes)
            .expect("caller releases reserved Cook work");
    }

    /// Captures exact member bytes and deep-validates the complete selected delivery closure.
    pub fn finish(self) -> Result<PreparedRuntimeContent, CookedContentError> {
        let Self {
            assembly,
            recipe,
            plan,
            runtime,
        } = self;
        let fingerprints = CookedContentFingerprints {
            plugin_plan: plan.plugin_plan().fingerprint().as_bytes(),
            schema: plan
                .schema_validation()
                .composition_fingerprint()
                .as_bytes(),
            runtime_codecs: plan.runtime_codecs_fingerprint().as_bytes(),
        };
        drop(plan);
        let ((base, pack), expectation, retained_bytes) = assembly.finish(
            fingerprints,
            size_of::<PreparedRuntimeContent>(),
            |catalog, members, expected, retained| {
                let (base, pack, retained_bytes) = RuntimeContentBase::capture_members(
                    catalog,
                    members,
                    expected,
                    recipe,
                    runtime.with_retained_bytes(retained),
                )?;
                Ok::<_, CookedContentError>(((base, pack), retained_bytes))
            },
            |(base, _)| Ok(base.validate_deep()?.members()),
        )?;
        Ok(PreparedRuntimeContent {
            base,
            pack,
            expectation,
            retained_bytes,
        })
    }
}

/// Complete captured content with independent producer expectation and no decoded residency.
#[derive(Debug)]
pub struct PreparedRuntimeContent {
    base: RuntimeContentBase,
    pack: CapturedPack,
    expectation: RuntimeContentExpectation,
    retained_bytes: usize,
}

impl PreparedRuntimeContent {
    #[must_use]
    pub fn base(&self) -> &RuntimeContentBase {
        &self.base
    }

    /// The original catalog and ordered member bytes shared with the admitted Runtime base.
    #[must_use]
    pub fn pack(&self) -> &CapturedPack {
        &self.pack
    }

    #[must_use]
    pub fn expectation(&self) -> &RuntimeContentExpectation {
        &self.expectation
    }

    /// The captured base and this wrapper's logical ownership. Later independent acquisitions
    /// retain their own Runtime charges; they do not change this preparation receipt.
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    #[must_use]
    pub fn into_base(self) -> RuntimeContentBase {
        self.base
    }

    pub(crate) fn into_pack(self) -> (CapturedPack, RuntimeContentExpectation) {
        (self.pack, self.expectation)
    }
}
