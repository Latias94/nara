//! One catalogue/executable agreement for both concrete encoded backings.

use std::{mem::size_of, sync::Arc};

use nara_core::ByteLimit;

use super::{
    DecodeBudget, EncodedBacking, ProductRecipe, RuntimeContentBase, RuntimeContentBaseInner,
    RuntimeContentBudgetKind, RuntimeContentCatalog, RuntimeContentCatalogLimits,
    RuntimeContentError, RuntimeContentExpectation, RuntimeContentLimits, budget_error, check,
    checked_add, resolve_runtime_configuration,
};
use crate::project_host::RuntimeConfigurationPlan;

pub(super) struct AdmittedCatalog {
    pub(super) bytes: Vec<u8>,
    pub(super) catalog: RuntimeContentCatalog,
    plan: RuntimeConfigurationPlan,
    limits: RuntimeContentLimits,
    declared_encoded: usize,
}

/// Bound the raw input before allocating it; the parsed model gets the true root residual.
pub(super) fn input_limits(
    expected: &RuntimeContentExpectation,
    limits: RuntimeContentLimits,
) -> Result<(usize, RuntimeContentCatalogLimits), RuntimeContentError> {
    let length = usize::try_from(expected.catalog_digest.length()).map_err(|_| {
        budget_error(
            RuntimeContentBudgetKind::TotalEncodedBytes,
            usize::MAX,
            limits.total_encoded_bytes.get(),
        )
    })?;
    check(
        RuntimeContentBudgetKind::TotalEncodedBytes,
        length,
        limits
            .catalog
            .encoded_bytes()
            .get()
            .min(limits.total_encoded_bytes.get()),
    )?;
    check(
        RuntimeContentBudgetKind::RetainedBytes,
        length,
        limits.retained_bytes.get(),
    )?;
    let residual = limits
        .retained_bytes
        .get()
        .checked_sub(length)
        .and_then(ByteLimit::new)
        .ok_or_else(|| {
            budget_error(
                RuntimeContentBudgetKind::RetainedBytes,
                length.saturating_add(1),
                limits.retained_bytes.get(),
            )
        })?;
    Ok((
        length,
        limits
            .catalog
            .with_retained_bytes(
                ByteLimit::new(residual.get().min(limits.catalog.retained_bytes().get()))
                    .expect("nonzero admitted catalog residual"),
            )
            .with_working_bytes(
                ByteLimit::new(
                    limits
                        .work_bytes
                        .get()
                        .min(limits.catalog.working_bytes().get()),
                )
                .expect("nonzero catalog work bound"),
            ),
    ))
}

impl AdmittedCatalog {
    pub(super) fn new(
        bytes: Vec<u8>,
        expected: &RuntimeContentExpectation,
        recipe: ProductRecipe,
        limits: RuntimeContentLimits,
        catalog_limits: RuntimeContentCatalogLimits,
    ) -> Result<Self, RuntimeContentError> {
        if nara_fs::ContentDigest::of_bytes(&bytes) != expected.catalog_digest {
            return Err(RuntimeContentError::CatalogDigestMismatch);
        }
        let catalog = RuntimeContentCatalog::from_json_bytes(&bytes, catalog_limits)
            .map_err(RuntimeContentError::Catalog)?;
        if catalog.target() != &expected.target {
            return Err(RuntimeContentError::TargetMismatch);
        }
        let configuration = catalog
            .configuration()
            .clone()
            .resolve()
            .map_err(RuntimeContentError::Configuration)?;
        let plan = resolve_runtime_configuration(&configuration, recipe)
            .map_err(RuntimeContentError::Composition)?;
        if plan.plugin_plan().fingerprint().as_bytes() != catalog.plugin_plan_fingerprint() {
            return Err(RuntimeContentError::RecipeMismatch);
        }
        if plan
            .schema_validation()
            .composition_fingerprint()
            .as_bytes()
            != catalog.schema_fingerprint()
        {
            return Err(RuntimeContentError::SchemaMismatch);
        }
        if plan.runtime_codecs_fingerprint().as_bytes() != catalog.runtime_codecs_fingerprint() {
            return Err(RuntimeContentError::CodecsMismatch);
        }
        let mut declared_encoded = bytes.len();
        for member in catalog.members() {
            plan.runtime_codecs().validate_member(member)?;
            let size = usize::try_from(member.digest().length()).map_err(|_| {
                budget_error(
                    RuntimeContentBudgetKind::MemberEncodedBytes,
                    usize::MAX,
                    limits.member_encoded_bytes.get(),
                )
            })?;
            check(
                RuntimeContentBudgetKind::MemberEncodedBytes,
                size,
                limits.member_encoded_bytes.get(),
            )?;
            declared_encoded = checked_add(
                RuntimeContentBudgetKind::TotalEncodedBytes,
                declared_encoded,
                size,
            )?;
        }
        check(
            RuntimeContentBudgetKind::TotalEncodedBytes,
            declared_encoded,
            limits.total_encoded_bytes.get(),
        )?;
        Ok(Self {
            bytes,
            catalog,
            plan,
            limits,
            declared_encoded,
        })
    }

    /// Admit the complete backing before its member table or any payload allocation.
    pub(super) fn retained_bytes(&self, captured: bool) -> Result<usize, RuntimeContentError> {
        let table_entry = if captured {
            size_of::<Vec<u8>>()
        } else {
            size_of::<u64>()
        };
        let table = self
            .catalog
            .members()
            .len()
            .checked_mul(table_entry)
            .ok_or_else(|| {
                budget_error(
                    RuntimeContentBudgetKind::RetainedBytes,
                    usize::MAX,
                    self.limits.retained_bytes.get(),
                )
            })?;
        let encoded = if captured {
            self.declared_encoded
        } else {
            self.bytes.len()
        };
        let retained = [
            encoded,
            self.catalog.retained_bytes(),
            table,
            size_of::<RuntimeContentBaseInner>(),
            2 * crate::scene_retention::ARC_CONTROL_BLOCK_BYTES,
        ]
        .into_iter()
        .try_fold(0, |total, bytes| {
            checked_add(RuntimeContentBudgetKind::RetainedBytes, total, bytes)
        })?;
        check(
            RuntimeContentBudgetKind::RetainedBytes,
            retained,
            self.limits.retained_bytes.get(),
        )?;
        Ok(retained)
    }

    pub(super) fn publish(self, backing: EncodedBacking, retained: usize) -> RuntimeContentBase {
        RuntimeContentBase {
            inner: Arc::new(RuntimeContentBaseInner {
                catalog_digest: nara_fs::ContentDigest::of_bytes(&self.bytes),
                catalog: self.catalog,
                plan: self.plan,
                catalog_bytes: self.bytes,
                backing,
                budget: DecodeBudget::new(
                    retained,
                    self.limits.retained_bytes.get(),
                    self.limits.work_bytes.get(),
                ),
                limits: self.limits,
            }),
        }
    }
}
