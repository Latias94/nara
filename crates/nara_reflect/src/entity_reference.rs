//! Schema-governed traversal and rewriting for durable entity references.

use std::collections::BTreeSet;

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_identity::{EntityReference, EntityReferenceRemap, IdentityRemapError};

use crate::{
    ComponentCapability, ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment,
    ComponentSchema, ComponentValue, ComponentValueKind,
};

const DEFAULT_ENTITY_REFERENCE_TRAVERSAL_NODES: usize = 16_384;
const DEFAULT_ENTITY_REFERENCE_TRAVERSAL_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_ENTITY_REFERENCE_TRAVERSAL_DEPTH: usize = 64;
const PERSISTENT_RUNTIME_UUID_TEXT_BYTES: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityReferenceTraversalLimits {
    nodes: ItemLimit,
    bytes: ByteLimit,
    depth: DepthLimit,
}

impl EntityReferenceTraversalLimits {
    #[must_use]
    pub const fn new(nodes: ItemLimit, bytes: ByteLimit, depth: DepthLimit) -> Self {
        Self {
            nodes,
            bytes,
            depth,
        }
    }

    #[must_use]
    pub const fn nodes(self) -> ItemLimit {
        self.nodes
    }

    #[must_use]
    pub const fn bytes(self) -> ByteLimit {
        self.bytes
    }

    #[must_use]
    pub const fn depth(self) -> DepthLimit {
        self.depth
    }
}

impl Default for EntityReferenceTraversalLimits {
    fn default() -> Self {
        Self {
            nodes: ItemLimit::new(DEFAULT_ENTITY_REFERENCE_TRAVERSAL_NODES)
                .expect("default entity reference traversal node limit is non-zero"),
            bytes: ByteLimit::new(DEFAULT_ENTITY_REFERENCE_TRAVERSAL_BYTES)
                .expect("default entity reference traversal byte limit is non-zero"),
            depth: DepthLimit::new(DEFAULT_ENTITY_REFERENCE_TRAVERSAL_DEPTH)
                .expect("default entity reference traversal depth limit is non-zero"),
        }
    }
}

#[derive(Debug)]
pub enum ComponentEntityReferenceRewriteError<E> {
    NodeLimit {
        maximum: usize,
    },
    ByteLimit {
        maximum: usize,
    },
    DepthLimit {
        maximum: usize,
    },
    PathIndexOverflow,
    DuplicateDeclaredPath {
        path: ComponentFieldPath,
    },
    UndeclaredReference {
        path: ComponentFieldPath,
    },
    MissingEntityRefCapability {
        path: ComponentFieldPath,
    },
    RequiredReferenceMissing {
        path: ComponentFieldPath,
    },
    InvalidReferenceValue {
        path: ComponentFieldPath,
        actual: ComponentValueKind,
    },
    InvalidPath {
        path: ComponentFieldPath,
        error: ComponentFieldPathError,
    },
    Rewrite {
        path: ComponentFieldPath,
        error: E,
    },
}

/// Rewrites structurally declared entity-reference leaves without changing the input value.
///
/// This validates the `EntityRef` structural marker. Callers remain responsible for checking the
/// operation-specific field capability, such as `Scene`, `Save`, or `Edit`, before invoking it.
/// The rewrite callback must compute candidates only and must not publish external side effects.
pub fn rewrite_declared_entity_references<E>(
    schema: &ComponentSchema,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    mut rewrite: impl FnMut(&ComponentFieldPath, &EntityReference) -> Result<EntityReference, E>,
) -> Result<ComponentValue, ComponentEntityReferenceRewriteError<E>> {
    let reference_paths = collect_reference_paths(value, limits)?;

    let mut fields = schema.fields.iter().collect::<Vec<_>>();
    fields.sort_by(|left, right| left.path.cmp(&right.path));
    if let Some(duplicate) = fields.windows(2).find(|pair| pair[0].path == pair[1].path) {
        return Err(
            ComponentEntityReferenceRewriteError::DuplicateDeclaredPath {
                path: duplicate[0].path.clone(),
            },
        );
    }

    let declared_paths = fields
        .iter()
        .copied()
        .filter(|field| matches!(field.value_kind, ComponentValueKind::EntityRef))
        .map(|field| field.path.clone())
        .collect::<BTreeSet<_>>();
    if let Some(path) = reference_paths
        .iter()
        .find(|path| !declared_paths.contains(*path))
    {
        return Err(ComponentEntityReferenceRewriteError::UndeclaredReference {
            path: path.clone(),
        });
    }

    let mut validated = Vec::new();
    for field in fields
        .into_iter()
        .filter(|field| matches!(field.value_kind, ComponentValueKind::EntityRef))
    {
        if !field.has_capability(ComponentCapability::EntityRef) {
            return Err(
                ComponentEntityReferenceRewriteError::MissingEntityRefCapability {
                    path: field.path.clone(),
                },
            );
        }

        let field_value = match value.get_path(&field.path) {
            Ok(value) => value,
            Err(error)
                if !field.required
                    && matches!(
                        error,
                        ComponentFieldPathError::MissingField { .. }
                            | ComponentFieldPathError::IndexOutOfBounds { .. }
                    ) =>
            {
                continue;
            }
            Err(error) => {
                return Err(ComponentEntityReferenceRewriteError::InvalidPath {
                    path: field.path.clone(),
                    error,
                });
            }
        };
        if !field.required && matches!(field_value, ComponentValue::Null) {
            continue;
        }
        let ComponentValue::EntityReference(reference) = field_value else {
            return Err(
                if field.required && matches!(field_value, ComponentValue::Null) {
                    ComponentEntityReferenceRewriteError::RequiredReferenceMissing {
                        path: field.path.clone(),
                    }
                } else {
                    ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                        path: field.path.clone(),
                        actual: field_value.kind(),
                    }
                },
            );
        };
        validated.push((&field.path, reference));
    }

    let mut replacements = Vec::with_capacity(validated.len());
    for (path, reference) in validated {
        let replacement = rewrite(path, reference).map_err(|error| {
            ComponentEntityReferenceRewriteError::Rewrite {
                path: path.clone(),
                error,
            }
        })?;
        replacements.push((path.clone(), replacement));
    }

    let mut rewritten = value.clone();
    for (path, replacement) in replacements {
        let replacement = ComponentValue::EntityReference(replacement);
        if path.is_empty() {
            rewritten = replacement;
        } else {
            rewritten
                .replace_path(&path, replacement)
                .map_err(|error| ComponentEntityReferenceRewriteError::InvalidPath {
                    path,
                    error,
                })?;
        }
    }
    Ok(rewritten)
}

