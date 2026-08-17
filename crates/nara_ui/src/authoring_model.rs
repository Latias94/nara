//! Toolkit-neutral authoring facts for one persistent UI Prefab.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock},
};

use nara_reflect::{
    ComponentFieldId, ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::{
    PrefabDocument, SceneComponentRecord, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation,
};
use nara_ui_kernel::{UiPoint, UiRect, UiSize, UiSurfaceContentTransform};
use thiserror::Error;

use crate::codec;

const MAX_AUTHORING_NODES: usize = 4_096;
const MAX_AUTHORING_FIELDS: usize = 32_768;
const MAX_AUTHORING_DEPTH: usize = 64;
const MAX_COMPOSITE_FIELDS: usize = 8;

/// Closed first-party UI component vocabulary exposed to Editor authoring products.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UiAuthoringComponentKind {
    Root,
    Node,
    Visual,
    Text,
    Button,
    ButtonVisuals,
    Semantics,
    Theme,
}

impl UiAuthoringComponentKind {
    const ALL: [Self; 8] = [
        Self::Root,
        Self::Node,
        Self::Visual,
        Self::Text,
        Self::Button,
        Self::ButtonVisuals,
        Self::Semantics,
        Self::Theme,
    ];

    const fn type_name(self) -> &'static str {
        match self {
            Self::Root => "nara.ui.UiRoot",
            Self::Node => "nara.ui.UiNode",
            Self::Visual => "nara.ui.UiVisual",
            Self::Text => "nara.ui.UiText",
            Self::Button => "nara.ui.UiButton",
            Self::ButtonVisuals => "nara.ui.UiButtonVisuals",
            Self::Semantics => "nara.ui.UiSemantics",
            Self::Theme => "nara.ui.UiTheme",
        }
    }

    #[must_use]
    pub fn component_type(self) -> ComponentTypeId {
        ComponentTypeId::new(self.type_name())
    }
}

/// Stable source-local identity of one Inspector property.
///
/// Composite properties use their first canonical field as the primary identity; the complete
/// bounded member set remains part of the descriptor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiAuthoringFieldKey {
    entity: SceneEntityId,
    component: ComponentTypeId,
    primary_field: ComponentFieldId,
}

impl UiAuthoringFieldKey {
    #[must_use]
    pub fn new(
        entity: SceneEntityId,
        component: ComponentTypeId,
        primary_field: ComponentFieldId,
    ) -> Self {
        Self {
            entity,
            component,
            primary_field,
        }
    }

    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        &self.entity
    }

    #[must_use]
    pub const fn component(&self) -> &ComponentTypeId {
        &self.component
    }

    #[must_use]
    pub const fn primary_field(&self) -> &ComponentFieldId {
        &self.primary_field
    }
}

/// Logical Inspector grouping. Toolkit labels are presentation and never identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringFieldGroup {
    Root,
    Layout,
    Flex,
    Appearance,
    Text,
    Interaction,
    Theme,
    Semantics,
}

/// Commit behavior expected by a toolkit Adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringCommitPolicy {
    Discrete,
    Continuous,
}

/// Optional numeric metadata that a toolkit may render as an appropriate control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiAuthoringScalarMetadata {
    minimum: Option<f64>,
    maximum: Option<f64>,
    step: f64,
    precision: u8,
    unit: Option<&'static str>,
}

impl UiAuthoringScalarMetadata {
    const fn new(
        minimum: Option<f64>,
        maximum: Option<f64>,
        step: f64,
        precision: u8,
        unit: Option<&'static str>,
    ) -> Self {
        Self {
            minimum,
            maximum,
            step,
            precision,
            unit,
        }
    }

    #[must_use]
    pub const fn minimum(self) -> Option<f64> {
        self.minimum
    }

    #[must_use]
    pub const fn maximum(self) -> Option<f64> {
        self.maximum
    }

    #[must_use]
    pub const fn step(self) -> f64 {
        self.step
    }

    #[must_use]
    pub const fn precision(self) -> u8 {
        self.precision
    }

    #[must_use]
    pub const fn unit(self) -> Option<&'static str> {
        self.unit
    }
}

/// Typed control vocabulary shared by egui and future first-party Editor Adapters.
#[derive(Debug, Clone, PartialEq)]
pub enum UiAuthoringFieldKind {
    Bool,
    SignedInteger,
    UnsignedInteger,
    Scalar(UiAuthoringScalarMetadata),
    Text { multiline: bool },
    Action,
    Enum { choices: Vec<&'static str> },
    Dimension,
    Color,
    Brush,
    ReadOnly,
}

/// Authored dimension variant, kept separate from resolved layout size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringDimensionKind {
    Auto,
    Pixels,
    Percent,
}

/// Authored brush discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiBrushAuthoringKind {
    Solid,
    Role,
    Image,
}

/// Typed current or proposed value for one descriptor.
#[derive(Debug, Clone, PartialEq)]
pub enum UiAuthoringFieldValue {
    Bool(bool),
    SignedInteger(i64),
    UnsignedInteger(u64),
    Scalar(f64),
    Text(String),
    Action(String),
    Enum(String),
    Dimension {
        kind: UiAuthoringDimensionKind,
        value: f64,
    },
    Color {
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    },
    Brush {
        kind: UiBrushAuthoringKind,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        reference: String,
    },
    Raw(Vec<ComponentValue>),
}

/// Exact canonical member retained by one descriptor.
#[derive(Debug, Clone, PartialEq)]
pub struct UiAuthoringFieldMember {
    field: ComponentFieldId,
    value_kind: ComponentValueKind,
    value: ComponentValue,
}

impl UiAuthoringFieldMember {
    #[must_use]
    pub const fn field(&self) -> &ComponentFieldId {
        &self.field
    }

    #[must_use]
    pub const fn value_kind(&self) -> ComponentValueKind {
        self.value_kind
    }

    #[must_use]
    pub const fn value(&self) -> &ComponentValue {
        &self.value
    }
}

/// Reason a durable field remains visible but cannot be edited in this product slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringReadOnlyReason {
    SemanticAuthoringDeferred,
    AssetSelectionDeferred,
    UnsupportedComplexValue,
    ComponentVersionMismatch,
    InvalidStoredValue,
}

/// Whether the descriptor admits a mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringEditability {
    Editable,
    ReadOnly(UiAuthoringReadOnlyReason),
}

/// One stable, schema-backed field or bounded composite field.
#[derive(Debug, Clone, PartialEq)]
pub struct UiAuthoringFieldDescriptor {
    key: UiAuthoringFieldKey,
    component_version: ComponentSchemaVersion,
    label: &'static str,
    group: UiAuthoringFieldGroup,
    kind: UiAuthoringFieldKind,
    commit_policy: UiAuthoringCommitPolicy,
    editability: UiAuthoringEditability,
    members: Vec<UiAuthoringFieldMember>,
    value: UiAuthoringFieldValue,
}

impl UiAuthoringFieldDescriptor {
    #[must_use]
    pub const fn key(&self) -> &UiAuthoringFieldKey {
        &self.key
    }

    #[must_use]
    pub const fn component_version(&self) -> ComponentSchemaVersion {
        self.component_version
    }

    #[must_use]
    pub const fn label(&self) -> &'static str {
        self.label
    }

    #[must_use]
    pub const fn group(&self) -> UiAuthoringFieldGroup {
        self.group
    }

    #[must_use]
    pub const fn kind(&self) -> &UiAuthoringFieldKind {
        &self.kind
    }

    #[must_use]
    pub const fn commit_policy(&self) -> UiAuthoringCommitPolicy {
        self.commit_policy
    }

    #[must_use]
    pub const fn editability(&self) -> UiAuthoringEditability {
        self.editability
    }

    #[must_use]
    pub fn members(&self) -> &[UiAuthoringFieldMember] {
        &self.members
    }

    #[must_use]
    pub const fn value(&self) -> &UiAuthoringFieldValue {
        &self.value
    }

    /// Lowers one exact value to an atomic patch over the descriptor-declared member set.
    pub fn lower_edit(
        &self,
        value: UiAuthoringFieldValue,
    ) -> Result<ScenePatchDocument, UiAuthoringEditError> {
        if let UiAuthoringEditability::ReadOnly(reason) = self.editability {
            return Err(UiAuthoringEditError::ReadOnly(reason));
        }
        if let UiAuthoringFieldValue::Brush {
            kind: UiBrushAuthoringKind::Image,
            reference,
            ..
        } = &value
            && !matches!(
                &self.value,
                UiAuthoringFieldValue::Brush {
                    kind: UiBrushAuthoringKind::Image,
                    reference: accepted_reference,
                    ..
                } if accepted_reference == reference
            )
        {
            return Err(UiAuthoringEditError::ReadOnly(
                UiAuthoringReadOnlyReason::AssetSelectionDeferred,
            ));
        }
        let values = lower_descriptor_values(&self.kind, value)?;
        if values.len() != self.members.len() {
            return Err(UiAuthoringEditError::DescriptorMismatch);
        }
        Ok(ScenePatchDocument::new(
            self.members
                .iter()
                .zip(values)
                .map(|(member, value)| ScenePatchOperation::SetField {
                    entity: self.key.entity.clone(),
                    component: self.key.component.clone(),
                    component_version: self.component_version,
                    field: member.field.clone(),
                    value,
                }),
        ))
    }

    /// Revalidates an Adapter-provided patch against this descriptor's exact canonical contract.
    ///
    /// The Host calls this before publication so callers cannot bypass composite membership,
    /// read-only policy, enum choices, or numeric ranges by constructing tooling actions directly.
    pub fn validate_patch(&self, patch: &ScenePatchDocument) -> Result<(), UiAuthoringEditError> {
        if let UiAuthoringEditability::ReadOnly(reason) = self.editability {
            return Err(UiAuthoringEditError::ReadOnly(reason));
        }
        if patch.operations.len() != self.members.len() {
            return Err(UiAuthoringEditError::DescriptorMismatch);
        }
        let values = patch
            .operations
            .iter()
            .zip(&self.members)
            .map(|(operation, member)| {
                let ScenePatchOperation::SetField {
                    entity,
                    component,
                    component_version,
                    field,
                    value,
                } = operation
                else {
                    return Err(UiAuthoringEditError::DescriptorMismatch);
                };
                if entity != &self.key.entity
                    || component != &self.key.component
                    || *component_version != self.component_version
                    || field != &member.field
                    || value.kind() != member.value_kind
                {
                    return Err(UiAuthoringEditError::DescriptorMismatch);
                }
                Ok(value.clone())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let value = parse_descriptor_value(&self.kind, &values)
            .ok_or(UiAuthoringEditError::KindMismatch)?;
        let canonical = self.lower_edit(value)?;
        if canonical != *patch {
            return Err(UiAuthoringEditError::DescriptorMismatch);
        }
        Ok(())
    }
}

/// Typed lowering failure. Schema validation still runs again before document publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum UiAuthoringEditError {
    #[error("the UI authoring field is read-only: {0:?}")]
    ReadOnly(UiAuthoringReadOnlyReason),
    #[error("the proposed value does not match the descriptor kind")]
    KindMismatch,
    #[error("the proposed enum value is not one of the descriptor choices")]
    InvalidEnum,
    #[error("the proposed value is non-finite or outside the descriptor range")]
    OutOfRange,
    #[error("the proposed composite value does not match the descriptor member set")]
    DescriptorMismatch,
    #[error("the proposed edit does not match the descriptor commit policy")]
    CommitPolicyMismatch,
    #[error("direct manipulation is read-only: {0:?}")]
    DirectManipulationReadOnly(UiDirectManipulationReadOnlyReason),
    #[error("the proposed edit makes the complete UI component invalid")]
    InvalidComponentValue,
    #[error("the proposed component value could not be represented")]
    ValueEncodingFailed,
}

