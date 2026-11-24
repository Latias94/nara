//! Prepares a private saved Project session before the final Ready name switch.

use std::{
    mem::size_of,
    sync::{Arc, OnceLock},
};

use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError, PathValidationError,
    RelativePath,
};
use nara_project::{EffectiveProjectSettings, ProjectAuthoringState};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringSchemaBasis, SceneFileLimits, SourceDocumentKey,
};

#[cfg(feature = "presentation-2d")]
use super::super::reserve_preview_identity_store;
use super::super::{
    AUTHORING_BASIS_ENCODED_MULTIPLIER, AuthoringProjectAdmissionBudget,
    AuthoringProjectAdmissionError, AuthoringProjectAdmissionUsage,
    AuthoringProjectSessionSnapshot, AuthoringProjectSnapshotInner,
    AuthoringProjectSourceAdmissionError, AuthoringSchemaReceipt, AuthoringSourceFact,
    ProjectSourceIndexState, SOURCE_OPEN_HANDLE_PEAK, capture, capture_saved_startup,
    checked_product, checked_sum, read_saved_startup,
};
use super::publication::{AcceptedComponentApplyOutput, ComponentApplyReadyInput};
use crate::{
    project_host::{
        authoring::{
            AdmittedAuthoringManifest, PROJECT_MANIFEST_PATH,
            bundle::AdmittedAuthoringSchemaBundle,
            recovery_storage::RecoveryPackageData,
            schema_models::{
                SchemaModelsError, ordinary_schema_capture_budget, selected_schema_retained_bytes,
            },
            source_inventory::{SourceInventory, SourceInventoryError, with_candidate_storage},
        },
        file_publication::AcceptedFilePublication,
    },
    scene_retention::ARC_CONTROL_BLOCK_BYTES,
};

