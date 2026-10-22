//! Source-built document registration and Host-owned publication lanes.

use std::{any::TypeId, collections::BTreeMap};

use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError, RelativePath,
};
use nara_tooling::{
    EditorDocumentId, EditorDocumentSourceBinding, EditorExtensionDocument,
    EditorExtensionDocumentCandidate, EditorExtensionDocumentError, EditorPersistenceCheckpoint,
    EditorPersistenceFailureStage, EditorPersistenceRejection, EditorPersistenceResult,
};

use super::persistence::RetainedHost;
use super::{EditorExecutionLane, EditorProjectSession};
use crate::project_host::file_publication::FilePublicationReadSource;
use crate::project_host::persistence::{
    FilePublicationHost, PersistenceSaveOutcome, editor_digest, publish_editor_checkpoint,
};

const EXTENSION_DOCUMENT_BYTE_LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EditorExtensionDocumentHostError {
    #[error("document type identity is empty")]
    EmptyTypeId,
    #[error("document type identity is already registered")]
    DuplicateTypeId,
    #[error("document owner is not registered")]
    UnregisteredType,
    #[error("document source path is invalid")]
    InvalidSource,
    #[error("document source could not be read")]
    SourceRead,
    #[error("document codec rejected the source")]
    Decode(EditorExtensionDocumentError),
    #[error("document publication lane could not be opened")]
    PublicationOpen,
    #[error("document source is already open")]
    SourceAlreadyOpen,
    #[error("the workspace is busy or closing")]
    Busy,
}

type DecodeDocument =
    fn(&[u8]) -> Result<EditorExtensionDocumentCandidate, EditorExtensionDocumentError>;

struct Registration {
    owner: TypeId,
    decode: DecodeDocument,
}

#[derive(Clone)]
pub(super) struct ExtensionPersistenceLane {
    pub(super) document: EditorDocumentId,
    pub(super) source: EditorDocumentSourceBinding,
    pub(super) host: RetainedHost<FilePublicationHost>,
    decode: DecodeDocument,
}

#[derive(Default)]
pub(super) struct EditorExtensionDocumentsHost {
    registered: BTreeMap<&'static str, Registration>,
    lanes: BTreeMap<EditorDocumentId, ExtensionPersistenceLane>,
    source_root: Option<DirectoryCapability>,
}

impl EditorExtensionDocumentsHost {
    fn register<D: EditorExtensionDocument>(
        &mut self,
    ) -> Result<(), EditorExtensionDocumentHostError> {
        if D::TYPE_ID.is_empty() {
            return Err(EditorExtensionDocumentHostError::EmptyTypeId);
        }
        if self.registered.contains_key(D::TYPE_ID) {
            return Err(EditorExtensionDocumentHostError::DuplicateTypeId);
        }
        self.registered.insert(
            D::TYPE_ID,
            Registration {
                owner: TypeId::of::<D>(),
                decode: EditorExtensionDocumentCandidate::decode::<D>,
            },
        );
        Ok(())
    }

    pub(super) fn lane(&self, document: EditorDocumentId) -> Option<ExtensionPersistenceLane> {
        self.lanes.get(&document).cloned()
    }

