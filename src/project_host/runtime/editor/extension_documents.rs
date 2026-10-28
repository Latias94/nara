//! Source-built document registration and Host-owned publication lanes.

use std::{any::TypeId, collections::BTreeMap};

use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FsError, RelativePath};
use nara_tooling::{
    EditorDocumentId, EditorDocumentSourceBinding, EditorExtensionDocument,
    EditorExtensionDocumentCandidate, EditorExtensionDocumentError, EditorPersistenceCheckpoint,
    EditorPersistenceFailureStage, EditorPersistenceRejection, EditorPersistenceResult,
};

use super::EditorProjectSession;
use super::persistence::RetainedHost;
use super::source_admission::SourceAdmissionError;
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
    #[error("managed artifacts must be imported before opening an editable project copy")]
    PrivateArtifactSource,
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

    pub(super) fn lanes(&self) -> impl Iterator<Item = &ExtensionPersistenceLane> {
        self.lanes.values()
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
        true
    }
}

impl EditorProjectSession {
    /// Registers a trusted source-built domain. Ordinary project files never load extension code.
    pub fn register_extension_document<D: EditorExtensionDocument>(
        &mut self,
    ) -> Result<(), EditorExtensionDocumentHostError> {
        if !self.accepts_new_work() || self.schema_adoption_fenced() {
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
            || self.schema_adoption_fenced()
            || self.pending_workspace_intent.is_some()
            || self.persistence.has_pending_work()
            || self.pending_document_open.is_some()
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
        if self.exact_source_owner(source).is_some() {
            return Err(EditorExtensionDocumentHostError::SourceAlreadyOpen);
        }
        let root = self.source_project_root();
        let path = RelativePath::new(source)
            .map_err(|_| EditorExtensionDocumentHostError::InvalidSource)?;
        let file = root
            .open_file(&path)
            .map_err(|_| EditorExtensionDocumentHostError::SourceRead)?;
        self.exclude_source_aliases(source, &file)
            .map_err(extension_source_error)?;
        let (owner, publication, digest) = open_source::<D>(&root, source, &file)?;
        // Retain file custody before publishing a document slot, so every visible document has a
        // persistence owner even if opening a later document fails.
        let publication = self
            .authoring_runtime
            .host_custody()
            .retain_publication(publication)
            .ok_or(EditorExtensionDocumentHostError::PublicationOpen)?;
        let check = publication
            .try_with(|host| host.read_source().open())
            .ok_or(SourceAdmissionError::HostUnavailable)
            .and_then(|file| file.map_err(|_| SourceAdmissionError::SourceRead))
            .and_then(|file| self.exclude_source_aliases(source, &file));
        if let Err(error) = check {
            publication.retire();
            return Err(extension_source_error(error));
        }
        let previous_active = self.workspace.active_document();
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
            self.discard_unpublished_document(document, previous_active);
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
        self.publish_editor_frame(false);
        Ok(document)
    }
}

fn extension_source_error(error: SourceAdmissionError) -> EditorExtensionDocumentHostError {
    match error {
        SourceAdmissionError::AlreadyOpen => EditorExtensionDocumentHostError::SourceAlreadyOpen,
        SourceAdmissionError::PrivateArtifact => {
            EditorExtensionDocumentHostError::PrivateArtifactSource
        }
        SourceAdmissionError::HostUnavailable => EditorExtensionDocumentHostError::Busy,
        SourceAdmissionError::SourceRead => EditorExtensionDocumentHostError::SourceRead,
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
        file,
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
            file,
            digest: ContentDigest::of_bytes(&bytes),
        })
    }
}

pub(super) struct PreparedExtensionReopen {
    candidate: EditorExtensionDocumentCandidate,
    file: FileCapability,
    digest: ContentDigest,
}

impl EditorProjectSession {
    pub(super) fn publish_extension_reopen(
        &mut self,
        lane: &ExtensionPersistenceLane,
        revision: nara_core::DocumentRevision,
        prepared: Result<PreparedExtensionReopen, ExtensionReopenFailure>,
    ) -> EditorPersistenceResult {
        let document = lane.document;
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
                host.commit_reconciled(prepared.file, prepared.digest);
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
