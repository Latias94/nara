use std::fmt::{self, Debug, Formatter};

use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity, FsError, RelativePath};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalDependencyParent {
    CheckoutRoot,
    Descendant(RelativePath),
}

impl LocalDependencyParent {
    pub(crate) fn depth(&self) -> usize {
        match self {
            Self::CheckoutRoot => 0,
            Self::Descendant(path) => path.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NaraDependencySourceKind {
    LocalCheckout,
    ImmutableGit,
}

pub struct NaraDependencySource {
    inner: NaraDependencySourceInner,
}

enum NaraDependencySourceInner {
    LocalCheckout {
        checkout: DirectoryCapability,
        destination_parent: LocalDependencyParent,
    },
    ImmutableGit {
        repository: String,
        revision: String,
    },
}

impl NaraDependencySource {
    #[must_use]
    pub fn local(checkout: DirectoryCapability, destination_parent: LocalDependencyParent) -> Self {
        Self {
            inner: NaraDependencySourceInner::LocalCheckout {
                checkout,
                destination_parent,
            },
        }
    }

    pub fn immutable_git(
        repository: impl Into<String>,
        revision: impl Into<String>,
    ) -> Result<Self, NaraDependencySourceError> {
        let repository = repository.into();
        validate_git_repository(&repository)?;
        let revision = revision.into();
        validate_git_revision(&revision)?;
        Ok(Self {
            inner: NaraDependencySourceInner::ImmutableGit {
                repository,
                revision,
            },
        })
    }

    #[must_use]
    pub const fn kind(&self) -> NaraDependencySourceKind {
        match &self.inner {
            NaraDependencySourceInner::LocalCheckout { .. } => {
                NaraDependencySourceKind::LocalCheckout
            }
            NaraDependencySourceInner::ImmutableGit { .. } => {
                NaraDependencySourceKind::ImmutableGit
            }
        }
    }
}

impl Debug for NaraDependencySource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NaraDependencySource")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NaraDependencyReceipt {
    inner: NaraDependencyReceiptInner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NaraDependencyReceiptInner {
    LocalCheckout {
        checkout_identity: FileIdentity,
        destination_parent_identity: FileIdentity,
        descendant_depth: usize,
    },
    ImmutableGit {
        repository_digest: ContentDigest,
        revision: String,
    },
}

impl NaraDependencyReceipt {
    #[must_use]
    pub const fn kind(&self) -> NaraDependencySourceKind {
        match &self.inner {
            NaraDependencyReceiptInner::LocalCheckout { .. } => {
                NaraDependencySourceKind::LocalCheckout
            }
            NaraDependencyReceiptInner::ImmutableGit { .. } => {
                NaraDependencySourceKind::ImmutableGit
            }
        }
    }

    #[must_use]
    pub fn immutable_git_revision(&self) -> Option<&str> {
        match &self.inner {
            NaraDependencyReceiptInner::ImmutableGit { revision, .. } => Some(revision),
            NaraDependencyReceiptInner::LocalCheckout { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NaraDependencySourceErrorKind {
    UnsupportedGitRepository,
    InvalidGitRevision,
}

#[derive(Debug, Error)]
#[error("Nara dependency source is invalid: {kind:?}")]
pub struct NaraDependencySourceError {
    kind: NaraDependencySourceErrorKind,
}

impl NaraDependencySourceError {
    #[must_use]
    pub const fn kind(&self) -> NaraDependencySourceErrorKind {
        self.kind
    }
}

pub(crate) struct PreparedDependency {
    pub(crate) manifest_fragment: String,
    pub(crate) receipt: NaraDependencyReceipt,
}

#[derive(Debug, Error)]
pub(crate) enum DependencyPreparationError {
    #[error("local dependency destination parent could not be resolved")]
    LocalParentUnavailable(#[source] FsError),
    #[error("local dependency destination parent is outside the authorized checkout relation")]
    LocalParentMismatch,
}

pub(crate) fn prepare_dependency(
    source: NaraDependencySource,
    publication_parent: &DirectoryCapability,
) -> Result<PreparedDependency, DependencyPreparationError> {
    match source.inner {
        NaraDependencySourceInner::LocalCheckout {
            checkout,
            destination_parent,
        } => {
            let authorized_parent = match &destination_parent {
                LocalDependencyParent::CheckoutRoot => checkout.share(),
                LocalDependencyParent::Descendant(path) => checkout
                    .open_directory(path)
                    .map_err(DependencyPreparationError::LocalParentUnavailable)?,
            };
            if authorized_parent.identity() != publication_parent.identity() {
                return Err(DependencyPreparationError::LocalParentMismatch);
            }

            let parent_jumps = destination_parent.depth().saturating_add(1);
            let cargo_path = std::iter::repeat_n("..", parent_jumps)
                .collect::<Vec<_>>()
                .join("/");
            Ok(PreparedDependency {
                manifest_fragment: format!("path = \"{cargo_path}\""),
                receipt: NaraDependencyReceipt {
                    inner: NaraDependencyReceiptInner::LocalCheckout {
                        checkout_identity: checkout.identity(),
                        destination_parent_identity: publication_parent.identity(),
                        descendant_depth: destination_parent.depth(),
                    },
                },
            })
        }
        NaraDependencySourceInner::ImmutableGit {
            repository,
            revision,
        } => Ok(PreparedDependency {
            manifest_fragment: format!("git = \"{repository}\", rev = \"{revision}\""),
            receipt: NaraDependencyReceipt {
                inner: NaraDependencyReceiptInner::ImmutableGit {
                    repository_digest: ContentDigest::of_bytes(repository.as_bytes()),
                    revision,
                },
            },
        }),
    }
}

fn validate_git_repository(repository: &str) -> Result<(), NaraDependencySourceError> {
    const MAX_GIT_REPOSITORY_BYTES: usize = 2_048;
    let valid = repository.len() <= MAX_GIT_REPOSITORY_BYTES
        && repository.starts_with("https://")
        && repository.len() > "https://".len()
        && repository.bytes().all(|byte| {
            byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\'' | b'\\' | b'#' | b'?' | b'&')
        });
    if valid {
        Ok(())
    } else {
        Err(NaraDependencySourceError {
            kind: NaraDependencySourceErrorKind::UnsupportedGitRepository,
        })
    }
}

fn validate_git_revision(revision: &str) -> Result<(), NaraDependencySourceError> {
    let exact_object_id = matches!(revision.len(), 40 | 64)
        && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        && revision.bytes().all(|byte| !byte.is_ascii_uppercase());
    if exact_object_id {
        Ok(())
    } else {
        Err(NaraDependencySourceError {
            kind: NaraDependencySourceErrorKind::InvalidGitRevision,
        })
    }
}
