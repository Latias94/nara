//! Request-wide logical cost accounting. Released temporaries do not refund work.

use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_reflect::{
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentPropertyReadBudget, ComponentSchema,
    ComponentValue, ComponentValueCost, ComponentValueMaterializationLimits, EntityReference,
    EntityReferenceTraversalLimits,
};

use super::PrefabCopyError;
use crate::{PrefabDocument, SceneFileLimits, ScenePatchApplyLimits};

/// Inclusive whole-request limits, using the Scene source owner's logical ownership model.
///
/// Raw nodes bound every borrowed source/declaration walk before copying. Value nodes/bytes
/// aggregate admitted raw values and rewritten payloads. Default materialization and source
/// classification share the same cumulative work allowance, including discarded scratch.
/// Work nodes aggregate interpretation/selector/reference visits; work bytes aggregate logical
/// work and modeled allocation deltas (512 per structural item, 96 per value node, plus content).
/// These are conservative work/content bounds, not allocator capacity or process RSS estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabCopyLimits {
    raw_nodes: usize,
    value_nodes: usize,
    value_bytes: usize,
    work_nodes: usize,
    work_bytes: usize,
}

impl PrefabCopyLimits {
    pub const MAX_DOCUMENTS: usize = 32;
    /// Matches the existing raw Scene/Prefab cost walker and Reflect materializer.
    pub const MAX_DEPTH: usize = 64;

    /// Zero allowances are refused. Document count and semantic depth have fixed hard ceilings.
    #[must_use]
    pub const fn new(
        raw_nodes: usize,
        value_nodes: usize,
        value_bytes: usize,
        work_nodes: usize,
        work_bytes: usize,
    ) -> Option<Self> {
        if raw_nodes == 0
            || value_nodes == 0
            || value_bytes == 0
            || work_nodes == 0
            || work_bytes == 0
        {
            return None;
        }
        Some(Self {
            raw_nodes,
            value_nodes,
            value_bytes,
            work_nodes,
            work_bytes,
        })
    }
}

impl Default for PrefabCopyLimits {
    fn default() -> Self {
        Self {
            raw_nodes: 65_536,
            value_nodes: 262_144,
            value_bytes: 16 * 1024 * 1024,
            work_nodes: 4_000_000,
            work_bytes: 256 * 1024 * 1024,
        }
    }
}

pub(super) struct CopyBudget {
    limits: PrefabCopyLimits,
    raw_nodes: usize,
    value_nodes: usize,
    value_bytes: usize,
    work_nodes: usize,
    work_bytes: usize,
    pub(super) property_reads: ComponentPropertyReadBudget,
}

impl CopyBudget {
    pub(super) fn new(limits: PrefabCopyLimits) -> Self {
        Self {
            limits,
            raw_nodes: 0,
            value_nodes: 0,
            value_bytes: 0,
            work_nodes: 0,
            work_bytes: 0,
            property_reads: ComponentPropertyReadBudget::new(
                ItemLimit::new(limits.work_nodes).expect("nonzero copy work nodes"),
                ByteLimit::new(limits.work_bytes).expect("nonzero copy work bytes"),
            ),
        }
    }

    pub(super) fn charge(
        &mut self,
        nodes: usize,
        bytes: usize,
        allocated: usize,
    ) -> Result<(), PrefabCopyError> {
        add(&mut self.work_nodes, nodes, self.limits.work_nodes)?;
        add(&mut self.work_bytes, bytes, self.limits.work_bytes)?;
        add(&mut self.work_bytes, allocated, self.limits.work_bytes)
    }

    pub(super) fn allocate(&mut self, bytes: Option<usize>) -> Result<(), PrefabCopyError> {
        self.charge(0, 0, bytes.ok_or(PrefabCopyError::BudgetExceeded)?)
    }