pub fn remap_declared_entity_references(
    schema: &ComponentSchema,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    remap: &EntityReferenceRemap,
) -> Result<ComponentValue, ComponentEntityReferenceRewriteError<IdentityRemapError>> {
    rewrite_declared_entity_references(schema, value, limits, |_path, reference| {
        remap.rewrite_entity_reference(reference)
    })
}

enum TraversalFrame<'a> {
    Visit {
        value: &'a ComponentValue,
        depth: usize,
        segment: Option<ComponentFieldPathSegment>,
    },
    Exit,
}

fn collect_reference_paths<E>(
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
) -> Result<Vec<ComponentFieldPath>, ComponentEntityReferenceRewriteError<E>> {
    let mut observed_nodes = 0_usize;
    let mut observed_bytes = 0_usize;
    let mut reference_paths = Vec::new();
    let mut path = Vec::new();
    let mut stack = vec![TraversalFrame::Visit {
        value,
        depth: 1,
        segment: None,
    }];

    while let Some(frame) = stack.pop() {
        let TraversalFrame::Visit {
            value,
            depth,
            segment,
        } = frame
        else {
            path.pop();
            continue;
        };

        if let Some(segment) = segment {
            path.push(segment);
            stack.push(TraversalFrame::Exit);
        }
        if depth > limits.depth().get() {
            return Err(ComponentEntityReferenceRewriteError::DepthLimit {
                maximum: limits.depth().get(),
            });
        }
        observed_nodes = observed_nodes.checked_add(1).ok_or(
            ComponentEntityReferenceRewriteError::NodeLimit {
                maximum: limits.nodes().get(),
            },
        )?;
        if observed_nodes > limits.nodes().get() {
            return Err(ComponentEntityReferenceRewriteError::NodeLimit {
                maximum: limits.nodes().get(),
            });
        }
        observed_bytes = observed_bytes
            .checked_add(dynamic_value_bytes(value).ok_or(
                ComponentEntityReferenceRewriteError::ByteLimit {
                    maximum: limits.bytes().get(),
                },
            )?)
            .ok_or(ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: limits.bytes().get(),
            })?;
        if observed_bytes > limits.bytes().get() {
            return Err(ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: limits.bytes().get(),
            });
        }

        match value {
            ComponentValue::EntityReference(_) => {
                reference_paths.push(ComponentFieldPath::new(path.iter().cloned()));
            }
            ComponentValue::List(items) => {
                for (index, item) in items.iter().enumerate().rev() {
                    let index = u32::try_from(index)
                        .map_err(|_| ComponentEntityReferenceRewriteError::PathIndexOverflow)?;
                    stack.push(TraversalFrame::Visit {
                        value: item,
                        depth: depth.saturating_add(1),
                        segment: Some(ComponentFieldPathSegment::index(index)),
                    });
                }
            }
            ComponentValue::Map(fields) => {
                for (field, value) in fields.iter().rev() {
                    stack.push(TraversalFrame::Visit {
                        value,
                        depth: depth.saturating_add(1),
                        segment: Some(ComponentFieldPathSegment::field(field.clone())),
                    });
                }
            }
            ComponentValue::Null
            | ComponentValue::Bool(_)
            | ComponentValue::I64(_)
            | ComponentValue::U64(_)
            | ComponentValue::F64(_)
            | ComponentValue::String(_) => {}
        }
    }
    Ok(reference_paths)
}

fn dynamic_value_bytes(value: &ComponentValue) -> Option<usize> {
    match value {
        ComponentValue::String(value) => Some(value.len()),
        ComponentValue::Map(fields) => fields
            .keys()
            .try_fold(0_usize, |total, field| total.checked_add(field.len())),
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) => {
            Some(entity.as_str().len())
        }
        ComponentValue::EntityReference(EntityReference::Persistent { entity }) => entity
            .namespace
            .as_str()
            .len()
            .checked_add(PERSISTENT_RUNTIME_UUID_TEXT_BYTES),
        ComponentValue::Null
        | ComponentValue::Bool(_)
        | ComponentValue::I64(_)
        | ComponentValue::U64(_)
        | ComponentValue::F64(_)
        | ComponentValue::List(_) => Some(0),
    }
}
