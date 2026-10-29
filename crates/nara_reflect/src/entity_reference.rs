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
const MODELED_REFERENCE_BTREE_ENTRY_BYTES: usize = 96;

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

/// Keeps the caller's work refusal separate from reference interpretation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityReferencePlanError<R, E> {
    Reference(ComponentEntityReferenceRewriteError<R>),
    Admission(E),
}

impl<R, E> From<ReferenceTraversalError<R, E>> for EntityReferencePlanError<R, E> {
    fn from(error: ReferenceTraversalError<R, E>) -> Self {
        match error {
            ReferenceTraversalError::Reference(error) => Self::Reference(error),
            ReferenceTraversalError::Admission(error) => Self::Admission(error),
        }
    }
}

/// Plans references in a borrowed normalized value under an already admitted current Schema.
///
/// The caller owns Schema admission and default materialization. This entry reuses the complete
/// reference walker without revalidating Schema declarations or cloning the value tree. The
/// callback receives work deltas: visited nodes, logical bytes and modeled allocation bytes.
/// Scratch growth, paths, declared sets, output Vec capacity and field ID copies are admitted
/// before allocation, with no refunds for discarded scratch. Leaf paths move into the plan.
///
/// A denied callback returns its exact error without allocating an error path. Neither an empty
/// plan nor a value without EntityRef leaves bypasses structural scanning and its work admission.
pub fn plan_declared_entity_references_with_admission<R, E>(
    schema: &ComponentSchema,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<DeclaredEntityReferencePlan, EntityReferencePlanError<R, E>> {
    let leaves = collect_present_reference_leaves_with_admission::<R, E>(
        schema,
        value,
        limits,
        ComponentValueKind::EntityRef,
        true,
        &mut admit,
    )?;
    let mut admission = ReferenceAdmission {
        callback: &mut admit,
        maximum_bytes: limits.bytes().get(),
    };
    let mut references = Vec::new();
    admission.reserve(&mut references, leaves.len())?;
    for leaf in leaves {
        admission.allocation(Some(leaf.field.id().as_str().len()))?;
        references.push(PlannedEntityReference {
            field_id: leaf.field.id().clone(),
            path: leaf.path,
            required: leaf.required,
        });
    }
    Ok(DeclaredEntityReferencePlan { references })
}

/// Plans references relative to one complete replacement payload for a resolved value shape.
///
/// The caller supplies the admitted owning root field and its selected shape (for example from
/// property resolution). Capabilities still come from that field. Paths start at this payload,
/// so a scalar reference occupies the empty path. This neither resolves a property nor admits
/// a source edit, native value or default; those remain the caller's existing owner boundaries.
pub fn plan_declared_value_entity_references<E>(
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
) -> Result<DeclaredEntityReferencePlan, ComponentEntityReferenceRewriteError<E>> {
    let references = collect_present_value_reference_leaves(
        field,
        shape,
        value,
        limits,
        ComponentValueKind::EntityRef,
    )?;
    Ok(DeclaredEntityReferencePlan {
        references: references
            .into_iter()
            .map(|reference| PlannedEntityReference {
                field_id: reference.field.id().clone(),
                path: reference.path,
                required: reference.required,
            })
            .collect(),
    })
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

pub(crate) enum ReferenceTraversalError<R, E> {
    Reference(ComponentEntityReferenceRewriteError<R>),
    Admission(E),
}

impl<R, E> From<ComponentEntityReferenceRewriteError<R>> for ReferenceTraversalError<R, E> {
    fn from(error: ComponentEntityReferenceRewriteError<R>) -> Self {
        Self::Reference(error)
    }
}

fn without_admission<T, R>(
    result: Result<T, ReferenceTraversalError<R, std::convert::Infallible>>,
) -> Result<T, ComponentEntityReferenceRewriteError<R>> {
    result.map_err(|error| match error {
        ReferenceTraversalError::Reference(error) => error,
        ReferenceTraversalError::Admission(never) => match never {},
    })
}

struct ReferenceAdmission<'a, E> {
    callback: &'a mut dyn FnMut(usize, usize, usize) -> Result<(), E>,
    maximum_bytes: usize,
}

impl<E> ReferenceAdmission<'_, E> {
    fn charge<R>(
        &mut self,
        nodes: usize,
        logical_bytes: usize,
        allocation_bytes: usize,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        (self.callback)(nodes, logical_bytes, allocation_bytes)
            .map_err(ReferenceTraversalError::Admission)
    }

    fn allocation<R>(&mut self, bytes: Option<usize>) -> Result<(), ReferenceTraversalError<R, E>> {
        let bytes = bytes.ok_or(ComponentEntityReferenceRewriteError::ByteLimit {
            maximum: self.maximum_bytes,
        })?;
        self.charge(0, 0, bytes)
    }

    fn reserve<T, R>(
        &mut self,
        values: &mut Vec<T>,
        additional: usize,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        if values.capacity().saturating_sub(values.len()) >= additional {
            return Ok(());
        }
        let overflow = || ComponentEntityReferenceRewriteError::ByteLimit {
            maximum: self.maximum_bytes,
        };
        let needed = values.len().checked_add(additional).ok_or_else(overflow)?;
        let capacity = needed.max(values.capacity().saturating_mul(2)).max(4);
        // Charge the entire new request: working allocation is cumulative, without refunds.
        self.allocation(capacity.checked_mul(size_of::<T>()))?;
        values.reserve_exact(capacity - values.len());
        Ok(())
    }

    fn path<R>(
        &mut self,
        segments: &[ComponentFieldPathSegment],
    ) -> Result<ComponentFieldPath, ReferenceTraversalError<R, E>> {
        self.allocation(path_allocation_bytes(segments))?;
        Ok(ComponentFieldPath::new(segments.iter().cloned()))
    }
}