/// Ordered source-local hierarchy row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiAuthoringHierarchyNode {
    entity: SceneEntityId,
    parent: Option<SceneEntityId>,
    depth: u16,
    structural_order: u32,
    components: Vec<UiAuthoringComponentKind>,
    gizmos: UiAuthoringGizmoCapabilities,
}

impl UiAuthoringHierarchyNode {
    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        &self.entity
    }

    #[must_use]
    pub const fn parent(&self) -> Option<&SceneEntityId> {
        self.parent.as_ref()
    }

    #[must_use]
    pub const fn depth(&self) -> u16 {
        self.depth
    }

    #[must_use]
    pub const fn structural_order(&self) -> u32 {
        self.structural_order
    }

    #[must_use]
    pub fn components(&self) -> &[UiAuthoringComponentKind] {
        &self.components
    }

    #[must_use]
    pub const fn gizmos(&self) -> UiAuthoringGizmoCapabilities {
        self.gizmos
    }
}

/// Why one direct manipulation class is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiDirectManipulationReadOnlyReason {
    FlowManaged,
    MissingAuthoredField,
    AutoDimension,
    PercentDimension,
    InvalidAuthoredValue,
}

/// Per-operation direct manipulation admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiDirectManipulationCapability {
    Editable,
    ReadOnly(UiDirectManipulationReadOnlyReason),
}

/// Closed two-dimensional delta relative to the exact current authoring projection.
#[derive(Debug, Clone, PartialEq)]
pub enum UiAuthoringDirectEdit {
    /// Moves an absolute pixel-positioned node in authored logical units.
    MoveBy {
        entity: SceneEntityId,
        delta: [f64; 2],
    },
    /// Resizes the bottom-right corner of an absolute pixel-sized node.
    ResizeBy {
        entity: SceneEntityId,
        delta: [f64; 2],
    },
}

/// Move and resize admission derived only from authored layout fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiAuthoringGizmoCapabilities {
    move_node: UiDirectManipulationCapability,
    resize_node: UiDirectManipulationCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiDirectGeometryKind {
    Move,
    Resize,
}

impl UiDirectGeometryKind {
    const fn primary_fields(self) -> [&'static str; 2] {
        match self {
            Self::Move => ["style.left.kind", "style.top.kind"],
            Self::Resize => ["style.width.kind", "style.height.kind"],
        }
    }

    fn from_primary_field(field: &str) -> Option<Self> {
        match field {
            "style.left.kind" => Some(Self::Move),
            "style.width.kind" => Some(Self::Resize),
            _ => None,
        }
    }
}

impl UiAuthoringGizmoCapabilities {
    #[must_use]
    pub const fn move_node(self) -> UiDirectManipulationCapability {
        self.move_node
    }

    #[must_use]
    pub const fn resize_node(self) -> UiDirectManipulationCapability {
        self.resize_node
    }
}

/// Stable source-map row used later to bind resolved Canvas facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiAuthoringSourceMapEntry {
    entity: SceneEntityId,
    structural_order: u32,
}

impl UiAuthoringSourceMapEntry {
    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        &self.entity
    }

    #[must_use]
    pub const fn structural_order(&self) -> u32 {
        self.structural_order
    }
}

/// Resolved Canvas fact for one exact source node, produced by the real UI compiler.
#[derive(Debug, Clone, PartialEq)]
pub struct UiAuthoringCanvasFact {
    entity: SceneEntityId,
    rect: UiRect,
    clip_rect: Option<UiRect>,
    paint_order: u32,
    visible: bool,
}

impl UiAuthoringCanvasFact {
    #[must_use]
    pub fn new(
        entity: SceneEntityId,
        rect: UiRect,
        clip_rect: Option<UiRect>,
        paint_order: u32,
        visible: bool,
    ) -> Self {
        Self {
            entity,
            rect,
            clip_rect,
            paint_order,
            visible,
        }
    }

    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        &self.entity
    }

    #[must_use]
    pub const fn rect(&self) -> UiRect {
        self.rect
    }

    #[must_use]
    pub const fn clip_rect(&self) -> Option<UiRect> {
        self.clip_rect
    }

    #[must_use]
    pub const fn paint_order(&self) -> u32 {
        self.paint_order
    }

    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }
}

/// Resource-free resolved Canvas facts. Native/GPU/presentation identities remain Host-owned.
#[derive(Debug, Clone, PartialEq)]
pub struct UiAuthoringCanvasReady {
    content_transform: UiSurfaceContentTransform,
    facts: Arc<[UiAuthoringCanvasFact]>,
}

impl UiAuthoringCanvasReady {
    #[must_use]
    pub fn new(
        content_transform: UiSurfaceContentTransform,
        facts: impl IntoIterator<Item = UiAuthoringCanvasFact>,
    ) -> Self {
        Self {
            content_transform,
            facts: facts.into_iter().collect::<Vec<_>>().into(),
        }
    }

    #[must_use]
    pub const fn logical_size(&self) -> UiSize {
        self.content_transform.authored_logical_size()
    }

    #[must_use]
    pub const fn content_transform(&self) -> UiSurfaceContentTransform {
        self.content_transform
    }

    /// Maps normalized physical-surface coordinates through the exact content transform.
    /// Letterbox and safe-area points are rejected rather than clamped into authored content.
    #[must_use]
    pub fn authored_point_from_surface_uv(&self, uv: [f32; 2]) -> Option<UiPoint> {
        if !uv
            .into_iter()
            .all(|value| value.is_finite() && (0.0..1.0).contains(&value))
        {
            return None;
        }
        let physical = self.content_transform.physical_surface_size();
        self.content_transform.physical_to_authored(UiPoint::new(
            uv[0] * physical.width(),
            uv[1] * physical.height(),
        ))
    }

    /// Resolves the topmost visible source entity at one normalized physical-surface point.
    #[must_use]
    pub fn source_at_surface_uv(&self, uv: [f32; 2]) -> Option<&SceneEntityId> {
        let point = self.authored_point_from_surface_uv(uv)?;
        self.facts
            .iter()
            .filter(|fact| fact.visible())
            .filter(|fact| {
                let Some(visible) = fact
                    .clip_rect()
                    .map_or(Some(fact.rect()), |clip| fact.rect().intersect(clip))
                else {
                    return false;
                };
                visible.contains(point)
            })
            .max_by_key(|fact| fact.paint_order())
            .map(UiAuthoringCanvasFact::entity)
    }

    #[must_use]
    pub fn facts(&self) -> &[UiAuthoringCanvasFact] {
        &self.facts
    }
}

/// Reason the Canvas cannot currently be projected while durable authoring remains available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringCanvasUnavailableReason {
    PrimarySurfaceProfileMissing,
    PresentationNotCompiled,
    SourceNotActive,
}

/// Closed authoring Canvas outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum UiAuthoringCanvasProjection {
    Unavailable(UiAuthoringCanvasUnavailableReason),
    Rejected(Vec<UiAuthoringDiagnostic>),
    Ready(UiAuthoringCanvasReady),
}

/// Typed durable projection diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiAuthoringDiagnostic {
    entity: Option<SceneEntityId>,
    component: Option<ComponentTypeId>,
    field: Option<ComponentFieldId>,
    reason: UiAuthoringDiagnosticReason,
}

impl UiAuthoringDiagnostic {
    /// Creates a projection-level diagnostic for a failed renderer-neutral Canvas compile.
    #[must_use]
    pub const fn canvas_compilation_rejected() -> Self {
        Self {
            entity: None,
            component: None,
            field: None,
            reason: UiAuthoringDiagnosticReason::CanvasCompilationRejected,
        }
    }

    #[must_use]
    pub const fn entity(&self) -> Option<&SceneEntityId> {
        self.entity.as_ref()
    }

