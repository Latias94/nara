//! Borrowed relocation demand for disk guards, including dormant physical declarations.

use std::borrow::Cow;

use super::super::super::payload::{visit_property_references, visit_record_references};
use super::super::super::relocation::AddressRole;
#[cfg(test)]
mod tests;
use super::*;
use crate::patch::SourceCostObserver;
use crate::{
    PrefabComponentIntent, PrefabEntityTarget, PrefabInstance, PrefabOverlay, SceneComponentRecord,
    SceneEntityPlacement, ScenePlacementParent, ScenePlacementSlot,
};
use nara_identity::EntityReference;
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId};

pub(super) fn requires_relocation(
    source: SourceDocumentInput<'_>,
    key: &SourceDocumentKey,
    relocation: &BranchRelocation<'_, '_>,
    basis: &AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    let remaining = work.traversal()?;
    let limits = crate::ScenePatchApplyLimits::default()
        .with_validation_work(remaining.nodes())
        .with_validation_value_nodes(remaining.nodes())
        .with_validation_value_bytes(remaining.bytes());
    let mut observer = RelocationObserver {
        key,
        relocation,
        basis,
        preserve_unavailable,
        work,
        required: false,
        error: None,
    };
    let cost = crate::patch::bounded_source_cost_with_observer(source, limits, &mut observer);
    if let Some(error) = observer.error {
        return Err(error);
    }
    let required = observer.required;
    cost.ok_or(ExtractionError::Capacity)?;
    Ok(required)
}

struct RelocationObserver<'a, 'cut, 'source> {
    key: &'a SourceDocumentKey,
    relocation: &'a BranchRelocation<'cut, 'source>,
    basis: &'a AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &'a mut ExtractionWork,
    required: bool,
    error: Option<ExtractionError>,
}

impl RelocationObserver<'_, '_, '_> {
    fn finish(&mut self, result: Result<(), ExtractionError>) -> bool {
        match result {
            Ok(()) => true,
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    fn target(
        &mut self,
        scope: &SourceDocumentKey,
        target: &PrefabEntityTarget,
        role: AddressRole,
    ) -> Result<(), ExtractionError> {
        self.work.visit(1, 0, 0)?;
        self.required |= matches!(
            self.relocation
                .retained_target(scope, target, role, |bytes| self.work.admit(bytes))?,
            Cow::Owned(_)
        );
        Ok(())
    }

    fn placement(
        &mut self,
        scope: &SourceDocumentKey,
        placement: &SceneEntityPlacement,
    ) -> Result<(), ExtractionError> {
        self.target(scope, &placement.target, AddressRole::Entity)?;
        if let ScenePlacementParent::Entity(target) = &placement.placement.parent {
            self.target(scope, target, AddressRole::Entity)?;
        }
        if let ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) =
            &placement.placement.slot
        {
            self.target(scope, target, AddressRole::Entity)?;
        }
        Ok(())
    }

    fn record_references(
        &mut self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> Result<(), ExtractionError> {
        let mut changed = false;
        visit_record_references(
            component,
            record,
            self.basis,
            self.preserve_unavailable,
            self.work,
            &mut |reference, work| {
                reference_changed(self.relocation, self.key, reference, work, &mut changed)
            },
        )?;
        self.required |= changed;
        Ok(())
    }

    fn property_references(
        &mut self,
        component: &ComponentTypeId,
        version: ComponentSchemaVersion,
        claims: &[crate::PrefabPropertyClaim],
    ) -> Result<(), ExtractionError> {
        let mut changed = false;
        visit_property_references(
            component,
            version,
            claims,
            self.basis,
            self.preserve_unavailable,
            self.work,
            &mut |reference, work| {
                reference_changed(self.relocation, self.key, reference, work, &mut changed)
            },
        )?;
        self.required |= changed;
        Ok(())
    }
}

fn reference_changed(
    relocation: &BranchRelocation<'_, '_>,
    key: &SourceDocumentKey,
    reference: &EntityReference,
    work: &mut ExtractionWork,
    changed: &mut bool,
) -> Result<(), ExtractionError> {
    work.visit(1, 0, 0)?;
    *changed |= matches!(
        relocation.retained_reference(key, reference, |bytes| work.admit(bytes))?,
        Cow::Owned(_)
    );
    Ok(())
}

impl SourceCostObserver for RelocationObserver<'_, '_, '_> {
    fn progress(&mut self, nodes: usize, logical_bytes: usize) -> bool {
        let result = self.work.settle_raw_progress(nodes, logical_bytes);
        self.finish(result)
    }

    fn admit_scratch(&mut self, bytes: Option<usize>) -> bool {
        let result = self.work.admit(bytes);
        self.finish(result)
    }

    fn source_placements(&mut self, placements: &[SceneEntityPlacement]) -> bool {
        let result = (|| {
            for placement in placements {
                self.placement(self.key, placement)?;
            }
            Ok(())
        })();
        self.finish(result)
    }

    fn instance(&mut self, instance: &PrefabInstance) -> bool {
        let result = (|| {
            self.work.visit(1, 0, 0)?;
            self.required |= matches!(
                self.relocation
                    .retained_binding(&instance.binding, |bytes| self.work.admit(bytes))?,
                Cow::Owned(_)
            );
            if let Some(arguments) = &instance.arguments {
                for reference in arguments.values.values() {
                    reference_changed(
                        self.relocation,
                        self.key,
                        reference,
                        self.work,
                        &mut self.required,
                    )?;
                }
            }
            Ok(())
        })();
        self.finish(result)
    }

    fn overlay(&mut self, overlay: &PrefabOverlay) -> bool {
        let result = (|| {
            for claim in &overlay.instances {
                self.target(&overlay.base, &claim.target, AddressRole::InstanceSlot)?;
            }
            for intent in &overlay.components {
                let target = match intent {
                    PrefabComponentIntent::Set { target, .. }
                    | PrefabComponentIntent::Suppress { target, .. }
                    | PrefabComponentIntent::Properties { target, .. } => target,
                };
                self.target(&overlay.base, target, AddressRole::Entity)?;
            }
            for placement in &overlay.placements {
                self.placement(&overlay.base, placement)?;
            }
            for target in &overlay.suppressions {
                self.target(&overlay.base, target, AddressRole::Entity)?;
            }
            Ok(())
        })();
        self.finish(result)
    }

    fn record(&mut self, component: &ComponentTypeId, record: &SceneComponentRecord) -> bool {
        let result = self.record_references(component, record);
        self.finish(result)
    }

    fn component_intent(&mut self, intent: &PrefabComponentIntent) -> bool {
        if self.preserve_unavailable {
            match self.work.source_intent_unavailable(intent, self.basis) {
                Ok(true) => return true,
                Ok(false) => {}
                Err(error) => return self.finish(Err(error)),
            }
        }
        let result = match intent {
            PrefabComponentIntent::Properties {
                component,
                version,
                claims,
                ..
            } => self.property_references(component, *version, claims),
            _ => Ok(()),
        };
        self.finish(result)
    }
}
