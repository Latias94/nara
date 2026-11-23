use super::*;
use crate::{
    PrefabComponentEdit, PrefabInstanceBinding, SceneComponentRecord, SourceDocumentKind,
    authoring::prepared::stale_basis,
    prefab::{SourceDocumentInput, inspection::ObservedPrefabComponentEdit},
};

impl SceneAuthoringSession {
    #[doc(hidden)]
    pub fn prepare_prefab_component_edit(
        &self,
        base_revision: SceneAuthoringRevision,
        edit: &ObservedPrefabComponentEdit,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedSceneAuthoringEdit, ScenePatchReport> {
        let patch = prepare_patch(
            SourceDocumentInput::Scene(self.document()),
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
    pub fn prepare_prefab_component_edit(
        &self,
        base_revision: SceneAuthoringRevision,
        edit: &ObservedPrefabComponentEdit,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedPrefabAuthoringEdit, ScenePatchReport> {
        let patch = prepare_patch(
            SourceDocumentInput::Prefab(self.document()),
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
    source: SourceDocumentInput<'_>,
    revision: SceneAuthoringRevision,
    base_revision: SceneAuthoringRevision,
    basis: Option<AuthoringSchemaBasis>,
    edit: &ObservedPrefabComponentEdit,
    limits: PrefabDeclarationLimits,
) -> Result<ScenePatchDocument, ScenePatchReport> {
    if revision != base_revision
        || !edit.matches_working_source(source, revision)
        || basis.as_ref() != Some(&edit.basis)
    {
        return Err(stale_basis());
    }
    let entities = match source {
        SourceDocumentInput::Scene(document) => &document.entities,
        SourceDocumentInput::Prefab(document) => &document.entities,
    };
    lower_write(entities, edit, limits).map_err(PrefabDeclarationError::report)
}

fn lower_write(
    entities: &[SceneEntityRecord],
    edit: &ObservedPrefabComponentEdit,
    limits: PrefabDeclarationLimits,
) -> Result<ScenePatchDocument, PrefabDeclarationError> {
    use PrefabDeclarationError as Error;

    if matches!(edit.edit, PrefabComponentEdit::Revert) {
        let previous = edit.existing.as_ref().ok_or(Error::StaleObservedIntent)?;
        return lower_revert(
            entities,
            &PrefabInstancePath::new(edit.anchor.clone()),
            limits,
            |overlay| {
                overlay
                    .components
                    .get(previous.index)
                    .filter(|current| current.exactly_matches(&previous.intent))
                    .map(|_| {
                        Some(PrefabOverlaySlot::Component {
                            index: previous.index,
                        })
                    })
                    .ok_or(Error::StaleObservedIntent)
            },
        );
    }

    let mut budget = DeclarationBudget::new(limits);
    budget.location(edit.anchor.as_str(), &[])?;
    let root = anchor_instance(entities, &edit.anchor)?;
    let original = root.as_ref().ok_or(Error::InstanceAbsent)?;
    let overlay = original.overrides.as_ref();
    if edit.base.kind() != SourceDocumentKind::Prefab
        || overlay.is_some_and(|overlay| overlay.base != edit.base)
        || matches!(&original.binding, PrefabInstanceBinding::Root { base, .. } if base != &edit.base)
        || original
            .arguments
            .as_ref()
            .is_some_and(|args| args.base != edit.base)
    {
        return Err(Error::StaleObservedIntent);
    }
    budget.instance(original, 2)?;
    let intents = overlay.map_or(&[][..], |overlay| overlay.components.as_slice());
    let existing = if let Some(previous) = &edit.existing {
        let current = intents
            .get(previous.index)
            .filter(|current| current.exactly_matches(&previous.intent))
            .ok_or(Error::StaleObservedIntent)?;
        Some((previous.index, current))
    } else {
        if intents
            .iter()
            .any(|intent| intent.address() == (&edit.target, &edit.component))
        {
            return Err(Error::StaleObservedIntent);
        }
        None
    };
    let record = match &edit.edit {
        PrefabComponentEdit::Set { value, .. } => Some((value, edit.owner.as_str())),
        PrefabComponentEdit::Suppress => None,
        PrefabComponentEdit::Revert => {
            unreachable!("Revert uses the existing declaration transaction")
        }
    };
    let cost = budget
        .remaining_cost_limits(1)
        .component_edit_delta_cost(
            overlay.is_none().then_some(&edit.base),
            &edit.target,
            &edit.component,
            record,
        )
        .map_err(|_| Error::BudgetExceeded)?;
    budget.charge(cost.nodes, cost.logical_bytes())?;
    let intent = match &edit.edit {
        PrefabComponentEdit::Set { mode, value } => PrefabComponentIntent::Set {
            mode: *mode,
            target: edit.target.clone(),
            component: edit.component.clone(),
            record: SceneComponentRecord::new(edit.version, value.clone())
                .with_owner(edit.owner.clone()),
        },
        PrefabComponentEdit::Suppress => PrefabComponentIntent::Suppress {
            target: edit.target.clone(),
            component: edit.component.clone(),
        },
        PrefabComponentEdit::Revert => {
            unreachable!("Revert uses the existing declaration transaction")
        }
    };
    if existing.is_some_and(|(_, current)| current.exactly_matches(&intent)) {
        return Err(Error::AlreadyWritten);
    }
    let mut value = original.clone();
    let overlay = value
        .overrides
        .get_or_insert_with(|| PrefabOverlay::new(edit.base.clone()));
    if let Some((index, _)) = existing {
        overlay.components[index] = intent;
    } else {
        overlay.components.push(intent);
    }
    Ok(ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: edit.anchor.clone(),
            expected: root.clone(),
            value: Some(value),
        },
    ]))
}
