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

    #[must_use]
    pub const fn local_checkout_identity(&self) -> Option<FileIdentity> {
        match &self.inner {
            NaraDependencyReceiptInner::LocalCheckout {
                checkout_identity, ..
            } => Some(*checkout_identity),
            NaraDependencyReceiptInner::ImmutableGit { .. } => None,
        }
    }

    #[must_use]
    pub const fn local_destination_parent_identity(&self) -> Option<FileIdentity> {
        match &self.inner {
            NaraDependencyReceiptInner::LocalCheckout {
                destination_parent_identity,
                ..
            } => Some(*destination_parent_identity),
            NaraDependencyReceiptInner::ImmutableGit { .. } => None,
        }
    }

    #[must_use]
    pub const fn local_descendant_depth(&self) -> Option<usize> {
        match &self.inner {
            NaraDependencyReceiptInner::LocalCheckout {
                descendant_depth, ..
            } => Some(*descendant_depth),
            NaraDependencyReceiptInner::ImmutableGit { .. } => None,
        }
    }

    #[must_use]
    pub const fn immutable_git_repository_digest(&self) -> Option<ContentDigest> {
        match &self.inner {
            NaraDependencyReceiptInner::ImmutableGit {
                repository_digest, ..
            } => Some(*repository_digest),
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
    pub(crate) creation_command_arguments: String,
    pub(crate) receipt: NaraDependencyReceipt,
    pub(crate) revalidation: PreparedDependencyRevalidation,
}

pub(crate) enum PreparedDependencyRevalidation {
    LocalCheckout {
        checkout: DirectoryCapability,
        destination_parent: LocalDependencyParent,
        destination_parent_identity: FileIdentity,
    },
    ImmutableGit,
}

impl PreparedDependencyRevalidation {
    pub(crate) fn revalidate(
        &self,
        publication_parent: &DirectoryCapability,
    ) -> Result<(), DependencyRevalidationError> {
        let Self::LocalCheckout {
            checkout,
            destination_parent,
            destination_parent_identity,
        } = self
        else {
            return Ok(());
        };
        let authorized_parent = match destination_parent {
            LocalDependencyParent::CheckoutRoot => checkout.share(),
            LocalDependencyParent::Descendant(path) => checkout
                .open_directory(path)
                .map_err(DependencyRevalidationError::LocalParentUnavailable)?,
        };
        if authorized_parent.identity() != *destination_parent_identity
            || publication_parent.identity() != *destination_parent_identity
        {
            return Err(DependencyRevalidationError::LocalParentMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub(crate) enum DependencyPreparationError {
    #[error("local dependency destination parent could not be resolved")]
    LocalParentUnavailable(#[source] FsError),
    #[error("local dependency destination parent is outside the authorized checkout relation")]
    LocalParentMismatch,
}

#[derive(Debug, Error)]
pub(crate) enum DependencyRevalidationError {
    #[error("local dependency destination parent could not be reopened")]
    LocalParentUnavailable(#[source] FsError),
    #[error(
        "local dependency destination parent no longer matches the authorized checkout relation"
    )]
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
            let author_checkout = match destination_parent.depth() {
                0 => ".".to_owned(),
                depth => std::iter::repeat_n("..", depth)
                    .collect::<Vec<_>>()
                    .join("/"),
            };
            Ok(PreparedDependency {
                manifest_fragment: format!("path = \"{cargo_path}\""),
                creation_command_arguments: format!("--local-checkout {author_checkout}"),
                receipt: NaraDependencyReceipt {
                    inner: NaraDependencyReceiptInner::LocalCheckout {
                        checkout_identity: checkout.identity(),
                        destination_parent_identity: publication_parent.identity(),
                        descendant_depth: destination_parent.depth(),
                    },
                },
                revalidation: PreparedDependencyRevalidation::LocalCheckout {
                    destination_parent_identity: publication_parent.identity(),
                    checkout,
                    destination_parent,
                },
            })
        }
        NaraDependencySourceInner::ImmutableGit {
            repository,
            revision,
        } => Ok(PreparedDependency {
            manifest_fragment: format!("git = \"{repository}\", rev = \"{revision}\""),
            creation_command_arguments: format!("--git {repository} --rev {revision}"),
            receipt: NaraDependencyReceipt {
                inner: NaraDependencyReceiptInner::ImmutableGit {
                    repository_digest: ContentDigest::of_bytes(repository.as_bytes()),
                    revision,
                },
            },
            revalidation: PreparedDependencyRevalidation::ImmutableGit,
        }),
    }
}

pub(super) fn validate_git_repository(repository: &str) -> Result<(), NaraDependencySourceError> {
    const MAX_GIT_REPOSITORY_BYTES: usize = 2_048;
    let valid = repository.len() <= MAX_GIT_REPOSITORY_BYTES
        && repository
            .strip_prefix("https://")
            .and_then(|authority_and_path| authority_and_path.split_once('/'))
            .is_some_and(|(authority, path)| {
                valid_git_authority(authority)
                    && !path.is_empty()
                    && path.split('/').all(|component| {
                        !component.is_empty()
                            && !matches!(component, "." | "..")
                            && component.bytes().all(valid_git_path_byte)
                    })
            });
    if valid {
        Ok(())
    } else {
        Err(NaraDependencySourceError {
            kind: NaraDependencySourceErrorKind::UnsupportedGitRepository,
        })
    }
}

fn valid_git_authority(authority: &str) -> bool {
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let (host, port) = authority
        .rsplit_once(':')
        .map_or((authority, None), |(host, port)| (host, Some(port)));
    if !host.split('.').all(|label| {
        !label.is_empty()
            && label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }) {
        return false;
    }
    port.is_none_or(|port| {
        !port.is_empty()
            && port.bytes().all(|byte| byte.is_ascii_digit())
            && port.parse::<u16>().is_ok_and(|port| port != 0)
    })
}

fn valid_git_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~')
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
