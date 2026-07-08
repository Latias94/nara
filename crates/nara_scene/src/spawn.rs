use std::collections::BTreeMap;

use nara_asset::{AssetServer, ProjectAssetDatabase};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::{Component, Entity, World};
use nara_reflect::{ComponentDecodeContext, ComponentRegistry};

use crate::{
    PrefabDocument, PrefabExpansionReport, PrefabSourceResolver, SceneDocument, SceneEntityId,
    ScenePatchDocument,
    hierarchy::{Parent, sync_children},
    validation::preflight_scene_with_context,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneInstanceId(u64);

impl SceneInstanceId {
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct SceneEntitySource {
    pub instance_id: SceneInstanceId,
    pub entity_id: SceneEntityId,
}

impl SceneEntitySource {
    #[must_use]
    pub fn export_id(&self) -> SceneEntityId {
        if self.instance_id.raw() == 1 {
            return self.entity_id.clone();
        }

        SceneEntityId::new(format!(
            "instance_{}/{}",
            self.instance_id.raw(),
            self.entity_id.as_str()
        ))
        .expect("scene entity source should produce valid export ids")
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneEntityMap {
    entities: BTreeMap<SceneEntityId, Entity>,
}

impl SceneEntityMap {
    pub fn insert(&mut self, scene_id: SceneEntityId, entity: Entity) -> Option<Entity> {
        self.entities.insert(scene_id, entity)
    }

    #[must_use]
    pub fn get(&self, scene_id: &SceneEntityId) -> Option<Entity> {
        self.entities.get(scene_id).copied()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SceneEntityId, Entity)> + '_ {
        self.entities
            .iter()
            .map(|(scene_id, entity)| (scene_id, *entity))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneSpawnReport {
    pub entity_map: SceneEntityMap,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Default)]
pub struct SceneSpawner {
    next_instance_id: u64,
}

impl SceneSpawner {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_instance_id: 1,
        }
    }

    pub fn spawn(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
    ) -> SceneSpawnReport {
        self.spawn_with_asset_context(world, registry, document, None)
    }

    pub fn spawn_with_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        self.spawn_with_asset_context(world, registry, document, Some(database))
    }

    pub fn spawn_with_prefab_resolver<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        resolver: &R,
    ) -> SceneSpawnReport {
        let expansion = document.expand_prefabs(registry, resolver);
        self.spawn_prefab_expansion(world, registry, expansion, None)
    }

    pub fn spawn_with_prefab_resolver_and_asset_database<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        let expansion = document.expand_prefabs_with_asset_database(registry, resolver, database);
        self.spawn_prefab_expansion(world, registry, expansion, Some(database))
    }

    fn spawn_with_asset_context(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        document: &SceneDocument,
        database: Option<&ProjectAssetDatabase>,
    ) -> SceneSpawnReport {
        let mut asset_server = world
            .get_resource::<AssetServer>()
            .cloned()
            .unwrap_or_default();
        let (preflight, asset_server_touched) = {
            let mut context = ComponentDecodeContext::with_asset_server(&mut asset_server);
            if let Some(database) = database {
                context = context.with_project_asset_database(database);
            }
            let preflight = preflight_scene_with_context(document, registry, &mut context);
            (preflight, context.asset_server_touched())
        };
        if preflight.diagnostics.has_errors() {
            return SceneSpawnReport {
                entity_map: SceneEntityMap::default(),
                diagnostics: preflight.diagnostics,
            };
        }

        if asset_server_touched {
            world.insert_resource(asset_server);
        }

        let instance_id = SceneInstanceId::from_raw(self.next_instance_id);
        self.next_instance_id = self.next_instance_id.saturating_add(1).max(1);

        let mut entity_map = SceneEntityMap::default();
        for entity in &preflight.entities {
            let runtime_entity = world.spawn_empty().id();
            world.entity_mut(runtime_entity).insert(SceneEntitySource {
                instance_id,
                entity_id: entity.id.clone(),
            });
            entity_map.insert(entity.id.clone(), runtime_entity);
        }

        let mut diagnostics = preflight.diagnostics;
        for entity in preflight.entities {
            let Some(runtime_entity) = entity_map.get(&entity.id) else {
                diagnostics.push(
                    Diagnostic::error("scene.internal-missing-entity", "missing spawned entity")
                        .with_entity_id(entity.id.as_str()),
                );
                continue;
            };

            for component in entity.components {
                if let Err(error) = component.apply(world, runtime_entity) {
                    diagnostics.push(
                        Diagnostic::error("scene.component-apply-failed", error.to_string())
                            .with_entity_id(entity.id.as_str()),
                    );
                }
            }

            if let Some(parent_id) = entity.parent {
                if let Some(parent_entity) = entity_map.get(&parent_id) {
                    world
                        .entity_mut(runtime_entity)
                        .insert(Parent(parent_entity));
                }
            }
        }

        sync_children(world);

        SceneSpawnReport {
            entity_map,
            diagnostics,
        }
    }

    fn spawn_prefab_expansion(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        expansion: PrefabExpansionReport,
        database: Option<&ProjectAssetDatabase>,
    ) -> SceneSpawnReport {
        let mut diagnostics = expansion.diagnostics;
        if diagnostics.has_errors() {
            return SceneSpawnReport {
                entity_map: SceneEntityMap::default(),
                diagnostics,
            };
        }

        let document = expansion
            .document
            .expect("successful prefab expansion should include document");
        let mut report = match database {
            Some(database) => self.spawn_with_asset_database(world, registry, &document, database),
            None => self.spawn(world, registry, &document),
        };
        diagnostics.extend(report.diagnostics);
        report.diagnostics = diagnostics;
        report
    }

    pub fn spawn_prefab(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
    ) -> SceneSpawnReport {
        self.spawn_prefab_with_patch(world, registry, prefab, &ScenePatchDocument::default())
    }

    pub fn spawn_prefab_with_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        self.spawn_prefab_with_patch_and_asset_database(
            world,
            registry,
            prefab,
            &ScenePatchDocument::default(),
            database,
        )
    }

    pub fn spawn_prefab_with_patch(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
        patch: &ScenePatchDocument,
    ) -> SceneSpawnReport {
        let instantiate = prefab.instantiate_with_patch(registry, patch);
        let mut diagnostics = instantiate.diagnostics;
        if diagnostics.has_errors() {
            return SceneSpawnReport {
                entity_map: SceneEntityMap::default(),
                diagnostics,
            };
        }

        let document = instantiate
            .document
            .expect("successful prefab patch instantiation should include document");
        let mut report = self.spawn(world, registry, &document);
        diagnostics.extend(report.diagnostics);
        report.diagnostics = diagnostics;
        report
    }

    pub fn spawn_prefab_with_patch_and_asset_database(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        prefab: &PrefabDocument,
        patch: &ScenePatchDocument,
        database: &ProjectAssetDatabase,
    ) -> SceneSpawnReport {
        let instantiate =
            prefab.instantiate_with_patch_and_asset_database(registry, patch, database);
        let mut diagnostics = instantiate.diagnostics;
        if diagnostics.has_errors() {
            return SceneSpawnReport {
                entity_map: SceneEntityMap::default(),
                diagnostics,
            };
        }

        let document = instantiate
            .document
            .expect("successful prefab patch instantiation should include document");
        let mut report = self.spawn_with_asset_database(world, registry, &document, database);
        diagnostics.extend(report.diagnostics);
        report.diagnostics = diagnostics;
        report
    }
}

