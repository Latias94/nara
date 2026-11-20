use std::{collections::BTreeSet, io::Write, sync::mpsc};

use motion_physics::authored::{DebugSnapshot, GeneratedCollision};
use nara_app::{ExecutionProfile, FixedTime};
use nara_reflect::{
    ComponentRegistryBuilder, ComponentRegistryError, ComponentSchemaCatalog,
    ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
    ComponentSchemaProviderSourceError, PersistentComponentProvider,
};
use nara_scene::{SceneDocumentCandidate, SceneEntityPlacement, SceneEntitySource};
use nara_transform::{Transform2d, fixed_input_spatial_snapshot};

use super::*;

pub(super) fn generated(owner: &CollisionWorld, entity: Entity) -> GeneratedCollision {
    owner
        .records()
        .find(|(current, _)| **current == entity)
        .unwrap()
        .1
        .clone()
}

pub(super) fn assert_same_pair(actual: &GeneratedCollision, expected: &GeneratedCollision) {
    assert_eq!(actual.body, expected.body);
    assert_eq!(actual.collider, expected.collider);
    assert_eq!(actual.origin, expected.origin);
}

pub(super) fn native_vertices(owner: &CollisionWorld, entity: Entity) -> Vec<Vec2> {
    let record = generated(owner, entity);
    let collider = owner.collider(record.collider).unwrap();
    let points = if owner.body(record.body).unwrap().is_dynamic() {
        let extents = collider.shape().as_cuboid().unwrap().half_extents;
        vec![
            Vec2::new(-extents.x, -extents.y),
            Vec2::new(extents.x, -extents.y),
            Vec2::new(extents.x, extents.y),
            Vec2::new(-extents.x, extents.y),
        ]
    } else {
        let polygon = collider.shape().as_convex_polygon().unwrap();
        assert_eq!(polygon.points().len(), 4);
        polygon.points().to_vec()
    };
    points
        .into_iter()
        .map(|point| collider.position().transform_point(point))
        .collect()
}

fn assert_current_capture(owner: &CollisionWorld, expected: &DebugSnapshot) {
    let current = owner.debug_snapshot();
    assert_eq!(current.shapes, expected.shapes);
    assert_eq!(current.solved, expected.solved);
    assert_eq!(current.input_stamp, expected.input_stamp);
    assert!(current.input_stamp.is_some());
}

pub(super) fn assert_withheld_capture(owner: &CollisionWorld) {
    let current = owner.debug_snapshot();
    assert!(current.shapes.is_empty());
    assert_eq!(current.input_stamp, None);
    assert!(!current.solved);
}

type NativeOperation =
    Box<dyn FnOnce(&mut World, &mut CollisionWorld, &mut Vec<SourceScene>) + Send>;

#[derive(Clone, Resource)]
struct PendingOperation(Arc<Mutex<Option<NativeOperation>>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct NativeMutation;

fn run_native_operation(world: &mut World) {
    let pending = Arc::clone(&world.resource::<PendingOperation>().0);
    let operation = pending.lock().unwrap().take();
    if let Some(operation) = operation {
        let owner = Arc::clone(&world.resource::<CollisionOwner>().0);
        let sources = Arc::clone(&world.resource::<Sources>().0);
        operation(
            world,
            &mut owner.lock().unwrap(),
            &mut sources.lock().unwrap(),
        );
    }
}

struct NativeCasePlugin(Arc<Mutex<CollisionWorld>>);

impl Plugin for NativeCasePlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("fixture.motion.collision-native-case"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara_transform::TRANSFORM_PLUGIN_ID])
        .provides_schema(&[PROVIDER])
        .shutdown_obligations(&[SHUTDOWN]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            SHUTDOWN,
            RuntimeCloseParticipantId::new("fixture.motion.collision-native-case.close"),
            CollisionClose(Arc::clone(&self.0)),
        )?;
        Ok(())
    }
}

pub(super) struct NativeCase {
    pub(super) runtime: RuntimeInstance,
    pub(super) owner: Arc<Mutex<CollisionWorld>>,
    pub(super) sources: Arc<Mutex<Vec<SourceScene>>>,
    pending: Arc<Mutex<Option<NativeOperation>>>,
}

impl NativeCase {
    pub(super) fn new(
        scenes: Vec<SourceScene>,
        registry: &ComponentRegistrySnapshot,
        profile: ExecutionProfile,
        configure: impl FnOnce(&mut App),
    ) -> Self {
        let owner = Arc::new(Mutex::new(CollisionWorld::new()));
        let pending = Arc::new(Mutex::new(None));
        let sources = Arc::new(Mutex::new(scenes));
        let mut app = App::new();
        app.set_execution_profile(profile).unwrap();
        app.add_plugins((
            nara_reflect::PreloadedComponentRegistryPlugin::new(registry.clone()),
            nara_hierarchy::HierarchyPlugin,
            nara_transform::TransformPlugin,
        ))
        .unwrap();
        app.insert_resource(CollisionOwner(Arc::clone(&owner)))
            .unwrap()
            .insert_resource(Sources(Arc::clone(&sources)))
            .unwrap()
            .insert_resource(PendingOperation(Arc::clone(&pending)))
            .unwrap()
            .add_systems(nara_app::StartupStage::Scene, synchronize)
            .unwrap()
            .add_systems(NativeMutation, run_native_operation)
            .unwrap();
        app.get_schedule_mut(NativeMutation)
            .unwrap()
            .unwrap()
            .set_executor(SingleThreadedExecutor::new());
        add_fixed_transform_input_systems(&mut app, run_native_operation).unwrap();
        app.add_plugin(NativeCasePlugin(Arc::clone(&owner)))
            .unwrap();
        configure(&mut app);
        let runtime = RuntimeInstance::start(app).unwrap();
        assert!(
            component_registry(runtime.world())
                .unwrap()
                .ptr_eq(registry)
        );
        Self {
            runtime,
            owner,
            sources,
            pending,
        }
    }

