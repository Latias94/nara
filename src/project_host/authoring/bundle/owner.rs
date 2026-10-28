//! Additive data-only bundle preparation and accepted-leaf attachment.
//!
//! A prepared basis does not manufacture a successor file receipt. The incumbent remains the
//! physical observation during source preparation; only an accepted publication can join it to
//! the prepared metadata. Whole-project and Workspace adoption are separate owner operations.

use std::{fmt, sync::Arc};

use nara_fs::ContentDigest;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleError, AuthoringSchemaBundleLimits,
    ComponentSchemaOwnerSnapshot,
};
use nara_scene::__private::AuthoringAdapterReceipt;
use nara_scene::{AuthoringDocumentPublicationError, AuthoringSchemaBasis};

use super::super::{
    AUTHORING_BASIS_ENCODED_MULTIPLIER, AuthoringProjectAdmissionBudget,
    AuthoringProjectAdmissionError, AuthoringSchemaReceipt,
    adapter::{FirstPartyAuthoringAdapterError, project_authoring_adapters},
};
use super::AdmittedAuthoringSchemaBundle;
use crate::project_host::file_publication::{
    AcceptedFilePublication, FilePublicationEvidence, FilePublicationFailureStage,
    FilePublicationHost,
};

pub(crate) enum AuthoringSchemaOwnerPreparation {
    AlreadyAdmitted,
    Prepared {
        owner: PreparedAuthoringSchemaOwner,
        publication: FilePublicationHost,
    },
}

/// Immutable metadata held before any successor bundle leaf exists.
pub(crate) struct PreparedAuthoringSchemaOwner {
    incumbent: Arc<AuthoringSchemaReceipt>,
    bundle: Arc<AuthoringSchemaBundle>,
    bytes: Arc<[u8]>,
    digest: ContentDigest,
    settings: nara_project::EffectiveProjectSettings,
    bundle_path: nara_project::ProjectPath,
    adapters: Box<[AuthoringAdapterReceipt]>,
    basis: AuthoringSchemaBasis,
    base_budget: AuthoringProjectAdmissionBudget,
    budget: AuthoringProjectAdmissionBudget,
}

pub(crate) struct AdmittedAuthoringSchemaOwner {
    pub(crate) schema: Arc<AuthoringSchemaReceipt>,
    pub(crate) adapters: Box<[AuthoringAdapterReceipt]>,
    pub(crate) basis: AuthoringSchemaBasis,
    pub(crate) publication: FilePublicationEvidence,
    pub(crate) usage: super::super::AuthoringProjectAdmissionUsage,
}

