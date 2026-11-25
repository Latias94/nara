//! Concrete Runtime formats, independent of authoring source acquisition.

use std::{
    any::Any,
    error::Error,
    fmt,
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
};

use nara_core::ByteLimit;
use nara_core::diagnostic::ReportDiagnostic as Diagnostic;

use crate::{AssetProductRef, AssetReferenceDemand, ImportedProductId};

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
/// Retained output includes the inline product and all allocations it owns, excluding encoded
/// input and Host wrapper/base storage. After decode, the caller can refund unused output through
/// [`RuntimeAssetCodec::runtime_retained_bytes`], bounded by the original promise. Codecs enforce
/// their format bounds; these promises are not allocation interception.
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

    /// Reports the complete modeled retention of a returned decoded product.
    ///
    /// Include `size_of::<P>()`, owned strings/collections and shared allocations kept by this
    /// product. Exclude the caller's encoded input, immutable base, wrapper and temporary decode
    /// work. The result must fit [`AssetRuntimeLimits::retained_bytes`] as reserved before decode;
    /// a product cannot borrow unreserved capacity after allocation. Multiple decoded products
    /// are charged independently; cloning their shared Host wrapper retains one charge.
    ///
    /// This trusted bounded query only borrows the actual decoded value. It must not allocate
    /// product/query scratch, perform I/O, mutate/publish state or acquire dependencies. A bounded
    /// domain diagnostic may report a value whose retained allocations this concrete codec cannot
    /// account for.
    fn runtime_retained_bytes(product: &P) -> Result<usize, AssetRuntimeError>;

    /// Visits every actual Product reference and its current load demand.
    ///
    /// The content Host aggregates references to the same Product with Immediate precedence and
    /// compares the complete result with the member's Required/Reference declarations before
    /// following any outgoing edge. Both demands require membership in the captured base.
    /// Enumerate all present references, including nested products in a compound value. Optional
    /// missing-allowed catalogue metadata is not a payload reference. A codec with no references
    /// must implement that empty inventory explicitly.
    ///
    /// This trusted bounded query borrows existing values and grants no I/O, allocation, mutation,
    /// dependency loading or publication. Stop and propagate the visitor's concrete failure
    /// unchanged. The original decode work promise covers this inventory operation.
    fn visit_runtime_dependencies<E>(
        product: &P,
        visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E>;

    /// Checks the complete logical identity carried by a decoded product.
    ///
    /// The content Host calls this for every product mapped to the encoded member, including
    /// its enclosing source. A valid payload for another source must not acquire the catalog's
    /// identity merely because its auxiliary ID or display name happens to match. This trusted
    /// pure query performs no I/O, allocation, mutation or publication. Catalog membership and
    /// residency remain separate Host obligations.
    #[must_use]
    fn contains_runtime_product(product: &P, reference: AssetProductRef) -> bool;
}

/// A failure while decoding or validating an asset from a verified Runtime member.
#[derive(Debug)]
pub enum AssetRuntimeDecodeError<E> {
    Codec(AssetRuntimeError),
    ProductIdentityMismatch,
    Dependency(E),
    RetainedBytes {
        required: usize,
        limit: usize,
    },
    /// The original unwind payload, for callers with an enclosing acquisition boundary.
    Panicked(Box<dyn Any + Send>),
}

impl<E> fmt::Display for AssetRuntimeDecodeError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codec(error) => fmt::Display::fmt(error, formatter),
            Self::ProductIdentityMismatch => {
                formatter.write_str("the decoded Runtime asset does not contain a declared product")
            }
            Self::Dependency(_) => {
                formatter.write_str("the Runtime asset dependencies were rejected")
            }
            Self::RetainedBytes { required, limit } => write!(
                formatter,
                "the decoded Runtime asset requires {required} retained bytes but only {limit} are covered"
            ),
            Self::Panicked(_) => {
                formatter.write_str("the Runtime asset decode or validation panicked")
            }
        }
    }
}

impl<E: Error + 'static> Error for AssetRuntimeDecodeError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Codec(error) => Some(error),
            Self::Dependency(error) => Some(error),
            _ => None,
        }
    }
}

/// Decodes and validates a verified member, returning its CPU product and retained byte count.
///
/// Supply every product mapped to the member and the retained limit admitted before decode.
/// The encoded owner is released after decode. Both membership checks run before any dependency
/// visits. `finish_dependencies` consumes the caller's inventory to check it against the declared
/// dependencies before the codec measures retained bytes.
pub fn decode_runtime_asset<D, C, E, Inventory>(
    encoded: impl AsRef<[u8]>,
    products: &[AssetProductRef],
    retained_limit: ByteLimit,
    dependencies: Inventory,
    mut visit_dependency: impl FnMut(
        &mut Inventory,
        AssetProductRef,
        AssetReferenceDemand,
    ) -> Result<(), E>,
    finish_dependencies: impl FnOnce(Inventory) -> Result<(), E>,
) -> Result<(D::Product, usize), AssetRuntimeDecodeError<E>>
where
    D: AssetDomain,
    C: RuntimeAssetCodec<D::Product>,
{
    catch_unwind(AssertUnwindSafe(move || {
        let mut dependencies = dependencies;
        let encoded = encoded;
        let value = C::decode_runtime(encoded.as_ref()).map_err(AssetRuntimeDecodeError::Codec)?;
        drop(encoded);

        for &reference in products {
            if !D::contains_product(&value, reference.product())
                || !C::contains_runtime_product(&value, reference)
            {
                return Err(AssetRuntimeDecodeError::ProductIdentityMismatch);
            }
        }
        C::visit_runtime_dependencies(&value, |product, demand| {
            visit_dependency(&mut dependencies, product, demand)
        })
        .map_err(AssetRuntimeDecodeError::Dependency)?;
        finish_dependencies(dependencies).map_err(AssetRuntimeDecodeError::Dependency)?;

        let retained = C::runtime_retained_bytes(&value).map_err(AssetRuntimeDecodeError::Codec)?;
        let inline = size_of::<D::Product>();
        if retained < inline {
            return Err(AssetRuntimeDecodeError::RetainedBytes {
                required: inline,
                limit: retained,
            });
        }
        if retained > retained_limit.get() {
            return Err(AssetRuntimeDecodeError::RetainedBytes {
                required: retained,
                limit: retained_limit.get(),
            });
        }
        Ok((value, retained))
    }))
    .unwrap_or_else(|payload| Err(AssetRuntimeDecodeError::Panicked(payload)))
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
