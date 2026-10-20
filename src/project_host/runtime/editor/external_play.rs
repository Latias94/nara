use std::{error::Error, fmt};

use nara_fs::{ContentDigest, RelativePath};
use nara_project::ProjectPath;
use nara_tooling::{EditorRuntimeAvailability, EditorRuntimeUnavailableReason};

use crate::project_content::{StartupContentMemberKind, startup_content_digest_from_members};
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, AuthoringProjectSessionSnapshot, AuthoringProjectSnapshot,
    AuthoringSourceKind, PreparedAuthoringProject, project_authoring_adapters,
};
use crate::project_host::product_launch::{
    EditorExternalPlaySourceFact, EditorExternalPlaySourceKind, EditorExternalPlaySourceReceipt,
    EditorExternalPlayTarget, SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
};

use super::{EditorProjectSession, single_error};
use nara_tooling::{
    EditorExternalPlayObservation, EditorExternalPlayOutput, EditorExternalPlayRequest,
    EditorExternalPlayState, EditorExternalPlayView, EditorPlayCommand, EditorPlayOperationResult,
    EditorPlayRejection, EditorPlayRequestResult,
};

/// Capability-free source authority admitted for one supervised desktop launch.
///
/// The decoded receipt remains the single semantic source of project/profile and strict Runtime
/// expectations. Canonical bytes are retained separately for immutable generation identity and the
/// child bootstrap envelope. Neither value contains source capabilities or absolute paths.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisedDesktopSourceAuthority {
    canonical_source_receipt: Box<[u8]>,
    receipt: EditorExternalPlaySourceReceipt,
}

/// Typed failure while preparing an Editor-owned source receipt for supervised Play.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorExternalPlayReceiptError {
    NotAuthoringProject,
    ContractMissing,
    ContractUnsupported,
    ProjectIdentityMissing,
    CargoSourceUnavailable,
    StartupClosureRejected,
    SourceBasisChanged,
    SourceReceiptRejected,
}

impl fmt::Display for EditorExternalPlayReceiptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotAuthoringProject => "the Editor project is not in the Authoring lane",
            Self::ContractMissing => {
                "the selected project profile does not declare a supervised Play contract"
            }
            Self::ContractUnsupported => {
                "the selected project profile declares an unsupported execution contract"
            }
            Self::ProjectIdentityMissing => {
                "the supervised Play project is missing project.stable_id"
            }
            Self::CargoSourceUnavailable => {
                "the Cargo manifest or lockfile could not be admitted for supervised Play"
            }
            Self::StartupClosureRejected => {
                "the saved startup content could not be admitted for supervised Play"
            }
            Self::SourceBasisChanged => {
                "the project manifest or authoring schema changed after the Editor session opened"
            }
            Self::SourceReceiptRejected => {
                "the supervised Play source receipt could not be encoded canonically"
            }
        })
    }
}

impl Error for EditorExternalPlayReceiptError {}

/// Returns the neutral runtime availability advertised by an Authoring project.
pub(crate) fn authoring_runtime_availability(
    project: &AuthoringProjectSessionSnapshot,
) -> EditorRuntimeAvailability {
    let Some(contract) = project.settings().execution.contract.as_ref() else {
        return EditorRuntimeAvailability::Unavailable(
            EditorRuntimeUnavailableReason::SupervisedPlayContractMissing,
        );
    };
    if !contract.is_desktop_supervised_play_v1() {
        return EditorRuntimeAvailability::Unavailable(
            EditorRuntimeUnavailableReason::SupervisedPlayContractUnsupported,
        );
    }
    if project.settings().project.stable_id.is_none() {
        return EditorRuntimeAvailability::Unavailable(
            EditorRuntimeUnavailableReason::ProjectIdentityMissing,
        );
    }
    if !supervised_play_platform_supported() {
        return EditorRuntimeAvailability::Unavailable(
            EditorRuntimeUnavailableReason::SupervisedPlayPlatformUnsupported,
        );
    }
    EditorRuntimeAvailability::ExternalSupervised
}