    pub(super) fn document(&mut self, document: &PrefabDocument) -> Result<(), PrefabCopyError> {
        let limits = ScenePatchApplyLimits::default()
            .with_validation_work(items(self.limits.raw_nodes.saturating_sub(self.raw_nodes))?)
            .with_validation_value_nodes(items(
                self.limits.value_nodes.saturating_sub(self.value_nodes),
            )?)
            .with_validation_value_bytes(bytes(
                self.limits.value_bytes.saturating_sub(self.value_bytes),
            )?);
        let cost =
            crate::patch::bounded_prefab_source_cost_with_admission(document, limits, |amount| {
                self.allocate(amount)
            })?
            .ok_or(PrefabCopyError::BudgetExceeded)?;
        let nodes = cost.operation_count();
        add(&mut self.raw_nodes, nodes, self.limits.raw_nodes)?;
        add(
            &mut self.value_nodes,
            cost.value_cost().nodes(),
            self.limits.value_nodes,
        )?;
        add(
            &mut self.value_bytes,
            cost.value_cost().logical_bytes(),
            self.limits.value_bytes,
        )?;
        let allocated = cost
            .operation_count()
            .checked_mul(512)
            .and_then(|total| total.checked_add(cost.value_cost().nodes().checked_mul(96)?))
            .and_then(|total| total.checked_add(cost.logical_bytes()));
        self.charge(
            nodes.saturating_add(cost.value_cost().nodes()),
            cost.logical_bytes(),
            0,
        )?;
        self.allocate(allocated)
    }

    pub(super) fn value(&mut self, value: &ComponentValue) -> Result<(), PrefabCopyError> {
        let remaining_nodes = self.limits.value_nodes.saturating_sub(self.value_nodes);
        let remaining_bytes = self.limits.value_bytes.saturating_sub(self.value_bytes);
        let cost = value
            .cost_up_to_with_admission(remaining_nodes, remaining_bytes, |amount| {
                self.allocate(amount)
            })?
            .ok_or(PrefabCopyError::BudgetExceeded)?;
        add(&mut self.value_nodes, cost.nodes(), self.limits.value_nodes)?;
        add(
            &mut self.value_bytes,
            cost.logical_bytes(),
            self.limits.value_bytes,
        )?;
        self.charge(cost.nodes(), cost.logical_bytes(), 0)
    }

    pub(super) fn reference(&mut self, reference: &EntityReference) -> Result<(), PrefabCopyError> {
        let cost = reference.cost();
        self.charge(cost.nodes(), cost.logical_bytes(), 0)?;
        let allocated = match reference {
            EntityReference::Authored { reference } => reference.through.iter().try_fold(
                std::mem::size_of_val(reference).saturating_add(cost.logical_bytes()),
                |bytes, hop| {
                    bytes
                        .checked_add(std::mem::size_of_val(hop))?
                        .checked_add(hop.expected_base.checked_retained_bytes()?)
                },
            ),
            _ => std::mem::size_of_val(reference).checked_add(cost.logical_bytes()),
        };
        self.allocate(allocated)
    }

    /// The shared override materializer checks effective nodes/bytes before cloning. Reserve
    /// its temporary ownership and default-plan scratch before entering that non-callback API.
    pub(super) fn effective_record(
        &mut self,
        schema: &ComponentSchema,
        record: &crate::SceneComponentRecord,
    ) -> Result<(), PrefabCopyError> {
        let cost = self.planning_cost(&record.value)?;
        self.charge(
            cost.nodes(),
            cost.logical_bytes(),
            cost.nodes()
                .saturating_mul(96)
                .saturating_add(cost.logical_bytes()),
        )?;
        for field in schema.fields() {
            self.field_plan(field)?;
            // Contextual source validation separately debits its actual canonical-default
            // traversal and scratch. This reservation covers materializer-owned temporaries.
            if let Some(default) = record
                .contextual_defaults
                .get(field.id())
                .or(field.default_value())
            {
                let cost = self.planning_cost(default)?;
                self.charge(
                    cost.nodes(),
                    cost.logical_bytes(),
                    cost.nodes()
                        .saturating_mul(96)
                        .saturating_add(cost.logical_bytes()),
                )?;
            }
        }
        Ok(())
    }

