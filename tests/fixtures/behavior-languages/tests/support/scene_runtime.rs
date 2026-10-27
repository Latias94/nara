//! Finite language-fixture custody over the public Scene data family.

use std::sync::Arc;

use nara_asset::AssetRef;
use nara_ecs::{Entity, World, bevy_ecs::world::WorldId};
use nara_identity::EntityLookup;
use nara_reflect::{
    ComponentRegistry, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentTypeId,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    ExpandedSceneCandidate, InMemoryPrefabSourceResolver, Name, PrefabDocument,
    SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentBindings, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind, SpawnedSceneInstance,
    retire_and_despawn_scene_instance,
};

pub fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

pub fn mover() -> ComponentTypeId {
    ComponentTypeId::new("trial.behavior.Mover")
}

pub fn sensor() -> ComponentTypeId {
    ComponentTypeId::new("trial.behavior.Sensor")
}

pub struct Definition {
    pub owner: ComponentSchemaOwnerSnapshot,
    pub bindings: SceneComponentBindings,
}

impl Definition {
    pub fn from_catalog(
        owner_id: &str,
        catalog: ComponentSchemaCatalog,
    ) -> Result<Arc<Self>, String> {
        let owner = ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new(owner_id)
                .map_err(|error| error.to_string())?,
            catalog,
            None,
        )
        .map_err(|error| error.to_string())?;
        let mut native = ComponentRegistry::new();
        SCENE_COMPONENTS_SCHEMA_PROVIDER
            .register_or_validate_into(&mut native)
            .map_err(|error| error.to_string())?;
        native.freeze().map_err(|error| error.to_string())?;
        let bindings = SceneComponentBindings::new(
            native.snapshot().map_err(|error| error.to_string())?,
            [owner.clone()],
        )
        .map_err(|error| error.to_string())?;
        Ok(Arc::new(Self { owner, bindings }))
    }
}

fn source_key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "f1390a84-11df-49b0-a901-d7a045141039".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

pub fn compile(
    document: &SceneDocument,
    definition: &Definition,
    prefab: Option<&PrefabDocument>,
) -> Result<ExpandedSceneCandidate, String> {
    let mut resolver = InMemoryPrefabSourceResolver::new();
    if let Some(prefab) = prefab {
        resolver.insert(
            AssetRef::path("base.prefab.json").unwrap(),
            source_key("base", SourceDocumentKind::Prefab),
            prefab.clone(),
        );
    }
    let report = definition.bindings.expand_prefabs(document, &resolver);
    if report.diagnostics.has_errors() {
        return Err(format!("{:?}", report.diagnostics));
    }
    report
        .candidate
        .map(|candidate| candidate.bind_source(source_key("scene", SourceDocumentKind::Scene)))
        .ok_or_else(|| "Scene compilation returned no candidate".into())
}

pub struct Runtime {
    pub world: World,
    instance: SpawnedSceneInstance,
    definition: Arc<Definition>,
}

/// A retained observation of one exact Scene entity, independent of language code lifetime.
#[derive(Clone)]
pub struct Target {
    world: WorldId,
    instance: SpawnedSceneInstance,
    source: SceneEntityId,
    entity: Entity,
}

impl Runtime {
    pub fn matches_definition(&self, definition: &Definition) -> bool {
        self.definition
            .bindings
            .same_definitions(&definition.bindings)
    }

    pub fn materialize(
        definition: Arc<Definition>,
        document: &SceneDocument,
        prefab: Option<&PrefabDocument>,
    ) -> Result<Self, String> {
        let candidate = compile(document, &definition, prefab)?;
        let mut world = World::new();
        let prepared = definition
            .bindings
            .prepare_scene(
                &mut world,
                &candidate,
                &SceneOccurrenceKey::new("trial", "main").unwrap(),
                None,
            )
            .map_err(|report| format!("{:?}", report.diagnostics))?;
        let report = prepared.commit(&mut world);
        if report.diagnostics.has_errors() {
            return Err(format!("{:?}", report.diagnostics));
        }
        let instance = report
            .instance
            .ok_or("Scene publication returned no receipt")?;
        Ok(Self {
            world,
            instance,
            definition,
        })
    }

    pub fn target(&self, source: &SceneEntityId) -> Target {
        let EntityLookup::Resolved(entity) = self.instance.resolve(&self.world, source) else {
            panic!("the fixture requires a live Scene target: {source}");
        };
        Target {
            world: self.world.id(),
            instance: self.instance.clone(),
            source: source.clone(),
            entity,
        }
    }

    pub fn entity(&self, target: &Target) -> Result<Entity, &'static str> {
        if self.world.id() != target.world || self.instance != target.instance {
            return Err("foreign Runtime");
        }
        match self.instance.resolve(&self.world, &target.source) {
            EntityLookup::Resolved(entity) if entity == target.entity => Ok(entity),
            _ => Err("retired Scene target"),
        }
    }

    pub fn components(&self, target: &Target) -> Result<Vec<ComponentTypeId>, String> {
        self.definition
            .bindings
            .data_components(&self.world, self.entity(target)?)
            .map(|members| members.map(|(component, _)| component.clone()).collect())
            .map_err(|error| error.to_string())
    }

    pub fn read(
        &self,
        target: &Target,
        component: &ComponentTypeId,
    ) -> Result<Option<&SceneComponentRecord>, String> {
        self.definition
            .bindings
            .data_component(&self.world, self.entity(target)?, component)
            .map_err(|error| error.to_string())
    }

    pub fn native_name(&self, target: &Target) -> &str {
        self.world
            .get::<Name>(self.entity(target).unwrap())
            .unwrap()
            .as_str()
    }

    pub fn export(&self) -> Result<SceneDocument, String> {
        let report = self.definition.bindings.export_scene(&self.world);
        if report.diagnostics.has_errors() {
            return Err(format!("{:?}", report.diagnostics));
        }
        report
            .into_output()
            .map(|output| output.document)
            .ok_or_else(|| "Scene export returned no document".into())
    }

    pub fn retire(&mut self) -> Result<(), String> {
        retire_and_despawn_scene_instance(&mut self.world, &self.instance)
            .map(|_| ())
            .map_err(|diagnostics| format!("{diagnostics:?}"))
    }
}
