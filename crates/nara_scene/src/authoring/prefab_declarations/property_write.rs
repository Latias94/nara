use nara_reflect::{__private::component_values_exactly_match, ComponentValue};

use super::{DeclarationBudget, PrefabDeclarationError, PrefabDeclarationLimits, anchor_instance};
use crate::{
    AuthoringSchemaBasis, PrefabAuthoringSession, PrefabComponentIntent, PrefabInstanceBinding,
    PrefabOverlay, PrefabPropertyClaim, PrefabPropertyIntent, PreparedPrefabAuthoringEdit,
    PreparedSceneAuthoringEdit, SceneAuthoringRevision, SceneAuthoringSession, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation, ScenePatchReport, SourceDocumentKind,
    authoring::prepared::stale_basis, prefab::inspection::ObservedPrefabPropertyWrite,
};

impl SceneAuthoringSession {
    #[doc(hidden)]
    pub fn prepare_prefab_property_write(
        &self,
        base_revision: SceneAuthoringRevision,
        edit: &ObservedPrefabPropertyWrite,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedSceneAuthoringEdit, ScenePatchReport> {
        let patch = prepare_patch(
            &self.document.entities,
            self.revision(),
            base_revision,
            self.authoring_schema_basis(),
            edit,
            limits,
        )?;
        self.prepare_authoring_edit(base_revision, &patch)
    }
}

impl PrefabAuthoringSession {
    #[doc(hidden)]
    pub fn prepare_prefab_property_write(
        &self,
        base_revision: SceneAuthoringRevision,
        edit: &ObservedPrefabPropertyWrite,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedPrefabAuthoringEdit, ScenePatchReport> {
        let patch = prepare_patch(
            &self.document.entities,
            self.revision(),
            base_revision,
            self.authoring_schema_basis(),
            edit,
            limits,
        )?;
        self.prepare_authoring_edit(base_revision, &patch)
    }
}

fn prepare_patch(
    entities: &[SceneEntityRecord],
    revision: SceneAuthoringRevision,
    base_revision: SceneAuthoringRevision,
    basis: Option<AuthoringSchemaBasis>,
    edit: &ObservedPrefabPropertyWrite,
    limits: PrefabDeclarationLimits,
) -> Result<ScenePatchDocument, ScenePatchReport> {
    if revision != base_revision {
        return Err(stale_basis());
    }
    let basis = basis.ok_or_else(|| PrefabDeclarationError::NotAdmitted.report())?;
    if basis != edit.basis {
        return Err(stale_basis());
    }
    if basis.schema(&edit.component).map(|schema| schema.version()) != Some(edit.version) {
        return Err(PrefabDeclarationError::StaleObservedIntent.report());
    }
    lower_write(entities, edit, limits).map_err(PrefabDeclarationError::report)
}

fn lower_write(
    entities: &[SceneEntityRecord],
    edit: &ObservedPrefabPropertyWrite,
    limits: PrefabDeclarationLimits,
) -> Result<ScenePatchDocument, PrefabDeclarationError> {
    use PrefabDeclarationError as Error;

    let mut budget = DeclarationBudget::new(limits);
    budget.location(edit.anchor.as_str(), &[])?;
    let root = anchor_instance(entities, &edit.anchor)?;
    let original = root.as_ref().ok_or(Error::InstanceAbsent)?;
    let overlay = original.overrides.as_ref();
    if edit.base.kind() != SourceDocumentKind::Prefab
        || overlay.is_some_and(|overlay| overlay.base != edit.base)
        || matches!(
            &original.binding,
            PrefabInstanceBinding::Root { base, .. } if base != &edit.base
        )
        || original
            .arguments
            .as_ref()
            .is_some_and(|args| args.base != edit.base)
        || matches!(
            &edit.intent,
            PrefabPropertyIntent::Set(
                ComponentValue::List(_)
                    | ComponentValue::Map(_)
                    | ComponentValue::EntityReference(_)
            )
        )
    {
        return Err(Error::StaleObservedIntent);
    }

    budget.instance(original, 2)?;
    let intents = overlay.map_or(&[][..], |overlay| overlay.components.as_slice());
    let exact_claim = selected_claim_index(intents, edit)?;
    let cost = budget
        .remaining_cost_limits(1)
        .property_write_delta_cost(
            overlay.is_none().then_some(&edit.base),
            edit.existing_intent
                .is_none()
                .then_some((&edit.target, &edit.component)),
            exact_claim.is_none().then_some(&edit.property),
            &edit.intent,
        )
        .map_err(|_| Error::BudgetExceeded)?;
    budget.charge(cost.nodes, cost.logical_bytes())?;

    let mut value = original.clone();
    let overlay = value
        .overrides
        .get_or_insert_with(|| PrefabOverlay::new(edit.base.clone()));
    let intent = edit.intent.clone();
    let new_claim = |intent| PrefabPropertyClaim {
        path: edit.property.clone(),
        intent,
    };
    if let Some(index) = edit.existing_intent {
        let Some(PrefabComponentIntent::Properties { claims, .. }) =
            overlay.components.get_mut(index)
        else {
            return Err(Error::StaleObservedIntent);
        };
        if let Some(index) = exact_claim {
            claims
                .get_mut(index)
                .ok_or(Error::StaleObservedIntent)?
                .intent = intent;
        } else {
            claims.push(new_claim(intent));
        }
    } else {
        overlay.components.push(PrefabComponentIntent::Properties {
            target: edit.target.clone(),
            component: edit.component.clone(),
            version: edit.version,
            claims: vec![new_claim(intent)],
        });
    }
    Ok(ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: edit.anchor.clone(),
            expected: root.clone(),
            value: Some(value),
        },
    ]))
}

