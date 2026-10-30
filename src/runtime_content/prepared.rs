//! Actual-demand Scene preparation from one original immutable base.

use std::{mem::size_of, sync::Arc};

use nara_asset::{AssetProductRef, AssetRef};
use nara_image::{ImageAsset, ImageDomain};
use nara_tilemap::{
    TileSetDescriptor, TileSetDomain, tilemap_membership_work_bytes,
    validate_persisted_tilemap_membership,
};

use super::{
    DecodeLease, RuntimeCodecRole, RuntimeContentAsset, RuntimeContentBase,
    RuntimeContentBudgetKind, RuntimeContentError, RuntimeContentLease, RuntimeContentMemberKind,
    RuntimeContentScene, codecs::DecodedAsset, native::validate_tileset_image,
    traversal::MemberTraversal,
};
use crate::project_content::ProjectDomainAssets;

#[cfg(test)]
mod tests;

pub(crate) struct ProcessedSceneContent {
    pub(crate) scene: RuntimeContentScene,
    pub(crate) dependencies: Vec<RuntimeContentScene>,
    pub(crate) domains: ProjectDomainAssets,
    pub(crate) images: Vec<RuntimeContentAsset<ImageAsset>>,
    pub(crate) tilesets: Vec<RuntimeContentAsset<TileSetDescriptor>>,
    // Drop the actual closure and its indices before releasing their metadata reservation.
    pub(crate) _lease: RuntimeContentLease,
}

struct DecodedClosure {
    scenes: Vec<(usize, RuntimeContentScene)>,
    assets: Vec<DecodedAsset>,
    _work: DecodeLease,
}

impl ProcessedSceneContent {
    pub(crate) fn base(&self) -> &RuntimeContentBase {
        self.scene.base()
    }

    pub(crate) fn product(&self) -> AssetProductRef {
        self.scene.product()
    }
}

impl RuntimeContentBase {
    fn decode_current_closure(
        &self,
        product: AssetProductRef,
    ) -> Result<DecodedClosure, RuntimeContentError> {
        let count = self.catalog().members().len();
        let scene_capacity = self
            .catalog()
            .members()
            .iter()
            .filter(|member| matches!(member.kind(), RuntimeContentMemberKind::Scene { .. }))
            .count();
        let asset_capacity = count - scene_capacity;
        let output_bytes = scene_capacity
            .checked_mul(size_of::<(usize, RuntimeContentScene)>())
            .and_then(|bytes| {
                bytes.checked_add(asset_capacity.checked_mul(size_of::<DecodedAsset>())?)
            })
            .ok_or(RuntimeContentError::BudgetOverflow {
                kind: RuntimeContentBudgetKind::WorkBytes,
            })?;
        // Scratch is bounded before allocation and overlaps every live decoder promise. Actual
        // decoded values keep their independent retained leases throughout discovery and assembly.
        let mut pending = MemberTraversal::new(self)?;
        let output_work = self.inner.budget.reserve(0, output_bytes)?;
        let root = self.member_index(product)?;
        let mut scenes = Vec::with_capacity(scene_capacity);
        let mut assets = Vec::with_capacity(asset_capacity);
        pending.enqueue(self, root)?;
        while let Some(index) = pending.next() {
            let member = &self.catalog().members()[index];
            match member.kind() {
                RuntimeContentMemberKind::Scene { .. } => {
                    let selected = if index == root {
                        product
                    } else {
                        *member
                            .products()
                            .first()
                            .ok_or(RuntimeContentError::ProductIdentityMismatch)?
                    };
                    scenes.push((index, self.acquire_scene_member(selected)?));
                }
                RuntimeContentMemberKind::Asset { .. } => {
                    assets.push(self.plan().runtime_codecs().acquire_member(self, index)?);
                }
            }
            // Acquisition has now admitted the exact actual dependency set. Unverified catalogue
            // edges never enter the pending set or cause a decoder/native preparation to run.
            pending.follow_admitted(self, index, false)?;
        }
        scenes.sort_unstable_by_key(|(index, _)| *index);
        assets.sort_unstable_by_key(|asset| asset.member);
        drop(pending);
        Ok(DecodedClosure {
            scenes,
            assets,
            _work: output_work,
        })
    }