pub(crate) fn build_supervised_desktop_source_receipt(
    project: &AuthoringProjectSessionSnapshot,
) -> Result<Vec<u8>, EditorExternalPlayReceiptError> {
    if !matches!(
        authoring_runtime_availability(project),
        EditorRuntimeAvailability::ExternalSupervised
    ) {
        return Err(match project.settings().execution.contract.as_ref() {
            None => EditorExternalPlayReceiptError::ContractMissing,
            Some(contract) if !contract.is_desktop_supervised_play_v1() => {
                EditorExternalPlayReceiptError::ContractUnsupported
            }
            Some(_) if project.settings().project.stable_id.is_none() => {
                EditorExternalPlayReceiptError::ProjectIdentityMissing
            }
            Some(_) => EditorExternalPlayReceiptError::StartupClosureRejected,
        });
    }

    let project_stable_id = project
        .settings()
        .project
        .stable_id
        .clone()
        .ok_or(EditorExternalPlayReceiptError::ProjectIdentityMissing)?;
    let profile_name = project
        .settings()
        .profile_name
        .as_deref()
        .unwrap_or("default")
        .to_owned();

    let current = readmit_current_authoring_project(project)?;
    if !authoring_basis_matches(project, &current) {
        return Err(EditorExternalPlayReceiptError::SourceBasisChanged);
    }

    let mut source_facts = Vec::with_capacity(current.source_facts().len() + 4);
    append_source_fact(
        EditorExternalPlaySourceKind::ProjectManifest,
        ProjectPath::new("nara.toml".to_owned())
            .map_err(|_| EditorExternalPlayReceiptError::SourceReceiptRejected)?,
        current.manifest_digest(),
        &mut source_facts,
    )?;
    append_source_fact(
        EditorExternalPlaySourceKind::AuthoringSchemaBundle,
        current.authoring_bundle_path().clone(),
        current.authoring_bundle_digest(),
        &mut source_facts,
    )?;

    let mut content_members = Vec::with_capacity(current.source_facts().len());
    for fact in current.source_facts() {
        let (source_kind, content_kind) = authoring_source_kinds(fact.kind());
        let path = fact.project_path().clone();
        append_source_fact(source_kind, path.clone(), fact.digest(), &mut source_facts)?;
        content_members.push((content_kind, path, fact.digest()));
    }

    append_cargo_fact(
        &current.project_root(),
        "Cargo.toml",
        EditorExternalPlaySourceKind::CargoManifest,
        &mut source_facts,
    )?;
    append_cargo_fact(
        &current.project_root(),
        "Cargo.lock",
        EditorExternalPlaySourceKind::CargoLock,
        &mut source_facts,
    )?;

    let content_digest =
        startup_content_digest_from_members(current.manifest_digest(), content_members)
            .map_err(|_| EditorExternalPlayReceiptError::StartupClosureRejected)?;
    let schema_fingerprint = parse_schema_fingerprint(&current.schema_fingerprint_hex())
        .ok_or(EditorExternalPlayReceiptError::SourceReceiptRejected)?;
    let receipt = EditorExternalPlaySourceReceipt::new(
        project_stable_id,
        profile_name,
        EditorExternalPlayTarget::Desktop,
        SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
        current.manifest_digest(),
        current.authoring_bundle_digest(),
        content_digest,
        schema_fingerprint,
        source_facts,
    )
    .map_err(|_| EditorExternalPlayReceiptError::SourceReceiptRejected)?;
    receipt
        .encode_canonical()
        .map_err(|_| EditorExternalPlayReceiptError::SourceReceiptRejected)
}

fn readmit_current_authoring_project(
    project: &AuthoringProjectSessionSnapshot,
) -> Result<AuthoringProjectSnapshot, EditorExternalPlayReceiptError> {
    let mut admission = AuthoringProjectAdmissionHost::default();
    let schema = admission
        .prepare(
            project.project_root(),
            project.settings().profile_name.as_deref(),
        )
        .and_then(|prepared| prepared.admit_bundle())
        .map_err(|_| EditorExternalPlayReceiptError::StartupClosureRejected)?
        .publish();
    let adapters = project_authoring_adapters(schema.bundle())
        .map_err(|_| EditorExternalPlayReceiptError::StartupClosureRejected)?;
    let receipt = PreparedAuthoringProject::new(schema, adapters)
        .admit_startup_closure()
        .map_err(|_| EditorExternalPlayReceiptError::StartupClosureRejected)?
        .publish();
    Ok(receipt.snapshot())
}

