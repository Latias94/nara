//! Explicit one-field authored upgrades, separate from classification-only successors.

use super::*;
use nara_reflect::{ComponentFieldPath, ComponentPropertyPath, ComponentPropertyReadBudget};

#[cfg(test)]
mod tests;

/// Shares the live complete raw-source admission with data-only recovery validation.
pub(crate) fn admit_authored_upgrade_document(
    roots: &[SceneEntityId],
    entities: &[SceneEntityRecord],
    limits: SceneFileLimits,
    admit: &mut reclassification::AdmissionCallback<'_>,
) -> Result<(), ScenePatchReport> {
    reclassification::admit_document(roots, entities, limits, admit)
}

impl AuthoringDocumentState {
    /// Classifies a retained original through the ordinary source walker under one exact basis.
    /// The source owner already admitted its state/source storage before calling this bridge.
    pub(crate) fn classify_authored_upgrade_source(
        document: &SceneDocument,
        source: AuthoringDocumentSource,
        kind: AuthoringDocumentKind,
        previous: &AuthoringSchemaBasis,
        limits: SceneFileLimits,
        admit: &mut reclassification::AdmissionCallback<'_>,
    ) -> Result<Self, ScenePatchReport> {
        let mut refusal = None;
        let result = {
            let mut callback = |stage, bytes| {
                admit(stage, bytes).map_err(|report| {
                    refusal = Some(report);
                    AuthoringDocumentPublicationError::new(DiagnosticReport::default())
                })
            };
            classify_scene_document_impl(
                document,
                source,
                kind,
                Arc::clone(&previous.0),
                limits,
                Some(&mut callback),
            )
        };
        match refusal {
            Some(report) => Err(report),
            None => result.map_err(reclassification::publication_report),
        }
    }
}

/// A reviewed component successor changing one existing root field's value contract.
///
/// The project computes conversion semantics. This owner proves the permitted declaration and
/// source footprint, including dormant overlays; it never executes transformation code.
#[derive(Debug)]
pub struct AuthoredFieldUpgrade {
    previous: AuthoringSchemaBasis,
    successor: AuthoringSchemaBasis,
    component: ComponentTypeId,
    field: ComponentFieldId,
    next_property: ComponentPropertyPath,
}

struct UpgradeSourceValidation<'admission, 'callback> {
    limits: SceneFileLimits,
    selectors: ComponentPropertyReadBudget,
    admit: &'admission mut reclassification::AdmissionCallback<'callback>,
}

impl UpgradeSourceValidation<'_, '_> {
    fn claim_paths(&mut self, claims: &[PrefabPropertyClaim]) -> Result<(), ScenePatchReport> {
        let comparisons = (usize::BITS - claims.len().leading_zeros()) as usize + 1;
        for claim in claims {
            let bytes = claim.path.logical_bytes().map_err(|_| refused())?;
            charge_validation(
                self.admit,
                "authored-upgrade-property-paths",
                comparisons,
                bytes.checked_mul(comparisons).ok_or_else(refused)?,
                0,
            )?;
        }
        let index = claims
            .len()
            .checked_mul(std::mem::size_of::<&ComponentPropertyPath>());
        (self.admit)("authored-upgrade-property-path-index", index)?;
        index.ok_or_else(refused)?;
        crate::prefab::validate_claim_paths(claims).map_err(|_| refused())
    }

    fn record(
        &mut self,
        schema: &ComponentSchema,
        record: &SceneComponentRecord,
    ) -> Result<(), ScenePatchReport> {
        let effective = nara_reflect::__private::effective_component_value_with_default_overrides_with_admission(
            schema,
            &record.value,
            &record.contextual_defaults,
            self.limits.component_materialization_limits(),
            |nodes, bytes, allocated| {
                charge_validation(
                    self.admit,
                    "authored-upgrade-record-defaults",
                    nodes,
                    bytes,
                    allocated,
                )
            },
        )
        .map_err(|error| match error {
            nara_reflect::__private::EffectiveValueAdmissionError::Admission(report) => report,
            nara_reflect::__private::EffectiveValueAdmissionError::Value(_) => refused(),
        })?;
        nara_reflect::__private::validate_component_value_with_admission(
            schema,
            effective.as_ref(),
            self.limits.component_materialization_limits(),
            |nodes, bytes, allocated| {
                charge_validation(
                    self.admit,
                    "authored-upgrade-record-validation",
                    nodes,
                    bytes,
                    allocated,
                )
            },
        )
        .map_err(|error| match error {
            nara_reflect::__private::ComponentValueAdmissionError::Admission(report) => report,
            nara_reflect::__private::ComponentValueAdmissionError::Value(_) => refused(),
        })
    }

