//! Explicit data-only bundle successors and actual-leaf attachment.
//!
//! A prepared basis does not manufacture a publication receipt. Source preparation uses either
//! the incumbent leaf before publication or an explicitly observed successor leaf. Whole-project
//! and Workspace adoption are separate owner operations.

use std::{fmt, sync::Arc};

use nara_fs::ContentDigest;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleError, AuthoringSchemaBundleLimits,
    ComponentSchemaOwnerChange, ComponentSchemaOwnerSnapshot,
};
use nara_scene::__private::AuthoringAdapterReceipt;
use nara_scene::{
    AuthoringDocumentPublicationError, AuthoringSchemaBasis, SchemaReclassificationKind,
};

use super::super::{
    AUTHORING_BASIS_ENCODED_MULTIPLIER, AuthoringProjectAdmissionBudget,
    AuthoringProjectAdmissionError, AuthoringSchemaReceipt,
    adapter::{FirstPartyAuthoringAdapterError, project_authoring_adapters},
};
use super::{AdmittedAuthoringSchemaBundle, admit_bundle};
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

/// Explicit recovery either prepares an ordinary publication or observes its exact successor.
pub(crate) enum AuthoringSchemaOwnerReconciliation {
    Publication(AuthoringSchemaOwnerPreparation),
    Observed(PreparedAuthoringSchemaOwner),
}

