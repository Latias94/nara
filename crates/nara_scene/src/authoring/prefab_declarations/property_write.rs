use nara_reflect::{__private::component_values_exactly_match, ComponentValueCost};

use super::{DeclarationBudget, PrefabDeclarationError, PrefabDeclarationLimits, anchor_instance};
use crate::{
    AuthoringSchemaBasis, PrefabAuthoringSession, PrefabComponentIntent, PrefabEntityTarget,
    PrefabInstanceBinding, PrefabOverlay, PrefabPropertyClaim, PrefabPropertyIntent,
    PreparedPrefabAuthoringEdit, PreparedSceneAuthoringEdit, SceneAuthoringRevision,
    SceneAuthoringSession, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    ScenePatchReport, SourceDocumentKind,
    authoring::prepared::stale_basis,
    prefab::{SourceDocumentInput, inspection::ObservedPrefabPropertyWrite},
};

impl SceneAuthoringSession {
    #[doc(hidden)]
    pub fn prepare_prefab_property_write(
        &self,
        base_revision: SceneAuthoringRevision,
        edit: &ObservedPrefabPropertyWrite,
    ) -> Result<PreparedSceneAuthoringEdit, ScenePatchReport> {
        let patch = prepare_patch(
            SourceDocumentInput::Scene(self.document()),
            self.revision(),
            base_revision,
            self.authoring_schema_basis(),
            edit,
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
    ) -> Result<PreparedPrefabAuthoringEdit, ScenePatchReport> {
        let patch = prepare_patch(
            SourceDocumentInput::Prefab(self.document()),
            self.revision(),
            base_revision,
            self.authoring_schema_basis(),
            edit,
        )?;
        self.prepare_authoring_edit(base_revision, &patch)
    }
}

fn prepare_patch(
    source: SourceDocumentInput<'_>,
    revision: SceneAuthoringRevision,
    base_revision: SceneAuthoringRevision,
    basis: Option<AuthoringSchemaBasis>,
    edit: &ObservedPrefabPropertyWrite,
) -> Result<ScenePatchDocument, ScenePatchReport> {
    if revision != base_revision || !edit.matches_working_source(source, revision) {
        return Err(stale_basis());
    }
    let basis = basis.ok_or_else(|| PrefabDeclarationError::NotAdmitted.report())?;
    if basis != edit.basis {
        return Err(stale_basis());
    }
    if basis.schema(&edit.component).map(|schema| schema.version()) != Some(edit.version) {
        return Err(PrefabDeclarationError::StaleObservedIntent.report());
    }
    let entities = match source {
        SourceDocumentInput::Scene(document) => &document.entities,
        SourceDocumentInput::Prefab(document) => &document.entities,
    };
    lower_write(entities, edit, edit.existing_claim).map_err(PrefabDeclarationError::report)
}

/// Admits both source roots and the change before keeping the request across a drive.
pub(crate) fn admit_property_write<'a>(
    entities: &'a [SceneEntityRecord],
    edit: &ObservedPrefabPropertyWrite,
    limits: PrefabDeclarationLimits,
) -> Result<
    (
        Option<&'a PrefabEntityTarget>,
        Option<usize>,
        ComponentValueCost,
    ),
    PrefabDeclarationError,
> {
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
    {
        return Err(Error::StaleObservedIntent);
    }

    budget.instance(original, 2)?;
    let intents = overlay.map_or(&[][..], |overlay| overlay.components.as_slice());
    let group = observed_group(intents, edit)?;
    let exact_claim = selected_claim_index(intents, group.map(|(_, claims)| claims), edit)?;
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
    Ok((
        group.map(|(target, _)| target),
        exact_claim,
        cost.value_cost,
    ))
}

fn lower_write(
    entities: &[SceneEntityRecord],
    edit: &ObservedPrefabPropertyWrite,
    exact_claim: Option<usize>,
) -> Result<ScenePatchDocument, PrefabDeclarationError> {
    use PrefabDeclarationError as Error;

    let root = anchor_instance(entities, &edit.anchor)?;
    let original = root.as_ref().ok_or(Error::InstanceAbsent)?;
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

fn observed_group<'a>(
    intents: &'a [PrefabComponentIntent],
    edit: &ObservedPrefabPropertyWrite,
) -> Result<Option<(&'a PrefabEntityTarget, &'a [PrefabPropertyClaim])>, PrefabDeclarationError> {
    let Some(index) = edit.existing_intent else {
        return Ok(None);
    };
    match intents.get(index) {
        Some(PrefabComponentIntent::Properties {
            target,
            component,
            version,
            claims,
        }) if component == &edit.component && *version == edit.version => {
            Ok(Some((target, claims)))
        }
        _ => Err(PrefabDeclarationError::StaleObservedIntent),
    }
}

fn selected_claim_index(
    intents: &[PrefabComponentIntent],
    group: Option<&[PrefabPropertyClaim]>,
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
    let Some(claims) = group else {
        return Ok(None);
    };

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
