//! One reviewed Schema admission and its exclusively owned final Ready publisher.
//!
//! This owner does not prove complete source membership, working-copy revisions, execution
//! retirement or whole-project adoption. The enclosing operation must validate those facts and
//! hold every required installation guard before calling `BoundSourceUpgradeReady::publish`.
//! Ordinary project admission continues to reject Pending. The private future receipt never
//! escapes with an empty manifest capability, including through the observed-Pending view.

use std::{
    mem::size_of,
    sync::{Arc, OnceLock},
};

use nara_core::{ByteLimit, ItemLimit};
use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FileIdentity, RelativePath};
use nara_project::{
    EffectiveProjectSettings, ProjectAuthoringRecoveryDigest, ProjectAuthoringState,
    ProjectManifest, ProjectPath,
};
use nara_reflect::{AuthoringSchemaBundle, ComponentPropertyReadBudget};
#[cfg(test)]
use nara_reflect::{ComponentFieldSchema, ComponentSchema, ComponentShapeMember};
use nara_scene::{
    __private::{AuthoredFieldUpgrade, AuthoringAdapterReceipt},
    AuthoringDocumentPublicationError, AuthoringSchemaBasis, ScenePatchReport,
};

use super::{
    AdmittedAuthoringManifest, AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
    AuthoringProjectAdmissionLimits, AuthoringProjectAdmissionUsage,
    AuthoringProjectAttemptGeneration, AuthoringSchemaReceipt, PROJECT_MANIFEST_PATH,
    adapter::FirstPartyAuthoringAdapterError,
    bundle::AdmittedAuthoringSchemaBundle,
    recovery_storage::ObservedPendingRecoveryStorage,
    schema_models::{
        SchemaModelsError, SelectedSchemaModels, ordinary_schema_capture_budget,
        prepare_schema_models,
    },
    source_upgrade_inventory::RecoveredSourceUpgradeInventory,
    source_upgrade_recovery::SourceUpgradeRecovery,
};
use crate::project_host::file_publication::{
    FilePublicationBinding, FilePublicationEvidence, FilePublicationFailureStage,
    FilePublicationHost, FilePublicationOutcome, FilePublicationRejection,
};

#[cfg(test)]
use super::adapter::project_authoring_adapters;

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

// Exact-path checks and digest reads have bounded native scratch. These are overlapping peak
// reservations, refunded only after the corresponding opens/hash operations have returned.
const SOURCE_SCRATCH: usize = 2 * 65_536;
const HANDLE_PEAK: usize = 12;
const PREPARED_HANDLE_RESERVATION: usize = 4;
const PUBLISHER_HANDLE_RESERVATION: usize = 2;
// Pending manifest, recovery file, two bundle observations and the final publisher/cleanup.
pub(super) const RETAINED_HANDLE_CAPACITY: usize =
    PREPARED_HANDLE_RESERVATION + PUBLISHER_HANDLE_RESERVATION;

/// Prepared before Pending, shared by the complete proposal and the eventual Ready receipt.
/// There is exactly one successor basis admission; observing published bytes never re-admits it.
pub(super) struct PreparedSourceUpgradeSchema {
    origin: SourceUpgradeSchemaOrigin,
    operation: ProjectAuthoringRecoveryDigest,
    bundle: Arc<AuthoringSchemaBundle>,
    bundle_bytes: Arc<[u8]>,
    bundle_digest: ContentDigest,
    future_manifest: ProjectManifest,
    future_manifest_bytes: Arc<[u8]>,
    future_manifest_digest: ContentDigest,
    future_settings: EffectiveProjectSettings,
    capture_budget: AuthoringProjectAdmissionBudget,
    models_retained_bytes: usize,
    adapters: Box<[AuthoringAdapterReceipt]>,
    upgrade: AuthoredFieldUpgrade,
}

/// Data definitions and current native authority have separate provenance after a restart.
/// Live sources retain their exact incumbent; recovery retains only this process's observations
/// and newly admitted old definitions. Neither variant reconstructs an original file receipt.
enum SourceUpgradeSchemaOrigin {
    Live(Arc<AuthoringSchemaReceipt>),
    Recovery {
        root: DirectoryCapability,
        bundle_path: ProjectPath,
        bundle_parent: DirectoryCapability,
        attempt: AuthoringProjectAttemptGeneration,
        previous_models: SelectedSchemaModels,
    },
}

impl SourceUpgradeSchemaOrigin {
    fn root(&self) -> &DirectoryCapability {
        match self {
            Self::Live(receipt) => &receipt.project_root,
            Self::Recovery { root, .. } => root,
        }
    }
    fn bundle_path(&self) -> &ProjectPath {
        match self {
            Self::Live(receipt) => receipt.bundle_path(),
            Self::Recovery { bundle_path, .. } => bundle_path,
        }
    }
    fn bundle_parent_identity(&self) -> Option<FileIdentity> {
        match self {
            Self::Live(receipt) => receipt.bundle_parent_identity(),
            Self::Recovery { bundle_parent, .. } => Some(bundle_parent.identity()),
        }
    }
    fn attempt(&self) -> AuthoringProjectAttemptGeneration {
        match self {
            Self::Live(receipt) => receipt.attempt,
            Self::Recovery { attempt, .. } => *attempt,
        }
    }
}

