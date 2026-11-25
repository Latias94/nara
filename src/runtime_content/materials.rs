use std::{any::Any, mem::size_of, sync::Arc};

use nara_asset::{AssetProductRef, AssetRef, AssetRuntimeError};
use nara_core::diagnostic::{DiagnosticCode, ReportDiagnostic as Diagnostic, SafeSummary};
use nara_render::image::ImageDomain;
use nara_render::material::{
    MATERIAL_PLUGIN_ID, Material2dAsset, Material2dDeclaration, Material2dRecord,
};
use nara_scene::CompiledSceneComponent;

use super::{
    DecodeLease, RuntimeCodecRole, RuntimeContentBase, RuntimeContentBudgetKind,
    RuntimeContentError, RuntimeContentMember, checked_add,
};
use crate::project_content::materials::material_record_work_bytes;

#[cfg(test)]
mod tests;

impl RuntimeContentBase {
    pub(super) fn selects_material_publication(&self) -> bool {
        self.plan()
            .plugin_plan()
            .entries()
            .iter()
            .any(|entry| entry.plugin_id() == MATERIAL_PLUGIN_ID)
    }

    pub(super) fn validate_material_defaults(
        &self,
        member: &RuntimeContentMember,
        material: &Material2dAsset,
        native: bool,
    ) -> Result<(), RuntimeContentError> {
        let codecs = self.plan().runtime_codecs();
        for (slot, reference) in material.textures() {
            let AssetRef::Product(target) = reference else {
                return Err(RuntimeContentError::ProductIdentityMismatch);
            };
            let target_member = self
                .member(*target)
                .ok_or(RuntimeContentError::UndeclaredProduct)?;
            let valid = if native {
                codecs.role(target_member) == Some(RuntimeCodecRole::Image)
            } else {
                codecs.selects_family::<ImageDomain>(target_member)
            };
            if !valid {
                let _work = self.inner.budget.reserve(0, slot.len())?;
                return Err(RuntimeContentError::MaterialTextureKindMismatch {
                    material: material.product(),
                    slot: slot.clone(),
                    target: *target,
                });
            }
            if native {
                self.validate_immediate_dependency(member, *target)?;
            }
        }
        Ok(())
    }
}

pub(super) struct SceneMaterialBinding {
    record: Material2dRecord,
    _work: DecodeLease,
}

impl SceneMaterialBinding {
    pub(super) fn read(
        base: &RuntimeContentBase,
        component: &CompiledSceneComponent,
    ) -> Result<Self, RuntimeContentError> {
        let scratch = material_record_work_bytes(component.value_cost()).ok_or(
            RuntimeContentError::BudgetOverflow {
                kind: RuntimeContentBudgetKind::WorkBytes,
            },
        )?;
        let work = base.inner.budget.reserve(0, scratch)?;
        let record = Material2dRecord::from_component_value(component.value())
            .map_err(|_| binding_mismatch())?;
        Ok(Self {
            record,
            _work: work,
        })
    }

    pub(super) fn validate<'a>(
        &self,
        lookup: impl FnOnce(AssetProductRef) -> Option<(AssetProductRef, &'a Material2dDeclaration)>,
        has_image: impl Fn(AssetProductRef) -> bool,
    ) -> Result<(), RuntimeContentError> {
        let AssetRef::Product(selected) = self.record.material() else {
            return Err(binding_mismatch());
        };
        let (product, declaration) = lookup(*selected).ok_or_else(binding_mismatch)?;
        self.record
            .validate_resolved_product(product, declaration)
            .map_err(|_| binding_mismatch())?;
        if self.record.texture_overrides().values().any(
            |reference| !matches!(reference, AssetRef::Product(product) if has_image(*product)),
        ) {
            return Err(binding_mismatch());
        }
        Ok(())
    }
}

fn binding_mismatch() -> RuntimeContentError {
    RuntimeContentError::Asset(AssetRuntimeError::new(Diagnostic::error(
        DiagnosticCode::new("runtime.content.material-binding-mismatch")
            .expect("engine diagnostic code"),
        SafeSummary::new("Scene material values do not match the admitted material product")
            .expect("engine diagnostic summary"),
    )))
}

struct RuntimeMaterialOwner {
    _lease: DecodeLease,
    _metadata: DecodeLease,
    _base: RuntimeContentBase,
}

pub(super) fn retain_acquisition(
    base: &RuntimeContentBase,
    value: &mut dyn Any,
    lease: DecodeLease,
    retained: usize,
) -> Result<(Option<DecodeLease>, usize), RuntimeContentError> {
    let Some(asset) = value.downcast_mut::<Material2dAsset>() else {
        return Ok((Some(lease), retained));
    };
    let kind = RuntimeContentBudgetKind::RetainedBytes;
    let view = asset
        .generation()
        .lifetime_owner_retained_bytes()
        .ok_or(RuntimeContentError::BudgetOverflow { kind })?;
    let metadata = checked_add(kind, view, size_of::<RuntimeMaterialOwner>())?;
    let metadata = checked_add(
        kind,
        metadata,
        crate::scene_retention::ARC_CONTROL_BLOCK_BYTES,
    )?;
    let retained = checked_add(kind, retained, metadata)?;
    let metadata = base.inner.budget.reserve(metadata, 0)?;
    let owner = Arc::new(RuntimeMaterialOwner {
        _lease: lease,
        _metadata: metadata,
        _base: base.clone(),
    });
    *asset = asset.with_lifetime_owner(owner);
    Ok((None, retained))
}