    #[must_use]
    pub const fn component(&self) -> Option<&ComponentTypeId> {
        self.component.as_ref()
    }

    #[must_use]
    pub const fn field(&self) -> Option<&ComponentFieldId> {
        self.field.as_ref()
    }

    #[must_use]
    pub const fn reason(&self) -> UiAuthoringDiagnosticReason {
        self.reason
    }
}

/// Closed diagnostic reason; free-form backend/toolkit strings do not become durable identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringDiagnosticReason {
    ComponentVersionMismatch,
    MissingRequiredField,
    InvalidStoredValue,
    CanvasCompilationRejected,
}

/// Immutable authoring projection over exactly one persistent Prefab document.
#[derive(Debug, Clone, PartialEq)]
pub struct UiAuthoringProjection {
    hierarchy: Arc<[UiAuthoringHierarchyNode]>,
    fields: Arc<[UiAuthoringFieldDescriptor]>,
    components: Arc<BTreeMap<(SceneEntityId, ComponentTypeId), SceneComponentRecord>>,
    source_map: Arc<[UiAuthoringSourceMapEntry]>,
    canvas: UiAuthoringCanvasProjection,
    diagnostics: Arc<[UiAuthoringDiagnostic]>,
}

impl UiAuthoringProjection {
    #[must_use]
    pub fn hierarchy(&self) -> &[UiAuthoringHierarchyNode] {
        &self.hierarchy
    }

    #[must_use]
    pub fn fields(&self) -> &[UiAuthoringFieldDescriptor] {
        &self.fields
    }

    #[must_use]
    pub fn source_map(&self) -> &[UiAuthoringSourceMapEntry] {
        &self.source_map
    }

    #[must_use]
    pub const fn canvas(&self) -> &UiAuthoringCanvasProjection {
        &self.canvas
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[UiAuthoringDiagnostic] {
        &self.diagnostics
    }

    pub(crate) fn component_record(
        &self,
        entity: &SceneEntityId,
        kind: UiAuthoringComponentKind,
    ) -> Option<&SceneComponentRecord> {
        self.components
            .get(&(entity.clone(), kind.component_type()))
    }

    #[must_use]
    pub fn field(&self, key: &UiAuthoringFieldKey) -> Option<&UiAuthoringFieldDescriptor> {
        self.fields
            .binary_search_by(|field| field.key.cmp(key))
            .ok()
            .map(|index| &self.fields[index])
    }

    /// Revalidates one exact field patch against descriptor and codec facts.
    pub fn validate_field_patch(
        &self,
        key: &UiAuthoringFieldKey,
        commit_policy: UiAuthoringCommitPolicy,
        patch: &ScenePatchDocument,
    ) -> Result<(), UiAuthoringEditError> {
        let descriptor = self
            .field(key)
            .ok_or(UiAuthoringEditError::DescriptorMismatch)?;
        if descriptor.commit_policy != commit_policy {
            return Err(UiAuthoringEditError::CommitPolicyMismatch);
        }
        descriptor.validate_patch(patch)?;
        self.validate_component_patch(key, patch)
    }

    /// Lowers one closed two-dimensional manipulation without exposing canonical fields.
    pub fn lower_direct_edit(
        &self,
        edit: &UiAuthoringDirectEdit,
    ) -> Result<(UiAuthoringFieldKey, ScenePatchDocument), UiAuthoringEditError> {
        let (entity, kind, delta) = match edit {
            UiAuthoringDirectEdit::MoveBy { entity, delta } => {
                (entity, UiDirectGeometryKind::Move, *delta)
            }
            UiAuthoringDirectEdit::ResizeBy { entity, delta } => {
                (entity, UiDirectGeometryKind::Resize, *delta)
            }
        };
        let descriptors = self.direct_geometry_descriptors(entity, kind)?;
        let mut values = [0.0; 2];
        for (index, descriptor) in descriptors.into_iter().enumerate() {
            values[index] = direct_descriptor_pixel_value(descriptor)? + delta[index];
        }
        self.lower_direct_geometry(entity, kind, values)
    }

    /// Revalidates an exact direct-manipulation patch against current authored geometry facts.
    pub fn validate_direct_patch(
        &self,
        key: &UiAuthoringFieldKey,
        patch: &ScenePatchDocument,
    ) -> Result<(), UiAuthoringEditError> {
        let kind = UiDirectGeometryKind::from_primary_field(key.primary_field.as_str())
            .ok_or(UiAuthoringEditError::DescriptorMismatch)?;
        if key.component != UiAuthoringComponentKind::Node.component_type() {
            return Err(UiAuthoringEditError::DescriptorMismatch);
        }
        let descriptors = self.direct_geometry_descriptors(&key.entity, kind)?;
        let expected_operations = descriptors
            .iter()
            .map(|descriptor| descriptor.members.len())
            .sum::<usize>();
        if patch.operations.len() != expected_operations {
            return Err(UiAuthoringEditError::DescriptorMismatch);
        }

        let mut offset = 0;
        let mut values = [0.0; 2];
        for (index, descriptor) in descriptors.into_iter().enumerate() {
            let end = offset + descriptor.members.len();
            let descriptor_patch =
                ScenePatchDocument::new(patch.operations[offset..end].iter().cloned());
            descriptor.validate_patch(&descriptor_patch)?;
            values[index] = direct_pixel_value(descriptor, &descriptor_patch)?;
            offset = end;
        }

        let (canonical_key, canonical_patch) =
            self.lower_direct_geometry(&key.entity, kind, values)?;
        if canonical_key != *key || canonical_patch != *patch {
            return Err(UiAuthoringEditError::DescriptorMismatch);
        }
        Ok(())
    }

    fn lower_direct_geometry(
        &self,
        entity: &SceneEntityId,
        kind: UiDirectGeometryKind,
        values: [f64; 2],
    ) -> Result<(UiAuthoringFieldKey, ScenePatchDocument), UiAuthoringEditError> {
        let descriptors = self.direct_geometry_descriptors(entity, kind)?;
        let operations = descriptors
            .iter()
            .zip(values)
            .map(|(descriptor, value)| {
                descriptor.lower_edit(UiAuthoringFieldValue::Dimension {
                    kind: UiAuthoringDimensionKind::Pixels,
                    value,
                })
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|patch| patch.operations)
            .collect::<Vec<_>>();
        let key = descriptors[0].key.clone();
        let patch = ScenePatchDocument::new(operations);
        self.validate_component_patch(&key, &patch)?;
        Ok((key, patch))
    }

    fn direct_geometry_descriptors(
        &self,
        entity: &SceneEntityId,
        kind: UiDirectGeometryKind,
    ) -> Result<[&UiAuthoringFieldDescriptor; 2], UiAuthoringEditError> {
        let node = self
            .hierarchy
            .iter()
            .find(|node| node.entity == *entity)
            .ok_or(UiAuthoringEditError::DescriptorMismatch)?;
        let capability = match kind {
            UiDirectGeometryKind::Move => node.gizmos.move_node(),
            UiDirectGeometryKind::Resize => node.gizmos.resize_node(),
        };
        if let UiDirectManipulationCapability::ReadOnly(reason) = capability {
            return Err(UiAuthoringEditError::DirectManipulationReadOnly(reason));
        }

        let component = UiAuthoringComponentKind::Node.component_type();
        let [first, second] = kind.primary_fields();
        let first = self
            .field(&UiAuthoringFieldKey::new(
                entity.clone(),
                component.clone(),
                ComponentFieldId::new(first),
            ))
            .ok_or(UiAuthoringEditError::DescriptorMismatch)?;
        let second = self
            .field(&UiAuthoringFieldKey::new(
                entity.clone(),
                component,
                ComponentFieldId::new(second),
            ))
            .ok_or(UiAuthoringEditError::DescriptorMismatch)?;
        Ok([first, second])
    }

    fn validate_component_patch(
        &self,
        key: &UiAuthoringFieldKey,
        patch: &ScenePatchDocument,
    ) -> Result<(), UiAuthoringEditError> {
        let record = self
            .components
            .get(&(key.entity.clone(), key.component.clone()))
            .ok_or(UiAuthoringEditError::DescriptorMismatch)?;
        let schema = authoring_schemas()
            .get(&key.component)
            .ok_or(UiAuthoringEditError::DescriptorMismatch)?;
        let mut candidate = record.value.clone();
        for operation in &patch.operations {
            let ScenePatchOperation::SetField {
                entity,
                component,
                component_version,
                field,
                value,
            } = operation
            else {
                return Err(UiAuthoringEditError::DescriptorMismatch);
            };
            if entity != &key.entity || component != &key.component {
                return Err(UiAuthoringEditError::DescriptorMismatch);
            }
            let field = schema
                .fields
                .iter()
                .find(|candidate| candidate.id == *field)
                .ok_or(UiAuthoringEditError::DescriptorMismatch)?;
            if *component_version != schema.version || value.kind() != field.value_kind {
                return Err(UiAuthoringEditError::DescriptorMismatch);
            }
            candidate
                .replace_path(&field.path, value.clone())
                .map_err(|_| UiAuthoringEditError::DescriptorMismatch)?;
        }
        codec::validate_ui_component_value(&key.component, &candidate)
            .map_err(|_| UiAuthoringEditError::InvalidComponentValue)
    }

    /// Rebinds only the transient Canvas outcome; durable source facts remain unchanged.
    #[must_use]
    pub fn with_canvas(mut self, canvas: UiAuthoringCanvasProjection) -> Self {
        self.canvas = canvas;
        self
    }
}

fn direct_pixel_value(
    descriptor: &UiAuthoringFieldDescriptor,
    patch: &ScenePatchDocument,
) -> Result<f64, UiAuthoringEditError> {
    let values = patch
        .operations
        .iter()
        .map(|operation| match operation {
            ScenePatchOperation::SetField { value, .. } => Ok(value.clone()),
            _ => Err(UiAuthoringEditError::DescriptorMismatch),
        })
        .collect::<Result<Vec<_>, _>>()?;
    match parse_descriptor_value(&descriptor.kind, &values) {
        Some(UiAuthoringFieldValue::Dimension {
            kind: UiAuthoringDimensionKind::Pixels,
            value,
        }) => Ok(value),
        _ => Err(UiAuthoringEditError::DescriptorMismatch),
    }
}

fn direct_descriptor_pixel_value(
    descriptor: &UiAuthoringFieldDescriptor,
) -> Result<f64, UiAuthoringEditError> {
    match descriptor.value() {
        UiAuthoringFieldValue::Dimension {
            kind: UiAuthoringDimensionKind::Pixels,
            value,
        } => Ok(*value),
        _ => Err(UiAuthoringEditError::DescriptorMismatch),
    }
}

/// Whole-projection failure before any Editor frame can be published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum UiAuthoringProjectionError {
    #[error("the Prefab does not contain a source-local UI root")]
    MissingUiRoot,
    #[error("a source-local UI root is missing its required UiNode component")]
    UiRootMissingNode,
    #[error("the UI authoring hierarchy contains a missing entity")]
    MissingEntity,
    #[error("the UI authoring hierarchy contains a cycle or duplicate placement")]
    InvalidHierarchy,
    #[error("the UI authoring hierarchy exceeds its node or depth ceiling")]
    HierarchyLimitExceeded,
    #[error("the UI authoring projection exceeds its field ceiling")]
    FieldLimitExceeded,
}

/// Projects the persistent UI vocabulary of one Prefab without importing Editor/toolkit identity.
pub fn project_prefab_authoring(
    prefab: &PrefabDocument,
) -> Result<UiAuthoringProjection, UiAuthoringProjectionError> {
    let entities = prefab
        .entities
        .iter()
        .map(|entity| (&entity.id, entity))
        .collect::<BTreeMap<_, _>>();
    let ordered = ordered_entities(prefab, &entities)?;
    let root_type = UiAuthoringComponentKind::Root.component_type();
    let roots = ordered
        .iter()
        .filter(|entity| entity.components.contains_key(&root_type))
        .copied()
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return Err(UiAuthoringProjectionError::MissingUiRoot);
    }
    let node_type = UiAuthoringComponentKind::Node.component_type();
    if roots
        .iter()
        .any(|root| !root.components.contains_key(&node_type))
    {
        return Err(UiAuthoringProjectionError::UiRootMissingNode);
    }

