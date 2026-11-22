//! Strict payload validation without retaining the decoded resource closure or publishing stores.

#[cfg(feature = "runtime-2d")]
use std::{
    mem::{size_of, size_of_val},
    sync::Arc,
};

#[cfg(feature = "runtime-2d")]
use nara_asset::AssetRef;
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
use nara_asset::{AssetProductRef, AssetRuntimeError};
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, PublicDiagnosticIdentifier,
    SafeSummary,
};
use nara_fs::ContentDigest;
#[cfg(feature = "runtime-2d")]
use nara_image::ImageDomain;
#[cfg(feature = "runtime-2d")]
use nara_material::{Material2dAsset, Material2dDeclaration, Material2dDomain};
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
use nara_reflect::ComponentTypeId;
#[cfg(feature = "runtime-2d")]
use nara_reflect::{
    ComponentValueRef, asset_reference_decode_work_bytes, decode_asset_reference_value,
};
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
use nara_scene::SceneEntityId;
#[cfg(feature = "runtime-2d")]
use nara_sprite::{
    SPRITE_COMPONENT_ID, SPRITE_IMAGE_FIELD_ID, SPRITE_SCHEMA_OWNER_ID,
    persisted_sprite_image_value,
};
#[cfg(feature = "runtime-2d")]
use nara_tilemap::{
    TileSetDescriptor, TileSetDomain, TilemapMembershipRequirement,
    prepare_persisted_tilemap_membership, tilemap_membership_work_bytes,
};
#[cfg(all(feature = "runtime-2d", feature = "runtime-ui"))]
use nara_ui::__private::persisted_ui_visual_image_value;
#[cfg(feature = "runtime-ui")]
use nara_ui::{
    __private::{UI_VISUAL_ID, UI_VISUAL_IMAGE_FIELD_ID},
    UI_SCHEMA_OWNER_ID, UiVisual,
};

#[cfg(feature = "runtime-2d")]
use super::{
    DecodeBudget, DecodeLease, RuntimeCodecRole, RuntimeContentBudgetKind, RuntimeContentScene,
    checked_add,
    materials::SceneMaterialBinding,
    native::{ImageFact, validate_tileset_image},
};
use super::{
    RuntimeContentBase, RuntimeContentError, RuntimeContentMemberKind, traversal::MemberTraversal,
};

/// The result of checking the actual mandatory payload closure of one immutable base.
///
/// Root Scenes, addressable roots and global Input participate. Every reachable Required or
/// Reference target uses its selected production decoder and complete dependency admission.
/// The compiled first-party validators check Scene Sprite, UI Image and material bindings,
/// Material texture kinds and Image/TileSet/Tilemap CPU constraints against decoded facts and
/// selected codec roles. A selected stock desktop startup map also checks physical keyboard bindings.
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

#[cfg(feature = "runtime-2d")]
struct NativeFacts {
    images: Vec<(usize, ImageFact)>,
    tilesets: Vec<(usize, TileSetDescriptor)>,
    tilemap_requirements: WorkFacts<TilemapMembershipRequirement>,
    material_bindings: WorkFacts<SceneMaterialBinding>,
    materials: WorkFacts<MaterialFact>,
    _work: DecodeLease,
}

#[cfg(feature = "runtime-2d")]
struct MaterialFact {
    member: usize,
    product: AssetProductRef,
    declaration: Material2dDeclaration,
    _work: DecodeLease,
}

#[cfg(feature = "runtime-2d")]
struct WorkFacts<T> {
    values: Vec<T>,
    _work: Option<DecodeLease>,
}

#[cfg(feature = "runtime-2d")]
impl<T> WorkFacts<T> {
    const fn new() -> Self {
        Self {
            values: Vec::new(),
            _work: None,
        }
    }

