use std::fmt::{self, Debug, Formatter};

use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity, FsError, RelativePath};
use thiserror::Error;

/// The selected checkout and publication parent, relative to one shared authority.
///
/// Each `None` coordinate denotes the common ancestor itself. Both endpoints are
/// resolved through this capability session; independently opened native handles
/// cannot establish the relation.
pub struct LocalDependencyRelation {
    common_ancestor: DirectoryCapability,
    checkout: Option<RelativePath>,
    destination_parent: Option<RelativePath>,
}

impl LocalDependencyRelation {
    #[must_use]
    pub fn new(
        common_ancestor: DirectoryCapability,
        checkout: Option<RelativePath>,
        destination_parent: Option<RelativePath>,
    ) -> Self {
        Self {
            common_ancestor,
            checkout,
            destination_parent,
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
    LocalCheckout(LocalDependencyRelation),
    ImmutableGit {
        repository: String,
        revision: String,
    },
}

impl NaraDependencySource {
    #[must_use]
    pub fn local(relation: LocalDependencyRelation) -> Self {
        Self {
            inner: NaraDependencySourceInner::LocalCheckout(relation),
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
            NaraDependencySourceInner::LocalCheckout(_) => NaraDependencySourceKind::LocalCheckout,
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
        cargo_path: String,
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
    pub(crate) fn local_cargo_path(&self) -> Option<&str> {
        match &self.inner {
            NaraDependencyReceiptInner::LocalCheckout { cargo_path, .. } => Some(cargo_path),
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
        relation: LocalDependencyRelation,
        checkout_identity: FileIdentity,
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
            relation,
            checkout_identity,
            destination_parent_identity,
        } = self
        else {
            return Ok(());
        };
        let checkout = resolve_endpoint(&relation.common_ancestor, relation.checkout.as_ref())
            .map_err(DependencyRevalidationError::LocalCheckoutUnavailable)?;
        if checkout.identity() != *checkout_identity {
            return Err(DependencyRevalidationError::LocalCheckoutMismatch);
        }
        let authorized_parent = resolve_endpoint(
            &relation.common_ancestor,
            relation.destination_parent.as_ref(),
        )
        .map_err(DependencyRevalidationError::LocalParentUnavailable)?;
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
    #[error("local dependency checkout could not be resolved through the common ancestor")]
    LocalCheckoutUnavailable(#[source] FsError),
    #[error("local dependency destination parent could not be resolved")]
    LocalParentUnavailable(#[source] FsError),
    #[error("local dependency destination parent does not match the common-ancestor relation")]
    LocalParentMismatch,
    #[error("local dependency coordinate is not UTF-8 and cannot be represented in Cargo.toml")]
    NonUtf8LocalCoordinate,
}

#[derive(Debug, Error)]
pub(crate) enum DependencyRevalidationError {
    #[error("local dependency checkout could not be reopened through the common ancestor")]
    LocalCheckoutUnavailable(#[source] FsError),
    #[error("local dependency checkout no longer matches the prepared relation")]
    LocalCheckoutMismatch,
    #[error("local dependency destination parent could not be reopened")]
    LocalParentUnavailable(#[source] FsError),
    #[error("local dependency destination parent no longer matches the common-ancestor relation")]
    LocalParentMismatch,
}

pub(crate) fn prepare_dependency(
    source: NaraDependencySource,
    publication_parent: &DirectoryCapability,
) -> Result<PreparedDependency, DependencyPreparationError> {
    match source.inner {
        NaraDependencySourceInner::LocalCheckout(relation) => {
            let checkout = resolve_endpoint(&relation.common_ancestor, relation.checkout.as_ref())
                .map_err(DependencyPreparationError::LocalCheckoutUnavailable)?;
            let authorized_parent = resolve_endpoint(
                &relation.common_ancestor,
                relation.destination_parent.as_ref(),
            )
            .map_err(DependencyPreparationError::LocalParentUnavailable)?;
            if authorized_parent.identity() != publication_parent.identity() {
                return Err(DependencyPreparationError::LocalParentMismatch);
            }

            let parent_depth = relation
                .destination_parent
                .as_ref()
                .map_or(0, RelativePath::len);
            let checkout_components = relation
                .checkout
                .as_ref()
                .map(|path| path.components())
                .unwrap_or(&[])
                .iter()
                .map(|component| {
                    component
                        .to_str()
                        .ok_or(DependencyPreparationError::NonUtf8LocalCoordinate)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let cargo_path = relative_checkout_coordinate(parent_depth + 1, &checkout_components);
            let author_checkout = relative_checkout_coordinate(parent_depth, &checkout_components);
            Ok(PreparedDependency {
                manifest_fragment: format!("path = {}", toml::Value::String(cargo_path.clone())),
                creation_command_arguments: format!(
                    "--local-checkout '{}'",
                    author_checkout.replace('\'', "''")
                ),
                receipt: NaraDependencyReceipt {
                    inner: NaraDependencyReceiptInner::LocalCheckout {
                        checkout_identity: checkout.identity(),
                        destination_parent_identity: publication_parent.identity(),
                        cargo_path,
                    },
                },
                revalidation: PreparedDependencyRevalidation::LocalCheckout {
                    destination_parent_identity: publication_parent.identity(),
                    checkout_identity: checkout.identity(),
                    relation,
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

fn resolve_endpoint(
    common_ancestor: &DirectoryCapability,
    coordinate: Option<&RelativePath>,
) -> Result<DirectoryCapability, FsError> {
    coordinate.map_or_else(
        || Ok(common_ancestor.share()),
        |path| common_ancestor.open_directory(path),
    )
}

fn relative_checkout_coordinate(parent_jumps: usize, checkout_components: &[&str]) -> String {
    let coordinate = std::iter::repeat_n("..", parent_jumps)
        .chain(checkout_components.iter().copied())
        .collect::<Vec<_>>()
        .join("/");
    if coordinate.is_empty() {
        ".".to_owned()
    } else {
        coordinate
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
