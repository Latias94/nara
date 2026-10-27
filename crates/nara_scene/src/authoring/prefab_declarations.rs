//! Bounded source declaration observations and exact source-owned Revert preparation.

use std::{error::Error, fmt};

use super::{
    PrefabAuthoringSession, PreparedPrefabAuthoringEdit, PreparedSceneAuthoringEdit,
    SceneAuthoringRevision, SceneAuthoringSession, history_error_report,
};
use crate::{
    AuthoringSchemaBasis, PrefabComponentIntent, PrefabInstance, PrefabInstancePath,
    PrefabInstancePathSegment, PrefabOverlay, PrefabOverlaySlot, PrefabRevert, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
    prefab::{PrefabOverlayCostLimits, prefab_instance_cost},
};

/// Inclusive logical limits for source declaration observation and Revert lowering.
///
/// Nodes include raw declaration/target/value nodes and copied location headers/steps.
/// Bytes include the raw-storage cost plus each copied anchor's UTF-8 bytes and nine bytes
/// per path step (one discriminant and one logical 64-bit index). Depth bounds both semantic
/// storage depth and path steps. These are logical work/content limits, not allocator or RSS
/// measurements. Existing preparation separately admits the complete document and history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabDeclarationLimits {
    max_nodes: usize,
    max_logical_bytes: usize,
    max_depth: usize,
}

impl PrefabDeclarationLimits {
    pub const MAX_NODES: usize = 65_536;
    pub const MAX_LOGICAL_BYTES: usize = 4 * 1024 * 1024;
    pub const MAX_DEPTH: usize = 64;

    /// Rejects zero or limits above the corresponding public ceilings.
    #[must_use]
    pub const fn new(max_nodes: usize, max_logical_bytes: usize, max_depth: usize) -> Option<Self> {
        if max_nodes == 0
            || max_nodes > Self::MAX_NODES
            || max_logical_bytes == 0
            || max_logical_bytes > Self::MAX_LOGICAL_BYTES
            || max_depth == 0
            || max_depth > Self::MAX_DEPTH
        {
            return None;
        }
        Some(Self {
            max_nodes,
            max_logical_bytes,
            max_depth,
        })
    }

    #[must_use]
    pub const fn max_nodes(self) -> usize {
        self.max_nodes
    }

    #[must_use]
    pub const fn max_logical_bytes(self) -> usize {
        self.max_logical_bytes
    }

    #[must_use]
    pub const fn max_depth(self) -> usize {
        self.max_depth
    }
}

impl Default for PrefabDeclarationLimits {
    fn default() -> Self {
        Self {
            max_nodes: 4_096,
            max_logical_bytes: 256 * 1024,
            max_depth: Self::MAX_DEPTH,
        }
    }
}

/// Refusal without copying unbounded caller selectors or source payloads into an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabDeclarationError {
    NotAdmitted,
    AnchorNotFound,
    PathNotFound { step: usize },
    InstanceAbsent,
    OverlayAbsent,
    SlotNotFound,
    BudgetExceeded,
}

impl fmt::Display for PrefabDeclarationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.summary())
    }
}

impl Error for PrefabDeclarationError {}

impl PrefabDeclarationError {
    const fn summary(self) -> &'static str {
        match self {
            Self::NotAdmitted => "source has no authoring admission",
            Self::AnchorNotFound => "direct source anchor was not found",
            Self::PathNotFound { .. } => "nested source declaration path was not found",
            Self::InstanceAbsent => "the selected optional instance declaration is absent",
            Self::OverlayAbsent => "the selected instance has no overlay",
            Self::SlotNotFound => "the selected overlay intent was not found",
            Self::BudgetExceeded => "source declaration work exceeds its logical limits",
        }
    }

    fn report(self) -> ScenePatchReport {
        let code = match self {
            Self::NotAdmitted => "scene.authoring-prefab-not-admitted",
            Self::AnchorNotFound => "scene.authoring-prefab-anchor-not-found",
            Self::PathNotFound { .. } => "scene.authoring-prefab-path-not-found",
            Self::InstanceAbsent => "scene.authoring-prefab-instance-absent",
            Self::OverlayAbsent => "scene.authoring-prefab-overlay-absent",
            Self::SlotNotFound => "scene.authoring-prefab-slot-not-found",
            Self::BudgetExceeded => "scene.authoring-prefab-budget-exceeded",
        };
        history_error_report(code, self.summary())
    }
}

