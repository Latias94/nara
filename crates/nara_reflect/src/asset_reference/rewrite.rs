//! Bounded rewriting of previously collected asset leaves in an unpublished value.

use std::{
    convert::Infallible,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_asset::AssetRef;

use super::{
    DeclaredAssetReference, asset_reference_encoding_work, asset_reference_value_work,
    decode_asset_reference, encode_asset_reference_value,
};
use crate::{
    ComponentFieldPath, ComponentFieldPathSegment, ComponentValue, EntityReferenceTraversalLimits,
};

const MODELED_VALUE_NODE_BYTES: usize = 96;

/// Separates stale or invalid recorded leaves, rewrite failures and caller work refusals.
///
/// Error paths are copied only after work admission. The admission error itself carries no
/// copied path and requires no bounds on the caller's error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetReferenceRewriteError<R, A = Infallible> {
    NodeLimit {
        maximum: usize,
    },
    ByteLimit {
        maximum: usize,
    },
    DepthLimit {
        maximum: usize,
    },
    ConflictingPaths {
        path: ComponentFieldPath,
        other: ComponentFieldPath,
    },
    InvalidPath {
        path: ComponentFieldPath,
    },
    InvalidReferenceValue {
        path: ComponentFieldPath,
    },
    ChangedReference {
        path: ComponentFieldPath,
    },
    Rewrite {
        path: ComponentFieldPath,
        error: R,
    },
    Admission(A),
}

impl<R, A> Display for AssetReferenceRewriteError<R, A> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeLimit { maximum } => {
                write!(
                    formatter,
                    "asset reference rewrite exceeded {maximum} work nodes"
                )
            }
            Self::ByteLimit { maximum } => {
                write!(
                    formatter,
                    "asset reference rewrite exceeded {maximum} logical bytes"
                )
            }
            Self::DepthLimit { maximum } => {
                write!(
                    formatter,
                    "asset reference rewrite exceeded value depth {maximum}"
                )
            }
            Self::ConflictingPaths { path, other } => write!(
                formatter,
                "asset reference paths '{path}' and '{other}' duplicate or overlap"
            ),
            Self::InvalidPath { path } => {
                write!(
                    formatter,
                    "recorded asset reference path '{path}' no longer exists"
                )
            }
            Self::InvalidReferenceValue { path } => write!(
                formatter,
                "recorded asset reference at '{path}' is no longer a canonical reference"
            ),
            Self::ChangedReference { path } => write!(
                formatter,
                "asset reference at '{path}' differs from the originally collected reference"
            ),
            Self::Rewrite { path, .. } => {
                write!(
                    formatter,
                    "asset reference rewrite callback failed at '{path}'"
                )
            }
            Self::Admission(_) => formatter.write_str("asset reference rewrite work was refused"),
        }
    }
}

impl<R: Error + 'static, A: Error + 'static> Error for AssetReferenceRewriteError<R, A> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Rewrite { error, .. } => Some(error),
            Self::Admission(error) => Some(error),
            _ => None,
        }
    }
}

/// Rewrites only previously collected asset leaves in an unpublished candidate.
///
/// Every recorded path and its decoded original reference is validated before the first
/// rewrite callback. Duplicate or ancestor/descendant paths refuse. Paths must be relative to
/// `value`: component-relative field records require the complete component, while selected
/// payload records use the payload, including its empty root path.
///
/// This uses the default reference-work limits and the canonical asset decoder/field encoder.
/// It does not scan for other references, match ordinary strings, fill defaults or admit Schema.
/// Callbacks run in the supplied record order. A callback or later work failure can leave
/// earlier replacements changed; the caller must discard or otherwise own the unpublished
/// candidate on failure. Work allocated by the rewrite callback remains the caller's concern.
pub fn rewrite_declared_asset_references<E>(
    value: &mut ComponentValue,
    references: &[DeclaredAssetReference],
    rewrite: impl FnMut(&DeclaredAssetReference) -> Result<AssetRef, E>,
) -> Result<(), AssetReferenceRewriteError<E>> {
    rewrite_declared_asset_references_with_admission(
        value,
        references,
        EntityReferenceTraversalLimits::default(),
        rewrite,
        |_, _, _| Ok::<(), Infallible>(()),
    )
}

