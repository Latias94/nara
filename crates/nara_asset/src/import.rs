//! Typed source preparation, independent of project Hosts, Scenes and presentation.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_core::ByteLimit;
use nara_diagnostic::Diagnostic;

use crate::{AssetDomain, AssetMetaIdentityCandidate, AssetPath};

type SourceReader<'a, E> = dyn FnMut(&AssetPath, ByteLimit) -> Result<Arc<[u8]>, E> + 'a;

/// Conservative reservations promised by trusted source-import code.
///
/// Work includes parsing settings and temporary values. Retained bytes cover the product's
/// allocations, including shared input buffers it keeps. The caller accounts for input reads,
/// identity and collection storage separately. These promises do not intercept allocations.
#[derive(Debug, Clone, Copy)]
pub struct AssetImportLimits {
    source_bytes: ByteLimit,
    work_bytes: ByteLimit,
    retained_bytes: ByteLimit,
}

impl AssetImportLimits {
    #[must_use]
    pub const fn new(
        source_bytes: ByteLimit,
        work_bytes: ByteLimit,
        retained_bytes: ByteLimit,
    ) -> Self {
        Self {
            source_bytes,
            work_bytes,
            retained_bytes,
        }
    }

    #[must_use]
    pub const fn source_bytes(self) -> ByteLimit {
        self.source_bytes
    }

    #[must_use]
    pub const fn work_bytes(self) -> ByteLimit {
        self.work_bytes
    }

    #[must_use]
    pub const fn retained_bytes(self) -> ByteLimit {
        self.retained_bytes
    }
}

/// A format/settings rejection or an unchanged error from the caller's scoped source reader.
#[derive(Debug)]
pub enum AssetImportError<E> {
    Domain(Box<Diagnostic>),
    Source(E),
}

impl<E> AssetImportError<E> {
    #[must_use]
    pub fn domain(diagnostic: Diagnostic) -> Self {
        Self::Domain(Box::new(diagnostic))
    }
}

impl<E> Display for AssetImportError<E> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Domain(_) => "asset source preparation was rejected",
            Self::Source(_) => "an asset source input was unavailable",
        })
    }
}

impl<E: Error + 'static> Error for AssetImportError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Domain(_) => None,
            Self::Source(error) => Some(error),
        }
    }
}

/// A stateless typed source/import facet selected by trusted compiled composition.
///
/// `SOURCE_KIND` selects the exact persisted `AssetSourceKind::Other`, not an extension.
/// Preparation receives bounded immutable inputs and returns effect-free CPU data. Every
/// auxiliary input must pass through the supplied reader. Products and destructors must not
/// panic or perform external effects. Runtime decode, cook and Editor settings are separate
/// capabilities; implementing this trait grants none of them.
pub trait SourceAssetImporter: AssetDomain {
    type Settings: serde::de::DeserializeOwned;

    const SOURCE_KIND: &'static str;

    fn limits() -> AssetImportLimits;

    /// Keeps concrete read failures typed without coupling the importer to a project Host.
    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<Self::Product, AssetImportError<E>>;
}

/// An admitted metadata candidate, primary bytes and a caller-authorized auxiliary reader.
///
/// The caller admits primary size/identity and reserves the importer's limits before invoking
/// preparation. Its reader owns path scope, observation deduplication, edge accounting and read
/// bounds. No directory handle, mutable World or project publication authority is exposed.
pub struct AssetImportSource<'a, E> {
    metadata: &'a AssetMetaIdentityCandidate,
    bytes: Arc<[u8]>,
    reader: &'a mut SourceReader<'a, E>,
}

impl<'a, E> AssetImportSource<'a, E> {
    #[must_use]
    pub fn new(
        metadata: &'a AssetMetaIdentityCandidate,
        bytes: Arc<[u8]>,
        reader: &'a mut SourceReader<'a, E>,
    ) -> Self {
        Self {
            metadata,
            bytes,
            reader,
        }
    }

    #[must_use]
    pub const fn metadata(&self) -> &AssetMetaIdentityCandidate {
        self.metadata
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Reads a declared auxiliary input and preserves the caller's exact failure.
    pub fn read_dependency(
        &mut self,
        path: &AssetPath,
        limit: ByteLimit,
    ) -> Result<Arc<[u8]>, AssetImportError<E>> {
        (self.reader)(path, limit).map_err(AssetImportError::Source)
    }
}