fn authoring_basis_matches(
    project: &AuthoringProjectSessionSnapshot,
    current: &AuthoringProjectSnapshot,
) -> bool {
    current.root_identity() == project.project_root().identity()
        && current.settings() == project.settings()
        && current.manifest_digest() == project.manifest_digest()
        && current.authoring_bundle_digest() == project.authoring_bundle_digest()
        && current.schema_fingerprint_hex() == project.schema_fingerprint_hex()
}

const fn authoring_source_kinds(
    kind: AuthoringSourceKind,
) -> (EditorExternalPlaySourceKind, StartupContentMemberKind) {
    match kind {
        AuthoringSourceKind::Scene => (
            EditorExternalPlaySourceKind::Scene,
            StartupContentMemberKind::Scene,
        ),
        AuthoringSourceKind::Prefab => (
            EditorExternalPlaySourceKind::Prefab,
            StartupContentMemberKind::Prefab,
        ),
        AuthoringSourceKind::AssetMetadata => (
            EditorExternalPlaySourceKind::AssetMetadata,
            StartupContentMemberKind::AssetMetadata,
        ),
        AuthoringSourceKind::AssetSource => (
            EditorExternalPlaySourceKind::AssetSource,
            StartupContentMemberKind::AssetSource,
        ),
    }
}

fn append_source_fact(
    kind: EditorExternalPlaySourceKind,
    path: ProjectPath,
    digest: ContentDigest,
    facts: &mut Vec<EditorExternalPlaySourceFact>,
) -> Result<(), EditorExternalPlayReceiptError> {
    let fact = EditorExternalPlaySourceFact::new(kind, path, digest)
        .map_err(|_| EditorExternalPlayReceiptError::SourceReceiptRejected)?;
    facts.push(fact);
    Ok(())
}

fn append_cargo_fact(
    root: &nara_fs::DirectoryCapability,
    path: &str,
    kind: EditorExternalPlaySourceKind,
    facts: &mut Vec<EditorExternalPlaySourceFact>,
) -> Result<(), EditorExternalPlayReceiptError> {
    let project_path = ProjectPath::new(path.to_owned())
        .map_err(|_| EditorExternalPlayReceiptError::SourceReceiptRejected)?;
    let relative = RelativePath::new(path)
        .map_err(|_| EditorExternalPlayReceiptError::SourceReceiptRejected)?;
    let file = root
        .open_file(&relative)
        .map_err(|_| EditorExternalPlayReceiptError::CargoSourceUnavailable)?;
    let digest = file
        .digest(64 * 1024 * 1024)
        .map_err(|_| EditorExternalPlayReceiptError::CargoSourceUnavailable)?;
    append_source_fact(kind, project_path, digest, facts)
}

