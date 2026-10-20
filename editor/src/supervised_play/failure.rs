use std::{error::Error, io};

use super::generation::GenerationStoreError;

/// Data-only failure facts; error text and filesystem paths never leave the physical worker.
#[derive(Clone, Copy)]
pub(super) struct HostFailureFacts {
    pub(super) stage: &'static str,
    pub(super) reason: &'static str,
    pub(super) operation: Option<&'static str>,
    pub(super) fs_failure: Option<&'static str>,
    pub(super) io_kind: Option<io::ErrorKind>,
    pub(super) os_error: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_facts_keep_closed_reason_operation_and_filesystem_category() {
        fn assert_send<T: Send>() {}
        assert_send::<HostFailureFacts>();
        let error = GenerationStoreError::Capability {
            operation: "begin-generation-directory",
            source: nara::fs::FsError::Unsupported {
                operation: nara::fs::FsOperation::BeginDirectoryPublication,
                capability: "private-token /private/store",
            },
        };
        let facts = HostFailureFacts::generation("generation-publish", &error);
        assert_eq!(facts.stage, "generation-publish");
        assert_eq!(facts.reason, "capability");
        assert_eq!(facts.operation, Some("begin-generation-directory"));
        assert_eq!(facts.fs_failure, Some("unsupported"));
        assert_eq!(facts.io_kind, None);
        assert_eq!(facts.os_error, None);
        let proof = HostFailureFacts::generation(
            "generation-publish",
            &GenerationStoreError::PublicationProofInsufficient,
        );
        assert_eq!(proof.reason, "publication-proof-insufficient");
        assert_eq!(proof.operation, None);
    }
}

impl HostFailureFacts {
    pub(super) fn capture(
        stage: &'static str,
        reason: &'static str,
        source: Option<&(dyn Error + 'static)>,
    ) -> Self {
        let mut facts = Self {
            stage,
            reason,
            operation: None,
            fs_failure: None,
            io_kind: None,
            os_error: None,
        };
        let mut source = source;
        for _ in 0..8 {
            let Some(error) = source else { break };
            if let Some(error) = error.downcast_ref::<io::Error>() {
                facts.io_kind = Some(error.kind());
                facts.os_error = error.raw_os_error();
                break;
            }
            if let Some(error) = error.downcast_ref::<nara::fs::FsError>() {
                facts.fs_failure = Some(match error {
                    nara::fs::FsError::Unsupported { .. } => "unsupported",
                    nara::fs::FsError::Unproven { .. } => "unproven",
                    nara::fs::FsError::Io { .. } => "io",
                    _ => "contract-rejected",
                });
            }
            source = error.source();
        }
        facts
    }

    pub(super) fn generation(stage: &'static str, error: &GenerationStoreError) -> Self {
        use GenerationStoreError as E;

        let reason = match error {
            E::PublicationCancelled => "publication-cancelled",
            E::ArtifactPathNotAbsolute => "artifact-path-not-absolute",
            E::ArtifactNotPlainFile => "artifact-not-plain-file",
            E::ArtifactNotExecutable => "artifact-not-executable",
            E::ArtifactTooLarge => "artifact-too-large",
            E::ArtifactChangedDuringCopy => "artifact-changed-during-copy",
            E::ActiveGenerationExists => "active-generation-exists",
            E::ActiveWorkDirectoryExists => "active-work-directory-exists",
            E::WorkDirectoryRequiresActiveGeneration => "work-directory-requires-active-generation",
            E::CandidateMismatch => "candidate-mismatch",
            E::GenerationNotActive => "generation-not-active",
            E::LastGoodUnavailable => "last-good-unavailable",
            E::StoreClosed => "store-closed",
            E::CleanupIncomplete => "cleanup-incomplete",
            E::NamespaceNotPlainDirectory => "namespace-not-plain-directory",
            E::StoreIdentityInvalid => "store-identity-invalid",
            E::GenerationPathInvalid => "generation-path-invalid",
            E::GenerationBindingChanged => "generation-binding-changed",
            E::GenerationTreeInvalid => "generation-tree-invalid",
            E::RunStoreNameExhausted => "run-store-name-exhausted",
            E::GenerationSequenceExhausted => "generation-sequence-exhausted",
            E::WorkDirectorySequenceExhausted => "work-directory-sequence-exhausted",
            E::WorkDirectoryIdentityInvalid => "work-directory-identity-invalid",
            E::ClockBeforeEpoch => "clock-before-epoch",
            E::ClockOutOfRange => "clock-out-of-range",
            E::SourceReceiptTooLarge => "source-receipt-too-large",
            E::SourceReceiptEmpty => "source-receipt-empty",
            E::TooManyFeatures => "too-many-features",
            E::ManifestRelativePathInvalid => "manifest-relative-path-invalid",
            E::ProductIdentity(_) => "product-identity-invalid",
            E::ManifestTextEmpty { .. } => "manifest-text-empty",
            E::ManifestTextTooLarge { .. } => "manifest-text-too-large",
            E::LaunchManifestTooLarge => "launch-manifest-too-large",
            E::AllocationFailed => "allocation-failed",
            E::InternalNameRejected => "internal-name-rejected",
            E::PublicationBeginOrphaned => "publication-begin-orphaned",
            E::PublicationRejected => "publication-rejected",
            E::PublicationProofInsufficient => "publication-proof-insufficient",
            E::StagingCleanupIncomplete => "staging-cleanup-incomplete",
            E::PublishedCleanupIncomplete => "published-cleanup-incomplete",
            E::GenerationCleanupIncomplete => "generation-cleanup-incomplete",
            E::PublicationBegin(_) => "publication-begin",
            E::Io { .. } => "io",
            E::Capability { .. } => "capability",
        };
        let mut facts = Self::capture(stage, reason, Some(error));
        if let E::Io { operation, .. } | E::Capability { operation, .. } = error {
            facts.operation = Some(operation);
        }
        facts
    }
}
