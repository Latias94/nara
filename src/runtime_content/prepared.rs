//! A complete processed Scene closure prepared from one original immutable base.

use std::{mem::size_of, sync::Arc};

use nara_asset::{AssetProductRef, AssetRef};
use nara_image::{ImageAsset, ImageDomain};
use nara_tilemap::{
    TileSetDescriptor, TileSetDomain, tilemap_membership_work_bytes,
    validate_persisted_tilemap_membership,
};

use super::{
    RuntimeCodecRole, RuntimeContentAsset, RuntimeContentBase, RuntimeContentBudgetKind,
    RuntimeContentDependencyRole, RuntimeContentError, RuntimeContentLease,
    RuntimeContentMemberKind, RuntimeContentScene,
};
use crate::project_content::ProjectDomainAssets;

pub(crate) struct ProcessedSceneContent {
    pub(crate) scene: RuntimeContentScene,
    pub(crate) dependencies: Vec<RuntimeContentScene>,
    pub(crate) domains: ProjectDomainAssets,
    pub(crate) images: Vec<RuntimeContentAsset<ImageAsset>>,
    pub(crate) tilesets: Vec<RuntimeContentAsset<TileSetDescriptor>>,
    // Drop the actual closure and its indices before releasing their metadata reservation.
    pub(crate) _lease: RuntimeContentLease,
}

struct RequiredMembers {
    indices: Vec<usize>,
    _lease: RuntimeContentLease,
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
    /// Prepares one complete Required resource closure without mutating a receiving World.
    pub(crate) fn prepare_scene_content(
        &self,
        product: AssetProductRef,
    ) -> Result<Arc<ProcessedSceneContent>, RuntimeContentError> {
        let count = self.catalog().members().len();
        // Graph scratch belongs to preparation, not the published Scene's retained charge.
        // Each member is visited at most once, including cyclic declarations.
        let graph_bytes = count
            .checked_mul(size_of::<bool>() + 2 * size_of::<usize>())
            .ok_or(RuntimeContentError::BudgetOverflow {
                kind: RuntimeContentBudgetKind::WorkBytes,
            })?;
        let scene = self.acquire_scene(product)?;
        let graph_work = self.inner.budget.reserve(0, graph_bytes)?;
        let root = self.member_index(product)?;
        let mut seen = vec![false; count];
        let mut pending = Vec::with_capacity(count);
        let mut members = Vec::with_capacity(count);
        seen[root] = true;
        pending.push(root);
        while let Some(index) = pending.pop() {
            let member = &self.catalog().members()[index];
            if index != root {
                members.push(index);
            }
            for dependency in member.dependencies() {
                if dependency.role() != RuntimeContentDependencyRole::Required {
                    continue;
                }
                let next = self.member_index(dependency.product())?;
                if !seen[next] {
                    seen[next] = true;
                    pending.push(next);
                }
            }
        }
        members.sort_unstable();
        // The finite membership graph is complete. Keep its index under retained admission
        // while sequential Scene/asset decoders reuse the released work allowance.
        let index_bytes =
            count
                .checked_mul(size_of::<usize>())
                .ok_or(RuntimeContentError::BudgetOverflow {
                    kind: RuntimeContentBudgetKind::RetainedBytes,
                })?;
        let members = RequiredMembers {
            _lease: self.reserve_retained(index_bytes)?,
            indices: members,
        };
        drop((pending, seen, graph_work));
        let members = &members.indices;
        let scene_count = members
            .iter()
            .filter(|index| {
                matches!(
                    self.catalog().members()[**index].kind(),
                    RuntimeContentMemberKind::Scene { .. }
                )
            })
            .count();
        let codecs = self.plan().runtime_codecs();
        let image_count = members
            .iter()
            .filter(|index| {
                codecs.role(&self.catalog().members()[**index]) == Some(RuntimeCodecRole::Image)
            })
            .count();
        let tileset_count = members
            .iter()
            .filter(|index| {
                codecs.role(&self.catalog().members()[**index]) == Some(RuntimeCodecRole::TileSet)
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
        for index in members {
            let member = &self.catalog().members()[*index];
            if matches!(member.kind(), RuntimeContentMemberKind::Scene { .. }) {
                let product = *member
                    .products()
                    .first()
                    .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
                dependencies.push(self.acquire_scene_member(product)?);
            }
        }
        let domains = codecs.prepare_collections(self, members)?;
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
        for image in &images {
            let product = image
                .product()
                .product()
                .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
            if self
                .member(image.reference())
                .map(|member| member.products())
                != Some(&[product][..])
            {
                return Err(RuntimeContentError::ProductIdentityMismatch);
            }
        }
        for tileset in &tilesets {
            let descriptor = tileset.product();
            let member = self
                .member(tileset.reference())
                .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
            if member.products() != &[descriptor.product()] {
                return Err(RuntimeContentError::ProductIdentityMismatch);
            }
            self.validate_required_dependency(member, descriptor.image_product())?;
            let image = images
                .iter()
                .find(|image| image.product().product() == Some(descriptor.image_product()))
                .ok_or(RuntimeContentError::NativeResourceUnavailable)?;
            descriptor
                .validate_image_extent(image.product().extent())
                .map_err(RuntimeContentError::TileSet)?;
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
