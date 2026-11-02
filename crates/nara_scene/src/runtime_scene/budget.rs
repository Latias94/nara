use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
};

use nara_reflect::{
    ComponentTypeId, ComponentValue, ComponentValueCost, PersistentComponentSchemaOwnerId,
};

use super::{RuntimeSceneError, RuntimeSceneLimits};
use crate::{
    CompiledSceneDocument, PrefabProjectionStep, SceneComponentBindings, SceneEntityId,
    SceneEntityProvenance, SceneEntityProvenanceMap, SourceDocumentKey, SourceDocumentOrigin,
    compiled::claims::{FlatSceneClaims, FlatSceneComponentClaim, FlatSceneEntityClaim},
    provenance::{provenance_entry_bytes, provenance_string_bytes},
};

#[derive(Debug)]
pub(super) struct WorkBudget {
    limits: RuntimeSceneLimits,
    used: usize,
}

impl WorkBudget {
    pub const fn new(limits: RuntimeSceneLimits) -> Self {
        Self { limits, used: 0 }
    }

    pub const fn limits(&self) -> RuntimeSceneLimits {
        self.limits
    }

    #[cfg(test)]
    pub const fn used(&self) -> usize {
        self.used
    }

    pub fn reserve(&mut self, bytes: Option<usize>) -> Result<(), RuntimeSceneError> {
        let observed = bytes.and_then(|bytes| self.used.checked_add(bytes));
        let maximum = self.limits.working_bytes().get();
        match observed {
            Some(observed) if observed <= maximum => {
                self.used = observed;
                Ok(())
            }
            _ => Err(RuntimeSceneError::budget(
                "working-bytes",
                observed.unwrap_or(usize::MAX),
                maximum,
            )),
        }
    }

    pub fn admit(
        &mut self,
        _: usize,
        _: usize,
        bytes: Option<usize>,
    ) -> Result<(), RuntimeSceneError> {
        self.reserve(bytes)
    }

    pub fn witness(&mut self, bindings: &SceneComponentBindings) -> Result<(), RuntimeSceneError> {
        self.reserve(
            bindings
                .data_owners()
                .len()
                .checked_mul(size_of::<nara_reflect::ComponentSchemaOwnerSnapshot>()),
        )
    }

    /// Admit the complete independent flat payload before copying any of its owned values.
    /// The value-node model is the same conservative 96-byte model used by Reflect admission.
    pub fn document_clone(
        &mut self,
        document: &CompiledSceneDocument,
    ) -> Result<(), RuntimeSceneError> {
        let mut counts = FlatBudget::new(document.roots(), self)?;
        check(
            "entities",
            document.entities().len(),
            self.limits.base().entities().get(),
        )?;
        self.reserve(
            document
                .entities()
                .len()
                .checked_mul(size_of::<FlatSceneEntityClaim>()),
        )?;
        self.reserve(
            document
                .roots()
                .len()
                .checked_mul(size_of::<SceneEntityId>()),
        )?;
        for root in document.roots() {
            self.reserve(provenance_string_bytes(root.as_str()))?;
        }
        for entity in document.entities() {
            counts.entity(
                entity.id(),
                entity.children(),
                entity.components().len(),
                self,
            )?;
            self.reserve(provenance_string_bytes(entity.id().as_str()))?;
            self.reserve(
                entity
                    .children()
                    .len()
                    .checked_mul(size_of::<SceneEntityId>()),
            )?;
            for child in entity.children() {
                self.reserve(provenance_string_bytes(child.as_str()))?;
            }
            for (component, record) in entity.components() {
                let cost =
                    counts.component(component, record.schema_owner(), record.value(), self)?;
                self.reserve(
                    size_of::<(ComponentTypeId, FlatSceneComponentClaim)>()
                        .checked_add(6 * size_of::<usize>())
                        .and_then(|bytes| bytes.checked_add(component.as_str().len()))
                        .and_then(|bytes| {
                            bytes.checked_add(
                                record
                                    .schema_owner()
                                    .map_or(0, |owner| owner.as_str().len()),
                            )
                        }),
                )?;
                self.reserve(
                    cost.nodes()
                        .checked_mul(96)
                        .and_then(|value| value.checked_add(cost.logical_bytes())),
                )?;
            }
        }
        placement_depth(
            document.roots(),
            document
                .entities()
                .iter()
                .map(|entity| (entity.id(), entity.children())),
            document.entities().len(),
            counts.placements,
            self,
        )
    }

