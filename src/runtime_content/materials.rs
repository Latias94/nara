use std::{any::Any, mem::size_of, sync::Arc};

use nara_material::Material2dAsset;

use super::{
    DecodeLease, RuntimeContentBase, RuntimeContentBudgetKind, RuntimeContentError, checked_add,
};

#[cfg(test)]
mod tests;

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
