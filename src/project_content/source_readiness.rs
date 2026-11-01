//! Current disk readiness for one source load, independent of a proposed manifest's bytes.
//!
//! This gate refuses Pending through the manifest ingestion owner's existing diagnostic. It
//! does not inspect a recovery package. Once that format has an owner, missing/corrupt/mismatched
//! Pending package diagnostics must be integrated there before claiming recovery validation.
//! Delivery shares its entry observation across Scene loads and revalidates it again while
//! retaining the complete union receipt, before that receipt can escape to its caller.
//! A caller holding a platform-enforced exclusive manifest lock must arrange its readable
//! readiness observation through that lock's owner. This gate never bypasses a failed disk read.

use nara_fs::FileCapability;
use nara_project::DEFAULT_MANIFEST_BYTE_LIMIT;

use crate::project_host::{
    ProjectCandidateError, ProjectCandidateErrorKind, ingest_project_manifest_bytes,
};

use super::{
    BudgetTicket, ContentDigest, ProjectContentBudgetKind, ProjectContentError,
    ProjectContentErrorKind, ProjectContentLoader, ProjectSettingsCandidate,
    RELATIVE_OPEN_HANDLE_PEAK, RelativePath,
};

const PROJECT_MANIFEST: &str = "nara.toml";

pub(super) struct SourceReadiness {
    // Keep the observed leaf alive: a bare FileIdentity cannot prevent native ID reuse.
    manifest: FileCapability,
    digest: ContentDigest,
    // Drop the handle before releasing its reservation.
    _budget: BudgetTicket,
}

impl SourceReadiness {
    pub(super) fn capture(
        loader: &ProjectContentLoader,
        candidate: &ProjectSettingsCandidate,
    ) -> Result<Self, ProjectContentError> {
        let mut budget = loader.budget_host.reserve();
        budget
            .set_many(&[
                (ProjectContentBudgetKind::DirectoryDepth, 1),
                (ProjectContentBudgetKind::DirectoryEntries, 1),
                (ProjectContentBudgetKind::PathBytes, PROJECT_MANIFEST.len()),
                (ProjectContentBudgetKind::Files, 1),
                (
                    ProjectContentBudgetKind::OpenHandles,
                    RELATIVE_OPEN_HANDLE_PEAK,
                ),
            ])
            .map_err(ProjectContentError::budget)?;
        let relative = RelativePath::new(PROJECT_MANIFEST)
            .expect("the engine-owned manifest name is a valid relative path");
        let manifest = loader
            .root
            .open_file(&relative)
            .map_err(ProjectCandidateError::from_manifest_authority)
            .map_err(manifest_error)?;
        let limit = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT)
            .expect("the project manifest byte limit fits usize on supported targets");
        // Match the source reader's pre-read ceiling admission. The manifest owner's
        // AuthoringProjectAdmissionBudget::charge_manifest_bytes counts decoded/decode work
        // by encoded bytes; use that same logical work unit, not an invented heap multiplier.
        budget
            .set_many(&[
                (ProjectContentBudgetKind::EncodedBytes, limit),
                (ProjectContentBudgetKind::WorkBytes, limit),
            ])
            .map_err(ProjectContentError::budget)?;
        let bytes = manifest
            .read_to_end_bounded(DEFAULT_MANIFEST_BYTE_LIMIT)
            .map_err(ProjectCandidateError::from_manifest_authority)
            .map_err(manifest_error)?;
        budget
            .set_many(&[
                (ProjectContentBudgetKind::EncodedBytes, bytes.len()),
                (ProjectContentBudgetKind::WorkBytes, bytes.len()),
            ])
            .map_err(ProjectContentError::budget)?;
        let observed = ingest_project_manifest_bytes(
            &bytes,
            candidate.settings().profile_name.as_deref(),
            loader.root.identity(),
        )
        .map_err(manifest_error)?;
        // Ingestion requires explicit authoring.state and rejects Pending source projection.
        let digest = observed.manifest_digest();
        drop(observed);
        drop(bytes);
        budget
            .set_many(&[
                (ProjectContentBudgetKind::OpenHandles, 1),
                (ProjectContentBudgetKind::EncodedBytes, 0),
                (ProjectContentBudgetKind::WorkBytes, 0),
            ])
            .map_err(ProjectContentError::budget)?;
        Ok(Self {
            manifest,
            digest,
            _budget: budget,
        })
    }

    pub(super) fn revalidate(
        &self,
        loader: &ProjectContentLoader,
        candidate: &ProjectSettingsCandidate,
    ) -> Result<(), ProjectContentError> {
        // Reopen through the root, rather than rereading a detached historical manifest leaf.
        let current = Self::capture(loader, candidate)?;
        if current.manifest.identity() != self.manifest.identity() || current.digest != self.digest
        {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::HostAuthorityRejected,
                "project.content.readiness-source-changed",
                "Project readiness manifest changed during source content loading",
            ));
        }
        // Deliberately compare actual entry/final observations, not candidate.manifest_digest():
        // startup repair checks a proposed manifest while its old Ready manifest is still on disk.
        Ok(())
    }
}

fn manifest_error(error: ProjectCandidateError) -> ProjectContentError {
    let kind = match error.kind() {
        ProjectCandidateErrorKind::HostIo => ProjectContentErrorKind::HostIo,
        ProjectCandidateErrorKind::HostAuthorityUnsupported => {
            ProjectContentErrorKind::HostAuthorityUnsupported
        }
        ProjectCandidateErrorKind::HostAuthorityUnproven => {
            ProjectContentErrorKind::HostAuthorityUnproven
        }
        _ => ProjectContentErrorKind::HostAuthorityRejected,
    };
    ProjectContentError::with_report(kind, error.diagnostics().clone())
}
