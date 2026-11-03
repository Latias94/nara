//! Read-only operation dispatch over one authenticated Pending manifest and package capture.

use std::mem::size_of;

use nara_core::{
    ByteLimit, DepthLimit, ItemLimit, PersistentFileHeader, SerdeShapeLimits,
    SerdeShapePreflightError, preflight_serde_shape,
};
use nara_fs::DirectoryCapability;
use nara_project::ProjectAuthoringRecovery;

use super::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
    AuthoringProjectAdmissionLimits, AuthoringSourceOperationLimits,
    recovery_storage::{LoadedPendingRecoveryStorage, RecoveryStorageError, RecoveryStorageHost},
};

const PACKAGE_BYTES: usize = 64 * 1024 * 1024;
const HEADER_FIXED_BYTES: usize = 16 * 1024;
const SHAPE_TEXT_COPIES: usize = 3;
const SHAPE_NODE_BYTES: usize = 128;
const HEADER_TEXT_COPIES: usize = 16;

/// The authenticated package's format kind, independently of its path or Open diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingAuthoringRecoveryKind {
    AuthoredSchemaUpgrade,
    PrefabExtraction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingAuthoringRecoveryInspectionErrorKind {
    Transport,
    Admission,
    Structure,
    Header,
    UnsupportedKind,
}

#[derive(Debug, thiserror::Error)]
#[error("Pending source-authoring inspection failed: {detail}")]
pub struct PendingAuthoringRecoveryInspectionError {
    #[source]
    detail: InspectionFailure,
}

#[derive(Debug, thiserror::Error)]
enum InspectionFailure {
    #[error("actual Pending/package transport could not be admitted")]
    Transport(#[source] RecoveryStorageError),
    #[error("the selected operation allowance refused read-only inspection")]
    Admission(#[source] AuthoringProjectAdmissionError),
    #[error("Pending inspection storage accounting overflowed")]
    AccountingOverflow,
    #[error("the authenticated package exceeds its bounded JSON shape or has invalid syntax")]
    Structure,
    #[error("the authenticated package has an invalid persistent header")]
    Header,
    #[error("the authenticated package is not a supported source-authoring recovery kind")]
    UnsupportedKind,
}

impl PendingAuthoringRecoveryInspectionError {
    pub fn kind(&self) -> PendingAuthoringRecoveryInspectionErrorKind {
        match self.detail {
            InspectionFailure::Transport(RecoveryStorageError::Admission(_))
            | InspectionFailure::Admission(_)
            | InspectionFailure::AccountingOverflow => {
                PendingAuthoringRecoveryInspectionErrorKind::Admission
            }
            InspectionFailure::Transport(_) => {
                PendingAuthoringRecoveryInspectionErrorKind::Transport
            }
            InspectionFailure::Structure => PendingAuthoringRecoveryInspectionErrorKind::Structure,
            InspectionFailure::Header => PendingAuthoringRecoveryInspectionErrorKind::Header,
            InspectionFailure::UnsupportedKind => {
                PendingAuthoringRecoveryInspectionErrorKind::UnsupportedKind
            }
        }
    }

    pub fn host_work_limit_exceeded(&self) -> bool {
        matches!(
            self.detail,
            InspectionFailure::Admission(AuthoringProjectAdmissionError::DecodeWorkExceeded { .. })
                | InspectionFailure::Transport(RecoveryStorageError::Admission(
                    AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
                ))
        )
    }

    pub fn open_handles_limit_exceeded(&self) -> bool {
        matches!(
            self.detail,
            InspectionFailure::Admission(
                AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. }
            ) | InspectionFailure::Transport(RecoveryStorageError::Admission(
                AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. }
            ))
        )
    }
}

impl From<AuthoringProjectAdmissionError> for PendingAuthoringRecoveryInspectionError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Self {
            detail: InspectionFailure::Admission(error),
        }
    }
}

impl From<RecoveryStorageError> for PendingAuthoringRecoveryInspectionError {
    fn from(error: RecoveryStorageError) -> Self {
        Self {
            detail: InspectionFailure::Transport(error),
        }
    }
}