    pub(super) fn has_source(&self, project_path: &str) -> bool {
        if self.has_exact_source(project_path) {
            return true;
        }
        if self.lanes.is_empty() {
            return false;
        }
        let Some(root) = &self.source_root else {
            return true;
        };
        let Ok(path) = RelativePath::new(project_path) else {
            return false;
        };
        // Existing boolean callers must refuse Open when alias exclusion cannot be checked.
        match root.open_file(&path) {
            Ok(file) => self.has_file(&file).unwrap_or(true),
            Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                false
            }
            Err(_) => true,
        }
    }

    fn has_exact_source(&self, project_path: &str) -> bool {
        self.lanes
            .values()
            .any(|lane| lane.source.locator() == project_path)
    }

    fn has_file(&self, file: &FileCapability) -> Result<bool, EditorExtensionDocumentHostError> {
        for lane in self.lanes.values() {
            let source = lane
                .host
                .try_with(|host| host.read_source())
                .ok_or(EditorExtensionDocumentHostError::Busy)?;
            if source_aliases(file, source)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn retire(&mut self, document: EditorDocumentId) -> bool {
        if self
            .lanes
            .get(&document)
            .is_some_and(|lane| !lane.host.retire())
        {
            return false;
        }
        self.lanes.remove(&document);
        if self.lanes.is_empty() {
            self.source_root = None;
        }
        true
    }

    pub(super) fn mark_uncertain(&mut self, document: EditorDocumentId) {
        if let Some(lane) = self.lanes.get(&document) {
            lane.host.try_with(|host| host.mark_uncertain());
        }
    }
}

impl EditorProjectSession {
    /// Registers a trusted source-built domain. Ordinary project files never load extension code.
    pub fn register_extension_document<D: EditorExtensionDocument>(
        &mut self,
    ) -> Result<(), EditorExtensionDocumentHostError> {
        if !self.accepts_new_work() {
            return Err(EditorExtensionDocumentHostError::Busy);
        }
        self.extension_documents.register::<D>()
    }

    /// Opens one registered document from a project-relative path.
    ///
    /// Use during trusted composition or an explicit product command. Source reads are bounded;
    /// subsequent Save/Reopen use the ordinary background persistence and dirty-close paths.
    pub fn open_extension_document<D: EditorExtensionDocument>(
        &mut self,
        source: &str,
    ) -> Result<EditorDocumentId, EditorExtensionDocumentHostError> {
        if !self.accepts_new_work()
            || self.pending_workspace_intent.is_some()
            || self.pending_persistence.is_some()
            || self.document_open_preparing()
        {
            return Err(EditorExtensionDocumentHostError::Busy);
        }
        let registration = self
            .extension_documents
            .registered
            .get(D::TYPE_ID)
            .filter(|registration| registration.owner == TypeId::of::<D>())
            .ok_or(EditorExtensionDocumentHostError::UnregisteredType)?;
        let decode = registration.decode;
        let source_path = nara_asset::AssetPath::new(source.to_owned())
            .map_err(|_| EditorExtensionDocumentHostError::InvalidSource)?;
        let source = source_path.as_str();
        let binding = EditorDocumentSourceBinding::new(source.to_owned());
        if self.extension_documents.has_exact_source(source)
            || self
                .scene_lane
                .as_ref()
                .is_some_and(|lane| lane.project_path.as_ref() == source)
            || self
                .prefab_lane
                .as_ref()
                .is_some_and(|lane| lane.project_path.as_ref() == source)
        {
            return Err(EditorExtensionDocumentHostError::SourceAlreadyOpen);
        }
        let root = match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => snapshot.project_root(),
            EditorExecutionLane::TrustedCompiled(lane) => lane.content_loader.project_root(),
        };
        let path = RelativePath::new(source)
            .map_err(|_| EditorExtensionDocumentHostError::InvalidSource)?;
        let file = root
            .open_file(&path)
            .map_err(|_| EditorExtensionDocumentHostError::SourceRead)?;
        if self.extension_documents.has_file(&file)? {
            return Err(EditorExtensionDocumentHostError::SourceAlreadyOpen);
        }
        let scene_source = self
            .scene_lane
            .as_ref()
            .map(|lane| {
                lane.host
                    .try_with(|host| host.read_source())
                    .ok_or(EditorExtensionDocumentHostError::Busy)
            })
            .transpose()?;
        let prefab_source = self
            .prefab_lane
            .as_ref()
            .map(|lane| {
                lane.host
                    .try_with(|host| host.read_source())
                    .ok_or(EditorExtensionDocumentHostError::Busy)
            })
            .transpose()?;
        for retained_source in scene_source.into_iter().chain(prefab_source) {
            if source_aliases(&file, retained_source)? {
                return Err(EditorExtensionDocumentHostError::SourceAlreadyOpen);
            }
        }
        let (owner, publication, digest) = open_source::<D>(&root, source, &file)?;
        // Retain file custody before publishing a document slot, so every visible document has a
        // persistence owner even if opening a later document fails.
        let publication = self
            .authoring_runtime
            .host_custody()
            .retain_publication(publication)
            .ok_or(EditorExtensionDocumentHostError::PublicationOpen)?;
        let document = match self
            .workspace
            .open_extension_document(binding.clone(), owner)
        {
            Ok(document) => document,
            Err(_) => {
                publication.retire();
                return Err(EditorExtensionDocumentHostError::SourceAlreadyOpen);
            }
        };
        let report = self.persistence_authority.bind_opened_source_digest(
            &mut self.workspace,
            document,
            editor_digest(digest),
        );
        if !report.applied {
            let _ = self.workspace.__discard_document(document);
            publication.retire();
            return Err(EditorExtensionDocumentHostError::PublicationOpen);
        }
        self.extension_documents.lanes.insert(
            document,
            ExtensionPersistenceLane {
                document,
                source: binding,
                host: publication,
                decode,
            },
        );
        self.extension_documents.source_root = Some(root);
        self.publish_editor_frame(false);
        Ok(document)
    }
}

