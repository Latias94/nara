//! Association between one captured source and its actual immutable encoded output.
//!
//! A digest supplied by a caller cannot establish this association. Both publication and
//! already-published observation consume output issued by the checkpoint's own codec path.

use std::{convert::Infallible, error::Error, fmt};

use nara_core::ByteLimit;
use nara_fs::{ContentDigest, FileCapability, FsError, ReplaceReceipt};
use nara_scene::{
    SceneFileEncoding, SceneFileEncodingAdmissionError, SceneFileLimits, SceneFormatError,
};

use super::{
    EditorDocumentDigest, EditorExtensionDocumentError, EditorPersistenceCheckpoint,
    EditorPersistenceCommit, EditorPersistenceContent,
};

/// Frozen codec output associated with the exact move-only source capture that produced it.
/// No arbitrary bytes or caller-provided digest can be attached to a checkpoint.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EncodedEditorPersistenceCheckpoint {
    checkpoint: EditorPersistenceCheckpoint,
    // Extension bytes already belong to the captured domain output; do not copy them again.
    bytes: Option<Vec<u8>>,
    digest: ContentDigest,
}

impl EncodedEditorPersistenceCheckpoint {
    #[must_use]
    pub fn checkpoint(&self) -> &EditorPersistenceCheckpoint {
        &self.checkpoint
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match &self.bytes {
            Some(bytes) => bytes,
            None => self
                .checkpoint
                .extension_bytes()
                .expect("captured extension output was checked before encoding")
                .expect("failed extension output cannot be admitted"),
        }
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    /// Releases output while returning the original nominal capture for an explicit retry.
    pub fn into_checkpoint(self) -> EditorPersistenceCheckpoint {
        self.checkpoint
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub enum EditorPersistenceEncodingError<E = Infallible> {
    Format(SceneFormatError),
    Extension(EditorExtensionDocumentError),
    DocumentKind,
    EncodedBytesExceeded,
    Admission(E),
}

impl<E> fmt::Display for EditorPersistenceEncodingError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => fmt::Display::fmt(error, formatter),
            Self::Extension(error) => fmt::Display::fmt(error, formatter),
            Self::DocumentKind => {
                formatter.write_str("the source kind does not match this encoder")
            }
            Self::EncodedBytesExceeded => {
                formatter.write_str("captured output exceeds the encoded byte limit")
            }
            Self::Admission(_) => formatter.write_str("encoded output admission was refused"),
        }
    }
}

impl<E: fmt::Debug> Error for EditorPersistenceEncodingError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Format(error) => Some(error),
            Self::Extension(error) => Some(error),
            _ => None,
        }
    }
}

/// Failed encoding returns the same capture; it does not issue another source identity.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorPersistenceEncodingFailure<E = Infallible> {
    checkpoint: EditorPersistenceCheckpoint,
    error: EditorPersistenceEncodingError<E>,
}

impl<E> EditorPersistenceEncodingFailure<E> {
    pub fn error(&self) -> &EditorPersistenceEncodingError<E> {
        &self.error
    }

    pub fn into_checkpoint(self) -> EditorPersistenceCheckpoint {
        self.checkpoint
    }

    pub fn into_parts(
        self,
    ) -> (
        EditorPersistenceCheckpoint,
        EditorPersistenceEncodingError<E>,
    ) {
        (self.checkpoint, self.error)
    }
}

impl EditorPersistenceCheckpoint {
    /// Encodes the retained Scene/Prefab version through the canonical, pre-growth bounded codec.
    pub fn encode(
        self,
        encoding: SceneFileEncoding,
        limits: SceneFileLimits,
    ) -> Result<EncodedEditorPersistenceCheckpoint, EditorPersistenceEncodingFailure> {
        self.encode_with_admission(encoding, limits, |_| Ok::<(), Infallible>(()))
    }

