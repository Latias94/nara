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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedEntityReference {
    path: ComponentFieldPath,
    required: bool,
}

/// A value-specific plan for visiting schema-declared entity references.
///
/// Plans are validated against the value used to create them and retain only the declared field
/// paths needed by later visits. They do not retain or clone the complete [`ComponentValue`].
///
/// A plan is intended for the original value or an unpublished, structurally equivalent
/// candidate. Changing a planned path can make the plan stale;
/// [`DeclaredEntityReferencePlan::visit`] and
/// [`DeclaredEntityReferencePlan::rewrite_in_place`] report that through the existing typed
/// path/value errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredEntityReferencePlan {
    references: Vec<PlannedEntityReference>,
}

impl DeclaredEntityReferencePlan {
    /// Visits every reference captured by this plan without rescanning the complete value tree.
    ///
    /// All planned paths are validated before the first callback is invoked.
    pub fn visit<E>(
        &self,
        value: &ComponentValue,
        mut visit: impl FnMut(&ComponentFieldPath, &EntityReference) -> Result<(), E>,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        self.validate_value(value)?;
        for planned in &self.references {
            let reference = planned_reference(value, planned)?;
            visit(&planned.path, reference).map_err(|error| {
                ComponentEntityReferenceRewriteError::Rewrite {
                    path: planned.path.clone(),
                    error,
                }
            })?;
        }
        Ok(())
    }

    /// Rewrites every planned reference directly in an unpublished candidate.
    ///
    /// This validates every planned path before invoking the first callback, then touches only
    /// those paths. A callback failure can leave references rewritten earlier in the plan changed;
    /// callers must therefore use this only with a candidate that has not been published.
    pub fn rewrite_in_place<E>(
        &self,
        value: &mut ComponentValue,
        mut rewrite: impl FnMut(&ComponentFieldPath, &EntityReference) -> Result<EntityReference, E>,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        self.validate_value(value)?;
        for planned in &self.references {
            let reference = planned_reference_mut(value, planned)?;
            let replacement = rewrite(&planned.path, reference).map_err(|error| {
                ComponentEntityReferenceRewriteError::Rewrite {
                    path: planned.path.clone(),
                    error,
                }
            })?;
            *reference = replacement;
        }
        Ok(())
    }

    fn validate_value<E>(
        &self,
        value: &ComponentValue,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        for planned in &self.references {
            planned_reference(value, planned)?;
        }
        Ok(())
    }
}

/// Validates a value's complete entity-reference structure and prepares path-only access.
///
/// This performs the bounded complete-tree validation once. Later plan visits traverse only the
/// validated entity-reference paths.
pub fn plan_declared_entity_references<E>(
    schema: &ComponentSchema,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
) -> Result<DeclaredEntityReferencePlan, ComponentEntityReferenceRewriteError<E>> {
    let mut fields = schema.fields.iter().collect::<Vec<_>>();
    fields.sort_by(|left, right| left.path.cmp(&right.path));
    let declared_paths = fields
        .iter()
        .copied()
        .filter(|field| matches!(field.value_kind, ComponentValueKind::EntityRef))
        .map(|field| field.path.clone())
        .collect::<BTreeSet<_>>();
    let undeclared_path = find_undeclared_reference_path(value, limits, &declared_paths)?;

    if let Some(duplicate) = fields.windows(2).find(|pair| pair[0].path == pair[1].path) {
        return Err(
            ComponentEntityReferenceRewriteError::DuplicateDeclaredPath {
                path: duplicate[0].path.clone(),
            },
        );
    }
    if let Some(path) = undeclared_path {
        return Err(ComponentEntityReferenceRewriteError::UndeclaredReference { path });
    }

    let mut references = Vec::new();
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
        if !matches!(field_value, ComponentValue::EntityReference(_)) {
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
        }
        references.push(PlannedEntityReference {
            path: field.path.clone(),
            required: field.required,
        });
    }

    Ok(DeclaredEntityReferencePlan { references })
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
    rewrite: impl FnMut(&ComponentFieldPath, &EntityReference) -> Result<EntityReference, E>,
) -> Result<ComponentValue, ComponentEntityReferenceRewriteError<E>> {
    let plan = plan_declared_entity_references::<E>(schema, value, limits)?;
    let mut rewritten = value.clone();
    plan.rewrite_in_place(&mut rewritten, rewrite)?;
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

fn planned_reference<'a, E>(
    value: &'a ComponentValue,
    planned: &PlannedEntityReference,
) -> Result<&'a EntityReference, ComponentEntityReferenceRewriteError<E>> {
    let field_value = value.get_path(&planned.path).map_err(|error| {
        ComponentEntityReferenceRewriteError::InvalidPath {
            path: planned.path.clone(),
            error,
        }
    })?;
    let ComponentValue::EntityReference(reference) = field_value else {
        return Err(invalid_planned_reference(planned, field_value));
    };
    Ok(reference)
}