fn parse_schema_fingerprint(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(bytes)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
const fn supervised_play_platform_supported() -> bool {
    true
}

#[cfg(windows)]
const fn supervised_play_platform_supported() -> bool {
    true
}

#[cfg(not(any(all(unix, target_os = "linux", target_arch = "x86_64"), windows)))]
const fn supervised_play_platform_supported() -> bool {
    false
}

impl SupervisedDesktopSourceAuthority {
    #[must_use]
    pub fn canonical_source_receipt(&self) -> &[u8] {
        &self.canonical_source_receipt
    }

    #[must_use]
    pub fn project_stable_id(&self) -> &str {
        self.receipt.project_stable_id()
    }

    #[must_use]
    pub fn profile_name(&self) -> &str {
        self.receipt.profile_name()
    }

    #[must_use]
    pub const fn manifest_digest(&self) -> ContentDigest {
        self.receipt.manifest_digest()
    }

    #[must_use]
    pub const fn authoring_bundle_digest(&self) -> ContentDigest {
        self.receipt.authoring_bundle_digest()
    }

    #[must_use]
    pub const fn content_digest(&self) -> ContentDigest {
        self.receipt.content_digest()
    }

    #[must_use]
    pub const fn schema_fingerprint(&self) -> [u8; 32] {
        self.receipt.schema_fingerprint()
    }

    /// Returns the admitted Cargo manifest digest carried by the source receipt.
    #[must_use]
    pub fn cargo_manifest_digest(&self) -> Option<ContentDigest> {
        self.receipt
            .source_facts()
            .iter()
            .find(|fact| fact.kind() == EditorExternalPlaySourceKind::CargoManifest)
            .map(EditorExternalPlaySourceFact::digest)
    }

    /// Returns the admitted Cargo lockfile digest carried by the source receipt.
    #[must_use]
    pub fn cargo_lock_digest(&self) -> Option<ContentDigest> {
        self.receipt
            .source_facts()
            .iter()
            .find(|fact| fact.kind() == EditorExternalPlaySourceKind::CargoLock)
            .map(EditorExternalPlaySourceFact::digest)
    }
}

/// Admits one canonical Editor-owned source receipt for the exact supervised desktop contract.
#[doc(hidden)]
pub fn admit_supervised_desktop_source_receipt(
    encoded: &[u8],
) -> Result<SupervisedDesktopSourceAuthority, SupervisedDesktopSourceAdmissionError> {
    let receipt = EditorExternalPlaySourceReceipt::decode_canonical(encoded)
        .map_err(|_| SupervisedDesktopSourceAdmissionError::InvalidSourceReceipt)?;
    if receipt.target() != EditorExternalPlayTarget::Desktop {
        return Err(SupervisedDesktopSourceAdmissionError::UnsupportedTarget);
    }
    if receipt.launch_context_version() != SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION {
        return Err(
            SupervisedDesktopSourceAdmissionError::UnsupportedLaunchContextVersion {
                expected: SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
                actual: receipt.launch_context_version(),
            },
        );
    }

    Ok(SupervisedDesktopSourceAuthority {
        canonical_source_receipt: encoded.into(),
        receipt,
    })
}

/// Typed failure from admitting an Editor-owned source receipt for supervised desktop launch.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisedDesktopSourceAdmissionError {
    InvalidSourceReceipt,
    UnsupportedTarget,
    UnsupportedLaunchContextVersion { expected: u16, actual: u16 },
}

impl fmt::Display for SupervisedDesktopSourceAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceReceipt => formatter
                .write_str("supervised desktop source receipt is malformed or is not canonical"),
            Self::UnsupportedTarget => {
                formatter.write_str("supervised desktop source receipt targets another product")
            }
            Self::UnsupportedLaunchContextVersion { expected, actual } => write!(
                formatter,
                "supervised desktop source receipt uses launch context version {actual}; expected {expected}"
            ),
        }
    }
}

impl Error for SupervisedDesktopSourceAdmissionError {}

impl EditorProjectSession {
    pub(crate) fn external_play_view(&self) -> Option<EditorExternalPlayView> {
        if !matches!(
            self.project_readiness().runtime(),
            EditorRuntimeAvailability::ExternalSupervised
        ) {
            return None;
        }
        let view = self.external_play_view.clone().unwrap_or_else(|| {
            EditorExternalPlayView::new(
                EditorExternalPlayState::Idle,
                None,
                false,
                EditorExternalPlayOutput::default(),
            )
        });
        Some(EditorExternalPlayView::new(
            view.state(),
            view.generation(),
            view.last_good_available() && self.external_source_matches_bound_generation(),
            view.output().clone(),
        ))
    }

    #[doc(hidden)]
    pub fn take_external_play_request(&mut self) -> Option<EditorExternalPlayRequest> {
        self.external_play_request.take()
    }