    fn property(
        &mut self,
        schema: &ComponentSchema,
        claim: &PrefabPropertyClaim,
    ) -> Result<(), ScenePatchReport> {
        let selected =
            nara_reflect::__private::resolve_component_property_schema_claim_with_admission(
                schema,
                schema.version,
                &claim.path,
                &mut self.selectors,
                |nodes, bytes, allocated| {
                    charge_validation(
                        self.admit,
                        "authored-upgrade-property-selector",
                        nodes,
                        bytes,
                        allocated,
                    )
                },
            )
            .map_err(property_validation_error)?;
        let payload = match &claim.intent {
            PrefabPropertyIntent::Set(value) => Some(value),
            PrefabPropertyIntent::Absent if claim.path.steps.is_empty() => {
                let default = selected.field.default_value();
                if selected.field.required && default.is_none() {
                    return Err(refused());
                }
                // The claim does not own the target record's contextual default environment.
                // Validate the declaration here; the compiler selects its existing default.
                if selected
                    .field
                    .shape()
                    .contains_kind(nara_reflect::ComponentValueKind::EntityRef)
                {
                    None
                } else {
                    default
                }
            }
            PrefabPropertyIntent::Absent => return Err(refused()),
        };
        if let Some(payload) = payload {
            nara_reflect::__private::validate_component_property_value_with_admission(
                selected.shape,
                &claim.path,
                payload,
                self.limits.component_materialization_limits(),
                |nodes, bytes, allocated| {
                    charge_validation(
                        self.admit,
                        "authored-upgrade-property-value",
                        nodes,
                        bytes,
                        allocated,
                    )
                },
            )
            .map_err(property_validation_error)?;
        }
        Ok(())
    }
}

fn property_validation_error(
    error: nara_reflect::__private::ComponentPropertyAdmissionError<ScenePatchReport>,
) -> ScenePatchReport {
    match error {
        nara_reflect::__private::ComponentPropertyAdmissionError::Admission(report) => report,
        nara_reflect::__private::ComponentPropertyAdmissionError::Property(_) => refused(),
    }
}

fn charge_validation(
    admit: &mut reclassification::AdmissionCallback<'_>,
    stage: &'static str,
    nodes: usize,
    bytes: usize,
    allocated: usize,
) -> Result<(), ScenePatchReport> {
    let total = nodes
        .checked_add(bytes)
        .and_then(|bytes| bytes.checked_add(allocated));
    admit(stage, total)?;
    total.map(|_| ()).ok_or_else(refused)
}

impl AuthoredFieldUpgrade {
    pub fn previous_basis(&self) -> &AuthoringSchemaBasis {
        &self.previous
    }

    pub fn successor_basis(&self) -> &AuthoringSchemaBasis {
        &self.successor
    }

    pub fn prepare(
        previous: AuthoringSchemaBasis,
        successor: AuthoringSchemaBasis,
        component: ComponentTypeId,
        field: ComponentFieldId,
        next_property: ComponentPropertyPath,
        selector_budget: &mut ComponentPropertyReadBudget,
    ) -> Result<Self, ScenePatchReport> {
        let upgrade = Self {
            previous,
            successor,
            component,
            field,
            next_property,
        };
        upgrade.validate_successor(selector_budget)?;
        Ok(upgrade)
    }