/// Complete owned source observation, not activation or mutation authority.
///
/// Nested paths are in source declaration order: additions before instance claims at each
/// overlay. They include explicit `None` instance claims, exclude additions without an instance,
/// and do not repeat `path` itself. No inherited Prefab is resolved and no materialized ID is used.
/// Retained indices require this exact revision and Schema admission at later preparation.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefabInstanceObservation {
    pub revision: SceneAuthoringRevision,
    pub schema: AuthoringSchemaBasis,
    pub path: PrefabInstancePath,
    pub instance: Option<PrefabInstance>,
    pub nested_instances: Vec<PrefabInstancePath>,
}

impl SceneAuthoringSession {
    /// Observes a complete optional declaration after aggregate payload/location admission.
    pub fn observe_prefab_instance(
        &self,
        path: &PrefabInstancePath,
        limits: PrefabDeclarationLimits,
    ) -> Result<PrefabInstanceObservation, PrefabDeclarationError> {
        let schema = self
            .authoring_schema_basis()
            .ok_or(PrefabDeclarationError::NotAdmitted)?;
        observe(
            &self.document.entities,
            self.revision(),
            schema,
            path,
            limits,
        )
    }

    /// Lowers one source-owned Revert into the ordinary exact transaction and inverse.
    ///
    /// The caller must also validate any retained observation's exact Schema basis.
    /// This prepares current source only; it neither resolves a base nor edits another source.
    pub fn prepare_prefab_revert(
        &self,
        base_revision: SceneAuthoringRevision,
        path: &PrefabInstancePath,
        action: PrefabRevert,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedSceneAuthoringEdit, ScenePatchReport> {
        if base_revision != self.revision() {
            return Err(super::prepared::stale_basis());
        }
        if self.authority.published().is_none() {
            return Err(PrefabDeclarationError::NotAdmitted.report());
        }
        let patch = lower_revert(&self.document.entities, path, action, limits)
            .map_err(PrefabDeclarationError::report)?;
        self.prepare_authoring_edit(base_revision, &patch)
    }
}

impl PrefabAuthoringSession {
    /// Uses the same bounded raw observation without changing nominal Prefab ownership.
    pub fn observe_prefab_instance(
        &self,
        path: &PrefabInstancePath,
        limits: PrefabDeclarationLimits,
    ) -> Result<PrefabInstanceObservation, PrefabDeclarationError> {
        let schema = self
            .authoring_schema_basis()
            .ok_or(PrefabDeclarationError::NotAdmitted)?;
        observe(
            &self.document.entities,
            self.revision(),
            schema,
            path,
            limits,
        )
    }

    /// Prepares one Revert through this Prefab's existing exact preparation/history owner.
    ///
    /// Retained observation Schema admission must be checked by the calling adapter.
    pub fn prepare_prefab_revert(
        &self,
        base_revision: SceneAuthoringRevision,
        path: &PrefabInstancePath,
        action: PrefabRevert,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedPrefabAuthoringEdit, ScenePatchReport> {
        if base_revision != self.revision() {
            return Err(super::prepared::stale_basis());
        }
        if self.authority.published().is_none() {
            return Err(PrefabDeclarationError::NotAdmitted.report());
        }
        let patch = lower_revert(&self.document.entities, path, action, limits)
            .map_err(PrefabDeclarationError::report)?;
        self.prepare_authoring_edit(base_revision, &patch)
    }
}

fn observe(
    entities: &[SceneEntityRecord],
    revision: SceneAuthoringRevision,
    schema: AuthoringSchemaBasis,
    path: &PrefabInstancePath,
    limits: PrefabDeclarationLimits,
) -> Result<PrefabInstanceObservation, PrefabDeclarationError> {
    let mut budget = DeclarationBudget::new(limits);
    budget.location(path.anchor.as_str(), &path.steps)?;
    let root = anchor_instance(entities, path)?;
    let instance = instance_at(root, &path.steps)?;
    if let Some(instance) = instance {
        budget.instance(instance, 1)?;
    }
    let mut steps = path.steps.clone();
    // This traversal reads declaration containers only. Raw payload accounting remains with
    // the shared cost owner; all output locations are admitted before their paths are copied.
    visit_nested(instance, &mut steps, &mut |steps| {
        budget.location(path.anchor.as_str(), steps)
    })?;
    let mut nested_instances = Vec::new();
    visit_nested(instance, &mut steps, &mut |steps| {
        nested_instances.push(PrefabInstancePath {
            anchor: path.anchor.clone(),
            steps: steps.to_vec(),
        });
        Ok(())
    })?;
    Ok(PrefabInstanceObservation {
        revision,
        schema,
        path: path.clone(),
        instance: instance.clone(),
        nested_instances,
    })
}

fn lower_revert(
    entities: &[SceneEntityRecord],
    path: &PrefabInstancePath,
    action: PrefabRevert,
    limits: PrefabDeclarationLimits,
) -> Result<ScenePatchDocument, PrefabDeclarationError> {
    let mut budget = DeclarationBudget::new(limits);
    budget.location(path.anchor.as_str(), &path.steps)?;
    let root = anchor_instance(entities, path)?;
    let original = root
        .as_ref()
        .ok_or(PrefabDeclarationError::InstanceAbsent)?;
    // Lowering owns complete expected/value snapshots even for a deeply nested Revert.
    // Charge two unchanged roots conservatively before cloning either; removal cannot grow one.
    budget.instance(original, 2)?;
    let selected = instance_at(root, &path.steps)?
        .as_ref()
        .ok_or(PrefabDeclarationError::InstanceAbsent)?;
    let overlay = selected
        .overrides
        .as_ref()
        .ok_or(PrefabDeclarationError::OverlayAbsent)?;
    validate_revert(overlay, action)?;
    let mut value = root.clone();
    let selected = instance_at_mut(&mut value, &path.steps)?
        .as_mut()
        .ok_or(PrefabDeclarationError::InstanceAbsent)?;
    match action {
        PrefabRevert::Overlay => selected.overrides = None,
        PrefabRevert::Intent(slot) => revert_intent(
            selected
                .overrides
                .as_mut()
                .ok_or(PrefabDeclarationError::OverlayAbsent)?,
            slot,
        ),
    }
    Ok(ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: path.anchor.clone(),
            expected: root.clone(),
            value,
        },
    ]))
}