/// Cumulative inspection work and its observed native-handle ceiling. A concrete operation
/// continues this same allowance, including the read and header classification below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingAuthoringRecoveryUsage {
    pub host_work: usize,
    pub peak_open_handles: usize,
}

/// A read-only authenticated transport. Header classification grants no semantic transition,
/// source replacement, Schema admission or Ready authority.
#[must_use]
pub struct PendingAuthoringRecoveryInspection {
    loaded: LoadedPendingRecoveryStorage,
    budget: AuthoringProjectAdmissionBudget,
    limits: AuthoringSourceOperationLimits,
    kind: PendingAuthoringRecoveryKind,
}

impl std::fmt::Debug for PendingAuthoringRecoveryInspection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingAuthoringRecoveryInspection")
            .field("reference", self.reference())
            .field("kind", &self.kind)
            .field("limits", &self.limits)
            .field("usage", &self.usage())
            .finish_non_exhaustive()
    }
}

impl PendingAuthoringRecoveryInspection {
    /// Reads and authenticates the actual package before classifying its header. The concrete
    /// payload remains opaque; no project code, converter, Workspace or publisher runs here.
    /// The 64 MiB transport ceiling is not a promise of header admission: conservative parser
    /// and error-message storage must also fit the unchanged 256 MiB retained allowance.
    /// Header scratch alone reserves 16 times the package length plus 16 KiB, in addition to
    /// the live capture. A larger valid package can therefore return an Admission refusal.
    pub fn inspect(
        root: DirectoryCapability,
        limits: AuthoringSourceOperationLimits,
    ) -> Result<Self, PendingAuthoringRecoveryInspectionError> {
        let mut budget =
            AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
                .for_source_operation(limits.host_work(), limits.open_handles());
        budget.charge_source_bytes(0, 0, size_of::<Self>(), size_of::<Self>())?;
        let loaded = RecoveryStorageHost::load_pending(
            root,
            ByteLimit::new(PACKAGE_BYTES).unwrap(),
            &mut budget,
        )?;
        let kind = classify(loaded.encoded_bytes(), &mut budget)?;
        Ok(Self {
            loaded,
            budget,
            limits,
            kind,
        })
    }

    pub const fn kind(&self) -> PendingAuthoringRecoveryKind {
        self.kind
    }
    pub fn reference(&self) -> &ProjectAuthoringRecovery {
        self.loaded.reference()
    }
    pub const fn limits(&self) -> AuthoringSourceOperationLimits {
        self.limits
    }
    pub fn usage(&self) -> PendingAuthoringRecoveryUsage {
        PendingAuthoringRecoveryUsage {
            host_work: self.budget.usage().decode_work(),
            peak_open_handles: self.budget.usage().peak_open_handles(),
        }
    }

    /// Transfers the original read and allowance; concrete payload admission must not reload it.
    pub(super) fn into_parts(
        self,
    ) -> (
        LoadedPendingRecoveryStorage,
        AuthoringProjectAdmissionBudget,
        AuthoringSourceOperationLimits,
    ) {
        let Self {
            loaded,
            mut budget,
            limits,
            ..
        } = self;
        budget
            .reconcile_retained_reservation(size_of::<Self>(), 0)
            .expect("only the consumed inspection envelope reservation ends here");
        (loaded, budget, limits)
    }
}