/// Rewrites collected leaves with explicit limits and pre-allocation work admission.
///
/// The callback receives cumulative-work deltas: visited nodes, logical bytes and modeled
/// allocation bytes. Limits bound cumulative node/byte work and complete reference value depth. Work
/// includes repeated path lookups, original reference decoding and replacement encoding.
/// The sorted borrowed-path scratch, decoder allocations, complete replacement map and owned
/// error paths are admitted before construction. Allocation uses the existing 96-byte value
/// node model plus owned strings and encoding collection scratch, excluding allocator capacity.
/// Discarded scratch and previous replacements receive no refunds.
///
/// A refusal returns `Admission` with the exact caller error and allocates no error path.
/// Rewrite errors use `Rewrite`; invalid/stale records use the interpretation/path variants.
/// All records validate before any rewrite callback, but subsequent failures can retain prior
/// replacements in the unpublished candidate, as in [`rewrite_declared_asset_references`].
pub fn rewrite_declared_asset_references_with_admission<R, A>(
    value: &mut ComponentValue,
    references: &[DeclaredAssetReference],
    limits: EntityReferenceTraversalLimits,
    mut rewrite: impl FnMut(&DeclaredAssetReference) -> Result<AssetRef, R>,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), A>,
) -> Result<(), AssetReferenceRewriteError<R, A>> {
    if references.is_empty() {
        return Ok(());
    }
    let mut work = RewriteWork {
        limits,
        nodes: 0,
        bytes: 0,
        admit: &mut admit,
    };
    validate_paths(references, &mut work)?;
    for reference in references {
        work.path(reference.path())?;
        let Some(original) = value_at_path(value, reference.path()) else {
            return Err(AssetReferenceRewriteError::InvalidPath {
                path: work.copy_path(reference.path())?,
            });
        };
        if let Some(decoded_work) = asset_reference_value_work(original) {
            work.value_depth(reference.path(), decoded_work.depth)?;
            work.charge(
                decoded_work.nodes,
                decoded_work.logical_bytes,
                decoded_work.decode_allocation_bytes,
            )?;
        } else {
            work.charge(1, 0, 0)?;
        }
        // The shared decoder's owned text and bounded product UUID scratch were admitted
        // above. This helper maintains neither another wire grammar nor a Schema walker.
        let decoded = match decode_asset_reference(original) {
            Ok(decoded) => decoded,
            Err(_) => {
                return Err(AssetReferenceRewriteError::InvalidReferenceValue {
                    path: work.copy_path(reference.path())?,
                });
            }
        };
        if &decoded != reference.asset_ref() {
            return Err(AssetReferenceRewriteError::ChangedReference {
                path: work.copy_path(reference.path())?,
            });
        }
    }
    for reference in references {
        work.path(reference.path())?;
        let Some(target) = value_at_path_mut(value, reference.path()) else {
            return Err(AssetReferenceRewriteError::InvalidPath {
                path: work.copy_path(reference.path())?,
            });
        };
        let replacement = match rewrite(reference) {
            Ok(replacement) => replacement,
            Err(error) => {
                return Err(AssetReferenceRewriteError::Rewrite {
                    path: work.copy_path(reference.path())?,
                    error,
                });
            }
        };
        let encoded_work = asset_reference_encoding_work(&replacement);
        work.value_depth(reference.path(), encoded_work.depth)?;
        let allocated = encoded_work
            .logical_bytes
            .checked_add(encoded_work.nodes * MODELED_VALUE_NODE_BYTES)
            .and_then(|bytes| {
                bytes.checked_add(encoded_work.map_entries * size_of::<(String, ComponentValue)>())
            })
            .ok_or_else(|| work.byte_limit())?;
        work.charge(encoded_work.nodes, encoded_work.logical_bytes, allocated)?;
        *target = encode_asset_reference_value(&replacement);
    }
    Ok(())
}

fn validate_paths<R, A>(
    references: &[DeclaredAssetReference],
    work: &mut RewriteWork<'_, A>,
) -> Result<(), AssetReferenceRewriteError<R, A>> {
    work.charge(references.len(), 0, 0)?;
    for reference in references {
        work.path(reference.path())?;
    }
    let allocated = references
        .len()
        .checked_mul(size_of::<&DeclaredAssetReference>())
        .ok_or_else(|| work.byte_limit())?;
    work.charge(0, 0, allocated)?;
    let mut sorted: Vec<_> = references.iter().collect();
    sorted.sort_unstable_by(|left, right| left.path().cmp(right.path()));
    for pair in sorted.windows(2) {
        let path = pair[0].path();
        let other = pair[1].path();
        if other.segments().starts_with(path.segments()) {
            // Admit both copies together: a refusal cannot allocate even the first path.
            let allocated = path_allocation_bytes(path)
                .and_then(|bytes| bytes.checked_add(path_allocation_bytes(other)?))
                .ok_or_else(|| work.byte_limit())?;
            let logical = path_logical_bytes(path).saturating_add(path_logical_bytes(other));
            work.charge(0, logical, allocated)?;
            return Err(AssetReferenceRewriteError::ConflictingPaths {
                path: path.clone(),
                other: other.clone(),
            });
        }
    }
    Ok(())
}