fn source_aliases(
    file: &FileCapability,
    source: FilePublicationReadSource,
) -> Result<bool, EditorExtensionDocumentHostError> {
    // Resolve through retained parent custody and observe the current atomic-save replacement.
    match source.open() {
        Ok(current) => Ok(current.identity() == file.identity()),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok(false)
        }
        Err(_) => Err(EditorExtensionDocumentHostError::SourceRead),
    }
}

fn open_source<D: EditorExtensionDocument>(
    root: &DirectoryCapability,
    source: &str,
    file: &FileCapability,
) -> Result<(D, FilePublicationHost, ContentDigest), EditorExtensionDocumentHostError> {
    let bytes = file
        .read_to_end_bounded(EXTENSION_DOCUMENT_BYTE_LIMIT)
        .map_err(|_| EditorExtensionDocumentHostError::SourceRead)?;
    let digest = ContentDigest::of_bytes(&bytes);
    let owner = D::decode(&bytes).map_err(EditorExtensionDocumentHostError::Decode)?;
    let publication = FilePublicationHost::bind_existing(
        root,
        source,
        file.identity(),
        digest,
        EXTENSION_DOCUMENT_BYTE_LIMIT,
    )
    .map_err(|_| EditorExtensionDocumentHostError::PublicationOpen)?;
    Ok((owner, publication, digest))
}

impl ExtensionPersistenceLane {
    pub(super) fn save(&self, checkpoint: EditorPersistenceCheckpoint) -> PersistenceSaveOutcome {
        let bytes = match checkpoint.extension_bytes() {
            Some(Ok(bytes)) => bytes.to_vec(),
            Some(Err(_)) => {
                return PersistenceSaveOutcome::Failed(
                    nara_tooling::EditorPersistenceFailureStage::Encode,
                );
            }
            None => {
                return PersistenceSaveOutcome::Rejected(
                    EditorPersistenceRejection::DocumentKindMismatch,
                );
            }
        };
        self.host
            .with_worker(|host| {
                publish_editor_checkpoint(
                    host,
                    checkpoint,
                    &bytes,
                    #[cfg(test)]
                    false,
                )
            })
            .unwrap_or(PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::Busy,
            ))
    }

    pub(super) fn capture_reopen(
        &self,
    ) -> Result<ExtensionReopenRequest, EditorPersistenceRejection> {
        self.host
            .try_with(|host| ExtensionReopenRequest {
                source: host.read_source(),
                decode: self.decode,
            })
            .ok_or(EditorPersistenceRejection::Busy)
    }
}

