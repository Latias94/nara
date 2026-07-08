use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::{Entity, World};
use nara_reflect::ComponentRegistry;

use crate::{
    PrefabSourceResolver, SceneDocument, SceneEntityMap, ScenePatchDocument, ScenePatchReport,
    SceneSpawnReport, SceneSpawner, hierarchy::sync_children,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SceneAuthoringHistoryStatus {
    pub undo_depth: usize,
    pub redo_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneAuthoringSyncReport {
    pub synced: bool,
    pub removed_entities: usize,
    pub entity_map: SceneEntityMap,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug)]
pub struct SceneAuthoringSession {
    document: SceneDocument,
    undo_stack: Vec<ScenePatchDocument>,
    redo_stack: Vec<ScenePatchDocument>,
    live_entities: SceneEntityMap,
    live_dirty: bool,
    spawner: SceneSpawner,
}

impl SceneAuthoringSession {
    #[must_use]
    pub fn new(document: SceneDocument) -> Self {
        Self {
            document,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            live_entities: SceneEntityMap::default(),
            live_dirty: true,
            spawner: SceneSpawner::new(),
        }
    }

    #[must_use]
    pub fn document(&self) -> &SceneDocument {
        &self.document
    }

    pub fn replace_document(&mut self, document: SceneDocument) {
        self.document = document;
        self.clear_history();
        self.live_dirty = true;
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
    pub fn live_entity_map(&self) -> &SceneEntityMap {
        &self.live_entities
    }

    #[must_use]
    pub fn is_live_dirty(&self) -> bool {
        self.live_dirty
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
        let report = self.spawner.spawn(world, registry, &self.document);
        self.finish_world_sync(world, report)
    }

    pub fn sync_world_with_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> SceneAuthoringSyncReport {
        let report =
            self.spawner
                .spawn_with_asset_database(world, registry, &self.document, database);
        self.finish_world_sync(world, report)
    }

    pub fn sync_world_with_prefab_resolver<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        resolver: &R,
    ) -> SceneAuthoringSyncReport {
        let report =
            self.spawner
                .spawn_with_prefab_resolver(world, registry, &self.document, resolver);
        self.finish_world_sync(world, report)
    }

    pub fn sync_world_with_prefab_resolver_and_asset_database<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> SceneAuthoringSyncReport {
        let report = self.spawner.spawn_with_prefab_resolver_and_asset_database(
            world,
            registry,
            &self.document,
            resolver,
            database,
        );
        self.finish_world_sync(world, report)
    }

    pub fn clear_live_world(&mut self, world: &mut World) -> usize {
        let removed_entities = despawn_entities(world, live_entities(&self.live_entities));
        self.live_entities = SceneEntityMap::default();
        self.live_dirty = !self.document.entities.is_empty();
        sync_children(world);
        removed_entities
    }

    fn record_forward_patch(&mut self, patch: &ScenePatchDocument, report: &ScenePatchReport) {
        if !report.applied || patch.is_empty() {
            return;
        }
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

        self.live_dirty = true;
        if let Some(inverse) = report.inverse.clone() {
            match direction {
                HistoryDirection::Undo => self.redo_stack.push(inverse),
                HistoryDirection::Redo => self.undo_stack.push(inverse),
            }
        }
        report
    }

    fn finish_world_sync(
        &mut self,
        world: &mut World,
        report: SceneSpawnReport,
    ) -> SceneAuthoringSyncReport {
        let SceneSpawnReport {
            entity_map,
            diagnostics,
        } = report;

        if diagnostics.has_errors() {
            despawn_entities(world, live_entities(&entity_map));
            sync_children(world);
            return SceneAuthoringSyncReport {
                synced: false,
                removed_entities: 0,
                entity_map: self.live_entities.clone(),
                diagnostics,
            };
        }

        let removed_entities = despawn_entities(world, live_entities(&self.live_entities));
        self.live_entities = entity_map.clone();
        self.live_dirty = false;
        sync_children(world);

        SceneAuthoringSyncReport {
            synced: true,
            removed_entities,
            entity_map,
            diagnostics,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryDirection {
    Undo,
    Redo,
}

fn history_miss_report(code: impl Into<String>, message: impl Into<String>) -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::info(code, message));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

fn live_entities(entity_map: &SceneEntityMap) -> Vec<Entity> {
    entity_map.iter().map(|(_, entity)| entity).collect()
}

fn despawn_entities(world: &mut World, entities: Vec<Entity>) -> usize {
    entities
        .into_iter()
        .filter(|entity| {
            if world.get_entity(*entity).is_err() {
                return false;
            }
            world.despawn(*entity)
        })
        .count()
}