    let mut projection = ProjectionBuilder::default();
    for root in roots {
        projection.project_root(root, &entities, authoring_schemas())?;
    }
    projection.finish()
}

fn authoring_schemas() -> &'static BTreeMap<ComponentTypeId, ComponentSchema> {
    static SCHEMAS: OnceLock<BTreeMap<ComponentTypeId, ComponentSchema>> = OnceLock::new();
    SCHEMAS.get_or_init(|| {
        codec::ui_schema_catalog()
            .components
            .into_iter()
            .map(|schema| (schema.id.clone(), schema))
            .collect()
    })
}

#[derive(Default)]
struct ProjectionBuilder {
    hierarchy: Vec<UiAuthoringHierarchyNode>,
    fields: Vec<UiAuthoringFieldDescriptor>,
    components: BTreeMap<(SceneEntityId, ComponentTypeId), SceneComponentRecord>,
    source_map: Vec<UiAuthoringSourceMapEntry>,
    diagnostics: Vec<UiAuthoringDiagnostic>,
    projected: BTreeSet<SceneEntityId>,
}

impl ProjectionBuilder {
    fn project_root(
        &mut self,
        root: &SceneEntityRecord,
        entities: &BTreeMap<&SceneEntityId, &SceneEntityRecord>,
        schemas: &BTreeMap<ComponentTypeId, ComponentSchema>,
    ) -> Result<(), UiAuthoringProjectionError> {
        self.project_node(root, None, 0, entities, schemas)
    }

    fn project_node(
        &mut self,
        entity: &SceneEntityRecord,
        parent: Option<&SceneEntityId>,
        depth: usize,
        entities: &BTreeMap<&SceneEntityId, &SceneEntityRecord>,
        schemas: &BTreeMap<ComponentTypeId, ComponentSchema>,
    ) -> Result<(), UiAuthoringProjectionError> {
        if depth > MAX_AUTHORING_DEPTH || self.hierarchy.len() == MAX_AUTHORING_NODES {
            return Err(UiAuthoringProjectionError::HierarchyLimitExceeded);
        }
        if !self.projected.insert(entity.id.clone()) {
            return Err(UiAuthoringProjectionError::InvalidHierarchy);
        }
        let structural_order = u32::try_from(self.hierarchy.len())
            .map_err(|_| UiAuthoringProjectionError::HierarchyLimitExceeded)?;
        let components = entity
            .components
            .keys()
            .filter_map(component_kind)
            .collect::<Vec<_>>();
        let gizmos = authored_gizmo_capabilities(entity);
        self.hierarchy.push(UiAuthoringHierarchyNode {
            entity: entity.id.clone(),
            parent: parent.cloned(),
            depth: u16::try_from(depth)
                .map_err(|_| UiAuthoringProjectionError::HierarchyLimitExceeded)?,
            structural_order,
            components,
            gizmos,
        });
        self.source_map.push(UiAuthoringSourceMapEntry {
            entity: entity.id.clone(),
            structural_order,
        });
        self.project_component_fields(entity, schemas)?;

        for child in &entity.children {
            let child = entities
                .get(child)
                .copied()
                .ok_or(UiAuthoringProjectionError::MissingEntity)?;
            if child
                .components
                .contains_key(&UiAuthoringComponentKind::Root.component_type())
            {
                continue;
            }
            if !child
                .components
                .contains_key(&UiAuthoringComponentKind::Node.component_type())
            {
                continue;
            }
            self.project_node(child, Some(&entity.id), depth + 1, entities, schemas)?;
        }
        Ok(())
    }

    fn project_component_fields(
        &mut self,
        entity: &SceneEntityRecord,
        schemas: &BTreeMap<ComponentTypeId, ComponentSchema>,
    ) -> Result<(), UiAuthoringProjectionError> {
        for (component, record) in &entity.components {
            let Some(kind) = component_kind(component) else {
                continue;
            };
            let Some(schema) = schemas.get(component) else {
                continue;
            };
            self.components
                .insert((entity.id.clone(), component.clone()), record.clone());
            let version_matches = record.version == schema.version;
            if !version_matches {
                self.diagnostics.push(UiAuthoringDiagnostic {
                    entity: Some(entity.id.clone()),
                    component: Some(component.clone()),
                    field: None,
                    reason: UiAuthoringDiagnosticReason::ComponentVersionMismatch,
                });
            }
            let mut descriptors = descriptors_for_component(&entity.id, kind, record, schema);
            if !version_matches {
                for descriptor in &mut descriptors {
                    descriptor.editability = UiAuthoringEditability::ReadOnly(
                        UiAuthoringReadOnlyReason::ComponentVersionMismatch,
                    );
                }
            }
            if self.fields.len().saturating_add(descriptors.len()) > MAX_AUTHORING_FIELDS {
                return Err(UiAuthoringProjectionError::FieldLimitExceeded);
            }
            for descriptor in &descriptors {
                if matches!(
                    descriptor.editability,
                    UiAuthoringEditability::ReadOnly(UiAuthoringReadOnlyReason::InvalidStoredValue)
                ) {
                    self.diagnostics.push(UiAuthoringDiagnostic {
                        entity: Some(entity.id.clone()),
                        component: Some(component.clone()),
                        field: Some(descriptor.key.primary_field.clone()),
                        reason: UiAuthoringDiagnosticReason::InvalidStoredValue,
                    });
                }
            }
            self.fields.extend(descriptors);
        }
        Ok(())
    }

    fn finish(mut self) -> Result<UiAuthoringProjection, UiAuthoringProjectionError> {
        self.fields.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(UiAuthoringProjection {
            hierarchy: self.hierarchy.into(),
            fields: self.fields.into(),
            components: Arc::new(self.components),
            source_map: self.source_map.into(),
            canvas: UiAuthoringCanvasProjection::Unavailable(
                UiAuthoringCanvasUnavailableReason::PresentationNotCompiled,
            ),
            diagnostics: self.diagnostics.into(),
        })
    }
}

fn ordered_entities<'a>(
    prefab: &'a PrefabDocument,
    entities: &BTreeMap<&'a SceneEntityId, &'a SceneEntityRecord>,
) -> Result<Vec<&'a SceneEntityRecord>, UiAuthoringProjectionError> {
    let mut ordered = Vec::with_capacity(prefab.entities.len());
    let mut visited = BTreeSet::new();
    let mut pending = prefab
        .roots
        .iter()
        .rev()
        .map(|entity| (entity, 0usize))
        .collect::<Vec<_>>();
    while let Some((entity, depth)) = pending.pop() {
        if depth > MAX_AUTHORING_DEPTH || ordered.len() == MAX_AUTHORING_NODES {
            return Err(UiAuthoringProjectionError::HierarchyLimitExceeded);
        }
        if !visited.insert(entity.clone()) {
            return Err(UiAuthoringProjectionError::InvalidHierarchy);
        }
        let record = entities
            .get(entity)
            .copied()
            .ok_or(UiAuthoringProjectionError::MissingEntity)?;
        ordered.push(record);
        pending.extend(record.children.iter().rev().map(|child| (child, depth + 1)));
    }
    if visited.len() != prefab.entities.len() {
        return Err(UiAuthoringProjectionError::InvalidHierarchy);
    }
    Ok(ordered)
}

