//! Bounded build facts; storage and typed execution share these identities.

use std::{fmt, sync::Arc};

use nara_asset::{AssetProductRef, AssetReferenceDemand, AssetSourcePath};
use nara_core::{ByteLimit, FormatKind, FormatVersion, ItemLimit};
use nara_fs::{ContentDigest, FileLockError, FsError, TemporaryFileFailure};
use serde::{Deserialize, Serialize};

/// Exact byte identity, never an authored Product or a recipe identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactContentDigest {
    length: u64,
    hash: [u8; 32],
}

impl ArtifactContentDigest {
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self::from_content(ContentDigest::of_bytes(bytes))
    }

    #[must_use]
    pub const fn from_content(digest: ContentDigest) -> Self {
        Self {
            length: digest.length(),
            hash: *digest.as_bytes(),
        }
    }

    #[must_use]
    pub const fn content(self) -> ContentDigest {
        ContentDigest::from_parts(self.length, self.hash)
    }

    #[must_use]
    pub const fn length(self) -> u64 {
        self.length
    }

    pub(super) fn hex(self) -> String {
        self.hash.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

/// Closed input/executable meaning, independent of output bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ArtifactRecipeDigest(pub(super) [u8; 32]);

/// Discovery key available before replaying the prior dynamic dependency observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ArtifactSeedDigest(pub(super) [u8; 32]);

impl ArtifactRecipeDigest {
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl ArtifactSeedDigest {
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Host-selected implementation meaning and independently observed implementation bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactBuildDefinition {
    pub id: String,
    pub revision: FormatVersion,
    pub implementation: ArtifactContentDigest,
}

/// The current admitted settings/selection known before dependency discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactBuildSeed {
    pub source: AssetProductRef,
    pub location: Option<AssetSourcePath>,
    pub primary: Option<ArtifactContentDigest>,
    pub definitions: Vec<ArtifactBuildDefinition>,
    pub settings: Vec<u8>,
    pub target: Vec<u8>,
    pub format: FormatKind,
    pub format_version: FormatVersion,
}

impl ArtifactBuildSeed {
    /// Hashes the selected facts without constructing an intermediate encoded buffer.
    /// Admission and allocation limits remain the executing owner's responsibility.
    pub fn digest(&self) -> Result<ArtifactSeedDigest, ArtifactBuildError> {
        super::format::seed_digest(self)
    }

    /// Observations must already be canonical, as returned by a verified stored group.
    pub fn recipe_digest(
        &self,
        observations: &[ArtifactBuildObservation],
    ) -> Result<ArtifactRecipeDigest, ArtifactBuildError> {
        super::format::recipe_digest(self, observations)
    }
}

/// Observed build inputs, distinct from Immediate/Deferred Runtime references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum ArtifactBuildObservation {
    Raw {
        path: AssetSourcePath,
        digest: Option<ArtifactContentDigest>,
    },
    Processed {
        product: AssetProductRef,
        group: ArtifactContentDigest,
        member: ArtifactContentDigest,
        format: FormatKind,
        format_version: FormatVersion,
    },
}

/// One complete encoded member and its actual Product mappings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactGroupMember {
    pub name: String,
    pub digest: ArtifactContentDigest,
    pub format: FormatKind,
    pub format_version: FormatVersion,
    pub products: Vec<AssetProductRef>,
    /// Exact Runtime reference metadata; these are not tracked build inputs.
    pub dependencies: Vec<(AssetProductRef, AssetReferenceDemand)>,
}

/// Trusted operation output. The caller reserves its allocation before executing the encoder.
pub struct ArtifactBuildOutput {
    pub name: String,
    pub format: FormatKind,
    pub format_version: FormatVersion,
    pub products: Vec<AssetProductRef>,
    pub dependencies: Vec<(AssetProductRef, AssetReferenceDemand)>,
    pub bytes: Arc<[u8]>,
}

/// Complete proposed membership, never successful publication merely because it was constructed.
pub struct ArtifactGroupCandidate {
    pub seed: ArtifactBuildSeed,
    pub observations: Vec<ArtifactBuildObservation>,
    pub outputs: Vec<ArtifactBuildOutput>,
}