fn classify(
    encoded: &[u8],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<PendingAuthoringRecoveryKind, PendingAuthoringRecoveryInspectionError> {
    // Shape retains at most 2N escaped-string capacity and N copied key bytes. Node storage
    // covers the duplicate-key trees; its errors contain bounded descriptions, not input text.
    let fixed = encoded
        .len()
        .checked_mul(SHAPE_TEXT_COPIES)
        .and_then(|bytes| bytes.checked_add(HEADER_FIXED_BYTES))
        .ok_or_else(accounting_overflow)?;
    let available = budget
        .limits
        .retained_candidate_bytes()
        .get()
        .saturating_sub(budget.retained_candidate_bytes);
    let nodes = ((available.saturating_sub(fixed)) / SHAPE_NODE_BYTES)
        .min(encoded.len())
        .min(1_000_000);
    let nodes = ItemLimit::new(nodes).ok_or(
        AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
            maximum: budget.limits.retained_candidate_bytes().get(),
        },
    )?;
    let scratch = fixed
        .checked_add(
            nodes
                .get()
                .checked_mul(SHAPE_NODE_BYTES)
                .ok_or_else(accounting_overflow)?,
        )
        .ok_or_else(accounting_overflow)?;
    let before = budget.retained_candidate_bytes;
    if let Err(error) = budget.charge_source_bytes(0, 0, encoded.len(), scratch) {
        budget.retained_candidate_bytes = before;
        return Err(error.into());
    }
    let shape_result = (|| {
        let shape = SerdeShapeLimits::new(
            DepthLimit::new(64).unwrap(),
            nodes,
            ItemLimit::new(nodes.get().min(16_384)).unwrap(),
            ByteLimit::new(PACKAGE_BYTES).unwrap(),
            ByteLimit::new(PACKAGE_BYTES).unwrap(),
        );
        let mut parser = serde_json::Deserializer::from_slice(encoded);
        preflight_serde_shape(&mut parser, shape).map_err(|error| match error {
            SerdeShapePreflightError::Shape(_) | SerdeShapePreflightError::Parse(_) => {
                PendingAuthoringRecoveryInspectionError {
                    detail: InspectionFailure::Structure,
                }
            }
        })?;
        parser
            .end()
            .map_err(|_| PendingAuthoringRecoveryInspectionError {
                detail: InspectionFailure::Structure,
            })?;
        drop(parser);
        Ok::<_, PendingAuthoringRecoveryInspectionError>(())
    })();
    // The parser, duplicate-key sets and any syntax/shape error have physically dropped.
    budget
        .reconcile_retained_reservation(scratch, 0)
        .expect("shape scratch ends before header admission");
    shape_result?;

    // Locked serde_json 1.0.150 / serde 1.0.228: parser scratch uses at most 2N capacity;
    // all live owned header strings/semver identifiers share at most N input text. A terminal
    // error's Unexpected::Str Debug output can expand to 6N, with String growth below 12N.
    // Unknown-field/key errors and semver errors are no larger. 16N plus fixed owner/message
    // overhead therefore covers these simultaneous owners. IgnoredAny keeps payload opaque.
    // Charge the same conservative text work before parsing or constructing an error message.
    let work = encoded
        .len()
        .checked_mul(HEADER_TEXT_COPIES)
        .ok_or_else(accounting_overflow)?;
    let scratch = work
        .checked_add(HEADER_FIXED_BYTES)
        .ok_or_else(accounting_overflow)?;
    let before = budget.retained_candidate_bytes;
    if let Err(error) = budget.charge_source_bytes(0, 0, work, scratch) {
        // No header allocation occurred; earlier shape/read work is still spent.
        budget.retained_candidate_bytes = before;
        return Err(error.into());
    }
    let result = (|| {
        let header: PersistentFileHeader = serde_json::from_slice(encoded).map_err(|_| {
            PendingAuthoringRecoveryInspectionError {
                detail: InspectionFailure::Header,
            }
        })?;
        match header.kind().as_str() {
            "authored_upgrade_recovery" => Ok(PendingAuthoringRecoveryKind::AuthoredSchemaUpgrade),
            "prefab_extraction_recovery" => Ok(PendingAuthoringRecoveryKind::PrefabExtraction),
            _ => Err(PendingAuthoringRecoveryInspectionError {
                detail: InspectionFailure::UnsupportedKind,
            }),
        }
    })();
    // The owned header, parser and any expanded serde error have physically dropped.
    budget
        .reconcile_retained_reservation(scratch, 0)
        .expect("inspection retains only its loaded transport and finite kind");
    result
}

fn accounting_overflow() -> PendingAuthoringRecoveryInspectionError {
    PendingAuthoringRecoveryInspectionError {
        detail: InspectionFailure::AccountingOverflow,
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
