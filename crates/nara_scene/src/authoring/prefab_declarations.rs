//! Bounded source declaration observations and exact source-owned edit preparation.

use std::{error::Error, fmt};

use nara_reflect::{ComponentSchemaVersion, ComponentTypeId};

use super::{
    PrefabAuthoringSession, PreparedPrefabAuthoringEdit, PreparedSceneAuthoringEdit,
    SceneAuthoringRevision, SceneAuthoringSession, history_error_report,
};
use crate::{
    AuthoringSchemaBasis, ObservedPrefabRevert, PrefabComponentIntent, PrefabEntityTarget,
    PrefabInstance, PrefabInstancePath, PrefabInstancePathSegment, PrefabOverlay,
    PrefabOverlaySlot, PrefabPropertyClaim, PrefabPropertyClaimRef, PrefabPropertyIntent,
    PrefabRevert, SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    ScenePatchReport,
    prefab::{PrefabOverlayCostLimits, prefab_instance_cost},
};

mod component_write;
mod property_write;

/// Inclusive logical limits for source declaration observation and edit lowering.
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
    ComponentClaimRequired,
    PropertyClaimRequired,
    ConflictingClaim,
    StaleObservedIntent,
    AlreadyWritten,
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
            Self::ComponentClaimRequired => "component Revert requires the observed declaration",
            Self::PropertyClaimRequired => "property Revert requires the observed declaration",
            Self::ConflictingClaim => "a same-layer property claim is an ancestor or descendant",
            Self::StaleObservedIntent => {
                "the observed declaration disagrees with current source intent"
            }
            Self::AlreadyWritten => "the selected declaration already has this exact value",
            Self::BudgetExceeded => "source declaration work exceeds its logical limits",
        }
    }

    /// Returns the diagnostic identity used when this refusal is reported by the source owner.
    #[must_use]
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::NotAdmitted => "scene.authoring-prefab-not-admitted",
            Self::AnchorNotFound => "scene.authoring-prefab-anchor-not-found",
            Self::PathNotFound { .. } => "scene.authoring-prefab-path-not-found",
            Self::InstanceAbsent => "scene.authoring-prefab-instance-absent",
            Self::OverlayAbsent => "scene.authoring-prefab-overlay-absent",
            Self::SlotNotFound => "scene.authoring-prefab-slot-not-found",
            Self::ComponentClaimRequired => "scene.authoring-prefab-component-claim-required",
            Self::PropertyClaimRequired => "scene.authoring-prefab-property-claim-required",
            Self::ConflictingClaim => "scene.authoring-prefab-conflicting-claim",
            Self::StaleObservedIntent => "scene.authoring-prefab-stale-observed-intent",
            Self::AlreadyWritten => "scene.authoring-prefab-already-written",
            Self::BudgetExceeded => "scene.authoring-prefab-budget-exceeded",
        }
    }

    fn report(self) -> ScenePatchReport {
        history_error_report(self.diagnostic_code(), self.summary())
    }
}

/// Complete owned source observation. Revert preparation revalidates its source and Schema.
///
/// Nested paths are in source declaration order: additions before instance claims at each
/// overlay. They include explicit `None` instance claims, exclude additions without an instance,
/// and do not repeat `path` itself. No inherited Prefab is resolved and no materialized ID is used.
/// Retained indices require this exact revision and Schema admission at later preparation.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefabInstanceObservation {
    revision: SceneAuthoringRevision,
    schema: AuthoringSchemaBasis,
    path: PrefabInstancePath,
    instance: Option<PrefabInstance>,
    nested_instances: Vec<PrefabInstancePath>,
}

impl PrefabInstanceObservation {
    #[must_use]
    pub const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub const fn schema(&self) -> &AuthoringSchemaBasis {
        &self.schema
    }

    #[must_use]
    pub const fn path(&self) -> &PrefabInstancePath {
        &self.path
    }

    #[must_use]
    pub const fn instance(&self) -> Option<&PrefabInstance> {
        self.instance.as_ref()
    }

    #[must_use]
    pub fn nested_instances(&self) -> &[PrefabInstancePath] {
        &self.nested_instances
    }