/// Logical allocation/count bounds for one store and all escaped verified groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactBuildLimits {
    pub metadata_bytes: ByteLimit,
    pub member_bytes: ByteLimit,
    pub group_bytes: ByteLimit,
    pub retained_bytes: ByteLimit,
    pub work_bytes: ByteLimit,
    pub members: ItemLimit,
    pub observations: ItemLimit,
    pub index_entries: ItemLimit,
}

impl Default for ArtifactBuildLimits {
    fn default() -> Self {
        Self {
            metadata_bytes: ByteLimit::new(4 * 1024 * 1024).unwrap(),
            member_bytes: ByteLimit::new(128 * 1024 * 1024).unwrap(),
            group_bytes: ByteLimit::new(512 * 1024 * 1024).unwrap(),
            retained_bytes: ByteLimit::new(1024 * 1024 * 1024).unwrap(),
            work_bytes: ByteLimit::new(512 * 1024 * 1024).unwrap(),
            members: ItemLimit::new(4096).unwrap(),
            observations: ItemLimit::new(8192).unwrap(),
            index_entries: ItemLimit::new(8192).unwrap(),
        }
    }
}

#[derive(Debug)]
pub enum ArtifactBuildError {
    Host(FsError),
    Lock(FileLockError),
    Temporary(TemporaryFileFailure),
    InvalidCache(&'static str),
    InvalidCandidate(&'static str),
    Budget {
        kind: &'static str,
        required: usize,
        limit: usize,
    },
    Overflow,
    Stale,
    Aborted,
    Panicked,
    Domain(Box<nara_diagnostic::Diagnostic>),
    Codec(nara_asset::AssetRuntimeError),
    Image(nara_image::ImageImportError),
    Cook(nara_image::ImageCookError),
    DeterminismConflict,
    /// The index name may have changed; the receipt remains available for recovery observation.
    Uncertain {
        receipt: Box<super::store::ArtifactCommitReceipt>,
        cause: Box<ArtifactBuildError>,
    },
}

impl fmt::Display for ArtifactBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host(error) => write!(f, "artifact filesystem operation failed: {error}"),
            Self::Lock(error) => write!(f, "artifact store lock failed: {error}"),
            Self::Temporary(error) => write!(f, "artifact temporary publication failed: {error}"),
            Self::InvalidCache(reason) => write!(f, "artifact cache is invalid: {reason}"),
            Self::InvalidCandidate(reason) => write!(f, "artifact candidate is invalid: {reason}"),
            Self::Budget {
                kind,
                required,
                limit,
            } => write!(
                f,
                "artifact {kind} requires {required} bytes/items, exceeding {limit}"
            ),
            Self::Overflow => f.write_str("artifact allocation or extent overflow"),
            Self::Stale => f.write_str("artifact inputs or request changed before publication"),
            Self::Aborted => f.write_str("artifact publication was aborted"),
            Self::Panicked => f.write_str("trusted artifact operation panicked"),
            Self::Domain(_) => f.write_str("artifact domain operation was rejected"),
            Self::Codec(error) => write!(f, "artifact Runtime codec failed: {error}"),
            Self::Image(error) => write!(f, "artifact Image import failed: {error}"),
            Self::Cook(error) => write!(f, "artifact Image cook failed: {error}"),
            Self::DeterminismConflict => {
                f.write_str("one closed artifact recipe produced different complete output")
            }
            Self::Uncertain { cause, .. } => {
                write!(f, "artifact index publication is uncertain: {cause}")
            }
        }
    }
}

impl std::error::Error for ArtifactBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Host(error) => Some(error),
            Self::Lock(error) => Some(error),
            Self::Temporary(error) => Some(error),
            Self::Codec(error) => Some(error),
            Self::Image(error) => Some(error),
            Self::Cook(error) => Some(error),
            Self::Uncertain { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}

impl From<FsError> for ArtifactBuildError {
    fn from(value: FsError) -> Self {
        Self::Host(value)
    }
}

impl From<TemporaryFileFailure> for ArtifactBuildError {
    fn from(value: TemporaryFileFailure) -> Self {
        Self::Temporary(value)
    }
}