    pub(super) fn admit<R: Send + 'static>(
        &mut self,
        action: impl FnOnce(&mut World, &mut CollisionWorld, &mut Vec<SourceScene>) -> R
        + Send
        + 'static,
    ) -> R {
        self.admit_delta(FixedTime::DEFAULT_TIMESTEP, action)
    }

    pub(super) fn admit_delta<R: Send + 'static>(
        &mut self,
        delta: std::time::Duration,
        action: impl FnOnce(&mut World, &mut CollisionWorld, &mut Vec<SourceScene>) -> R
        + Send
        + 'static,
    ) -> R {
        let result = self.enqueue(action);
        self.runtime.drive(delta).unwrap();
        result
            .try_recv()
            .expect("the synchronous fixed input callback must have completed")
    }

    pub(super) fn mutate<R: Send + 'static>(
        &mut self,
        action: impl FnOnce(&mut World, &mut CollisionWorld, &mut Vec<SourceScene>) -> R
        + Send
        + 'static,
    ) -> R {
        let result = self.enqueue(action);
        self.runtime.run_schedule(NativeMutation).unwrap();
        result
            .try_recv()
            .expect("the synchronous native mutation callback must have completed")
    }

    fn enqueue<R: Send + 'static>(
        &self,
        action: impl FnOnce(&mut World, &mut CollisionWorld, &mut Vec<SourceScene>) -> R
        + Send
        + 'static,
    ) -> mpsc::Receiver<R> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut pending = self.pending.lock().unwrap();
        assert!(pending.is_none());
        *pending = Some(Box::new(move |world, owner, sources| {
            assert!(sender.send(action(world, owner, sources)).is_ok());
        }));
        receiver
    }

    pub(super) fn finish(self) {
        self.runtime.finish().unwrap();
    }
}

pub(super) fn native_registry() -> ComponentRegistrySnapshot {
    let mut builder = ComponentRegistryBuilder::new();
    nara_transform::TRANSFORM_SCHEMA_PROVIDER
        .register_into(&mut builder)
        .unwrap();
    COLLISION_SCHEMA.register_into(&mut builder).unwrap();
    let registry = builder.freeze().unwrap();
    COLLISION_SCHEMA.validate_snapshot(&registry).unwrap();
    registry
}

pub(super) fn collision_inputs(scenes: &[SourceScene]) -> Vec<CollisionInput<'_>> {
    scenes
        .iter()
        .filter(|scene| scene.active)
        .map(|scene| CollisionInput {
            instance: scene.session.live_instance().unwrap(),
            revision: scene.session.revision(),
        })
        .collect()
}

#[derive(Debug, Clone)]
pub(super) struct NativeState {
    pub(super) entity: Entity,
    pub(super) record: GeneratedCollision,
    pub(super) translation: Vec2,
    rotation: f32,
    pub(super) velocity: Vec2,
    angular_velocity: f32,
    sleeping: bool,
    vertices: Vec<Vec2>,
}

pub(super) fn native_state(owner: &CollisionWorld) -> Vec<NativeState> {
    owner
        .records()
        .map(|(entity, record)| {
            let body = owner.body(record.body).unwrap();
            NativeState {
                entity: *entity,
                record: record.clone(),
                translation: body.translation(),
                rotation: body.rotation().angle(),
                velocity: body.linvel(),
                angular_velocity: body.angvel(),
                sleeping: body.is_sleeping(),
                vertices: native_vertices(owner, *entity),
            }
        })
        .collect()
}

pub(super) fn assert_native_unchanged(
    owner: &CollisionWorld,
    expected: &[NativeState],
    changed: Option<Entity>,
) {
    assert_eq!(owner.counts(), (expected.len(), expected.len()));
    assert_native_states_unchanged(&native_state(owner), expected, changed);
}

pub(super) fn assert_native_states_unchanged(
    actual: &[NativeState],
    expected: &[NativeState],
    changed: Option<Entity>,
) {
    assert_eq!(actual.len(), expected.len());
    for expected in expected
        .iter()
        .filter(|entry| Some(entry.entity) != changed)
    {
        let actual = actual
            .iter()
            .find(|entry| entry.entity == expected.entity)
            .unwrap();
        assert_same_pair(&actual.record, &expected.record);
        assert_eq!(actual.translation, expected.translation);
        assert_eq!(actual.rotation, expected.rotation);
        assert_eq!(actual.velocity, expected.velocity);
        assert_eq!(actual.angular_velocity, expected.angular_velocity);
        assert_eq!(actual.sleeping, expected.sleeping);
        assert_eq!(actual.vertices, expected.vertices);
    }
}

fn assert_membership(harness: &Harness) {
    let sources = harness.sources.lock().unwrap();
    let expected = sources
        .iter()
        .filter(|scene| scene.active)
        .flat_map(|scene| {
            let receipt = scene.session.live_instance().unwrap();
            receipt.entity_ids().iter().filter_map(|entity_id| {
                let EntityLookup::Resolved(entity) =
                    receipt.resolve(harness.runtime.world(), entity_id)
                else {
                    panic!("the selected current receipt must resolve every member");
                };
                harness
                    .runtime
                    .world()
                    .get::<CollisionDefinition>(entity)
                    .map(|_| entity)
            })
        })
        .collect::<BTreeSet<_>>();
    let owner = harness.owner.lock().unwrap();
    let actual = owner
        .records()
        .map(|(entity, _)| *entity)
        .collect::<BTreeSet<_>>();
    let snapshot = owner.debug_snapshot();
    let debug = snapshot
        .shapes
        .iter()
        .map(|shape| shape.entity)
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert_eq!(debug, expected);
    assert_eq!(snapshot.shapes.len(), expected.len());
    assert_eq!(owner.counts(), (expected.len(), expected.len()));
    for (entity, record) in owner.records() {
        assert!(owner.body(record.body).is_some());
        assert!(owner.collider(record.collider).is_some());
        let source = harness
            .runtime
            .world()
            .get::<SceneEntitySource>(*entity)
            .unwrap();
        assert_eq!(record.origin.source, *source.containing_source());
        assert_eq!(record.origin.occurrence, *source.occurrence());
        assert_eq!(record.origin.provenance, *source.provenance());
    }
}

