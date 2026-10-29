//! Request-wide logical cost accounting. Released temporaries do not refund work.

use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_reflect::{
    ComponentPropertyReadBudget, ComponentValue, ComponentValueMaterializationLimits,
    EntityReference, EntityReferenceTraversalLimits,
};

use super::PrefabCopyError;
use crate::{PrefabDocument, PrefabExpansionLimits, PrefabExpansionOptions, ScenePatchApplyLimits};

/// Inclusive whole-request limits, using the Scene compiler's logical ownership model.
///
/// Raw nodes bound every borrowed source/declaration walk before copying. Value nodes/bytes
/// aggregate admitted raw values and normalized payloads, including defaults and compiler output.
/// Work nodes aggregate interpretation/selector/reference visits; work bytes aggregate logical
/// work and modeled allocation deltas (512 per structural item, 96 per value node, plus content).
/// The final compiler consumes only the remaining allowances in one complete-closure call.
/// Its remaining byte allowance is divided between reference traversal and allocations;
/// traversal is reserved because the production report exposes only modeled allocation cost.
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
        let cost = crate::patch::bounded_scene_entities_source_cost_with_admission(
            &document.entities,
            limits,
            |amount| self.allocate(amount),
        )?
        .ok_or(PrefabCopyError::BudgetExceeded)?;
        let nodes = cost
            .operation_count()
            .saturating_add(document.roots.len())
            .saturating_add(1);
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
        let roots = document.roots.iter().try_fold(0usize, |total, root| {
            total
                .checked_add(std::mem::size_of_val(root))?
                .checked_add(root.as_str().len())
        });
        let allocated = cost
            .operation_count()
            .checked_mul(512)
            .and_then(|total| total.checked_add(cost.value_cost().nodes().checked_mul(96)?))
            .and_then(|total| total.checked_add(cost.logical_bytes()))
            .and_then(|total| total.checked_add(roots?));
        self.charge(
            nodes.saturating_add(cost.value_cost().nodes()),
            cost.logical_bytes(),
            0,
        )?;
        self.allocate(allocated)
    }

    pub(super) fn value(
        &mut self,
        value: &ComponentValue,
        source: usize,
    ) -> Result<(), PrefabCopyError> {
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
        self.charge(cost.nodes(), cost.logical_bytes(), 0)?;
        self.reject_persistent(value, source, 1)
    }

    fn reject_persistent(
        &mut self,
        value: &ComponentValue,
        source: usize,
        depth: usize,
    ) -> Result<(), PrefabCopyError> {
        if depth > PrefabCopyLimits::MAX_DEPTH {
            return Err(PrefabCopyError::BudgetExceeded);
        }
        self.charge(1, 0, 0)?;
        match value {
            ComponentValue::EntityReference(EntityReference::Persistent { .. }) => {
                Err(PrefabCopyError::PersistentReference { source })
            }
            ComponentValue::List(values) => {
                for value in values {
                    self.reject_persistent(value, source, depth + 1)?;
                }
                Ok(())
            }
            ComponentValue::Map(values) => {
                for value in values.values() {
                    self.reject_persistent(value, source, depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
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

    pub(super) fn compiler_options(&self) -> Result<PrefabExpansionOptions, PrefabCopyError> {
        let remaining_nodes = self.limits.work_nodes.saturating_sub(self.work_nodes);
        let remaining_bytes = self.limits.work_bytes.saturating_sub(self.work_bytes) / 2;
        Ok(PrefabExpansionOptions::default()
            .with_max_depth(PrefabCopyLimits::MAX_DOCUMENTS)
            .with_limits(
                PrefabExpansionLimits::default()
                    .with_materialized_entities(items(
                        self.limits.raw_nodes.saturating_sub(self.raw_nodes),
                    )?)
                    .with_materialized_components(items(
                        self.limits.raw_nodes.saturating_sub(self.raw_nodes),
                    )?)
                    .with_resolved_instances(items(
                        self.limits.raw_nodes.saturating_sub(self.raw_nodes),
                    )?)
                    .with_overlay_declarations(items(
                        self.limits.raw_nodes.saturating_sub(self.raw_nodes),
                    )?)
                    .with_materialized_value_nodes(items(
                        self.limits.value_nodes.saturating_sub(self.value_nodes),
                    )?)
                    .with_materialized_value_bytes(bytes(
                        self.limits.value_bytes.saturating_sub(self.value_bytes),
                    )?)
                    .with_reference_work_nodes(items(remaining_nodes)?)
                    .with_reference_work_bytes(bytes(remaining_bytes)?)
                    .with_working_bytes(bytes(remaining_bytes)?),
            ))
    }

    pub(super) fn compiler_working_bytes(&self) -> Result<ByteLimit, PrefabCopyError> {
        bytes(self.limits.work_bytes.saturating_sub(self.work_bytes))
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