    #[doc(hidden)]
    pub fn apply_external_play_observation(
        &mut self,
        observation: EditorExternalPlayObservation,
    ) -> bool {
        if self.external_play_attempt != Some(observation.attempt()) {
            return false;
        }
        if let Some(source_digest) = observation.source_receipt_digest() {
            self.external_play_bound_source_digest = Some(source_digest);
        }
        let view = EditorExternalPlayView::new(
            observation.state(),
            observation.generation(),
            observation.last_good_available() && self.external_source_matches_bound_generation(),
            observation.output().clone(),
        );
        let changed = self.external_play_view.as_ref() != Some(&view)
            || observation
                .result()
                .is_some_and(|result| self.play_result != Some(result));
        self.external_play_view = Some(view);
        if let Some(result) = observation.result() {
            self.play_result = Some(result);
        }
        changed
    }

    pub(crate) fn request_external_play(
        &mut self,
        command: EditorPlayCommand,
    ) -> EditorPlayRequestResult {
        if self.reopen_pending()
            && matches!(
                command,
                EditorPlayCommand::Play
                    | EditorPlayCommand::Restart
                    | EditorPlayCommand::RunLastGood
            )
        {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        let operation = super::play::play_operation(command);
        let current = self.external_play_view().unwrap_or_else(|| {
            EditorExternalPlayView::new(
                EditorExternalPlayState::Idle,
                None,
                false,
                EditorExternalPlayOutput::default(),
            )
        });
        let unsupported = match command {
            EditorPlayCommand::Pause => Some(nara_tooling::EditorPlayControl::Pause),
            EditorPlayCommand::Resume => Some(nara_tooling::EditorPlayControl::Resume),
            EditorPlayCommand::StepFixedTick => {
                Some(nara_tooling::EditorPlayControl::StepFixedTick)
            }
            _ => None,
        };
        if let Some(control) = unsupported {
            let reason = EditorPlayRejection::UnsupportedControl(control);
            self.play_result = Some(EditorPlayOperationResult::Rejected { operation, reason });
            return EditorPlayRequestResult::Rejected(reason);
        }

        if !external_command_allowed(current.state(), command) {
            return EditorPlayRequestResult::Rejected(
                if matches!(
                    current.state(),
                    EditorExternalPlayState::Building
                        | EditorExternalPlayState::Starting
                        | EditorExternalPlayState::Running
                        | EditorExternalPlayState::Stopping
                        | EditorExternalPlayState::CleanupIncomplete
                ) {
                    EditorPlayRejection::Busy
                } else {
                    EditorPlayRejection::InvalidState
                },
            );
        }
        if command == EditorPlayCommand::RunLastGood && !current.last_good_available() {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState);
        }
        if command == EditorPlayCommand::Restart {
            self.refresh_current_saved_source_digest();
            if !self.external_source_matches_bound_generation() {
                self.play_result = Some(EditorPlayOperationResult::Rejected {
                    operation,
                    reason: EditorPlayRejection::RuntimeOutOfDate,
                });
                return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeOutOfDate);
            }
        }
        if matches!(
            command,
            EditorPlayCommand::Play | EditorPlayCommand::RunLastGood
        ) && !self.saved_documents_are_launchable()
        {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::RuntimeOutOfDate,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeOutOfDate);
        }

        let source_receipt = if matches!(
            command,
            EditorPlayCommand::Play | EditorPlayCommand::RunLastGood
        ) {
            match self.supervised_desktop_source_receipt() {
                Ok(receipt) => Some(receipt),
                Err(error) => {
                    self.current_saved_source_digest = None;
                    let (reason, code, summary) = match error {
                        EditorExternalPlayReceiptError::SourceBasisChanged => (
                            EditorPlayRejection::RuntimeOutOfDate,
                            "project.editor.supervised-play-source-out-of-date",
                            "the project manifest or authoring schema changed after this Editor session opened",
                        ),
                        _ => (
                            EditorPlayRejection::RuntimeUnavailable(
                                EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
                            ),
                            "project.editor.supervised-play-source-rejected",
                            "the saved project source could not be admitted for supervised Play",
                        ),
                    };
                    let _ = self.diagnostics.extend(single_error(code, summary));
                    self.play_result =
                        Some(EditorPlayOperationResult::Rejected { operation, reason });
                    return EditorPlayRequestResult::Rejected(reason);
                }
            }
        } else {
            None
        };
        let source_receipt_digest = source_receipt.as_deref().map(ContentDigest::of_bytes);
        if command == EditorPlayCommand::RunLastGood
            && source_receipt_digest != self.external_play_bound_source_digest
        {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::RuntimeOutOfDate,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeOutOfDate);
        }
        let attempt = self.next_external_play_attempt;
        self.next_external_play_attempt = self.next_external_play_attempt.saturating_add(1);
        let request = EditorExternalPlayRequest::new(attempt, operation, source_receipt)
            .expect("a supervised Play attempt is allocated from a non-zero counter");
        self.external_play_attempt = Some(attempt);
        self.external_play_request = Some(request);
        if let Some(source_digest) = source_receipt_digest {
            self.current_saved_source_digest = Some(source_digest);
            self.external_play_bound_source_digest = Some(source_digest);
        }
        let state = match command {
            EditorPlayCommand::Stop
            | EditorPlayCommand::Cancel
            | EditorPlayCommand::Restart
            | EditorPlayCommand::RetryClose
            | EditorPlayCommand::RetryRetirement => EditorExternalPlayState::Stopping,
            _ => EditorExternalPlayState::Building,
        };
        let output = if matches!(
            command,
            EditorPlayCommand::Play | EditorPlayCommand::Restart | EditorPlayCommand::RunLastGood
        ) {
            EditorExternalPlayOutput::default()
        } else {
            current.output().clone()
        };
        self.external_play_view = Some(EditorExternalPlayView::new(
            state,
            current.generation(),
            current.last_good_available(),
            output,
        ));
        self.play_result = Some(EditorPlayOperationResult::Pending { operation });
        EditorPlayRequestResult::Accepted
    }

    pub(crate) fn external_source_matches_bound_generation(&self) -> bool {
        let Some(bound) = self.external_play_bound_source_digest else {
            return true;
        };
        self.current_saved_source_digest == Some(bound)
    }

    fn saved_documents_are_launchable(&self) -> bool {
        // Degraded authoring is still eligible for supervised desktop Play. The source receipt
        // builder and child READY admission own strict executable validation; this predicate only
        // prevents a launch from silently ignoring unsaved document edits.
        self.authoring_project().is_some()
            && self
                .workspace
                .__document_ids()
                .into_iter()
                .all(|document| !self.workspace.document_is_dirty(document).unwrap_or(true))
    }

    pub(super) fn refresh_current_saved_source_digest(&mut self) {
        if self.external_play_bound_source_digest.is_none() {
            return;
        }
        self.current_saved_source_digest = self
            .supervised_desktop_source_receipt()
            .ok()
            .map(|receipt| ContentDigest::of_bytes(&receipt));
    }
}