fn planned_reference_mut<'a, E>(
    value: &'a mut ComponentValue,
    planned: &PlannedEntityReference,
) -> Result<&'a mut EntityReference, ComponentEntityReferenceRewriteError<E>> {
    let field_value = value.get_path_mut(&planned.path).map_err(|error| {
        ComponentEntityReferenceRewriteError::InvalidPath {
            path: planned.path.clone(),
            error,
        }
    })?;
    match field_value {
        ComponentValue::EntityReference(reference) => Ok(reference),
        other => Err(invalid_planned_reference(planned, other)),
    }
}

fn invalid_planned_reference<E>(
    planned: &PlannedEntityReference,
    value: &ComponentValue,
) -> ComponentEntityReferenceRewriteError<E> {
    if planned.required && matches!(value, ComponentValue::Null) {
        ComponentEntityReferenceRewriteError::RequiredReferenceMissing {
            path: planned.path.clone(),
        }
    } else {
        ComponentEntityReferenceRewriteError::InvalidReferenceValue {
            path: planned.path.clone(),
            actual: value.kind(),
        }
    }
}

fn find_undeclared_reference_path<E>(
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    declared_paths: &BTreeSet<ComponentFieldPath>,
) -> Result<Option<ComponentFieldPath>, ComponentEntityReferenceRewriteError<E>> {
    let mut traversal = ReferencePathTraversal {
        limits,
        declared_paths,
        path: Vec::new(),
        observed_nodes: 0,
        observed_bytes: 0,
        first_undeclared_path: None,
    };
    traversal.visit(value)?;
    Ok(traversal.first_undeclared_path)
}

enum TraversalPathSegment<'a> {
    Field(&'a str),
    Index(u32),
}

impl TraversalPathSegment<'_> {
    fn to_owned(&self) -> ComponentFieldPathSegment {
        match self {
            Self::Field(field) => ComponentFieldPathSegment::field((*field).to_owned()),
            Self::Index(index) => ComponentFieldPathSegment::index(*index),
        }
    }
}

struct ReferencePathTraversal<'value, 'declared> {
    limits: EntityReferenceTraversalLimits,
    declared_paths: &'declared BTreeSet<ComponentFieldPath>,
    path: Vec<TraversalPathSegment<'value>>,
    observed_nodes: usize,
    observed_bytes: usize,
    first_undeclared_path: Option<ComponentFieldPath>,
}

enum ReferenceTraversalFrame<'a> {
    Visit {
        value: &'a ComponentValue,
        depth: usize,
        segment: Option<TraversalPathSegment<'a>>,
    },
    ContinueList {
        items: std::iter::Enumerate<std::slice::Iter<'a, ComponentValue>>,
        depth: usize,
    },
    ContinueMap {
        fields: std::collections::btree_map::Iter<'a, String, ComponentValue>,
        depth: usize,
    },
    Exit,
}

