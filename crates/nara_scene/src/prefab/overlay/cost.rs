//! Borrowed raw-storage accounting, independent of activation and Schema availability.

#[cfg(test)]
use nara_asset::AssetRef;
use nara_reflect::{
    ComponentPropertyKey, ComponentPropertyStep, ComponentValue, ComponentValueCost,
};

use super::{
    PrefabAddition, PrefabComponentIntent, PrefabEntityTarget, PrefabOverlay,
    PrefabPlacementParent, PrefabPlacementSlot, PrefabPropertyIntent,
};
use crate::provenance::{CostScratchAdmission, reserve_cost_scratch};
use crate::{PrefabInstance, SceneComponentRecord, SourceDocumentKey, SourceDocumentOrigin};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PrefabOverlayCost {
    /// Actual payload values only; selector keys and metadata remain structural bytes.
    pub(crate) value_cost: ComponentValueCost,
    pub(crate) structural_logical_bytes: usize,
    /// Raw declaration/target/path/value nodes, suitable for a pre-copy work ceiling.
    pub(crate) nodes: usize,
    /// Maximum semantic storage depth, including nested values and instance declarations.
    pub(crate) depth: usize,
    /// Each overlay, row, property claim, argument block/input and contextual default.
    pub(crate) declarations: usize,
    pub(crate) entities: usize,
    pub(crate) placements: usize,
    pub(crate) components: usize,
    pub(crate) prefab_instances: usize,
}

impl PrefabOverlayCost {
    pub(crate) const fn logical_bytes(self) -> usize {
        self.structural_logical_bytes
            .saturating_add(self.value_cost.logical_bytes())
    }
}

/// Inclusive ceilings. Subtract already retained aggregate costs before walking.
/// Logical bytes model raw content, not allocator ownership or retained owner bytes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrefabOverlayCostLimits {
    pub(crate) nodes: usize,
    pub(crate) depth: usize,
    pub(crate) declarations: usize,
    pub(crate) entities: usize,
    pub(crate) placements: usize,
    pub(crate) components: usize,
    pub(crate) prefab_instances: usize,
    pub(crate) value_nodes: usize,
    pub(crate) value_bytes: usize,
    pub(crate) logical_bytes: usize,
}