fn component_kind(component: &ComponentTypeId) -> Option<UiAuthoringComponentKind> {
    UiAuthoringComponentKind::ALL
        .into_iter()
        .find(|kind| kind.type_name() == component.as_str())
}

fn descriptors_for_component(
    entity: &SceneEntityId,
    component_kind: UiAuthoringComponentKind,
    record: &SceneComponentRecord,
    schema: &ComponentSchema,
) -> Vec<UiAuthoringFieldDescriptor> {
    let mut descriptors = Vec::new();
    let mut consumed = BTreeSet::new();
    for field in &schema.fields {
        if consumed.contains(&field.id) {
            continue;
        }
        let members = composite_members(component_kind, field, schema);
        for member in &members {
            consumed.insert(member.id.clone());
        }
        descriptors.push(build_descriptor(
            entity,
            component_kind,
            record,
            schema,
            &members,
        ));
    }
    descriptors
}

fn composite_members<'a>(
    component: UiAuthoringComponentKind,
    field: &'a ComponentFieldSchema,
    schema: &'a ComponentSchema,
) -> Vec<&'a ComponentFieldSchema> {
    let id = field.id.as_str();
    let member_ids = match component {
        UiAuthoringComponentKind::Node
            if matches!(
                id,
                "style.width.kind" | "style.height.kind" | "style.left.kind" | "style.top.kind"
            ) =>
        {
            let prefix = id.trim_end_matches(".kind");
            vec![format!("{prefix}.kind"), format!("{prefix}.value")]
        }
        UiAuthoringComponentKind::Visual
            if matches!(id, "background.kind" | "border_brush.kind") =>
        {
            brush_member_ids(id.trim_end_matches(".kind"))
        }
        UiAuthoringComponentKind::Text if id == "brush.kind" => brush_member_ids("brush"),
        UiAuthoringComponentKind::Text if id == "has_semantic_text" => {
            vec!["has_semantic_text".to_owned(), "semantic_text".to_owned()]
        }
        UiAuthoringComponentKind::Theme if id.ends_with(".red") => {
            let prefix = id.trim_end_matches(".red");
            color_member_ids(prefix)
        }
        _ => vec![id.to_owned()],
    };
    member_ids
        .into_iter()
        .filter_map(|id| {
            schema
                .fields
                .iter()
                .find(|candidate| candidate.id.as_str() == id)
        })
        .take(MAX_COMPOSITE_FIELDS)
        .collect()
}

fn brush_member_ids(prefix: &str) -> Vec<String> {
    ["kind", "red", "green", "blue", "alpha", "reference"]
        .into_iter()
        .map(|field| format!("{prefix}.{field}"))
        .collect()
}

fn color_member_ids(prefix: &str) -> Vec<String> {
    ["red", "green", "blue", "alpha"]
        .into_iter()
        .map(|field| format!("{prefix}.{field}"))
        .collect()
}

fn build_descriptor(
    entity: &SceneEntityId,
    component: UiAuthoringComponentKind,
    record: &SceneComponentRecord,
    schema: &ComponentSchema,
    fields: &[&ComponentFieldSchema],
) -> UiAuthoringFieldDescriptor {
    let primary = fields
        .first()
        .expect("every UI authoring descriptor has at least one field");
    let metadata = descriptor_metadata(component, primary.id.as_str(), fields.len());
    let members = fields
        .iter()
        .filter_map(|field| {
            record
                .value
                .get_path(&field.path)
                .ok()
                .cloned()
                .map(|value| UiAuthoringFieldMember {
                    field: field.id.clone(),
                    value_kind: field.value_kind,
                    value,
                })
        })
        .collect::<Vec<_>>();
    let complete = members.len() == fields.len();
    let values = members
        .iter()
        .map(|member| member.value.clone())
        .collect::<Vec<_>>();
    let value = parse_descriptor_value(&metadata.kind, &values);
    let (value, editability) = match value {
        Some(value) if complete => (value, metadata.editability),
        _ => (
            UiAuthoringFieldValue::Raw(values),
            UiAuthoringEditability::ReadOnly(UiAuthoringReadOnlyReason::InvalidStoredValue),
        ),
    };
    UiAuthoringFieldDescriptor {
        key: UiAuthoringFieldKey::new(entity.clone(), schema.id.clone(), primary.id.clone()),
        component_version: record.version,
        label: metadata.label,
        group: metadata.group,
        kind: metadata.kind,
        commit_policy: metadata.commit_policy,
        editability,
        members,
        value,
    }
}

struct DescriptorMetadata {
    label: &'static str,
    group: UiAuthoringFieldGroup,
    kind: UiAuthoringFieldKind,
    commit_policy: UiAuthoringCommitPolicy,
    editability: UiAuthoringEditability,
}

fn descriptor_metadata(
    component: UiAuthoringComponentKind,
    field: &str,
    member_count: usize,
) -> DescriptorMetadata {
    if component == UiAuthoringComponentKind::Semantics {
        return DescriptorMetadata {
            label: semantic_label(field),
            group: UiAuthoringFieldGroup::Semantics,
            kind: UiAuthoringFieldKind::ReadOnly,
            commit_policy: UiAuthoringCommitPolicy::Discrete,
            editability: UiAuthoringEditability::ReadOnly(
                UiAuthoringReadOnlyReason::SemanticAuthoringDeferred,
            ),
        };
    }
    if member_count == 2 && field.ends_with(".kind") {
        return DescriptorMetadata {
            label: dimension_label(field),
            group: UiAuthoringFieldGroup::Layout,
            kind: UiAuthoringFieldKind::Dimension,
            commit_policy: UiAuthoringCommitPolicy::Discrete,
            editability: UiAuthoringEditability::Editable,
        };
    }
    if member_count == 6 && field.ends_with(".kind") {
        return DescriptorMetadata {
            label: brush_label(field),
            group: UiAuthoringFieldGroup::Appearance,
            kind: UiAuthoringFieldKind::Brush,
            commit_policy: UiAuthoringCommitPolicy::Discrete,
            editability: UiAuthoringEditability::Editable,
        };
    }
    if member_count == 4 && field.ends_with(".red") {
        return DescriptorMetadata {
            label: "Color",
            group: UiAuthoringFieldGroup::Theme,
            kind: UiAuthoringFieldKind::Color,
            commit_policy: UiAuthoringCommitPolicy::Discrete,
            editability: UiAuthoringEditability::Editable,
        };
    }
    if component == UiAuthoringComponentKind::Text && field == "has_semantic_text" {
        return DescriptorMetadata {
            label: "Semantic text",
            group: UiAuthoringFieldGroup::Text,
            kind: UiAuthoringFieldKind::ReadOnly,
            commit_policy: UiAuthoringCommitPolicy::Discrete,
            editability: UiAuthoringEditability::ReadOnly(
                UiAuthoringReadOnlyReason::SemanticAuthoringDeferred,
            ),
        };
    }

    let (group, label, kind, commit_policy) = single_field_metadata(component, field);
    DescriptorMetadata {
        label,
        group,
        kind,
        commit_policy,
        editability: UiAuthoringEditability::Editable,
    }
}

fn single_field_metadata(
    component: UiAuthoringComponentKind,
    field: &str,
) -> (
    UiAuthoringFieldGroup,
    &'static str,
    UiAuthoringFieldKind,
    UiAuthoringCommitPolicy,
) {
    use UiAuthoringCommitPolicy::{Continuous, Discrete};
    use UiAuthoringFieldGroup::{Flex, Interaction, Layout, Root, Text, Theme};

    let enum_kind = |choices: &'static [&'static str]| UiAuthoringFieldKind::Enum {
        choices: choices.to_vec(),
    };
    match (component, field) {
        (UiAuthoringComponentKind::Root, "layer") => (
            Root,
            "Layer",
            enum_kind(&["base", "overlay", "modal"]),
            Discrete,
        ),
        (UiAuthoringComponentKind::Node, "style.position_mode") => (
            Layout,
            "Position mode",
            enum_kind(&["flow", "absolute"]),
            Discrete,
        ),
        (UiAuthoringComponentKind::Node, "style.flex_direction") => {
            (Flex, "Direction", enum_kind(&["row", "column"]), Discrete)
        }
        (UiAuthoringComponentKind::Node, "style.align_items") => (
            Flex,
            "Align items",
            enum_kind(&["start", "center", "end", "stretch"]),
            Discrete,
        ),
        (UiAuthoringComponentKind::Node, "style.justify_content") => (
            Flex,
            "Justify content",
            enum_kind(&["start", "center", "end", "space_between"]),
            Discrete,
        ),
        (UiAuthoringComponentKind::Node, "style.overflow") => (
            Layout,
            "Overflow",
            enum_kind(&["visible", "clip"]),
            Discrete,
        ),
        (UiAuthoringComponentKind::Text, "alignment") => (
            Text,
            "Alignment",
            enum_kind(&["start", "center", "end"]),
            Discrete,
        ),
        (UiAuthoringComponentKind::Text, "overflow") => {
            (Text, "Overflow", enum_kind(&["wrap", "clip"]), Discrete)
        }
        (UiAuthoringComponentKind::ButtonVisuals, _) if field != "focus_border_width" => (
            Interaction,
            button_visual_label(field),
            enum_kind(color_role_choices()),
            Discrete,
        ),
        (UiAuthoringComponentKind::Button, "action") => (
            Interaction,
            "Action",
            UiAuthoringFieldKind::Action,
            Discrete,
        ),
        (UiAuthoringComponentKind::Button, "label") => (
            Interaction,
            "Label",
            UiAuthoringFieldKind::Text { multiline: false },
            Discrete,
        ),
        (UiAuthoringComponentKind::Text, "content") => (
            Text,
            "Content",
            UiAuthoringFieldKind::Text { multiline: true },
            Discrete,
        ),
        (UiAuthoringComponentKind::Theme, "revision") => (
            Theme,
            "Revision",
            UiAuthoringFieldKind::UnsignedInteger,
            Discrete,
        ),
        (_, "style.z_index") => (
            Layout,
            "Z index",
            UiAuthoringFieldKind::SignedInteger,
            Discrete,
        ),
        (_, "style.visible") => (Layout, "Visible", UiAuthoringFieldKind::Bool, Discrete),
        (_, "style.stacking_context") => (
            Layout,
            "Stacking context",
            UiAuthoringFieldKind::Bool,
            Discrete,
        ),
        (UiAuthoringComponentKind::Button, "enabled") => {
            (Interaction, "Enabled", UiAuthoringFieldKind::Bool, Discrete)
        }
        (_, _) => {
            let metadata = scalar_metadata(component, field);
            let kind = match field {
                "semantic_text" => UiAuthoringFieldKind::Text { multiline: true },
                _ if field.ends_with("label") => UiAuthoringFieldKind::Text { multiline: false },
                _ => UiAuthoringFieldKind::Scalar(metadata),
            };
            (
                default_group(component, field),
                default_label(field),
                kind,
                Continuous,
            )
        }
    }
}