    fn push(&mut self, budget: &Arc<DecodeBudget>, value: T) -> Result<(), RuntimeContentError> {
        if self.values.len() == self.values.capacity() {
            let capacity = self.values.len().saturating_mul(2).max(4);
            let bytes = capacity.checked_mul(size_of::<T>()).ok_or(
                RuntimeContentError::BudgetOverflow {
                    kind: RuntimeContentBudgetKind::WorkBytes,
                },
            )?;
            // Keep the old allocation and its lease while reserving and moving into the new one.
            // Release them only after their actual slots have been disposed.
            let work = budget.reserve(0, bytes)?;
            let mut values = Vec::with_capacity(capacity);
            values.append(&mut self.values);
            self.values = values;
            self._work = Some(work);
        }
        self.values.push(value);
        Ok(())
    }
}

#[cfg(feature = "runtime-2d")]
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
            tilemap_requirements: WorkFacts::new(),
            material_bindings: WorkFacts::new(),
            materials: WorkFacts::new(),
            _work: work,
        })
    }

    fn capture_scene(
        &mut self,
        base: &RuntimeContentBase,
        scene: &RuntimeContentScene,
    ) -> Result<(), RuntimeContentError> {
        let tilemap = ComponentTypeId::new("nara.tilemap.Tilemap");
        let material = ComponentTypeId::new("nara.material2d");
        let sprite = ComponentTypeId::new(SPRITE_COMPONENT_ID);
        #[cfg(feature = "runtime-ui")]
        let ui_visual = ui_visual_component(base);
        for entity in scene.candidate().document().entities() {
            if let Some(record) = entity.components().get(&tilemap) {
                let scratch = tilemap_membership_work_bytes(record.value()).ok_or(
                    RuntimeContentError::BudgetOverflow {
                        kind: RuntimeContentBudgetKind::WorkBytes,
                    },
                )?;
                let _parser_work = base.inner.budget.reserve(0, scratch)?;
                if let Some(requirement) = prepare_persisted_tilemap_membership(record.value())
                    .map_err(RuntimeContentError::Tilemap)?
                {
                    // Only complete Products can escape the parser's scratch charge.
                    if !matches!(requirement.asset_ref(), AssetRef::Product(_)) {
                        return Err(RuntimeContentError::ProductIdentityMismatch);
                    }
                    self.tilemap_requirements
                        .push(&base.inner.budget, requirement)?;
                }
            }
            if let Some(record) = entity.components().get(&material) {
                let binding = SceneMaterialBinding::read(base, record)?;
                self.material_bindings.push(&base.inner.budget, binding)?;
            }
            if let Some(record) = entity.components().get(&sprite)
                && record
                    .schema_owner()
                    .is_some_and(|owner| owner.as_str() == SPRITE_SCHEMA_OWNER_ID.as_str())
                && let Some(image) = persisted_sprite_image_value(record.value())
            {
                validate_scene_image(
                    base,
                    scene.product(),
                    entity.id(),
                    image,
                    SceneImageBinding::Sprite,
                )?;
            }
            #[cfg(feature = "runtime-ui")]
            if let Some(component) = ui_visual
                && let Some(record) = entity.components().get(component)
                && let Some(image) = persisted_ui_visual_image_value(record.value())
            {
                validate_scene_image(
                    base,
                    scene.product(),
                    entity.id(),
                    image,
                    SceneImageBinding::UiVisual,
                )?;
            }
        }
        Ok(())
    }

    fn capture_material(
        &mut self,
        base: &RuntimeContentBase,
        member: usize,
        material: &Material2dAsset,
    ) -> Result<(), RuntimeContentError> {
        // Keep only the declaration needed by Scene bindings after the decoded Shader drops.
        let declaration = material.generation().declaration();
        let fields = declaration.parameters().fields();
        let textures = declaration.textures();
        let kind = RuntimeContentBudgetKind::WorkBytes;
        let mut bytes = checked_add(kind, size_of_val(fields), size_of_val(textures))?;
        bytes = checked_add(kind, bytes, declaration.id().len())?;
        for (id, name) in fields.iter().map(|field| (field.id(), field.name())).chain(
            textures
                .iter()
                .map(|texture| (texture.id(), texture.name())),
        ) {
            bytes = checked_add(kind, bytes, id.len())?;
            bytes = checked_add(kind, bytes, name.len())?;
        }
        let work = base.inner.budget.reserve(0, bytes)?;
        self.materials.push(
            &base.inner.budget,
            MaterialFact {
                member,
                product: material.product(),
                declaration: declaration.clone(),
                _work: work,
            },
        )
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
        for requirement in &self.tilemap_requirements.values {
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
        self.materials
            .values
            .sort_unstable_by_key(|fact| fact.member);
        let materials = self.materials.values.as_slice();
        for binding in &self.material_bindings.values {
            binding.validate(
                |product| {
                    let member = base.catalog().member_index(product)?;
                    let slot = materials
                        .binary_search_by_key(&member, |fact| fact.member)
                        .ok()?;
                    let fact = &materials[slot];
                    Some((fact.product, &fact.declaration))
                },
                |product| {
                    base.catalog().member_index(product).is_some_and(|member| {
                        self.images
                            .binary_search_by_key(&member, |(index, _)| *index)
                            .is_ok_and(|slot| self.images[slot].1.product == product)
                    })
                },
            )?;
        }
        Ok(())
    }
}