    fn validate_successor(
        &self,
        selector_budget: &mut ComponentPropertyReadBudget,
    ) -> Result<(), ScenePatchReport> {
        let previous = &self.previous.0;
        let successor = &self.successor.0;
        let old = previous.schemas.get(&self.component).ok_or_else(refused)?;
        let next = successor.schemas.get(&self.component).ok_or_else(refused)?;
        if self.previous == self.successor
            || previous.owners.len() != successor.owners.len()
            || previous.schemas.len() != successor.schemas.len()
            || old.owner != next.owner
            || self.next_property.root != self.field
        {
            return Err(refused());
        }
        let mut selected_owner = false;
        for (old_owner, next_owner) in previous.owners.iter().zip(&successor.owners) {
            if old_owner.owner_id() != next_owner.owner_id() {
                return Err(refused());
            }
            if old_owner.owner_id() == &old.owner {
                if old_owner.current().generation().checked_add(1)
                    != Some(next_owner.current().generation())
                    || next_owner.predecessor() != Some(old_owner.current())
                    || old_owner.current().type_tombstones != next_owner.current().type_tombstones
                {
                    return Err(refused());
                }
                selected_owner = true;
            } else if old_owner.current() != next_owner.current()
                || old_owner.predecessor() != next_owner.predecessor()
            {
                return Err(refused());
            }
        }
        if !selected_owner {
            return Err(refused());
        }
        for (component, old_entry) in &previous.schemas {
            let next_entry = successor.schemas.get(component).ok_or_else(refused)?;
            if old_entry.owner != next_entry.owner {
                return Err(refused());
            }
            if old_entry.owner == old.owner {
                if !reclassification::data_adapter_owner_successor(
                    next_entry.adapter.as_ref(),
                    old_entry.adapter.as_ref(),
                ) {
                    return Err(refused());
                }
            } else if old_entry.adapter != next_entry.adapter {
                return Err(refused());
            }
            if component != &self.component && old_entry.schema != next_entry.schema {
                return Err(refused());
            }
        }
        let old = &old.schema;
        let next = &next.schema;
        if old.id != next.id
            || old.aliases != next.aliases
            || old.capabilities != next.capabilities
            || old.field_tombstones != next.field_tombstones
            || old.version.0.checked_add(1) != Some(next.version.0)
            || old.fields.len() != next.fields.len()
        {
            return Err(refused());
        }
        let mut selected_field = false;
        for (old_field, next_field) in old.fields.iter().zip(&next.fields) {
            if old_field.id != next_field.id {
                return Err(refused());
            }
            if old_field.id == self.field {
                if old_field.path != next_field.path
                    || old_field.aliases != next_field.aliases
                    || old_field.capabilities != next_field.capabilities
                    || old_field.member_tombstones != next_field.member_tombstones
                    || !old_field.required
                    || !next_field.required
                    || old_field.default_value.is_some()
                    || next_field.default_value.is_some()
                {
                    return Err(refused());
                }
                selected_field = true;
            } else if old_field != next_field {
                return Err(refused());
            }
        }
        if !selected_field {
            return Err(refused());
        }
        let storage = self.storage_path();
        match (&old.authoring_initial_value, &next.authoring_initial_value) {
            (None, None) => {}
            (Some(old), Some(next)) if preserves_outside(old, next, storage.segments()) => {}
            _ => return Err(refused()),
        }
        nara_reflect::resolve_component_property_schema_claim(
            next,
            next.version,
            &self.next_property,
            selector_budget,
        )
        .map_err(|_| refused())?;
        Ok(())
    }

    fn storage_path(&self) -> &ComponentFieldPath {
        // The private constructor validates both associations before publishing this proof.
        self.previous
            .schema(&self.component)
            .and_then(|schema| schema.fields.iter().find(|field| field.id == self.field))
            .expect("validated authored field upgrade")
            .path()
    }

    fn versions(&self) -> (ComponentSchemaVersion, ComponentSchemaVersion) {
        (
            self.previous.schema(&self.component).unwrap().version,
            self.successor.schema(&self.component).unwrap().version,
        )
    }

