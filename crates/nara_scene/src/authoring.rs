use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::World;
use nara_identity::{SpawnedSceneInstance, TombstoneCause};
use nara_reflect::ComponentRegistry;

use crate::{
    PrefabSourceResolver, SceneDocument, ScenePatchDocument, ScenePatchReport, SceneSpawnReport,
    SceneSpawner,
    diagnostics::{error as diagnostic_error, info as diagnostic_info, with_codec_error},
    patch::{PreparedScenePatch, scene_patch_cost},
    spawn::{SceneInstanceRetirementTransactionError, retire_scene_instance_exact},
};

static NEXT_AUTHORING_SOURCE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneAuthoringSourceId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneAuthoringRevision {
    source_id: SceneAuthoringSourceId,
    generation: u64,
}

/// Opaque identity for one committed authoring content state.
///
/// Revisions remain monotonic across every accepted transition. Content-state identities instead
/// follow Undo and Redo so callers can recognize that the document has returned to an earlier
/// saved state without hashing the document again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneAuthoringContentState {
    source_id: SceneAuthoringSourceId,
    generation: u64,
}

/// Fixed limits for the combined Undo and Redo history of one authoring session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneAuthoringHistoryLimits {
    max_records: usize,
    max_operations: usize,
    max_value_nodes: usize,
    max_logical_bytes: usize,
    max_inverse_operations: usize,
    max_inverse_value_nodes: usize,
    max_inverse_logical_bytes: usize,
}

impl SceneAuthoringHistoryLimits {
    pub const DEFAULT_MAX_RECORDS: usize = 256;
    pub const DEFAULT_MAX_OPERATIONS: usize = 1_048_576;
    pub const DEFAULT_MAX_VALUE_NODES: usize = 1_048_576;
    pub const DEFAULT_MAX_LOGICAL_BYTES: usize = 64 * 1024 * 1024;
    pub const DEFAULT_MAX_INVERSE_OPERATIONS: usize = 262_144;
    pub const DEFAULT_MAX_INVERSE_VALUE_NODES: usize = 262_144;
    pub const DEFAULT_MAX_INVERSE_LOGICAL_BYTES: usize = 16 * 1024 * 1024;

    #[must_use]
    pub const fn new(
        max_records: usize,
        max_operations: usize,
        max_value_nodes: usize,
        max_logical_bytes: usize,
        max_inverse_operations: usize,
        max_inverse_value_nodes: usize,
        max_inverse_logical_bytes: usize,
    ) -> Option<Self> {
        if max_records == 0
            || max_operations == 0
            || max_value_nodes == 0
            || max_logical_bytes == 0
            || max_inverse_operations == 0
            || max_inverse_value_nodes == 0
            || max_inverse_logical_bytes == 0
            || max_inverse_operations > max_operations
            || max_inverse_value_nodes > max_value_nodes
            || max_inverse_logical_bytes > max_logical_bytes
        {
            return None;
        }
        Some(Self {
            max_records,
            max_operations,
            max_value_nodes,
            max_logical_bytes,
            max_inverse_operations,
            max_inverse_value_nodes,
            max_inverse_logical_bytes,
        })
    }

    #[must_use]
    pub const fn max_records(self) -> usize {
        self.max_records
    }

    #[must_use]
    pub const fn max_operations(self) -> usize {
        self.max_operations
    }

    #[must_use]
    pub const fn max_value_nodes(self) -> usize {
        self.max_value_nodes
    }

    #[must_use]
    pub const fn max_logical_bytes(self) -> usize {
        self.max_logical_bytes
    }

    #[must_use]
    pub const fn max_inverse_operations(self) -> usize {
        self.max_inverse_operations
    }

    #[must_use]
    pub const fn max_inverse_value_nodes(self) -> usize {
        self.max_inverse_value_nodes
    }

    #[must_use]
    pub const fn max_inverse_logical_bytes(self) -> usize {
        self.max_inverse_logical_bytes
    }
}