#[must_use]
pub fn spawn_scene(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn(world, registry, document)
}

#[must_use]
pub fn spawn_scene_with_asset_database(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
    database: &ProjectAssetDatabase,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_with_asset_database(world, registry, document, database)
}

#[must_use]
pub fn spawn_scene_with_prefab_resolver<R: PrefabSourceResolver + ?Sized>(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
    resolver: &R,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_with_prefab_resolver(world, registry, document, resolver)
}

#[must_use]
pub fn spawn_scene_with_prefab_resolver_and_asset_database<R: PrefabSourceResolver + ?Sized>(
    world: &mut World,
    registry: &ComponentRegistry,
    document: &SceneDocument,
    resolver: &R,
    database: &ProjectAssetDatabase,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_with_prefab_resolver_and_asset_database(
        world, registry, document, resolver, database,
    )
}

#[must_use]
pub fn spawn_prefab(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_prefab(world, registry, prefab)
}

#[must_use]
pub fn spawn_prefab_with_asset_database(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
    database: &ProjectAssetDatabase,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_prefab_with_asset_database(world, registry, prefab, database)
}

#[must_use]
pub fn spawn_prefab_with_patch(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
    patch: &ScenePatchDocument,
) -> SceneSpawnReport {
    SceneSpawner::new().spawn_prefab_with_patch(world, registry, prefab, patch)
}

#[must_use]
pub fn spawn_prefab_with_patch_and_asset_database(
    world: &mut World,
    registry: &ComponentRegistry,
    prefab: &PrefabDocument,
    patch: &ScenePatchDocument,
    database: &ProjectAssetDatabase,
) -> SceneSpawnReport {
    SceneSpawner::new()
        .spawn_prefab_with_patch_and_asset_database(world, registry, prefab, patch, database)
}