    /// Selected-shape planning uses Reflect's ordinary walker. Admit its path storage using
    /// the finite traversal depth and the already bounded payload, without interpreting it.
    pub(super) fn selected_plan(
        &mut self,
        field: &ComponentFieldSchema,
        value: &ComponentValue,
    ) -> Result<(), PrefabCopyError> {
        self.field_plan(field)?;
        let cost = self.planning_cost(value)?;
        self.charge(
            cost.nodes(),
            cost.logical_bytes(),
            cost.nodes()
                .saturating_mul(PrefabCopyLimits::MAX_DEPTH.saturating_mul(96))
                .saturating_add(
                    cost.logical_bytes()
                        .saturating_mul(PrefabCopyLimits::MAX_DEPTH),
                ),
        )
    }

    fn planning_cost(
        &mut self,
        value: &ComponentValue,
    ) -> Result<ComponentValueCost, PrefabCopyError> {
        let nodes = self.limits.work_nodes.saturating_sub(self.work_nodes);
        let bytes = self.limits.work_bytes.saturating_sub(self.work_bytes);
        value
            .cost_up_to_with_admission(nodes, bytes, |amount| self.allocate(amount))?
            .ok_or(PrefabCopyError::BudgetExceeded)
    }

    fn field_plan(&mut self, field: &ComponentFieldSchema) -> Result<(), PrefabCopyError> {
        let path_bytes = field
            .path()
            .segments()
            .iter()
            .fold(0usize, |bytes, segment| {
                bytes.saturating_add(96).saturating_add(match segment {
                    ComponentFieldPathSegment::Field(name) => name.len(),
                    ComponentFieldPathSegment::Index(_) => 0,
                })
            });
        self.charge(
            field.path().segments().len().saturating_add(1),
            field.id().as_str().len().saturating_add(path_bytes),
            path_bytes
                .saturating_mul(3)
                .saturating_add(field.id().as_str().len()),
        )
    }

    pub(super) fn materialization(
        &self,
    ) -> Result<ComponentValueMaterializationLimits, PrefabCopyError> {
        Ok(ComponentValueMaterializationLimits::new(
            items(self.limits.value_nodes.saturating_sub(self.value_nodes))?,
            bytes(self.limits.value_bytes.saturating_sub(self.value_bytes))?,
            DepthLimit::new(PrefabCopyLimits::MAX_DEPTH).expect("nonzero copy depth"),
        ))
    }

    pub(super) fn traversal(&self) -> Result<EntityReferenceTraversalLimits, PrefabCopyError> {
        Ok(EntityReferenceTraversalLimits::new(
            items(self.limits.work_nodes.saturating_sub(self.work_nodes))?,
            bytes(self.limits.work_bytes.saturating_sub(self.work_bytes))?,
            DepthLimit::new(PrefabCopyLimits::MAX_DEPTH).expect("nonzero copy depth"),
        ))
    }

    pub(super) fn source_limits(&self) -> Result<SceneFileLimits, PrefabCopyError> {
        // Classification's per-record traversal allowance is independent of prior records.
        // Its allocation/work deltas still debit this request before each owning operation.
        Ok(SceneFileLimits::default()
            .with_component_value_nodes(items(self.limits.value_nodes)?)
            .with_component_value_bytes(bytes(self.limits.value_bytes)?))
    }
}

pub(super) fn asset_bytes(reference: &AssetRef) -> usize {
    std::mem::size_of::<AssetRef>().saturating_add(match reference {
        AssetRef::Path(path) => path.as_str().len(),
        AssetRef::StableId(_) => 36,
        // Source and product identities are inline. Reflect separately admits all nested
        // value nodes and strings when it encodes the rewritten canonical field payload.
        AssetRef::Product(_) => 0,
    })
}

fn add(value: &mut usize, amount: usize, maximum: usize) -> Result<(), PrefabCopyError> {
    *value = value
        .checked_add(amount)
        .filter(|value| *value <= maximum)
        .ok_or(PrefabCopyError::BudgetExceeded)?;
    Ok(())
}