    /// Prepares only verified Immediate data/resources without mutating a receiving World.
    pub(crate) fn prepare_scene_content(
        &self,
        product: AssetProductRef,
    ) -> Result<Arc<ProcessedSceneContent>, RuntimeContentError> {
        if !self.catalog().scenes().contains(&product) {
            return Err(RuntimeContentError::UndeclaredScene);
        }
        let decoded = self.decode_current_closure(product)?;
        let root = self.member_index(product)?;
        let scene = decoded
            .scenes
            .iter()
            .find(|(index, _)| *index == root)
            .ok_or(RuntimeContentError::NotScene)?
            .1
            .clone();
        let scene_count = decoded.scenes.len() - 1;
        let codecs = self.plan().runtime_codecs();
        let image_count = decoded
            .assets
            .iter()
            .filter(|asset| {
                codecs.role(&self.catalog().members()[asset.member])
                    == Some(RuntimeCodecRole::Image)
            })
            .count();
        let tileset_count = decoded
            .assets
            .iter()
            .filter(|asset| {
                codecs.role(&self.catalog().members()[asset.member])
                    == Some(RuntimeCodecRole::TileSet)
            })
            .count();
        let kind = RuntimeContentBudgetKind::RetainedBytes;
        let bytes = image_count
            .checked_mul(size_of::<RuntimeContentAsset<ImageAsset>>())
            .and_then(|bytes| {
                bytes.checked_add(
                    tileset_count
                        .checked_mul(size_of::<RuntimeContentAsset<TileSetDescriptor>>())?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(scene_count.checked_mul(size_of::<RuntimeContentScene>())?)
            })
            .and_then(|bytes| bytes.checked_add(size_of::<ProcessedSceneContent>()))
            .and_then(|bytes| bytes.checked_add(crate::scene_retention::ARC_CONTROL_BLOCK_BYTES))
            .ok_or(RuntimeContentError::BudgetOverflow { kind })?;
        let lease = self.reserve_retained(bytes)?;
        let mut dependencies = Vec::with_capacity(scene_count);
        for (index, scene) in &decoded.scenes {
            if *index != root {
                dependencies.push(scene.clone());
            }
        }
        let domains = codecs.prepare_collections(self, &decoded.assets)?;
        let mut images = Vec::with_capacity(image_count);
        if let Some(collection) = domains.collection::<ImageDomain>() {
            images.extend(
                collection
                    .runtime_acquisitions()
                    .iter()
                    .filter(|asset| {
                        self.member(asset.reference()).is_some_and(|member| {
                            codecs.role(member) == Some(RuntimeCodecRole::Image)
                        })
                    })
                    .cloned(),
            );
        }
        let mut tilesets = Vec::with_capacity(tileset_count);
        if let Some(collection) = domains.collection::<TileSetDomain>() {
            tilesets.extend(
                collection
                    .runtime_acquisitions()
                    .iter()
                    .filter(|asset| {
                        self.member(asset.reference()).is_some_and(|member| {
                            codecs.role(member) == Some(RuntimeCodecRole::TileSet)
                        })
                    })
                    .cloned(),
            );
        }
        // Typed collections and native preparations share these acquisitions. Only the discovery
        // wrappers are temporary; no codec runs again when the final collections are assembled.
        drop(decoded);
        for image in &images {
            self.image_fact(image.reference(), image.product())?;
        }
        for tileset in &tilesets {
            let descriptor = tileset.product();
            self.validate_tileset_member(tileset.reference(), descriptor)?;
            let image = images
                .iter()
                .find(|image| image.product().product() == Some(descriptor.image_product()))
                .map(|image| image.product().extent());
            validate_tileset_image(descriptor, image)?;
        }
        let tilemap = nara_reflect::ComponentTypeId::new("nara.tilemap.Tilemap");
        for entity in std::iter::once(&scene)
            .chain(dependencies.iter())
            .flat_map(|scene| &scene.candidate().document().entities)
        {
            let Some(record) = entity.components.get(&tilemap) else {
                continue;
            };
            let scratch = tilemap_membership_work_bytes(&record.value).ok_or(
                RuntimeContentError::BudgetOverflow {
                    kind: RuntimeContentBudgetKind::WorkBytes,
                },
            )?;
            let _work = self.inner.budget.reserve(0, scratch)?;
            validate_persisted_tilemap_membership(&record.value, |reference| {
                let AssetRef::Product(product) = reference else {
                    return None;
                };
                tilesets
                    .iter()
                    .find(|asset| asset.product().product() == *product)
                    .map(|asset| asset.product().atlas())
            })
            .map_err(RuntimeContentError::Tilemap)?;
        }
        Ok(Arc::new(ProcessedSceneContent {
            scene,
            dependencies,
            domains,
            images,
            tilesets,
            _lease: lease,
        }))
    }
}