fn path_logical_bytes(segments: &[ComponentFieldPathSegment]) -> usize {
    segments.iter().fold(0usize, |bytes, segment| {
        bytes.saturating_add(match segment {
            ComponentFieldPathSegment::Field(name) => name.len(),
            ComponentFieldPathSegment::Index(_) => size_of::<u32>(),
        })
    })
}

fn path_allocation_bytes(segments: &[ComponentFieldPathSegment]) -> Option<usize> {
    segments.iter().try_fold(
        segments
            .len()
            .checked_mul(size_of::<ComponentFieldPathSegment>())?,
        |bytes, segment| match segment {
            ComponentFieldPathSegment::Field(name) => bytes.checked_add(name.len()),
            ComponentFieldPathSegment::Index(_) => Some(bytes),
        },
    )
}

pub(crate) struct PresentReferenceLeaf<'schema, 'value> {
    pub(crate) field: &'schema ComponentFieldSchema,
    pub(crate) path: ComponentFieldPath,
    pub(crate) value: &'value ComponentValue,
    required: bool,
}

pub(crate) fn collect_present_reference_leaves<'schema, 'value, R>(
    schema: &'schema ComponentSchema,
    value: &'value ComponentValue,
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
) -> Result<Vec<PresentReferenceLeaf<'schema, 'value>>, ComponentEntityReferenceRewriteError<R>> {
    without_admission(collect_present_reference_leaves_with_admission(
        schema,
        value,
        limits,
        reference_kind,
        false,
        &mut |_, _, _| Ok::<(), std::convert::Infallible>(()),
    ))
}