impl PreparedSourceUpgradeSchema {
    /// Admits new model authority from the verified package and this process's real observations.
    /// The old definitions explain the stored pair; they are not a restored live file receipt.
    pub(super) fn prepare_recovered(
        pending: &ObservedPendingRecoveryStorage<SourceUpgradeRecovery>,
        inventory: &RecoveredSourceUpgradeInventory,
        profile: Option<&str>,
        attempt: AuthoringProjectAttemptGeneration,
        ordinary_limits: AuthoringProjectAdmissionLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeReadyError> {
        with_candidate_storage(budget, |budget| {
            let recovery = pending.recovery();
            budget.charge_source_bytes(
                0,
                0,
                add(
                    profile.map_or(0, str::len),
                    recovery.selected_profile().map_or(0, str::len),
                )?,
                0,
            )?;
            if profile.is_some() && profile != recovery.selected_profile() {
                return Err(SourceUpgradeReadyError::ConfigurationMismatch);
            }
            pending
                .validate(budget)
                .map_err(SourceUpgradeReadyError::RecoveryStorage)?;
            inventory
                .validate(pending, budget)
                .map_err(SourceUpgradeReadyError::RecoveredInventory)?;
            let previous_models =
                prepare_schema_models(recovery.bundle().old_disk.bytes(), budget)?;
            let SelectedSchemaModels {
                bundle,
                adapters,
                basis,
                retained_bytes,
            } = prepare_schema_models(recovery.bundle().successor.bytes(), budget)?;
            let selector_work = add(
                recovery.bundle().old_disk.bytes().len(),
                recovery.bundle().successor.bytes().len(),
            )?;
            budget.charge_source_bytes(0, 0, selector_work, 0)?;
            let mut selectors = ComponentPropertyReadBudget::new(
                ItemLimit::new(selector_work.max(1)).unwrap(),
                ByteLimit::new(selector_work.max(1)).unwrap(),
            );
            let (component, field, property) = recovery.field_upgrade();
            let upgrade = AuthoredFieldUpgrade::prepare(
                previous_models.basis.clone(),
                basis,
                component.clone(),
                field.clone(),
                property.clone(),
                &mut selectors,
            )
            .map_err(SourceUpgradeReadyError::Upgrade)?;
            let future_manifest = parse_manifest(recovery.manifest().successor.bytes(), budget)?;
            if !matches!(
                future_manifest.authoring.state,
                ProjectAuthoringState::Ready
            ) || future_manifest.authoring.schema_bundle.as_deref()
                != Some(recovery.bundle().path.as_str())
            {
                return Err(SourceUpgradeReadyError::ConfigurationMismatch);
            }
            let future_settings = future_manifest
                .resolve_profile(recovery.selected_profile())
                .map_err(AuthoringProjectAdmissionError::Profile)?;
            let encoded = recovery.bundle().successor.bytes();
            let copied = add(
                add(encoded.len(), recovery.manifest().successor.bytes().len())?,
                recovery.bundle().path.len(),
            )?;
            budget.charge_source_bytes(0, 0, copied, add(copied, size_of::<Self>())?)?;
            let bundle_path = ProjectPath::new(recovery.bundle().path.clone())
                .map_err(|_| SourceUpgradeReadyError::ConfigurationMismatch)?;
            let capture_budget = ordinary_schema_capture_budget(
                ordinary_limits,
                recovery.manifest().successor.bytes().len(),
                encoded.len(),
                bundle_path.as_str().len(),
                retained_bytes,
            )?;
            Ok(Self {
                origin: SourceUpgradeSchemaOrigin::Recovery {
                    root: pending.root().share(),
                    bundle_path,
                    bundle_parent: inventory.bundle_parent().share(),
                    attempt,
                    previous_models,
                },
                operation: recovery.digest(),
                bundle,
                bundle_bytes: Arc::from(encoded),
                bundle_digest: content_digest(recovery.bundle().successor.digest()),
                future_manifest,
                future_manifest_bytes: Arc::from(recovery.manifest().successor.bytes()),
                future_manifest_digest: content_digest(recovery.manifest().successor.digest()),
                future_settings,
                capture_budget,
                models_retained_bytes: retained_bytes,
                adapters,
                upgrade,
            })
        })
    }

    /// Uses the live project's actual previous nominal basis. The outer owner must additionally
    /// bind that token to its complete Workspace; equal bundle bytes do not prove live custody.
    pub(super) fn prepare(
        incumbent: &Arc<AuthoringSchemaReceipt>,
        previous_basis: &AuthoringSchemaBasis,
        recovery: &SourceUpgradeRecovery,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeReadyError> {
        with_candidate_storage(budget, |budget| {
            Self::prepare_candidate(incumbent, previous_basis, recovery, budget)
        })
    }

    fn prepare_candidate(
        incumbent: &Arc<AuthoringSchemaReceipt>,
        previous_basis: &AuthoringSchemaBasis,
        recovery: &SourceUpgradeRecovery,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeReadyError> {
        budget.charge_source_bytes(
            0,
            0,
            add(
                add(
                    incumbent.manifest_bytes().len(),
                    incumbent.bundle_bytes().len(),
                )?,
                add(
                    incumbent
                        .settings()
                        .profile_name
                        .as_deref()
                        .map_or(0, str::len),
                    recovery.selected_profile().map_or(0, str::len),
                )?,
            )?,
            0,
        )?;
        if !matches!(
            incumbent.settings().authoring_state,
            ProjectAuthoringState::Ready
        ) || recovery.manifest().path != PROJECT_MANIFEST_PATH
            || recovery.manifest().old_disk.bytes() != incumbent.manifest_bytes()
            || recovery.bundle().path != incumbent.bundle_path().as_str()
            || recovery.bundle().old_disk.bytes() != incumbent.bundle_bytes()
            || recovery.source_universe() != incumbent.settings().project.source_universe
            || recovery.selected_profile() != incumbent.settings().profile_name.as_deref()
        {
            return Err(SourceUpgradeReadyError::OriginalSchemaMismatch);
        }
        verify_named(
            &incumbent.project_root,
            PROJECT_MANIFEST_PATH,
            Some((
                incumbent.manifest_identity(),
                incumbent.manifest_parent_identity(),
            )),
            incumbent.manifest_digest(),
            budget,
            SourceUpgradeReadySource::Manifest,
        )?;
        verify_named(
            &incumbent.project_root,
            incumbent.bundle_path().as_str(),
            Some((
                incumbent.bundle_identity(),
                incumbent.bundle_parent_identity(),
            )),
            incumbent.bundle_digest(),
            budget,
            SourceUpgradeReadySource::Bundle,
        )?;

        // Recovery's owning admission already proved canonical Ready bytes and unchanged
        // complete configuration. Retain and parse those actual bytes, not an invented manifest.
        let future_manifest = parse_manifest(recovery.manifest().successor.bytes(), budget)?;
        if !matches!(
            future_manifest.authoring.state,
            ProjectAuthoringState::Ready
        ) || future_manifest.authoring.schema_bundle.as_deref()
            != Some(incumbent.bundle_path().as_str())
        {
            return Err(SourceUpgradeReadyError::ConfigurationMismatch);
        }
        let future_settings = future_manifest
            .resolve_profile(recovery.selected_profile())
            .map_err(AuthoringProjectAdmissionError::Profile)?;
        if &future_settings != incumbent.settings() {
            return Err(SourceUpgradeReadyError::ConfigurationMismatch);
        }
        let encoded = recovery.bundle().successor.bytes();
        let SelectedSchemaModels {
            bundle,
            adapters,
            basis,
            retained_bytes,
        } = prepare_schema_models(encoded, budget)?;
        let (component, field, next_property) = recovery.field_upgrade();
        // The selector's private budget and the full comparison use a pre-admitted allowance;
        // a failed validation never rolls back work in the enclosing cumulative controller.
        let selector_work = add(encoded.len(), incumbent.bundle_bytes().len())?;
        budget.charge_source_bytes(0, 0, selector_work, 0)?;
        let mut selectors = ComponentPropertyReadBudget::new(
            ItemLimit::new(selector_work.max(1)).expect("selector work is non-zero"),
            ByteLimit::new(selector_work.max(1)).expect("selector bytes are non-zero"),
        );
        let upgrade = AuthoredFieldUpgrade::prepare(
            previous_basis.clone(),
            basis,
            component.clone(),
            field.clone(),
            next_property.clone(),
            &mut selectors,
        )
        .map_err(SourceUpgradeReadyError::Upgrade)?;
        let copied = add(encoded.len(), recovery.manifest().successor.bytes().len())?;
        budget.charge_source_bytes(0, 0, copied, add(copied, size_of::<Self>())?)?;
        // This is the ordinary successor capture baseline, not the upgrade's cumulative work
        // or policy. Admission uses the actual already prepared models without a second basis.
        let capture_budget = ordinary_schema_capture_budget(
            incumbent.limits(),
            recovery.manifest().successor.bytes().len(),
            encoded.len(),
            incumbent.bundle_path().as_str().len(),
            retained_bytes,
        )?;
        Ok(Self {
            origin: SourceUpgradeSchemaOrigin::Live(Arc::clone(incumbent)),
            operation: recovery.digest(),
            bundle,
            bundle_bytes: Arc::from(encoded),
            bundle_digest: content_digest(recovery.bundle().successor.digest()),
            future_manifest,
            future_manifest_bytes: Arc::from(recovery.manifest().successor.bytes()),
            future_manifest_digest: content_digest(recovery.manifest().successor.digest()),
            future_settings,
            capture_budget,
            models_retained_bytes: retained_bytes,
            adapters,
            upgrade,
        })
    }

    pub(super) fn upgrade(&self) -> &AuthoredFieldUpgrade {
        &self.upgrade
    }
    pub(super) fn basis(&self) -> &AuthoringSchemaBasis {
        self.upgrade.successor_basis()
    }
    pub(super) fn adapters(&self) -> &[AuthoringAdapterReceipt] {
        &self.adapters
    }
    pub(super) fn bundle(&self) -> &AuthoringSchemaBundle {
        &self.bundle
    }
    pub(super) fn previous_bundle(&self) -> &AuthoringSchemaBundle {
        match &self.origin {
            SourceUpgradeSchemaOrigin::Live(incumbent) => incumbent.bundle(),
            SourceUpgradeSchemaOrigin::Recovery {
                previous_models, ..
            } => &previous_models.bundle,
        }
    }
    pub(super) fn incumbent(&self) -> Option<&Arc<AuthoringSchemaReceipt>> {
        match &self.origin {
            SourceUpgradeSchemaOrigin::Live(incumbent) => Some(incumbent),
            SourceUpgradeSchemaOrigin::Recovery { .. } => None,
        }
    }
    pub(super) const fn operation(&self) -> ProjectAuthoringRecoveryDigest {
        self.operation
    }
    pub(super) fn settings(&self) -> &EffectiveProjectSettings {
        &self.future_settings
    }
}

/// Actual Pending observations and preallocated, still-private future receipt storage.
pub(super) struct PreparedSourceUpgradeReady {
    selected: PreparedSourceUpgradeSchema,
    pending: Arc<AuthoringSchemaReceipt>,
    future: Arc<AuthoringSchemaReceipt>,
    recovery_path: String,
    recovery_file: FileCapability,
    recovery_digest: ContentDigest,
}

/// The enclosing operation's confirmed Pending leaf, retained continuously across preparation.
/// It is an expected observation, not permission to replace an independently rewritten manifest.
pub(super) struct SourceUpgradePendingManifest<'a> {
    pub(super) capability: &'a FileCapability,
    pub(super) digest: ContentDigest,
}

struct ReadyReceiptStorage {
    pending: Arc<AuthoringSchemaReceipt>,
    future: Arc<AuthoringSchemaReceipt>,
    recovery_path: String,
    recovery_file: FileCapability,
    recovery_digest: ContentDigest,
}

impl PreparedSourceUpgradeReady {
    pub(super) fn ordinary_capture_budget(&self) -> AuthoringProjectAdmissionBudget {
        self.selected.capture_budget
    }

    /// Requires the already-published successor bundle and the actual recovery package referred
    /// to by Pending. The supplied typed package comes from the recovery data owner, not a parser
    /// bypass. Its digest joins it to the exact live bytes read here.
    pub(super) fn prepare(
        selected: PreparedSourceUpgradeSchema,
        recovery: &SourceUpgradeRecovery,
        pending_observation: SourceUpgradePendingManifest<'_>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeReadyPreparationFailure> {
        // The incumbent selected Schema was charged before this call. Refusal returns that
        // same owner and releases only this attempt's dropped Pending/future/read storage.
        let result = with_candidate_storage(budget, |budget| {
            Self::prepare_storage(&selected, recovery, pending_observation, budget)
        });
        match result {
            Ok(storage) => Ok(Self {
                selected,
                pending: storage.pending,
                future: storage.future,
                recovery_path: storage.recovery_path,
                recovery_file: storage.recovery_file,
                recovery_digest: storage.recovery_digest,
            }),
            Err(error) => Err(SourceUpgradeReadyPreparationFailure { selected, error }),
        }
    }

    fn prepare_storage(
        selected: &PreparedSourceUpgradeSchema,
        recovery: &SourceUpgradeRecovery,
        pending_observation: SourceUpgradePendingManifest<'_>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<ReadyReceiptStorage, SourceUpgradeReadyError> {
        if selected.operation != recovery.digest() {
            return Err(SourceUpgradeReadyError::RecoveryMismatch);
        }
        budget.retain_open_handles(PREPARED_HANDLE_RESERVATION)?;
        let root = selected.origin.root();
        let manifest = read_pending_manifest(root, pending_observation, budget)?;
        let pending_manifest = parse_manifest(&manifest.1, budget)?;
        let ProjectAuthoringState::Pending(reference) = &pending_manifest.authoring.state else {
            return Err(SourceUpgradeReadyError::NotPending);
        };
        if reference.recovery_digest() != selected.operation {
            return Err(SourceUpgradeReadyError::RecoveryMismatch);
        }
        budget.charge_source_bytes(
            0,
            0,
            reference.recovery_path().as_str().len(),
            reference.recovery_path().as_str().len(),
        )?;
        let recovery_path = reference.recovery_path().as_str().to_owned();
        let recovery_digest = content_digest(selected.operation);
        let recovery_file = verify_named(
            root,
            &recovery_path,
            None,
            recovery_digest,
            budget,
            SourceUpgradeReadySource::Recovery,
        )?;
        let pending_settings = pending_manifest
            .resolve_profile(selected.future_settings.profile_name.as_deref())
            .map_err(AuthoringProjectAdmissionError::Profile)?;
        // Every modeled field and all profiles must agree. Only readiness itself changes.
        let mut comparable = pending_manifest;
        comparable.authoring.state = ProjectAuthoringState::Ready;
        budget.charge_source_bytes(0, 0, selected.future_manifest_bytes.len(), 0)?;
        if comparable != selected.future_manifest {
            return Err(SourceUpgradeReadyError::ConfigurationMismatch);
        }
        let bundle_path = selected.origin.bundle_path();
        let bundle_file = verify_named(
            root,
            bundle_path.as_str(),
            None,
            selected.bundle_digest,
            budget,
            SourceUpgradeReadySource::Bundle,
        )?;
        if bundle_file.parent_identity() != selected.origin.bundle_parent_identity() {
            return Err(SourceUpgradeReadyError::SourceChanged(
                SourceUpgradeReadySource::Bundle,
            ));
        }
        let future_bundle_file = verify_named(
            root,
            bundle_path.as_str(),
            Some((bundle_file.identity(), bundle_file.parent_identity())),
            selected.bundle_digest,
            budget,
            SourceUpgradeReadySource::Bundle,
        )?;
        let storage = add(
            size_of::<Self>(),
            mul(
                2,
                add(
                    size_of::<AuthoringSchemaReceipt>(),
                    size_of::<AdmittedAuthoringManifest>(),
                )?,
            )?,
        )?;
        let paths = mul(bundle_path.as_str().len(), 2)?;
        budget.charge_source_bytes(0, 0, paths, add(storage, paths)?)?;
        let bundle = |file| {
            AdmittedAuthoringSchemaBundle::from_observation(
                bundle_path.clone(),
                file,
                Arc::clone(&selected.bundle_bytes),
                selected.bundle_digest,
                Arc::clone(&selected.bundle),
            )
        };
        let pending_capture_budget = ordinary_schema_capture_budget(
            selected.capture_budget.limits,
            manifest.1.len(),
            selected.bundle_bytes.len(),
            bundle_path.as_str().len(),
            selected.models_retained_bytes,
        )?;
        let pending = Arc::new(AuthoringSchemaReceipt {
            attempt: selected.origin.attempt(),
            project_root: root.share(),
            manifest: Arc::new(AdmittedAuthoringManifest {
                capability: OnceLock::from(manifest.0),
                digest: ContentDigest::of_bytes(&manifest.1),
                bytes: Arc::from(manifest.1),
            }),
            settings: pending_settings,
            bundle: bundle(bundle_file),
            budget: pending_capture_budget,
        });
        // This Arc and its manifest slot stay private. The independent observed receipt above
        // retains real Pending facts; it never aliases this unfilled future capability.
        let future = Arc::new(AuthoringSchemaReceipt {
            attempt: selected.origin.attempt(),
            project_root: root.share(),
            manifest: Arc::new(AdmittedAuthoringManifest {
                capability: OnceLock::new(),
                bytes: Arc::clone(&selected.future_manifest_bytes),
                digest: selected.future_manifest_digest,
            }),
            settings: selected.future_settings.clone(),
            bundle: bundle(future_bundle_file),
            budget: selected.capture_budget,
        });
        Ok(ReadyReceiptStorage {
            recovery_path,
            pending,
            future,
            recovery_file,
            recovery_digest,
        })
    }

    /// A private upgrade capture input containing actual Pending bytes/settings and the actual
    /// successor bundle leaf. It grants no ordinary Ready admission or project adoption.
    pub(super) fn observed_pending_schema(&self) -> &Arc<AuthoringSchemaReceipt> {
        &self.pending
    }
    pub(super) fn basis(&self) -> &AuthoringSchemaBasis {
        self.selected.basis()
    }
    pub(super) fn adapters(&self) -> &[AuthoringAdapterReceipt] {
        self.selected.adapters()
    }
    pub(super) fn settings(&self) -> &EffectiveProjectSettings {
        self.selected.settings()
    }

    pub(super) fn validate(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeReadyError> {
        let root = &self.pending.project_root;
        verify_named(
            root,
            PROJECT_MANIFEST_PATH,
            Some((
                self.pending.manifest_identity(),
                self.pending.manifest_parent_identity(),
            )),
            self.pending.manifest_digest(),
            budget,
            SourceUpgradeReadySource::Manifest,
        )?;
        verify_named(
            root,
            self.pending.bundle_path().as_str(),
            Some((
                self.pending.bundle_identity(),
                self.pending.bundle_parent_identity(),
            )),
            self.pending.bundle_digest(),
            budget,
            SourceUpgradeReadySource::Bundle,
        )?;
        verify_named(
            root,
            &self.recovery_path,
            Some((
                self.recovery_file.identity(),
                self.recovery_file.parent_identity(),
            )),
            self.recovery_digest,
            budget,
            SourceUpgradeReadySource::Recovery,
        )?;
        Ok(())
    }

    /// Consumes this owner and binds its own publisher. No caller-supplied accepted publication
    /// can fill the future receipt, and no mutable publisher reference leaves the bound operation.
    pub(super) fn bind(
        self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<BoundSourceUpgradeReady, SourceUpgradeReadyBindFailure> {
        let result = with_candidate_storage(budget, |budget| {
            self.validate(budget).and_then(|()| {
                with_source_scratch(budget, |budget| {
                    budget.retain_open_handles(PUBLISHER_HANDLE_RESERVATION)?;
                    budget.charge_reference(PROJECT_MANIFEST_PATH.len(), 0, false)?;
                    budget.charge_open_file(HANDLE_PEAK)?;
                    budget.charge_source_bytes(
                        self.pending.manifest_bytes().len(),
                        0,
                        self.pending.manifest_bytes().len(),
                        size_of::<FilePublicationHost>(),
                    )?;
                    let publisher = FilePublicationHost::bind_existing(
                        &self.pending.project_root,
                        PROJECT_MANIFEST_PATH,
                        self.pending.manifest.capability(),
                        self.pending.manifest_digest(),
                        nara_project::DEFAULT_MANIFEST_BYTE_LIMIT,
                    )
                    .map_err(SourceUpgradeReadyError::Publisher)?;
                    if Some(publisher.parent_identity()) != self.pending.manifest_parent_identity()
                        || publisher.binding()
                            != (FilePublicationBinding::Existing {
                                identity: self.pending.manifest_identity(),
                                digest: self.pending.manifest_digest(),
                            })
                    {
                        return Err(SourceUpgradeReadyError::SourceChanged(
                            SourceUpgradeReadySource::Manifest,
                        ));
                    }
                    Ok(publisher)
                })
            })
        });
        match result {
            Ok(publisher) => Ok(BoundSourceUpgradeReady {
                prepared: self,
                publisher,
            }),
            Err(error) => Err(SourceUpgradeReadyBindFailure {
                prepared: self,
                error,
            }),
        }
    }
}

/// Only this consumed owner can perform the last manifest transition for its prepared Schema.
pub(super) struct BoundSourceUpgradeReady {
    prepared: PreparedSourceUpgradeReady,
    publisher: FilePublicationHost,
}

impl BoundSourceUpgradeReady {
    pub(super) fn basis(&self) -> &AuthoringSchemaBasis {
        self.prepared.basis()
    }
    pub(super) fn observed_pending_schema(&self) -> &Arc<AuthoringSchemaReceipt> {
        self.prepared.observed_pending_schema()
    }
    pub(super) fn cleanup_pending(&self) -> bool {
        self.publisher.has_cleanup()
    }
    pub(super) fn uncertain(&self) -> bool {
        self.publisher.uncertain()
    }
    pub(super) fn retry_cleanup(&mut self) -> bool {
        self.publisher.retry_cleanup()
    }

    #[cfg(test)]
    pub(super) fn test_fail_next_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        self.publisher.test_fail_after_write()
    }

    /// The enclosing operation must keep all project/Workspace/lane guards alive across this
    /// call. On Published, semantic installation is already prepared and must be infallible.
    pub(super) fn publish(
        mut self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<SourceUpgradeReadyOutcome, SourceUpgradeReadyPublicationRefusal> {
        let reservation = match with_candidate_storage(budget, |budget| {
            self.prepared
                .validate(budget)
                .and_then(|()| self.reserve_publication(budget))
        }) {
            Ok(reservation) => reservation,
            Err(error) => return Err(SourceUpgradeReadyPublicationRefusal { ready: self, error }),
        };
        // Unique private receipt ownership is established before publication. Published needs
        // only a slot write and moves; there is no Arc allocation, parser or readmission here.
        Arc::get_mut(&mut self.prepared.future)
            .expect("the future Ready receipt has never escaped its private owner");
        let outcome = self
            .publisher
            .publish(&self.prepared.selected.future_manifest_bytes);
        budget.retained_candidate_bytes = budget
            .retained_candidate_bytes
            .checked_sub(reservation)
            .expect("publication scratch remains charged until the file owner returns");
        match outcome {
            FilePublicationOutcome::Published(accepted) => {
                let (publication, source) = accepted.into_parts();
                let (capability, _) = source.into_parts();
                let manifest = &self.prepared.future.manifest;
                let filled = manifest.capability.set(capability);
                assert!(
                    filled.is_ok(),
                    "only this bound Ready publisher fills its private manifest slot"
                );
                // One publisher slot now backs the actual Ready leaf. Its old Pending read
                // and this owner's private recovery read have ended. Pending Schema shares
                // remain charged until their owning Session/verification scopes end.
                drop(self.publisher);
                drop(self.prepared.recovery_file);
                budget.release_open_handles(2);
                release_pending_observation(self.prepared.pending, budget);
                let selected = self.prepared.selected;
                Ok(SourceUpgradeReadyOutcome::Published(
                    PublishedSourceUpgradeReady {
                        schema: self.prepared.future,
                        adapters: selected.adapters,
                        basis: selected.upgrade.successor_basis().clone(),
                        publication,
                        usage: budget.usage(),
                    },
                ))
            }
            FilePublicationOutcome::Rejected(reason) => Ok(SourceUpgradeReadyOutcome::Rejected {
                ready: self,
                reason,
            }),
            FilePublicationOutcome::Failed(stage) => {
                Ok(SourceUpgradeReadyOutcome::Failed { ready: self, stage })
            }
            FilePublicationOutcome::Uncertain(publication) => {
                Ok(SourceUpgradeReadyOutcome::Uncertain {
                    ready: self,
                    publication,
                })
            }
        }
    }

    fn reserve_publication(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<usize, SourceUpgradeReadyError> {
        // The file owner hashes the incumbent and candidate, checks the resulting name and
        // verifies the retained candidate. It owns the physical publication/evidence tier.
        let ready = self.prepared.selected.future_manifest_bytes.len();
        let cost = self
            .publisher
            .binding()
            .checked_content_cost(ready)
            .ok_or(SourceUpgradeReadyError::AccountingOverflow)?;
        budget.charge_open_file(HANDLE_PEAK)?;
        budget.charge_reference(PROJECT_MANIFEST_PATH.len(), 0, false)?;
        let reservation = add(SOURCE_SCRATCH, ready)?;
        budget.charge_source_bytes(
            cost.encoded_read_bytes,
            0,
            cost.content_work_bytes,
            reservation,
        )?;
        Ok(reservation)
    }
}

pub(super) struct PublishedSourceUpgradeReady {
    pub(super) schema: Arc<AuthoringSchemaReceipt>,
    pub(super) adapters: Box<[AuthoringAdapterReceipt]>,
    pub(super) basis: AuthoringSchemaBasis,
    pub(super) publication: FilePublicationEvidence,
    pub(super) usage: AuthoringProjectAdmissionUsage,
}

impl PublishedSourceUpgradeReady {
    /// Moves the confirmed receipt into the already-bound Session. Its previous Pending
    /// observation is released while the operation keeps its own cumulative usage evidence.
    /// No admission, allocation or filesystem work remains after final publication.
    pub(super) fn install_schema(
        self,
        current: &mut Arc<AuthoringSchemaReceipt>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> (FilePublicationEvidence, AuthoringProjectAdmissionUsage) {
        let pending = std::mem::replace(current, self.schema);
        release_pending_observation(pending, budget);
        (self.publication, budget.usage())
    }
}

/// Only upgrade-specific Pending receipts reach this installation path. Their manifest and
/// bundle observations were charged together before preparation. Escaped shares keep those
/// two native handles alive and charged; this owner releases them only when its Arc is last.
fn release_pending_observation(
    pending: Arc<AuthoringSchemaReceipt>,
    budget: &mut AuthoringProjectAdmissionBudget,
) {
    if let Some(receipt) = Arc::into_inner(pending) {
        drop(receipt);
        budget.release_open_handles(2);
    }
}

#[must_use = "retain the exclusive publisher and truthful per-file outcome for reconciliation"]
pub(super) enum SourceUpgradeReadyOutcome {
    Published(PublishedSourceUpgradeReady),
    Rejected {
        ready: BoundSourceUpgradeReady,
        reason: FilePublicationRejection,
    },
    Failed {
        ready: BoundSourceUpgradeReady,
        stage: FilePublicationFailureStage,
    },
    Uncertain {
        ready: BoundSourceUpgradeReady,
        publication: FilePublicationEvidence,
    },
}

pub(super) struct SourceUpgradeReadyBindFailure {
    pub(super) prepared: PreparedSourceUpgradeReady,
    pub(super) error: SourceUpgradeReadyError,
}

pub(super) struct SourceUpgradeReadyPreparationFailure {
    pub(super) selected: PreparedSourceUpgradeSchema,
    pub(super) error: SourceUpgradeReadyError,
}

pub(super) struct SourceUpgradeReadyPublicationRefusal {
    pub(super) ready: BoundSourceUpgradeReady,
    pub(super) error: SourceUpgradeReadyError,
}

impl std::fmt::Debug for SourceUpgradeReadyBindFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceUpgradeReadyBindFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for SourceUpgradeReadyPreparationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceUpgradeReadyPreparationFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for SourceUpgradeReadyPublicationRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceUpgradeReadyPublicationRefusal")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceUpgradeReadySource {
    Manifest,
    Bundle,
    Recovery,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceUpgradeReadyError {
    #[error("the actual recovery storage changed")]
    RecoveryStorage(#[source] super::recovery_storage::RecoveryStorageError),
    #[error("the complete reopened mixed source inventory was refused")]
    RecoveredInventory(super::source_upgrade_inventory::SourceUpgradeInventoryError),
    #[error("the reviewed original Schema does not match the admitted project")]
    OriginalSchemaMismatch,
    #[error("the actual project manifest is not Pending")]
    NotPending,
    #[error("the upgrade's complete project configuration changed")]
    ConfigurationMismatch,
    #[error("the Pending recovery package differs from the selected upgrade")]
    RecoveryMismatch,
    #[error("the actual {0:?} leaf, parent, name or digest changed")]
    SourceChanged(SourceUpgradeReadySource),
    #[error("the complete manifest is invalid")]
    ManifestInvalid,
    #[error("authoring Adapter preparation failed")]
    Adapters(#[source] FirstPartyAuthoringAdapterError),
    #[error("successor authoring basis admission failed")]
    Basis(#[source] AuthoringDocumentPublicationError),
    #[error("the selected authored-field upgrade is not valid")]
    Upgrade(ScenePatchReport),
    #[error("the exclusive Ready publisher could not bind Pending: {0:?}")]
    Publisher(FilePublicationFailureStage),
    #[error("authored-upgrade Schema admission exceeded its shared budget")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("authored-upgrade Schema accounting overflowed")]
    AccountingOverflow,
    #[error(
        "actual selected Schema storage ({actual} bytes) exceeds its admitted preparation reservation ({reserved} bytes)"
    )]
    SchemaPreparationBoundExceeded { reserved: usize, actual: usize },
}

impl From<SchemaModelsError> for SourceUpgradeReadyError {
    fn from(error: SchemaModelsError) -> Self {
        match error {
            SchemaModelsError::Adapters(error) => Self::Adapters(error),
            SchemaModelsError::Basis(error) => Self::Basis(error),
            SchemaModelsError::Admission(error) => Self::Admission(error),
            SchemaModelsError::AccountingOverflow => Self::AccountingOverflow,
            SchemaModelsError::SchemaPreparationBoundExceeded { reserved, actual } => {
                Self::SchemaPreparationBoundExceeded { reserved, actual }
            }
        }
    }
}

fn verify_named(
    root: &DirectoryCapability,
    path: &str,
    expected: Option<(FileIdentity, Option<FileIdentity>)>,
    digest: ContentDigest,
    budget: &mut AuthoringProjectAdmissionBudget,
    kind: SourceUpgradeReadySource,
) -> Result<FileCapability, SourceUpgradeReadyError> {
    with_source_scratch(budget, |budget| {
        budget.charge_reference(path.len(), 1, true)?;
        budget.charge_open_file(HANDLE_PEAK)?;
        let bytes = usize::try_from(digest.length())
            .map_err(|_| SourceUpgradeReadyError::AccountingOverflow)?;
        budget.charge_source_bytes(bytes, 0, add(bytes, path.len())?, 0)?;
        let path =
            RelativePath::new(path).map_err(|_| SourceUpgradeReadyError::SourceChanged(kind))?;
        let file = root
            .open_file_exact(&path)
            .map_err(|_| SourceUpgradeReadyError::SourceChanged(kind))?;
        if expected.is_some_and(|(identity, parent)| {
            file.identity() != identity || file.parent_identity() != parent
        }) || file.verify_digest(digest, digest.length()).is_err()
        {
            return Err(SourceUpgradeReadyError::SourceChanged(kind));
        }
        Ok(file)
    })
}

fn read_pending_manifest(
    root: &DirectoryCapability,
    expected: SourceUpgradePendingManifest<'_>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(FileCapability, Vec<u8>), SourceUpgradeReadyError> {
    with_source_scratch(budget, |budget| {
        budget.charge_reference(PROJECT_MANIFEST_PATH.len(), 0, false)?;
        budget.charge_open_file(HANDLE_PEAK)?;
        let (limit, _) = budget.read_limit(nara_project::DEFAULT_MANIFEST_BYTE_LIMIT as usize)?;
        // The bounded reader may retry eight times. Admit its maximum work before any read,
        // then retain only one byte buffer; failed work remains in the same controller.
        budget.charge_source_bytes(limit, 0, mul(limit, 8)?, limit)?;
        let path =
            RelativePath::new(PROJECT_MANIFEST_PATH).expect("the fixed manifest path is valid");
        let file = root.open_file_exact(&path).map_err(|_| {
            SourceUpgradeReadyError::SourceChanged(SourceUpgradeReadySource::Manifest)
        })?;
        if file.parent_identity() != Some(root.identity())
            || file.identity() != expected.capability.identity()
            || file.parent_identity() != expected.capability.parent_identity()
        {
            return Err(SourceUpgradeReadyError::SourceChanged(
                SourceUpgradeReadySource::Manifest,
            ));
        }
        let bytes = file.read_to_end_bounded(limit as u64).map_err(|_| {
            SourceUpgradeReadyError::SourceChanged(SourceUpgradeReadySource::Manifest)
        })?;
        // The buffer's capacity was fixed by nara_fs's bounded snapshot reader. Release the
        // unused reservation, keeping cumulative IO/work admission untouched.
        budget.retained_candidate_bytes -= limit - bytes.len();
        budget.charge_source_bytes(0, 0, bytes.len(), 0)?;
        if ContentDigest::of_bytes(&bytes) != expected.digest {
            return Err(SourceUpgradeReadyError::SourceChanged(
                SourceUpgradeReadySource::Manifest,
            ));
        }
        Ok((file, bytes))
    })
}

fn parse_manifest(
    bytes: &[u8],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ProjectManifest, SourceUpgradeReadyError> {
    // nara_project's bounded owner admits at most 4,096 nodes. Reserve complete profile and
    // resolved-settings storage per possible node; no duplicate manifest parser is introduced.
    let nodes = bytes.len().min(4_096);
    let slot = add(
        size_of::<nara_project::ProjectProfileOverlay>(),
        size_of::<EffectiveProjectSettings>(),
    )?;
    let retained = add(
        mul(bytes.len(), 8)?,
        add(mul(nodes, slot)?, size_of::<ProjectManifest>())?,
    )?;
    budget.charge_source_bytes(0, retained, mul(bytes.len(), 4)?, retained)?;
    let parsed = ProjectManifest::parse_toml_bytes(bytes);
    if parsed.has_errors() {
        return Err(SourceUpgradeReadyError::ManifestInvalid);
    }
    parsed
        .manifest
        .ok_or(SourceUpgradeReadyError::ManifestInvalid)
}

/// Refusal drops all newly prepared models before releasing their storage. Existing selected
/// owners stay charged, and actual encoded/decoded/work observations remain cumulative.
fn with_candidate_storage<T>(
    budget: &mut AuthoringProjectAdmissionBudget,
    prepare: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, SourceUpgradeReadyError>,
) -> Result<T, SourceUpgradeReadyError> {
    let retained_before = budget.retained_candidate_bytes;
    let handles_before = budget.retained_open_handles;
    let result = prepare(budget);
    if result.is_err() {
        let temporary = budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .ok_or(SourceUpgradeReadyError::AccountingOverflow)?;
        budget
            .reconcile_retained_reservation(temporary, 0)
            .map_err(|_| SourceUpgradeReadyError::AccountingOverflow)?;
        let closed = budget
            .retained_open_handles
            .checked_sub(handles_before)
            .ok_or(SourceUpgradeReadyError::AccountingOverflow)?;
        budget.release_open_handles(closed);
    }
    result
}

fn with_source_scratch<T>(
    budget: &mut AuthoringProjectAdmissionBudget,
    operation: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, SourceUpgradeReadyError>,
) -> Result<T, SourceUpgradeReadyError> {
    let before = budget.retained_candidate_bytes;
    if let Err(error) = budget.charge_source_bytes(0, 0, 0, SOURCE_SCRATCH) {
        let refused = budget
            .retained_candidate_bytes
            .checked_sub(before)
            .expect("refused scratch admission has allocated no storage");
        budget.retained_candidate_bytes -= refused;
        return Err(error.into());
    }
    let result = operation(budget);
    budget.retained_candidate_bytes = budget
        .retained_candidate_bytes
        .checked_sub(SOURCE_SCRATCH)
        .expect("the admitted source scratch is retained until its operation returns");
    result
}

fn add(left: usize, right: usize) -> Result<usize, SourceUpgradeReadyError> {
    left.checked_add(right)
        .ok_or(SourceUpgradeReadyError::AccountingOverflow)
}
fn mul(left: usize, right: usize) -> Result<usize, SourceUpgradeReadyError> {
    left.checked_mul(right)
        .ok_or(SourceUpgradeReadyError::AccountingOverflow)
}
fn content_digest(digest: ProjectAuthoringRecoveryDigest) -> ContentDigest {
    ContentDigest::from_parts(digest.length(), *digest.as_bytes())
}