fn assert_native_width(
    harness: &Harness,
    scene: usize,
    name: &str,
    expected: f32,
) -> GeneratedCollision {
    let entity = harness.entity(scene, name);
    let owner = harness.owner.lock().unwrap();
    let record = generated(&owner, entity);
    let vertices = native_vertices(&owner, entity);
    let left = vertices
        .iter()
        .map(|point| point.x)
        .fold(f32::INFINITY, f32::min);
    let right = vertices
        .iter()
        .map(|point| point.x)
        .fold(f32::NEG_INFINITY, f32::max);
    assert_eq!((right - left) * 0.5, expected);
    let snapshot = owner.debug_snapshot();
    let shape = snapshot
        .shapes
        .iter()
        .find(|shape| shape.entity == entity)
        .unwrap();
    assert!((shape.vertices[1].x - shape.vertices[0].x - expected * 2.0).abs() < 1e-5);
    assert_eq!(shape.origin, record.origin);
    assert_eq!(
        record.origin.revision,
        harness.sources.lock().unwrap()[scene].session.revision()
    );
    record
}

#[test]
fn managed_fixed_steps_publish_actual_solver_contacts_and_immutable_native_debug_geometry() {
    let mut harness = Harness::new(vec![SourceScene::new(document(), "simulation")]);
    let entity = harness.entity(0, "box");
    let before = harness.owner.lock().unwrap().debug_snapshot();
    let original = (*before).clone();
    let record = generated(&harness.owner.lock().unwrap(), entity);
    assert_eq!(
        harness
            .owner
            .lock()
            .unwrap()
            .body(record.body)
            .unwrap()
            .translation(),
        Vec2::ZERO
    );
    assert_eq!(
        harness
            .owner
            .lock()
            .unwrap()
            .collider(record.collider)
            .unwrap()
            .translation(),
        Vec2::new(0.0, 3.0)
    );
    assert_eq!(
        record.origin.source,
        source("collision-scene", SourceDocumentKind::Scene)
    );
    assert_eq!(
        record.origin.occurrence,
        SceneOccurrenceKey::new("fixture.collision", "simulation").unwrap()
    );
    assert_eq!(record.origin.provenance.local_entity_id(), &id("box"));
    assert!(record.origin.provenance.projection_path().is_none());
    assert_eq!(record.origin.component, ComponentTypeId::new(COMPONENT));
    assert_eq!(
        record.origin.properties,
        ["center", "half_width", "half_height", "dynamic"]
            .map(|field| ComponentPropertyPath::new(ComponentFieldId::new(field)))
    );
    assert_eq!(
        record.origin.revision,
        harness.sources.lock().unwrap()[0].session.revision()
    );
    for _ in 0..120 {
        harness.runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    }
    let owner = harness.owner.lock().unwrap();
    let body = owner.body(record.body).unwrap();
    assert!(body.is_dynamic());
    assert!((body.translation().y + 2.5).abs() < 0.025);
    assert!(owner.contacts());
    assert!(owner.contact_between(entity, harness.entity(0, "floor")));
    let collider = owner.collider(record.collider).unwrap();
    let cuboid = collider.shape().as_cuboid().unwrap();
    assert_eq!(cuboid.half_extents.x, 0.5);
    assert_eq!(cuboid.half_extents.y, 0.5);
    let current = owner.debug_snapshot();
    let shape = current
        .shapes
        .iter()
        .find(|shape| shape.entity == entity)
        .unwrap();
    let center = Vec2::new(collider.translation().x, collider.translation().y);
    assert!((center.y - 0.5).abs() < 0.025);
    let root = harness.runtime.world().get::<Transform2d>(entity).unwrap();
    assert_eq!(root.translation, body.translation());
    assert_eq!(root.scale, Vec2::ONE);
    for (actual, offset) in shape.vertices.iter().zip([
        Vec2::new(-0.5, -0.5),
        Vec2::new(0.5, -0.5),
        Vec2::new(0.5, 0.5),
        Vec2::new(-0.5, 0.5),
    ]) {
        assert!((*actual - (center + offset)).length() < 1e-5);
    }
    assert_eq!(shape.origin, record.origin);
    assert_eq!(*before, original);
    assert_ne!(*current, *before);
    assert_eq!(
        harness
            .runtime
            .world()
            .get::<CollisionDefinition>(entity)
            .unwrap()
            .center
            .y,
        3.0
    );
    drop(owner);
    harness.sync();
    harness.admit();
    assert_eq!(
        generated(&harness.owner.lock().unwrap(), entity).body,
        record.body
    );
    let owner = Arc::clone(&harness.owner);
    harness.finish();
    {
        let owner = owner.lock().unwrap();
        assert_eq!(owner.counts(), (0, 0));
        assert!(owner.body(record.body).is_none());
        assert!(owner.collider(record.collider).is_none());
        assert_withheld_capture(&owner);
    }
    let mut closed_case =
        NativeCase::new(vec![], &native_registry(), ExecutionProfile::Serial, |_| {});
    closed_case.admit(move |world, _, _| {
        assert!(owner.lock().unwrap().step_and_writeback(world).is_err());
    });
    closed_case.finish();
    assert_eq!(*before, original);
    assert_eq!(current.shapes.len(), 2);
}