/// Immutable metadata with separate live-incumbent and actual source-observation authority.
pub(crate) struct PreparedAuthoringSchemaOwner {
    incumbent: Arc<AuthoringSchemaReceipt>,
    observation: Arc<AuthoringSchemaReceipt>,
    bundle: Arc<AuthoringSchemaBundle>,
    bytes: Arc<[u8]>,
    digest: ContentDigest,
    settings: nara_project::EffectiveProjectSettings,
    bundle_path: nara_project::ProjectPath,
    adapters: Box<[AuthoringAdapterReceipt]>,
    basis: AuthoringSchemaBasis,
    reclassification: SchemaReclassificationKind,
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

/// An actual read leaf joined to prepared metadata without asserting a publication attempt.
pub(crate) struct ObservedAuthoringSchemaOwner {
    pub(crate) schema: Arc<AuthoringSchemaReceipt>,
    pub(crate) adapters: Box<[AuthoringAdapterReceipt]>,
    pub(crate) basis: AuthoringSchemaBasis,
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
    #[error(
        "creation metadata evolution requires the exact existing owner and immediate predecessor"
    )]
    InvalidCreationSuccessor,
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
        change: ComponentSchemaOwnerChange,
    ) -> Result<AuthoringSchemaOwnerPreparation, AuthoringSchemaOwnerError> {
        self.validate_owner_predecessor()?;
        let mut budget = AuthoringProjectAdmissionBudget::new(self.limits());
        budget.charge_manifest_bytes(self.manifest_bytes().len())?;
        self.prepare_owner_with_budget(owner, change, budget)
    }

    /// Reads the selected bundle for a new review, preserving the original manifest and owners.
    /// This does not refresh or reuse a previously reviewed nominal basis.
    pub(crate) fn prepare_observed_owner(
        self: &Arc<Self>,
        owner: &ComponentSchemaOwnerSnapshot,
        change: ComponentSchemaOwnerChange,
    ) -> Result<AuthoringSchemaOwnerReconciliation, AuthoringSchemaOwnerError> {
        self.validate_owner_manifest()?;
        let mut budget = AuthoringProjectAdmissionBudget::new(self.limits());
        budget.charge_manifest_bytes(self.manifest_bytes().len())?;
        let observed = admit_bundle(&self.project_root, self.bundle_path().clone(), &mut budget)?;
        if observed.capability().parent_identity() != self.bundle_parent_identity() {
            return Err(AuthoringSchemaOwnerError::BundleChanged);
        }
        if observed.bundle() == self.bundle() {
            // Ordinary publication still requires the exact incumbent leaf, not equal metadata
            // independently rebound by name. Retain this read's work in the same allowance.
            self.validate_owner_predecessor()?;
            budget.charge_source_bytes(observed.bytes().len(), 0, 0, 0)?;
            return self
                .prepare_owner_with_budget(owner, change, budget)
                .map(AuthoringSchemaOwnerReconciliation::Publication);
        }
        let expected = self
            .owner_successor(owner, change)?
            .ok_or(AuthoringSchemaOwnerError::BundleChanged)?;
        if observed.bundle() != &expected {
            return Err(AuthoringSchemaOwnerError::BundleChanged);
        }
        let mut prepared = PreparedAuthoringSchemaOwner::prepare_metadata(
            self,
            observed.shared_bundle(),
            Arc::clone(&observed.bytes),
            reclassification_kind(owner, change),
            budget,
        )?;
        prepared.observation = Arc::new(AuthoringSchemaReceipt {
            attempt: self.attempt,
            project_root: self.project_root.share(),
            manifest: Arc::clone(&self.manifest),
            settings: self.settings.clone(),
            bundle: observed,
            budget: prepared.base_budget,
        });
        prepared.validate()?;
        Ok(AuthoringSchemaOwnerReconciliation::Observed(prepared))
    }

    fn owner_successor(
        &self,
        owner: &ComponentSchemaOwnerSnapshot,
        change: ComponentSchemaOwnerChange,
    ) -> Result<Option<AuthoringSchemaBundle>, AuthoringSchemaOwnerError> {
        if let Some(previous) = self.bundle().owner(owner.owner_id()) {
            if previous.current() == owner.current()
                && previous.predecessor() == owner.predecessor()
            {
                return Ok(None);
            }
            if change != ComponentSchemaOwnerChange::CreationMetadata {
                return Err(AuthoringSchemaOwnerError::ExistingOwnerChanged);
            }
            if !owner.is_creation_metadata_successor_of(previous) {
                return Err(AuthoringSchemaOwnerError::InvalidCreationSuccessor);
            }
            return AuthoringSchemaBundle::from_owner_snapshots(self.bundle().owners().iter().map(
                |existing| {
                    if existing.owner_id() == owner.owner_id() {
                        owner.clone()
                    } else {
                        existing.clone()
                    }
                },
            ))
            .map(Some)
            .map_err(AuthoringSchemaOwnerError::Composition);
        }
        if change == ComponentSchemaOwnerChange::CreationMetadata {
            return Err(AuthoringSchemaOwnerError::InvalidCreationSuccessor);
        }
        if owner.current().generation() != 1 || owner.predecessor().is_some() {
            return Err(AuthoringSchemaOwnerError::UnsupportedLineage);
        }
        AuthoringSchemaBundle::from_owner_snapshots(
            self.bundle()
                .owners()
                .iter()
                .cloned()
                .chain([owner.clone()]),
        )
        .map(Some)
        .map_err(AuthoringSchemaOwnerError::Composition)
    }

    fn prepare_owner_with_budget(
        self: &Arc<Self>,
        owner: &ComponentSchemaOwnerSnapshot,
        change: ComponentSchemaOwnerChange,
        mut budget: AuthoringProjectAdmissionBudget,
    ) -> Result<AuthoringSchemaOwnerPreparation, AuthoringSchemaOwnerError> {
        let Some(bundle) = self.owner_successor(owner, change)? else {
            return Ok(AuthoringSchemaOwnerPreparation::AlreadyAdmitted);
        };
        let bytes = bundle
            .to_json_string()
            .map_err(AuthoringSchemaOwnerError::Encode)?
            .into_bytes();
        budget.charge_bundle_bytes(bytes.len())?;
        budget.charge_reference(self.bundle_path().as_str().len(), 0, false)?;
        budget.charge_open_file(2)?;
        budget.charge_open_file(2)?;
        let prepared = PreparedAuthoringSchemaOwner::prepare_metadata(
            self,
            Arc::new(bundle),
            Arc::from(bytes),
            reclassification_kind(owner, change),
            budget,
        )?;
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
            owner: prepared,
            publication,
        })
    }

    fn validate_owner_manifest(&self) -> Result<(), AuthoringSchemaOwnerError> {
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
        Ok(())
    }

    fn validate_owner_predecessor(&self) -> Result<(), AuthoringSchemaOwnerError> {
        self.validate_owner_manifest()?;
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
    fn prepare_metadata(
        incumbent: &Arc<AuthoringSchemaReceipt>,
        bundle: Arc<AuthoringSchemaBundle>,
        bytes: Arc<[u8]>,
        reclassification: SchemaReclassificationKind,
        mut budget: AuthoringProjectAdmissionBudget,
    ) -> Result<Self, AuthoringSchemaOwnerError> {
        // Both publication and explicit observation use one metadata preparation kernel and
        // one nominal admission. This accounts logical ownership rather than physical RSS.
        let metadata = bytes
            .len()
            .checked_mul(AUTHORING_BASIS_ENCODED_MULTIPLIER)
            .ok_or(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                    maximum: incumbent.limits().retained_candidate_bytes().get(),
                },
            )?;
        budget.charge_source_bytes(0, metadata, metadata, metadata)?;
        let adapters =
            project_authoring_adapters(&bundle).map_err(AuthoringSchemaOwnerError::Adapters)?;
        let basis = AuthoringSchemaBasis::admit(&bundle, &adapters)
            .map_err(AuthoringSchemaOwnerError::Basis)?;
        Ok(Self {
            incumbent: Arc::clone(incumbent),
            observation: Arc::clone(incumbent),
            bundle,
            digest: ContentDigest::of_bytes(&bytes),
            bytes,
            settings: incumbent.settings.clone(),
            bundle_path: incumbent.bundle_path().clone(),
            adapters,
            basis,
            reclassification,
            base_budget: budget,
            budget,
        })
    }

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
    pub(crate) fn reclassification_kind(&self) -> &SchemaReclassificationKind {
        &self.reclassification
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

    pub(crate) fn observation(&self) -> &Arc<AuthoringSchemaReceipt> {
        &self.observation
    }

    /// Reserves retained operation inputs before their allocation, including refused attempts.
    pub(crate) fn retain_operation_inputs(
        &mut self,
        bytes: usize,
    ) -> Result<(), AuthoringProjectAdmissionError> {
        let mut budget = self.budget();
        let result = budget.charge_source_bytes(0, 0, 0, bytes);
        self.retain_capture_budget(budget);
        result
    }

    /// Revalidates the original review input; it never refreshes metadata behind that review.
    pub(crate) fn validate(&self) -> Result<(), AuthoringSchemaOwnerError> {
        self.budget.ensure_within_limits()?;
        self.observation.validate_owner_predecessor()
    }

    /// Transfers the exact observed read leaf without IO or a synthetic publication receipt.
    /// The caller separately retains any evidence from the original publication attempt.
    pub(crate) fn attach_observed(self) -> Result<ObservedAuthoringSchemaOwner, Self> {
        if Arc::ptr_eq(&self.incumbent, &self.observation)
            || self.budget.ensure_within_limits().is_err()
        {
            return Err(self);
        }
        Ok(ObservedAuthoringSchemaOwner {
            schema: self.observation,
            adapters: self.adapters,
            basis: self.basis,
            usage: self.budget.usage(),
        })
    }

    /// Joins prepared semantics to the actual accepted leaf without another Schema admission.
    pub(crate) fn attach(
        self,
        publication: AcceptedFilePublication,
    ) -> Result<AdmittedAuthoringSchemaOwner, AuthoringSchemaOwnerAttachmentFailure> {
        let source = publication.source();
        let evidence = publication.evidence().receipt();
        let digest = self.digest;
        if !Arc::ptr_eq(&self.incumbent, &self.observation)
            || self.budget.ensure_within_limits().is_err()
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

fn reclassification_kind(
    owner: &ComponentSchemaOwnerSnapshot,
    change: ComponentSchemaOwnerChange,
) -> SchemaReclassificationKind {
    match change {
        ComponentSchemaOwnerChange::Add => SchemaReclassificationKind::DeclarationExtension,
        ComponentSchemaOwnerChange::CreationMetadata => {
            SchemaReclassificationKind::CreationMetadata {
                owner: owner.owner_id().clone(),
            }
        }
    }
}

#[cfg(test)]
mod tests;
