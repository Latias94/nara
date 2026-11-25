//! Actual-demand Scene preparation from one original immutable base.

use std::{mem::size_of, sync::Arc};

use nara_asset::AssetProductRef;
#[cfg(feature = "runtime-2d")]
use nara_asset::AssetRef;
#[cfg(feature = "runtime-2d")]
use nara_render::image::{ImageAsset, ImageDomain};
#[cfg(feature = "runtime-2d")]
use nara_render::material::{Material2dAsset, Material2dDomain};
#[cfg(feature = "runtime-2d")]
use nara_render::tilemap::{
    TileSetDescriptor, TileSetDomain, tilemap_membership_work_bytes,
    validate_persisted_tilemap_membership,
};

use super::{
    DecodeLease, RuntimeContentBase, RuntimeContentBudgetKind, RuntimeContentDependencyRole,
    RuntimeContentError, RuntimeContentLease, RuntimeContentMemberKind, RuntimeContentScene,
    codecs::DecodedAsset, traversal::MemberTraversal,
};
#[cfg(feature = "runtime-2d")]
use super::{
    RuntimeCodecRole, RuntimeContentAsset, materials::SceneMaterialBinding,
    native::validate_tileset_image,
};
use crate::project_assets::ProjectDomainAssets;

#[cfg(test)]
mod tests;

