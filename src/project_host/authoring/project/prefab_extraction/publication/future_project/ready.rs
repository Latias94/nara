//! The extraction's unchanged Schema receipts and exact final manifest publication.

use std::sync::OnceLock;

use nara_project::{ProjectAuthoringState, ProjectPath};
use nara_reflect::AuthoringSchemaBundle;

use super::*;
#[cfg(feature = "tooling")]
use crate::project_host::authoring::schema_models::selected_schema_retained_bytes;
use crate::project_host::authoring::{
    AdmittedAuthoringManifest, PROJECT_MANIFEST_PATH, bundle::AdmittedAuthoringSchemaBundle,
    schema_models::ordinary_schema_capture_budget, source_inventory::SourceInventoryManifest,
};

pub(super) struct PreparedExtractionReady {
    pending: Arc<AuthoringSchemaReceipt>,
    future: Arc<AuthoringSchemaReceipt>,
    basis: AuthoringSchemaBasis,
    ordinary: AuthoringProjectAdmissionBudget,
}

struct Models {
    bundle: Arc<AuthoringSchemaBundle>,
    bytes: Arc<[u8]>,
    basis: AuthoringSchemaBasis,
    adapters: Box<[AuthoringAdapterReceipt]>,
    retained: usize,
    limits: AuthoringProjectAdmissionLimits,
}

