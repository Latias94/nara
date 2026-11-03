use std::sync::{Arc, Mutex};

use motion_physics::authored::{
    CollisionDefinition, CollisionInput, CollisionWorld, register_collision,
};
use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginSchemaProviderId, PluginShutdownObligationId, RuntimeCloseContext,
    RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
    RuntimeCloseProgress, RuntimeInstance,
};
use nara_asset::AssetRef;
use nara_core::Vec2;
use nara_ecs::{
    Entity, Resource, World, bevy_ecs::schedule::SingleThreadedExecutor, schedule::ScheduleLabel,
};
use nara_identity::{EntityLookup, SpawnedSceneInstance};
use nara_reflect::{
    ComponentFieldId, ComponentPropertyPath, ComponentRegistryPlugin, ComponentRegistrySnapshot,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
    ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, PersistentComponentProvider, component_registry,
    register_schema_provider_for_plugin,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabComponentIntent, PrefabDocument, PrefabEntityTarget,
    PrefabInstance, PrefabInstanceBinding, PrefabOverlay, PrefabPropertyClaim,
    PrefabPropertyIntent, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneEntityRecord, SceneOccurrenceKey, ScenePatchDocument, ScenePatchOperation,
    SourceDocumentKey, SourceDocumentKind,
};

mod tests;

const COMPONENT: &str = "fixture.motion.CollisionDefinition";
const PLUGIN: PluginId = PluginId::new("fixture.motion.collision-authoring");
const PROVIDER: PluginSchemaProviderId =
    PluginSchemaProviderId::new("fixture.motion.collision-authoring.schema");
const SHUTDOWN: PluginShutdownObligationId =
    PluginShutdownObligationId::new("fixture.motion.collision-authoring.shutdown");

fn catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![CollisionDefinition::persistent_component_schema()],
        ..Default::default()
    })
}

#[derive(Clone, Resource)]
struct CollisionOwner(Arc<Mutex<CollisionWorld>>);

#[derive(Clone, Resource)]
struct Sources(Arc<Mutex<Vec<SourceScene>>>);

struct SourceScene {
    session: SceneAuthoringSession,
    source: SourceDocumentKey,
    occurrence: SceneOccurrenceKey,
    resolver: InMemoryPrefabSourceResolver,
    active: bool,
}

impl SourceScene {
    fn new(document: SceneDocument, slot: &str) -> Self {
        Self {
            session: SceneAuthoringSession::new(document),
            source: source("collision-scene", SourceDocumentKind::Scene),
            occurrence: SceneOccurrenceKey::new("fixture.collision", slot).unwrap(),
            resolver: InMemoryPrefabSourceResolver::new(),
            active: true,
        }
    }
}

struct CollisionPlugin {
    owner: CollisionOwner,
    sources: Sources,
}

