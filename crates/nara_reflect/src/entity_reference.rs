//! Schema-governed traversal and rewriting for durable entity references.

use std::collections::BTreeSet;

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_identity::{EntityReference, EntityReferenceRemap, IdentityRemapError};

use crate::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldPathError,
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentSchema, ComponentShapeMember,
    ComponentValue, ComponentValueKind, ComponentValueShape,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    MissingAssetRefCapability {
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
    field_id: ComponentFieldId,
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
/// path/value errors. Adding references, changing collection membership, or changing an optional
/// leaf from Null to a reference requires a new plan; path-only visits do not rescan for new leaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredEntityReferencePlan {
    references: Vec<PlannedEntityReference>,
}

impl DeclaredEntityReferencePlan {
    #[must_use]
    pub fn root_field_id(&self, path: &ComponentFieldPath) -> Option<&ComponentFieldId> {
        self.references
            .iter()
            .find(|reference| &reference.path == path)
            .map(|reference| &reference.field_id)
    }

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
    let references =
        collect_present_reference_leaves(schema, value, limits, ComponentValueKind::EntityRef)?
            .into_iter()
            .map(|reference| PlannedEntityReference {
                field_id: reference.field.id().clone(),
                path: reference.path,
                required: reference.required,
            })
            .collect();

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

pub(crate) struct PresentReferenceLeaf<'schema> {
    pub(crate) field: &'schema ComponentFieldSchema,
    pub(crate) path: ComponentFieldPath,
    required: bool,
}

pub(crate) fn collect_present_reference_leaves<'schema, E>(
    schema: &'schema ComponentSchema,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
) -> Result<Vec<PresentReferenceLeaf<'schema>>, ComponentEntityReferenceRewriteError<E>> {
    let _ = find_undeclared_reference_path(
        value,
        limits,
        &BTreeSet::new(),
        0,
        0,
        &ComponentFieldPath::empty(),
    )?;
    if schema.fields.len() > limits.nodes().get() {
        return Err(ComponentEntityReferenceRewriteError::NodeLimit {
            maximum: limits.nodes().get(),
        });
    }
    let mut fields = schema.fields.iter().collect::<Vec<_>>();
    fields.sort_by(|left, right| left.path.cmp(&right.path));
    if let Some(duplicate) = fields.windows(2).find(|pair| pair[0].path == pair[1].path) {
        return Err(
            ComponentEntityReferenceRewriteError::DuplicateDeclaredPath {
                path: duplicate[0].path.clone(),
            },
        );
    }

    let mut traversal = PresentReferenceTraversal::new(limits, reference_kind);
    for field in fields {
        if !field.shape.contains_kind(ComponentValueKind::EntityRef)
            && !field.shape.contains_kind(ComponentValueKind::AssetRef)
            && !field.has_capability(ComponentCapability::EntityRef)
            && !field.has_capability(ComponentCapability::AssetRef)
        {
            continue;
        }
        let field_value = match value.get_path(field.path()) {
            Ok(value) => value,
            Err(error)
                if !field.is_required()
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
        traversal.visit_field(field, field_value)?;
    }
    let undeclared = find_undeclared_reference_path(
        value,
        limits,
        &traversal.entity_paths,
        traversal.result_nodes,
        traversal.result_bytes,
        &ComponentFieldPath::empty(),
    )?;
    if let Some(path) = undeclared {
        return Err(ComponentEntityReferenceRewriteError::UndeclaredReference { path });
    }
    Ok(traversal.references)
}

pub(crate) fn collect_present_field_reference_leaves<'schema, E>(
    field: &'schema ComponentFieldSchema,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
) -> Result<Vec<PresentReferenceLeaf<'schema>>, ComponentEntityReferenceRewriteError<E>> {
    let _ = find_undeclared_reference_path(value, limits, &BTreeSet::new(), 0, 0, field.path())?;
    let mut traversal = PresentReferenceTraversal::new(limits, reference_kind);
    traversal.visit_field(field, value)?;
    if let Some(path) = find_undeclared_reference_path(
        value,
        limits,
        &traversal.entity_paths,
        traversal.result_nodes,
        traversal.result_bytes,
        field.path(),
    )? {
        return Err(ComponentEntityReferenceRewriteError::UndeclaredReference { path });
    }
    Ok(traversal.references)
}

fn path_depth(path: &ComponentFieldPath) -> usize {
    path.segments().len().saturating_add(1)
}

struct PresentReferenceTraversal<'schema> {
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
    shape_nodes: usize,
    result_nodes: usize,
    result_bytes: usize,
    entity_paths: BTreeSet<ComponentFieldPath>,
    references: Vec<PresentReferenceLeaf<'schema>>,
}