pub(super) struct ExtensionReopenRequest {
    source: FilePublicationReadSource,
    decode: DecodeDocument,
}

pub(super) enum ExtensionReopenFailure {
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
    Decode(EditorExtensionDocumentError),
}

impl ExtensionReopenRequest {
    pub(super) fn prepare(self) -> Result<PreparedExtensionReopen, ExtensionReopenFailure> {
        let file = self.source.open().map_err(|error| match error {
            FsError::Io { ref source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
                ExtensionReopenFailure::Rejected(EditorPersistenceRejection::TargetDeleted)
            }
            FsError::Unsupported { .. } | FsError::Unproven { .. } => {
                ExtensionReopenFailure::Rejected(EditorPersistenceRejection::UnsupportedGuarantee)
            }
            _ => ExtensionReopenFailure::Failed(EditorPersistenceFailureStage::OpenTarget),
        })?;
        let bytes = file
            .read_to_end_bounded(EXTENSION_DOCUMENT_BYTE_LIMIT)
            .map_err(|_| {
                ExtensionReopenFailure::Failed(EditorPersistenceFailureStage::ReadTarget)
            })?;
        let candidate = (self.decode)(&bytes).map_err(ExtensionReopenFailure::Decode)?;
        Ok(PreparedExtensionReopen {
            candidate,
            identity: file.identity(),
            digest: ContentDigest::of_bytes(&bytes),
        })
    }
}

pub(super) struct PreparedExtensionReopen {
    candidate: EditorExtensionDocumentCandidate,
    identity: FileIdentity,
    digest: ContentDigest,
}

impl EditorProjectSession {
    pub(super) fn publish_extension_reopen(
        &mut self,
        document: EditorDocumentId,
        revision: nara_core::DocumentRevision,
        prepared: Result<PreparedExtensionReopen, ExtensionReopenFailure>,
    ) -> EditorPersistenceResult {
        let rejected = |reason| EditorPersistenceResult::Rejected {
            request: nara_tooling::EditorPersistenceRequestKind::Reopen,
            document: Some(document),
            reason,
        };
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(ExtensionReopenFailure::Rejected(reason)) => return rejected(reason),
            Err(ExtensionReopenFailure::Failed(stage)) => {
                return EditorPersistenceResult::Failed {
                    request: nara_tooling::EditorPersistenceRequestKind::Reopen,
                    document: Some(document),
                    stage,
                };
            }
            Err(ExtensionReopenFailure::Decode(error)) => {
                self.diagnostics.push(nara_diagnostic::Diagnostic::error(
                    error.code(),
                    error.summary(),
                ));
                return EditorPersistenceResult::Failed {
                    request: nara_tooling::EditorPersistenceRequestKind::Reopen,
                    document: Some(document),
                    stage: EditorPersistenceFailureStage::Decode,
                };
            }
        };
        let Some(lane) = self.extension_documents.lane(document) else {
            return rejected(EditorPersistenceRejection::MissingDocument);
        };
        let Some(report) = lane.host.try_with(|host| {
            let report = self
                .persistence_authority
                .publish_reopened_extension_document(
                    &mut self.workspace,
                    document,
                    revision,
                    &lane.source,
                    prepared.candidate,
                    editor_digest(prepared.digest),
                );
            if report.as_ref().is_ok_and(|report| report.applied) {
                host.commit_reconciled(prepared.identity, prepared.digest);
            }
            report
        }) else {
            return rejected(EditorPersistenceRejection::Busy);
        };
        match report {
            Ok(report) if report.applied => EditorPersistenceResult::Opened {
                document,
                revision: report
                    .revision
                    .expect("an accepted document retains its revision"),
                digest: editor_digest(prepared.digest),
            },
            _ => rejected(EditorPersistenceRejection::StaleRevision),
        }
    }
}