pub(crate) fn collect_present_reference_leaves_with_admission<'schema, 'value, R, E>(
    schema: &'schema ComponentSchema,
    value: &'value ComponentValue,
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
    admitted_schema: bool,
    callback: &mut dyn FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Vec<PresentReferenceLeaf<'schema, 'value>>, ReferenceTraversalError<R, E>> {
    let mut admission = ReferenceAdmission {
        callback,
        maximum_bytes: limits.bytes().get(),
    };
    let _ = find_undeclared_reference_path(
        value,
        limits,
        &BTreeSet::new(),
        0,
        0,
        &ComponentFieldPath::empty(),
        &mut admission,
    )?;
    if schema.fields.len() > limits.nodes().get() {
        return Err(ComponentEntityReferenceRewriteError::NodeLimit {
            maximum: limits.nodes().get(),
        }
        .into());
    }
    let mut fields: Vec<&ComponentFieldSchema> = Vec::new();
    admission.reserve(&mut fields, schema.fields.len())?;
    fields.extend(&schema.fields);
    fields.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    if let Some(duplicate) = fields.windows(2).find(|pair| pair[0].path == pair[1].path) {
        let path = admission.path(duplicate[0].path.segments())?;
        return Err(ComponentEntityReferenceRewriteError::DuplicateDeclaredPath { path }.into());
    }

    let mut traversal =
        PresentReferenceTraversal::new(limits, reference_kind, admitted_schema, admission);
    for field in fields {
        traversal.admission.charge(
            1,
            field
                .id()
                .as_str()
                .len()
                .saturating_add(path_logical_bytes(field.path.segments())),
            0,
        )?;
        if !admitted_schema
            && !field.shape.contains_kind(ComponentValueKind::EntityRef)
            && !field.shape.contains_kind(ComponentValueKind::AssetRef)
            && !field.has_capability(ComponentCapability::EntityRef)
            && !field.has_capability(ComponentCapability::AssetRef)
        {
            continue;
        }
        let Some(field_value) = traversal.field_value(value, field)? else {
            continue;
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
        &mut traversal.admission,
    )?;
    if let Some(path) = undeclared {
        return Err(ComponentEntityReferenceRewriteError::UndeclaredReference { path }.into());
    }
    Ok(traversal.references)
}

pub(crate) fn collect_present_field_reference_leaves<'schema, 'value, R>(
    field: &'schema ComponentFieldSchema,
    value: &'value ComponentValue,
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
) -> Result<Vec<PresentReferenceLeaf<'schema, 'value>>, ComponentEntityReferenceRewriteError<R>> {
    without_admission(collect_present_field_reference_leaves_with_admission(
        field,
        value,
        limits,
        reference_kind,
        false,
        &mut |_, _, _| Ok::<(), std::convert::Infallible>(()),
    ))
}

pub(crate) fn collect_present_field_reference_leaves_with_admission<'schema, 'value, R, E>(
    field: &'schema ComponentFieldSchema,
    value: &'value ComponentValue,
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
    admitted_schema: bool,
    callback: &mut dyn FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Vec<PresentReferenceLeaf<'schema, 'value>>, ReferenceTraversalError<R, E>> {
    let mut admission = ReferenceAdmission {
        callback,
        maximum_bytes: limits.bytes().get(),
    };
    let _ = find_undeclared_reference_path(
        value,
        limits,
        &BTreeSet::new(),
        0,
        0,
        field.path(),
        &mut admission,
    )?;
    let mut traversal =
        PresentReferenceTraversal::new(limits, reference_kind, admitted_schema, admission);
    traversal.visit_field(field, value)?;
    if let Some(path) = find_undeclared_reference_path(
        value,
        limits,
        &traversal.entity_paths,
        traversal.result_nodes,
        traversal.result_bytes,
        field.path(),
        &mut traversal.admission,
    )? {
        return Err(ComponentEntityReferenceRewriteError::UndeclaredReference { path }.into());
    }
    Ok(traversal.references)
}

/// Traverses an already selected payload without rebasing it onto component storage.
pub(crate) fn collect_present_value_reference_leaves<'schema, 'value, R>(
    field: &'schema ComponentFieldSchema,
    shape: &ComponentValueShape,
    value: &'value ComponentValue,
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
) -> Result<Vec<PresentReferenceLeaf<'schema, 'value>>, ComponentEntityReferenceRewriteError<R>> {
    without_admission(collect_present_value_reference_leaves_with_admission(
        field,
        shape,
        value,
        limits,
        reference_kind,
        false,
        &mut |_, _, _| Ok::<(), std::convert::Infallible>(()),
    ))
}

/// Borrows the selected shape and shares the ordinary payload walker's allocation boundaries.
pub(crate) fn collect_present_value_reference_leaves_with_admission<'schema, 'value, R, E>(
    field: &'schema ComponentFieldSchema,
    shape: &ComponentValueShape,
    value: &'value ComponentValue,
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
    admitted_schema: bool,
    callback: &mut dyn FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Vec<PresentReferenceLeaf<'schema, 'value>>, ReferenceTraversalError<R, E>> {
    let mut admission = ReferenceAdmission {
        callback,
        maximum_bytes: limits.bytes().get(),
    };
    let root = ComponentFieldPath::empty();
    let _ = find_undeclared_reference_path(
        value,
        limits,
        &BTreeSet::new(),
        0,
        0,
        &root,
        &mut admission,
    )?;
    let mut traversal =
        PresentReferenceTraversal::new(limits, reference_kind, admitted_schema, admission);
    traversal.visit_field_shape(field, shape, value, &root)?;
    if let Some(path) = find_undeclared_reference_path(
        value,
        limits,
        &traversal.entity_paths,
        traversal.result_nodes,
        traversal.result_bytes,
        &root,
        &mut traversal.admission,
    )? {
        return Err(ComponentEntityReferenceRewriteError::UndeclaredReference { path }.into());
    }
    Ok(traversal.references)
}

fn path_depth(path: &ComponentFieldPath) -> usize {
    path.segments().len().saturating_add(1)
}

struct PresentReferenceTraversal<'schema, 'value, 'admission, E> {
    limits: EntityReferenceTraversalLimits,
    reference_kind: ComponentValueKind,
    shape_nodes: usize,
    result_nodes: usize,
    result_bytes: usize,
    entity_paths: BTreeSet<ComponentFieldPath>,
    references: Vec<PresentReferenceLeaf<'schema, 'value>>,
    admitted_schema: bool,
    admission: ReferenceAdmission<'admission, E>,
}

impl<'schema, 'value, 'admission, E> PresentReferenceTraversal<'schema, 'value, 'admission, E> {
    fn new(
        limits: EntityReferenceTraversalLimits,
        reference_kind: ComponentValueKind,
        admitted_schema: bool,
        admission: ReferenceAdmission<'admission, E>,
    ) -> Self {
        Self {
            limits,
            reference_kind,
            shape_nodes: 0,
            result_nodes: 0,
            result_bytes: 0,
            entity_paths: BTreeSet::new(),
            references: Vec::new(),
            admitted_schema,
            admission,
        }
    }

    fn field_value<R>(
        &mut self,
        mut value: &'value ComponentValue,
        field: &ComponentFieldSchema,
    ) -> Result<Option<&'value ComponentValue>, ReferenceTraversalError<R, E>> {
        for (offset, segment) in field.path.segments().iter().enumerate() {
            self.admission.charge(1, 0, 0)?;
            let child = match (segment, value) {
                (ComponentFieldPathSegment::Field(name), ComponentValue::Map(fields)) => {
                    fields.get(name)
                }
                (ComponentFieldPathSegment::Index(index), ComponentValue::List(items)) => {
                    items.get(usize::try_from(*index).unwrap_or(usize::MAX))
                }
                _ => None,
            };
            if let Some(child) = child {
                value = child;
                continue;
            }
            let missing = matches!(
                (segment, value),
                (ComponentFieldPathSegment::Field(_), ComponentValue::Map(_))
                    | (ComponentFieldPathSegment::Index(_), ComponentValue::List(_))
            );
            if missing && !field.is_required() {
                return Ok(None);
            }
            let path = self.admission.path(field.path.segments())?;
            let error = match (segment, value) {
                (ComponentFieldPathSegment::Field(name), ComponentValue::Map(_)) => {
                    let path = self.admission.path(&field.path.segments()[..=offset])?;
                    self.admission.allocation(Some(name.len()))?;
                    ComponentFieldPathError::MissingField {
                        path,
                        field: name.clone(),
                    }
                }
                (ComponentFieldPathSegment::Index(index), ComponentValue::List(items)) => {
                    ComponentFieldPathError::IndexOutOfBounds {
                        path: self.admission.path(&field.path.segments()[..=offset])?,
                        index: *index,
                        len: items.len(),
                    }
                }
                (ComponentFieldPathSegment::Field(_), _) => ComponentFieldPathError::ExpectedMap {
                    path: self.admission.path(&field.path.segments()[..offset])?,
                },
                (ComponentFieldPathSegment::Index(_), _) => ComponentFieldPathError::ExpectedList {
                    path: self.admission.path(&field.path.segments()[..offset])?,
                },
            };
            return Err(ComponentEntityReferenceRewriteError::InvalidPath { path, error }.into());
        }
        Ok(Some(value))
    }

    fn check_capability<R>(
        &mut self,
        field: &ComponentFieldSchema,
        capability: ComponentCapability,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        if field.has_capability(capability) {
            return Ok(());
        }
        let path = self.admission.path(field.path.segments())?;
        Err(match capability {
            ComponentCapability::EntityRef => {
                ComponentEntityReferenceRewriteError::MissingEntityRefCapability { path }
            }
            _ => ComponentEntityReferenceRewriteError::MissingAssetRefCapability { path },
        }
        .into())
    }

    fn visit_field<R>(
        &mut self,
        field: &'schema ComponentFieldSchema,
        value: &'value ComponentValue,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        self.visit_field_shape(field, &field.shape, value, field.path())
    }

    fn visit_field_shape<R>(
        &mut self,
        field: &'schema ComponentFieldSchema,
        shape: &ComponentValueShape,
        value: &'value ComponentValue,
        root: &ComponentFieldPath,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        if !self.admitted_schema {
            shape.validate().map_err(|_| {
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
                if shape.contains_kind(kind) && !field.has_capability(capability) {
                    return Err(match capability {
                        ComponentCapability::EntityRef => {
                            ComponentEntityReferenceRewriteError::MissingEntityRefCapability {
                                path: field.path.clone(),
                            }
                        }
                        _ => ComponentEntityReferenceRewriteError::MissingAssetRefCapability {
                            path: field.path.clone(),
                        },
                    }
                    .into());
                }
            }
        }
        self.admission
            .allocation(path_allocation_bytes(root.segments()))?;
        let mut path = root.segments().to_vec();
        self.visit(field, shape, value, &mut path, path_depth(root))
    }

    fn visit<R>(
        &mut self,
        field: &'schema ComponentFieldSchema,
        mut shape: &ComponentValueShape,
        value: &'value ComponentValue,
        path: &mut Vec<ComponentFieldPathSegment>,
        depth: usize,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        if depth > self.limits.depth().get() {
            return Err(ComponentEntityReferenceRewriteError::DepthLimit {
                maximum: self.limits.depth().get(),
            }
            .into());
        }
        loop {
            self.admission.charge(1, 0, 0)?;
            self.shape_nodes = self.shape_nodes.saturating_add(1);
            if self.shape_nodes > self.limits.nodes().get() {
                return Err(ComponentEntityReferenceRewriteError::NodeLimit {
                    maximum: self.limits.nodes().get(),
                }
                .into());
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
                self.check_capability(field, ComponentCapability::EntityRef)?;
                self.record(field, path, ComponentValueKind::EntityRef, value)?;
            }
            (ComponentValueShape::AssetRef, value) => {
                self.check_capability(field, ComponentCapability::AssetRef)?;
                let asset_work = crate::asset_reference::asset_reference_value_work(value);
                // Entity traversal also validates asset leaves. Admit the shared decoder's
                // scratch before it parses either legacy text or a structured product.
                if self.reference_kind != ComponentValueKind::AssetRef
                    && let Some(work) = &asset_work
                {
                    self.admission.charge(
                        work.nodes,
                        work.logical_bytes,
                        work.decode_allocation_bytes,
                    )?;
                }
                if self.reference_kind != ComponentValueKind::AssetRef
                    && !crate::asset_reference::is_asset_reference_value(value)
                {
                    return Err(
                        ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                            path: self.admission.path(path)?,
                            actual: value.kind(),
                        }
                        .into(),
                    );
                }
                let payload_bytes = asset_work.as_ref().map_or_else(
                    || {
                        value
                            .get("value")
                            .and_then(ComponentValue::as_str)
                            .map_or(0, str::len)
                    },
                    |work| work.payload_bytes,
                );
                self.result_bytes = self.result_bytes.saturating_add(payload_bytes);
                self.record(field, path, ComponentValueKind::AssetRef, value)?;
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
                    self.admission.reserve(path, 1)?;
                    path.push(ComponentFieldPathSegment::index(index));
                    self.visit(field, item, value, path, depth.saturating_add(1))?;
                    path.pop();
                }
            }
            (ComponentValueShape::Map { value: shape }, ComponentValue::Map(values)) => {
                for (name, value) in values {
                    self.admission.allocation(Some(name.len()))?;
                    self.admission.reserve(path, 1)?;
                    path.push(ComponentFieldPathSegment::field(name));
                    self.visit(field, shape, value, path, depth.saturating_add(1))?;
                    path.pop();
                }
            }
            (ComponentValueShape::Opaque { kind }, value) if value.kind() == *kind => {}
            (_, value) => {
                let path = self.admission.path(path)?;
                return Err(if matches!(shape, ComponentValueShape::EntityRef)
                    && matches!(value, ComponentValue::Null)
                {
                    ComponentEntityReferenceRewriteError::RequiredReferenceMissing { path }
                } else {
                    ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                        path,
                        actual: value.kind(),
                    }
                }
                .into());
            }
        }
        Ok(())
    }

    fn visit_record<R>(
        &mut self,
        field: &'schema ComponentFieldSchema,
        members: &[ComponentShapeMember],
        values: &'value std::collections::BTreeMap<String, ComponentValue>,
        path: &mut Vec<ComponentFieldPathSegment>,
        depth: usize,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        self.admission.allocation(
            members
                .len()
                .checked_mul(MODELED_REFERENCE_BTREE_ENTRY_BYTES),
        )?;
        let names = members
            .iter()
            .map(|member| member.name.as_str())
            .collect::<BTreeSet<_>>();
        for member in members {
            self.admission
                .charge(1, member.name.len(), member.name.len())?;
            self.admission.reserve(path, 1)?;
            path.push(ComponentFieldPathSegment::field(&member.name));
            let Some(value) = values.get(&member.name) else {
                let owned = self.admission.path(path)?;
                let error_path = self.admission.path(path)?;
                self.admission.allocation(Some(member.name.len()))?;
                return Err(ComponentEntityReferenceRewriteError::InvalidPath {
                    error: ComponentFieldPathError::MissingField {
                        path: error_path,
                        field: member.name.clone(),
                    },
                    path: owned,
                }
                .into());
            };
            self.visit(field, &member.shape, value, path, depth.saturating_add(1))?;
            path.pop();
        }
        if let Some(name) = values.keys().find(|name| !names.contains(name.as_str())) {
            self.admission.allocation(Some(name.len()))?;
            self.admission.reserve(path, 1)?;
            path.push(ComponentFieldPathSegment::field(name));
            return Err(
                ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                    path: self.admission.path(path)?,
                    actual: values[name].kind(),
                }
                .into(),
            );
        }
        Ok(())
    }

    fn record<R>(
        &mut self,
        field: &'schema ComponentFieldSchema,
        path: &[ComponentFieldPathSegment],
        kind: ComponentValueKind,
        value: &'value ComponentValue,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        if kind != ComponentValueKind::EntityRef && kind != self.reference_kind {
            return Ok(());
        }
        self.result_nodes = self.result_nodes.saturating_add(1);
        if self.result_nodes > self.limits.nodes().get() {
            return Err(ComponentEntityReferenceRewriteError::NodeLimit {
                maximum: self.limits.nodes().get(),
            }
            .into());
        }
        self.result_bytes = self
            .result_bytes
            .saturating_add(field.id().as_str().len())
            .saturating_add(path_logical_bytes(path));
        if self.result_bytes > self.limits.bytes().get() {
            return Err(ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: self.limits.bytes().get(),
            }
            .into());
        }
        let asset_work = (kind == ComponentValueKind::AssetRef)
            .then(|| crate::asset_reference::asset_reference_value_work(value))
            .flatten();
        let payload_bytes = if let Some(work) = &asset_work {
            work.payload_bytes
        } else if kind == ComponentValueKind::AssetRef {
            value
                .get("value")
                .and_then(ComponentValue::as_str)
                .map_or(0, str::len)
        } else {
            0
        };
        let logical_bytes = field
            .id()
            .as_str()
            .len()
            .saturating_add(path_logical_bytes(path))
            .saturating_add(payload_bytes);
        let output_bytes = if kind == self.reference_kind && kind == ComponentValueKind::AssetRef {
            size_of::<crate::DeclaredAssetReference>()
                .checked_add(field.id().as_str().len())
                .and_then(|bytes| {
                    // Product diagnostics retain no supplied text, but canonical UUID checks
                    // use bounded scratch. Legacy StableId failures still own their input.
                    bytes.checked_add(
                        asset_work
                            .as_ref()
                            .map_or(0, |work| work.decode_allocation_bytes),
                    )
                })
                .ok_or(ComponentEntityReferenceRewriteError::ByteLimit {
                    maximum: self.limits.bytes().get(),
                })?
        } else {
            0
        };
        // Admit the owned output before cloning even its first path or parsing its AssetRef.
        self.admission.charge(1, logical_bytes, output_bytes)?;
        let path = self.admission.path(path)?;
        if kind == ComponentValueKind::EntityRef {
            self.admission
                .allocation(Some(MODELED_REFERENCE_BTREE_ENTRY_BYTES))?;
            let declared_path = self.admission.path(path.segments())?;
            self.entity_paths.insert(declared_path);
        }
        if kind == self.reference_kind {
            self.admission.reserve(&mut self.references, 1)?;
            self.references.push(PresentReferenceLeaf {
                field,
                path,
                value,
                required: true,
            });
        }
        Ok(())
    }
}