impl Default for SceneAuthoringHistoryLimits {
    fn default() -> Self {
        Self::new(
            Self::DEFAULT_MAX_RECORDS,
            Self::DEFAULT_MAX_OPERATIONS,
            Self::DEFAULT_MAX_VALUE_NODES,
            Self::DEFAULT_MAX_LOGICAL_BYTES,
            Self::DEFAULT_MAX_INVERSE_OPERATIONS,
            Self::DEFAULT_MAX_INVERSE_VALUE_NODES,
            Self::DEFAULT_MAX_INVERSE_LOGICAL_BYTES,
        )
        .expect("default authoring history limits are valid")
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneAuthoringHistoryUsage {
    pub records: usize,
    pub operations: usize,
    pub value_nodes: usize,
    pub logical_bytes: usize,
}

impl SceneAuthoringRevision {
    #[must_use]
    pub const fn source_id(self) -> SceneAuthoringSourceId {
        self.source_id
    }

    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }

    #[must_use]
    const fn next(self) -> Self {
        Self {
            source_id: self.source_id,
            generation: self.generation + 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SceneAuthoringHistoryStatus {
    pub undo_depth: usize,
    pub redo_depth: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SceneAuthoringHistoryCost {
    operations: usize,
    value_nodes: usize,
    logical_bytes: usize,
}

impl SceneAuthoringHistoryCost {
    fn for_patch(patch: &ScenePatchDocument) -> Self {
        let patch_cost = scene_patch_cost(patch);
        let values = patch_cost.value_cost();
        Self {
            operations: patch_cost.operation_count(),
            value_nodes: values.nodes(),
            logical_bytes: patch_cost.logical_bytes(),
        }
    }

    const fn max(self, other: Self) -> Self {
        Self {
            operations: if self.operations > other.operations {
                self.operations
            } else {
                other.operations
            },
            value_nodes: if self.value_nodes > other.value_nodes {
                self.value_nodes
            } else {
                other.value_nodes
            },
            logical_bytes: if self.logical_bytes > other.logical_bytes {
                self.logical_bytes
            } else {
                other.logical_bytes
            },
        }
    }

    const fn saturating_add(self, other: Self) -> Self {
        Self {
            operations: self.operations.saturating_add(other.operations),
            value_nodes: self.value_nodes.saturating_add(other.value_nodes),
            logical_bytes: self.logical_bytes.saturating_add(other.logical_bytes),
        }
    }

    const fn saturating_sub(self, other: Self) -> Self {
        Self {
            operations: self.operations.saturating_sub(other.operations),
            value_nodes: self.value_nodes.saturating_sub(other.value_nodes),
            logical_bytes: self.logical_bytes.saturating_sub(other.logical_bytes),
        }
    }
}

#[derive(Debug)]
struct SceneAuthoringHistoryEntry {
    patch: ScenePatchDocument,
    from_state: SceneAuthoringContentState,
    to_state: SceneAuthoringContentState,
    cost: SceneAuthoringHistoryCost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneAuthoringSyncReport {
    pub synced: bool,
    pub removed_entities: usize,
    pub live_instance: Option<SpawnedSceneInstance>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneAuthoringClearReport {
    pub cleared: bool,
    pub removed_entities: usize,
    pub live_instance: Option<SpawnedSceneInstance>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug)]
pub struct SceneAuthoringSession {
    document: SceneDocument,
    revision: SceneAuthoringRevision,
    content_state: SceneAuthoringContentState,
    history_limits: SceneAuthoringHistoryLimits,
    history_cost: SceneAuthoringHistoryCost,
    undo_stack: VecDeque<SceneAuthoringHistoryEntry>,
    redo_stack: VecDeque<SceneAuthoringHistoryEntry>,
    live_instance: Option<SpawnedSceneInstance>,
    live_dirty: bool,
    source_upgrade_required: bool,
}

impl SceneAuthoringSession {
    #[must_use]
    pub fn new(document: SceneDocument) -> Self {
        Self::new_with_history_limits(document, SceneAuthoringHistoryLimits::default())
    }

    #[must_use]
    pub fn new_with_history_limits(
        document: SceneDocument,
        history_limits: SceneAuthoringHistoryLimits,
    ) -> Self {
        Self::new_with_source_state(document, false, history_limits)
    }

    fn new_with_source_state(
        document: SceneDocument,
        source_upgrade_required: bool,
        history_limits: SceneAuthoringHistoryLimits,
    ) -> Self {
        let revision = SceneAuthoringRevision {
            source_id: next_authoring_source_id(),
            generation: 0,
        };
        Self {
            document,
            revision,
            content_state: SceneAuthoringContentState {
                source_id: revision.source_id,
                generation: revision.generation,
            },
            history_limits,
            history_cost: SceneAuthoringHistoryCost::default(),
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            live_instance: None,
            live_dirty: true,
            source_upgrade_required,
        }
    }

    #[cfg(feature = "serde")]
    pub fn try_from_file_candidate(
        candidate: crate::SceneDocumentCandidate,
        registry: &ComponentRegistry,
    ) -> Result<Self, crate::SceneFilePublicationError> {
        let published = candidate.publish(registry)?;
        let source_upgrade_required = published.source_upgrade_required();
        Ok(Self::new_with_source_state(
            published.into_document(),
            source_upgrade_required,
            SceneAuthoringHistoryLimits::default(),
        ))
    }

    #[cfg(feature = "serde")]
    pub fn try_from_file_candidate_with_asset_database(
        candidate: crate::SceneDocumentCandidate,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> Result<Self, crate::SceneFilePublicationError> {
        let (document, source_upgrade_required) = candidate.into_canonical_document(registry)?;
        let diagnostics = document.validate_authoring_with_asset_database(registry, database);
        if diagnostics.has_errors() {
            return Err(crate::SceneFilePublicationError::new(diagnostics));
        }
        Ok(Self::new_with_source_state(
            document,
            source_upgrade_required,
            SceneAuthoringHistoryLimits::default(),
        ))
    }

    #[must_use]
    pub fn document(&self) -> &SceneDocument {
        &self.document
    }

    #[must_use]
    pub fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub fn content_state(&self) -> SceneAuthoringContentState {
        self.content_state
    }

    #[must_use]
    pub fn owns_content_state(&self, state: SceneAuthoringContentState) -> bool {
        self.revision.source_id == state.source_id
    }

    #[must_use]
    pub const fn history_limits(&self) -> SceneAuthoringHistoryLimits {
        self.history_limits
    }

    #[must_use]
    pub fn history_usage(&self) -> SceneAuthoringHistoryUsage {
        SceneAuthoringHistoryUsage {
            records: self.undo_stack.len().saturating_add(self.redo_stack.len()),
            operations: self.history_cost.operations,
            value_nodes: self.history_cost.value_nodes,
            logical_bytes: self.history_cost.logical_bytes,
        }
    }

    pub fn replace_document(&mut self, document: SceneDocument) {
        self.replace_document_with_source_state(document, false);
    }

    fn replace_document_with_source_state(
        &mut self,
        document: SceneDocument,
        source_upgrade_required: bool,
    ) {
        let source_id = next_authoring_source_id();
        self.document = document;
        self.revision = SceneAuthoringRevision {
            source_id,
            generation: 0,
        };
        self.content_state = SceneAuthoringContentState {
            source_id,
            generation: 0,
        };
        self.clear_history();
        self.live_dirty = true;
        self.source_upgrade_required = source_upgrade_required;
    }

    #[cfg(feature = "serde")]
    pub fn try_replace_file_candidate(
        &mut self,
        candidate: crate::SceneDocumentCandidate,
        registry: &ComponentRegistry,
    ) -> Result<(), crate::SceneFilePublicationError> {
        let published = candidate.publish(registry)?;
        let source_upgrade_required = published.source_upgrade_required();
        self.replace_document_with_source_state(published.into_document(), source_upgrade_required);
        Ok(())
    }

    #[cfg(feature = "serde")]
    pub fn try_replace_file_candidate_with_asset_database(
        &mut self,
        candidate: crate::SceneDocumentCandidate,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> Result<(), crate::SceneFilePublicationError> {
        let (document, source_upgrade_required) = candidate.into_canonical_document(registry)?;
        let diagnostics = document.validate_authoring_with_asset_database(registry, database);
        if diagnostics.has_errors() {
            return Err(crate::SceneFilePublicationError::new(diagnostics));
        }
        self.replace_document_with_source_state(document, source_upgrade_required);
        Ok(())
    }

    fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.history_cost = SceneAuthoringHistoryCost::default();
    }

    #[must_use]
    pub fn history_status(&self) -> SceneAuthoringHistoryStatus {
        SceneAuthoringHistoryStatus {
            undo_depth: self.undo_stack.len(),
            redo_depth: self.redo_stack.len(),
        }
    }

    #[must_use]
    pub fn live_instance(&self) -> Option<&SpawnedSceneInstance> {
        self.live_instance.as_ref()
    }

    #[must_use]
    pub fn is_live_dirty(&self) -> bool {
        self.live_dirty
    }

    #[must_use]
    pub fn source_upgrade_required(&self) -> bool {
        self.source_upgrade_required
    }

    #[doc(hidden)]
    pub fn acknowledge_source_saved(&mut self, state: SceneAuthoringContentState) -> bool {
        if !self.owns_content_state(state) {
            return false;
        }
        self.source_upgrade_required = false;
        true
    }

    pub fn apply_patch(
        &mut self,
        patch: &ScenePatchDocument,
        registry: &ComponentRegistry,
    ) -> ScenePatchReport {
        self.apply_forward_patch_with(patch, patch.prepare_to_scene(&self.document, registry))
    }

    pub fn apply_patch_with_asset_database(
        &mut self,
        patch: &ScenePatchDocument,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        self.apply_forward_patch_with(
            patch,
            patch.prepare_to_scene_with_asset_database(&self.document, registry, database),
        )
    }

    #[cfg(feature = "serde")]
    pub fn apply_file_patch_candidate(
        &mut self,
        candidate: crate::ScenePatchDocumentCandidate,
        registry: &ComponentRegistry,
    ) -> ScenePatchReport {
        match candidate.into_canonical_document(registry) {
            Ok((patch, _)) => self.apply_patch(&patch, registry),
            Err(error) => crate::format::publication_error_patch_report(error),
        }
    }

    #[cfg(feature = "serde")]
    pub fn apply_file_patch_candidate_with_asset_database(
        &mut self,
        candidate: crate::ScenePatchDocumentCandidate,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        match candidate.into_canonical_document(registry) {
            Ok((patch, _)) => self.apply_patch_with_asset_database(&patch, registry, database),
            Err(error) => crate::format::publication_error_patch_report(error),
        }
    }

    pub fn undo(&mut self, registry: &ComponentRegistry) -> ScenePatchReport {
        self.apply_history_patch_with(HistoryDirection::Undo, |patch, document| {
            patch.prepare_to_scene(document, registry)
        })
    }

    pub fn undo_with_asset_database(
        &mut self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        self.apply_history_patch_with(HistoryDirection::Undo, |patch, document| {
            patch.prepare_to_scene_with_asset_database(document, registry, database)
        })
    }

    pub fn redo(&mut self, registry: &ComponentRegistry) -> ScenePatchReport {
        self.apply_history_patch_with(HistoryDirection::Redo, |patch, document| {
            patch.prepare_to_scene(document, registry)
        })
    }

    pub fn redo_with_asset_database(
        &mut self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        self.apply_history_patch_with(HistoryDirection::Redo, |patch, document| {
            patch.prepare_to_scene_with_asset_database(document, registry, database)
        })
    }

    pub fn sync_world(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
    ) -> SceneAuthoringSyncReport {
        let mut spawner = SceneSpawner::new();
        let report = match self.live_instance.as_ref() {
            Some(current) => spawner.replace(world, registry, &self.document, current),
            None => spawner.spawn(world, registry, &self.document),
        };
        self.finish_world_sync(report)
    }

    pub fn sync_world_with_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> SceneAuthoringSyncReport {
        let mut spawner = SceneSpawner::new();
        let report = match self.live_instance.as_ref() {
            Some(current) => spawner.replace_with_asset_database(
                world,
                registry,
                &self.document,
                current,
                database,
            ),
            None => spawner.spawn_with_asset_database(world, registry, &self.document, database),
        };
        self.finish_world_sync(report)
    }

    pub fn sync_world_with_prefab_resolver<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        resolver: &R,
    ) -> SceneAuthoringSyncReport {
        let mut spawner = SceneSpawner::new();
        let report = match self.live_instance.as_ref() {
            Some(current) => spawner.replace_with_prefab_resolver(
                world,
                registry,
                &self.document,
                current,
                resolver,
            ),
            None => spawner.spawn_with_prefab_resolver(world, registry, &self.document, resolver),
        };
        self.finish_world_sync(report)
    }

    pub fn sync_world_with_prefab_resolver_and_asset_database<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> SceneAuthoringSyncReport {
        let mut spawner = SceneSpawner::new();
        let report = match self.live_instance.as_ref() {
            Some(current) => spawner.replace_with_prefab_resolver_and_asset_database(
                world,
                registry,
                &self.document,
                current,
                resolver,
                database,
            ),
            None => spawner.spawn_with_prefab_resolver_and_asset_database(
                world,
                registry,
                &self.document,
                resolver,
                database,
            ),
        };
        self.finish_world_sync(report)
    }

    pub fn clear_live_world(&mut self, world: &mut World) -> SceneAuthoringClearReport {
        let Some(current) = self.live_instance.clone() else {
            return SceneAuthoringClearReport {
                cleared: true,
                removed_entities: 0,
                live_instance: None,
                diagnostics: DiagnosticReport::default(),
            };
        };
        let retired = match retire_scene_instance_exact(world, &current, TombstoneCause::Unloaded) {
            Ok(retired) => retired,
            Err(error) => {
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(match error {
                    SceneInstanceRetirementTransactionError::Identity(error) => {
                        crate::diagnostics::with_identity_error(
                            diagnostic_error(
                                "scene.identity-retirement-failed",
                                "Scene identity retirement failed",
                            ),
                            &error,
                        )
                    }
                    SceneInstanceRetirementTransactionError::IdentitySupport(error) => {
                        crate::diagnostics::with_identity_support_error(
                            diagnostic_error(
                                "scene.identity-support-ineligible",
                                "Scene identity support is ineligible for retirement",
                            ),
                            &error,
                        )
                    }
                    SceneInstanceRetirementTransactionError::PersistentApply(error) => {
                        with_codec_error(
                            diagnostic_error(
                                "scene.persistent-apply-ineligible",
                                "Persistent scene components are ineligible for retirement",
                            ),
                            &error,
                        )
                    }
                    SceneInstanceRetirementTransactionError::Hierarchy => diagnostic_error(
                        "scene.hierarchy-retirement-ineligible",
                        "Scene hierarchy is ineligible for exact retirement",
                    ),
                    SceneInstanceRetirementTransactionError::Lifecycle => diagnostic_error(
                        "scene.lifecycle-retirement-ineligible",
                        "Scene retirement would run unsupported lifecycle work",
                    ),
                });
                return SceneAuthoringClearReport {
                    cleared: false,
                    removed_entities: 0,
                    live_instance: Some(current),
                    diagnostics,
                };
            }
        };

        let removed_entities = retired.len();
        let diagnostics = DiagnosticReport::default();
        self.live_instance = None;
        self.live_dirty = !self.document.entities.is_empty();
        SceneAuthoringClearReport {
            cleared: true,
            removed_entities,
            live_instance: None,
            diagnostics,
        }
    }

    fn apply_forward_patch_with(
        &mut self,
        patch: &ScenePatchDocument,
        prepared: Result<PreparedScenePatch, ScenePatchReport>,
    ) -> ScenePatchReport {
        let (candidate, report) = match prepared {
            Ok(prepared) => prepared.into_parts(),
            Err(report) => return report,
        };
        if !report.applied || patch.is_empty() {
            return report;
        }

        let Some(inverse) = report.inverse.clone() else {
            return reject_history_publication(
                report,
                "scene.authoring-history-inverse-missing",
                "accepted authoring patch did not produce an inverse",
            );
        };
        let cost = SceneAuthoringHistoryCost::for_patch(patch)
            .max(SceneAuthoringHistoryCost::for_patch(&inverse));
        if !self.history_entry_is_admissible(cost) {
            return reject_history_publication(
                report,
                "scene.authoring-history-entry-limit-exceeded",
                "authoring patch exceeds the per-entry history limit",
            );
        }

        let before_state = self.content_state;
        self.document = candidate;
        self.advance_revision();
        let after_state = self.content_state_for_current_revision();
        self.content_state = after_state;
        self.live_dirty = true;
        self.clear_redo_history();
        self.push_undo_history(SceneAuthoringHistoryEntry {
            patch: inverse,
            from_state: after_state,
            to_state: before_state,
            cost,
        });
        report
    }

    fn apply_history_patch_with(
        &mut self,
        direction: HistoryDirection,
        apply: impl FnOnce(
            &ScenePatchDocument,
            &SceneDocument,
        ) -> Result<PreparedScenePatch, ScenePatchReport>,
    ) -> ScenePatchReport {
        let Some(entry) = self.pop_history_entry(direction) else {
            return history_miss_report(direction.empty_code(), direction.empty_summary());
        };
        if self.content_state != entry.from_state {
            self.restore_history_entry(direction, entry);
            return history_error_report(
                "scene.authoring-history-state-mismatch",
                "authoring history does not match the current content state",
            );
        }

        let (candidate, report) = match apply(&entry.patch, &self.document) {
            Ok(prepared) => prepared.into_parts(),
            Err(report) => {
                self.restore_history_entry(direction, entry);
                return report;
            }
        };
        let Some(inverse) = report.inverse.clone() else {
            self.restore_history_entry(direction, entry);
            return reject_history_publication(
                report,
                "scene.authoring-history-inverse-missing",
                "accepted history patch did not produce an inverse",
            );
        };
        let observed_cost = SceneAuthoringHistoryCost::for_patch(&entry.patch)
            .max(SceneAuthoringHistoryCost::for_patch(&inverse));
        if observed_cost.operations > entry.cost.operations
            || observed_cost.value_nodes > entry.cost.value_nodes
            || observed_cost.logical_bytes > entry.cost.logical_bytes
        {
            self.restore_history_entry(direction, entry);
            return reject_history_publication(
                report,
                "scene.authoring-history-cost-changed",
                "history inverse exceeded the cost admitted by the originating edit",
            );
        }

        let reverse = SceneAuthoringHistoryEntry {
            patch: inverse,
            from_state: entry.to_state,
            to_state: entry.from_state,
            cost: entry.cost,
        };
        self.document = candidate;
        self.advance_revision();
        self.content_state = entry.to_state;
        self.live_dirty = true;
        self.restore_history_entry(direction.opposite(), reverse);
        report
    }

    fn history_entry_is_admissible(&self, cost: SceneAuthoringHistoryCost) -> bool {
        cost.operations <= self.history_limits.max_inverse_operations
            && cost.value_nodes <= self.history_limits.max_inverse_value_nodes
            && cost.logical_bytes <= self.history_limits.max_inverse_logical_bytes
    }

    fn clear_redo_history(&mut self) {
        for entry in self.redo_stack.drain(..) {
            self.history_cost = self.history_cost.saturating_sub(entry.cost);
        }
    }

    fn push_undo_history(&mut self, entry: SceneAuthoringHistoryEntry) {
        self.history_cost = self.history_cost.saturating_add(entry.cost);
        self.undo_stack.push_back(entry);
        while self.history_usage_exceeds_limits() {
            let evicted = self
                .undo_stack
                .pop_front()
                .expect("an admitted entry fits alone within the total history limits");
            self.history_cost = self.history_cost.saturating_sub(evicted.cost);
        }
    }

    fn history_usage_exceeds_limits(&self) -> bool {
        self.undo_stack.len().saturating_add(self.redo_stack.len())
            > self.history_limits.max_records
            || self.history_cost.operations > self.history_limits.max_operations
            || self.history_cost.value_nodes > self.history_limits.max_value_nodes
            || self.history_cost.logical_bytes > self.history_limits.max_logical_bytes
    }

    fn pop_history_entry(
        &mut self,
        direction: HistoryDirection,
    ) -> Option<SceneAuthoringHistoryEntry> {
        match direction {
            HistoryDirection::Undo => self.undo_stack.pop_back(),
            HistoryDirection::Redo => self.redo_stack.pop_back(),
        }
    }

    fn restore_history_entry(
        &mut self,
        direction: HistoryDirection,
        entry: SceneAuthoringHistoryEntry,
    ) {
        match direction {
            HistoryDirection::Undo => self.undo_stack.push_back(entry),
            HistoryDirection::Redo => self.redo_stack.push_back(entry),
        }
    }

    fn content_state_for_current_revision(&self) -> SceneAuthoringContentState {
        SceneAuthoringContentState {
            source_id: self.revision.source_id,
            generation: self.revision.generation,
        }
    }

    fn advance_revision(&mut self) {
        self.revision = self.revision.next();
    }

    fn finish_world_sync(&mut self, report: SceneSpawnReport) -> SceneAuthoringSyncReport {
        let removed_entities = report.retired_entities();
        let SceneSpawnReport {
            instance,
            diagnostics,
            ..
        } = report;

        if diagnostics.has_errors() {
            return SceneAuthoringSyncReport {
                synced: false,
                removed_entities: 0,
                live_instance: self.live_instance.clone(),
                diagnostics,
            };
        }

        let Some(instance) = instance else {
            let mut diagnostics = diagnostics;
            diagnostics.push(diagnostic_error(
                "scene.identity-instance-missing",
                "Successful scene synchronization did not publish an identity instance",
            ));
            return SceneAuthoringSyncReport {
                synced: false,
                removed_entities: 0,
                live_instance: self.live_instance.clone(),
                diagnostics,
            };
        };

        self.live_instance = Some(instance.clone());
        self.live_dirty = false;

        SceneAuthoringSyncReport {
            synced: true,
            removed_entities,
            live_instance: Some(instance),
            diagnostics,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryDirection {
    Undo,
    Redo,
}

impl HistoryDirection {
    const fn opposite(self) -> Self {
        match self {
            Self::Undo => Self::Redo,
            Self::Redo => Self::Undo,
        }
    }

    const fn empty_code(self) -> &'static str {
        match self {
            Self::Undo => "scene.undo-empty",
            Self::Redo => "scene.redo-empty",
        }
    }

    const fn empty_summary(self) -> &'static str {
        match self {
            Self::Undo => "undo history is empty",
            Self::Redo => "redo history is empty",
        }
    }
}

fn next_authoring_source_id() -> SceneAuthoringSourceId {
    SceneAuthoringSourceId(NEXT_AUTHORING_SOURCE_ID.fetch_add(1, Ordering::Relaxed))
}

fn history_miss_report(code: &'static str, summary: &'static str) -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic_info(code, summary));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

fn history_error_report(code: &'static str, summary: &'static str) -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic_error(code, summary));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

fn reject_history_publication(
    mut report: ScenePatchReport,
    code: &'static str,
    summary: &'static str,
) -> ScenePatchReport {
    report.applied = false;
    report.inverse = None;
    report.diagnostics.push(diagnostic_error(code, summary));
    report
}