    /// Selects a stored declaration, or the entire overlay for `None`, from this observation.
    /// Index zero selects the observed instance and subsequent indices select nested instances
    /// in observation order. Preparation checks that the selected slot exists in the exact source.
    #[must_use]
    pub fn revert(
        &self,
        instance_index: usize,
        slot: Option<PrefabOverlaySlot>,
    ) -> Option<PrefabRevert<'_>> {
        let path = match instance_index.checked_sub(1) {
            Some(index) => self.nested_instances.get(index)?,
            None => &self.path,
        };
        Some(PrefabRevert::Observed(ObservedPrefabRevert {
            observation: self,
            path,
            slot,
        }))
    }
}

/// Borrowed property declarations from one stored overlay. Indices belong to this revision.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct PrefabPropertyClaims<'a> {
    components: &'a [PrefabComponentIntent],
    revision: Option<SceneAuthoringRevision>,
}

/// One component's stored property declarations, with their complete authored target.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct PrefabComponentPropertyClaims<'a> {
    pub target: &'a PrefabEntityTarget,
    pub component: &'a ComponentTypeId,
    pub version: ComponentSchemaVersion,
    pub claims: &'a [PrefabPropertyClaim],
}

impl<'a> PrefabPropertyClaims<'a> {
    #[must_use]
    pub const fn component_intent_count(self) -> usize {
        self.components.len()
    }

    /// Looks up a single component slot without walking or decoding its payload.
    #[must_use]
    pub fn component(self, index: usize) -> Option<PrefabComponentPropertyClaims<'a>> {
        let PrefabComponentIntent::Properties {
            target,
            component,
            version,
            claims,
        } = self.components.get(index)?
        else {
            return None;
        };
        Some(PrefabComponentPropertyClaims {
            target,
            component,
            version: *version,
            claims,
        })
    }

    /// Borrows the complete observed declaration for one property coordinate.
    #[must_use]
    pub fn claim_at(self, slot: PrefabOverlaySlot) -> Option<PrefabPropertyClaimRef<'a>> {
        let PrefabOverlaySlot::Property {
            component_index,
            claim_index,
        } = slot
        else {
            return None;
        };
        let component = self.component(component_index)?;
        Some(PrefabPropertyClaimRef {
            slot,
            target: component.target,
            component: component.component,
            version: component.version,
            claim: component.claims.get(claim_index)?,
            revision: self.revision,
        })
    }

    pub fn intent(
        self,
        slot: PrefabOverlaySlot,
    ) -> Result<&'a PrefabPropertyIntent, PrefabDeclarationError> {
        self.claim_at(slot)
            .map(|declaration| &declaration.claim().intent)
            .ok_or(PrefabDeclarationError::SlotNotFound)
    }
}

impl PrefabOverlay {
    /// Borrows a property declaration from this overlay observation.
    #[must_use]
    pub fn property_claim_at(&self, slot: PrefabOverlaySlot) -> Option<PrefabPropertyClaimRef<'_>> {
        PrefabPropertyClaims {
            components: &self.components,
            revision: None,
        }
        .claim_at(slot)
    }
}

impl SceneAuthoringSession {
    /// Borrows the exact stored property declarations without opening a Prefab dependency.
    #[doc(hidden)]
    pub fn prefab_property_claims(
        &self,
        anchor: &SceneEntityId,
        steps: &[PrefabInstancePathSegment],
    ) -> Result<PrefabPropertyClaims<'_>, PrefabDeclarationError> {
        if self.authority.published().is_none() {
            return Err(PrefabDeclarationError::NotAdmitted);
        }
        property_claims(
            &self.document.entities,
            anchor,
            steps,
            Some(self.revision()),
        )
    }

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
    /// Retained observations require their original source, revision and Schema admission.
    /// This prepares current source only; it neither resolves a base nor edits another source.
    pub fn prepare_prefab_revert(
        &self,
        base_revision: SceneAuthoringRevision,
        path: &PrefabInstancePath,
        action: PrefabRevert<'_>,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedSceneAuthoringEdit, ScenePatchReport> {
        if base_revision != self.revision() {
            return Err(super::prepared::stale_basis());
        }
        let schema = self
            .authoring_schema_basis()
            .ok_or_else(|| PrefabDeclarationError::NotAdmitted.report())?;
        let patch = lower_revert(&self.document.entities, path, limits, |overlay| {
            validate_revert(overlay, path, base_revision, &schema, action)
        })
        .map_err(PrefabDeclarationError::report)?;
        self.prepare_authoring_edit(base_revision, &patch)
    }
}

