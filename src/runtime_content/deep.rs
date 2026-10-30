//! Strict payload validation without retaining the decoded resource closure or publishing stores.

use std::mem::size_of;

use nara_asset::AssetRef;
use nara_fs::ContentDigest;
use nara_image::ImageDomain;
use nara_tilemap::{
    TileSetDescriptor, TileSetDomain, TilemapMembershipRequirement,
    prepare_persisted_tilemap_membership, tilemap_membership_work_bytes,
};

use super::{
    DecodeLease, RuntimeCodecRole, RuntimeContentBase, RuntimeContentBudgetKind,
    RuntimeContentError, RuntimeContentMemberKind, RuntimeContentScene,
    native::{ImageFact, validate_tileset_image},
    traversal::MemberTraversal,
};

/// The result of checking the actual mandatory payload closure of one immutable base.
///
/// Root Scenes, addressable roots and global Input participate. Every reachable Required or
/// Reference target uses its selected production decoder and complete dependency admission.
/// First-party Image/TileSet/Tilemap CPU constraints are checked against actual decoded facts.
/// Soft metadata alone triggers no work. Opening a base does not perform this operation.
///
/// This report retains no decoded products or publication authority. It proves neither native
/// device allocation nor arbitrary gameplay behavior. Counts describe unique encoded members
/// and their admitted Product mappings, independently of how many references reach them.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeContentValidationReport {
    catalog_digest: ContentDigest,
    members: usize,
    products: usize,
}

impl RuntimeContentValidationReport {
    #[must_use]
    pub const fn catalog_digest(self) -> ContentDigest {
        self.catalog_digest
    }

    #[must_use]
    pub const fn members(self) -> usize {
        self.members
    }

    #[must_use]
    pub const fn products(self) -> usize {
        self.products
    }
}

struct NativeFacts {
    images: Vec<(usize, ImageFact)>,
    tilesets: Vec<(usize, TileSetDescriptor)>,
    requirements: TilemapRequirements,
    _work: DecodeLease,
}

struct TilemapRequirements {
    values: Vec<TilemapMembershipRequirement>,
    _work: DecodeLease,
}

impl TilemapRequirements {
    fn push(
        &mut self,
        budget: &std::sync::Arc<super::DecodeBudget>,
        value: TilemapMembershipRequirement,
    ) -> Result<(), RuntimeContentError> {
        // Runtime dependency admission has already required complete Products at every leaf.
        // In particular, no owned source Path string may escape the parser's scratch charge.
        if !matches!(value.asset_ref(), AssetRef::Product(_)) {
            return Err(RuntimeContentError::ProductIdentityMismatch);
        }
        if self.values.len() == self.values.capacity() {
            let capacity = self.values.len().saturating_mul(2).max(4);
            let bytes = capacity
                .checked_mul(size_of::<TilemapMembershipRequirement>())
                .ok_or(RuntimeContentError::BudgetOverflow {
                    kind: RuntimeContentBudgetKind::WorkBytes,
                })?;
            // Keep the old allocation and its lease while reserving and moving into the new one.
            // Release them only after their actual slots have been disposed.
            let work = budget.reserve(0, bytes)?;
            let mut values = Vec::with_capacity(capacity);
            values.append(&mut self.values);
            self.values = values;
            self._work = work;
        }
        self.values.push(value);
        Ok(())
    }
}