impl PreparedExtractionReady {
    pub(super) fn prepare(
        pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
        bundle: PublicationBundle<'_>,
        original: PublicationSchema<'_>,
        attempt: AuthoringProjectAttemptGeneration,
        ordinary_limits: AuthoringProjectAdmissionLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(Self, Box<[AuthoringAdapterReceipt]>), PrefabExtractionProjectError> {
        // Both projections survive their temporary manifest parsers. Reserve them separately
        // before resolving profiles, so returning settings never refunds their live storage.
        let recovery = pending.recovery();
        let ready_bytes = recovery.final_ready_manifest().bytes();
        let settings_bytes = product(sum(pending.manifest_bytes().len(), ready_bytes.len())?, 128)?;
        budget.charge_source_bytes(0, 0, settings_bytes, settings_bytes)?;
        let mut settings = None;
        with_temporary_storage(
            budget,
            |budget| -> Result<(), PrefabExtractionProjectError> {
                let mut actual = SourceInventoryManifest::parse(pending.manifest_bytes(), budget)?;
                super::super::super::pending::validate_configuration(pending, &mut actual, budget)
                    .map_err(ProjectPrefabExtractionFutureInventoryError::from)?;
                let mut future = SourceInventoryManifest::parse(ready_bytes, budget)?;
                if !matches!(
                    future.value_mut().authoring.state,
                    ProjectAuthoringState::Ready
                ) {
                    return Err(PrefabExtractionProjectError::Configuration);
                }
                let resolve = |manifest: &mut SourceInventoryManifest| {
                    manifest
                        .value_mut()
                        .resolve_profile(recovery.selected_profile())
                        .map_err(AuthoringProjectAdmissionError::Profile)
                };
                settings = Some((resolve(&mut actual)?, resolve(&mut future)?));
                Ok(())
            },
        )?;
        let (pending_settings, future_settings) = settings.expect("successful profile preparation");
        let models = select_models(original, recovery, ordinary_limits, budget)?;
        budget.charge_source_bytes(0, 0, models.bytes.len(), 0)?;
        if models.bytes.as_ref() != recovery.unchanged_bundle().blob().bytes() {
            return Err(PrefabExtractionProjectError::Configuration);
        }
        let ordinary = ordinary_schema_capture_budget(
            models.limits,
            ready_bytes.len(),
            models.bytes.len(),
            bundle.path.len(),
            models.retained,
        )?;
        let pending_budget = ordinary_schema_capture_budget(
            models.limits,
            pending.manifest_bytes().len(),
            models.bytes.len(),
            bundle.path.len(),
            models.retained,
        )?;
        let stored = checked_sum([
            size_of::<Self>(),
            product(
                2,
                size_of::<AuthoringSchemaReceipt>() + size_of::<AdmittedAuthoringManifest>(),
            )?,
            product(2, bundle.path.len())?,
            pending.manifest_bytes().len(),
            ready_bytes.len(),
            8 * crate::scene_retention::ARC_CONTROL_BLOCK_BYTES,
        ])?;
        budget.charge_source_bytes(0, 0, stored, stored)?;
        // Pending manifest + its bundle and the private future's separate bundle observation.
        // The future manifest slot receives the final publisher's real leaf, never a read.
        budget.retain_open_handles(3)?;
        let manifest_file = open_verified(
            pending.root(),
            PROJECT_MANIFEST_PATH,
            pending.capability().identity(),
            pending.capability().parent_identity(),
            pending.digest(),
            budget,
        )?;
        let pending_bundle = open_verified(
            pending.root(),
            bundle.path,
            bundle.identity,
            bundle.parent,
            bundle.digest,
            budget,
        )?;
        let future_bundle = open_verified(
            pending.root(),
            bundle.path,
            bundle.identity,
            bundle.parent,
            bundle.digest,
            budget,
        )?;
        let bundle_path = ProjectPath::new(bundle.path)
            .map_err(|_| PrefabExtractionProjectError::Configuration)?;
        let observed_bundle = |file| {
            AdmittedAuthoringSchemaBundle::from_observation(
                bundle_path.clone(),
                file,
                Arc::clone(&models.bytes),
                bundle.digest,
                Arc::clone(&models.bundle),
            )
        };
        let pending_schema = Arc::new(AuthoringSchemaReceipt {
            attempt,
            project_root: pending.root().share(),
            manifest: Arc::new(AdmittedAuthoringManifest {
                capability: OnceLock::from(manifest_file),
                bytes: Arc::from(pending.manifest_bytes()),
                digest: pending.digest(),
            }),
            settings: pending_settings,
            bundle: observed_bundle(pending_bundle),
            budget: pending_budget,
        });
        let future = Arc::new(AuthoringSchemaReceipt {
            attempt,
            project_root: pending.root().share(),
            manifest: Arc::new(AdmittedAuthoringManifest {
                capability: OnceLock::new(),
                bytes: Arc::from(ready_bytes),
                digest: ContentDigest::of_bytes(ready_bytes),
            }),
            settings: future_settings,
            bundle: observed_bundle(future_bundle),
            budget: ordinary,
        });
        Ok((
            Self {
                pending: pending_schema,
                future,
                basis: models.basis,
                ordinary,
            },
            models.adapters,
        ))
    }

    pub(super) fn pending_schema(&self) -> &Arc<AuthoringSchemaReceipt> {
        &self.pending
    }
    pub(super) fn basis(&self) -> &AuthoringSchemaBasis {
        &self.basis
    }
    pub(super) fn ordinary_budget(&self) -> AuthoringProjectAdmissionBudget {
        self.ordinary
    }
    pub(super) fn bytes(&self) -> &[u8] {
        &self.future.manifest.bytes
    }

    pub(super) fn bind_publisher(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<FilePublicationHost, PrefabExtractionProjectError> {
        budget.retain_open_handles(2)?;
        let scratch = sum(NATIVE_SCRATCH, product(PROJECT_MANIFEST_PATH.len(), 2)?)?;
        budget.charge_open_file(HANDLE_PEAK)?;
        budget.charge_reference(PROJECT_MANIFEST_PATH.len(), 0, false)?;
        budget.charge_source_bytes(
            self.pending.manifest_bytes().len(),
            0,
            self.pending.manifest_bytes().len(),
            sum(scratch, size_of::<FilePublicationHost>())?,
        )?;
        let publisher = FilePublicationHost::bind_existing(
            &self.pending.project_root,
            PROJECT_MANIFEST_PATH,
            self.pending.manifest.capability(),
            self.pending.manifest_digest(),
            nara_project::DEFAULT_MANIFEST_BYTE_LIMIT,
        )
        .map_err(PrefabExtractionProjectError::Publisher)?;
        release(budget, scratch)?;
        if Some(publisher.parent_identity()) != self.pending.manifest_parent_identity()
            || publisher.binding()
                != (FilePublicationBinding::Existing {
                    identity: self.pending.manifest_identity(),
                    digest: self.pending.manifest_digest(),
                })
        {
            return Err(PrefabExtractionProjectError::Configuration);
        }
        Ok(publisher)
    }

    pub(super) fn reserve_publication(
        &self,
        publisher: &FilePublicationHost,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<usize, PrefabExtractionProjectError> {
        let bytes = self.bytes().len();
        let cost = publisher
            .binding()
            .checked_content_cost(bytes)
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        budget.charge_open_file(HANDLE_PEAK)?;
        budget.charge_reference(PROJECT_MANIFEST_PATH.len(), 0, false)?;
        let reservation = sum(NATIVE_SCRATCH, bytes)?;
        budget.charge_source_bytes(
            cost.encoded_read_bytes,
            0,
            cost.content_work_bytes,
            reservation,
        )?;
        Ok(reservation)
    }

    pub(super) fn assert_private(&mut self) {
        Arc::get_mut(&mut self.future)
            .expect("the unfilled extraction Ready receipt has never escaped");
        assert!(
            self.future.manifest.capability.get().is_none(),
            "only the final publisher fills this slot"
        );
    }

    /// No parser, allocation, filesystem operation or fallible acceptance follows Ready.
    pub(super) fn install(
        self,
        accepted: AcceptedFilePublication,
        session: &mut AuthoringProjectSessionSnapshot,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> FilePublicationEvidence {
        let (evidence, source) = accepted.into_parts();
        let (capability, digest) = source.into_parts();
        assert_eq!(
            digest, self.future.manifest.digest,
            "the publisher used the fixed Ready bytes"
        );
        assert!(
            self.future.manifest.capability.set(capability).is_ok(),
            "exclusive Ready slot"
        );
        session.source_index.commit_manifest_publication(
            self.future.manifest_identity(),
            self.future.manifest_digest(),
        );
        let previous = std::mem::replace(&mut session.schema, self.future);
        assert!(
            Arc::ptr_eq(&previous, &self.pending),
            "the saved candidate owns this actual Pending receipt"
        );
        drop(previous);
        if let Some(receipt) = Arc::into_inner(self.pending) {
            drop(receipt);
            budget.release_open_handles(2);
        }
        evidence
    }
}

fn select_models(
    original: PublicationSchema<'_>,
    recovery: &PrefabExtractionRecovery,
    ordinary_limits: AuthoringProjectAdmissionLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Models, PrefabExtractionProjectError> {
    let (bundle, bytes, basis, adapters, retained, limits) = match original {
        #[cfg(feature = "tooling")]
        PublicationSchema::Live {
            receipt,
            adapters,
            basis,
        } => {
            budget.charge_source_bytes(
                0,
                0,
                product(
                    receipt.bundle_bytes().len(),
                    AUTHORING_BASIS_ENCODED_MULTIPLIER,
                )?,
                0,
            )?;
            let retained = selected_schema_retained_bytes(receipt.bundle(), adapters)?;
            (
                receipt.bundle.shared_bundle(),
                receipt.bundle.shared_bytes(),
                basis,
                adapters,
                retained,
                receipt.budget.limits,
            )
        }
        PublicationSchema::Recovered(models) => {
            let encoded = recovery.unchanged_bundle().blob().bytes();
            budget.charge_source_bytes(0, 0, encoded.len(), encoded.len())?;
            (
                Arc::clone(&models.bundle),
                Arc::from(encoded),
                models.basis.clone(),
                models.adapters.as_ref(),
                models.retained_bytes,
                ordinary_limits,
            )
        }
    };
    let adapter_bytes = adapter_receipts_retained_bytes(adapters)?;
    budget.charge_source_bytes(0, 0, adapter_bytes, adapter_bytes)?;
    Ok(Models {
        bundle,
        bytes,
        basis,
        adapters: adapters.to_vec().into_boxed_slice(),
        retained,
        limits,
    })
}

fn open_verified(
    root: &DirectoryCapability,
    path: &str,
    identity: FileIdentity,
    parent: Option<FileIdentity>,
    digest: ContentDigest,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<FileCapability, PrefabExtractionProjectError> {
    // Pre-admitted retained slots belong to the surrounding preparation, not this scratch scope.
    let scratch = sum(NATIVE_SCRATCH, product(path.len(), 2)?)?;
    budget.charge_open_file(HANDLE_PEAK)?;
    budget.charge_reference(path.len(), 0, false)?;
    let bytes =
        usize::try_from(digest.length()).map_err(|_| SourceInventoryError::AccountingOverflow)?;
    budget.charge_source_bytes(bytes, 0, sum(bytes, path.len())?, scratch)?;
    let result = (|| {
        let path = RelativePath::new(path)?;
        let file = root.open_file_exact(&path)?;
        if file.identity() != identity || file.parent_identity() != parent {
            return Err(PrefabExtractionProjectError::Configuration);
        }
        file.verify_digest(digest, digest.length())?;
        Ok(file)
    })();
    release(budget, scratch)?;
    result
}