fn scalar_metadata(component: UiAuthoringComponentKind, field: &str) -> UiAuthoringScalarMetadata {
    if field.contains("palette.") {
        return UiAuthoringScalarMetadata::new(Some(0.0), Some(1.0), 0.01, 3, None);
    }
    if field == "logical_size" || field == "logical_text_size" || field == "focus_border_width" {
        return UiAuthoringScalarMetadata::new(Some(f64::EPSILON), None, 0.5, 1, Some("px"));
    }
    if matches!(
        component,
        UiAuthoringComponentKind::Node | UiAuthoringComponentKind::Visual
    ) && (field.contains("padding")
        || field.ends_with("gap")
        || field.contains("radii")
        || field == "border_width"
        || field.ends_with("flex_grow")
        || field.ends_with("flex_shrink"))
    {
        return UiAuthoringScalarMetadata::new(Some(0.0), None, 0.5, 1, Some("px"));
    }
    UiAuthoringScalarMetadata::new(None, None, 0.1, 2, None)
}

fn default_group(component: UiAuthoringComponentKind, field: &str) -> UiAuthoringFieldGroup {
    match component {
        UiAuthoringComponentKind::Root => UiAuthoringFieldGroup::Root,
        UiAuthoringComponentKind::Node if field.contains("flex") || field.contains("align") => {
            UiAuthoringFieldGroup::Flex
        }
        UiAuthoringComponentKind::Node => UiAuthoringFieldGroup::Layout,
        UiAuthoringComponentKind::Visual => UiAuthoringFieldGroup::Appearance,
        UiAuthoringComponentKind::Text => UiAuthoringFieldGroup::Text,
        UiAuthoringComponentKind::Button | UiAuthoringComponentKind::ButtonVisuals => {
            UiAuthoringFieldGroup::Interaction
        }
        UiAuthoringComponentKind::Theme => UiAuthoringFieldGroup::Theme,
        UiAuthoringComponentKind::Semantics => UiAuthoringFieldGroup::Semantics,
    }
}

fn default_label(field: &str) -> &'static str {
    match field {
        "style.gap" => "Gap",
        "style.padding.left" => "Padding left",
        "style.padding.right" => "Padding right",
        "style.padding.top" => "Padding top",
        "style.padding.bottom" => "Padding bottom",
        "style.flex_grow" => "Flex grow",
        "style.flex_shrink" => "Flex shrink",
        "border_width" => "Border width",
        "radii.top_left" => "Top-left radius",
        "radii.top_right" => "Top-right radius",
        "radii.bottom_right" => "Bottom-right radius",
        "radii.bottom_left" => "Bottom-left radius",
        "logical_size" => "Text size",
        "logical_text_size" => "Text size",
        "focus_border_width" => "Focus border width",
        _ => "Value",
    }
}

fn semantic_label(field: &str) -> &'static str {
    match field {
        "role" => "Role",
        "name" => "Name",
        "has_value" | "value" => "Value",
        "has_selected" | "selected" => "Selected",
        "has_expanded" | "expanded" => "Expanded",
        "actions" => "Actions",
        _ => "Semantics",
    }
}

fn dimension_label(field: &str) -> &'static str {
    if field.contains("width") {
        "Width"
    } else if field.contains("height") {
        "Height"
    } else if field.contains("left") {
        "Left"
    } else {
        "Top"
    }
}

fn brush_label(field: &str) -> &'static str {
    if field.starts_with("border_brush") {
        "Border brush"
    } else if field.starts_with("brush") {
        "Text brush"
    } else {
        "Background"
    }
}

fn button_visual_label(field: &str) -> &'static str {
    match field {
        "normal_text" => "Normal text",
        "hovered_background" => "Hovered background",
        "hovered_text" => "Hovered text",
        "pressed_background" => "Pressed background",
        "pressed_text" => "Pressed text",
        "focused_border" => "Focused border",
        "focused_text" => "Focused text",
        "disabled_background" => "Disabled background",
        "disabled_text" => "Disabled text",
        _ => "Interaction color",
    }
}

fn color_role_choices() -> &'static [&'static str] {
    &[
        "surface",
        "panel",
        "primary_text",
        "secondary_text",
        "accent",
        "positive",
        "warning",
        "danger",
        "disabled",
        "focus",
    ]
}

fn parse_descriptor_value(
    kind: &UiAuthoringFieldKind,
    values: &[ComponentValue],
) -> Option<UiAuthoringFieldValue> {
    match kind {
        UiAuthoringFieldKind::Bool => Some(UiAuthoringFieldValue::Bool(values.first()?.as_bool()?)),
        UiAuthoringFieldKind::SignedInteger => Some(UiAuthoringFieldValue::SignedInteger(
            values.first()?.as_i64()?,
        )),
        UiAuthoringFieldKind::UnsignedInteger => Some(UiAuthoringFieldValue::UnsignedInteger(
            values.first()?.as_u64()?,
        )),
        UiAuthoringFieldKind::Scalar(_) => {
            Some(UiAuthoringFieldValue::Scalar(values.first()?.as_f64()?))
        }
        UiAuthoringFieldKind::Text { .. } => Some(UiAuthoringFieldValue::Text(
            values.first()?.as_str()?.to_owned(),
        )),
        UiAuthoringFieldKind::Action => Some(UiAuthoringFieldValue::Action(
            values.first()?.as_str()?.to_owned(),
        )),
        UiAuthoringFieldKind::Enum { choices } => {
            let value = values.first()?.as_str()?;
            choices
                .contains(&value)
                .then(|| UiAuthoringFieldValue::Enum(value.to_owned()))
        }
        UiAuthoringFieldKind::Dimension => {
            let kind = match values.first()?.as_str()? {
                "auto" => UiAuthoringDimensionKind::Auto,
                "px" => UiAuthoringDimensionKind::Pixels,
                "percent" => UiAuthoringDimensionKind::Percent,
                _ => return None,
            };
            Some(UiAuthoringFieldValue::Dimension {
                kind,
                value: values.get(1)?.as_f64()?,
            })
        }
        UiAuthoringFieldKind::Color => Some(UiAuthoringFieldValue::Color {
            red: values.first()?.as_f64()?,
            green: values.get(1)?.as_f64()?,
            blue: values.get(2)?.as_f64()?,
            alpha: values.get(3)?.as_f64()?,
        }),
        UiAuthoringFieldKind::Brush => {
            let kind = match values.first()?.as_str()? {
                "solid" => UiBrushAuthoringKind::Solid,
                "role" => UiBrushAuthoringKind::Role,
                "image" => UiBrushAuthoringKind::Image,
                _ => return None,
            };
            Some(UiAuthoringFieldValue::Brush {
                kind,
                red: values.get(1)?.as_f64()?,
                green: values.get(2)?.as_f64()?,
                blue: values.get(3)?.as_f64()?,
                alpha: values.get(4)?.as_f64()?,
                reference: values.get(5)?.as_str()?.to_owned(),
            })
        }
        UiAuthoringFieldKind::ReadOnly => Some(UiAuthoringFieldValue::Raw(values.to_vec())),
    }
}

