//! Immutable typed source-build values with their original encoded and decoded custody.

use std::{
    alloc::Layout,
    fmt,
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, atomic::AtomicUsize},
};

use nara_asset::{AssetReferenceDemand, RuntimeAssetCodec, SourceAssetBuild};
use serde::Serialize;

use super::{
    ArtifactBuildError, ArtifactBuildStore, VerifiedArtifactGroup,
    budget::{ArtifactBuildLease, add, check, mul},
};

/// One immutable decoded source build and its exact verified encoded group.
///
/// Clones share one complete owner and its original store reservations. Borrowing the product
/// never separates it from that owner. Live values belong in the existing typed `Assets<T>`.
pub struct BuiltSourceAsset<P> {
    inner: Arc<BuiltSourceAssetInner<P>>,
}

struct BuiltSourceAssetInner<P> {
    value: Option<P>,
    group: VerifiedArtifactGroup,
    decoded_retained_bytes: usize,
    _lease: ArtifactBuildLease,
}

impl<P> Clone for BuiltSourceAsset<P> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<P> BuiltSourceAsset<P> {
    /// Borrows the product while retaining its complete accounting owner.
    #[must_use]
    pub fn value(&self) -> &P {
        self.inner.value()
    }

    /// Returns the exact immutable encoded group and its original closed artifact recipe.
    #[must_use]
    pub fn group(&self) -> &VerifiedArtifactGroup {
        &self.inner.group
    }

    /// Modeled product, enclosing owner and Arc allocation charge shared by all clones.
    ///
    /// The encoded group retains its separate original charge. This is logical retention,
    /// including allocation alignment, rather than allocator capacity or process memory.
    #[must_use]
    pub fn decoded_retained_bytes(&self) -> usize {
        self.inner.decoded_retained_bytes
    }
}

impl<P> fmt::Debug for BuiltSourceAsset<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BuiltSourceAsset")
            .field("group_digest", &self.inner.group.digest())
            .field("decoded_retained_bytes", &self.inner.decoded_retained_bytes)
            .finish_non_exhaustive()
    }
}

impl<P> BuiltSourceAssetInner<P> {
    fn value(&self) -> &P {
        self.value
            .as_ref()
            .expect("a published source-build owner contains its decoded value")
    }

    fn drop_value(&mut self) -> Result<(), ArtifactBuildError> {
        catch_unwind(AssertUnwindSafe(|| {
            drop(self.value.take());
            Ok(())
        }))
        .unwrap_or(Err(ArtifactBuildError::Panicked))
    }
}

impl<P> Drop for BuiltSourceAssetInner<P> {
    fn drop(&mut self) {
        let _ = self.drop_value();
    }
}

/// Decodes one explicitly selected source-build codec without publishing live state.
///
/// The caller validates the group's source against the original request before invoking this
/// operation. Its closed artifact recipe remains distinct from a source import recipe.
pub(super) fn decode_source_build<D>(
    store: &ArtifactBuildStore,
    group: VerifiedArtifactGroup,
) -> Result<BuiltSourceAsset<D::Product>, ArtifactBuildError>
where
    D: SourceAssetBuild,
    D::Settings: Serialize,
{
    catch_unwind(AssertUnwindSafe(|| decode_inner::<D>(store, group)))
        .unwrap_or(Err(ArtifactBuildError::Panicked))
}

fn decode_inner<D>(
    store: &ArtifactBuildStore,
    group: VerifiedArtifactGroup,
) -> Result<BuiltSourceAsset<D::Product>, ArtifactBuildError>
where
    D: SourceAssetBuild,
    D::Settings: Serialize,
{
    let [member] = group.members() else {
        return Err(ArtifactBuildError::InvalidCandidate(
            "a typed source build requires exactly one Runtime member",
        ));
    };
    if member.name != "runtime"
        || member.format.as_str() != D::Codec::RUNTIME_KIND
        || member.format_version.get() != D::Codec::RUNTIME_VERSION
        || group.seed().format != member.format
        || group.seed().format_version != member.format_version
    {
        return Err(ArtifactBuildError::InvalidCandidate(
            "source-build Runtime member format mismatch",
        ));
    }
    let bytes = group
        .member_bytes("runtime")
        .ok_or(ArtifactBuildError::InvalidCandidate(
            "source-build Runtime bytes are absent",
        ))?;
    let limits = D::Codec::runtime_limits();
    check(
        "decoded-member-bytes",
        bytes.len(),
        limits.encoded_bytes().get(),
    )?;
    check(
        "decoded-product-promise",
        size_of::<D::Product>(),
        limits.retained_bytes().get(),
    )?;
    let store_limits = store.limits();
    check(
        "decoded-products",
        member.products.len(),
        store_limits.members.get(),
    )?;
    check(
        "decoded-dependencies",
        member.dependencies.len(),
        store_limits.observations.get(),
    )?;
    let scratch = add(
        mul(member.products.len(), size_of::<bool>())?,
        mul(
            member.dependencies.len(),
            size_of::<Option<AssetReferenceDemand>>(),
        )?,
    )?;
    let _work = store.reserve_work(add(limits.work_bytes().get(), scratch)?)?;
    let overhead = owner_overhead::<D::Product>()?;
    let lease = store.reserve_retained(add(limits.retained_bytes().get(), overhead)?)?;
    let mut products = vec![false; member.products.len()];
    let mut dependencies = vec![None; member.dependencies.len()];
    let mut owner = BuiltSourceAssetInner {
        value: None,
        group,
        decoded_retained_bytes: 0,
        _lease: lease,
    };
    let validated = catch_unwind(AssertUnwindSafe(|| {
        let bytes =
            owner
                .group
                .member_bytes("runtime")
                .ok_or(ArtifactBuildError::InvalidCandidate(
                    "source-build Runtime bytes are absent",
                ))?;
        owner.value = Some(D::Codec::decode_runtime(bytes).map_err(ArtifactBuildError::Codec)?);
        validate_products::<D>(
            owner.value(),
            &owner.group,
            &mut products,
            store_limits.members.get(),
        )?;
        validate_dependencies::<D>(
            owner.value(),
            &owner.group,
            &mut dependencies,
            store_limits.observations.get(),
        )?;
        let retained =
            D::Codec::runtime_retained_bytes(owner.value()).map_err(ArtifactBuildError::Codec)?;
        check(
            "decoded-product-retained",
            size_of::<D::Product>(),
            retained,
        )?;
        check(
            "decoded-product-retained",
            retained,
            limits.retained_bytes().get(),
        )?;
        owner.decoded_retained_bytes = add(retained, overhead)?;
        owner._lease.shrink_to(owner.decoded_retained_bytes)?;
        Ok(())
    }))
    .unwrap_or(Err(ArtifactBuildError::Panicked));
    if let Err(error) = validated {
        owner.drop_value()?;
        return Err(error);
    }
    Ok(BuiltSourceAsset {
        inner: Arc::new(owner),
    })
}