    /// The same file policy applies to decoded claims and the final rewritten cook payload.
    /// This counts storage and shape only; the compiled owner admits all semantic contracts.
    pub fn claims(&mut self, claims: &FlatSceneClaims) -> Result<(), RuntimeSceneError> {
        check(
            "entities",
            claims.entities.len(),
            self.limits.base().entities().get(),
        )?;
        let mut counts = FlatBudget::new(&claims.roots, self)?;
        for entity in &claims.entities {
            counts.entity(&entity.id, &entity.children, entity.components.len(), self)?;
            for (component, record) in &entity.components {
                counts.component(component, record.schema_owner.as_ref(), &record.value, self)?;
            }
        }
        placement_depth(
            &claims.roots,
            claims
                .entities
                .iter()
                .map(|entity| (&entity.id, entity.children.as_slice())),
            claims.entities.len(),
            counts.placements,
            self,
        )
    }

    fn value_shape(&mut self, value: &ComponentValue) -> Result<(), RuntimeSceneError> {
        enum Frame<'a> {
            List(std::slice::Iter<'a, ComponentValue>, usize),
            Map(
                std::collections::btree_map::Iter<'a, String, ComponentValue>,
                usize,
            ),
        }
        let mut pending = Vec::new();
        let mut current = Some((value, 1usize));
        while let Some((value, depth)) = current {
            check(
                "component-value-depth",
                depth,
                self.limits.value_limits().depth().get(),
            )?;
            let frame = match value {
                ComponentValue::List(values) if !values.is_empty() => {
                    Some(Frame::List(values.iter(), depth.saturating_add(1)))
                }
                ComponentValue::Map(values) if !values.is_empty() => {
                    Some(Frame::Map(values.iter(), depth.saturating_add(1)))
                }
                ComponentValue::String(text) => {
                    check_string(text, self.limits)?;
                    None
                }
                ComponentValue::EntityReference(nara_reflect::EntityReference::SceneLocal {
                    entity,
                }) => {
                    check_string(entity.as_str(), self.limits)?;
                    None
                }
                ComponentValue::EntityReference(nara_reflect::EntityReference::Persistent {
                    entity,
                }) => {
                    check_string(entity.namespace.as_str(), self.limits)?;
                    None
                }
                _ => None,
            };
            if let Some(frame) = frame {
                if pending.len() == pending.capacity() {
                    let capacity = pending
                        .len()
                        .saturating_add(1)
                        .max(pending.capacity().saturating_mul(2))
                        .max(4);
                    self.reserve(capacity.checked_mul(size_of::<Frame<'_>>()))?;
                    pending.reserve_exact(capacity - pending.len());
                }
                pending.push(frame);
            }
            current = loop {
                let next = match pending.last_mut() {
                    Some(Frame::List(values, depth)) => values.next().map(|value| (value, *depth)),
                    Some(Frame::Map(values, depth)) => match values.next() {
                        Some((key, value)) => {
                            check_string(key, self.limits)?;
                            Some((value, *depth))
                        }
                        None => None,
                    },
                    None => break None,
                };
                if next.is_some() {
                    break next;
                }
                pending.pop();
            };
        }
        Ok(())
    }
}

#[derive(Default)]
struct FlatBudget {
    placements: usize,
    components: usize,
    diagnostic_sources: usize,
    nodes: usize,
    bytes: usize,
}

impl FlatBudget {
    fn new(roots: &[SceneEntityId], work: &mut WorkBudget) -> Result<Self, RuntimeSceneError> {
        let mut counts = Self {
            placements: roots.len(),
            ..Self::default()
        };
        check(
            "placements",
            counts.placements,
            work.limits.base().placements().get(),
        )?;
        for root in roots {
            counts.string(root.as_str(), work.limits)?;
        }
        Ok(counts)
    }

    fn entity(
        &mut self,
        id: &SceneEntityId,
        children: &[SceneEntityId],
        components: usize,
        work: &mut WorkBudget,
    ) -> Result<(), RuntimeSceneError> {
        self.placements = self.placements.saturating_add(children.len());
        self.components = self.components.saturating_add(components);
        self.diagnostic_sources = self
            .diagnostic_sources
            .saturating_add(1)
            .saturating_add(components);
        let base = work.limits.base();
        check("placements", self.placements, base.placements().get())?;
        check("components", self.components, base.components().get())?;
        check(
            "diagnostic-sources",
            self.diagnostic_sources,
            base.diagnostic_sources().get(),
        )?;
        self.string(id.as_str(), work.limits)?;
        for child in children {
            self.string(child.as_str(), work.limits)?;
        }
        Ok(())
    }

    fn component(
        &mut self,
        component: &ComponentTypeId,
        owner: Option<&PersistentComponentSchemaOwnerId>,
        value: &ComponentValue,
        work: &mut WorkBudget,
    ) -> Result<ComponentValueCost, RuntimeSceneError> {
        self.string(component.as_str(), work.limits)?;
        if let Some(owner) = owner {
            self.string(owner.as_str(), work.limits)?;
        }
        // Keep the Scene file model: four version bytes and one optional-owner tag.
        self.bytes = self.bytes.saturating_add(5);
        check(
            "component-value-bytes",
            self.bytes,
            work.limits.base().component_value_bytes().get(),
        )?;
        let cost = value_cost(value, self.nodes, self.bytes, work)?;
        work.value_shape(value)?;
        self.nodes = self.nodes.saturating_add(cost.nodes());
        self.bytes = self.bytes.saturating_add(cost.logical_bytes());
        Ok(cost)
    }

    fn string(&mut self, value: &str, limits: RuntimeSceneLimits) -> Result<(), RuntimeSceneError> {
        check_string(value, limits)?;
        self.bytes = self.bytes.saturating_add(value.len());
        check(
            "component-value-bytes",
            self.bytes,
            limits.base().component_value_bytes().get(),
        )
    }
}

fn placement_depth<'a>(
    roots: &'a [SceneEntityId],
    entities: impl Iterator<Item = (&'a SceneEntityId, &'a [SceneEntityId])>,
    entity_count: usize,
    placements: usize,
    work: &mut WorkBudget,
) -> Result<(), RuntimeSceneError> {
    work.reserve(
        entity_count
            .checked_mul(size_of::<(&SceneEntityId, &[SceneEntityId])>() + 6 * size_of::<usize>()),
    )?;
    // Invalid input may name missing entities. Bound visited entries by every placement,
    // rather than assuming that shared semantic admission has already closed the hierarchy.
    work.reserve(placements.checked_mul(
        size_of::<(&SceneEntityId, usize)>() + size_of::<&SceneEntityId>() + 6 * size_of::<usize>(),
    ))?;
    let children = entities.collect::<BTreeMap<_, _>>();
    let mut visited = BTreeSet::new();
    let mut pending = Vec::with_capacity(placements);
    pending.extend(roots.iter().rev().map(|root| (root, 1usize)));
    while let Some((entity, depth)) = pending.pop() {
        check(
            "placement-depth",
            depth,
            work.limits.base().placement_depth().get(),
        )?;
        if !visited.insert(entity) {
            continue;
        }
        if let Some(children) = children.get(entity) {
            pending.extend(
                children
                    .iter()
                    .rev()
                    .map(|child| (child, depth.saturating_add(1))),
            );
        }
    }
    Ok(())
}

pub(super) fn check(
    kind: &'static str,
    observed: usize,
    maximum: usize,
) -> Result<(), RuntimeSceneError> {
    if observed > maximum {
        return Err(RuntimeSceneError::budget(kind, observed, maximum));
    }
    Ok(())
}

fn check_string(value: &str, limits: RuntimeSceneLimits) -> Result<(), RuntimeSceneError> {
    check(
        "wire-string-bytes",
        value.len(),
        limits.base().shape().string_bytes().get(),
    )
}

fn source_strings(
    key: &SourceDocumentKey,
    limits: RuntimeSceneLimits,
) -> Result<(), RuntimeSceneError> {
    match key.origin() {
        SourceDocumentOrigin::Named { name, .. } => check_string(name, limits),
        SourceDocumentOrigin::ProjectLocation { path, .. } => check_string(path.as_str(), limits),
        SourceDocumentOrigin::StableAsset { .. } => Ok(()),
    }
}

pub(super) fn value_cost(
    value: &ComponentValue,
    used_nodes: usize,
    used_bytes: usize,
    work: &mut WorkBudget,
) -> Result<ComponentValueCost, RuntimeSceneError> {
    let limits = work.limits().value_limits();
    value
        .cost_up_to_with_admission(
            limits.nodes().get().saturating_sub(used_nodes),
            limits.bytes().get().saturating_sub(used_bytes),
            |bytes| work.reserve(bytes),
        )?
        .ok_or_else(|| {
            RuntimeSceneError::refused(
                "scene.runtime-component-budget",
                "Runtime component values exceed their aggregate node or byte limit",
            )
        })
}

/// Logical provenance retention uses the existing Scene ownership model, counting shared keys
/// conservatively per occurrence. The containing key participates even for an empty Scene.
pub(super) struct ProvenanceBudget {
    limits: RuntimeSceneLimits,
    bytes: usize,
    nodes: usize,
}

impl ProvenanceBudget {
    pub fn new(
        source: &SourceDocumentKey,
        limits: RuntimeSceneLimits,
    ) -> Result<Self, RuntimeSceneError> {
        source_strings(source, limits)?;
        let mut budget = Self {
            limits,
            bytes: 0,
            nodes: 0,
        };
        budget.add(source.checked_retained_bytes(), 1)?;
        Ok(budget)
    }