/// Publication already happened. Keep its exact leaf and metadata for explicit reconciliation.
pub(crate) struct AuthoringSchemaOwnerAttachmentFailure {
    pub(crate) prepared: PreparedAuthoringSchemaOwner,
    pub(crate) publication: AcceptedFilePublication,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AuthoringSchemaOwnerError {
    #[error("the original project manifest no longer matches its admitted observation")]
    ManifestChanged,
    #[error("the original authoring bundle no longer matches its admitted observation")]
    BundleChanged,
    #[error("adding an owner requires a generation-one catalog without predecessor")]
    UnsupportedLineage,
    #[error("an existing schema owner's complete content differs from the request")]
    ExistingOwnerChanged,
    #[error("authoring owner composition failed")]
    Composition(#[source] AuthoringSchemaBundleError),
    #[error("authoring bundle encoding failed")]
    Encode(#[source] AuthoringSchemaBundleError),
    #[error("authoring adapter preparation failed")]
    Adapters(#[source] FirstPartyAuthoringAdapterError),
    #[error("authoring basis admission failed")]
    Basis(#[source] AuthoringDocumentPublicationError),
    #[error("authoring owner preparation exceeds its project budget")]
    Budget(#[from] AuthoringProjectAdmissionError),
    #[error("the bundle publisher could not bind the admitted leaf")]
    Publisher(FilePublicationFailureStage),
}

impl AuthoringSchemaReceipt {
    /// Composes one admitted metadata owner without granting execution or replacing live sources.
    pub(crate) fn prepare_owner(
        self: &Arc<Self>,
        owner: &ComponentSchemaOwnerSnapshot,
    ) -> Result<AuthoringSchemaOwnerPreparation, AuthoringSchemaOwnerError> {
        self.validate_owner_predecessor()?;
        if let Some(previous) = self.bundle().owner(owner.owner_id()) {
            return if previous.current() == owner.current()
                && previous.predecessor() == owner.predecessor()
            {
                Ok(AuthoringSchemaOwnerPreparation::AlreadyAdmitted)
            } else {
                Err(AuthoringSchemaOwnerError::ExistingOwnerChanged)
            };
        }
        if owner.current().generation() != 1 || owner.predecessor().is_some() {
            return Err(AuthoringSchemaOwnerError::UnsupportedLineage);
        }
        let bundle = AuthoringSchemaBundle::from_owner_snapshots(
            self.bundle()
                .owners()
                .iter()
                .cloned()
                .chain([owner.clone()]),
        )
        .map_err(AuthoringSchemaOwnerError::Composition)?;
        let bytes = bundle
            .to_json_string()
            .map_err(AuthoringSchemaOwnerError::Encode)?
            .into_bytes();
        let mut budget = AuthoringProjectAdmissionBudget::new(self.limits());
        budget.charge_manifest_bytes(self.manifest_bytes().len())?;
        budget.charge_bundle_bytes(bytes.len())?;
        budget.charge_reference(self.bundle_path().as_str().len(), 0, false)?;
        budget.charge_open_file(2)?;
        budget.charge_open_file(2)?;
        // Reuse the Authoring owner's existing encoded-to-metadata model before preparing
        // adapters and the nominal declaration index. This is logical ownership, not RSS.
        let metadata = bytes
            .len()
            .checked_mul(AUTHORING_BASIS_ENCODED_MULTIPLIER)
            .ok_or(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                    maximum: self.limits().retained_candidate_bytes().get(),
                },
            )?;
        budget.charge_source_bytes(0, metadata, metadata, metadata)?;
        let adapters =
            project_authoring_adapters(&bundle).map_err(AuthoringSchemaOwnerError::Adapters)?;
        let basis = AuthoringSchemaBasis::admit(&bundle, &adapters)
            .map_err(AuthoringSchemaOwnerError::Basis)?;
        let publication = FilePublicationHost::bind_existing(
            &self.project_root,
            self.bundle_path().as_str(),
            self.bundle.capability(),
            self.bundle_digest(),
            bundle_byte_limit(),
        )
        .map_err(AuthoringSchemaOwnerError::Publisher)?;
        if Some(publication.parent_identity()) != self.bundle_parent_identity() {
            return Err(AuthoringSchemaOwnerError::BundleChanged);
        }
        Ok(AuthoringSchemaOwnerPreparation::Prepared {
            owner: PreparedAuthoringSchemaOwner {
                incumbent: Arc::clone(self),
                bundle: Arc::new(bundle),
                digest: ContentDigest::of_bytes(&bytes),
                bytes: Arc::from(bytes),
                settings: self.settings.clone(),
                bundle_path: self.bundle_path().clone(),
                adapters,
                basis,
                base_budget: budget,
                budget,
            },
            publication,
        })
    }

    fn validate_owner_predecessor(&self) -> Result<(), AuthoringSchemaOwnerError> {
        let manifest = self
            .project_root
            .open_file(
                &nara_fs::RelativePath::new(super::super::PROJECT_MANIFEST_PATH)
                    .expect("the fixed project manifest path is valid"),
            )
            .map_err(|_| AuthoringSchemaOwnerError::ManifestChanged)?;
        if manifest.identity() != self.manifest_identity()
            || manifest.parent_identity() != self.manifest_parent_identity()
            || manifest
                .verify_digest(
                    self.manifest_digest(),
                    nara_project::DEFAULT_MANIFEST_BYTE_LIMIT,
                )
                .is_err()
        {
            return Err(AuthoringSchemaOwnerError::ManifestChanged);
        }
        let publication = FilePublicationHost::bind_existing(
            &self.project_root,
            self.bundle_path().as_str(),
            self.bundle.capability(),
            self.bundle_digest(),
            bundle_byte_limit(),
        )
        .map_err(|_| AuthoringSchemaOwnerError::BundleChanged)?;
        if Some(publication.parent_identity()) != self.bundle_parent_identity() {
            return Err(AuthoringSchemaOwnerError::BundleChanged);
        }
        Ok(())
    }
}

impl PreparedAuthoringSchemaOwner {
    pub(in crate::project_host::authoring) fn budget(&self) -> AuthoringProjectAdmissionBudget {
        self.budget
    }

    pub(in crate::project_host::authoring) fn retain_capture_budget(
        &mut self,
        budget: AuthoringProjectAdmissionBudget,
    ) {
        self.budget = budget;
    }

    pub(in crate::project_host::authoring) fn adapters(&self) -> &[AuthoringAdapterReceipt] {
        &self.adapters
    }

    pub(crate) fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.basis
    }
    pub(crate) fn bundle(&self) -> &AuthoringSchemaBundle {
        &self.bundle
    }
    pub(crate) fn bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }
    pub(crate) fn incumbent(&self) -> &Arc<AuthoringSchemaReceipt> {
        &self.incumbent
    }

    /// Revalidates the original review input; it never refreshes metadata behind that review.
    pub(crate) fn validate(&self) -> Result<(), AuthoringSchemaOwnerError> {
        self.budget.ensure_within_limits()?;
        self.incumbent.validate_owner_predecessor()
    }

    /// Joins prepared semantics to the actual accepted leaf without another Schema admission.
    pub(crate) fn attach(
        self,
        publication: AcceptedFilePublication,
    ) -> Result<AdmittedAuthoringSchemaOwner, AuthoringSchemaOwnerAttachmentFailure> {
        let source = publication.source();
        let evidence = publication.evidence().receipt();
        let digest = self.digest;
        if self.budget.ensure_within_limits().is_err()
            || evidence.previous_identity() != Some(self.incumbent.bundle_identity())
            || evidence.candidate_identity() != source.identity()
            || evidence.published_identity() != Some(source.identity())
            || evidence.verified_content_digest() != Some(digest)
            || source.digest() != digest
            || source.parent_identity() != self.incumbent.bundle_parent_identity()
            || publication.target().to_str() != self.bundle_path.as_str().rsplit('/').next()
        {
            return Err(AuthoringSchemaOwnerAttachmentFailure {
                prepared: self,
                publication,
            });
        }
        let (publication, source) = publication.into_parts();
        let (capability, digest) = source.into_parts();
        let schema = AuthoringSchemaReceipt {
            attempt: self.incumbent.attempt,
            project_root: self.incumbent.project_root.share(),
            manifest: Arc::clone(&self.incumbent.manifest),
            settings: self.settings,
            bundle: AdmittedAuthoringSchemaBundle {
                path: self.bundle_path,
                capability,
                bytes: self.bytes,
                digest,
                bundle: self.bundle,
            },
            budget: self.base_budget,
        };
        Ok(AdmittedAuthoringSchemaOwner {
            schema: Arc::new(schema),
            adapters: self.adapters,
            basis: self.basis,
            publication,
            usage: self.budget.usage(),
        })
    }
}

impl fmt::Debug for PreparedAuthoringSchemaOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedAuthoringSchemaOwner")
            .field("owners", &self.bundle.owners().len())
            .field("encoded_bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for AuthoringSchemaOwnerAttachmentFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringSchemaOwnerAttachmentFailure")
            .field("prepared", &self.prepared)
            .field("publication", &self.publication)
            .finish()
    }
}

fn bundle_byte_limit() -> u64 {
    u64::try_from(AuthoringSchemaBundleLimits::default().encoded_bytes().get())
        .expect("the bundle limit fits the filesystem byte limit")
}

#[cfg(test)]
mod tests;