impl NativeFacts {
    fn new(base: &RuntimeContentBase) -> Result<Self, RuntimeContentError> {
        let codecs = base.plan().runtime_codecs();
        let images = base
            .catalog()
            .members()
            .iter()
            .filter(|member| codecs.role(member) == Some(RuntimeCodecRole::Image))
            .count();
        let tilesets = base
            .catalog()
            .members()
            .iter()
            .filter(|member| codecs.role(member) == Some(RuntimeCodecRole::TileSet))
            .count();
        let bytes = images
            .checked_mul(size_of::<(usize, ImageFact)>())
            .and_then(|bytes| {
                bytes.checked_add(tilesets.checked_mul(size_of::<(usize, TileSetDescriptor)>())?)
            })
            .ok_or(RuntimeContentError::BudgetOverflow {
                kind: RuntimeContentBudgetKind::WorkBytes,
            })?;
        let work = base.inner.budget.reserve(0, bytes)?;
        Ok(Self {
            images: Vec::with_capacity(images),
            tilesets: Vec::with_capacity(tilesets),
            requirements: TilemapRequirements {
                values: Vec::new(),
                _work: base.inner.budget.reserve(0, 0)?,
            },
            _work: work,
        })
    }

    fn capture_scene(
        &mut self,
        base: &RuntimeContentBase,
        scene: &RuntimeContentScene,
    ) -> Result<(), RuntimeContentError> {
        let tilemap = nara_reflect::ComponentTypeId::new("nara.tilemap.Tilemap");
        for entity in &scene.candidate().document().entities {
            let Some(record) = entity.components.get(&tilemap) else {
                continue;
            };
            let scratch = tilemap_membership_work_bytes(&record.value).ok_or(
                RuntimeContentError::BudgetOverflow {
                    kind: RuntimeContentBudgetKind::WorkBytes,
                },
            )?;
            let _parser_work = base.inner.budget.reserve(0, scratch)?;
            if let Some(requirement) = prepare_persisted_tilemap_membership(&record.value)
                .map_err(RuntimeContentError::Tilemap)?
            {
                self.requirements.push(&base.inner.budget, requirement)?;
            }
        }
        Ok(())
    }

    fn finish(mut self, base: &RuntimeContentBase) -> Result<(), RuntimeContentError> {
        self.images.sort_unstable_by_key(|(index, _)| *index);
        self.tilesets.sort_unstable_by_key(|(index, _)| *index);
        for (_, descriptor) in &self.tilesets {
            let index = base.member_index(descriptor.image_product())?;
            let extent = self
                .images
                .binary_search_by_key(&index, |(index, _)| *index)
                .ok()
                .map(|slot| self.images[slot].1)
                .filter(|image| image.product == descriptor.image_product())
                .map(|image| image.extent);
            validate_tileset_image(descriptor, extent)?;
        }
        for requirement in &self.requirements.values {
            let AssetRef::Product(product) = requirement.asset_ref() else {
                return Err(RuntimeContentError::ProductIdentityMismatch);
            };
            let index = base.member_index(*product)?;
            let atlas = self
                .tilesets
                .binary_search_by_key(&index, |(index, _)| *index)
                .ok()
                .map(|slot| self.tilesets[slot].1)
                .filter(|descriptor| descriptor.product() == *product)
                .map(|descriptor| descriptor.atlas());
            requirement
                .validate_atlas(atlas)
                .map_err(RuntimeContentError::Tilemap)?;
        }
        Ok(())
    }
}