impl<'schema> PresentReferenceTraversal<'schema> {
    fn new(limits: EntityReferenceTraversalLimits, reference_kind: ComponentValueKind) -> Self {
        Self {
            limits,
            reference_kind,
            shape_nodes: 0,
            result_nodes: 0,
            result_bytes: 0,
            entity_paths: BTreeSet::new(),
            references: Vec::new(),
        }
    }

    fn visit_field<E>(
        &mut self,
        field: &'schema ComponentFieldSchema,
        value: &ComponentValue,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        field.shape.validate().map_err(|_| {
            ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                path: field.path.clone(),
                actual: value.kind(),
            }
        })?;
        for (kind, capability) in [
            (
                ComponentValueKind::EntityRef,
                ComponentCapability::EntityRef,
            ),
            (ComponentValueKind::AssetRef, ComponentCapability::AssetRef),
        ] {
            if field.shape.contains_kind(kind) && !field.has_capability(capability) {
                return Err(match capability {
                    ComponentCapability::EntityRef => {
                        ComponentEntityReferenceRewriteError::MissingEntityRefCapability {
                            path: field.path.clone(),
                        }
                    }
                    _ => ComponentEntityReferenceRewriteError::MissingAssetRefCapability {
                        path: field.path.clone(),
                    },
                });
            }
        }
        let mut path = field.path.segments().to_vec();
        self.visit(
            field,
            &field.shape,
            value,
            &mut path,
            path_depth(field.path()),
        )
    }

    fn visit<E>(
        &mut self,
        field: &'schema ComponentFieldSchema,
        mut shape: &ComponentValueShape,
        value: &ComponentValue,
        path: &mut Vec<ComponentFieldPathSegment>,
        depth: usize,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        if depth > self.limits.depth().get() {
            return Err(ComponentEntityReferenceRewriteError::DepthLimit {
                maximum: self.limits.depth().get(),
            });
        }
        loop {
            self.shape_nodes = self.shape_nodes.saturating_add(1);
            if self.shape_nodes > self.limits.nodes().get() {
                return Err(ComponentEntityReferenceRewriteError::NodeLimit {
                    maximum: self.limits.nodes().get(),
                });
            }
            match shape {
                ComponentValueShape::Optional { .. } if matches!(value, ComponentValue::Null) => {
                    return Ok(());
                }
                ComponentValueShape::Optional { value } => shape = value,
                _ => break,
            }
        }
        match (shape, value) {
            (
                ComponentValueShape::Scalar {
                    kind,
                    numeric_domain,
                    numeric_interval,
                },
                value,
            ) if value.kind() == *kind
                && numeric_domain.is_none_or(|domain| domain.accepts_value(value))
                && numeric_interval
                    .is_none_or(|interval| interval.accepts_value(value, *numeric_domain)) => {}
            (ComponentValueShape::EntityRef, ComponentValue::EntityReference(_)) => {
                self.record(field, path, ComponentValueKind::EntityRef)?;
            }
            (ComponentValueShape::AssetRef, value) => {
                if self.reference_kind != ComponentValueKind::AssetRef
                    && !crate::asset_reference::is_asset_reference_value(value)
                {
                    return Err(
                        ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                            path: ComponentFieldPath::new(path.iter().cloned()),
                            actual: value.kind(),
                        },
                    );
                }
                if let Some(payload) = value.get("value").and_then(ComponentValue::as_str) {
                    self.result_bytes = self.result_bytes.saturating_add(payload.len());
                }
                self.record(field, path, ComponentValueKind::AssetRef)?;
            }
            (ComponentValueShape::Record { members }, ComponentValue::Map(values)) => {
                self.visit_record(field, members, values, path, depth)?;
            }
            (
                ComponentValueShape::List { item } | ComponentValueShape::KeyedList { item, .. },
                ComponentValue::List(values),
            ) => {
                for (index, value) in values.iter().enumerate() {
                    let index = u32::try_from(index)
                        .map_err(|_| ComponentEntityReferenceRewriteError::PathIndexOverflow)?;
                    path.push(ComponentFieldPathSegment::index(index));
                    self.visit(field, item, value, path, depth.saturating_add(1))?;
                    path.pop();
                }
            }
            (ComponentValueShape::Map { value: shape }, ComponentValue::Map(values)) => {
                for (name, value) in values {
                    path.push(ComponentFieldPathSegment::field(name));
                    self.visit(field, shape, value, path, depth.saturating_add(1))?;
                    path.pop();
                }
            }
            (ComponentValueShape::Opaque { kind }, value) if value.kind() == *kind => {}
            (_, value) => {
                let path = ComponentFieldPath::new(path.iter().cloned());
                return Err(
                    if matches!(shape, ComponentValueShape::EntityRef)
                        && matches!(value, ComponentValue::Null)
                    {
                        ComponentEntityReferenceRewriteError::RequiredReferenceMissing { path }
                    } else {
                        ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                            path,
                            actual: value.kind(),
                        }
                    },
                );
            }
        }
        Ok(())
    }

    fn visit_record<E>(
        &mut self,
        field: &'schema ComponentFieldSchema,
        members: &[ComponentShapeMember],
        values: &std::collections::BTreeMap<String, ComponentValue>,
        path: &mut Vec<ComponentFieldPathSegment>,
        depth: usize,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        let names = members
            .iter()
            .map(|member| member.name.as_str())
            .collect::<BTreeSet<_>>();
        for member in members {
            path.push(ComponentFieldPathSegment::field(&member.name));
            let value = values.get(&member.name).ok_or_else(|| {
                let path = ComponentFieldPath::new(path.iter().cloned());
                ComponentEntityReferenceRewriteError::InvalidPath {
                    error: ComponentFieldPathError::MissingField {
                        path: path.clone(),
                        field: member.name.clone(),
                    },
                    path,
                }
            })?;
            self.visit(field, &member.shape, value, path, depth.saturating_add(1))?;
            path.pop();
        }
        if let Some(name) = values.keys().find(|name| !names.contains(name.as_str())) {
            path.push(ComponentFieldPathSegment::field(name));
            return Err(
                ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                    path: ComponentFieldPath::new(path.iter().cloned()),
                    actual: values[name].kind(),
                },
            );
        }
        Ok(())
    }

    fn record<E>(
        &mut self,
        field: &'schema ComponentFieldSchema,
        path: &[ComponentFieldPathSegment],
        kind: ComponentValueKind,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        if kind != ComponentValueKind::EntityRef && kind != self.reference_kind {
            return Ok(());
        }
        self.result_nodes = self.result_nodes.saturating_add(1);
        if self.result_nodes > self.limits.nodes().get() {
            return Err(ComponentEntityReferenceRewriteError::NodeLimit {
                maximum: self.limits.nodes().get(),
            });
        }
        self.result_bytes = self.result_bytes.saturating_add(field.id().as_str().len());
        for segment in path {
            self.result_bytes = self.result_bytes.saturating_add(match segment {
                ComponentFieldPathSegment::Field(name) => name.len(),
                ComponentFieldPathSegment::Index(_) => size_of::<u32>(),
            });
        }
        if self.result_bytes > self.limits.bytes().get() {
            return Err(ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: self.limits.bytes().get(),
            });
        }
        let path = ComponentFieldPath::new(path.iter().cloned());
        if kind == ComponentValueKind::EntityRef {
            self.entity_paths.insert(path.clone());
        }
        if kind == self.reference_kind {
            self.references.push(PresentReferenceLeaf {
                field,
                path,
                required: true,
            });
        }
        Ok(())
    }
}

