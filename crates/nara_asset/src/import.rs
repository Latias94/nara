//! Typed source preparation, independent of project Hosts, Scenes and presentation.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_core::ByteLimit;
use nara_diagnostic::Diagnostic;

use crate::{
    AssetDomain, AssetMetaIdentityCandidate, AssetPath, AssetRef, AssetReferenceDemand,
    AssetSourceKind,
};

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

    /// Visits the complete inventory of durable source references in an admitted product.
    ///
    /// The concrete owner enumerates every present reference, including references nested in
    /// compound products, with its expected source kind and current demand. Borrow references
    /// and source kinds from existing data; do not construct owned inventory or query scratch.
    /// Both Immediate and Deferred demands require a valid target; absence is modeled separately.
    /// Only importers whose products contain no source references may use the empty default.
    ///
    /// This trusted pure query is bounded by the admitted product and the original import work
    /// reservation. It performs no I/O, allocation, mutation, dependency loading or publication.
    /// Stop at the first visitor failure and return that concrete error unchanged.
    ///
    /// The Host owns closure bounds, deduplication, source-kind validation and actual dependency
    /// import. Reading raw dependency bytes does not establish an imported product. Runtime
    /// dependency inventory remains the separate `RuntimeAssetCodec` contract.
    fn visit_source_dependencies<E>(
        _product: &Self::Product,
        _visitor: impl FnMut(&AssetRef, &AssetSourceKind, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        Ok(())
    }
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

    /// Reads a declared raw auxiliary input and preserves the caller's exact failure.
    ///
    /// This read does not import a dependency product. Product references must be reported through
    /// [`SourceAssetImporter::visit_source_dependencies`] for the Host to validate and import.
    pub fn read_dependency(
        &mut self,
        path: &AssetPath,
        limit: ByteLimit,
    ) -> Result<Arc<[u8]>, AssetImportError<E>> {
        (self.reader)(path, limit).map_err(AssetImportError::Source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetProductRef, ImportedProductId, StableAssetId};

    type TestDependency = (AssetRef, AssetSourceKind, AssetReferenceDemand);

    struct TestProduct {
        direct: TestDependency,
        nested: [TestDependency; 3],
    }

    struct DependencyImporter;

    impl AssetDomain for DependencyImporter {
        type Product = TestProduct;
    }

    impl SourceAssetImporter for DependencyImporter {
        type Settings = ();

        const SOURCE_KIND: &'static str = "test.dependencies";

        fn limits() -> AssetImportLimits {
            let bytes = ByteLimit::new(4096).unwrap();
            AssetImportLimits::new(bytes, bytes, bytes)
        }

        fn prepare<E>(
            _source: &mut AssetImportSource<'_, E>,
        ) -> Result<Self::Product, AssetImportError<E>> {
            Ok(dependency_product())
        }

        fn visit_source_dependencies<E>(
            product: &Self::Product,
            mut visitor: impl FnMut(&AssetRef, &AssetSourceKind, AssetReferenceDemand) -> Result<(), E>,
        ) -> Result<(), E> {
            for (reference, kind, demand) in std::iter::once(&product.direct).chain(&product.nested)
            {
                visitor(reference, kind, *demand)?;
            }
            Ok(())
        }
    }

    struct ReferenceFreeImporter;

    impl AssetDomain for ReferenceFreeImporter {
        type Product = ();
    }

    impl SourceAssetImporter for ReferenceFreeImporter {
        type Settings = ();

        const SOURCE_KIND: &'static str = "test.reference-free";

        fn limits() -> AssetImportLimits {
            AssetImportLimits::new(ByteLimit::ONE, ByteLimit::ONE, ByteLimit::ONE)
        }

        fn prepare<E>(
            _source: &mut AssetImportSource<'_, E>,
        ) -> Result<Self::Product, AssetImportError<E>> {
            Ok(())
        }
    }

    fn dependency_product() -> TestProduct {
        let source = StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap();
        let auxiliary =
            ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap();
        TestProduct {
            direct: (
                AssetRef::path("textures/player.png").unwrap(),
                AssetSourceKind::Image,
                AssetReferenceDemand::Immediate,
            ),
            nested: [
                (
                    AssetRef::StableId(source),
                    AssetSourceKind::Scene,
                    AssetReferenceDemand::Deferred,
                ),
                (
                    AssetRef::Product(AssetProductRef::new(source, auxiliary).unwrap()),
                    AssetSourceKind::Other("test.catalogue".to_owned()),
                    AssetReferenceDemand::Immediate,
                ),
                (
                    AssetRef::path("textures/player.png").unwrap(),
                    AssetSourceKind::Image,
                    AssetReferenceDemand::Deferred,
                ),
            ],
        }
    }

    #[test]
    fn source_inventory_borrows_every_reference_and_expected_kind_with_its_demand() {
        let product = dependency_product();
        let expected = [
            (&product.direct, AssetReferenceDemand::Immediate),
            (&product.nested[0], AssetReferenceDemand::Deferred),
            (&product.nested[1], AssetReferenceDemand::Immediate),
            (&product.nested[2], AssetReferenceDemand::Deferred),
        ];
        let mut visits = 0;

        let result =
            DependencyImporter::visit_source_dependencies(&product, |reference, kind, demand| {
                let (dependency, expected_demand) = expected[visits];
                assert!(std::ptr::eq(reference, &dependency.0));
                assert!(std::ptr::eq(kind, &dependency.1));
                assert_eq!(demand, expected_demand);
                visits += 1;
                Ok::<(), std::convert::Infallible>(())
            });

        assert_eq!(result, Ok(()));
        assert_eq!(visits, expected.len());
    }

    #[test]
    fn source_inventory_stops_and_returns_the_exact_visitor_failure() {
        let product = dependency_product();

        for fail_at in 0..4 {
            let failure = Box::new(37_u8);
            let original = std::ptr::from_ref(failure.as_ref());
            let mut failure = Some(failure);
            let mut visits = 0;

            let result = DependencyImporter::visit_source_dependencies(&product, |_, _, _| {
                let current = visits;
                visits += 1;
                if current == fail_at {
                    Err(failure.take().unwrap())
                } else {
                    Ok(())
                }
            });

            let returned = result.unwrap_err();
            assert!(std::ptr::eq(returned.as_ref(), original));
            assert_eq!(*returned, 37);
            assert_eq!(visits, fail_at + 1);
        }
    }

    #[test]
    fn source_inventory_defaults_to_no_references() {
        let mut visits = 0;
        let result = ReferenceFreeImporter::visit_source_dependencies(&(), |_, _, _| {
            visits += 1;
            Err("a reference-free importer must not call the visitor")
        });

        assert_eq!(result, Ok(()));
        assert_eq!(visits, 0);
    }
}
