use std::mem::size_of;

use nara_reflect::{ComponentValue, ComponentValueCost};

use super::{RuntimeSceneError, RuntimeSceneLimits};
use crate::{
    PrefabProjectionStep, SceneComponentBindings, SceneDocument, SceneEntityId,
    SceneEntityProvenance, SceneEntityProvenanceMap, SourceDocumentKey, SourceDocumentOrigin,
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

    pub fn admit(&mut self, _: usize, _: usize, bytes: usize) -> Result<(), RuntimeSceneError> {
        self.reserve(Some(bytes))
    }

    pub fn witness(&mut self, bindings: &SceneComponentBindings) -> Result<(), RuntimeSceneError> {
        self.reserve(
            bindings
                .data_owners()
                .len()
                .checked_mul(size_of::<nara_reflect::ComponentSchemaOwnerSnapshot>()),
        )
    }

    /// Admit borrowed raw values and the complete independent document clone before copying.
    /// The value-node model is the same conservative 96-byte model used by Reflect admission.
    pub fn document_clone(&mut self, document: &SceneDocument) -> Result<(), RuntimeSceneError> {
        let base = self.limits.base();
        check("entities", document.entities.len(), base.entities().get())?;
        self.reserve(
            document
                .entities
                .len()
                .checked_mul(size_of::<crate::SceneEntityRecord>()),
        )?;
        self.reserve(document.roots.len().checked_mul(size_of::<SceneEntityId>()))?;
        for root in &document.roots {
            check_string(root.as_str(), self.limits)?;
            self.reserve(provenance_string_bytes(root.as_str()))?;
        }
        let mut nodes = 0usize;
        let mut bytes = 0usize;
        let mut components = 0usize;
        let mut placements = document.roots.len();
        for entity in &document.entities {
            if entity.prefab.is_some() {
                return Err(RuntimeSceneError::refused(
                    "scene.runtime-unexpanded",
                    "Runtime cook requires complete expansion without Prefab declarations",
                ));
            }
            components = components.saturating_add(entity.components.len());
            placements = placements.saturating_add(entity.children.len());
            check("components", components, base.components().get())?;
            check("placements", placements, base.placements().get())?;
            check_string(entity.id.as_str(), self.limits)?;
            self.reserve(provenance_string_bytes(entity.id.as_str()))?;
            self.reserve(
                entity
                    .children
                    .len()
                    .checked_mul(size_of::<SceneEntityId>()),
            )?;
            for child in &entity.children {
                check_string(child.as_str(), self.limits)?;
                self.reserve(provenance_string_bytes(child.as_str()))?;
            }
            for (component, record) in &entity.components {
                check_string(component.as_str(), self.limits)?;
                if let Some(owner) = record.owner() {
                    check_string(owner.as_str(), self.limits)?;
                }
                self.reserve(Some(
                    size_of::<crate::SceneComponentRecord>()
                        .saturating_add(6 * size_of::<usize>())
                        .saturating_add(component.as_str().len())
                        .saturating_add(record.owner().map_or(0, |owner| owner.as_str().len())),
                ))?;
                let cost = value_cost(&record.value, nodes, bytes, self)?;
                self.value_shape(&record.value)?;
                nodes = nodes.saturating_add(cost.nodes());
                bytes = bytes.saturating_add(cost.logical_bytes());
                self.reserve(
                    cost.nodes()
                        .checked_mul(96)
                        .and_then(|value| value.checked_add(cost.logical_bytes())),
                )?;
            }
        }
        check("placements", placements, base.placements().get())?;
        // The existing helper also accounts record metadata and placement depth.
        crate::format::validate_scene_value_budget(document, base).map_err(scene_budget_error)
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

pub(super) fn scene_budget_error(error: crate::SceneFileBudgetError) -> RuntimeSceneError {
    use crate::SceneFileBudgetKind;
    let kind = match error.kind {
        SceneFileBudgetKind::Entities => "entities",
        SceneFileBudgetKind::Placements => "placements",
        SceneFileBudgetKind::PlacementDepth => "placement-depth",
        SceneFileBudgetKind::Components => "components",
        SceneFileBudgetKind::PrefabInstances => "prefab-instances",
        SceneFileBudgetKind::PatchOperations => "patch-operations",
        SceneFileBudgetKind::OverlayDeclarations => "overlay-declarations",
        SceneFileBudgetKind::DiagnosticSources => "diagnostic-sources",
        SceneFileBudgetKind::ComponentValueNodes => "component-value-nodes",
        SceneFileBudgetKind::ComponentValueBytes => "component-value-bytes",
    };
    RuntimeSceneError::budget(kind, error.observed, error.maximum)
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