impl PrefabOverlayCostLimits {
    pub(crate) const UNBOUNDED: Self = Self {
        nodes: usize::MAX,
        depth: usize::MAX,
        declarations: usize::MAX,
        entities: usize::MAX,
        placements: usize::MAX,
        components: usize::MAX,
        prefab_instances: usize::MAX,
        value_nodes: usize::MAX,
        value_bytes: usize::MAX,
        logical_bytes: usize::MAX,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrefabOverlayCostKind {
    Nodes,
    Depth,
    Declarations,
    Entities,
    Placements,
    Components,
    PrefabInstances,
    ValueNodes,
    ValueBytes,
    LogicalBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrefabOverlayCostError {
    pub(crate) kind: PrefabOverlayCostKind,
    pub(crate) observed: usize,
    pub(crate) maximum: usize,
}

pub(crate) fn prefab_instance_cost(
    instance: &PrefabInstance,
    limits: PrefabOverlayCostLimits,
) -> Result<PrefabOverlayCost, PrefabOverlayCostError> {
    CostWalker::new(limits, None).walk(Frame::Instance(instance, 1))
}

/// `Ok(None)` is scratch refusal; the outer cost owner retains the callback's typed error.
pub(crate) fn prefab_instance_cost_with_admission(
    instance: &PrefabInstance,
    limits: PrefabOverlayCostLimits,
    admit: &mut CostScratchAdmission<'_>,
) -> Result<Option<PrefabOverlayCost>, PrefabOverlayCostError> {
    let mut refused = false;
    let result = {
        let mut callback = |bytes| {
            let accepted = admit(bytes);
            refused |= !accepted;
            accepted
        };
        CostWalker::new(limits, Some(&mut callback)).walk(Frame::Instance(instance, 1))
    };
    if refused { Ok(None) } else { result.map(Some) }
}

pub(crate) fn prefab_overlay_cost(
    overlay: &PrefabOverlay,
    limits: PrefabOverlayCostLimits,
) -> Result<PrefabOverlayCost, PrefabOverlayCostError> {
    CostWalker::new(limits, None).walk(Frame::Overlay(overlay, 1))
}

// Iterator frames keep auxiliary storage proportional to nesting, rather than fan-out.
enum Frame<'a> {
    Instance(&'a PrefabInstance, usize),
    Overlay(&'a PrefabOverlay, usize),
    Additions(std::slice::Iter<'a, PrefabAddition>, usize),
    Instances(std::slice::Iter<'a, super::PrefabInstanceClaim>, usize),
}

struct CostWalker<'a, 'environment> {
    cost: PrefabOverlayCost,
    limits: PrefabOverlayCostLimits,
    admission: Option<&'a mut CostScratchAdmission<'environment>>,
}

impl<'a, 'environment> CostWalker<'a, 'environment> {
    fn new(
        limits: PrefabOverlayCostLimits,
        admission: Option<&'a mut CostScratchAdmission<'environment>>,
    ) -> Self {
        Self {
            cost: PrefabOverlayCost::default(),
            limits,
            admission,
        }
    }

    fn walk(mut self, first: Frame<'_>) -> Result<PrefabOverlayCost, PrefabOverlayCostError> {
        let mut pending = if self.admission.is_some() {
            let mut pending = Vec::new();
            self.reserve_scratch(&mut pending, 1)?;
            pending.push(first);
            pending
        } else {
            vec![first]
        };
        while let Some(frame) = pending.pop() {
            match frame {
                Frame::Instance(instance, depth) => {
                    self.node(depth)?;
                    add(
                        &mut self.cost.prefab_instances,
                        1,
                        self.limits.prefab_instances,
                        PrefabOverlayCostKind::PrefabInstances,
                    )?;
                    // Preserve the exact patch owner's AssetRef wire-value convention:
                    // map keys, kind spelling and payload, without formatting/copying it.
                    self.bytes(crate::patch::asset_ref_value_size(&instance.source).1)?;
                    if let Some(arguments) = &instance.arguments {
                        self.node(depth.saturating_add(1))?;
                        self.declarations(1)?;
                        self.source(&arguments.base, depth.saturating_add(2))?;
                        self.node(depth.saturating_add(2))?;
                        self.declarations(arguments.values.len())?;
                        for (input, reference) in &arguments.values {
                            self.node(depth.saturating_add(2))?;
                            self.bytes(input.as_str().len())?;
                            let cost = reference.cost();
                            self.nodes(cost.nodes(), depth.saturating_add(3))?;
                            self.bytes(cost.logical_bytes())?;
                        }
                    }
                    if let Some(overlay) = &instance.overrides {
                        self.reserve_scratch(&mut pending, 1)?;
                        pending.push(Frame::Overlay(overlay, depth.saturating_add(1)));
                    }
                }
                Frame::Overlay(overlay, depth) => {
                    self.node(depth)?;
                    self.declarations(1)?;
                    self.source(&overlay.base, depth.saturating_add(1))?;
                    self.declarations(overlay.additions.len())?;
                    add(
                        &mut self.cost.entities,
                        overlay.additions.len(),
                        self.limits.entities,
                        PrefabOverlayCostKind::Entities,
                    )?;
                    self.declarations(overlay.instances.len())?;
                    self.declarations(overlay.components.len())?;
                    self.declarations(overlay.placements.len())?;
                    self.declarations(overlay.suppressions.len())?;
                    add(
                        &mut self.cost.placements,
                        overlay.placements.len(),
                        self.limits.placements,
                        PrefabOverlayCostKind::Placements,
                    )?;
                    let child_depth = depth.saturating_add(1);
                    for component in &overlay.components {
                        self.component(component, child_depth)?;
                    }
                    for placement in &overlay.placements {
                        self.node(child_depth)?;
                        self.target(&placement.target, child_depth)?;
                        if let PrefabPlacementParent::Entity(target) = &placement.parent {
                            self.target(target, child_depth)?;
                        }
                        if let PrefabPlacementSlot::Before(target)
                        | PrefabPlacementSlot::After(target) = &placement.slot
                        {
                            self.target(target, child_depth)?;
                        }
                        self.bytes(2)?;
                    }
                    for target in &overlay.suppressions {
                        self.target(target, child_depth)?;
                    }
                    self.reserve_scratch(&mut pending, 2)?;
                    pending.push(Frame::Instances(overlay.instances.iter(), child_depth));
                    pending.push(Frame::Additions(overlay.additions.iter(), child_depth));
                }
                Frame::Additions(mut additions, depth) => {
                    if let Some(addition) = additions.next() {
                        self.reserve_scratch(&mut pending, 2)?;
                        pending.push(Frame::Additions(additions, depth));
                        self.node(depth)?;
                        self.bytes(addition.id.as_str().len())?;
                        self.components(addition.components.len())?;
                        for (component, record) in &addition.components {
                            self.bytes(component.as_str().len())?;
                            self.record(record, depth.saturating_add(1))?;
                        }
                        if let Some(instance) = &addition.prefab {
                            pending.push(Frame::Instance(instance, depth.saturating_add(1)));
                        }
                    }
                }
                Frame::Instances(mut instances, depth) => {
                    if let Some(claim) = instances.next() {
                        self.reserve_scratch(&mut pending, 2)?;
                        pending.push(Frame::Instances(instances, depth));
                        self.node(depth)?;
                        self.target(&claim.target, depth)?;
                        self.bytes(1)?;
                        if let Some(instance) = &claim.value {
                            pending.push(Frame::Instance(instance, depth.saturating_add(1)));
                        }
                    }
                }
            }
        }
        Ok(self.cost)
    }

    fn reserve_scratch<T>(
        &mut self,
        values: &mut Vec<T>,
        additional: usize,
    ) -> Result<(), PrefabOverlayCostError> {
        reserve_cost_scratch(values, additional, &mut self.admission)
            .ok_or_else(|| self.scratch_refusal())
    }

    fn scratch_refusal(&self) -> PrefabOverlayCostError {
        // Internal unwind only. The admitted entry converts this to Ok(None), and its outer
        // caller returns the exact admission error instead of exposing a semantic limit.
        PrefabOverlayCostError {
            kind: PrefabOverlayCostKind::Nodes,
            observed: self.cost.nodes,
            maximum: self.limits.nodes,
        }
    }

    fn component(
        &mut self,
        intent: &PrefabComponentIntent,
        depth: usize,
    ) -> Result<(), PrefabOverlayCostError> {
        self.node(depth)?;
        self.components(1)?;
        self.bytes(1)?;
        match intent {
            PrefabComponentIntent::Set {
                target,
                component,
                record,
                ..
            } => {
                self.target(target, depth)?;
                self.bytes(component.as_str().len().saturating_add(1))?;
                self.record(record, depth.saturating_add(1))?;
            }
            PrefabComponentIntent::Suppress { target, component } => {
                self.target(target, depth)?;
                self.bytes(component.as_str().len())?;
            }
            PrefabComponentIntent::Properties {
                target,
                component,
                claims,
                ..
            } => {
                self.target(target, depth)?;
                self.bytes(component.as_str().len().saturating_add(4))?;
                self.declarations(claims.len())?;
                for claim in claims {
                    self.node(depth.saturating_add(1))?;
                    self.bytes(claim.path.root.as_str().len())?;
                    for step in &claim.path.steps {
                        self.node(depth.saturating_add(2))?;
                        self.bytes(1)?;
                        self.bytes(match step {
                            ComponentPropertyStep::Member(member) => member.as_str().len(),
                            ComponentPropertyStep::MapEntry(key) => key.len(),
                            ComponentPropertyStep::ListItem { by, key } => {
                                by.as_str().len().saturating_add(match key {
                                    ComponentPropertyKey::String(key) => key.len(),
                                    ComponentPropertyKey::I64(_) | ComponentPropertyKey::U64(_) => {
                                        8
                                    }
                                })
                            }
                        })?;
                    }
                    self.bytes(1)?;
                    if let PrefabPropertyIntent::Set(value) = &claim.intent {
                        self.value(value, depth.saturating_add(2))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn target(
        &mut self,
        target: &PrefabEntityTarget,
        depth: usize,
    ) -> Result<(), PrefabOverlayCostError> {
        self.node(depth)?;
        self.bytes(target.entity.entity().as_str().len().saturating_add(1))?;
        for hop in &target.through {
            self.node(depth.saturating_add(1))?;
            self.bytes(hop.anchor.entity().as_str().len().saturating_add(1))?;
            self.source(&hop.expected_base, depth.saturating_add(2))?;
        }
        Ok(())
    }

    fn source(
        &mut self,
        source: &SourceDocumentKey,
        depth: usize,
    ) -> Result<(), PrefabOverlayCostError> {
        self.node(depth)?;
        self.bytes(match source.origin() {
            SourceDocumentOrigin::Named { name, .. } => 37_usize.saturating_add(name.len()),
            SourceDocumentOrigin::ProjectLocation { path, .. } => {
                37_usize.saturating_add(path.as_str().len())
            }
            SourceDocumentOrigin::StableAsset { .. } => 37,
        })
    }

    fn record(
        &mut self,
        record: &SceneComponentRecord,
        depth: usize,
    ) -> Result<(), PrefabOverlayCostError> {
        self.node(depth)?;
        self.bytes(5_usize.saturating_add(record.owner().map_or(0, |owner| owner.as_str().len())))?;
        self.value(&record.value, depth.saturating_add(1))?;
        self.declarations(record.contextual_defaults.len())?;
        for (field, value) in &record.contextual_defaults {
            self.node(depth.saturating_add(1))?;
            self.bytes(field.as_str().len())?;
            self.value(value, depth.saturating_add(2))?;
        }
        Ok(())
    }

    fn value(
        &mut self,
        value: &ComponentValue,
        depth: usize,
    ) -> Result<(), PrefabOverlayCostError> {
        // Visit incrementally: a rejected caller-owned tree is never copied or fully scanned.
        enum Children<'a> {
            List(std::slice::Iter<'a, ComponentValue>),
            Map(std::collections::btree_map::Iter<'a, String, ComponentValue>),
        }
        let mut pending = Vec::<(Children<'_>, usize)>::new();
        let mut current = Some((value, depth, 0_usize));
        let mut nodes = self.cost.value_cost.nodes();
        let mut bytes = self.cost.value_cost.logical_bytes();
        while let Some((value, depth, key_bytes)) = current {
            self.node(depth)?;
            let value_nodes = match value {
                ComponentValue::EntityReference(reference) => reference.cost().nodes(),
                _ => 1,
            };
            self.nodes(value_nodes.saturating_sub(1), depth)?;
            add(
                &mut nodes,
                value_nodes,
                self.limits.value_nodes,
                PrefabOverlayCostKind::ValueNodes,
            )?;
            let scalar_bytes = match value {
                ComponentValue::Null | ComponentValue::List(_) | ComponentValue::Map(_) => 0,
                ComponentValue::Bool(_) => 1,
                ComponentValue::I64(_) | ComponentValue::U64(_) | ComponentValue::F64(_) => 8,
                ComponentValue::String(value) => value.len(),
                ComponentValue::EntityReference(reference) => reference.cost().logical_bytes(),
            };
            add(
                &mut bytes,
                key_bytes.saturating_add(scalar_bytes),
                self.limits.value_bytes,
                PrefabOverlayCostKind::ValueBytes,
            )?;
            require(
                PrefabOverlayCostKind::LogicalBytes,
                self.cost.structural_logical_bytes.saturating_add(bytes),
                self.limits.logical_bytes,
            )?;
            match value {
                ComponentValue::List(values) => {
                    self.reserve_scratch(&mut pending, 1)?;
                    pending.push((Children::List(values.iter()), depth))
                }
                ComponentValue::Map(values) => {
                    self.reserve_scratch(&mut pending, 1)?;
                    pending.push((Children::Map(values.iter()), depth));
                }
                _ => {}
            }
            current = loop {
                let Some((children, parent_depth)) = pending.last_mut() else {
                    break None;
                };
                let next = match children {
                    Children::List(values) => values.next().map(|value| (value, 0)),
                    Children::Map(values) => values.next().map(|(key, value)| (value, key.len())),
                };
                if let Some((value, key_bytes)) = next {
                    break Some((value, parent_depth.saturating_add(1), key_bytes));
                }
                pending.pop();
            };
        }
        // ComponentValueCost owns the scalar/key-byte convention. Reuse its canonical result
        // after the bounded walk rather than creating a second constructible cost type.
        let value_cost = if let Some(admit) = self.admission.as_deref_mut() {
            value
                .cost_up_to_with_admission(usize::MAX, usize::MAX, |bytes| {
                    if admit(bytes) { Ok(()) } else { Err(()) }
                })
                .map_err(|()| self.scratch_refusal())?
                .ok_or_else(|| self.scratch_refusal())?
        } else {
            value.cost()
        };
        self.cost.value_cost = self.cost.value_cost.saturating_add(value_cost);
        Ok(())
    }

    fn node(&mut self, depth: usize) -> Result<(), PrefabOverlayCostError> {
        self.nodes(1, depth)
    }

    fn nodes(&mut self, amount: usize, depth: usize) -> Result<(), PrefabOverlayCostError> {
        require(PrefabOverlayCostKind::Depth, depth, self.limits.depth)?;
        self.cost.depth = self.cost.depth.max(depth);
        add(
            &mut self.cost.nodes,
            amount,
            self.limits.nodes,
            PrefabOverlayCostKind::Nodes,
        )
    }

    fn declarations(&mut self, amount: usize) -> Result<(), PrefabOverlayCostError> {
        add(
            &mut self.cost.declarations,
            amount,
            self.limits.declarations,
            PrefabOverlayCostKind::Declarations,
        )
    }

    fn components(&mut self, amount: usize) -> Result<(), PrefabOverlayCostError> {
        add(
            &mut self.cost.components,
            amount,
            self.limits.components,
            PrefabOverlayCostKind::Components,
        )
    }

    fn bytes(&mut self, amount: usize) -> Result<(), PrefabOverlayCostError> {
        self.cost.structural_logical_bytes =
            self.cost.structural_logical_bytes.saturating_add(amount);
        require(
            PrefabOverlayCostKind::LogicalBytes,
            self.cost.logical_bytes(),
            self.limits.logical_bytes,
        )
    }
}

fn add(
    counter: &mut usize,
    amount: usize,
    maximum: usize,
    kind: PrefabOverlayCostKind,
) -> Result<(), PrefabOverlayCostError> {
    let observed = counter.saturating_add(amount);
    require(kind, observed, maximum)?;
    *counter = observed;
    Ok(())
}

fn require(
    kind: PrefabOverlayCostKind,
    observed: usize,
    maximum: usize,
) -> Result<(), PrefabOverlayCostError> {
    if observed > maximum {
        Err(PrefabOverlayCostError {
            kind,
            observed,
            maximum,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use nara_asset::AssetPath;
    use nara_reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponentSchemaOwnerId,
    };

    use super::*;
    use crate::{
        PrefabComponentSetMode, PrefabEntityOrigin, PrefabInstanceClaim, PrefabInstanceHop,
        PrefabPlacement, PrefabPropertyClaim, SceneEntityId, SourceDocumentKind,
    };

    fn key(name: &str) -> SourceDocumentKey {
        SourceDocumentKey::named(
            "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
            name,
            SourceDocumentKind::Prefab,
        )
        .unwrap()
    }

    fn instance(overlay: PrefabOverlay) -> PrefabInstance {
        PrefabInstance {
            source: AssetRef::Path(AssetPath::new("p.prefab.json").unwrap()),
            arguments: None,
            overrides: Some(overlay),
        }
    }

    fn sample() -> PrefabInstance {
        let mut target = PrefabEntityTarget::source(SceneEntityId::new("entity").unwrap());
        target.through.push(PrefabInstanceHop {
            anchor: PrefabEntityOrigin::InstanceAddition(SceneEntityId::new("anchor").unwrap()),
            expected_base: key("nested"),
        });
        let mut overlay = PrefabOverlay::new(key("base"));
        let mut addition = PrefabAddition::new(SceneEntityId::new("added").unwrap());
        addition.components.insert(
            ComponentTypeId::new("test.component"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(1))
                .with_owner(PersistentComponentSchemaOwnerId::try_new("test.owner").unwrap()),
        );
        addition.prefab = Some(instance(PrefabOverlay::new(key("addition-base"))));
        overlay.additions.push(addition);
        overlay.instances.push(PrefabInstanceClaim {
            target: target.clone(),
            value: Some(instance(PrefabOverlay::new(key("replacement-base")))),
        });
        overlay.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: target.clone(),
            component: ComponentTypeId::new("test.other"),
            record: SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::EntityReference(nara_identity::EntityReference::Authored {
                    reference: target.clone().into(),
                }),
            ),
        });
        overlay.placements.push(PrefabPlacement {
            target: target.clone(),
            parent: PrefabPlacementParent::Entity(target.clone()),
            slot: PrefabPlacementSlot::After(target.clone()),
        });
        overlay.components.push(PrefabComponentIntent::Properties {
            target,
            component: ComponentTypeId::new("test.property"),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("items")).with_steps([
                    ComponentPropertyStep::ListItem {
                        by: ComponentFieldId::new("id"),
                        key: ComponentPropertyKey::String("selected".to_owned()),
                    },
                    ComponentPropertyStep::Member(ComponentFieldId::new("value")),
                ]),
                intent: PrefabPropertyIntent::Set(ComponentValue::map([(
                    "text",
                    ComponentValue::String("payload".to_owned()),
                )])),
            }],
        });
        instance(overlay)
    }

    #[test]
    fn complete_raw_tree_has_inclusive_independent_cost_ceilings() {
        let instance = sample();
        let cost = prefab_instance_cost(&instance, PrefabOverlayCostLimits::UNBOUNDED).unwrap();
        assert_eq!(cost.value_cost.nodes(), 7);
        // U64 + complete authored endpoint/hop/canonical key + selected map payload.
        assert_eq!(
            cost.value_cost.logical_bytes(),
            8 + (6 + 6 + 36 + 6 + 1) + 11
        );
        assert_eq!(cost.entities, 1);
        assert_eq!(cost.components, 3);
        assert_eq!(cost.placements, 1);
        assert_eq!(cost.prefab_instances, 3);
        assert!(cost.logical_bytes() > cost.value_cost.logical_bytes());
        let exact = PrefabOverlayCostLimits {
            nodes: cost.nodes,
            depth: cost.depth,
            declarations: cost.declarations,
            entities: cost.entities,
            placements: cost.placements,
            components: cost.components,
            prefab_instances: cost.prefab_instances,
            value_nodes: cost.value_cost.nodes(),
            value_bytes: cost.value_cost.logical_bytes(),
            logical_bytes: cost.logical_bytes(),
        };
        assert_eq!(prefab_instance_cost(&instance, exact), Ok(cost));
        for (limits, kind) in [
            (
                PrefabOverlayCostLimits {
                    nodes: cost.nodes - 1,
                    ..exact
                },
                PrefabOverlayCostKind::Nodes,
            ),
            (
                PrefabOverlayCostLimits {
                    depth: cost.depth - 1,
                    ..exact
                },
                PrefabOverlayCostKind::Depth,
            ),
            (
                PrefabOverlayCostLimits {
                    declarations: cost.declarations - 1,
                    ..exact
                },
                PrefabOverlayCostKind::Declarations,
            ),
            (
                PrefabOverlayCostLimits {
                    entities: 0,
                    ..exact
                },
                PrefabOverlayCostKind::Entities,
            ),
            (
                PrefabOverlayCostLimits {
                    placements: 0,
                    ..exact
                },
                PrefabOverlayCostKind::Placements,
            ),
            (
                PrefabOverlayCostLimits {
                    components: cost.components - 1,
                    ..exact
                },
                PrefabOverlayCostKind::Components,
            ),
            (
                PrefabOverlayCostLimits {
                    prefab_instances: cost.prefab_instances - 1,
                    ..exact
                },
                PrefabOverlayCostKind::PrefabInstances,
            ),
            (
                PrefabOverlayCostLimits {
                    value_nodes: cost.value_cost.nodes() - 1,
                    ..exact
                },
                PrefabOverlayCostKind::ValueNodes,
            ),
            (
                PrefabOverlayCostLimits {
                    value_bytes: cost.value_cost.logical_bytes() - 1,
                    ..exact
                },
                PrefabOverlayCostKind::ValueBytes,
            ),
            (
                PrefabOverlayCostLimits {
                    logical_bytes: cost.logical_bytes() - 1,
                    ..exact
                },
                PrefabOverlayCostKind::LogicalBytes,
            ),
        ] {
            assert_eq!(
                prefab_instance_cost(&instance, limits).unwrap_err().kind,
                kind
            );
        }
    }

    #[test]
    fn metadata_without_component_values_still_consumes_raw_bytes_and_nodes() {
        let mut overlay = PrefabOverlay::new(key("base"));
        let empty = prefab_overlay_cost(&overlay, PrefabOverlayCostLimits::UNBOUNDED).unwrap();
        let mut target = PrefabEntityTarget::addition(SceneEntityId::new("missing").unwrap());
        target.through.push(PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("anchor").unwrap()),
            expected_base: key("unresolved"),
        });
        overlay.suppressions.push(target);
        let metadata = prefab_overlay_cost(&overlay, PrefabOverlayCostLimits::UNBOUNDED).unwrap();
        assert_eq!(metadata.value_cost, ComponentValueCost::ZERO);
        assert!(metadata.logical_bytes() > empty.logical_bytes());
        assert!(metadata.nodes > empty.nodes);
        assert!(
            prefab_overlay_cost(
                &overlay,
                PrefabOverlayCostLimits {
                    logical_bytes: empty.logical_bytes(),
                    ..PrefabOverlayCostLimits::UNBOUNDED
                }
            )
            .is_err()
        );
    }
}