    /// Debits output capacity before growth and exact encoded bytes before their digest scan.
    /// The enclosing operation separately admits retained input, canonical copies and serializer
    /// work. A refused final scan returns the same capture without issuing encoded output.
    pub fn encode_with_admission<E>(
        self,
        encoding: SceneFileEncoding,
        limits: SceneFileLimits,
        mut admission: impl FnMut(usize) -> Result<(), E>,
    ) -> Result<EncodedEditorPersistenceCheckpoint, EditorPersistenceEncodingFailure<E>> {
        let result = match &self.content {
            EditorPersistenceContent::Scene(document) => {
                document.encode_with_admission(encoding, limits, &mut admission)
            }
            EditorPersistenceContent::Prefab(document) => {
                document.encode_with_admission(encoding, limits, &mut admission)
            }
            EditorPersistenceContent::Extension(_) => {
                return Err(EditorPersistenceEncodingFailure {
                    checkpoint: self,
                    error: EditorPersistenceEncodingError::DocumentKind,
                });
            }
        };
        match result {
            Ok(encoded) => {
                let bytes = encoded.into_bytes();
                if let Err(error) = admission(bytes.len()) {
                    return Err(EditorPersistenceEncodingFailure {
                        checkpoint: self,
                        error: EditorPersistenceEncodingError::Admission(error),
                    });
                }
                let digest = ContentDigest::of_bytes(&bytes);
                Ok(EncodedEditorPersistenceCheckpoint {
                    checkpoint: self,
                    bytes: Some(bytes),
                    digest,
                })
            }
            Err(error) => Err(EditorPersistenceEncodingFailure {
                checkpoint: self,
                error: match error {
                    SceneFileEncodingAdmissionError::Format(error) => {
                        EditorPersistenceEncodingError::Format(error)
                    }
                    SceneFileEncodingAdmissionError::Admission(error) => {
                        EditorPersistenceEncodingError::Admission(error)
                    }
                },
            }),
        }
    }

    /// Uses the domain's actual captured output allocation without a second encoding or copy.
    pub fn encode_extension(
        self,
        limit: ByteLimit,
    ) -> Result<EncodedEditorPersistenceCheckpoint, EditorPersistenceEncodingFailure> {
        let result = match self.extension_bytes() {
            Some(Ok(bytes)) if bytes.len() <= limit.get() => Ok(ContentDigest::of_bytes(bytes)),
            Some(Ok(_)) => Err(EditorPersistenceEncodingError::EncodedBytesExceeded),
            Some(Err(error)) => Err(EditorPersistenceEncodingError::Extension(error)),
            None => Err(EditorPersistenceEncodingError::DocumentKind),
        };
        match result {
            Ok(digest) => Ok(EncodedEditorPersistenceCheckpoint {
                checkpoint: self,
                bytes: None,
                digest,
            }),
            Err(error) => Err(EditorPersistenceEncodingFailure {
                checkpoint: self,
                error,
            }),
        }
    }
}

/// Refused receipt adoption retains both actual codec output and physical publication evidence.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorPersistencePublicationFailure {
    encoded: EncodedEditorPersistenceCheckpoint,
    receipt: ReplaceReceipt,
}

impl EditorPersistencePublicationFailure {
    pub(super) fn new(
        encoded: EncodedEditorPersistenceCheckpoint,
        receipt: ReplaceReceipt,
    ) -> Self {
        Self { encoded, receipt }
    }

    pub fn into_parts(self) -> (EncodedEditorPersistenceCheckpoint, ReplaceReceipt) {
        (self.encoded, self.receipt)
    }
}

/// An actual already-published leaf failed verification; the fixed output remains available.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorPersistenceObservationFailure {
    encoded: EncodedEditorPersistenceCheckpoint,
    error: FsError,
}

impl EditorPersistenceObservationFailure {
    pub fn into_parts(self) -> (EncodedEditorPersistenceCheckpoint, FsError) {
        (self.encoded, self.error)
    }
}

impl EditorPersistenceCommit {
    /// Verifies actual current file bytes against this exact captured output. The concrete Host
    /// must also bind this leaf to its actual configured parent/name before final installation.
    /// This is recovery evidence, not a fabricated replacement receipt.
    #[doc(hidden)]
    pub fn from_observed_file(
        encoded: EncodedEditorPersistenceCheckpoint,
        file: &FileCapability,
    ) -> Result<Self, EditorPersistenceObservationFailure> {
        let digest = encoded.digest();
        if let Err(error) = file.verify_digest(digest, digest.length()) {
            return Err(EditorPersistenceObservationFailure { encoded, error });
        }
        Ok(Self {
            checkpoint: encoded.into_checkpoint(),
            digest: EditorDocumentDigest::new(digest.length(), *digest.as_bytes()),
        })
    }
}