    fn record(
        &self,
        old: &SceneComponentRecord,
        next: &SceneComponentRecord,
        validation: &mut UpgradeSourceValidation<'_, '_>,
    ) -> Result<(), ScenePatchReport> {
        if old.owner.as_ref().is_some_and(|owner| {
            Some(owner) != self.previous.component_schema_owner(&self.component)
        }) {
            return if old == next { Ok(()) } else { Err(refused()) };
        }
        let (old_version, next_version) = self.versions();
        if old.owner != next.owner
            || old.version != old_version
            || next.version != next_version
            // This proof changes one required field with no default. Every contextual
            // declaration therefore belongs to an unchanged field and must remain exact.
            || old.contextual_defaults != next.contextual_defaults
            || !preserves_outside(&old.value, &next.value, self.storage_path().segments())
        {
            return Err(refused());
        }
        for (basis, record) in [(&self.previous, old), (&self.successor, next)] {
            let schema = basis.schema(&self.component).ok_or_else(refused)?;
            for (field, value) in &record.contextual_defaults {
                let cost = value.cost();
                charge_validation(
                    validation.admit,
                    "authored-upgrade-contextual-defaults",
                    cost.nodes().saturating_add(1),
                    cost.logical_bytes().saturating_add(field.as_str().len()),
                    0,
                )?;
            }
            crate::document::validate_contextual_defaults(
                schema,
                record,
                validation.limits.component_materialization_limits(),
            )
            .map_err(|_| refused())?;
        }
        validation.record(
            self.previous.schema(&self.component).ok_or_else(refused)?,
            old,
        )?;
        validation.record(
            self.successor.schema(&self.component).ok_or_else(refused)?,
            next,
        )?;
        Ok(())
    }

    fn components(
        &self,
        old: &BTreeMap<ComponentTypeId, SceneComponentRecord>,
        next: &BTreeMap<ComponentTypeId, SceneComponentRecord>,
        validation: &mut UpgradeSourceValidation<'_, '_>,
    ) -> Result<(), ScenePatchReport> {
        if old.len() != next.len() {
            return Err(refused());
        }
        for (component, old) in old {
            let next = next.get(component).ok_or_else(refused)?;
            if component == &self.component {
                self.record(old, next, validation)?;
            } else if old != next {
                return Err(refused());
            }
        }
        Ok(())
    }

    fn instance(
        &self,
        old: Option<&PrefabInstance>,
        next: Option<&PrefabInstance>,
        validation: &mut UpgradeSourceValidation<'_, '_>,
    ) -> Result<(), ScenePatchReport> {
        match (old, next) {
            (None, None) => Ok(()),
            (Some(old), Some(next))
                if old.source == next.source && old.arguments == next.arguments =>
            {
                match (&old.overrides, &next.overrides) {
                    (None, None) => Ok(()),
                    (Some(old), Some(next)) => self.overlay(old, next, validation),
                    _ => Err(refused()),
                }
            }
            _ => Err(refused()),
        }
    }

    fn overlay(
        &self,
        old: &crate::PrefabOverlay,
        next: &crate::PrefabOverlay,
        validation: &mut UpgradeSourceValidation<'_, '_>,
    ) -> Result<(), ScenePatchReport> {
        if old.base != next.base
            || old.placements != next.placements
            || old.suppressions != next.suppressions
            || old.additions.len() != next.additions.len()
            || old.instances.len() != next.instances.len()
            || old.components.len() != next.components.len()
        {
            return Err(refused());
        }
        for (old, next) in old.additions.iter().zip(&next.additions) {
            if old.id != next.id {
                return Err(refused());
            }
            self.components(&old.components, &next.components, validation)?;
            self.instance(old.prefab.as_ref(), next.prefab.as_ref(), validation)?;
        }
        for (old, next) in old.instances.iter().zip(&next.instances) {
            if old.target != next.target {
                return Err(refused());
            }
            self.instance(old.value.as_ref(), next.value.as_ref(), validation)?;
        }
        for (old, next) in old.components.iter().zip(&next.components) {
            self.component_intent(old, next, validation)?;
        }
        Ok(())
    }