fn external_command_allowed(state: EditorExternalPlayState, command: EditorPlayCommand) -> bool {
    match state {
        EditorExternalPlayState::Idle => matches!(
            command,
            EditorPlayCommand::Play | EditorPlayCommand::RunLastGood
        ),
        EditorExternalPlayState::Faulted => matches!(
            command,
            EditorPlayCommand::Play | EditorPlayCommand::RunLastGood | EditorPlayCommand::Stop
        ),
        EditorExternalPlayState::Building => {
            matches!(command, EditorPlayCommand::Cancel | EditorPlayCommand::Stop)
        }
        EditorExternalPlayState::Starting => {
            matches!(command, EditorPlayCommand::Cancel | EditorPlayCommand::Stop)
        }
        EditorExternalPlayState::Running => matches!(
            command,
            EditorPlayCommand::Stop | EditorPlayCommand::Restart
        ),
        EditorExternalPlayState::CleanupIncomplete => matches!(
            command,
            EditorPlayCommand::RetryRetirement | EditorPlayCommand::RetryClose
        ),
        EditorExternalPlayState::Stopping => false,
    }
}

#[cfg(test)]
mod tests {
    use nara_project::ProjectPath;

    use crate::project_host::product_launch::{
        EditorExternalPlaySourceFact, EditorExternalPlaySourceKind,
    };

    use super::*;

    fn source_fact(
        kind: EditorExternalPlaySourceKind,
        path: &str,
        bytes: &[u8],
    ) -> EditorExternalPlaySourceFact {
        EditorExternalPlaySourceFact::new(
            kind,
            ProjectPath::new(path).unwrap(),
            ContentDigest::of_bytes(bytes),
        )
        .unwrap()
    }