impl RuntimeContentBase {
    /// Checks all mandatory delivery payloads, releasing heavy outputs after each member.
    ///
    /// Every member is decoded once. Only finite indices and compact cross-member CPU facts
    /// survive until final verification, under the shared work limit. These facts overlap the
    /// current decoder's original promises; deep validation may refuse even when encoded capture
    /// succeeded. No native store or receiving World is changed.
    pub fn validate_deep(&self) -> Result<RuntimeContentValidationReport, RuntimeContentError> {
        let mut pending = MemberTraversal::new(self)?;
        for product in self
            .catalog()
            .scenes()
            .iter()
            .chain(self.catalog().addressable_roots())
        {
            pending.enqueue(self, self.member_index(*product)?)?;
        }
        if let Some(product) = self.plan().settings().input().action_map {
            pending.enqueue(self, self.member_index(product)?)?;
        }
        let mut facts = NativeFacts::new(self)?;
        let mut members = 0;
        let mut products = 0;
        while let Some(index) = pending.next() {
            let member = &self.catalog().members()[index];
            let product = *member
                .products()
                .first()
                .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
            match member.kind() {
                RuntimeContentMemberKind::Scene { .. } => {
                    let scene = self.acquire_scene_member(product)?;
                    facts.capture_scene(self, &scene)?;
                }
                RuntimeContentMemberKind::Asset { .. } => {
                    let asset = self.plan().runtime_codecs().acquire_member(self, index)?;
                    match self.plan().runtime_codecs().role(member) {
                        Some(RuntimeCodecRole::Image) => {
                            let image = asset
                                .typed::<ImageDomain>()
                                .ok_or(RuntimeContentError::NativeResourceUnavailable)?;
                            let fact = self.image_fact(image.reference(), image.product())?;
                            facts.images.push((index, fact));
                        }
                        Some(RuntimeCodecRole::TileSet) => {
                            let tileset = asset
                                .typed::<TileSetDomain>()
                                .ok_or(RuntimeContentError::NativeResourceUnavailable)?;
                            self.validate_tileset_member(tileset.reference(), tileset.product())?;
                            facts.tilesets.push((index, *tileset.product()));
                        }
                        Some(RuntimeCodecRole::Cpu) => {}
                        None => return Err(RuntimeContentError::CodecNotSelected),
                    }
                }
            }
            // The local acquisition and heavy output have now dropped. Only verified demand
            // may reveal the next members; Soft metadata never participates in this closure.
            pending.follow_admitted(self, index, true)?;
            members += 1;
            products += member.products().len();
        }
        facts.finish(self)?;
        Ok(RuntimeContentValidationReport {
            catalog_digest: self.catalog_digest(),
            members,
            products,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{mem::size_of, sync::Arc};

    use nara_asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId};
    use nara_reflect::{ComponentValue, encode_asset_reference_value};
    use nara_tilemap::{
        TilemapMembershipRequirement, prepare_persisted_tilemap_membership,
        tilemap_membership_work_bytes,
    };

    use super::super::{DecodeBudget, RuntimeContentBudgetKind, RuntimeContentError};
    use super::TilemapRequirements;

    const DECODER_WORK: usize = 4096;

    fn reference(tag: u8) -> AssetRef {
        let source =
            StableAssetId::parse_str(&format!("7c240000-0000-4000-8000-{tag:012x}")).unwrap();
        AssetRef::Product(AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap())
    }

    fn requirement(budget: &Arc<DecodeBudget>, tag: u8) -> TilemapMembershipRequirement {
        let value = ComponentValue::map([
            ("tileset", encode_asset_reference_value(&reference(tag))),
            (
                "cells",
                ComponentValue::List(vec![ComponentValue::map([
                    (
                        "coord",
                        ComponentValue::map([
                            ("x", ComponentValue::I64(i64::from(tag))),
                            ("y", ComponentValue::I64(0)),
                        ]),
                    ),
                    (
                        "cell",
                        ComponentValue::map([
                            ("tile", ComponentValue::U64(u64::from(tag))),
                            (
                                "color",
                                ComponentValue::map(
                                    ["r", "g", "b", "a"]
                                        .map(|field| (field, ComponentValue::f64(1.0).unwrap())),
                                ),
                            ),
                        ]),
                    ),
                ])]),
            ),
        ]);
        let scratch = tilemap_membership_work_bytes(&value).unwrap();
        let _parser_work = budget.reserve(0, scratch).unwrap();
        prepare_persisted_tilemap_membership(&value)
            .unwrap()
            .unwrap()
    }

    fn assert_work_used(budget: &Arc<DecodeBudget>, limit: usize, used: usize) {
        // Reserving exactly the expected residual proves the live charge without exposing a
        // meter. One more byte must refuse; the probe itself is returned before this helper exits.
        let remaining = budget.reserve(0, limit - used).unwrap();
        assert!(matches!(
            budget.reserve(0, 1),
            Err(RuntimeContentError::Budget {
                kind: RuntimeContentBudgetKind::WorkBytes,
                required,
                limit: actual_limit,
            }) if required == limit + 1 && actual_limit == limit
        ));
        drop(remaining);
    }

    #[test]
    fn requirement_growth_admits_old_and_new_capacity_with_live_decoder_at_exact_limit() {
        let old_bytes = 4 * size_of::<TilemapMembershipRequirement>();
        let new_bytes = 8 * size_of::<TilemapMembershipRequirement>();
        let limit = DECODER_WORK + old_bytes + new_bytes;
        let budget = DecodeBudget::new(0, 1, limit);
        let decoder = budget.reserve(0, DECODER_WORK).unwrap();
        let mut requirements = TilemapRequirements {
            values: Vec::new(),
            _work: budget.reserve(0, 0).unwrap(),
        };
        for tag in 0..4 {
            requirements
                .push(&budget, requirement(&budget, tag))
                .unwrap();
        }
        assert_eq!(requirements.values.len(), 4);
        assert_eq!(requirements.values.capacity(), 4);
        assert_work_used(&budget, limit, DECODER_WORK + old_bytes);

        requirements.push(&budget, requirement(&budget, 4)).unwrap();
        assert_eq!(requirements.values.len(), 5);
        assert_eq!(requirements.values.capacity(), 8);
        for (tag, requirement) in requirements.values.iter().enumerate() {
            assert_eq!(requirement.asset_ref(), &reference(tag as u8));
        }
        // The old allocation's charge returns only after the move; the new capacity and decoder
        // stay live together until their respective owners are dropped.
        assert_work_used(&budget, limit, DECODER_WORK + new_bytes);
        drop(requirements);
        assert_work_used(&budget, limit, DECODER_WORK);
        drop(decoder);
        assert_work_used(&budget, limit, 0);
    }

    #[test]
    fn requirement_growth_one_short_preserves_old_values_and_refunds_before_retry() {
        let old_bytes = 4 * size_of::<TilemapMembershipRequirement>();
        let new_bytes = 8 * size_of::<TilemapMembershipRequirement>();
        let exact = DECODER_WORK + old_bytes + new_bytes;
        let limit = exact - 1;
        let budget = DecodeBudget::new(0, 1, limit);
        let decoder = budget.reserve(0, DECODER_WORK).unwrap();
        let mut requirements = TilemapRequirements {
            values: Vec::new(),
            _work: budget.reserve(0, 0).unwrap(),
        };
        for tag in 0..4 {
            requirements
                .push(&budget, requirement(&budget, tag))
                .unwrap();
        }
        let old_allocation = requirements.values.as_ptr();
        assert_work_used(&budget, limit, DECODER_WORK + old_bytes);

        assert!(matches!(
            requirements.push(&budget, requirement(&budget, 4)),
            Err(RuntimeContentError::Budget {
                kind: RuntimeContentBudgetKind::WorkBytes,
                required,
                limit: actual_limit,
            }) if required == exact && actual_limit == limit
        ));
        assert_eq!(requirements.values.len(), 4);
        assert_eq!(requirements.values.capacity(), 4);
        assert_eq!(requirements.values.as_ptr(), old_allocation);
        for (tag, requirement) in requirements.values.iter().enumerate() {
            assert_eq!(requirement.asset_ref(), &reference(tag as u8));
        }
        assert_work_used(&budget, limit, DECODER_WORK + old_bytes);

        drop(decoder);
        assert_work_used(&budget, limit, old_bytes);
        requirements.push(&budget, requirement(&budget, 4)).unwrap();
        assert_eq!(requirements.values.len(), 5);
        assert_eq!(requirements.values.capacity(), 8);
        for (tag, requirement) in requirements.values.iter().enumerate() {
            assert_eq!(requirement.asset_ref(), &reference(tag as u8));
        }
        assert_work_used(&budget, limit, new_bytes);
        drop(requirements);
        assert_work_used(&budget, limit, 0);
    }
}
