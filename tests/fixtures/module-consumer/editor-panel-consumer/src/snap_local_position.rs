//! One-entity, source-local Transform2d snapping through existing authoring owners.
//!
//! This source-built example is not a world-space tool or an installable-tool protocol.

use nara::{
    reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue,
    },
    scene::{AuthoringPropertyEdit, SceneEntityId},
};
use nara_editor::{
    EditorNativeComposition, ImguiEditorPanel, ImguiEditorPanelContext, PanelActionDispatch,
    PanelSubmissionId,
    workspace::{
        EditorPanelDescriptor, EditorPanelModel, EditorPanelMultiplicity, EditorPanelPlacement,
        EditorPanelRegistrationError,
    },
};
use nara_tooling::{
    EditorActionOutcome, EditorDocumentId, EditorDocumentKind, EditorDocumentQueryBasis,
    EditorDocumentQueryRejection, EditorPropertyObservation, EditorPropertyRead,
    EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
    PreparedEditorDocumentEdit,
};

pub const DEFAULT_GRID: f64 = 16.0;
pub const PANEL_KIND: &str = "consumer.snap-local-position";
const TRANSFORM: &str = "nara.transform.Transform2d";
const TRANSLATION_ROOT_FIELD_IDS: [&str; 2] = ["translation.x", "translation.y"];

/// Tool policy refusals, separate from source/query/preparation/dispatch errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapRejection {
    InvalidGrid,
    NonFinitePosition,
    PositionOutOfRange,
    ResultOutOfRange,
    ObservationMismatch,
    ValueUnavailable,
}

impl std::fmt::Display for SnapRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidGrid => "Grid spacing must be a positive finite number",
            Self::NonFinitePosition => "Both local coordinates must be finite",
            Self::PositionOutOfRange => {
                "Local position exceeds the Transform2d finite f32 decode range"
            }
            Self::ResultOutOfRange => {
                "Snapped position exceeds the Transform2d finite f32 decode range"
            }
            Self::ObservationMismatch => {
                "Preview requires coherent X/Y observations at the exact captured basis"
            }
            Self::ValueUnavailable => {
                "Both Transform2d translation fields must be present F64 source values"
            }
        })
    }
}
impl std::error::Error for SnapRejection {}

/// Nearest local-zero grid point, with exact binary distance ties away from zero.
///
/// Source values stay f64. The range check is the real Transform codec's finite f32
/// range, not an extra coordinate cap or an implicit f32 quantization.
pub fn snap_local_position(position: [f64; 2], grid: f64) -> Result<[f64; 2], SnapRejection> {
    if !grid.is_finite() || grid <= 0.0 {
        return Err(SnapRejection::InvalidGrid);
    }
    if position.iter().any(|value| !value.is_finite()) {
        return Err(SnapRejection::NonFinitePosition);
    }
    let max = f64::from(f32::MAX);
    if position.iter().any(|value| value.abs() > max) {
        return Err(SnapRejection::PositionOutOfRange);
    }
    let result = position.map(|value| {
        let magnitude = value.abs();
        // Do not form value/grid: legal subnormal spacing can overflow that quotient.
        let remainder = magnitude % grid;
        if remainder == 0.0 {
            return value;
        }
        let up = grid - remainder;
        let snapped = if remainder < up {
            magnitude - remainder
        } else {
            magnitude + up
        };
        if snapped == 0.0 {
            0.0
        } else {
            snapped.copysign(value)
        }
    });
    if result
        .iter()
        .any(|value| !value.is_finite() || value.abs() > max)
    {
        return Err(SnapRejection::ResultOutOfRange);
    }
    Ok(result)
}

/// Exactly two root-scalar reads at one basis; no complete Inspector/component fallback.
pub fn read_requests(
    basis: &EditorDocumentQueryBasis,
    entity: &SceneEntityId,
) -> Result<[EditorPropertyRead; 2], EditorDocumentQueryRejection> {
    let request =
        |root_field_id: &str| -> Result<EditorPropertyRead, EditorDocumentQueryRejection> {
            let target = EditorPropertyTarget {
                basis: basis.clone(),
                entity: entity.clone(),
                component: ComponentTypeId::new(TRANSFORM),
                property: ComponentPropertyPath::new(ComponentFieldId::new(root_field_id)),
            };
            let selector_bytes = target
                .property
                .logical_bytes()
                .map_err(|_| EditorDocumentQueryRejection::InvalidPropertyReadLimits)?;
            let metadata = target
                .entity
                .as_str()
                .len()
                .saturating_add(target.component.as_str().len())
                .saturating_add(selector_bytes);
            let limits =
                EditorPropertyReadLimits::new(metadata, 1, 8)?.with_scan_limits(64, 4096)?;
            Ok(EditorPropertyRead { target, limits })
        };
    Ok([
        request(TRANSLATION_ROOT_FIELD_IDS[0])?,
        request(TRANSLATION_ROOT_FIELD_IDS[1])?,
    ])
}

