use super::*;
use crate::prefab::ComponentApplyScope;

impl<F, E> Compiler<'_, '_, '_, F, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(super) fn component_apply_destination(
        &mut self,
        apply: ComponentApplyScope<'_>,
    ) -> Result<ScopeId, SceneCompilationFailure> {
        let selected_scope = self
            .selected_anchor
            .and_then(|anchor| self.nodes[anchor].entered)
            .ok_or(SceneCompilationFailureKind::MissingTarget)?;
        let targets = [apply.selected, apply.destination];
        let route_nodes = targets.iter().fold(0usize, |nodes, target| {
            nodes.saturating_add(target.through.len().saturating_add(1))
        });
        self.context
            .admit_terminal_work(route_nodes, 0, Some(0))
            .map_err(|()| SceneCompilationFailureKind::WorkLimit)?;
        let route_bytes = targets.iter().try_fold(0usize, |bytes, target| {
            bytes.checked_add(target.logical_bytes_up_to(usize::MAX)?)
        });
        self.context
            .admit_terminal_work(0, route_bytes.unwrap_or(usize::MAX), Some(0))
            .map_err(|()| SceneCompilationFailureKind::WorkLimit)?;

        let selected = self.entered_target(0, apply.selected)?;
        let destination = self.entered_target(0, apply.destination)?;
        let selected_node = self.canonical_node(selected_scope, selected);
        let both_survive = [selected_scope, 0].into_iter().all(|scope| {
            let output = &self.scopes[scope].output;
            output.contains(&selected) && output.contains(&destination)
        });
        if !both_survive || self.canonical_node(selected_scope, destination) != selected_node {
            return Err(SceneCompilationFailureKind::MissingTarget.into());
        }
        let destination_record = self.nodes[destination].record;
        let Record::Source {
            scope: destination_scope,
            ..
        } = destination_record
        else {
            return Err(SceneCompilationFailureKind::Component.into());
        };
        if destination_scope == 0
            || self.scopes[destination_scope]
                .source
                .as_ref()
                .is_none_or(|source| !std::ptr::eq(source.document(), apply.destination_document))
        {
            return Err(SceneCompilationFailureKind::SourceChanged.into());
        }
        let slot = apply
            .declaration
            .map(|index| PrefabOverlaySlot::Component { index });
        self.verify_component_apply(apply, selected_scope, selected_node, destination_record)
            .map_err(|failure| self.scope_failure(selected_scope, slot, failure))?;
        Ok(destination_scope)
    }

    fn verify_component_apply(
        &mut self,
        apply: ComponentApplyScope<'_>,
        selected_scope: ScopeId,
        selected_node: NodeId,
        destination: Record,
    ) -> Result<(), SceneCompilationFailure> {
        let lookup_nodes = self.selected_component_overrides.len().saturating_add(1);
        let lookup_bytes = lookup_nodes.saturating_mul(apply.component.as_str().len());
        self.context
            .admit_terminal_work(lookup_nodes, lookup_bytes, Some(0))
            .map_err(|()| SceneCompilationFailureKind::WorkLimit)?;
        let overlay = self.overlay(selected_scope);
        let declaration = self.selected_component_overrides.iter().find_map(|entry| {
            if entry.node != selected_node {
                return None;
            }
            let intent = overlay?.components.get(entry.index)?;
            (intent.address().1 == apply.component).then_some((entry, intent))
        });
        let current = self.nodes[self.canonical_node(0, selected_node)]
            .programs
            .get(apply.component)
            .map(|program| program.record);
        let (observed, intent) = match (apply.declaration, declaration) {
            (None, None) => {
                let expected = self
                    .components(destination)
                    .contains_key(apply.component)
                    .then_some(destination);
                return (current == expected)
                    .then_some(())
                    .ok_or_else(|| SceneCompilationFailureKind::Component.into());
            }
            (Some(index), Some((observed, intent))) if index == observed.index => {
                (observed, intent)
            }
            _ => return Err(SceneCompilationFailureKind::Component.into()),
        };
        let exact_lower = observed.underlying == Some(destination);
        let valid = match intent {
            PrefabComponentIntent::Set { mode, .. } => {
                let lower_matches = match mode {
                    PrefabComponentSetMode::Add => {
                        observed.underlying.is_none()
                            && !self.components(destination).contains_key(apply.component)
                    }
                    PrefabComponentSetMode::Replace => exact_lower,
                };
                lower_matches
                    && current
                        == Some(Record::Set {
                            scope: selected_scope,
                            index: observed.index,
                        })
            }
            PrefabComponentIntent::Suppress { .. } => exact_lower && current.is_none(),
            PrefabComponentIntent::Properties { .. } => false,
        };
        valid
            .then_some(())
            .ok_or_else(|| SceneCompilationFailureKind::Component.into())
    }
}