fn selected_claim_index(
    intents: &[PrefabComponentIntent],
    edit: &ObservedPrefabPropertyWrite,
) -> Result<Option<usize>, PrefabDeclarationError> {
    use PrefabDeclarationError as Error;

    if intents.iter().enumerate().any(|(index, intent)| {
        let (target, component) = match intent {
            PrefabComponentIntent::Set {
                target, component, ..
            }
            | PrefabComponentIntent::Suppress { target, component }
            | PrefabComponentIntent::Properties {
                target, component, ..
            } => (target, component),
        };
        Some(index) != edit.existing_intent
            && target == &edit.target
            && component == &edit.component
    }) {
        return Err(Error::StaleObservedIntent);
    }
    let Some(index) = edit.existing_intent else {
        return Ok(None);
    };
    let Some(PrefabComponentIntent::Properties {
        component,
        version,
        claims,
        ..
    }) = intents.get(index)
    else {
        return Err(Error::StaleObservedIntent);
    };
    if component != &edit.component || *version != edit.version {
        return Err(Error::StaleObservedIntent);
    }

    let mut exact = None;
    for (index, claim) in claims.iter().enumerate() {
        if claim.path.root != edit.property.root {
            continue;
        }
        let steps = &claim.path.steps;
        let requested = &edit.property.steps;
        if steps == requested {
            if exact.replace((index, &claim.intent)).is_some() {
                return Err(Error::StaleObservedIntent);
            }
        } else if steps.starts_with(requested) || requested.starts_with(steps) {
            return Err(Error::ConflictingClaim);
        }
    }
    if exact.is_some_and(|(_, previous)| match (previous, &edit.intent) {
        (PrefabPropertyIntent::Set(previous), PrefabPropertyIntent::Set(requested)) => {
            component_values_exactly_match(previous, requested)
        }
        (PrefabPropertyIntent::Absent, PrefabPropertyIntent::Absent) => true,
        _ => false,
    }) {
        return Err(Error::AlreadyWritten);
    }
    Ok(exact.map(|(index, _)| index))
}

#[cfg(test)]
mod tests;