    fn source_receipt(
        launch_context_version: u16,
        scene: &[u8],
    ) -> EditorExternalPlaySourceReceipt {
        EditorExternalPlaySourceReceipt::new(
            "018f27e2-52b8-7bd4-b38f-68cb5725891f".to_owned(),
            "desktop".to_owned(),
            EditorExternalPlayTarget::Desktop,
            launch_context_version,
            ContentDigest::of_bytes(b"manifest"),
            ContentDigest::of_bytes(b"authoring bundle"),
            ContentDigest::of_bytes(b"content"),
            [7; 32],
            vec![
                source_fact(
                    EditorExternalPlaySourceKind::CargoManifest,
                    "Cargo.toml",
                    b"cargo manifest",
                ),
                source_fact(
                    EditorExternalPlaySourceKind::Scene,
                    "scenes/main.scene.json",
                    scene,
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn supervised_source_authority_retains_one_decoded_canonical_receipt() {
        let receipt = source_receipt(SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION, b"scene");
        let encoded = receipt.encode_canonical().unwrap();

        let authority = admit_supervised_desktop_source_receipt(&encoded).unwrap();

        assert_eq!(authority.canonical_source_receipt(), encoded);
        assert_eq!(authority.project_stable_id(), receipt.project_stable_id());
        assert_eq!(authority.profile_name(), receipt.profile_name());
        assert_eq!(authority.manifest_digest(), receipt.manifest_digest());
        assert_eq!(
            authority.authoring_bundle_digest(),
            receipt.authoring_bundle_digest()
        );
        assert_eq!(authority.content_digest(), receipt.content_digest());
        assert_eq!(authority.schema_fingerprint(), receipt.schema_fingerprint());
    }

    #[test]
    fn supervised_source_authority_rejects_tampered_and_noncanonical_receipts() {
        let receipt = source_receipt(SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION, b"scene");
        let mut tampered = receipt.encode_canonical().unwrap();
        tampered[0] ^= 0xff;
        assert_eq!(
            admit_supervised_desktop_source_receipt(&tampered),
            Err(SupervisedDesktopSourceAdmissionError::InvalidSourceReceipt)
        );

        let mut noncanonical = receipt.encode_canonical().unwrap();
        noncanonical.push(0);
        assert_eq!(
            admit_supervised_desktop_source_receipt(&noncanonical),
            Err(SupervisedDesktopSourceAdmissionError::InvalidSourceReceipt)
        );
    }

    #[test]
    fn supervised_source_authority_rejects_another_launch_context_version() {
        let actual = SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION + 1;
        let encoded = source_receipt(actual, b"scene").encode_canonical().unwrap();

        assert_eq!(
            admit_supervised_desktop_source_receipt(&encoded),
            Err(
                SupervisedDesktopSourceAdmissionError::UnsupportedLaunchContextVersion {
                    expected: SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
                    actual,
                }
            )
        );
    }

    #[test]
    fn external_command_matrix_keeps_restart_exact_and_cleanup_retry_explicit() {
        assert!(external_command_allowed(
            EditorExternalPlayState::Running,
            EditorPlayCommand::Restart
        ));
        assert!(!external_command_allowed(
            EditorExternalPlayState::Running,
            EditorPlayCommand::Play
        ));
        assert!(external_command_allowed(
            EditorExternalPlayState::CleanupIncomplete,
            EditorPlayCommand::RetryRetirement
        ));
        assert!(!external_command_allowed(
            EditorExternalPlayState::CleanupIncomplete,
            EditorPlayCommand::Restart
        ));
        assert!(external_command_allowed(
            EditorExternalPlayState::Faulted,
            EditorPlayCommand::Stop
        ));
        assert!(external_command_allowed(
            EditorExternalPlayState::Building,
            EditorPlayCommand::Cancel
        ));
        assert!(!external_command_allowed(
            EditorExternalPlayState::Stopping,
            EditorPlayCommand::Stop
        ));
    }
}