// ComponentValue::get_path/get_path_mut allocate path scratch even on success. These borrowed
// lookups interpret only the existing Field/Index segments and keep error allocation explicit.
fn value_at_path<'a>(
    mut value: &'a ComponentValue,
    path: &ComponentFieldPath,
) -> Option<&'a ComponentValue> {
    for segment in path.segments() {
        value = match (segment, value) {
            (ComponentFieldPathSegment::Field(name), ComponentValue::Map(fields)) => {
                fields.get(name)?
            }
            (ComponentFieldPathSegment::Index(index), ComponentValue::List(items)) => {
                items.get(usize::try_from(*index).ok()?)?
            }
            _ => return None,
        };
    }
    Some(value)
}

fn value_at_path_mut<'a>(
    mut value: &'a mut ComponentValue,
    path: &ComponentFieldPath,
) -> Option<&'a mut ComponentValue> {
    for segment in path.segments() {
        value = match (segment, value) {
            (ComponentFieldPathSegment::Field(name), ComponentValue::Map(fields)) => {
                fields.get_mut(name)?
            }
            (ComponentFieldPathSegment::Index(index), ComponentValue::List(items)) => {
                items.get_mut(usize::try_from(*index).ok()?)?
            }
            _ => return None,
        };
    }
    Some(value)
}

struct RewriteWork<'a, A> {
    limits: EntityReferenceTraversalLimits,
    nodes: usize,
    bytes: usize,
    admit: &'a mut dyn FnMut(usize, usize, usize) -> Result<(), A>,
}

impl<A> RewriteWork<'_, A> {
    fn byte_limit<R>(&self) -> AssetReferenceRewriteError<R, A> {
        AssetReferenceRewriteError::ByteLimit {
            maximum: self.limits.bytes().get(),
        }
    }

    fn charge<R>(
        &mut self,
        nodes: usize,
        logical: usize,
        allocated: usize,
    ) -> Result<(), AssetReferenceRewriteError<R, A>> {
        self.nodes = self
            .nodes
            .checked_add(nodes)
            .filter(|nodes| *nodes <= self.limits.nodes().get())
            .ok_or(AssetReferenceRewriteError::NodeLimit {
                maximum: self.limits.nodes().get(),
            })?;
        self.bytes = self
            .bytes
            .checked_add(logical)
            .filter(|bytes| *bytes <= self.limits.bytes().get())
            .ok_or_else(|| self.byte_limit())?;
        (self.admit)(nodes, logical, allocated).map_err(AssetReferenceRewriteError::Admission)
    }

    fn path<R>(
        &mut self,
        path: &ComponentFieldPath,
    ) -> Result<(), AssetReferenceRewriteError<R, A>> {
        if path.segments().len() > self.limits.depth().get() {
            return Err(AssetReferenceRewriteError::DepthLimit {
                maximum: self.limits.depth().get(),
            });
        }
        for segment in path.segments() {
            let bytes = match segment {
                ComponentFieldPathSegment::Field(name) => name.len(),
                ComponentFieldPathSegment::Index(_) => size_of::<u32>(),
            };
            self.charge(1, bytes, 0)?;
        }
        Ok(())
    }

    fn value_depth<R>(
        &self,
        path: &ComponentFieldPath,
        depth: usize,
    ) -> Result<(), AssetReferenceRewriteError<R, A>> {
        if path.segments().len().saturating_add(depth) > self.limits.depth().get() {
            return Err(AssetReferenceRewriteError::DepthLimit {
                maximum: self.limits.depth().get(),
            });
        }
        Ok(())
    }

    fn copy_path<R>(
        &mut self,
        path: &ComponentFieldPath,
    ) -> Result<ComponentFieldPath, AssetReferenceRewriteError<R, A>> {
        let allocated = path_allocation_bytes(path).ok_or_else(|| self.byte_limit())?;
        self.charge(0, path_logical_bytes(path), allocated)?;
        Ok(path.clone())
    }
}

fn path_logical_bytes(path: &ComponentFieldPath) -> usize {
    path.segments().iter().fold(0usize, |bytes, segment| {
        bytes.saturating_add(match segment {
            ComponentFieldPathSegment::Field(name) => name.len(),
            ComponentFieldPathSegment::Index(_) => size_of::<u32>(),
        })
    })
}

fn path_allocation_bytes(path: &ComponentFieldPath) -> Option<usize> {
    path.segments().iter().try_fold(
        path.segments()
            .len()
            .checked_mul(size_of::<ComponentFieldPathSegment>())?,
        |bytes, segment| match segment {
            ComponentFieldPathSegment::Field(name) => bytes.checked_add(name.len()),
            ComponentFieldPathSegment::Index(_) => Some(bytes),
        },
    )
}

#[cfg(test)]
mod tests;