#[cfg(feature = "runtime-ui")]
pub(super) fn ui_visual_component(base: &RuntimeContentBase) -> Option<&ComponentTypeId> {
    let registry = base.plan().schema_validation().snapshot();
    registry
        .schema_for_type::<UiVisual>()
        .map(|schema| schema.id())
        .filter(|id| {
            id.as_str() == UI_VISUAL_ID
                && registry.component_schema_owner(id) == Some(UI_SCHEMA_OWNER_ID)
        })
}

#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
pub(super) enum SceneImageBinding {
    #[cfg(feature = "runtime-2d")]
    Sprite,
    #[cfg(feature = "runtime-ui")]
    UiVisual,
}

#[cfg(feature = "runtime-2d")]
fn validate_scene_image(
    base: &RuntimeContentBase,
    scene: AssetProductRef,
    entity: &SceneEntityId,
    image: ComponentValueRef<'_>,
    binding: SceneImageBinding,
) -> Result<(), RuntimeContentError> {
    let target = {
        let scratch = asset_reference_decode_work_bytes(image)
            .ok_or(RuntimeContentError::ProductIdentityMismatch)?;
        let _work = base.inner.budget.reserve(0, scratch)?;
        let reference = decode_asset_reference_value(image)
            .map_err(|_| RuntimeContentError::ProductIdentityMismatch)?;
        reference
            .as_product()
            .ok_or(RuntimeContentError::ProductIdentityMismatch)?
    };
    let member = base
        .member(target)
        .ok_or(RuntimeContentError::UndeclaredProduct)?;
    if base.plan().runtime_codecs().role(member) != Some(RuntimeCodecRole::Image) {
        return Err(scene_image_publication_error(
            scene, entity, target, binding,
        ));
    }
    Ok(())
}