fn lower_descriptor_values(
    kind: &UiAuthoringFieldKind,
    value: UiAuthoringFieldValue,
) -> Result<Vec<ComponentValue>, UiAuthoringEditError> {
    match (kind, value) {
        (UiAuthoringFieldKind::Bool, UiAuthoringFieldValue::Bool(value)) => {
            Ok(vec![ComponentValue::Bool(value)])
        }
        (UiAuthoringFieldKind::SignedInteger, UiAuthoringFieldValue::SignedInteger(value)) => {
            Ok(vec![ComponentValue::I64(value)])
        }
        (UiAuthoringFieldKind::UnsignedInteger, UiAuthoringFieldValue::UnsignedInteger(value)) => {
            Ok(vec![ComponentValue::U64(value)])
        }
        (UiAuthoringFieldKind::Scalar(metadata), UiAuthoringFieldValue::Scalar(value)) => {
            validate_scalar(value, *metadata)?;
            Ok(vec![component_float(value)?])
        }
        (UiAuthoringFieldKind::Text { .. }, UiAuthoringFieldValue::Text(value)) => {
            Ok(vec![ComponentValue::String(value)])
        }
        (UiAuthoringFieldKind::Action, UiAuthoringFieldValue::Action(value)) => {
            Ok(vec![ComponentValue::String(value)])
        }
        (UiAuthoringFieldKind::Enum { choices }, UiAuthoringFieldValue::Enum(value)) => {
            if !choices.contains(&value.as_str()) {
                return Err(UiAuthoringEditError::InvalidEnum);
            }
            Ok(vec![ComponentValue::String(value)])
        }
        (UiAuthoringFieldKind::Dimension, UiAuthoringFieldValue::Dimension { kind, value }) => {
            let (kind, value) = match kind {
                UiAuthoringDimensionKind::Auto => ("auto", 0.0),
                UiAuthoringDimensionKind::Pixels if value.is_finite() && value >= 0.0 => {
                    ("px", value)
                }
                UiAuthoringDimensionKind::Percent
                    if value.is_finite() && (0.0..=1.0).contains(&value) =>
                {
                    ("percent", value)
                }
                UiAuthoringDimensionKind::Pixels | UiAuthoringDimensionKind::Percent => {
                    return Err(UiAuthoringEditError::OutOfRange);
                }
            };
            Ok(vec![
                ComponentValue::String(kind.to_owned()),
                component_float(value)?,
            ])
        }
        (
            UiAuthoringFieldKind::Color,
            UiAuthoringFieldValue::Color {
                red,
                green,
                blue,
                alpha,
            },
        ) => lower_color(red, green, blue, alpha),
        (
            UiAuthoringFieldKind::Brush,
            UiAuthoringFieldValue::Brush {
                kind,
                red,
                green,
                blue,
                alpha,
                reference,
            },
        ) => {
            let (kind, red, green, blue, alpha, reference) = match kind {
                UiBrushAuthoringKind::Solid if reference.is_empty() => {
                    ("solid", red, green, blue, alpha, reference)
                }
                UiBrushAuthoringKind::Role
                    if color_role_choices().contains(&reference.as_str()) =>
                {
                    ("role", 0.0, 0.0, 0.0, 0.0, reference)
                }
                UiBrushAuthoringKind::Image if !reference.is_empty() => {
                    ("image", red, green, blue, alpha, reference)
                }
                _ => return Err(UiAuthoringEditError::OutOfRange),
            };
            let mut values = vec![ComponentValue::String(kind.to_owned())];
            values.extend(lower_color(red, green, blue, alpha)?);
            values.push(ComponentValue::String(reference));
            Ok(values)
        }
        (UiAuthoringFieldKind::ReadOnly, _) => Err(UiAuthoringEditError::KindMismatch),
        _ => Err(UiAuthoringEditError::KindMismatch),
    }
}

fn lower_color(
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<Vec<ComponentValue>, UiAuthoringEditError> {
    let values = [red, green, blue, alpha];
    if values
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(UiAuthoringEditError::OutOfRange);
    }
    values.into_iter().map(component_float).collect()
}

fn validate_scalar(
    value: f64,
    metadata: UiAuthoringScalarMetadata,
) -> Result<(), UiAuthoringEditError> {
    if !value.is_finite()
        || metadata.minimum.is_some_and(|minimum| value < minimum)
        || metadata.maximum.is_some_and(|maximum| value > maximum)
    {
        return Err(UiAuthoringEditError::OutOfRange);
    }
    Ok(())
}

fn component_float(value: f64) -> Result<ComponentValue, UiAuthoringEditError> {
    ComponentValue::f64(value).map_err(|_| UiAuthoringEditError::ValueEncodingFailed)
}

fn authored_gizmo_capabilities(entity: &SceneEntityRecord) -> UiAuthoringGizmoCapabilities {
    let node_type = UiAuthoringComponentKind::Node.component_type();
    let Some(node) = entity.components.get(&node_type) else {
        return UiAuthoringGizmoCapabilities {
            move_node: UiDirectManipulationCapability::ReadOnly(
                UiDirectManipulationReadOnlyReason::MissingAuthoredField,
            ),
            resize_node: UiDirectManipulationCapability::ReadOnly(
                UiDirectManipulationReadOnlyReason::MissingAuthoredField,
            ),
        };
    };
    let position_mode = read_string_path(&node.value, &["style", "position_mode"]);
    if position_mode != Some("absolute") {
        return UiAuthoringGizmoCapabilities {
            move_node: UiDirectManipulationCapability::ReadOnly(
                UiDirectManipulationReadOnlyReason::FlowManaged,
            ),
            resize_node: UiDirectManipulationCapability::ReadOnly(
                UiDirectManipulationReadOnlyReason::FlowManaged,
            ),
        };
    }
    UiAuthoringGizmoCapabilities {
        move_node: dimensions_capability(&node.value, &["left", "top"]),
        resize_node: dimensions_capability(&node.value, &["width", "height"]),
    }
}

fn dimensions_capability(
    value: &ComponentValue,
    dimensions: &[&str],
) -> UiDirectManipulationCapability {
    let mut all_pixels = true;
    let mut has_auto = false;
    let mut has_percent = false;
    for dimension in dimensions {
        let Some(kind) = read_string_path(value, &["style", dimension, "kind"]) else {
            return UiDirectManipulationCapability::ReadOnly(
                UiDirectManipulationReadOnlyReason::MissingAuthoredField,
            );
        };
        all_pixels &= kind == "px";
        has_auto |= kind == "auto";
        has_percent |= kind == "percent";
    }
    if all_pixels {
        return UiDirectManipulationCapability::Editable;
    }
    if has_auto {
        return UiDirectManipulationCapability::ReadOnly(
            UiDirectManipulationReadOnlyReason::AutoDimension,
        );
    }
    if has_percent {
        return UiDirectManipulationCapability::ReadOnly(
            UiDirectManipulationReadOnlyReason::PercentDimension,
        );
    }
    UiDirectManipulationCapability::ReadOnly(
        UiDirectManipulationReadOnlyReason::InvalidAuthoredValue,
    )
}