    fn component_intent(
        &self,
        old: &crate::PrefabComponentIntent,
        next: &crate::PrefabComponentIntent,
        validation: &mut UpgradeSourceValidation<'_, '_>,
    ) -> Result<(), ScenePatchReport> {
        use crate::PrefabComponentIntent;
        match (old, next) {
            (
                PrefabComponentIntent::Set {
                    mode,
                    target,
                    component,
                    record,
                },
                PrefabComponentIntent::Set {
                    mode: next_mode,
                    target: next_target,
                    component: next_component,
                    record: next_record,
                },
            ) if component == &self.component
                && component == next_component
                && mode == next_mode
                && target == next_target =>
            {
                self.record(record, next_record, validation)
            }
            (
                PrefabComponentIntent::Properties {
                    target,
                    component,
                    version,
                    claims,
                },
                PrefabComponentIntent::Properties {
                    target: next_target,
                    component: next_component,
                    version: next_version,
                    claims: next_claims,
                },
            ) if component == &self.component
                && component == next_component
                && target == next_target
                && (*version, *next_version) == self.versions()
                && claims.len() == next_claims.len() =>
            {
                validation.claim_paths(claims)?;
                validation.claim_paths(next_claims)?;
                for (old, next) in claims.iter().zip(next_claims) {
                    if old.path.root == self.field {
                        if !old.path.steps.is_empty()
                            || next.path != self.next_property
                            || !matches!(old.intent, PrefabPropertyIntent::Set(_))
                            || !matches!(next.intent, PrefabPropertyIntent::Set(_))
                        {
                            return Err(refused());
                        }
                    } else if old != next {
                        return Err(refused());
                    }
                    validation.property(
                        self.previous.schema(&self.component).ok_or_else(refused)?,
                        old,
                    )?;
                    validation.property(
                        self.successor.schema(&self.component).ok_or_else(refused)?,
                        next,
                    )?;
                }
                Ok(())
            }
            _ if old == next => {
                // An unchanged affected Properties batch still carries the old version, even
                // when empty. Other unchanged intents have no component payload to reinterpret.
                if matches!(old, PrefabComponentIntent::Properties { component, .. }
                    if component == &self.component)
                    || matches!(old, PrefabComponentIntent::Set { component, .. }
                        if component == &self.component)
                {
                    Err(refused())
                } else {
                    Ok(())
                }
            }
            _ => Err(refused()),
        }
    }

    fn validate_sources(
        &self,
        (old_roots, old_entities): (&[SceneEntityId], &[SceneEntityRecord]),
        (next_roots, next_entities): (&[SceneEntityId], &[SceneEntityRecord]),
        validation: &mut UpgradeSourceValidation<'_, '_>,
    ) -> Result<(), ScenePatchReport> {
        if old_roots != next_roots || old_entities.len() != next_entities.len() {
            return Err(refused());
        }
        for (old, next) in old_entities.iter().zip(next_entities) {
            if old.id != next.id || old.children != next.children {
                return Err(refused());
            }
            self.components(&old.components, &next.components, validation)?;
            self.instance(old.prefab.as_ref(), next.prefab.as_ref(), validation)?;
        }
        Ok(())
    }
}

impl AuthoringDocumentState {
    /// Classifies the complete replacement only after its original basis and raw footprint pass.
    ///
    /// The enclosing operation owns cumulative admission and charges all live incumbents before
    /// preparing its first successor. This helper publishes neither source nor a history policy.
    pub(crate) fn prepare_authored_upgrade(
        &self,
        original: &SceneDocument,
        candidate: &SceneDocument,
        upgrade: &AuthoredFieldUpgrade,
        admit: &mut reclassification::AdmissionCallback<'_>,
    ) -> Result<Self, ScenePatchReport> {
        if self.document_kind() != AuthoringDocumentKind::Scene {
            return Err(refused());
        }
        self.validate_authored_upgrade(
            (&original.roots, &original.entities),
            (&candidate.roots, &candidate.entities),
            upgrade,
            admit,
        )?;
        crate::format::validate_scene_publication_shape_with_admission(
            candidate,
            self.limits,
            |bytes| admit("publication-buffer", Some(bytes)),
        )
        .map_err(|error| match error {
            crate::format::PublicationShapeAdmissionError::Admission(report) => report,
            crate::format::PublicationShapeAdmissionError::Format(_) => refused(),
        })?;
        self.classify_authored_upgrade(candidate, upgrade, admit)
    }