#[test]
fn source_edit_retires_old_generated_pairs_while_another_occurrence_keeps_solver_state() {
    let mut harness = Harness::new(vec![
        SourceScene::new(document(), "edited"),
        SourceScene::new(document(), "unrelated"),
    ]);
    let old_entity = harness.entity(0, "box");
    let unrelated = harness.entity(1, "box");
    let old = generated(&harness.owner.lock().unwrap(), old_entity);
    let retained = generated(&harness.owner.lock().unwrap(), unrelated);
    let before = harness.owner.lock().unwrap().debug_snapshot();
    let original = (*before).clone();
    for _ in 0..10 {
        harness.runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    }
    let position = harness
        .owner
        .lock()
        .unwrap()
        .body(retained.body)
        .unwrap()
        .translation();
    let velocity = harness
        .owner
        .lock()
        .unwrap()
        .body(retained.body)
        .unwrap()
        .linvel();
    assert!(velocity.length() > 0.0);
    harness.patch(0, &edit("box", "half_width", 1.25));
    let replacement = harness.entity(0, "box");
    let owner = harness.owner.lock().unwrap();
    let updated = generated(&owner, replacement);
    assert_ne!(updated.body, old.body);
    assert_ne!(updated.collider, old.collider);
    assert!(owner.body(old.body).is_none());
    assert!(owner.collider(old.collider).is_none());
    assert_ne!(updated.origin.revision, old.origin.revision);
    assert_eq!(
        owner
            .collider(updated.collider)
            .unwrap()
            .shape()
            .as_cuboid()
            .unwrap()
            .half_extents
            .x,
        1.25
    );
    let shape = owner.debug_snapshot();
    let vertices = shape
        .shapes
        .iter()
        .find(|shape| shape.entity == replacement)
        .unwrap()
        .vertices;
    assert!((vertices[1].x - vertices[0].x - 2.5).abs() < 1e-5);
    assert_same_pair(&generated(&owner, unrelated), &retained);
    assert_eq!(owner.body(retained.body).unwrap().translation(), position);
    assert_eq!(owner.body(retained.body).unwrap().linvel(), velocity);
    assert_eq!(*before, original);
    assert_eq!(owner.counts(), (4, 4));
    drop(owner);
    harness.finish();
    assert_eq!(*before, original);
}