fn read_string_path<'a>(value: &'a ComponentValue, fields: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for field in fields {
        current = current.get(field)?;
    }
    current.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_reflect::ComponentSchemaVersion;

    fn id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).expect("test entity id must be valid")
    }

    fn component(value: ComponentValue) -> SceneComponentRecord {
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
    }

    fn node_value(position: &str, width: (&str, f64), height: (&str, f64)) -> ComponentValue {
        ComponentValue::map([(
            "style",
            ComponentValue::map([
                ("position_mode", ComponentValue::String(position.to_owned())),
                (
                    "flex_direction",
                    ComponentValue::String("column".to_owned()),
                ),
                ("align_items", ComponentValue::String("stretch".to_owned())),
                (
                    "justify_content",
                    ComponentValue::String("start".to_owned()),
                ),
                ("gap", ComponentValue::f64(0.0).unwrap()),
                (
                    "padding",
                    ComponentValue::map([
                        ("left", ComponentValue::f64(0.0).unwrap()),
                        ("right", ComponentValue::f64(0.0).unwrap()),
                        ("top", ComponentValue::f64(0.0).unwrap()),
                        ("bottom", ComponentValue::f64(0.0).unwrap()),
                    ]),
                ),
                ("flex_grow", ComponentValue::f64(0.0).unwrap()),
                ("flex_shrink", ComponentValue::f64(1.0).unwrap()),
                ("width", dimension_value(width.0, width.1)),
                ("height", dimension_value(height.0, height.1)),
                ("left", dimension_value("px", 10.0)),
                ("top", dimension_value("px", 20.0)),
                ("z_index", ComponentValue::I64(0)),
                ("visible", ComponentValue::Bool(true)),
                ("overflow", ComponentValue::String("visible".to_owned())),
                ("stacking_context", ComponentValue::Bool(false)),
            ]),
        )])
    }

    fn dimension_value(kind: &str, value: f64) -> ComponentValue {
        ComponentValue::map([
            ("kind", ComponentValue::String(kind.to_owned())),
            ("value", ComponentValue::f64(value).unwrap()),
        ])
    }

    fn ui_prefab(position: &str, width: (&str, f64)) -> PrefabDocument {
        let root = id("root");
        let button = id("button");
        PrefabDocument::new(
            [root.clone()],
            [
                SceneEntityRecord::new(root.clone())
                    .with_children([button.clone()])
                    .with_component(
                        UiAuthoringComponentKind::Root.component_type(),
                        component(ComponentValue::map([(
                            "layer",
                            ComponentValue::String("base".to_owned()),
                        )])),
                    )
                    .with_component(
                        UiAuthoringComponentKind::Node.component_type(),
                        component(node_value("flow", ("percent", 1.0), ("percent", 1.0))),
                    ),
                SceneEntityRecord::new(button.clone())
                    .with_component(
                        UiAuthoringComponentKind::Node.component_type(),
                        component(node_value(position, width, ("px", 48.0))),
                    )
                    .with_component(
                        UiAuthoringComponentKind::Button.component_type(),
                        component(ComponentValue::map([
                            ("label", ComponentValue::String("Retry".to_owned())),
                            (
                                "action",
                                ComponentValue::String("reference-game.retry".to_owned()),
                            ),
                            ("enabled", ComponentValue::Bool(true)),
                            ("logical_text_size", ComponentValue::f64(20.0).unwrap()),
                        ])),
                    ),
            ],
        )
    }

    #[test]
    fn projection_owns_stable_hierarchy_fields_and_direct_geometry_admission() {
        let projection = project_prefab_authoring(&ui_prefab("absolute", ("px", 180.0)))
            .expect("valid UI Prefab must project");
        assert_eq!(projection.hierarchy.len(), 2);
        assert_eq!(projection.source_map.len(), 2);
        let button = projection
            .hierarchy
            .iter()
            .find(|node| node.entity().as_str() == "button")
            .expect("button node must exist");
        assert_eq!(button.parent().map(SceneEntityId::as_str), Some("root"));
        assert_eq!(
            button.gizmos().move_node(),
            UiDirectManipulationCapability::Editable
        );
        assert_eq!(
            button.gizmos().resize_node(),
            UiDirectManipulationCapability::Editable
        );
        assert!(projection.fields.iter().any(|field| {
            field.key().entity().as_str() == "button"
                && field.key().primary_field().as_str() == "style.width.kind"
                && matches!(field.kind(), UiAuthoringFieldKind::Dimension)
        }));
        assert!(matches!(
            projection.canvas(),
            UiAuthoringCanvasProjection::Unavailable(
                UiAuthoringCanvasUnavailableReason::PresentationNotCompiled
            )
        ));
    }

    #[test]
    fn composite_dimension_edit_lowers_to_one_atomic_patch() {
        let projection = project_prefab_authoring(&ui_prefab("absolute", ("px", 180.0)))
            .expect("valid UI Prefab must project");
        let width = projection
            .fields()
            .iter()
            .find(|field| {
                field.key().entity().as_str() == "button"
                    && field.key().primary_field().as_str() == "style.width.kind"
            })
            .expect("width descriptor must exist");
        let patch = width
            .lower_edit(UiAuthoringFieldValue::Dimension {
                kind: UiAuthoringDimensionKind::Pixels,
                value: 240.0,
            })
            .expect("valid width must lower");
        assert_eq!(patch.operations.len(), 2);
        assert!(patch.operations.iter().all(|operation| matches!(
            operation,
            ScenePatchOperation::SetField { entity, component, .. }
                if entity.as_str() == "button"
                    && component.as_str() == "nara.ui.UiNode"
        )));
    }

    #[test]
    fn descriptor_validation_rejects_partial_and_noncanonical_edits() {
        let projection = project_prefab_authoring(&ui_prefab("absolute", ("px", 180.0)))
            .expect("valid UI Prefab must project");
        let width = projection
            .fields()
            .iter()
            .find(|field| {
                field.key().entity().as_str() == "button"
                    && field.key().primary_field().as_str() == "style.width.kind"
            })
            .expect("width descriptor must exist");
        let canonical = width
            .lower_edit(UiAuthoringFieldValue::Dimension {
                kind: UiAuthoringDimensionKind::Pixels,
                value: 240.0,
            })
            .unwrap();
        assert_eq!(width.commit_policy(), UiAuthoringCommitPolicy::Discrete);
        assert_eq!(width.validate_patch(&canonical), Ok(()));
        assert_eq!(
            projection.validate_field_patch(
                width.key(),
                UiAuthoringCommitPolicy::Discrete,
                &canonical,
            ),
            Ok(())
        );
        assert_eq!(
            projection.validate_field_patch(
                width.key(),
                UiAuthoringCommitPolicy::Continuous,
                &canonical,
            ),
            Err(UiAuthoringEditError::CommitPolicyMismatch)
        );

        let partial = ScenePatchDocument::new([canonical.operations[0].clone()]);
        assert_eq!(
            width.validate_patch(&partial),
            Err(UiAuthoringEditError::DescriptorMismatch)
        );

        let mut out_of_range = canonical;
        let ScenePatchOperation::SetField { value, .. } = &mut out_of_range.operations[1] else {
            panic!("dimension value must be a field write");
        };
        *value = ComponentValue::f64(-1.0).unwrap();
        assert_eq!(
            width.validate_patch(&out_of_range),
            Err(UiAuthoringEditError::OutOfRange)
        );

        let action = projection
            .fields()
            .iter()
            .find(|field| {
                field.key().entity().as_str() == "button"
                    && field.key().primary_field().as_str() == "action"
            })
            .expect("button action descriptor must exist");
        let invalid_action = action
            .lower_edit(UiAuthoringFieldValue::Action(String::new()))
            .expect("field shape alone cannot validate an action identity");
        assert_eq!(
            projection.validate_field_patch(
                action.key(),
                UiAuthoringCommitPolicy::Discrete,
                &invalid_action,
            ),
            Err(UiAuthoringEditError::InvalidComponentValue)
        );
    }

    #[test]
    fn direct_move_lowers_and_revalidates_one_canonical_two_axis_patch() {
        let projection = project_prefab_authoring(&ui_prefab("absolute", ("px", 180.0)))
            .expect("valid UI Prefab must project");
        let edit = UiAuthoringDirectEdit::MoveBy {
            entity: id("button"),
            delta: [22.0, 44.0],
        };
        let (key, patch) = projection
            .lower_direct_edit(&edit)
            .expect("absolute pixel position must admit direct move");

        assert_eq!(key.primary_field().as_str(), "style.left.kind");
        assert_eq!(patch.operations.len(), 4);
        assert!(matches!(
            &patch.operations[1],
            ScenePatchOperation::SetField { field, value, .. }
                if field.as_str() == "style.left.value" && value.as_f64() == Some(32.0)
        ));
        assert!(matches!(
            &patch.operations[3],
            ScenePatchOperation::SetField { field, value, .. }
                if field.as_str() == "style.top.value" && value.as_f64() == Some(64.0)
        ));
        assert_eq!(projection.validate_direct_patch(&key, &patch), Ok(()));
    }

    #[test]
    fn representative_typed_descriptors_share_stable_schema_identity() {
        let mut prefab = ui_prefab("absolute", ("px", 180.0));
        let root = prefab
            .entities
            .iter_mut()
            .find(|entity| entity.id.as_str() == "root")
            .unwrap();
        root.components.insert(
            UiAuthoringComponentKind::Theme.component_type(),
            component(ComponentValue::map([
                ("revision", ComponentValue::U64(1)),
                (
                    "palette",
                    ComponentValue::map([(
                        "accent",
                        ComponentValue::map([
                            ("red", ComponentValue::f64(0.1).unwrap()),
                            ("green", ComponentValue::f64(0.2).unwrap()),
                            ("blue", ComponentValue::f64(0.3).unwrap()),
                            ("alpha", ComponentValue::f64(1.0).unwrap()),
                        ]),
                    )]),
                ),
            ])),
        );

        let projection = project_prefab_authoring(&prefab).unwrap();
        let descriptor = |component: &str, field: &str| {
            projection
                .fields()
                .iter()
                .find(|descriptor| {
                    descriptor.key().component().as_str() == component
                        && descriptor.key().primary_field().as_str() == field
                })
                .unwrap()
        };
        assert!(matches!(
            descriptor("nara.ui.UiRoot", "layer").kind(),
            UiAuthoringFieldKind::Enum { .. }
        ));
        assert!(matches!(
            descriptor("nara.ui.UiButton", "label").kind(),
            UiAuthoringFieldKind::Text { multiline: false }
        ));
        assert!(matches!(
            descriptor("nara.ui.UiButton", "action").kind(),
            UiAuthoringFieldKind::Action
        ));
        let color = descriptor("nara.ui.UiTheme", "palette.accent.red");
        assert!(matches!(color.kind(), UiAuthoringFieldKind::Color));
        assert_eq!(
            color
                .lower_edit(UiAuthoringFieldValue::Color {
                    red: 0.2,
                    green: 0.3,
                    blue: 0.4,
                    alpha: 1.0,
                })
                .unwrap()
                .operations
                .len(),
            4
        );
    }

    #[test]
    fn ui_root_without_ui_node_rejects_instead_of_publishing_an_empty_projection() {
        let root = id("root");
        let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(root)
            .with_component(
                UiAuthoringComponentKind::Root.component_type(),
                component(ComponentValue::map([(
                    "layer",
                    ComponentValue::String("base".to_owned()),
                )])),
            )]);
        assert_eq!(
            project_prefab_authoring(&prefab),
            Err(UiAuthoringProjectionError::UiRootMissingNode)
        );
    }

    #[test]
    fn component_version_mismatch_is_visible_but_read_only() {
        let mut prefab = ui_prefab("absolute", ("px", 180.0));
        let button = prefab
            .entities
            .iter_mut()
            .find(|entity| entity.id.as_str() == "button")
            .unwrap();
        button
            .components
            .get_mut(&UiAuthoringComponentKind::Button.component_type())
            .unwrap()
            .version = ComponentSchemaVersion::new(2).unwrap();

        let projection = project_prefab_authoring(&prefab).unwrap();
        let label = projection
            .fields()
            .iter()
            .find(|field| {
                field.key().component().as_str() == "nara.ui.UiButton"
                    && field.key().primary_field().as_str() == "label"
            })
            .unwrap();
        assert_eq!(
            label.editability(),
            UiAuthoringEditability::ReadOnly(UiAuthoringReadOnlyReason::ComponentVersionMismatch)
        );
        assert_eq!(
            label.lower_edit(UiAuthoringFieldValue::Text("Changed".to_owned())),
            Err(UiAuthoringEditError::ReadOnly(
                UiAuthoringReadOnlyReason::ComponentVersionMismatch
            ))
        );
    }

    #[test]
    fn flow_and_percent_geometry_never_expose_direct_handles() {
        let flow = project_prefab_authoring(&ui_prefab("flow", ("px", 180.0))).unwrap();
        let flow_button = &flow.hierarchy()[1];
        assert_eq!(
            flow_button.gizmos().move_node(),
            UiDirectManipulationCapability::ReadOnly(
                UiDirectManipulationReadOnlyReason::FlowManaged
            )
        );

        let percent = project_prefab_authoring(&ui_prefab("absolute", ("percent", 0.5))).unwrap();
        assert_eq!(
            percent.hierarchy()[1].gizmos().resize_node(),
            UiDirectManipulationCapability::ReadOnly(
                UiDirectManipulationReadOnlyReason::PercentDimension
            )
        );
        assert_eq!(
            percent.lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
                entity: id("button"),
                delta: [60.0, 24.0],
            }),
            Err(UiAuthoringEditError::DirectManipulationReadOnly(
                UiDirectManipulationReadOnlyReason::PercentDimension
            ))
        );
    }
}
