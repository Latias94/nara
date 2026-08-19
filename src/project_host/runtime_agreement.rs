use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FileIdentity, RelativePath};
use nara_project::ProjectPath;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleLimits, ComponentRegistrySnapshot,
    PersistentComponentSchemaOwnerId,
};

use super::{ProjectSettingsCandidate, ProjectSettingsLineage, RuntimePlan};
use crate::project_content::ProjectContentError;

/// Prepared proof that the selected executable schema owners agree with the declared authoring
/// bundle. The proof remains tied to the exact settings, root, and frozen schema snapshot until the
/// strict content loader finishes binding every reachable persistent record.
pub(crate) struct PreparedRuntimeSchemaAgreement {
    settings_lineage: ProjectSettingsLineage,
    manifest_digest: ContentDigest,
    schema_snapshot: ComponentRegistrySnapshot,
    bundle_path: ProjectPath,
    bundle_file: FileCapability,
    bundle_digest: ContentDigest,
}

impl PreparedRuntimeSchemaAgreement {
    pub(crate) fn matches_inputs(
        &self,
        project_root_identity: FileIdentity,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
    ) -> bool {
        self.settings_lineage == candidate.lineage()
            && self.settings_lineage.project_root_identity() == Some(project_root_identity)
            && self.manifest_digest == candidate.manifest_digest()
            && self
                .schema_snapshot
                .ptr_eq(plan.schema_validation().snapshot())
    }

    pub(crate) fn revalidate_and_into_closure_member(
        self,
        project_root: &DirectoryCapability,
    ) -> Result<(ProjectPath, ContentDigest), ProjectContentError> {
        self.bundle_file
            .verify_digest(self.bundle_digest, self.bundle_digest.length())
            .map_err(|_| {
                ProjectContentError::runtime_agreement_with_identifier(
                    "project.runtime-agreement.bundle-source-changed",
                    "Project authoring schema bundle bytes changed during content loading",
                    "path",
                    self.bundle_path.as_str(),
                )
            })?;
        let relative_path = RelativePath::new(self.bundle_path.as_str()).map_err(|_| {
            ProjectContentError::runtime_agreement_with_identifier(
                "project.runtime-agreement.bundle-path-invalid",
                "Project authoring schema bundle path is invalid",
                "path",
                self.bundle_path.as_str(),
            )
        })?;
        let current_file = project_root
            .open_file(&relative_path)
            .map_err(ProjectContentError::runtime_agreement_host)?;
        if current_file.identity() != self.bundle_file.identity() {
            return Err(ProjectContentError::runtime_agreement_with_identifier(
                "project.runtime-agreement.bundle-source-changed",
                "Project authoring schema bundle identity changed during content loading",
                "path",
                self.bundle_path.as_str(),
            ));
        }
        current_file
            .verify_digest(self.bundle_digest, self.bundle_digest.length())
            .map_err(|_| {
                ProjectContentError::runtime_agreement_with_identifier(
                    "project.runtime-agreement.bundle-source-changed",
                    "Project authoring schema bundle bytes changed during content loading",
                    "path",
                    self.bundle_path.as_str(),
                )
            })?;
        Ok((self.bundle_path, self.bundle_digest))
    }
}

pub(crate) fn prepare_runtime_schema_agreement(
    project_root: &DirectoryCapability,
    candidate: &ProjectSettingsCandidate,
    plan: &RuntimePlan,
) -> Result<PreparedRuntimeSchemaAgreement, ProjectContentError> {
    let bundle_path = candidate
        .settings()
        .authoring_schema_bundle
        .clone()
        .ok_or_else(|| {
            ProjectContentError::runtime_agreement(
                "project.runtime-agreement.bundle-missing",
                "Project runtime requires a declared authoring schema bundle",
            )
        })?;
    let relative_path = RelativePath::new(bundle_path.as_str()).map_err(|_| {
        ProjectContentError::runtime_agreement_with_identifier(
            "project.runtime-agreement.bundle-path-invalid",
            "Project authoring schema bundle path is invalid",
            "path",
            bundle_path.as_str(),
        )
    })?;
    let bundle_file = project_root
        .open_file(&relative_path)
        .map_err(ProjectContentError::runtime_agreement_host)?;
    let limits = AuthoringSchemaBundleLimits::default();
    let bundle_bytes = bundle_file
        .read_to_end_bounded(u64::try_from(limits.encoded_bytes().get()).unwrap_or(u64::MAX))
        .map_err(ProjectContentError::runtime_agreement_host)?;
    let bundle_digest = ContentDigest::of_bytes(&bundle_bytes);
    let bundle = AuthoringSchemaBundle::from_json_bytes_with_limits(&bundle_bytes, limits)
        .map_err(|_| {
            ProjectContentError::runtime_agreement_with_identifier(
                "project.runtime-agreement.bundle-invalid",
                "Project authoring schema bundle is invalid",
                "path",
                bundle_path.as_str(),
            )
        })?;

    for selected in plan.schema_validation().owner_receipts() {
        let owner_id = PersistentComponentSchemaOwnerId::try_new(selected.owner().as_str())
            .map_err(|_| {
                ProjectContentError::runtime_agreement_with_identifier(
                    "project.runtime-agreement.owner-id-invalid",
                    "Project schema owner identity cannot be compared with executable authority",
                    "schema_owner",
                    selected.owner().as_str(),
                )
            })?;
        let owner = bundle.owner(&owner_id).ok_or_else(|| {
            ProjectContentError::runtime_agreement_with_identifier(
                "project.runtime-agreement.selected-owner-missing",
                "Project authoring schema bundle is missing a selected native owner",
                "schema_owner",
                selected.owner().as_str(),
            )
        })?;
        let current = owner.current();
        if current.generation() != selected.generation()
            || current.fingerprint() != selected.catalog()
            || current.predecessor().copied() != selected.predecessor()
        {
            return Err(ProjectContentError::runtime_agreement_with_identifier(
                "project.runtime-agreement.selected-owner-divergent",
                "Project authoring schema owner differs from the selected native definition",
                "schema_owner",
                selected.owner().as_str(),
            ));
        }
    }

    Ok(PreparedRuntimeSchemaAgreement {
        settings_lineage: candidate.lineage(),
        manifest_digest: candidate.manifest_digest(),
        schema_snapshot: plan.schema_validation().snapshot().clone(),
        bundle_path,
        bundle_file,
        bundle_digest,
    })
}