impl PrefabAuthoringSession {
    /// Borrows the exact stored property declarations without opening a Prefab dependency.
    #[doc(hidden)]
    pub fn prefab_property_claims(
        &self,
        anchor: &SceneEntityId,
        steps: &[PrefabInstancePathSegment],
    ) -> Result<PrefabPropertyClaims<'_>, PrefabDeclarationError> {
        if self.authority.published().is_none() {
            return Err(PrefabDeclarationError::NotAdmitted);
        }
        property_claims(
            &self.document.entities,
            anchor,
            steps,
            Some(self.revision()),
        )
    }

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
    /// Retained observations require their original source, revision and Schema admission.
    pub fn prepare_prefab_revert(
        &self,
        base_revision: SceneAuthoringRevision,
        path: &PrefabInstancePath,
        action: PrefabRevert<'_>,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedPrefabAuthoringEdit, ScenePatchReport> {
        if base_revision != self.revision() {
            return Err(super::prepared::stale_basis());
        }
        let schema = self
            .authoring_schema_basis()
            .ok_or_else(|| PrefabDeclarationError::NotAdmitted.report())?;
        let patch = lower_revert(&self.document.entities, path, limits, |overlay| {
            validate_revert(overlay, path, base_revision, &schema, action)
        })
        .map_err(PrefabDeclarationError::report)?;
        self.prepare_authoring_edit(base_revision, &patch)
    }
}

impl PreparedSceneAuthoringEdit {
    /// Borrows stored claims from the exact candidate, with no dependency expansion.
    #[doc(hidden)]
    pub fn prefab_property_claims(
        &self,
        anchor: &SceneEntityId,
        steps: &[PrefabInstancePathSegment],
    ) -> Result<PrefabPropertyClaims<'_>, PrefabDeclarationError> {
        property_claims(&self.document().entities, anchor, steps, None)
    }
}

impl PreparedPrefabAuthoringEdit {
    /// Borrows stored claims from the exact candidate, with no dependency expansion.
    #[doc(hidden)]
    pub fn prefab_property_claims(
        &self,
        anchor: &SceneEntityId,
        steps: &[PrefabInstancePathSegment],
    ) -> Result<PrefabPropertyClaims<'_>, PrefabDeclarationError> {
        property_claims(&self.document().entities, anchor, steps, None)
    }
}