impl Plugin for CollisionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PLUGIN, PluginCategory::Runtime)
                .requires_plugins(&[nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID])
                .provides_schema(&[PROVIDER])
                .shutdown_obligations(&[SHUTDOWN]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        const DEFINITION: ComponentSchemaProviderDefinition =
            ComponentSchemaProviderDefinition::new(
                ComponentSchemaOwnerId::new("fixture.motion.collision-authoring"),
                PROVIDER,
                ComponentSchemaProviderBindingId::new("fixture.motion.collision-native", 1),
                catalog,
                register_collision,
            );
        register_schema_provider_for_plugin(app, PLUGIN, "CollisionDefinition", &DEFINITION)?;
        app.insert_resource(self.owner.clone())?
            .insert_resource(self.sources.clone())?
            .add_systems(Reconcile, synchronize)?
            .add_systems(CoreStage::FixedUpdate, step)?;
        app.get_schedule_mut(Reconcile)?
            .unwrap()
            .set_executor(SingleThreadedExecutor::new());
        app.register_plugin_runtime_close_participant(
            SHUTDOWN,
            RuntimeCloseParticipantId::new("fixture.motion.collision-authoring.close"),
            CollisionClose(Arc::clone(&self.owner.0)),
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct Reconcile;

fn synchronize(world: &mut World) {
    let registry = component_registry(world).unwrap().clone();
    let sources = Arc::clone(&world.resource::<Sources>().0);
    let owner = Arc::clone(&world.resource::<CollisionOwner>().0);
    let mut sources = sources.lock().unwrap();
    for scene in &mut *sources {
        if scene.active
            && (scene.session.live_instance().is_none() || scene.session.is_live_dirty())
        {
            let report = scene
                .session
                .sync_world_with_prefab_resolver(
                    world,
                    &registry,
                    &scene.source,
                    &scene.occurrence,
                    &scene.resolver,
                )
                .unwrap();
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        } else if !scene.active {
            let report = scene.session.clear_live_world(world);
            assert!(report.cleared, "{:?}", report.diagnostics);
        }
    }
    let inputs = sources
        .iter()
        .filter(|scene| scene.active)
        .map(|scene| CollisionInput {
            instance: scene.session.live_instance().unwrap(),
            revision: scene.session.revision(),
        })
        .collect::<Vec<_>>();
    owner.lock().unwrap().reconcile(world, &inputs).unwrap();
}

fn step(owner: nara_ecs::Res<CollisionOwner>) {
    owner.0.lock().unwrap().step().unwrap();
}

struct CollisionClose(Arc<Mutex<CollisionWorld>>);

impl RuntimeCloseParticipant for CollisionClose {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.lock().unwrap().close();
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begin_close(context)
    }
}

struct Harness {
    runtime: RuntimeInstance,
    owner: Arc<Mutex<CollisionWorld>>,
    sources: Arc<Mutex<Vec<SourceScene>>>,
}

impl Harness {
    fn new(scenes: Vec<SourceScene>) -> Self {
        let owner = Arc::new(Mutex::new(CollisionWorld::new()));
        let sources = Arc::new(Mutex::new(scenes));
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.add_plugin(CollisionPlugin {
            owner: CollisionOwner(Arc::clone(&owner)),
            sources: Sources(Arc::clone(&sources)),
        })
        .unwrap();
        let mut harness = Self {
            runtime: RuntimeInstance::start(app).unwrap(),
            owner,
            sources,
        };
        harness.sync();
        harness
    }

    fn registry(&self) -> ComponentRegistrySnapshot {
        component_registry(self.runtime.world()).unwrap().clone()
    }

    fn sync(&mut self) {
        self.runtime.run_schedule(Reconcile).unwrap();
    }

    fn entity(&self, scene: usize, name: &str) -> Entity {
        let sources = self.sources.lock().unwrap();
        resolve(
            self.runtime.world(),
            sources[scene].session.live_instance().unwrap(),
            name,
        )
    }

    fn patch(&mut self, scene: usize, patch: &ScenePatchDocument) {
        let registry = self.registry();
        let report = self.sources.lock().unwrap()[scene]
            .session
            .apply_patch(patch, &registry);
        assert!(report.applied, "{:?}", report.diagnostics);
        self.sync();
    }

    fn finish(self) {
        self.runtime.finish().unwrap();
    }
}

fn source(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "24838b71-89e8-4826-8516-53f807639d89".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn resolve(world: &World, receipt: &SpawnedSceneInstance, name: &str) -> Entity {
    match receipt.resolve(world, &id(name)) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live authored collision entity: {other:?}"),
    }
}

fn definition(
    center: Vec2,
    half_width: f32,
    half_height: f32,
    dynamic: bool,
) -> CollisionDefinition {
    CollisionDefinition {
        center,
        half_width,
        half_height,
        dynamic,
    }
}

fn record(name: &str, definition: CollisionDefinition) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name)).with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            definition.__encode_persistent_component().unwrap(),
        ),
    )
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        record("floor", definition(Vec2::new(0.0, -0.5), 10.0, 0.5, false)),
        record("box", definition(Vec2::new(0.0, 3.0), 0.5, 0.5, true)),
    ])
}

fn edit(name: &str, field: &str, value: f64) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id(name),
        component: ComponentTypeId::new(COMPONENT),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new(field),
        value: ComponentValue::f64(value).unwrap(),
    }])
}

fn prefab_scene() -> SourceScene {
    let request = AssetRef::path("prefabs/collision.prefab.json").unwrap();
    let prefab = PrefabDocument::from_ordered_roots([record(
        "shape",
        definition(Vec2::new(4.0, 0.0), 0.75, 0.25, false),
    )]);
    let mut scene = SourceScene::new(
        SceneDocument::from_ordered_roots(["left", "right"].map(|name| {
            let mut anchor = SceneEntityRecord::new(id(name));
            let overrides = (name == "right").then(|| {
                let mut overlay =
                    PrefabOverlay::new(source("collision-prefab", SourceDocumentKind::Prefab));
                overlay.components.push(PrefabComponentIntent::Properties {
                    target: PrefabEntityTarget::source(id("shape")),
                    component: ComponentTypeId::new(COMPONENT),
                    version: ComponentSchemaVersion::ONE,
                    claims: vec![PrefabPropertyClaim {
                        path: ComponentPropertyPath::new(ComponentFieldId::new("half_width")),
                        intent: PrefabPropertyIntent::Set(ComponentValue::f64(1.5).unwrap()),
                    }],
                });
                overlay
            });
            anchor.prefab = Some(PrefabInstance {
                binding: PrefabInstanceBinding::Forest,
                source: request.clone(),
                arguments: None,
                overrides,
            });
            anchor
        })),
        "prefab-pair",
    );
    scene.resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        request,
        source("collision-prefab", SourceDocumentKind::Prefab),
        prefab,
    );
    scene
}
