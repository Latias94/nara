//! Exact payload dependency admission before any outgoing catalogue edge can cause work.

use std::mem::size_of;

use nara_asset::{AssetProductRef, AssetReferenceDemand};

use super::{
    DecodeLease, RuntimeContentBase, RuntimeContentBudgetKind, RuntimeContentDependency,
    RuntimeContentDependencyRole, RuntimeContentError, RuntimeContentMember,
};

pub(super) struct DependencyInventory<'a> {
    member: &'a RuntimeContentMember,
    // One slot per admitted declaration avoids allocating for duplicate payload occurrences.
    // Keep the slots alive until their shared work reservation is returned.
    actual: Vec<Option<AssetReferenceDemand>>,
    _work: DecodeLease,
}

impl<'a> DependencyInventory<'a> {
    pub(super) fn new(
        base: &RuntimeContentBase,
        member: &'a RuntimeContentMember,
    ) -> Result<Self, RuntimeContentError> {
        let bytes = member
            .dependencies()
            .len()
            .checked_mul(size_of::<Option<AssetReferenceDemand>>())
            .ok_or(RuntimeContentError::BudgetOverflow {
                kind: RuntimeContentBudgetKind::WorkBytes,
            })?;
        let work = base.inner.budget.reserve(0, bytes)?;
        Ok(Self {
            member,
            actual: vec![None; member.dependencies().len()],
            _work: work,
        })
    }

    pub(super) fn observe(
        &mut self,
        product: AssetProductRef,
        demand: AssetReferenceDemand,
    ) -> Result<(), RuntimeContentError> {
        let index = self
            .member
            .dependencies()
            .binary_search_by_key(&product, RuntimeContentDependency::product)
            .map_err(|_| RuntimeContentError::DependencyMismatch {
                product,
                declared: None,
                actual: Some(demand),
            })?;
        if self.member.dependencies()[index].role() == RuntimeContentDependencyRole::Soft {
            return Err(RuntimeContentError::DependencyMismatch {
                product,
                declared: Some(RuntimeContentDependencyRole::Soft),
                actual: Some(demand),
            });
        }
        let slot = &mut self.actual[index];
        // A later Immediate occurrence upgrades an earlier Deferred occurrence. Repeated
        // references never create a second Product or a second preparation request.
        if slot.is_none() || demand == AssetReferenceDemand::Immediate {
            *slot = Some(demand);
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<(), RuntimeContentError> {
        for (declaration, actual) in self.member.dependencies().iter().zip(&self.actual) {
            let expected = match declaration.role() {
                RuntimeContentDependencyRole::Required => Some(AssetReferenceDemand::Immediate),
                RuntimeContentDependencyRole::Reference => Some(AssetReferenceDemand::Deferred),
                RuntimeContentDependencyRole::Soft => None,
            };
            if *actual != expected {
                return Err(RuntimeContentError::DependencyMismatch {
                    product: declaration.product(),
                    declared: Some(declaration.role()),
                    actual: *actual,
                });
            }
        }
        Ok(())
    }
}