fn property_claims<'a>(
    entities: &'a [SceneEntityRecord],
    anchor: &SceneEntityId,
    steps: &[PrefabInstancePathSegment],
    revision: Option<SceneAuthoringRevision>,
) -> Result<PrefabPropertyClaims<'a>, PrefabDeclarationError> {
    if steps.len() > PrefabDeclarationLimits::MAX_DEPTH {
        return Err(PrefabDeclarationError::BudgetExceeded);
    }
    let root = anchor_instance(entities, anchor)?;
    let overlay = instance_at(root, steps)?
        .as_ref()
        .ok_or(PrefabDeclarationError::InstanceAbsent)?
        .overrides
        .as_ref()
        .ok_or(PrefabDeclarationError::OverlayAbsent)?;
    Ok(PrefabPropertyClaims {
        components: &overlay.components,
        revision,
    })
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
    let root = anchor_instance(entities, &path.anchor)?;
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
    limits: PrefabDeclarationLimits,
    select: impl FnOnce(&PrefabOverlay) -> Result<Option<PrefabOverlaySlot>, PrefabDeclarationError>,
) -> Result<ScenePatchDocument, PrefabDeclarationError> {
    let mut budget = DeclarationBudget::new(limits);
    budget.location(path.anchor.as_str(), &path.steps)?;
    let root = anchor_instance(entities, &path.anchor)?;
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
    let slot = select(overlay)?;
    let mut value = root.clone();
    let selected = instance_at_mut(&mut value, &path.steps)?
        .as_mut()
        .ok_or(PrefabDeclarationError::InstanceAbsent)?;
    match slot {
        None => selected.overrides = None,
        Some(slot) => revert_intent(
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

pub(crate) fn anchor_instance<'a>(
    entities: &'a [SceneEntityRecord],
    anchor: &SceneEntityId,
) -> Result<&'a Option<PrefabInstance>, PrefabDeclarationError> {
    // Published Scene and Prefab records retain the same canonical ID ordering as entity_view.
    let index = entities
        .binary_search_by(|entity| entity.id.cmp(anchor))
        .map_err(|_| PrefabDeclarationError::AnchorNotFound)?;
    Ok(&entities[index].prefab)
}

pub(crate) fn instance_at<'a>(
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
    path: &PrefabInstancePath,
    revision: SceneAuthoringRevision,
    schema: &AuthoringSchemaBasis,
    action: PrefabRevert<'_>,
) -> Result<Option<PrefabOverlaySlot>, PrefabDeclarationError> {
    let (slot, observed) = match action {
        PrefabRevert::Overlay => return Ok(None),
        PrefabRevert::Property(expected) => {
            if expected.revision != Some(revision)
                || !overlay
                    .property_claim_at(expected.slot())
                    .is_some_and(|current| std::ptr::eq(current.claim(), expected.claim()))
            {
                return Err(PrefabDeclarationError::StaleObservedIntent);
            }
            return Ok(Some(expected.slot()));
        }
        PrefabRevert::Observed(selected) => {
            if selected.observation.revision() != revision
                || selected.observation.schema() != schema
                || selected.path != path
            {
                return Err(PrefabDeclarationError::StaleObservedIntent);
            }
            let Some(slot) = selected.slot else {
                return Ok(None);
            };
            (slot, true)
        }
        PrefabRevert::Intent(slot) => (slot, false),
    };
    let found = match slot {
        PrefabOverlaySlot::Addition { index } => index < overlay.additions.len(),
        PrefabOverlaySlot::InstanceClaim { index } => index < overlay.instances.len(),
        PrefabOverlaySlot::Component { index } if observed => index < overlay.components.len(),
        PrefabOverlaySlot::Component { .. } => {
            return Err(PrefabDeclarationError::ComponentClaimRequired);
        }
        PrefabOverlaySlot::Property { .. } if observed => overlay.property_claim_at(slot).is_some(),
        PrefabOverlaySlot::Property { .. } => {
            return Err(PrefabDeclarationError::PropertyClaimRequired);
        }
        PrefabOverlaySlot::Placement { index } => index < overlay.placements.len(),
        PrefabOverlaySlot::Suppression { index } => index < overlay.suppressions.len(),
    };
    found
        .then_some(Some(slot))
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
        self.charge(
            1_usize.saturating_add(steps.len()),
            anchor.len().saturating_add(steps.len().saturating_mul(9)),
        )
    }

    fn instance(
        &mut self,
        instance: &PrefabInstance,
        copies: usize,
    ) -> Result<(), PrefabDeclarationError> {
        let cost = prefab_instance_cost(instance, self.remaining_cost_limits(copies))
            .map_err(|_| PrefabDeclarationError::BudgetExceeded)?;
        self.charge(
            cost.nodes.saturating_mul(copies),
            cost.logical_bytes().saturating_mul(copies),
        )
    }

    fn remaining_cost_limits(&self, copies: usize) -> PrefabOverlayCostLimits {
        let nodes = self.limits.max_nodes.saturating_sub(self.nodes) / copies;
        let bytes = self.limits.max_logical_bytes.saturating_sub(self.bytes) / copies;
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
        }
    }

    fn charge(&mut self, nodes: usize, bytes: usize) -> Result<(), PrefabDeclarationError> {
        self.nodes = self.nodes.saturating_add(nodes);
        self.bytes = self.bytes.saturating_add(bytes);
        if self.nodes > self.limits.max_nodes || self.bytes > self.limits.max_logical_bytes {
            Err(PrefabDeclarationError::BudgetExceeded)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