fn anchor_instance<'a>(
    entities: &'a [SceneEntityRecord],
    path: &PrefabInstancePath,
) -> Result<&'a Option<PrefabInstance>, PrefabDeclarationError> {
    // Published Scene and Prefab records retain the same canonical ID ordering as entity_view.
    let index = entities
        .binary_search_by(|entity| entity.id.cmp(&path.anchor))
        .map_err(|_| PrefabDeclarationError::AnchorNotFound)?;
    Ok(&entities[index].prefab)
}

fn instance_at<'a>(
    root: &'a Option<PrefabInstance>,
    steps: &[PrefabInstancePathSegment],
) -> Result<&'a Option<PrefabInstance>, PrefabDeclarationError> {
    let mut instance = root;
    for (step, segment) in steps.iter().enumerate() {
        let missing = || PrefabDeclarationError::PathNotFound { step };
        let overlay = instance
            .as_ref()
            .and_then(|instance| instance.overrides.as_ref())
            .ok_or_else(missing)?;
        instance = match segment {
            PrefabInstancePathSegment::Addition { index } => {
                &overlay.additions.get(*index).ok_or_else(missing)?.prefab
            }
            PrefabInstancePathSegment::InstanceClaim { index } => {
                &overlay.instances.get(*index).ok_or_else(missing)?.value
            }
        };
    }
    Ok(instance)
}

fn instance_at_mut<'a>(
    root: &'a mut Option<PrefabInstance>,
    steps: &[PrefabInstancePathSegment],
) -> Result<&'a mut Option<PrefabInstance>, PrefabDeclarationError> {
    let mut instance = root;
    for (step, segment) in steps.iter().enumerate() {
        let missing = || PrefabDeclarationError::PathNotFound { step };
        let overlay = instance
            .as_mut()
            .and_then(|instance| instance.overrides.as_mut())
            .ok_or_else(missing)?;
        instance = match segment {
            PrefabInstancePathSegment::Addition { index } => {
                &mut overlay
                    .additions
                    .get_mut(*index)
                    .ok_or_else(missing)?
                    .prefab
            }
            PrefabInstancePathSegment::InstanceClaim { index } => {
                &mut overlay.instances.get_mut(*index).ok_or_else(missing)?.value
            }
        };
    }
    Ok(instance)
}

fn visit_nested(
    instance: &Option<PrefabInstance>,
    steps: &mut Vec<PrefabInstancePathSegment>,
    visitor: &mut impl FnMut(&[PrefabInstancePathSegment]) -> Result<(), PrefabDeclarationError>,
) -> Result<(), PrefabDeclarationError> {
    let Some(overlay) = instance
        .as_ref()
        .and_then(|instance| instance.overrides.as_ref())
    else {
        return Ok(());
    };
    for (index, addition) in overlay.additions.iter().enumerate() {
        if addition.prefab.is_some() {
            steps.push(PrefabInstancePathSegment::Addition { index });
            visitor(steps)?;
            visit_nested(&addition.prefab, steps, visitor)?;
            let _ = steps.pop();
        }
    }
    for (index, claim) in overlay.instances.iter().enumerate() {
        steps.push(PrefabInstancePathSegment::InstanceClaim { index });
        visitor(steps)?;
        visit_nested(&claim.value, steps, visitor)?;
        let _ = steps.pop();
    }
    Ok(())
}