#[derive(Debug, thiserror::Error)]
pub(in crate::project_host) enum ComponentApplyReadyError {
    #[error("the Pending project no longer matches the original Ready configuration or Schema")]
    Configuration,
    #[error("the saved startup does not match the authorized Component Apply files")]
    SavedGeneration,
    #[error("the private Project candidate lost its exclusive Schema or manifest storage")]
    PrivateStorage,
    #[error("the Ready session exceeds the original source operation allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("the final saved startup could not be captured")]
    Source(#[from] AuthoringProjectSourceAdmissionError),
    #[error("the final source inventory does not match Pending")]
    Inventory(#[from] SourceInventoryError),
    #[error("the original retained Schema models exceed ordinary capture limits")]
    Models,
    #[error("a retained Schema source changed while preparing Ready")]
    File(#[from] FsError),
    #[error("a retained Schema locator is invalid")]
    Path(#[from] PathValidationError),
}

impl From<SchemaModelsError> for ComponentApplyReadyError {
    fn from(error: SchemaModelsError) -> Self {
        match error {
            SchemaModelsError::Admission(error) => Self::Admission(error),
            SchemaModelsError::AccountingOverflow => {
                Self::Source(AuthoringProjectSourceAdmissionError::AccountingOverflow)
            }
            _ => Self::Models,
        }
    }
}

/// Owns the unpublished state and the allocation that will receive the actual Ready leaf.
#[must_use]
pub(super) struct PreparedComponentApplyReady {
    session: AuthoringProjectSessionSnapshot,
    manifest: Arc<AdmittedAuthoringManifest>,
    pending_manifest_storage: usize,
    continuing: AuthoringProjectAdmissionUsage,
}

impl PreparedComponentApplyReady {
    pub(super) fn prepare(
        original: &Arc<AuthoringSchemaReceipt>,
        basis: &AuthoringSchemaBasis,
        adapters: Arc<[AuthoringAdapterReceipt]>,
        scene_limits: SceneFileLimits,
        input: ComponentApplyReadyInput<'_>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, ComponentApplyReadyError> {
        with_candidate_storage(budget, |budget| {
            let ComponentApplyReadyInput {
                pending,
                physical,
                outputs,
            } = input;
            let inventory = physical.inventory();
            let recovery = pending.recovery();
            let bundle = recovery.unchanged_bundle();
            inventory.validate_manifest_observation(
                pending.root().identity(),
                pending.capability().identity(),
                pending.digest(),
            )?;
            let ready_bytes = recovery.final_ready_manifest();
            budget.charge_source_bytes(
                0,
                0,
                checked_sum([
                    ready_bytes.len(),
                    original.bundle_bytes().len(),
                    original.bundle_path().as_str().len(),
                    pending.manifest_bytes().len(),
                ])?,
                0,
            )?;
            if original.root_identity() != pending.root().identity()
                || original.manifest_parent_identity() != pending.capability().parent_identity()
                || original.settings().authoring_state != ProjectAuthoringState::Ready
                || original.settings().project.source_universe != recovery.source_universe()
                || original.settings().profile_name.as_deref() != recovery.selected_profile()
                || scene_limits != inventory.scene_limits()
                || ready_bytes != original.manifest_bytes()
                || bundle.path() != original.bundle_path().as_str()
                || bundle.blob().bytes() != original.bundle_bytes()
                || physical.bundle_digest() != original.bundle_digest()
                || physical.bundle_file().identity() != original.bundle_identity()
                || physical.bundle_file().parent_identity() != original.bundle_parent_identity()
            {
                return Err(ComponentApplyReadyError::Configuration);
            }
            for ((source, blob), output) in recovery.outputs().into_iter().zip(outputs) {
                budget.charge_source_bytes(
                    0,
                    0,
                    checked_sum([source.path().len(), source.key().retained_bytes()])?,
                    0,
                )?;
                if output.path() != source.path()
                    || output.key() != source.key()
                    || output.digest() != blob.digest()
                    || output.file().parent_identity() != Some(output.parent().identity())
                {
                    return Err(ComponentApplyReadyError::SavedGeneration);
                }
            }

            let pending_manifest_storage = checked_sum([
                size_of::<AdmittedAuthoringManifest>(),
                2 * ARC_CONTROL_BLOCK_BYTES,
                pending.manifest_bytes().len(),
            ])?;
            let settings_bytes = checked_product(pending.manifest_bytes().len(), 128)?;
            let private_storage = checked_sum([
                size_of::<Self>(),
                size_of::<AuthoringSchemaReceipt>(),
                size_of::<AdmittedAuthoringManifest>(),
                2 * ARC_CONTROL_BLOCK_BYTES,
                pending_manifest_storage,
                settings_bytes,
                original.bundle_path().as_str().len(),
            ])?;
            budget.charge_source_bytes(0, 0, private_storage, private_storage)?;
            let mut settings: EffectiveProjectSettings = original.settings().clone();
            settings.authoring_state = ProjectAuthoringState::Pending(pending.reference().clone());
            if &settings != physical.settings() {
                return Err(ComponentApplyReadyError::Configuration);
            }
            budget.charge_source_bytes(
                0,
                0,
                checked_product(
                    original.bundle_bytes().len(),
                    AUTHORING_BASIS_ENCODED_MULTIPLIER,
                )?,
                0,
            )?;
            let models = selected_schema_retained_bytes(original.bundle(), &adapters)?;
            let ordinary = ordinary_schema_capture_budget(
                original.limits(),
                ready_bytes.len(),
                original.bundle_bytes().len(),
                original.bundle_path().as_str().len(),
                models,
            )?;
            let manifest_file = retain_observed(
                pending.root(),
                PROJECT_MANIFEST_PATH,
                pending.capability(),
                pending.digest(),
                budget,
            )?;
            let bundle_file = retain_observed(
                pending.root(),
                bundle.path(),
                physical.bundle_file(),
                physical.bundle_digest(),
                budget,
            )?;
            let schema = Arc::new(AuthoringSchemaReceipt {
                attempt: original.attempt_generation(),
                project_root: pending.root().share(),
                manifest: Arc::new(AdmittedAuthoringManifest {
                    capability: OnceLock::from(manifest_file),
                    bytes: Arc::from(pending.manifest_bytes()),
                    digest: pending.digest(),
                }),
                settings,
                bundle: AdmittedAuthoringSchemaBundle::from_observation(
                    original.bundle_path().clone(),
                    bundle_file,
                    original.bundle.shared_bytes(),
                    original.bundle_digest(),
                    original.bundle.shared_bundle(),
                ),
                budget: ordinary,
            });
            let manifest = Arc::new(AdmittedAuthoringManifest {
                capability: OnceLock::new(),
                bytes: Arc::clone(&original.manifest.bytes),
                digest: original.manifest_digest(),
            });
            let before = budget.usage();
            let mut session = prepare_session(schema, basis, adapters, scene_limits, budget)?;
            require_saved_startup_generation(&session, inventory, outputs, budget)?;

            let captured = budget.usage();
            let mut continuing = ordinary;
            let retained = captured
                .retained_candidate_bytes()
                .checked_sub(before.retained_candidate_bytes())
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
            let handles = captured
                .retained_open_handles()
                .checked_sub(before.retained_open_handles())
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
            continuing.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([size_of::<AuthoringProjectSessionSnapshot>(), retained])?,
            )?;
            continuing.retain_open_handles(handles)?;
            session.usage = continuing.usage();
            if Arc::get_mut(&mut session.schema).is_none() {
                return Err(ComponentApplyReadyError::PrivateStorage);
            }
            let owner = Self {
                session,
                manifest,
                pending_manifest_storage,
                continuing: continuing.usage(),
            };
            owner.validate(budget)?;
            Ok(owner)
        })
    }

    #[cfg(test)]
    fn bytes(&self) -> &[u8] {
        &self.manifest.bytes
    }

    pub(super) fn validate(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), ComponentApplyReadyError> {
        self.session
            .startup_source_validation()
            .validate_with_budget(budget)?;
        Ok(())
    }

    /// Called with the accepted publication from this operation's Pending-to-Ready publisher.
    pub(super) fn install(
        mut self,
        accepted: AcceptedFilePublication,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> AuthoringProjectSessionSnapshot {
        let schema =
            Arc::get_mut(&mut self.session.schema).expect("private Component Apply Schema");
        assert_eq!(accepted.target().to_str(), Some(PROJECT_MANIFEST_PATH));
        assert_eq!(
            accepted.evidence().receipt().previous_identity(),
            Some(schema.manifest_identity()),
        );
        assert_eq!(
            accepted.source().parent_identity(),
            schema.manifest_parent_identity(),
        );
        let (_, source) = accepted.into_parts();
        let (file, digest) = source.into_parts();
        assert_eq!(digest, self.manifest.digest);
        assert!(
            self.manifest.capability.set(file).is_ok(),
            "exclusive Ready manifest slot"
        );
        schema.manifest = self.manifest;
        schema.settings.authoring_state = ProjectAuthoringState::Ready;
        self.session
            .source_index
            .commit_manifest_publication(schema.manifest_identity(), digest);
        budget.release_open_handles(1);
        budget
            .reconcile_retained_reservation(self.pending_manifest_storage, 0)
            .expect("the private Pending manifest reservation remained charged until installation");
        self.session.usage = AuthoringProjectAdmissionUsage {
            retained_candidate_bytes: self.continuing.retained_candidate_bytes(),
            retained_open_handles: self.continuing.retained_open_handles(),
            ..budget.usage()
        };
        self.session
    }
}

fn prepare_session(
    schema: Arc<AuthoringSchemaReceipt>,
    basis: &AuthoringSchemaBasis,
    adapters: Arc<[AuthoringAdapterReceipt]>,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<AuthoringProjectSessionSnapshot, ComponentApplyReadyError> {
    let conversion_storage =
        size_of::<AuthoringProjectSnapshotInner>() + 2 * ARC_CONTROL_BLOCK_BYTES;
    budget.charge_source_bytes(0, 0, 0, conversion_storage)?;
    let read = read_saved_startup(&schema, limits, budget)?;
    let mut startup = capture_saved_startup(
        &schema,
        limits,
        read,
        basis,
        schema.bundle_bytes().len(),
        budget,
        &[],
        &[],
        capture::AuthoringCapturePreview::Prepare,
    )?;
    #[cfg(feature = "presentation-2d")]
    reserve_preview_identity_store(&startup.capture, budget)?;
    let index_bytes = ProjectSourceIndexState::checked_retained_bytes_for_schema(&schema)?;
    budget.charge_source_bytes(0, 0, index_bytes, index_bytes)?;
    startup.capture.usage = budget.usage();
    let mut session = startup
        .into_candidate(schema, Box::default(), limits, budget.usage())
        .publish()
        .into_session_snapshot()
        .map_err(|_| ComponentApplyReadyError::PrivateStorage)?;
    session.adapters = adapters;
    budget.reconcile_retained_reservation(conversion_storage, 0)?;
    if session.startup_scene.schema_basis() != *basis {
        return Err(ComponentApplyReadyError::Configuration);
    }
    Ok(session)
}

fn retain_observed(
    root: &DirectoryCapability,
    path: &str,
    observed: &FileCapability,
    digest: ContentDigest,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<FileCapability, ComponentApplyReadyError> {
    let bytes =
        usize::try_from(digest.length()).map_err(|_| SourceInventoryError::AccountingOverflow)?;
    let scratch = checked_sum([65_536, checked_product(path.len(), 2)?])?;
    let path_shape = RelativePath::preflight(path)?;
    budget.retain_open_handles(1)?;
    budget.charge_open_file(SOURCE_OPEN_HANDLE_PEAK)?;
    budget.charge_reference(path.len(), path_shape.components(), false)?;
    budget.charge_source_bytes(bytes, 0, checked_sum([bytes, path.len(), 1])?, scratch)?;
    let file = root.open_file_exact(&RelativePath::new(path)?)?;
    if file.identity() != observed.identity()
        || file.parent_identity() != observed.parent_identity()
    {
        return Err(ComponentApplyReadyError::Configuration);
    }
    file.verify_digest(digest, digest.length())?;
    budget.reconcile_retained_reservation(scratch, 0)?;
    Ok(file)
}

fn require_saved_startup_generation(
    session: &AuthoringProjectSessionSnapshot,
    inventory: &SourceInventory,
    outputs: [&AcceptedComponentApplyOutput; 2],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ComponentApplyReadyError> {
    let records = inventory
        .participants()
        .len()
        .max(inventory.metadata().len());
    let lookup_depth = (usize::BITS - records.leading_zeros()) as usize + 1;
    let sources = session
        .scenes
        .iter()
        .map(|scene| (scene.source(), scene.native_source().key()))
        .chain(
            session
                .prefabs
                .iter()
                .map(|prefab| (prefab.source(), prefab.native_source().key())),
        );
    for (fact, key) in sources {
        let path = fact.project_path().as_str();
        admit_lookup(path, Some(key), lookup_depth, budget)?;
        let matches = if let Some(output) = outputs.iter().find(|output| output.path() == path) {
            key == output.key()
                && observation(fact)
                    == (
                        output.file().identity(),
                        output.file().parent_identity(),
                        output.digest(),
                    )
        } else {
            inventory.participant(path).is_some_and(|member| {
                let file = member.file();
                key == member.source_key()
                    && observation(fact) == (file.identity(), file.parent_identity(), file.digest())
            })
        };
        if !matches {
            return Err(ComponentApplyReadyError::SavedGeneration);
        }
    }
    for fact in session.source_facts() {
        let path = fact.project_path().as_str();
        if let Some(source_path) = path.strip_suffix(".meta") {
            admit_lookup(source_path, None, lookup_depth, budget)?;
            if let Some(metadata) = inventory.metadata_for_source(source_path) {
                let file = metadata.file();
                if observation(fact) != (file.identity(), file.parent_identity(), file.digest()) {
                    return Err(ComponentApplyReadyError::SavedGeneration);
                }
            }
        }
    }
    Ok(())
}

fn admit_lookup(
    path: &str,
    key: Option<&SourceDocumentKey>,
    lookup_depth: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), ComponentApplyReadyError> {
    let bytes = checked_sum([
        checked_product(checked_sum([path.len(), 1])?, lookup_depth + 2)?,
        key.map_or(0, SourceDocumentKey::retained_bytes),
    ])?;
    budget.charge_source_bytes(0, 0, bytes, 0)?;
    Ok(())
}

fn observation(fact: &AuthoringSourceFact) -> (FileIdentity, Option<FileIdentity>, ContentDigest) {
    (fact.identity(), fact.parent_identity(), fact.digest())
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
