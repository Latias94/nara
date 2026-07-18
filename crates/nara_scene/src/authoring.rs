use std::sync::atomic::{AtomicU64, Ordering};

use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Mut, World};
use nara_identity::{SpawnedSceneInstance, TombstoneCause, WorldIdentityDomain};
use nara_reflect::ComponentRegistry;

use crate::{
    PrefabSourceResolver, SceneDocument, ScenePatchDocument, ScenePatchReport, SceneSpawnReport,
    SceneSpawner,
    diagnostics::{
        error as diagnostic_error, info as diagnostic_info, warning as diagnostic_warning,
        with_codec_error,
    },
    hierarchy::sync_children,
    spawn::{
        resolved_scene_targets, validate_existing_scene_persistent_apply,
        validate_scene_identity_support,
    },
};

static NEXT_AUTHORING_SOURCE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneAuthoringSourceId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneAuthoringRevision {
    source_id: SceneAuthoringSourceId,
    generation: u64,
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
    undo_stack: Vec<ScenePatchDocument>,
    redo_stack: Vec<ScenePatchDocument>,
    live_instance: Option<SpawnedSceneInstance>,
    live_dirty: bool,
    source_upgrade_required: bool,
}

impl SceneAuthoringSession {
    #[must_use]
    pub fn new(document: SceneDocument) -> Self {
        Self::new_with_source_state(document, false)
    }

    fn new_with_source_state(document: SceneDocument, source_upgrade_required: bool) -> Self {
        Self {
            document,
            revision: SceneAuthoringRevision {
                source_id: next_authoring_source_id(),
                generation: 0,
            },
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
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

    pub fn replace_document(&mut self, document: SceneDocument) {
        self.replace_document_with_source_state(document, false);
    }

    fn replace_document_with_source_state(
        &mut self,
        document: SceneDocument,
        source_upgrade_required: bool,
    ) {
        self.document = document;
        self.advance_revision();
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

    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
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

    pub fn acknowledge_source_saved(&mut self) {
        self.source_upgrade_required = false;
    }

    pub fn apply_patch(
        &mut self,
        patch: &ScenePatchDocument,
        registry: &ComponentRegistry,
    ) -> ScenePatchReport {
        let report = patch.apply_to_scene(&mut self.document, registry);
        self.record_forward_patch(patch, &report);
        report
    }

    pub fn apply_patch_with_asset_database(
        &mut self,
        patch: &ScenePatchDocument,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        let report =
            patch.apply_to_scene_with_asset_database(&mut self.document, registry, database);
        self.record_forward_patch(patch, &report);
        report
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
        let Some(patch) = self.undo_stack.pop() else {
            return history_miss_report("scene.undo-empty", "undo history is empty");
        };
        let report = patch.apply_to_scene(&mut self.document, registry);
        self.record_history_patch(patch, report, HistoryDirection::Undo)
    }

    pub fn undo_with_asset_database(
        &mut self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        let Some(patch) = self.undo_stack.pop() else {
            return history_miss_report("scene.undo-empty", "undo history is empty");
        };
        let report =
            patch.apply_to_scene_with_asset_database(&mut self.document, registry, database);
        self.record_history_patch(patch, report, HistoryDirection::Undo)
    }

    pub fn redo(&mut self, registry: &ComponentRegistry) -> ScenePatchReport {
        let Some(patch) = self.redo_stack.pop() else {
            return history_miss_report("scene.redo-empty", "redo history is empty");
        };
        let report = patch.apply_to_scene(&mut self.document, registry);
        self.record_history_patch(patch, report, HistoryDirection::Redo)
    }

    pub fn redo_with_asset_database(
        &mut self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        let Some(patch) = self.redo_stack.pop() else {
            return history_miss_report("scene.redo-empty", "redo history is empty");
        };
        let report =
            patch.apply_to_scene_with_asset_database(&mut self.document, registry, database);
        self.record_history_patch(patch, report, HistoryDirection::Redo)
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
        if !world.contains_resource::<WorldIdentityDomain>() {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(crate::diagnostics::with_identity_error(
                diagnostic_error(
                    "scene.identity-retirement-failed",
                    "Scene identity retirement failed",
                ),
                &nara_identity::IdentityDomainError::WorldDomainUnavailable,
            ));
            return SceneAuthoringClearReport {
                cleared: false,
                removed_entities: 0,
                live_instance: Some(current),
                diagnostics,
            };
        }

        let current_targets = resolved_scene_targets(world, &current);
        if let Err(error) = validate_scene_identity_support(world, &current_targets) {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(crate::diagnostics::with_identity_support_error(
                diagnostic_error(
                    "scene.identity-support-ineligible",
                    "Scene identity support is ineligible for target-World apply",
                ),
                &error,
            ));
            return SceneAuthoringClearReport {
                cleared: false,
                removed_entities: 0,
                live_instance: Some(current),
                diagnostics,
            };
        }

        if let Err(error) = validate_existing_scene_persistent_apply(world, &current_targets) {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(with_codec_error(
                diagnostic_error(
                    "scene.persistent-apply-ineligible",
                    "Persistent scene components are ineligible for target-World apply",
                ),
                &error,
            ));
            return SceneAuthoringClearReport {
                cleared: false,
                removed_entities: 0,
                live_instance: Some(current),
                diagnostics,
            };
        }

        let retirement = world.resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
            domain.retire_scene_instance(world, &current, TombstoneCause::Unloaded)
        });
        let retired = match retirement {
            Ok(retired) => retired,
            Err(error) => {
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(crate::diagnostics::with_identity_error(
                    diagnostic_error(
                        "scene.identity-retirement-failed",
                        "Scene identity retirement failed",
                    ),
                    &error,
                ));
                return SceneAuthoringClearReport {
                    cleared: false,
                    removed_entities: 0,
                    live_instance: Some(current),
                    diagnostics,
                };
            }
        };

        let removed_entities = despawn_entities(world, &retired);
        let mut diagnostics = DiagnosticReport::default();
        if removed_entities != retired.len() {
            diagnostics.push(diagnostic_warning(
                "scene.retired-entity-already-missing",
                "A retired scene entity was already absent",
            ));
        }
        self.live_instance = None;
        self.live_dirty = !self.document.entities.is_empty();
        sync_children(world);
        SceneAuthoringClearReport {
            cleared: true,
            removed_entities,
            live_instance: None,
            diagnostics,
        }
    }

    fn record_forward_patch(&mut self, patch: &ScenePatchDocument, report: &ScenePatchReport) {
        if !report.applied || patch.is_empty() {
            return;
        }
        self.advance_revision();
        self.live_dirty = true;
        self.redo_stack.clear();
        if let Some(inverse) = report.inverse.clone() {
            self.undo_stack.push(inverse);
        }
    }

    fn record_history_patch(
        &mut self,
        patch: ScenePatchDocument,
        report: ScenePatchReport,
        direction: HistoryDirection,
    ) -> ScenePatchReport {
        if !report.applied {
            match direction {
                HistoryDirection::Undo => self.undo_stack.push(patch),
                HistoryDirection::Redo => self.redo_stack.push(patch),
            }
            return report;
        }

        self.advance_revision();
        self.live_dirty = true;
        if let Some(inverse) = report.inverse.clone() {
            match direction {
                HistoryDirection::Undo => self.redo_stack.push(inverse),
                HistoryDirection::Redo => self.undo_stack.push(inverse),
            }
        }
        report
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

fn despawn_entities(world: &mut World, entities: &[nara_ecs::Entity]) -> usize {
    entities
        .iter()
        .copied()
        .filter(|entity| world.despawn(*entity))
        .count()
}