impl<'value, 'declared> ReferencePathTraversal<'value, 'declared> {
    fn visit<E>(
        &mut self,
        root: &'value ComponentValue,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        let mut stack = vec![ReferenceTraversalFrame::Visit {
            value: root,
            depth: 1,
            segment: None,
        }];
        while let Some(frame) = stack.pop() {
            match frame {
                ReferenceTraversalFrame::Visit {
                    value,
                    depth,
                    segment,
                } => self.visit_value(value, depth, segment, &mut stack)?,
                ReferenceTraversalFrame::ContinueList { mut items, depth } => {
                    if let Some((index, item)) = items.next() {
                        let index = u32::try_from(index)
                            .map_err(|_| ComponentEntityReferenceRewriteError::PathIndexOverflow)?;
                        stack.push(ReferenceTraversalFrame::ContinueList { items, depth });
                        stack.push(ReferenceTraversalFrame::Visit {
                            value: item,
                            depth,
                            segment: Some(TraversalPathSegment::Index(index)),
                        });
                    }
                }
                ReferenceTraversalFrame::ContinueMap { mut fields, depth } => {
                    if let Some((field, value)) = fields.next() {
                        stack.push(ReferenceTraversalFrame::ContinueMap { fields, depth });
                        stack.push(ReferenceTraversalFrame::Visit {
                            value,
                            depth,
                            segment: Some(TraversalPathSegment::Field(field)),
                        });
                    }
                }
                ReferenceTraversalFrame::Exit => {
                    self.path.pop();
                }
            }
        }
        Ok(())
    }

    fn visit_value<E>(
        &mut self,
        value: &'value ComponentValue,
        depth: usize,
        segment: Option<TraversalPathSegment<'value>>,
        stack: &mut Vec<ReferenceTraversalFrame<'value>>,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        if let Some(segment) = segment {
            self.path.push(segment);
            stack.push(ReferenceTraversalFrame::Exit);
        }
        if depth > self.limits.depth().get() {
            return Err(ComponentEntityReferenceRewriteError::DepthLimit {
                maximum: self.limits.depth().get(),
            });
        }
        self.observed_nodes = self.observed_nodes.checked_add(1).ok_or(
            ComponentEntityReferenceRewriteError::NodeLimit {
                maximum: self.limits.nodes().get(),
            },
        )?;
        if self.observed_nodes > self.limits.nodes().get() {
            return Err(ComponentEntityReferenceRewriteError::NodeLimit {
                maximum: self.limits.nodes().get(),
            });
        }
        self.observed_bytes = self
            .observed_bytes
            .checked_add(dynamic_value_bytes(value).ok_or(
                ComponentEntityReferenceRewriteError::ByteLimit {
                    maximum: self.limits.bytes().get(),
                },
            )?)
            .ok_or(ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: self.limits.bytes().get(),
            })?;
        if self.observed_bytes > self.limits.bytes().get() {
            return Err(ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: self.limits.bytes().get(),
            });
        }

        match value {
            ComponentValue::EntityReference(_) => {
                if self.first_undeclared_path.is_none() {
                    let candidate = ComponentFieldPath::new(
                        self.path.iter().map(TraversalPathSegment::to_owned),
                    );
                    if !self.declared_paths.contains(&candidate) {
                        self.first_undeclared_path = Some(candidate);
                    }
                }
            }
            ComponentValue::List(items) => {
                stack.push(ReferenceTraversalFrame::ContinueList {
                    items: items.iter().enumerate(),
                    depth: depth.saturating_add(1),
                });
            }
            ComponentValue::Map(fields) => {
                stack.push(ReferenceTraversalFrame::ContinueMap {
                    fields: fields.iter(),
                    depth: depth.saturating_add(1),
                });
            }
            ComponentValue::Null
            | ComponentValue::Bool(_)
            | ComponentValue::I64(_)
            | ComponentValue::U64(_)
            | ComponentValue::F64(_)
            | ComponentValue::String(_) => {}
        }
        Ok(())
    }
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