fn owner_overhead<P>() -> Result<usize, ArtifactBuildError> {
    let (allocation, _) = Layout::new::<[AtomicUsize; 2]>()
        .extend(Layout::new::<BuiltSourceAssetInner<P>>())
        .map_err(|_| ArtifactBuildError::Overflow)?;
    allocation
        .pad_to_align()
        .size()
        .checked_sub(size_of::<P>())
        .ok_or(ArtifactBuildError::Overflow)
}

fn validate_products<D>(
    value: &D::Product,
    group: &VerifiedArtifactGroup,
    seen: &mut [bool],
    limit: usize,
) -> Result<(), ArtifactBuildError>
where
    D: SourceAssetBuild,
    D::Settings: Serialize,
{
    let member = &group.members()[0];
    let mut occurrences = 0;
    let mut failure = None;
    let visited = D::visit_products(value, |reference| {
        if failure.is_some() {
            return Err(());
        }
        let result: Result<(), ArtifactBuildError> = (|| {
            occurrences = add(occurrences, 1)?;
            check("decoded-product-occurrences", occurrences, limit)?;
            let index = member.products.binary_search(&reference).map_err(|_| {
                ArtifactBuildError::InvalidCandidate("decoded Product is not declared")
            })?;
            if seen[index]
                || reference.source() != group.seed().source.source()
                || !D::contains_product(value, reference.product())
                || !D::Codec::contains_runtime_product(value, reference)
            {
                return Err(ArtifactBuildError::InvalidCandidate(
                    "decoded source-build Product identity mismatch",
                ));
            }
            seen[index] = true;
            Ok(())
        })();
        result.map_err(|error| {
            failure = Some(error);
        })
    });
    if let Some(error) = failure {
        return Err(error);
    }
    visited.map_err(|()| {
        ArtifactBuildError::InvalidCandidate("decoded Product inventory was rejected")
    })?;
    if seen.iter().any(|seen| !seen) {
        return Err(ArtifactBuildError::InvalidCandidate(
            "declared Product is absent from the decoded source build",
        ));
    }
    Ok(())
}

fn validate_dependencies<D>(
    value: &D::Product,
    group: &VerifiedArtifactGroup,
    actual: &mut [Option<AssetReferenceDemand>],
    limit: usize,
) -> Result<(), ArtifactBuildError>
where
    D: SourceAssetBuild,
    D::Settings: Serialize,
{
    let member = &group.members()[0];
    let mut occurrences = 0;
    let mut failure = None;
    let visited = D::Codec::visit_runtime_dependencies(value, |reference, demand| {
        if failure.is_some() {
            return Err(());
        }
        let result: Result<(), ArtifactBuildError> = (|| {
            occurrences = add(occurrences, 1)?;
            check("decoded-dependency-occurrences", occurrences, limit)?;
            let index = member
                .dependencies
                .binary_search_by_key(&reference, |(product, _)| *product)
                .map_err(|_| {
                    ArtifactBuildError::InvalidCandidate(
                        "decoded Runtime dependency is not declared",
                    )
                })?;
            let slot = &mut actual[index];
            if slot.is_none() || demand == AssetReferenceDemand::Immediate {
                *slot = Some(demand);
            }
            Ok(())
        })();
        result.map_err(|error| {
            failure = Some(error);
        })
    });
    if let Some(error) = failure {
        return Err(error);
    }
    visited.map_err(|()| {
        ArtifactBuildError::InvalidCandidate("decoded Runtime dependency inventory was rejected")
    })?;
    if member
        .dependencies
        .iter()
        .zip(actual)
        .any(|((_, expected), actual)| *actual != Some(*expected))
    {
        return Err(ArtifactBuildError::InvalidCandidate(
            "decoded Runtime dependency demands differ from the declared inventory",
        ));
    }
    Ok(())
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