    pub(crate) fn prepare_authored_prefab_upgrade(
        &self,
        original: &PrefabDocument,
        candidate: &PrefabDocument,
        upgrade: &AuthoredFieldUpgrade,
        admit: &mut reclassification::AdmissionCallback<'_>,
    ) -> Result<Self, ScenePatchReport> {
        if self.document_kind() != AuthoringDocumentKind::Prefab
            || original.inputs != candidate.inputs
        {
            return Err(refused());
        }
        self.validate_authored_upgrade(
            (&original.roots, &original.entities),
            (&candidate.roots, &candidate.entities),
            upgrade,
            admit,
        )?;
        reclassification::validate_prefab_shape(candidate, self.limits, Some(admit))?;
        // The incumbent structural/classification kernel reads a Scene projection. Reserve its
        // complete coexisting copy before instantiate; footprint proof itself only borrows Prefab.
        reclassification::admit_document(
            &candidate.roots,
            &candidate.entities,
            self.limits,
            admit,
        )?;
        self.classify_authored_upgrade(&candidate.instantiate(), upgrade, admit)
    }

    fn validate_authored_upgrade(
        &self,
        original: (&[SceneEntityId], &[SceneEntityRecord]),
        candidate: (&[SceneEntityId], &[SceneEntityRecord]),
        upgrade: &AuthoredFieldUpgrade,
        admit: &mut reclassification::AdmissionCallback<'_>,
    ) -> Result<(), ScenePatchReport> {
        if self.schema_basis() != upgrade.previous {
            return Err(refused());
        }
        reclassification::admit_document(original.0, original.1, self.limits, admit)?;
        reclassification::admit_document(candidate.0, candidate.1, self.limits, admit)?;
        let mut validation = UpgradeSourceValidation {
            limits: self.limits,
            selectors: ComponentPropertyReadBudget::new(
                self.limits.component_value_nodes(),
                self.limits.component_value_bytes(),
            ),
            admit,
        };
        upgrade.validate_sources(original, candidate, &mut validation)?;
        let owner = upgrade.previous.component_schema_owner(&upgrade.component);
        if self.record_facts.iter().any(|record| {
            record.component() == &upgrade.component
                && record.resolved_owner() == owner
                && record.readiness() != AuthoringRecordReadiness::Available
        }) {
            return Err(refused());
        }
        Ok(())
    }

    fn classify_authored_upgrade(
        &self,
        candidate: &SceneDocument,
        upgrade: &AuthoredFieldUpgrade,
        admit: &mut reclassification::AdmissionCallback<'_>,
    ) -> Result<Self, ScenePatchReport> {
        let next =
            reclassification::classify_with_admission(candidate, self, &upgrade.successor, admit)?;
        let owner = upgrade.previous.component_schema_owner(&upgrade.component);
        if next.record_facts.iter().any(|record| {
            record.component() == &upgrade.component
                && record.resolved_owner() == owner
                && record.readiness() != AuthoringRecordReadiness::Available
        }) {
            return Err(refused());
        }
        Ok(next)
    }
}

fn preserves_outside(
    old: &ComponentValue,
    next: &ComponentValue,
    path: &[ComponentFieldPathSegment],
) -> bool {
    let Some((segment, tail)) = path.split_first() else {
        return true;
    };
    match (segment, old, next) {
        (
            ComponentFieldPathSegment::Field(selected),
            ComponentValue::Map(old),
            ComponentValue::Map(next),
        ) => {
            old.len() == next.len()
                && old.iter().all(|(key, old)| {
                    next.get(key).is_some_and(|next| {
                        if key == selected {
                            preserves_outside(old, next, tail)
                        } else {
                            old == next
                        }
                    })
                })
                && old.contains_key(selected)
        }
        (
            ComponentFieldPathSegment::Index(selected),
            ComponentValue::List(old),
            ComponentValue::List(next),
        ) => {
            let Ok(selected) = usize::try_from(*selected) else {
                return false;
            };
            selected < old.len()
                && old.len() == next.len()
                && old
                    .iter()
                    .zip(next)
                    .enumerate()
                    .all(|(index, (old, next))| {
                        if index == selected {
                            preserves_outside(old, next, tail)
                        } else {
                            old == next
                        }
                    })
        }
        _ => false,
    }
}

fn refused() -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic_error(
        "scene.authored-schema-upgrade-refused",
        "Authored upgrade does not match its exact Schema and source change footprint",
    ));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}
