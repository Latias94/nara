//! Optional typed source builds, independent of source-only import and Runtime acquisition.

use std::sync::Arc;

use nara_core::ByteLimit;
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};

use crate::{
    AssetImportError, AssetImportSource, AssetMetaIdentityCandidate, AssetPath, AssetProductRef,
    RuntimeAssetCodec, SourceAssetImporter,
};

type BuildReader<'a, E> = dyn FnMut(&AssetPath, ByteLimit) -> Result<Option<Arc<[u8]>>, E> + 'a;

/// A separately selected build operation for a typed source domain.
///
/// Source-only importers do not need this facet or serializable settings. The build owner admits
/// and normalizes the actual settings before lookup, records their canonical serialization and
/// passes those same settings to execution. Implementations must honor [`SourceAssetImporter::limits`]
/// for all build work and retained products, including any build-only inputs. These are trusted
/// Rust promises, not allocation interception or an I/O sandbox.
pub trait SourceAssetBuild: SourceAssetImporter + Sized
where
    Self::Settings: serde::Serialize,
{
    type Codec: RuntimeAssetCodec<Self::Product>;

    const BUILD_ID: &'static str;
    const BUILD_REVISION: u32;

    /// Validates effective settings and orders domain-semantic sets without changing their meaning.
    ///
    /// Do not silently replace admitted values with defaults or discard authored continuity.
    fn normalize_settings(settings: Self::Settings) -> Result<Self::Settings, Diagnostic>;

    /// Builds from tracked immutable inputs; the default executes the existing source importer.
    fn build<E>(
        source: &mut AssetBuildSource<'_, E>,
    ) -> Result<Self::Product, AssetImportError<E>> {
        source.prepare_import::<Self>()
    }

    /// Enumerates every actual primary and auxiliary mapping in the complete typed product.
    ///
    /// This trusted query borrows existing data and must not allocate, perform I/O, mutate the
    /// product or invent unused mappings. Stop and propagate the visitor's exact failure.
    fn visit_products<E>(
        product: &Self::Product,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E>;
}

/// Admitted build metadata, immutable primary bytes and a tracked presence-aware input reader.
///
/// The caller owns scope, observation recording, read limits and allocation admission. `None`
/// means observed absence only; authority, budget and I/O failures remain the caller's exact `E`.
/// Optional observation belongs to this build operation and grants no optional-read capability
/// to an ordinary [`AssetImportSource`].
pub struct AssetBuildSource<'a, E> {
    metadata: &'a AssetMetaIdentityCandidate,
    bytes: Arc<[u8]>,
    reader: &'a mut BuildReader<'a, E>,
}

impl<'a, E> AssetBuildSource<'a, E> {
    #[must_use]
    pub fn new(
        metadata: &'a AssetMetaIdentityCandidate,
        bytes: Arc<[u8]>,
        reader: &'a mut BuildReader<'a, E>,
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

    /// Reads a required input, rejecting observed absence with a bounded static diagnostic.
    pub fn read_dependency(
        &mut self,
        path: &AssetPath,
        limit: ByteLimit,
    ) -> Result<Arc<[u8]>, AssetImportError<E>> {
        self.read_optional_dependency(path, limit)?
            .ok_or_else(missing_dependency)
    }

    /// Reads a possibly absent input while preserving the caller's exact failure.
    pub fn read_optional_dependency(
        &mut self,
        path: &AssetPath,
        limit: ByteLimit,
    ) -> Result<Option<Arc<[u8]>>, AssetImportError<E>> {
        (self.reader)(path, limit).map_err(AssetImportError::Source)
    }

    /// Executes an unchanged source importer using this build's required tracked reads.
    pub fn prepare_import<D: SourceAssetImporter>(
        &mut self,
    ) -> Result<D::Product, AssetImportError<E>> {
        let metadata = self.metadata;
        let bytes = Arc::clone(&self.bytes);
        let mut reader = |path: &AssetPath, limit: ByteLimit| {
            (self.reader)(path, limit)
                .map_err(RequiredReadError::Source)?
                .ok_or(RequiredReadError::Missing)
        };
        let mut source = AssetImportSource::new(metadata, bytes, &mut reader);
        D::prepare(&mut source).map_err(|error| match error {
            AssetImportError::Domain(diagnostic) => AssetImportError::Domain(diagnostic),
            AssetImportError::Source(RequiredReadError::Source(error)) => {
                AssetImportError::Source(error)
            }
            AssetImportError::Source(RequiredReadError::Missing) => missing_dependency(),
        })
    }
}

enum RequiredReadError<E> {
    Missing,
    Source(E),
}

fn missing_dependency<E>() -> AssetImportError<E> {
    AssetImportError::domain(Diagnostic::error(
        DiagnosticCode::new("asset.build.required-input-missing")
            .expect("fixed valid build diagnostic code"),
        SafeSummary::new("A required source build input is absent")
            .expect("fixed safe build diagnostic summary"),
    ))
}