#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
pub(super) fn scene_image_publication_error(
    scene: AssetProductRef,
    entity: &SceneEntityId,
    target: AssetProductRef,
    binding: SceneImageBinding,
) -> RuntimeContentError {
    let (code, summary, component, field) = match binding {
        #[cfg(feature = "runtime-2d")]
        SceneImageBinding::Sprite => (
            "runtime.content.sprite-image-publication-unavailable",
            "Scene Sprite image requires an Image codec selected for native publication",
            SPRITE_COMPONENT_ID,
            SPRITE_IMAGE_FIELD_ID,
        ),
        #[cfg(feature = "runtime-ui")]
        SceneImageBinding::UiVisual => (
            "runtime.content.ui-visual-image-publication-unavailable",
            "Scene UiVisual background image requires an Image codec selected for native publication",
            UI_VISUAL_ID,
            UI_VISUAL_IMAGE_FIELD_ID,
        ),
    };
    let mut diagnostic = Diagnostic::error(
        DiagnosticCode::new(code).expect("engine diagnostic code"),
        SafeSummary::new(summary).expect("engine diagnostic summary"),
    );
    let scene = scene.to_string();
    let target = target.to_string();
    for (key, value) in [
        ("scene-product", scene.as_str()),
        ("entity", entity.as_str()),
        ("component", component),
        ("field", field),
        ("target-product", target.as_str()),
    ] {
        let key = DiagnosticFieldKey::new(key).expect("engine diagnostic field key");
        let field = match PublicDiagnosticIdentifier::new(value) {
            Ok(value) => DiagnosticField::public_identifier(key, value),
            Err(_) => DiagnosticField::sensitive(key),
        };
        diagnostic = diagnostic
            .try_with_field(field)
            .expect("unique bounded Scene image diagnostic fields");
    }
    RuntimeContentError::Asset(AssetRuntimeError::new(diagnostic))
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
        let startup_input_member = self
            .plan()
            .settings()
            .input()
            .action_map
            .map(|product| self.member_index(product))
            .transpose()?;
        if let Some(index) = startup_input_member {
            pending.enqueue(self, index)?;
        }
        #[cfg(feature = "desktop-winit")]
        let desktop_input_member = startup_input_member.filter(|_| self.uses_stock_keyboard());
        #[cfg(feature = "runtime-2d")]
        let mut facts = NativeFacts::new(self)?;
        #[cfg(feature = "runtime-2d")]
        let publishes_materials = self.selects_material_publication();
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
                    #[cfg(feature = "runtime-2d")]
                    facts.capture_scene(self, &scene)?;
                    drop(scene);
                }
                RuntimeContentMemberKind::Asset { .. } => {
                    let asset = self.plan().runtime_codecs().acquire_member(self, index)?;
                    #[cfg(feature = "desktop-winit")]
                    if desktop_input_member == Some(index) {
                        let input = asset
                            .typed::<nara_input::ActionMapDomain>()
                            .ok_or(RuntimeContentError::CodecMismatch)?;
                        super::input::validate_startup_keyboard(input.product())?;
                    }
                    #[cfg(feature = "runtime-2d")]
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
                        Some(role @ (RuntimeCodecRole::Cpu | RuntimeCodecRole::Material2d)) => {
                            if let Some(material) = asset.typed::<Material2dDomain>() {
                                let native =
                                    role == RuntimeCodecRole::Material2d && publishes_materials;
                                self.validate_material_defaults(
                                    member,
                                    material.product(),
                                    native,
                                )?;
                                if native {
                                    facts.capture_material(self, index, material.product())?;
                                }
                            }
                        }
                        None => return Err(RuntimeContentError::CodecNotSelected),
                    }
                    drop(asset);
                }
            }
            // The local acquisition and heavy output have now dropped. Only verified demand
            // may reveal the next members; Soft metadata never participates in this closure.
            pending.follow_admitted(self, index, true)?;
            members += 1;
            products += member.products().len();
        }
        #[cfg(feature = "runtime-2d")]
        facts.finish(self)?;
        Ok(RuntimeContentValidationReport {
            catalog_digest: self.catalog_digest(),
            members,
            products,
        })
    }
}

#[cfg(all(test, feature = "runtime-2d"))]
mod tests {
    use std::{mem::size_of, sync::Arc};

    use nara_asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId};
    use nara_reflect::{ComponentValue, encode_asset_reference_value};
    use nara_tilemap::{
        TilemapMembershipRequirement, prepare_persisted_tilemap_membership,
        tilemap_membership_work_bytes,
    };

    use super::super::{DecodeBudget, RuntimeContentBudgetKind, RuntimeContentError};
    use super::WorkFacts;

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
        let mut requirements = WorkFacts::new();
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
        let mut requirements = WorkFacts::new();
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
