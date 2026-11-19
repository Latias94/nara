use std::{any::Any, mem::size_of, sync::Arc};

use nara_asset::{AssetProductRef, AssetRef, AssetRuntimeError};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use nara_material::{Material2dAsset, Material2dDeclaration, Material2dRecord};
use nara_scene::CompiledSceneComponent;

use super::{
    DecodeLease, RuntimeContentBase, RuntimeContentBudgetKind, RuntimeContentError, checked_add,
};
use crate::project_content::materials::material_record_work_bytes;

#[cfg(test)]
mod tests;

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