fn find_undeclared_reference_path<R, E>(
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    declared_paths: &BTreeSet<ComponentFieldPath>,
    result_nodes: usize,
    result_bytes: usize,
    root_path: &ComponentFieldPath,
    admission: &mut ReferenceAdmission<'_, E>,
) -> Result<Option<ComponentFieldPath>, ReferenceTraversalError<R, E>> {
    let mut path = Vec::new();
    admission.reserve(&mut path, root_path.segments().len())?;
    path.extend(root_path.segments().iter().map(|segment| match segment {
        ComponentFieldPathSegment::Field(name) => TraversalPathSegment::Field(name),
        ComponentFieldPathSegment::Index(index) => TraversalPathSegment::Index(*index),
    }));
    let mut traversal = ReferencePathTraversal {
        limits,
        declared_paths,
        path,
        observed_nodes: result_nodes,
        observed_bytes: result_bytes,
        first_undeclared_path: None,
    };
    traversal.visit(value, path_depth(root_path), admission)?;
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
    fn visit<R, E>(
        &mut self,
        root: &'value ComponentValue,
        depth: usize,
        admission: &mut ReferenceAdmission<'_, E>,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        let mut stack = Vec::new();
        admission.reserve(&mut stack, 1)?;
        stack.push(ReferenceTraversalFrame::Visit {
            value: root,
            depth,
            segment: None,
        });
        while let Some(frame) = stack.pop() {
            match frame {
                ReferenceTraversalFrame::Visit {
                    value,
                    depth,
                    segment,
                } => self.visit_value(value, depth, segment, &mut stack, admission)?,
                ReferenceTraversalFrame::ContinueList { mut items, depth } => {
                    if let Some((index, item)) = items.next() {
                        let index = u32::try_from(index)
                            .map_err(|_| ComponentEntityReferenceRewriteError::PathIndexOverflow)?;
                        admission.reserve(&mut stack, 2)?;
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
                        admission.charge(0, field.len(), 0)?;
                        self.observed_bytes = self.observed_bytes.saturating_add(field.len());
                        if self.observed_bytes > self.limits.bytes().get() {
                            return Err(ComponentEntityReferenceRewriteError::ByteLimit {
                                maximum: self.limits.bytes().get(),
                            }
                            .into());
                        }
                        admission.reserve(&mut stack, 2)?;
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

    fn visit_value<R, E>(
        &mut self,
        value: &'value ComponentValue,
        depth: usize,
        segment: Option<TraversalPathSegment<'value>>,
        stack: &mut Vec<ReferenceTraversalFrame<'value>>,
        admission: &mut ReferenceAdmission<'_, E>,
    ) -> Result<(), ReferenceTraversalError<R, E>> {
        let dynamic_bytes =
            dynamic_value_bytes(value).ok_or(ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: self.limits.bytes().get(),
            })?;
        admission.charge(1, dynamic_bytes, 0)?;
        if let Some(segment) = segment {
            admission.reserve(&mut self.path, 1)?;
            self.path.push(segment);
            admission.reserve(stack, 1)?;
            stack.push(ReferenceTraversalFrame::Exit);
        }
        if depth > self.limits.depth().get() {
            return Err(ComponentEntityReferenceRewriteError::DepthLimit {
                maximum: self.limits.depth().get(),
            }
            .into());
        }
        self.observed_nodes = self.observed_nodes.checked_add(1).ok_or(
            ComponentEntityReferenceRewriteError::NodeLimit {
                maximum: self.limits.nodes().get(),
            },
        )?;
        if self.observed_nodes > self.limits.nodes().get() {
            return Err(ComponentEntityReferenceRewriteError::NodeLimit {
                maximum: self.limits.nodes().get(),
            }
            .into());
        }
        self.observed_bytes = self.observed_bytes.checked_add(dynamic_bytes).ok_or(
            ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: self.limits.bytes().get(),
            },
        )?;
        if self.observed_bytes > self.limits.bytes().get() {
            return Err(ComponentEntityReferenceRewriteError::ByteLimit {
                maximum: self.limits.bytes().get(),
            }
            .into());
        }

        match value {
            ComponentValue::EntityReference(_) => {
                if self.first_undeclared_path.is_none() {
                    let allocation_bytes = self
                        .path
                        .len()
                        .checked_mul(size_of::<ComponentFieldPathSegment>())
                        .and_then(|bytes| {
                            self.path.iter().try_fold(bytes, |bytes, segment| {
                                bytes.checked_add(match segment {
                                    TraversalPathSegment::Field(name) => name.len(),
                                    TraversalPathSegment::Index(_) => 0,
                                })
                            })
                        });
                    admission.allocation(allocation_bytes)?;
                    let candidate = ComponentFieldPath::new(
                        self.path.iter().map(TraversalPathSegment::to_owned),
                    );
                    if !self.declared_paths.contains(&candidate) {
                        self.first_undeclared_path = Some(candidate);
                    }
                }
            }
            ComponentValue::List(items) => {
                admission.reserve(stack, 1)?;
                stack.push(ReferenceTraversalFrame::ContinueList {
                    items: items.iter().enumerate(),
                    depth: depth.saturating_add(1),
                });
            }
            ComponentValue::Map(fields) => {
                admission.reserve(stack, 1)?;
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