pub(crate) struct ProcessedSceneContent {
    pub(crate) scene: RuntimeContentScene,
    pub(crate) dependencies: Vec<RuntimeContentScene>,
    pub(crate) deferred_scene_products: Vec<AssetProductRef>,
    pub(crate) domains: ProjectDomainAssets,
    #[cfg(feature = "runtime-2d")]
    pub(crate) images: Vec<RuntimeContentAsset<ImageAsset>>,
    #[cfg(feature = "runtime-2d")]
    pub(crate) tilesets: Vec<RuntimeContentAsset<TileSetDescriptor>>,
    #[cfg(feature = "runtime-2d")]
    pub(crate) materials: Vec<RuntimeContentAsset<Material2dAsset>>,
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
        self.prepare_scene_closure(product)
    }

    /// Prepares a Scene already selected by the Host's current Deferred dependency authority.
    pub(crate) fn prepare_scene_closure(
        &self,
        product: AssetProductRef,
    ) -> Result<Arc<ProcessedSceneContent>, RuntimeContentError> {
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
        let members = self.catalog().members();
        let deferred_scene_products = || {
            decoded
                .scenes
                .iter()
                .map(|(index, _)| *index)
                .chain(decoded.assets.iter().map(|asset| asset.member))
                .flat_map(|index| members[index].dependencies())
                .filter_map(|dependency| {
                    if dependency.role() != RuntimeContentDependencyRole::Reference {
                        return None;
                    }
                    let product = dependency.product();
                    let index = self.catalog().member_index(product)?;
                    (matches!(
                        members[index].kind(),
                        RuntimeContentMemberKind::Scene { .. }
                    ) && decoded
                        .scenes
                        .binary_search_by_key(&index, |(index, _)| *index)
                        .is_err())
                    .then_some(product)
                })
        };
        let deferred_count = deferred_scene_products().count();
        let codecs = self.plan().runtime_codecs();
        let kind = RuntimeContentBudgetKind::RetainedBytes;
        let bytes = scene_count
            .checked_mul(size_of::<RuntimeContentScene>())
            .and_then(|bytes| {
                bytes.checked_add(deferred_count.checked_mul(size_of::<AssetProductRef>())?)
            })
            .and_then(|bytes| bytes.checked_add(size_of::<ProcessedSceneContent>()))
            .and_then(|bytes| bytes.checked_add(crate::scene_retention::ARC_CONTROL_BLOCK_BYTES))
            .ok_or(RuntimeContentError::BudgetOverflow { kind })?;
        #[cfg(feature = "runtime-2d")]
        let (image_count, tileset_count, material_count, bytes) = {
            let image_count = decoded
                .assets
                .iter()
                .filter(|asset| {
                    codecs.role(&members[asset.member]) == Some(RuntimeCodecRole::Image)
                })
                .count();
            let tileset_count = decoded
                .assets
                .iter()
                .filter(|asset| {
                    codecs.role(&members[asset.member]) == Some(RuntimeCodecRole::TileSet)
                })
                .count();
            let publishes_materials = self.selects_material_publication();
            let mut material_count = 0;
            for asset in &decoded.assets {
                let Some(material) = asset.typed::<Material2dDomain>() else {
                    continue;
                };
                let member = &members[asset.member];
                let native = publishes_materials
                    && codecs.role(member) == Some(RuntimeCodecRole::Material2d);
                self.validate_material_defaults(member, material.product(), native)?;
                material_count += usize::from(native);
            }
            let bytes = image_count
                .checked_mul(size_of::<RuntimeContentAsset<ImageAsset>>())
                .and_then(|image_bytes| bytes.checked_add(image_bytes))
                .and_then(|bytes| {
                    bytes.checked_add(
                        tileset_count
                            .checked_mul(size_of::<RuntimeContentAsset<TileSetDescriptor>>())?,
                    )
                })
                .and_then(|bytes| {
                    bytes.checked_add(
                        material_count
                            .checked_mul(size_of::<RuntimeContentAsset<Material2dAsset>>())?,
                    )
                })
                .ok_or(RuntimeContentError::BudgetOverflow { kind })?;
            (image_count, tileset_count, material_count, bytes)
        };
        let lease = self.reserve_retained(bytes)?;
        let mut deferred_products = Vec::with_capacity(deferred_count);
        deferred_products.extend(deferred_scene_products());
        deferred_products.sort_unstable();
        deferred_products.dedup();
        let mut dependencies = Vec::with_capacity(scene_count);
        for (index, scene) in &decoded.scenes {
            if *index != root {
                dependencies.push(scene.clone());
            }
        }
        let domains = codecs.prepare_collections(self, &decoded.assets)?;
        #[cfg(feature = "runtime-2d")]
        let (images, tilesets, materials) = {
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
            let mut materials = Vec::with_capacity(material_count);
            if material_count != 0 {
                materials.extend(
                    decoded
                        .assets
                        .iter()
                        .filter(|asset| {
                            codecs.role(&members[asset.member])
                                == Some(RuntimeCodecRole::Material2d)
                        })
                        .filter_map(DecodedAsset::typed::<Material2dDomain>)
                        .cloned(),
                );
                materials.sort_unstable_by_key(RuntimeContentAsset::reference);
            }
            (images, tilesets, materials)
        };
        // Typed collections and native preparations share these acquisitions. Only the discovery
        // wrappers are temporary; no codec runs again when the final collections are assembled.
        drop(decoded);
        #[cfg(feature = "runtime-2d")]
        {
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
                .flat_map(|scene| scene.candidate().document().entities())
            {
                let Some(record) = entity.components().get(&tilemap) else {
                    continue;
                };
                let scratch = tilemap_membership_work_bytes(record.value()).ok_or(
                    RuntimeContentError::BudgetOverflow {
                        kind: RuntimeContentBudgetKind::WorkBytes,
                    },
                )?;
                let _work = self.inner.budget.reserve(0, scratch)?;
                validate_persisted_tilemap_membership(record.value(), |reference| {
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
            let material = nara_reflect::ComponentTypeId::new("nara.material2d");
            for entity in std::iter::once(&scene)
                .chain(dependencies.iter())
                .flat_map(|scene| scene.candidate().document().entities())
            {
                let Some(record) = entity.components().get(&material) else {
                    continue;
                };
                let binding = SceneMaterialBinding::read(self, record)?;
                binding.validate(
                    |product| {
                        let index = materials
                            .binary_search_by_key(&product, RuntimeContentAsset::reference)
                            .ok()?;
                        let value = materials[index].product();
                        Some((value.product(), value.generation().declaration()))
                    },
                    |product| images.iter().any(|image| image.reference() == product),
                )?;
            }
        }
        Ok(Arc::new(ProcessedSceneContent {
            scene,
            dependencies,
            deferred_scene_products: deferred_products,
            domains,
            #[cfg(feature = "runtime-2d")]
            images,
            #[cfg(feature = "runtime-2d")]
            tilesets,
            #[cfg(feature = "runtime-2d")]
            materials,
            _lease: lease,
        }))
    }
}