fn find_undeclared_reference_path<E>(
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    declared_paths: &BTreeSet<ComponentFieldPath>,
    result_nodes: usize,
    result_bytes: usize,
    root_path: &ComponentFieldPath,
) -> Result<Option<ComponentFieldPath>, ComponentEntityReferenceRewriteError<E>> {
    let mut traversal = ReferencePathTraversal {
        limits,
        declared_paths,
        path: root_path
            .segments()
            .iter()
            .map(|segment| match segment {
                ComponentFieldPathSegment::Field(name) => TraversalPathSegment::Field(name),
                ComponentFieldPathSegment::Index(index) => TraversalPathSegment::Index(*index),
            })
            .collect(),
        observed_nodes: result_nodes,
        observed_bytes: result_bytes,
        first_undeclared_path: None,
    };
    traversal.visit(value, path_depth(root_path))?;
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
        depth: usize,
    ) -> Result<(), ComponentEntityReferenceRewriteError<E>> {
        let mut stack = vec![ReferenceTraversalFrame::Visit {
            value: root,
            depth,
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
                        self.observed_bytes = self.observed_bytes.saturating_add(field.len());
                        if self.observed_bytes > self.limits.bytes().get() {
                            return Err(ComponentEntityReferenceRewriteError::ByteLimit {
                                maximum: self.limits.bytes().get(),
                            });
                        }
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
        | ComponentValue::List(_)
        | ComponentValue::Map(_) => Some(0),
    }
}
