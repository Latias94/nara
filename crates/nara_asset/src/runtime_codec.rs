//! Concrete Runtime formats, independent of authoring source acquisition.

use std::{error::Error, fmt};

use nara_core::ByteLimit;
use nara_diagnostic::Diagnostic;

use crate::ImportedProductId;

/// The typed CPU product family shared by independently selected operation facets.
///
/// This family alone installs no source importer, Runtime decoder, I/O or Host service. Distinct
/// families may deliberately use the same Rust product type without sharing identity or storage.
pub trait AssetDomain: Send + Sync + 'static {
    type Product: Send + Sync + 'static;

    /// Reports whether an admitted CPU product contains the selected logical product ID.
    ///
    /// This is a trusted pure query over existing product data: implementations must not allocate,
    /// perform I/O, load or mutate products, or publish state. The concrete domain owns auxiliary
    /// membership. The default admits only the primary product; this query supplies no execution
    /// or access authority.
    #[must_use]
    fn contains_product(_product: &Self::Product, id: ImportedProductId) -> bool {
        id == ImportedProductId::PRIMARY
    }
}

/// Conservative reservations for one trusted Runtime codec operation.
///
/// The caller bounds encoded input before reading and reserves work and retained output before
/// decoding. Encoding also reserves work and encoded output while retaining the original product.
/// Codecs enforce their format bounds; these promises are not allocation interception.
#[derive(Debug, Clone, Copy)]
pub struct AssetRuntimeLimits {
    encoded_bytes: ByteLimit,
    work_bytes: ByteLimit,
    retained_bytes: ByteLimit,
}

impl AssetRuntimeLimits {
    #[must_use]
    pub const fn new(
        encoded_bytes: ByteLimit,
        work_bytes: ByteLimit,
        retained_bytes: ByteLimit,
    ) -> Self {
        Self {
            encoded_bytes,
            work_bytes,
            retained_bytes,
        }
    }

    #[must_use]
    pub const fn encoded_bytes(self) -> ByteLimit {
        self.encoded_bytes
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

#[derive(Debug)]
pub struct AssetRuntimeError(Box<Diagnostic>);

impl AssetRuntimeError {
    #[must_use]
    pub fn new(diagnostic: Diagnostic) -> Self {
        Self(Box::new(diagnostic))
    }

    #[must_use]
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.0
    }

    #[must_use]
    pub fn into_diagnostic(self) -> Diagnostic {
        *self.0
    }
}

impl fmt::Display for AssetRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the Runtime asset format was rejected")
    }
}

impl Error for AssetRuntimeError {}

/// A compiled codec for one declared processed Runtime format and typed asset family.
///
/// Selection grants no source/import, target cook, filesystem or publication capability. A
/// source-only family need not implement this trait, and a Runtime-only family need not implement
/// `SourceAssetImporter`. Codecs accept only the selected immutable bytes or typed product, perform
/// no external effects and never acquire authoring source, metadata or an import cache.
///
/// `RUNTIME_KIND` and `RUNTIME_VERSION` select the persistent format. A Host can select different
/// codecs for the same family by requiring `RuntimeAssetCodec<D::Product>`; codec identity does
/// not create a second logical product family. The concrete codec owns version,
/// shape, product identity, semantic checks and deterministic encoding. The Host independently
/// verifies target, expected Schema/format and member integrity before candidate publication.
pub trait RuntimeAssetCodec<P: Send + Sync + 'static>: Send + Sync + 'static {
    const RUNTIME_KIND: &'static str;
    const RUNTIME_VERSION: u32;

    fn runtime_limits() -> AssetRuntimeLimits;

    fn encode_runtime(product: &P) -> Result<Box<[u8]>, AssetRuntimeError>;

    fn decode_runtime(bytes: &[u8]) -> Result<P, AssetRuntimeError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PrimaryDomain;

    impl AssetDomain for PrimaryDomain {
        type Product = String;
    }

    #[test]
    fn default_domain_membership_admits_only_the_primary_product() {
        let product = String::from("admitted primary product");
        let auxiliary =
            ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap();

        assert!(PrimaryDomain::contains_product(
            &product,
            ImportedProductId::PRIMARY
        ));
        assert!(!PrimaryDomain::contains_product(&product, auxiliary));
    }
}