#[derive(Debug, Clone, PartialEq)]
pub enum SnapPlan {
    NoChange,
    Change(SnapProposal),
}

/// Concrete intent only. The source owner prepares, validates and creates the inverse.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapProposal {
    basis: EditorDocumentQueryBasis,
    entity: SceneEntityId,
    grid: f64,
    before: [f64; 2],
    after: [f64; 2],
}

impl SnapProposal {
    #[must_use]
    pub fn basis(&self) -> &EditorDocumentQueryBasis {
        &self.basis
    }
    #[must_use]
    pub fn entity(&self) -> &SceneEntityId {
        &self.entity
    }
    #[must_use]
    pub fn grid(&self) -> f64 {
        self.grid
    }
    #[must_use]
    pub fn before(&self) -> [f64; 2] {
        self.before
    }
    #[must_use]
    pub fn after(&self) -> [f64; 2] {
        self.after
    }

    /// Only changed axes, in one owner transaction; signed zero alone is not an edit.
    #[must_use]
    pub fn property_edits(&self) -> Vec<AuthoringPropertyEdit> {
        TRANSLATION_ROOT_FIELD_IDS
            .iter()
            .enumerate()
            .filter(|(axis, _)| self.before[*axis] != self.after[*axis])
            .map(|(axis, field)| AuthoringPropertyEdit {
                entity: self.entity.clone(),
                component: ComponentTypeId::new(TRANSFORM),
                component_version: ComponentSchemaVersion::ONE,
                property: ComponentPropertyPath::new(ComponentFieldId::new(*field)),
                value: ComponentValue::f64(self.after[axis])
                    .expect("validated snap coordinates are finite"),
            })
            .collect()
    }
}

/// Checks observation coherence, not current liveness. A Session adapter must call
/// validate_authoring_query_basis with proposal.basis() before preparing its edits.
pub fn plan_snap_local_position(
    x: &EditorPropertyObservation,
    y: &EditorPropertyObservation,
    grid: f64,
) -> Result<SnapPlan, SnapRejection> {
    if x.target.basis != y.target.basis
        || x.target.entity != y.target.entity
        || [x, y]
            .into_iter()
            .zip(TRANSLATION_ROOT_FIELD_IDS)
            .any(|(observed, field)| {
                observed.target.component.as_str() != TRANSFORM
                    || observed.target.property.root.as_str() != field
                    || !observed.target.property.steps.is_empty()
            })
    {
        return Err(SnapRejection::ObservationMismatch);
    }
    let scalar = |observed: &EditorPropertyObservation| match &observed.value {
        EditorPropertyValue::Present(value) => {
            value.as_f64().ok_or(SnapRejection::ValueUnavailable)
        }
        _ => Err(SnapRejection::ValueUnavailable),
    };
    let before = [scalar(x)?, scalar(y)?];
    let after = snap_local_position(before, grid)?;
    if before == after {
        return Ok(SnapPlan::NoChange);
    }
    Ok(SnapPlan::Change(SnapProposal {
        basis: x.target.basis.clone(),
        entity: x.target.entity.clone(),
        grid,
        before,
        after,
    }))
}

pub fn install(
    composition: &mut EditorNativeComposition,
) -> Result<(), EditorPanelRegistrationError> {
    composition.register_panel(
        EditorPanelDescriptor::new(
            PANEL_KIND,
            "Snap Local Position",
            EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Right,
            true,
        ),
        SnapPanel::default,
    )
}

/// Toolkit-local input, one unpublished source candidate, and exact originating dispatch.
pub struct SnapPanel {
    grid_input: String,
    target: Option<(EditorDocumentId, SceneEntityId)>,
    preview: Option<(SnapProposal, PreparedEditorDocumentEdit)>,
    pending: Option<PanelSubmissionId>,
    completion: Option<(PanelSubmissionId, PanelActionDispatch)>,
    status: String,
}

impl Default for SnapPanel {
    fn default() -> Self {
        Self {
            grid_input: DEFAULT_GRID.to_string(),
            target: None,
            preview: None,
            pending: None,
            completion: None,
            status: String::new(),
        }
    }
}

impl SnapPanel {
    /// Read-only access to the actual retained owner preview, never a reconstructed patch.
    #[must_use]
    pub fn preview(&self) -> Option<(&SnapProposal, &PreparedEditorDocumentEdit)> {
        self.preview
            .as_ref()
            .map(|(proposal, candidate)| (proposal, candidate))
    }
    #[must_use]
    pub fn pending(&self) -> Option<PanelSubmissionId> {
        self.pending
    }
    /// The last observed dispatch belongs to its exact submission, not a later frame.
    #[must_use]
    pub fn completion(&self) -> Option<&(PanelSubmissionId, PanelActionDispatch)> {
        self.completion.as_ref()
    }

    fn selected(model: EditorPanelModel<'_>) -> Option<(EditorDocumentId, SceneEntityId)> {
        let document = model.document()?;
        if document.kind != EditorDocumentKind::Scene {
            return None;
        }
        let selection = model.selection()?;
        if selection.len() != 1 {
            return None;
        }
        Some((document.document, selection.top_entity()?.clone()))
    }