fn items(value: usize) -> Result<ItemLimit, PrefabCopyError> {
    ItemLimit::new(value).ok_or(PrefabCopyError::BudgetExceeded)
}
fn bytes(value: usize) -> Result<ByteLimit, PrefabCopyError> {
    ByteLimit::new(value).ok_or(PrefabCopyError::BudgetExceeded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PrefabInstance, PrefabReferenceArguments, SceneComponentRecord, SceneEntityRecord,
        SourceDocumentKey, SourceDocumentKind,
    };
    use nara_identity::{AuthoredEntityReference, ReferenceInputId, SceneEntityId};
    use nara_reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId};

    fn key() -> SourceDocumentKey {
        SourceDocumentKey::named(
            "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
            "leaf",
            SourceDocumentKind::Prefab,
        )
        .unwrap()
    }

    #[test]
    fn input_declarations_and_placements_use_the_preclone_document_allowance() {
        let mut document = PrefabDocument::default();
        document.placements.push(crate::SceneEntityPlacement::root(
            SceneEntityId::new("root").unwrap(),
        ));
        document.inputs.extend([
            ReferenceInputId::new("left").unwrap(),
            ReferenceInputId::new("right").unwrap(),
        ]);
        let limits = |raw| PrefabCopyLimits::new(raw, 100, 10_000, 100, 10_000).unwrap();
        CopyBudget::new(limits(6)).document(&document).unwrap();
        assert!(matches!(
            CopyBudget::new(limits(5)).document(&document),
            Err(PrefabCopyError::BudgetExceeded)
        ));

        let mut observed = CopyBudget::new(PrefabCopyLimits::default());
        observed.document(&document).unwrap();
        let exact = observed.work_bytes;
        let limits = |work_bytes| PrefabCopyLimits::new(100, 100, 10_000, 100, work_bytes).unwrap();
        CopyBudget::new(limits(exact)).document(&document).unwrap();
        assert!(matches!(
            CopyBudget::new(limits(exact - 1)).document(&document),
            Err(PrefabCopyError::BudgetExceeded)
        ));
    }

    #[test]
    fn hidden_contextual_default_value_cost_is_admitted_before_document_copy() {
        let input = ReferenceInputId::new("socket").unwrap();
        let hidden = ComponentValue::EntityReference(EntityReference::Authored {
            reference: AuthoredEntityReference::input(input),
        });
        let entity = SceneEntityRecord::new(SceneEntityId::new("root").unwrap()).with_component(
            ComponentTypeId::new("test.CopyDefault"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null)
                .with_contextual_default(ComponentFieldId::new("target"), hidden),
        );
        let document = PrefabDocument::from_ordered_roots([entity]);
        // Null owns one value node; the retained authored input default owns two more.
        let limits = |nodes| PrefabCopyLimits::new(100, nodes, 10_000, 1_000, 100_000).unwrap();
        CopyBudget::new(limits(3)).document(&document).unwrap();
        assert!(matches!(
            CopyBudget::new(limits(2)).document(&document),
            Err(PrefabCopyError::BudgetExceeded)
        ));
    }

    #[test]
    fn unused_argument_input_endpoint_participates_in_raw_document_work() {
        let input = ReferenceInputId::new("socket").unwrap();
        let mut entity = SceneEntityRecord::new(SceneEntityId::new("root").unwrap());
        entity.prefab = Some(PrefabInstance {
            source: AssetRef::path("prefabs/leaf.prefab.json").unwrap(),
            binding: crate::PrefabInstanceBinding::Forest,
            arguments: Some(PrefabReferenceArguments {
                base: key(),
                values: [(
                    input.clone(),
                    EntityReference::Authored {
                        reference: AuthoredEntityReference::input(input),
                    },
                )]
                .into(),
            }),
            overrides: None,
        });
        let document = PrefabDocument::from_ordered_roots([entity]);
        let mut observed = CopyBudget::new(PrefabCopyLimits::default());
        observed.document(&document).unwrap();
        // Placement/document/entity/binding/instance/arguments/base/map/entry and input.
        assert_eq!(observed.raw_nodes, 13);
        let limits = |raw| PrefabCopyLimits::new(raw, 100, 10_000, 1_000, 100_000).unwrap();
        CopyBudget::new(limits(13)).document(&document).unwrap();
        assert!(matches!(
            CopyBudget::new(limits(12)).document(&document),
            Err(PrefabCopyError::BudgetExceeded)
        ));
    }
}