    pub fn add(&mut self, bytes: Option<usize>, nodes: usize) -> Result<(), RuntimeSceneError> {
        self.bytes = bytes
            .and_then(|bytes| self.bytes.checked_add(bytes))
            .unwrap_or(usize::MAX);
        self.nodes = self.nodes.saturating_add(nodes);
        check(
            "provenance-bytes",
            self.bytes,
            self.limits.provenance_bytes().get(),
        )?;
        check(
            "provenance-nodes",
            self.nodes,
            self.limits.provenance_nodes().get(),
        )
    }

    pub fn entry(
        &mut self,
        id: &SceneEntityId,
        source: &SceneEntityProvenance,
    ) -> Result<(), RuntimeSceneError> {
        check_string(id.as_str(), self.limits)?;
        check_string(source.local_entity_id().as_str(), self.limits)?;
        self.add(provenance_entry_bytes(id, source.local_entity_id()), 1)?;
        if let Some(path) = source.projection_path() {
            check(
                "provenance-depth",
                path.steps().len(),
                self.limits.provenance_depth().get(),
            )?;
            check_string(path.root_anchor().as_str(), self.limits)?;
            self.add(provenance_string_bytes(path.root_anchor().as_str()), 0)?;
            for step in path.steps() {
                self.step(step)?;
            }
        }
        Ok(())
    }

