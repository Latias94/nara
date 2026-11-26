//! Adapts admitted catalog members and pack failures to the Runtime content API.

use std::{io::Write, panic::resume_unwind};

use nara_asset::{
    AssetRuntimeDecodeError,
    pack::{AssetPreparationError, DecodeBudgetError, PackBackingError, PackError, PackMember},
};

use super::{
    RuntimeContentBase, RuntimeContentBudgetKind, RuntimeContentError, RuntimeContentMember,
    budget_error,
};

impl RuntimeContentBase {
    pub(super) fn packed_member(&self, index: usize) -> PackMember<'_> {
        self.inner.backing.member(
            &self.inner.budget,
            index,
            self.inner.catalog.members()[index].digest(),
        )
    }

    /// Writes a version-1 local pack from this exact admitted base, without domain decoding.
    ///
    /// The stream starts at its current position. The caller owns fresh destination creation,
    /// publication and durability; a failed write can leave partial bytes. Held-file reads
    /// authenticate each member again. Captured members reuse their original authentication.
    pub fn write_pack(
        &self,
        output: &mut (impl Write + ?Sized),
    ) -> Result<(), RuntimeContentError> {
        self.inner.backing.write(
            output,
            &self.inner.budget,
            self.inner
                .catalog
                .members()
                .iter()
                .map(RuntimeContentMember::digest),
        )?;
        Ok(())
    }
}

impl From<PackError> for RuntimeContentError {
    fn from(error: PackError) -> Self {
        match error {
            PackError::Io(error) => Self::Host(error),
            PackError::InvalidHeader => Self::InvalidPackHeader,
            PackError::ExtentMismatch => Self::PackExtentMismatch,
            PackError::MemberDigestMismatch => Self::MemberDigestMismatch,
        }
    }
}

impl From<PackBackingError> for RuntimeContentError {
    fn from(error: PackBackingError) -> Self {
        match error {
            PackBackingError::Pack(error) => error.into(),
            PackBackingError::Budget(error) => error.into(),
        }
    }
}

impl From<DecodeBudgetError> for RuntimeContentError {
    fn from(error: DecodeBudgetError) -> Self {
        match error {
            DecodeBudgetError::RetainedBytes { required, limit } => {
                budget_error(RuntimeContentBudgetKind::RetainedBytes, required, limit)
            }
            DecodeBudgetError::WorkBytes { required, limit } => {
                budget_error(RuntimeContentBudgetKind::WorkBytes, required, limit)
            }
        }
    }
}

pub(super) fn asset_error(
    error: AssetPreparationError<RuntimeContentError>,
) -> RuntimeContentError {
    match error {
        AssetPreparationError::Backing(error) => error.into(),
        AssetPreparationError::EncodedBytes { required, limit } => budget_error(
            RuntimeContentBudgetKind::MemberEncodedBytes,
            required,
            limit,
        ),
        AssetPreparationError::RetainedBytesOverflow => RuntimeContentError::BudgetOverflow {
            kind: RuntimeContentBudgetKind::RetainedBytes,
        },
        AssetPreparationError::Decode(error) => match error {
            AssetRuntimeDecodeError::Codec(error) => RuntimeContentError::Asset(error),
            AssetRuntimeDecodeError::ProductIdentityMismatch => {
                RuntimeContentError::ProductIdentityMismatch
            }
            AssetRuntimeDecodeError::Dependency(error) => error,
            AssetRuntimeDecodeError::RetainedBytes { required, limit } => {
                budget_error(RuntimeContentBudgetKind::RetainedBytes, required, limit)
            }
            AssetRuntimeDecodeError::Panicked(payload) => resume_unwind(payload),
        },
    }
}