    fn prepare_preview(&mut self, context: &ImguiEditorPanelContext<'_, '_>) {
        self.preview = None;
        let grid = match self.grid_input.parse::<f64>() {
            Ok(grid) if grid.is_finite() && grid > 0.0 => grid,
            _ => {
                self.status = SnapRejection::InvalidGrid.to_string();
                return;
            }
        };
        let Some((document, entity)) = &self.target else {
            return;
        };
        let model = context.model();
        let Some(tab) = model.frame().workspace().document(*document) else {
            return;
        };
        let basis = EditorDocumentQueryBasis {
            document: *document,
            revision: tab.revision,
            schema: tab.authoring_schema_basis.clone(),
        };
        let requests = match read_requests(&basis, entity) {
            Ok(requests) => requests,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let [x, y] = requests;
        let x = match context.read_document_property(x) {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let y = match context.read_document_property(y) {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        match plan_snap_local_position(&x, &y, grid) {
            Ok(SnapPlan::NoChange) => self.status = "No representable source change".into(),
            Ok(SnapPlan::Change(proposal)) => {
                match context
                    .prepare_authoring_property_edit(proposal.basis(), &proposal.property_edits())
                {
                    Ok(candidate) => {
                        self.preview = Some((proposal, candidate));
                        self.status = "Preview only; source is unchanged".into();
                    }
                    Err(error) => self.status = error.to_string(),
                }
            }
            Err(error) => self.status = error.to_string(),
        }
    }
}

impl ImguiEditorPanel for SnapPanel {
    fn show(&mut self, ui: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        let selected = Self::selected(context.model());
        if self.target != selected {
            self.target = selected;
            self.preview = None;
            self.status = "Selection changed; Preview again before applying".into();
        }
        if let Some((_, entity)) = &self.target {
            if let Some(document) = context.model().document() {
                ui.text(&document.title);
            }
            ui.text(format!("Source entity: {}", entity.as_str()));
        } else {
            ui.text("Select one Scene entity with Transform 2D");
        }
        ui.text_wrapped(
            "Source-local coordinates; origin is local zero. Parent transforms are not converted.",
        );
        let input_changed = ui.with_disabled_if(self.pending.is_some(), || {
            ui.input_text("Grid spacing (local units)", &mut self.grid_input)
                .build()
        });
        if input_changed {
            self.preview = None;
            self.status = "Grid changed; Preview again before applying".into();
        }
        ui.text_wrapped(&self.status);
        if let Some((id, result)) = &self.completion {
            ui.text(format!("Submission {id:?}"));
            if ui.collapsing_header("Dispatch details", dear_imgui_rs::TreeNodeFlags::empty()) {
                ui.text_wrapped(format!("{result:#?}"));
            }
        }
        if let Some((proposal, candidate)) = &self.preview {
            for (axis, root_field_id) in TRANSLATION_ROOT_FIELD_IDS.iter().enumerate() {
                ui.text(format!(
                    "{}: {} -> {}",
                    root_field_id, proposal.before[axis], proposal.after[axis]
                ));
            }
            if ui.collapsing_header(
                "Owner candidate and inverse",
                dear_imgui_rs::TreeNodeFlags::empty(),
            ) {
                ui.text_wrapped(format!("Forward: {:#?}", candidate.patch()));
                ui.text_wrapped(format!("Inverse: {:#?}", candidate.inverse()));
            }
        }
        let busy = self.pending.is_some();
        if ui.with_disabled_if(busy || self.target.is_none(), || {
            ui.button_with_size("Preview", [80.0, 0.0])
        }) {
            self.prepare_preview(context);
        }
        ui.same_line();
        if ui.with_disabled_if(busy || self.preview.is_none(), || {
            ui.button_with_size("Apply", [80.0, 0.0])
        }) && let Some((proposal, candidate)) = self.preview.take()
        {
            match context.commit_authoring_edit(candidate) {
                Ok(id) => {
                    self.pending = Some(id);
                    self.completion = None;
                    self.status = "Queued; awaiting exact dispatch outcome".into();
                }
                Err((error, candidate)) => {
                    self.preview = Some((proposal, candidate));
                    self.status = error.to_string();
                }
            }
        }
        ui.same_line();
        if ui.with_disabled_if(busy || self.preview.is_none(), || {
            ui.button_with_size("Cancel", [80.0, 0.0])
        }) {
            self.preview = None;
            self.status = "Preview cancelled; source is unchanged".into();
        }
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        if self.pending != Some(id) {
            return;
        }
        self.pending = None;
        self.status = match &result {
            PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
                if report.applied =>
            {
                "Snap committed"
            }
            PanelActionDispatch::Dispatched(_) => "Snap dispatched but rejected; Preview again",
            PanelActionDispatch::NotDispatched(_) => "Snap was not dispatched; Preview again",
        }
        .into();
        self.completion = Some((id, result));
    }
}