    fn step(&mut self, step: &PrefabProjectionStep) -> Result<(), RuntimeSceneError> {
        check_string(step.entity_id().as_str(), self.limits)?;
        let bytes = match step {
            PrefabProjectionStep::SourceEntity {
                source_document,
                canonical_source,
                entity,
            } => {
                source_strings(canonical_source, self.limits)?;
                if let nara_asset::AssetRef::Path(path) = source_document {
                    check_string(path.as_str(), self.limits)?;
                }
                PrefabProjectionStep::source_allocation_bytes(
                    source_document,
                    canonical_source,
                    entity,
                )
            }
            PrefabProjectionStep::InstanceAddition { entity } => {
                PrefabProjectionStep::addition_allocation_bytes(entity)
            }
        };
        self.add(bytes, 1)
    }

    pub const fn bytes(&self) -> usize {
        self.bytes
    }
}

pub(super) fn provenance_cost(
    source: &SourceDocumentKey,
    provenance: &SceneEntityProvenanceMap,
    limits: RuntimeSceneLimits,
) -> Result<usize, RuntimeSceneError> {
    let mut budget = ProvenanceBudget::new(source, limits)?;
    check("entities", provenance.len(), limits.base().entities().get())?;
    for (id, origin) in provenance {
        budget.entry(id, origin)?;
    }
    Ok(budget.bytes())
}