fn validate_revert(
    overlay: &PrefabOverlay,
    action: PrefabRevert,
) -> Result<(), PrefabDeclarationError> {
    let found = match action {
        PrefabRevert::Overlay => true,
        PrefabRevert::Intent(slot) => match slot {
            PrefabOverlaySlot::Addition { index } => index < overlay.additions.len(),
            PrefabOverlaySlot::InstanceClaim { index } => index < overlay.instances.len(),
            PrefabOverlaySlot::Component { index } => index < overlay.components.len(),
            PrefabOverlaySlot::Property {
                component_index,
                claim_index,
            } => {
                matches!(overlay.components.get(component_index),
                    Some(PrefabComponentIntent::Properties { claims, .. })
                    if claim_index < claims.len())
            }
            PrefabOverlaySlot::Placement { index } => index < overlay.placements.len(),
            PrefabOverlaySlot::Suppression { index } => index < overlay.suppressions.len(),
        },
    };
    found
        .then_some(())
        .ok_or(PrefabDeclarationError::SlotNotFound)
}

fn revert_intent(overlay: &mut PrefabOverlay, slot: PrefabOverlaySlot) {
    match slot {
        PrefabOverlaySlot::Addition { index } => {
            overlay.additions.remove(index);
        }
        PrefabOverlaySlot::InstanceClaim { index } => {
            overlay.instances.remove(index);
        }
        PrefabOverlaySlot::Component { index } => {
            overlay.components.remove(index);
        }
        PrefabOverlaySlot::Property {
            component_index,
            claim_index,
        } => {
            if let PrefabComponentIntent::Properties { claims, .. } =
                &mut overlay.components[component_index]
            {
                claims.remove(claim_index);
                if claims.is_empty() {
                    overlay.components.remove(component_index);
                }
            }
        }
        PrefabOverlaySlot::Placement { index } => {
            overlay.placements.remove(index);
        }
        PrefabOverlaySlot::Suppression { index } => {
            overlay.suppressions.remove(index);
        }
    }
}

struct DeclarationBudget {
    limits: PrefabDeclarationLimits,
    nodes: usize,
    bytes: usize,
}

impl DeclarationBudget {
    fn new(limits: PrefabDeclarationLimits) -> Self {
        Self {
            limits,
            nodes: 0,
            bytes: 0,
        }
    }

    fn location(
        &mut self,
        anchor: &str,
        steps: &[PrefabInstancePathSegment],
    ) -> Result<(), PrefabDeclarationError> {
        if steps.len() > self.limits.max_depth {
            return Err(PrefabDeclarationError::BudgetExceeded);
        }
        self.nodes = self.nodes.saturating_add(1).saturating_add(steps.len());
        self.bytes = self
            .bytes
            .saturating_add(anchor.len())
            .saturating_add(steps.len().saturating_mul(9));
        self.check()
    }

    fn instance(
        &mut self,
        instance: &PrefabInstance,
        copies: usize,
    ) -> Result<(), PrefabDeclarationError> {
        let nodes = self.limits.max_nodes.saturating_sub(self.nodes) / copies;
        let bytes = self.limits.max_logical_bytes.saturating_sub(self.bytes) / copies;
        let cost = prefab_instance_cost(
            instance,
            PrefabOverlayCostLimits {
                nodes,
                depth: self.limits.max_depth,
                declarations: nodes,
                entities: nodes,
                placements: nodes,
                components: nodes,
                prefab_instances: nodes,
                value_nodes: nodes,
                value_bytes: bytes,
                logical_bytes: bytes,
            },
        )
        .map_err(|_| PrefabDeclarationError::BudgetExceeded)?;
        self.nodes = self.nodes.saturating_add(cost.nodes.saturating_mul(copies));
        self.bytes = self
            .bytes
            .saturating_add(cost.logical_bytes().saturating_mul(copies));
        self.check()
    }

    fn check(&self) -> Result<(), PrefabDeclarationError> {
        if self.nodes > self.limits.max_nodes || self.bytes > self.limits.max_logical_bytes {
            Err(PrefabDeclarationError::BudgetExceeded)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
