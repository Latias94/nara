//! Observes the surviving programs and canonical routes before compiler state is released.

use super::inputs::ResolvedReference;
use super::reference_program::{ReferenceProgram, ReferencePrograms};
use super::*;
use crate::prefab::inspection::{
    AuthoringPrefabComponentFact, AuthoringPrefabComponentOverrideKind,
    AuthoringPrefabPropertyClaim, AuthoringPrefabPropertyClaimKind, AuthoringPrefabRecordOrigin,
    CapturedPrefabInspection, InspectedComponent, InspectedComponentOwnership,
    InspectedPrefabComponentOverride, InspectedPrefabPropertyApply, InspectedUnderlyingComponent,
};
use crate::{
    AuthoringRecordOwnerClaim, AuthoringRecordReadiness, PrefabPropertyClaim, PrefabPropertyIntent,
};
use nara_ecs::identity::EntityReference;
use nara_reflect::{
    ComponentFieldId, ComponentFieldPathSegment, ComponentFieldSchema, ComponentPropertyStep,
    ComponentValue, ComponentValueRef,
};

const TREE_ENTRY_BYTES: usize = 6 * size_of::<usize>();

fn component_ownership(record: Record, selected_scope: ScopeId) -> InspectedComponentOwnership {
    match record {
        Record::Source { scope: 0, .. } => InspectedComponentOwnership::LocalRecord,
        Record::Addition { scope, .. } if scope == selected_scope => {
            InspectedComponentOwnership::LocalRecord
        }
        Record::Set { scope, .. } if scope == selected_scope => {
            InspectedComponentOwnership::SelectedOverride
        }
        _ => InspectedComponentOwnership::Inherited,
    }
}

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(super) fn capture_inspection(
        &mut self,
        identifiers: &BTreeMap<SceneEntityId, SceneEntityId>,
        references: &ReferencePrograms,
        payload_bytes: usize,
    ) -> Result<CapturedPrefabInspection, SceneCompilationFailure> {
        let selected_scope = self
            .selected_anchor
            .and_then(|anchor| self.nodes[anchor].entered)
            .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
        let mut result = CapturedPrefabInspection {
            retained_bytes: payload_bytes,
            ..Default::default()
        };
        let bytes = self.scopes[selected_scope]
            .output
            .iter()
            .filter(|&&facet| self.scopes[0].output.contains(&facet))
            .try_fold(0usize, |bytes, &facet| {
                let id = identifiers.get(&self.nodes[facet].locator)?;
                bytes
                    .checked_add(TREE_ENTRY_BYTES + size_of::<SceneEntityId>())?
                    .checked_add(self.relative_bytes(0, facet)?)?
                    .checked_add(provenance_string_bytes(id.as_str())?)
            });
        self.retain_inspection(&mut result, bytes)?;
        for &facet in &self.scopes[selected_scope].output {
            if !self.scopes[0].output.contains(&facet) {
                continue;
            }
            let target = self
                .relative(0, facet)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            let id = identifiers
                .get(&self.nodes[facet].locator)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            result.targets.insert(target, id.clone());
        }

        for SelectedComponentDeclaration {
            node,
            index,
            underlying,
        } in std::mem::take(&mut self.selected_component_overrides)
        {
            if !self.scopes[0].output.contains(&node) {
                continue;
            }
            let id = identifiers
                .get(&self.nodes[node].locator)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            let intent = self
                .overlay(selected_scope)
                .and_then(|overlay| overlay.components.get(index))
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            let underlying_bytes = match underlying {
                Some(record) => self.record_origin_bytes(record),
                None => Some(0),
            };
            self.retain_inspection(
                &mut result,
                provenance_string_bytes(intent.address().1.as_str()).and_then(|bytes| {
                    bytes
                        .checked_add(
                            2 * TREE_ENTRY_BYTES + size_of::<InspectedPrefabComponentOverride>(),
                        )?
                        .checked_add(provenance_string_bytes(id.as_str())?)?
                        .checked_add(underlying_bytes?)
                }),
            )?;
            let underlying = underlying
                .map(|record| {
                    Ok::<_, SceneCompilationFailure>(InspectedUnderlyingComponent {
                        origin: self.record_origin(record)?,
                        ownership: component_ownership(record, selected_scope),
                    })
                })
                .transpose()?;
            let intent = &self.overlay(selected_scope).unwrap().components[index];
            result
                .component_overrides
                .entry(id.clone())
                .or_default()
                .insert(
                    intent.address().1.clone(),
                    InspectedPrefabComponentOverride {
                        index,
                        kind: AuthoringPrefabComponentOverrideKind::of(intent),
                        underlying,
                    },
                );
        }

        // ReferencePrograms indexes the actual materialized selected entities, including empty
        // ones. Its keys already account for root merging and do not name suppressed records.
        for (&node, programs) in references {
            let id = identifiers
                .get(&self.nodes[node].locator)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            self.retain_inspection(
                &mut result,
                provenance_string_bytes(id.as_str()).and_then(|bytes| {
                    bytes.checked_add(
                        TREE_ENTRY_BYTES
                            + size_of::<SceneEntityId>()
                            + size_of::<BTreeMap<ComponentTypeId, InspectedComponent>>(),
                    )
                }),
            )?;
            let mut components = BTreeMap::new();
            for (component, references) in programs {
                let observation = self.capture_inspected_component(
                    &mut result,
                    selected_scope,
                    node,
                    component,
                    &references.value,
                )?;
                components.insert(component.clone(), observation);
                for origin in references.value.origins.values() {
                    if let Some(ResolvedReference::Local(target)) = &origin.resolved {
                        self.capture_inspected_reference(&mut result, identifiers, *target)?;
                    }
                }
            }
            result.entities.insert(id.clone(), components);
        }
        Ok(result)
    }

    fn capture_inspected_reference(
        &mut self,
        result: &mut CapturedPrefabInspection,
        identifiers: &BTreeMap<SceneEntityId, SceneEntityId>,
        target: NodeId,
    ) -> Result<(), SceneCompilationFailure> {
        let canonical = self.canonical_node(0, target);
        let id = identifiers
            .get(&self.nodes[canonical].locator)
            .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
        if result.references.contains_key(id) {
            return Ok(());
        }
        self.retain_inspection(
            result,
            self.relative_bytes(0, canonical).and_then(|bytes| {
                bytes
                    .checked_add(
                        TREE_ENTRY_BYTES
                            + size_of::<SceneEntityId>()
                            + size_of::<EntityReference>(),
                    )?
                    .checked_add(provenance_string_bytes(id.as_str())?)
            }),
        )?;
        let route = self
            .relative(0, canonical)
            .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
        result.references.insert(
            id.clone(),
            EntityReference::Authored {
                reference: route.into(),
            },
        );
        Ok(())
    }

    fn capture_inspected_component(
        &mut self,
        result: &mut CapturedPrefabInspection,
        selected_scope: ScopeId,
        node: NodeId,
        component: &ComponentTypeId,
        references: &ReferenceProgram,
    ) -> Result<InspectedComponent, SceneCompilationFailure> {
        let program = &self.nodes[node].programs[component];
        let original = self.component(program.record, component);
        let readiness = self
            .context
            .authority
            .unavailable_reason(component, original)
            .unwrap_or(AuthoringRecordReadiness::Available);
        let schema = self
            .context
            .authority
            .schema(component)
            .filter(|_| readiness.is_available());
        let fields = schema.map_or(&[][..], |schema| schema.fields());

        let header_bytes = self.record_origin_bytes(program.record).and_then(|bytes| {
            bytes
                .checked_add(
                    TREE_ENTRY_BYTES
                        + size_of::<ComponentTypeId>()
                        + size_of::<InspectedComponent>(),
                )?
                .checked_add(provenance_string_bytes(component.as_str())?.checked_mul(2)?)?
                .checked_add(
                    original
                        .owner()
                        .map_or(Some(0), |owner| provenance_string_bytes(owner.as_str()))?,
                )
        });
        let mut defaults = fields
            .iter()
            .filter(|field| uses_record_default(original.value(), field));
        let mut default_count = 0;
        let default_bytes = defaults.try_fold(0usize, |total, field| {
            default_count += 1;
            total
                .checked_add(size_of::<ComponentFieldId>())?
                .checked_add(provenance_string_bytes(field.id().as_str())?)
        });
        let mut claim_count = 0usize;
        let claim_bytes = program
            .properties
            .iter()
            .try_fold(0usize, |bytes, &(scope, index)| {
                let site = self.scopes[scope].site.as_ref()?;
                let site_bytes = self.site_bytes(site)?;
                let (_, claims) = self.inspection_properties(scope, index)?;
                claim_count = claim_count.checked_add(claims.len())?;
                claims.iter().try_fold(bytes, |bytes, claim| {
                    let steps = claim.path.steps.len();
                    bytes
                        .checked_add(size_of::<AuthoringPrefabPropertyClaim>())?
                        .checked_add(site_bytes)?
                        .checked_add(claim.path.logical_bytes().ok()?)?
                        .checked_add(steps.checked_mul(size_of::<ComponentPropertyStep>())?)?
                        .checked_add(steps.checked_mul(4 * size_of::<usize>())?)
                })
            });
        self.retain_inspection(
            result,
            header_bytes
                .and_then(|bytes| bytes.checked_add(default_bytes?)?.checked_add(claim_bytes?)),
        )?;

        let program = &self.nodes[node].programs[component];
        let original = self.component(program.record, component);
        let mut defaulted_roots = Vec::with_capacity(default_count);
        defaulted_roots.extend(fields.iter().filter_map(|field| {
            uses_record_default(original.value(), field).then(|| field.id().clone())
        }));
        let mut claims = Vec::with_capacity(claim_count);
        for &(scope, component_index) in &program.properties {
            let site = self.scopes[scope]
                .site
                .as_ref()
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            let (_, applied) = self
                .inspection_properties(scope, component_index)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            for (claim_index, claim) in applied.iter().enumerate() {
                claims.push(AuthoringPrefabPropertyClaim {
                    path: claim.path.clone(),
                    location: self.stable_site(site),
                    slot: PrefabOverlaySlot::Property {
                        component_index,
                        claim_index,
                    },
                    kind: match &claim.intent {
                        PrefabPropertyIntent::Set(_) => AuthoringPrefabPropertyClaimKind::Set,
                        PrefabPropertyIntent::Absent => AuthoringPrefabPropertyClaimKind::Absent,
                    },
                });
            }
        }
        let selected_overlay_intent = program
            .properties
            .iter()
            .find_map(|&(scope, index)| (scope == selected_scope).then_some(index));
        let record = program.record;
        let mut observation = InspectedComponent {
            fact: AuthoringPrefabComponentFact {
                component: component.clone(),
                owner: original.owner().cloned().map_or(
                    AuthoringRecordOwnerClaim::Unclaimed,
                    AuthoringRecordOwnerClaim::Declared,
                ),
                version: original.version,
                readiness,
                record: self.record_origin(record)?,
            },
            claims,
            defaulted_roots,
            payload: None,
            selected_overlay_intent,
            ownership: component_ownership(record, selected_scope),
            property_apply: None,
        };
        if readiness.is_available() {
            observation.property_apply = self.capture_property_apply(
                result,
                selected_scope,
                record,
                selected_overlay_intent,
                references,
            )?;
        }
        Ok(observation)
    }

    /// Uses the concrete component declaration even when root binding merges several facets.
    fn capture_property_apply(
        &mut self,
        result: &mut CapturedPrefabInspection,
        selected_scope: ScopeId,
        record: Record,
        selected_intent: Option<usize>,
        references: &ReferenceProgram,
    ) -> Result<Option<Box<InspectedPrefabPropertyApply>>, SceneCompilationFailure> {
        let Record::Source { scope, entity } = record else {
            return Ok(None);
        };
        if scope == 0 {
            return Ok(None);
        }
        let facet = *self.scopes[scope]
            .nodes
            .source_entity(&self.source_entities(scope)[entity].id)
            .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
        let claim_bytes = selected_intent.map_or(Some(0), |index| {
            let (target, _) = self.inspection_properties(selected_scope, index)?;
            target_clone_bytes(target)
        });
        self.retain_inspection(
            result,
            self.relative_bytes(0, facet).and_then(|bytes| {
                bytes
                    .checked_add(claim_bytes?)?
                    .checked_add(size_of::<InspectedPrefabPropertyApply>())
            }),
        )?;
        let record_target = self
            .relative(0, facet)
            .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
        let claim_target = selected_intent
            .and_then(|index| self.inspection_properties(selected_scope, index))
            .map(|(target, _)| target.clone());
        let mut reference_ownership: BTreeMap<PrefabOverlaySlot, bool> = BTreeMap::new();
        if let Some(index) = selected_intent {
            for origin in references.origins.values() {
                let Some((
                    layer,
                    slot @ PrefabOverlaySlot::Property {
                        component_index,
                        claim_index,
                    },
                )) = origin.declaration
                else {
                    continue;
                };
                if layer != selected_scope || component_index != index {
                    continue;
                }
                if let Some(owned) = reference_ownership.get_mut(&slot) {
                    *owned &= origin.destination_owns_route;
                } else if self
                    .inspection_properties(selected_scope, index)
                    .and_then(|(_, claims)| claims.get(claim_index))
                    .is_some_and(|claim| matches!(claim.intent, PrefabPropertyIntent::Set(_)))
                {
                    self.retain_inspection(
                        result,
                        Some(TREE_ENTRY_BYTES + size_of::<(PrefabOverlaySlot, bool)>()),
                    )?;
                    reference_ownership.insert(slot, origin.destination_owns_route);
                }
            }
        }
        Ok(Some(Box::new(InspectedPrefabPropertyApply {
            record_target,
            claim_target,
            reference_ownership,
        })))
    }

    pub(super) fn inspection_properties(
        &self,
        scope: ScopeId,
        index: usize,
    ) -> Option<(&PrefabEntityTarget, &[PrefabPropertyClaim])> {
        match self.overlay(scope)?.components.get(index)? {
            PrefabComponentIntent::Properties { target, claims, .. } => Some((target, claims)),
            _ => None,
        }
    }

    fn record_origin_bytes(&self, record: Record) -> Option<usize> {
        match record {
            Record::Source { scope, entity } => {
                let id = &self.source_entities(scope)[entity].id;
                let source = self.scopes[scope].source.as_ref();
                provenance_string_bytes(id.as_str())?.checked_add(
                    source.map_or(Some(0), |source| source.key().checked_retained_bytes())?,
                )
            }
            Record::Addition { scope, .. } | Record::Set { scope, .. } => {
                self.site_bytes(self.scopes[scope].site.as_ref()?)
            }
        }
    }

    fn record_origin(
        &self,
        record: Record,
    ) -> Result<AuthoringPrefabRecordOrigin, SceneCompilationFailure> {
        let (scope, slot) = match record {
            Record::Source { scope, entity } => {
                let source = self.scopes[scope]
                    .source
                    .as_ref()
                    .map_or(PrefabDeclarationSource::Containing, |source| {
                        PrefabDeclarationSource::Prefab(source.key().clone())
                    });
                return Ok(AuthoringPrefabRecordOrigin::SourceEntity {
                    source,
                    entity: self.source_entities(scope)[entity].id.clone(),
                });
            }
            Record::Addition { scope, index } => (scope, PrefabOverlaySlot::Addition { index }),
            Record::Set { scope, index } => (scope, PrefabOverlaySlot::Component { index }),
        };
        let site = self.scopes[scope]
            .site
            .as_ref()
            .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
        Ok(AuthoringPrefabRecordOrigin::Overlay {
            location: self.stable_site(site),
            slot,
        })
    }

    fn retain_inspection(
        &mut self,
        result: &mut CapturedPrefabInspection,
        bytes: Option<usize>,
    ) -> Result<(), SceneCompilationFailure> {
        self.admit(bytes)?;
        result.retained_bytes = bytes
            .and_then(|bytes| result.retained_bytes.checked_add(bytes))
            .ok_or(SceneCompilationFailureKind::WorkLimit)?;
        Ok(())
    }
}

// The compiler already proved defaults and their effective values. A record's authored
// presence is observed before those insertions, independently of the resulting value.
fn uses_record_default(value: &ComponentValue, field: &ComponentFieldSchema) -> bool {
    field.default_value().is_some()
        && field
            .path()
            .segments()
            .iter()
            .try_fold(
                ComponentValueRef::from(value),
                |value, segment| match segment {
                    ComponentFieldPathSegment::Field(name) => value.get(name),
                    ComponentFieldPathSegment::Index(index) => {
                        value.as_list()?.get(usize::try_from(*index).ok()?)
                    }
                },
            )
            .is_none()
}