#[test]
fn public_patch_undo_redo_and_disk_reopen_keep_the_exact_native_definition_without_solver_handles()
{
    let mut harness = Harness::new(vec![SourceScene::new(document(), "original")]);
    let registry = harness.registry();
    assert_eq!(harness.owner.lock().unwrap().counts(), (2, 2));
    harness.patch(0, &edit("box", "half_width", 1.25));
    let forward = assert_native_width(&harness, 0, "box", 1.25);
    let box_entity = harness.entity(0, "box");
    let updated = *harness
        .runtime
        .world()
        .get::<CollisionDefinition>(box_entity)
        .unwrap();
    assert_eq!(updated.half_width, 1.25);
    let accepted_revision = harness.sources.lock().unwrap()[0].session.revision();
    let saved = harness.sources.lock().unwrap()[0]
        .session
        .document()
        .clone();
    for record in &saved.entities {
        assert_eq!(
            record.components.keys().cloned().collect::<BTreeSet<_>>(),
            [
                ComponentTypeId::new(COMPONENT),
                ComponentTypeId::new(TRANSFORM)
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(record.components.len(), 2);
        assert_eq!(
            record.components[&ComponentTypeId::new(COMPONENT)].version,
            ComponentSchemaVersion::ONE
        );
        assert_eq!(
            record.components[&ComponentTypeId::new(TRANSFORM)],
            transform_record(Transform2d::IDENTITY)
        );
        let ComponentValue::Map(fields) =
            record.components[&ComponentTypeId::new(COMPONENT)].value()
        else {
            panic!("collision persistence uses one data-only record");
        };
        assert_eq!(
            fields.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            ["center", "dynamic", "half_height", "half_width"]
                .into_iter()
                .collect()
        );
    }
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(saved.to_json_string().unwrap().as_bytes())
        .unwrap();
    file.flush().unwrap();
    let disk = std::fs::read(file.path()).unwrap();
    let reopened = SceneAuthoringSession::try_from_file_candidate(
        SceneDocumentCandidate::decode_json_bytes(&disk).unwrap(),
        &registry,
    )
    .unwrap();
    assert_eq!(reopened.document(), &saved);
    assert!(
        harness.sources.lock().unwrap()[0]
            .session
            .undo(&registry)
            .applied
    );
    harness.sync();
    harness.admit();
    let undone = assert_native_width(&harness, 0, "box", 0.5);
    assert!(harness.owner.lock().unwrap().body(forward.body).is_none());
    assert!(
        harness
            .owner
            .lock()
            .unwrap()
            .collider(forward.collider)
            .is_none()
    );
    let current = harness.entity(0, "box");
    assert_eq!(
        harness
            .runtime
            .world()
            .get::<CollisionDefinition>(current)
            .unwrap()
            .half_width,
        0.5
    );
    assert!(
        harness.sources.lock().unwrap()[0]
            .session
            .redo(&registry)
            .applied
    );
    harness.sync();
    harness.admit();
    let redone = assert_native_width(&harness, 0, "box", 1.25);
    assert!(harness.owner.lock().unwrap().body(undone.body).is_none());
    assert!(
        harness
            .owner
            .lock()
            .unwrap()
            .collider(undone.collider)
            .is_none()
    );
    assert_ne!(
        harness.sources.lock().unwrap()[0].session.revision(),
        accepted_revision
    );
    let mut restored = SourceScene::new(SceneDocument::default(), "reopened");
    restored.session = reopened;
    harness.sources.lock().unwrap().push(restored);
    harness.sync();
    harness.admit();
    let reopened_pair = assert_native_width(&harness, 1, "box", 1.25);
    assert_ne!(reopened_pair.body, redone.body);
    assert_ne!(reopened_pair.collider, redone.collider);
    assert_eq!(assert_native_width(&harness, 0, "box", 1.25), redone);
    assert_membership(&harness);
    let restored = harness.entity(1, "box");
    let original = harness.entity(0, "box");
    assert_ne!(restored, original);
    assert_eq!(
        harness.runtime.world().get::<CollisionDefinition>(restored),
        harness.runtime.world().get::<CollisionDefinition>(original)
    );
    assert_eq!(
        harness.runtime.world().get::<Transform2d>(restored),
        Some(&Transform2d::IDENTITY)
    );
    assert_eq!(
        harness.runtime.world().get::<Transform2d>(restored),
        harness.runtime.world().get::<Transform2d>(original)
    );
    assert_eq!(harness.owner.lock().unwrap().counts(), (4, 4));
    let owner = Arc::clone(&harness.owner);
    harness.finish();
    assert_eq!(owner.lock().unwrap().counts(), (0, 0));
}

#[test]
fn invalid_authored_dimensions_refuse_without_advancing_source_or_changing_native_membership() {
    let mut harness = Harness::new(vec![SourceScene::new(document(), "invalid")]);
    harness.runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let entity = harness.entity(0, "box");
    let pair = generated(&harness.owner.lock().unwrap(), entity);
    let position = harness
        .owner
        .lock()
        .unwrap()
        .body(pair.body)
        .unwrap()
        .translation();
    let velocity = harness
        .owner
        .lock()
        .unwrap()
        .body(pair.body)
        .unwrap()
        .linvel();
    let registry = harness.registry();
    let before = harness.sources.lock().unwrap()[0]
        .session
        .document()
        .clone();
    let revision = harness.sources.lock().unwrap()[0].session.revision();
    for invalid in [0.0, -1.0, 64.5] {
        let report = harness.sources.lock().unwrap()[0]
            .session
            .apply_patch(&edit("box", "half_width", invalid), &registry);
        assert!(!report.applied);
        assert!(report.diagnostics.has_errors());
        assert_eq!(
            harness.sources.lock().unwrap()[0].session.document(),
            &before
        );
        assert_eq!(
            harness.sources.lock().unwrap()[0].session.revision(),
            revision
        );
        assert_eq!(harness.owner.lock().unwrap().counts(), (2, 2));
        harness.sync();
        harness.admit();
        assert_eq!(harness.entity(0, "box"), entity);
        let owner = harness.owner.lock().unwrap();
        assert_same_pair(&generated(&owner, entity), &pair);
        assert_eq!(owner.body(pair.body).unwrap().translation(), position);
        assert_eq!(owner.body(pair.body).unwrap().linvel(), velocity);
    }
    harness.finish();
}

#[test]
fn schema_refuses_admission_without_its_exact_native_codec_and_the_original_candidate_can_be_repaired()
 {
    fn catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
        Ok(ComponentSchemaCatalog {
            components: vec![CollisionDefinition::persistent_component_schema()],
            ..Default::default()
        })
    }
    fn repair_binding(
        candidate: &mut ComponentRegistryBuilder,
    ) -> Result<(), ComponentRegistryError> {
        candidate.register_component_schema(CollisionDefinition::persistent_component_schema())?;
        let catalog_before = candidate.catalog_candidate().clone();
        let original = std::mem::take(candidate);
        let failure = original.freeze().unwrap_err();
        let expected = ComponentRegistryError::MissingNativeBinding {
            component_id: ComponentTypeId::new(COMPONENT),
        };
        assert_eq!(failure.error(), &expected);
        let (mut original, error) = failure.into_parts();
        assert_eq!(error, expected);
        assert_eq!(original.catalog_candidate(), &catalog_before);
        original.register_native_component_with_codec::<CollisionDefinition, _, _>(
            &ComponentTypeId::new(COMPONENT),
            CollisionDefinition::__decode_persistent_component,
            CollisionDefinition::__encode_persistent_component,
        )?;
        *candidate = original;
        Ok(())
    }
    let repaired = ComponentSchemaProviderDefinition::new(
        COLLISION_SCHEMA.owner(),
        PROVIDER,
        ComponentSchemaProviderBindingId::new("fixture.motion.collision-native.repaired", 1),
        catalog,
        repair_binding,
    );
    let mut builder = ComponentRegistryBuilder::new();
    nara_transform::TRANSFORM_SCHEMA_PROVIDER
        .register_into(&mut builder)
        .unwrap();
    repaired.register_into(&mut builder).unwrap();
    let registry = builder.freeze().unwrap();
    let mut scene = SourceScene::new(document(), "repaired-binding");
    scene.source = source("repaired-binding", SourceDocumentKind::Scene);
    let mut case = NativeCase::new(vec![scene], &registry, ExecutionProfile::Serial, |_| {});
    case.admit(|world, owner, scenes| {
        let inputs = collision_inputs(scenes);
        let entity = resolve(world, inputs[0].instance, "box");
        assert_eq!(
            world.get::<CollisionDefinition>(entity).unwrap().half_width,
            0.5
        );
        assert_eq!(
            world.get::<Transform2d>(entity),
            Some(&Transform2d::IDENTITY)
        );
        assert!(fixed_input_spatial_snapshot(world).is_ok());
        owner.reconcile(world, &inputs).unwrap();
        assert_eq!(owner.counts(), (2, 2));
    });
    let owner = Arc::clone(&case.owner);
    case.finish();
    assert_eq!(owner.lock().unwrap().counts(), (0, 0));
}

#[test]
fn duplicate_remove_and_occurrence_unload_leave_only_the_current_authored_membership() {
    let mut harness = Harness::new(vec![
        SourceScene::new(document(), "first"),
        SourceScene::new(document(), "second"),
        prefab_scene(),
    ]);
    assert_eq!(harness.owner.lock().unwrap().counts(), (6, 6));
    assert_membership(&harness);
    let placements = harness.sources.lock().unwrap()[0]
        .session
        .document()
        .placements
        .clone();
    let mut duplicated = placements.clone();
    duplicated.push(SceneEntityPlacement::root(id("box-copy")));
    harness.patch(
        0,
        &ScenePatchDocument::new([
            ScenePatchOperation::AddEntity {
                entity: record("box-copy", definition(Vec2::new(2.0, 3.0), 0.5, 0.5, true)),
            },
            ScenePatchOperation::SetPlacements {
                expected: placements,
                value: duplicated,
            },
        ]),
    );
    assert_eq!(harness.owner.lock().unwrap().counts(), (7, 7));
    assert_membership(&harness);
    let first = harness.entity(0, "box");
    let copy = harness.entity(0, "box-copy");
    let second = harness.entity(1, "box");
    assert_ne!(first, copy);
    assert_ne!(first, second);
    harness.patch(
        0,
        &ScenePatchDocument::new([ScenePatchOperation::RemoveComponent {
            entity: id("box"),
            component: ComponentTypeId::new(COMPONENT),
        }]),
    );
    assert_eq!(harness.owner.lock().unwrap().counts(), (6, 6));
    assert_membership(&harness);
    assert!(
        harness
            .runtime
            .world()
            .get::<CollisionDefinition>(first)
            .is_none()
    );
    assert!(
        harness
            .runtime
            .world()
            .get::<CollisionDefinition>(second)
            .is_some()
    );
    let without_definition = harness.entity(0, "box");
    assert!(
        harness
            .runtime
            .world()
            .get::<CollisionDefinition>(without_definition)
            .is_none()
    );
    let copy = harness.entity(0, "box-copy");
    let copied_pair = generated(&harness.owner.lock().unwrap(), copy);
    let placements = harness.sources.lock().unwrap()[0]
        .session
        .document()
        .placements
        .clone();
    let remaining = placements
        .iter()
        .filter(|placement| {
            placement.target != nara_scene::PrefabEntityTarget::source(id("box-copy"))
        })
        .cloned()
        .collect();
    harness.patch(
        0,
        &ScenePatchDocument::new([
            ScenePatchOperation::RemoveEntity {
                entity: id("box-copy"),
            },
            ScenePatchOperation::SetPlacements {
                expected: placements,
                value: remaining,
            },
        ]),
    );
    assert_eq!(harness.owner.lock().unwrap().counts(), (5, 5));
    assert_membership(&harness);
    assert!(harness.runtime.world().get_entity(copy).is_err());
    assert!(
        harness
            .owner
            .lock()
            .unwrap()
            .body(copied_pair.body)
            .is_none()
    );
    assert!(
        harness
            .owner
            .lock()
            .unwrap()
            .collider(copied_pair.collider)
            .is_none()
    );
    let second_pairs = {
        let owner = harness.owner.lock().unwrap();
        owner
            .records()
            .filter(|(_, record)| {
                record.origin.occurrence
                    == SceneOccurrenceKey::new("fixture.collision", "second").unwrap()
            })
            .map(|(_, record)| record.clone())
            .collect::<Vec<_>>()
    };
    let preserved = {
        let owner = harness.owner.lock().unwrap();
        owner
            .records()
            .filter(|(_, record)| {
                record.origin.occurrence
                    != SceneOccurrenceKey::new("fixture.collision", "second").unwrap()
            })
            .map(|(entity, record)| (*entity, record.clone()))
            .collect::<Vec<_>>()
    };
    harness.sources.lock().unwrap()[1].active = false;
    harness.sync();
    harness.admit();
    assert_eq!(harness.owner.lock().unwrap().counts(), (3, 3));
    assert_membership(&harness);
    {
        let owner = harness.owner.lock().unwrap();
        for pair in second_pairs {
            assert!(owner.body(pair.body).is_none());
            assert!(owner.collider(pair.collider).is_none());
        }
        for (entity, record) in preserved {
            assert_eq!(generated(&owner, entity), record);
        }
    }
    assert!(harness.runtime.world().get_entity(second).is_err());
    let sources = harness.sources.lock().unwrap();
    let receipt = sources[2].session.live_instance().unwrap();
    let projected = receipt
        .entity_ids()
        .iter()
        .filter_map(
            |entity_id| match receipt.resolve(harness.runtime.world(), entity_id) {
                EntityLookup::Resolved(entity) => {
                    let provenance = harness
                        .runtime
                        .world()
                        .get::<SceneEntitySource>(entity)
                        .unwrap();
                    provenance
                        .provenance()
                        .projection_path()
                        .map(|path| path.root_anchor().clone())
                }
                _ => None,
            },
        )
        .collect::<BTreeSet<_>>();
    assert_eq!(projected, [id("left"), id("right")].into_iter().collect());
    let owner = harness.owner.lock().unwrap();
    let generated = owner
        .records()
        .filter(|(_, record)| record.origin.occurrence == sources[2].occurrence)
        .collect::<Vec<_>>();
    assert_eq!(generated.len(), 2);
    assert_ne!(generated[0].1.body, generated[1].1.body);
    assert_ne!(generated[0].1.collider, generated[1].1.collider);
    assert_eq!(
        generated
            .iter()
            .map(|(_, record)| record
                .origin
                .provenance
                .projection_path()
                .unwrap()
                .root_anchor()
                .clone())
            .collect::<BTreeSet<_>>(),
        [id("left"), id("right")].into_iter().collect()
    );
    for (entity, record) in generated {
        let source_context = harness
            .runtime
            .world()
            .get::<SceneEntitySource>(*entity)
            .unwrap();
        assert_eq!(record.origin.provenance, *source_context.provenance());
        assert_eq!(record.origin.source, sources[2].source);
        assert_eq!(record.origin.revision, sources[2].session.revision());
        assert_eq!(record.origin.component, ComponentTypeId::new(COMPONENT));
        assert_eq!(record.origin.provenance.local_entity_id(), &id("shape"));
        let path = record.origin.provenance.projection_path().unwrap();
        let [
            nara_scene::PrefabProjectionStep::SourceEntity {
                canonical_source,
                source_document,
                entity: local,
            },
        ] = path.steps()
        else {
            panic!("generated collision keeps its original typed Prefab step");
        };
        assert_eq!(
            canonical_source,
            &source("collision-prefab", SourceDocumentKind::Prefab)
        );
        assert_eq!(
            source_document,
            &AssetRef::path("prefabs/collision.prefab.json").unwrap()
        );
        assert_eq!(local, &id("shape"));
        let snapshot = owner.debug_snapshot();
        let shape = snapshot
            .shapes
            .iter()
            .find(|shape| shape.entity == *entity)
            .unwrap();
        assert_eq!(shape.origin, record.origin);
        let expected = match path.root_anchor().as_str() {
            "left" => 0.75,
            "right" => 1.5,
            _ => panic!("unexpected Prefab collision anchor"),
        };
        assert_eq!(
            owner.collider(record.collider).unwrap().translation().x,
            4.0
        );
        assert_eq!(owner.body(record.body).unwrap().translation().x, 4.0);
        let vertices = native_vertices(&owner, *entity);
        let minimum = vertices
            .iter()
            .map(|point| point.x)
            .fold(f32::INFINITY, f32::min);
        let maximum = vertices
            .iter()
            .map(|point| point.x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(maximum - minimum, expected * 2.0);
        assert!((shape.vertices[1].x - shape.vertices[0].x - expected * 2.0).abs() < 1e-5);
    }
    drop(owner);
    drop(sources);
    harness.finish();
}

#[test]
fn malformed_native_values_duplicate_foreign_and_stale_receipts_refuse_before_solver_changes() {
    let registry = native_registry();
    let mut scene = SourceScene::new(document(), "native-refusals");
    scene.source = source("native-refusals", SourceDocumentKind::Scene);
    let mut case = NativeCase::new(vec![scene], &registry, ExecutionProfile::Serial, |_| {});
    let (receipt, revision, entity, record, before) = case.admit(|world, owner, scenes| {
        let inputs = collision_inputs(scenes);
        owner.reconcile(world, &inputs).unwrap();
        let receipt = inputs[0].instance.clone();
        let entity = resolve(world, &receipt, "box");
        (
            receipt,
            inputs[0].revision,
            entity,
            generated(owner, entity),
            owner.debug_snapshot(),
        )
    });
    let original = (*before).clone();
    let native_before = native_state(&case.owner.lock().unwrap());
    let retained = record.clone();
    let malformed_state = native_before.clone();
    case.admit(move |world, owner, scenes| {
        let inputs = collision_inputs(scenes);
        world
            .get_mut::<CollisionDefinition>(entity)
            .unwrap()
            .half_width = f32::NAN;
        let malformed = owner.reconcile(world, &inputs).unwrap_err();
        assert!(malformed.has_errors());
        assert_eq!(owner.counts(), (2, 2));
        assert_withheld_capture(owner);
        assert_eq!(generated(owner, entity), retained);
        assert_eq!(owner.body(retained.body).unwrap().translation(), Vec2::ZERO);
        assert_eq!(owner.body(retained.body).unwrap().linvel(), Vec2::ZERO);
        assert_native_unchanged(owner, &malformed_state, None);
        world
            .get_mut::<CollisionDefinition>(entity)
            .unwrap()
            .half_width = 0.5;
        let duplicate = owner.reconcile(world, &[inputs[0], inputs[0]]).unwrap_err();
        assert!(
            duplicate.iter().any(|diagnostic| diagnostic.code().as_str()
                == "fixture.motion.collision.duplicate-input")
        );
        assert_native_unchanged(owner, &malformed_state, None);
        assert_withheld_capture(owner);
    });
    let mut other = NativeCase::new(vec![], &registry, ExecutionProfile::Serial, |_| {});
    let bound_owner = Arc::clone(&case.owner);
    let foreign_receipt = receipt.clone();
    let foreign_state = native_before.clone();
    other.admit(move |world, fresh, _| {
        let wrong_world = bound_owner
            .lock()
            .unwrap()
            .reconcile(world, &[])
            .unwrap_err();
        assert!(
            wrong_world
                .iter()
                .any(|diagnostic| diagnostic.code().as_str()
                    == "fixture.motion.collision.world-mismatch")
        );
        assert_native_unchanged(&bound_owner.lock().unwrap(), &foreign_state, None);
        let foreign = fresh
            .reconcile(
                world,
                &[CollisionInput {
                    instance: &foreign_receipt,
                    revision,
                }],
            )
            .unwrap_err();
        assert!(
            foreign.iter().any(|diagnostic| diagnostic.code().as_str()
                == "fixture.motion.collision.invalid-receipt")
        );
        assert_eq!(fresh.counts(), (0, 0));
        assert_withheld_capture(fresh);
    });
    other.finish();
    let materialization_registry = registry.clone();
    case.mutate(move |world, _, scenes| {
        let scene = &mut scenes[0];
        let removed = scene.session.apply_patch(
            &ScenePatchDocument::new([ScenePatchOperation::RemoveComponent {
                entity: id("box"),
                component: ComponentTypeId::new(COMPONENT),
            }]),
            &materialization_registry,
        );
        assert!(removed.applied, "{:?}", removed.diagnostics);
        let report = scene
            .session
            .sync_world(
                world,
                &materialization_registry,
                &scene.source,
                &scene.occurrence,
            )
            .unwrap();
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    });
    let retired = record.clone();
    case.admit(move |world, owner, scenes| {
        let stale = owner
            .reconcile(
                world,
                &[CollisionInput {
                    instance: &receipt,
                    revision,
                }],
            )
            .unwrap_err();
        assert!(
            stale.iter().any(|diagnostic| diagnostic.code().as_str()
                == "fixture.motion.collision.invalid-receipt")
        );
        assert_eq!(owner.counts(), (2, 2));
        assert_eq!(generated(owner, entity), retired);
        assert_native_unchanged(owner, &native_before, None);
        assert_withheld_capture(owner);
        owner.reconcile(world, &collision_inputs(scenes)).unwrap();
        assert_eq!(owner.counts(), (1, 1));
        assert!(owner.body(retired.body).is_none());
        assert!(owner.collider(retired.collider).is_none());
        owner.close();
        let closed = owner.reconcile(world, &[]).unwrap_err();
        assert!(closed.has_errors());
        assert!(owner.step_and_writeback(world).is_err());
        assert_eq!(owner.counts(), (0, 0));
        assert_withheld_capture(owner);
    });
    assert_eq!(*before, original);
    case.finish();
    assert_eq!(*before, original);
}

#[test]
fn late_invalid_input_and_source_mismatch_preserve_every_native_pair_before_same_entity_retry() {
    use nara_diagnostic::DiagnosticValueRef;

    let registry = native_registry();
    let mut scenes = vec![
        SourceScene::new(document(), "first"),
        SourceScene::new(document(), "last"),
    ];
    scenes[1].source = source("invalid-source", SourceDocumentKind::Scene);
    let mut case = NativeCase::new(scenes, &registry, ExecutionProfile::Serial, |_| {});
    for _ in 0..5 {
        case.admit(|world, owner, scenes| {
            owner.reconcile(world, &collision_inputs(scenes)).unwrap();
            owner.step_and_writeback(world).unwrap();
        });
    }
    let original_state = native_state(&case.owner.lock().unwrap());
    let snapshot = case.owner.lock().unwrap().debug_snapshot();
    let original_snapshot = (*snapshot).clone();
    case.mutate(|world, _, scenes| {
        let inputs = collision_inputs(scenes);
        let changed = resolve(world, inputs[0].instance, "box");
        let invalid = resolve(world, inputs[1].instance, "box");
        world
            .get_mut::<CollisionDefinition>(changed)
            .unwrap()
            .half_width = 1.0;
        world
            .get_mut::<CollisionDefinition>(invalid)
            .unwrap()
            .center
            .x = 4097.0;
    });
    case.admit(move |world, owner, scenes| {
        let inputs = collision_inputs(scenes);
        let changed = resolve(world, inputs[0].instance, "box");
        let invalid = resolve(world, inputs[1].instance, "box");
        let previous = generated(owner, changed);
        let failure = owner.reconcile(world, &inputs).unwrap_err();
        let diagnostic = failure
            .iter()
            .find(|diagnostic| {
                diagnostic.code().as_str() == "fixture.motion.collision.invalid-geometry"
            })
            .unwrap();
        let field = |key: &str| {
            diagnostic
                .fields()
                .iter()
                .find(|field| field.key().as_str() == key)
                .unwrap()
                .value()
        };
        assert_eq!(field("input-index"), DiagnosticValueRef::Unsigned(1));
        assert_eq!(field("field"), DiagnosticValueRef::Identifier("center"));
        assert_eq!(
            field("source"),
            DiagnosticValueRef::Identifier("invalid-source")
        );
        assert_eq!(
            field("occurrence-slot"),
            DiagnosticValueRef::Identifier("last")
        );
        assert_eq!(field("entity"), DiagnosticValueRef::Identifier("box"));
        assert_eq!(
            field("component"),
            DiagnosticValueRef::Identifier(COMPONENT)
        );
        assert_eq!(
            field("revision"),
            DiagnosticValueRef::Unsigned(scenes[1].session.revision().generation())
        );
        assert_native_unchanged(owner, &original_state, None);
        assert_withheld_capture(owner);
        assert!(owner.step_and_writeback(world).is_err());
        assert_native_unchanged(owner, &original_state, None);
        world
            .get_mut::<CollisionDefinition>(invalid)
            .unwrap()
            .center
            .x = 0.0;
        let source_context = world.get::<SceneEntitySource>(invalid).unwrap().clone();
        world.entity_mut(invalid).remove::<SceneEntitySource>();
        let failure = owner.reconcile(world, &inputs).unwrap_err();
        assert!(
            failure.iter().any(|diagnostic| diagnostic.code().as_str()
                == "fixture.motion.collision.source-mismatch")
        );
        assert_native_unchanged(owner, &original_state, None);
        assert_withheld_capture(owner);
        world.entity_mut(invalid).insert(source_context);
        owner.reconcile(world, &inputs).unwrap();
        let replacement = generated(owner, changed);
        assert_ne!(replacement.body, previous.body);
        assert_ne!(replacement.collider, previous.collider);
        assert!(owner.body(previous.body).is_none());
        assert!(owner.collider(previous.collider).is_none());
        assert_eq!(
            owner
                .collider(replacement.collider)
                .unwrap()
                .shape()
                .as_cuboid()
                .unwrap()
                .half_extents
                .x,
            1.0
        );
        let after = owner.debug_snapshot();
        let shape = after
            .shapes
            .iter()
            .find(|shape| shape.entity == changed)
            .unwrap();
        assert!((shape.vertices[1].x - shape.vertices[0].x - 2.0).abs() < 1e-5);
        assert_native_unchanged(owner, &original_state, Some(changed));
        assert_current_capture(owner, &after);
        assert!(!after.solved);
        assert_ne!(after.input_stamp, original_snapshot.input_stamp);
        assert_eq!(
            after.input_stamp,
            Some(
                fixed_input_spatial_snapshot(world)
                    .unwrap()
                    .stamp()
                    .unwrap()
            )
        );
        assert_eq!(owner.counts(), (4, 4));
    });
    assert_eq!(*snapshot, original_snapshot);
    let owner = Arc::clone(&case.owner);
    case.finish();
    assert_eq!(owner.lock().unwrap().counts(), (0, 0));
    assert_eq!(*snapshot, original_snapshot);
}
