//! Narrow type erasure for independently owned Editor documents.

use std::any::{Any, TypeId};
use std::fmt;
use std::sync::Arc;

use nara_core::{DocumentContentState, DocumentHistoryStatus, DocumentRevision};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};

/// A document domain owns its typed content, commands, history, and codec.
///
/// The workspace owns document identity and persistence checkpoints. The product Host owns file
/// capabilities and publication. Implementations should return stable content-state identities
/// across Undo/Redo so a document can become clean when it returns to its saved state.
/// Content identities must be unique for forward edits, including branches after Undo. Rejected
/// commands and history transitions leave the owner unchanged. Decode creates a fresh owner and
/// empty history; encode/view do not mutate it. Codecs must be pure, bounded work without I/O or
/// owner-thread native resources. Decode may run on the Host's I/O worker.
pub trait EditorExtensionDocument: Send + Sync + 'static {
    type Command: Send + Sync + 'static;
    type View: fmt::Debug + Send + Sync + 'static;

    const TYPE_ID: &'static str;

    fn title(&self) -> &str;
    fn content_state(&self) -> DocumentContentState;
    fn history_status(&self) -> DocumentHistoryStatus;
    fn view(&self) -> Self::View;
    /// Reject without mutating content or history. Accepted commands must commit atomically.
    fn apply(&mut self, command: &Self::Command) -> Result<(), EditorExtensionDocumentError>;
    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError>;
    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError>;
    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError>;
    fn decode(bytes: &[u8]) -> Result<Self, EditorExtensionDocumentError>
    where
        Self: Sized;
}

/// Safe, domain-owned failure for an extension document operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorExtensionDocumentError {
    code: DiagnosticCode,
    summary: SafeSummary,
}

impl EditorExtensionDocumentError {
    #[must_use]
    pub const fn new(code: DiagnosticCode, summary: SafeSummary) -> Self {
        Self { code, summary }
    }

    #[must_use]
    pub const fn code(self) -> DiagnosticCode {
        self.code
    }

    #[must_use]
    pub const fn summary(self) -> SafeSummary {
        self.summary
    }

    pub(super) fn diagnostic(self) -> Diagnostic {
        Diagnostic::error(self.code, self.summary)
    }
}

impl fmt::Display for EditorExtensionDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.summary.as_str())
    }
}

impl std::error::Error for EditorExtensionDocumentError {}

pub(super) trait ErasedExtensionDocument: Send + Sync {
    fn type_id(&self) -> &'static str;
    fn rust_type_id(&self) -> TypeId;
    fn title(&self) -> &str;
    fn content_state(&self) -> DocumentContentState;
    fn history_status(&self) -> DocumentHistoryStatus;
    fn view(&self) -> Arc<dyn Any + Send + Sync>;
    fn apply(
        &mut self,
        command: &(dyn Any + Send + Sync),
    ) -> Result<(), EditorExtensionDocumentError>;
    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError>;
    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError>;
    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError>;
}

pub(super) struct TypedExtensionDocument<D>(D);

impl<D: EditorExtensionDocument> TypedExtensionDocument<D> {
    pub(super) const fn new(document: D) -> Self {
        Self(document)
    }
}

impl<D: EditorExtensionDocument> ErasedExtensionDocument for TypedExtensionDocument<D> {
    fn type_id(&self) -> &'static str {
        D::TYPE_ID
    }

    fn rust_type_id(&self) -> TypeId {
        TypeId::of::<D>()
    }

    fn title(&self) -> &str {
        self.0.title()
    }

    fn content_state(&self) -> DocumentContentState {
        self.0.content_state()
    }

    fn history_status(&self) -> DocumentHistoryStatus {
        self.0.history_status()
    }

    fn view(&self) -> Arc<dyn Any + Send + Sync> {
        Arc::new(self.0.view())
    }

    fn apply(
        &mut self,
        command: &(dyn Any + Send + Sync),
    ) -> Result<(), EditorExtensionDocumentError> {
        let command = command.downcast_ref::<D::Command>().ok_or_else(|| {
            EditorExtensionDocumentError::new(
                DiagnosticCode::new("tooling.extension-document-command-type-mismatch")
                    .expect("static code"),
                SafeSummary::new("extension command does not match the registered document owner")
                    .expect("static summary"),
            )
        })?;
        self.0.apply(command)
    }

    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        self.0.undo()
    }

    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        self.0.redo()
    }

    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        self.0.encode()
    }
}

/// A typed command carried through the ordinary ordered action path.
///
/// Clones share an immutable payload. Equality identifies that same submission payload; domain
/// commands do not need Clone, Debug, PartialEq, or serialization.
#[derive(Clone)]
pub struct EditorExtensionCommand {
    pub(super) document: super::EditorDocumentId,
    pub(super) base_revision: DocumentRevision,
    pub(super) owner: TypeId,
    pub(super) payload: Arc<dyn Any + Send + Sync>,
}

impl EditorExtensionCommand {
    #[must_use]
    pub fn new<D: EditorExtensionDocument>(
        document: super::EditorDocumentId,
        base_revision: DocumentRevision,
        command: D::Command,
    ) -> Self {
        Self {
            document,
            base_revision,
            owner: TypeId::of::<D>(),
            payload: Arc::new(command),
        }
    }

    #[must_use]
    pub const fn document(&self) -> super::EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn base_revision(&self) -> DocumentRevision {
        self.base_revision
    }
}

impl fmt::Debug for EditorExtensionCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorExtensionCommand")
            .field("document", &self.document)
            .field("base_revision", &self.base_revision)
            .finish_non_exhaustive()
    }
}

impl PartialEq for EditorExtensionCommand {
    fn eq(&self, other: &Self) -> bool {
        self.document == other.document
            && self.base_revision == other.base_revision
            && self.owner == other.owner
            && Arc::ptr_eq(&self.payload, &other.payload)
    }
}

/// A decoded owner prepared outside the workspace and admitted against a captured revision.
#[doc(hidden)]
pub struct EditorExtensionDocumentCandidate(pub(super) Box<dyn ErasedExtensionDocument>);

impl EditorExtensionDocumentCandidate {
    pub fn decode<D: EditorExtensionDocument>(
        bytes: &[u8],
    ) -> Result<Self, EditorExtensionDocumentError> {
        D::decode(bytes).map(|document| Self(Box::new(TypedExtensionDocument::new(document))))
    }
}

/// Immutable typed domain projection retained by one workspace frame.
#[derive(Clone)]
pub struct EditorExtensionDocumentView {
    pub(super) document: super::EditorDocumentId,
    pub(super) type_id: &'static str,
    pub(super) owner: TypeId,
    pub(super) revision: nara_core::DocumentRevision,
    pub(super) value: Arc<dyn Any + Send + Sync>,
}

impl EditorExtensionDocumentView {
    #[must_use]
    pub const fn document(&self) -> super::EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn revision(&self) -> nara_core::DocumentRevision {
        self.revision
    }

    #[must_use]
    pub const fn type_id(&self) -> &'static str {
        self.type_id
    }

    #[must_use]
    pub fn get<D: EditorExtensionDocument>(&self) -> Option<&D::View> {
        (self.owner == TypeId::of::<D>())
            .then(|| self.value.downcast_ref())
            .flatten()
    }
}

impl fmt::Debug for EditorExtensionDocumentView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorExtensionDocumentView")
            .field("document", &self.document)
            .field("type_id", &self.type_id)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl PartialEq for EditorExtensionDocumentView {
    fn eq(&self, other: &Self) -> bool {
        self.document == other.document
            && self.type_id == other.type_id
            && self.revision == other.revision
    }
}
