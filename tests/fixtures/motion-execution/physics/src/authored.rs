//! Transformed local boxes reconciled into one concrete Rapier 0.33 world.

mod diagnostics;
mod geometry;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_core::{Mat3, Vec2};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Component, Entity, World, bevy_ecs::world::WorldId};
use nara_identity::{EntityLookup, SpawnedSceneInstance, WorldIdentityDomain};
use nara_reflect::{
    ComponentFieldId, ComponentPropertyPath, ComponentRegistryBuilder, ComponentRegistryError,
    ComponentTypeId, PersistentComponent,
};
use nara_scene::{
    SceneAuthoringRevision, SceneEntityProvenance, SceneEntitySource, SceneOccurrenceKey,
    SourceDocumentKey,
};
use nara_transform::{
    SpatialSnapshot2d, SpatialSnapshotStamp, Transform2d, fixed_input_spatial_snapshot,
};
use rapier2d::prelude::{Collider, ColliderHandle, PhysicsWorld, RigidBody, RigidBodyHandle};

const COMPONENT_ID: &str = "fixture.motion.LocalBox";
const PROPERTY_IDS: [&str; 4] = ["center", "half_width", "half_height", "dynamic"];
const MIN_HALF_EXTENT: f32 = 0.001;
const MAX_HALF_EXTENT: f32 = 64.0;
const MAX_CENTER_COORDINATE: f32 = 4096.0;

/// An entity-local box. Both half extents are finite and within `[0.001, 64]`.
///
/// The center is a local offset, independent of the entity's Transform. Local center coordinates,
/// transformed world corners and fixed-body native corners are bounded by `[-4096, 4096]`.
/// Fixed bodies admit full affine geometry only when Rapier retains all four corners within
/// `1e-4 + 1e-5 * abs(coordinate)`. Singular linear maps and edges that cannot be normalized in
/// native precision are refused. Dynamic bodies require an actual root and unit local scale.
#[derive(Debug, Clone, Copy, PartialEq, Component, PersistentComponent)]
#[nara(
    id = "fixture.motion.LocalBox",
    version = 1,
    alias = "Local box",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct CollisionDefinition {
    #[nara(id = "center", alias = "Center")]
    pub center: Vec2,
    #[nara(
        id = "half_width",
        alias = "Half width",
        range(min_inclusive = 0.001_f32, max_inclusive = 64.0_f32)
    )]
    pub half_width: f32,
    #[nara(
        id = "half_height",
        alias = "Half height",
        range(min_inclusive = 0.001_f32, max_inclusive = 64.0_f32)
    )]
    pub half_height: f32,
    #[nara(id = "dynamic", alias = "Dynamic")]
    pub dynamic: bool,
}

/// Registers exactly the derived persistent schema and native value codec, without a solver.
pub fn register_collision(
    registry: &mut ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component::<CollisionDefinition>()?;
    Ok(())
}

/// One selected live receipt and the authoring revision accepted by its owner.
#[derive(Debug, Clone, Copy)]
pub struct CollisionInput<'a> {
    pub instance: &'a SpawnedSceneInstance,
    pub revision: SceneAuthoringRevision,
}

/// Typed authored attribution shared by the generated body, collider, and debug shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionOrigin {
    pub source: SourceDocumentKey,
    pub occurrence: SceneOccurrenceKey,
    pub provenance: SceneEntityProvenance,
    pub component: ComponentTypeId,
    /// Stable selectors in the order `center`, `half_width`, `half_height`, `dynamic`.
    pub properties: [ComponentPropertyPath; 4],
    pub revision: SceneAuthoringRevision,
}

impl CollisionOrigin {
    fn from_source(source: &SceneEntitySource, revision: SceneAuthoringRevision) -> Self {
        Self {
            source: source.containing_source().clone(),
            occurrence: source.occurrence().clone(),
            provenance: source.provenance().clone(),
            component: ComponentTypeId::new(COMPONENT_ID),
            properties: PROPERTY_IDS
                .map(|id| ComponentPropertyPath::new(ComponentFieldId::new(id))),
            revision,
        }
    }
}

/// A borrowed observation of the native pair owned for one runtime Entity.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedCollision {
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
    pub origin: CollisionOrigin,
    definition: CollisionDefinition,
    scene_source: SceneEntitySource,
    source: CollisionSpatialSource,
    last_writeback: Option<Transform2d>,
}

/// An immutable numeric capture of one actual native collider in world coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct DebugShape {
    pub entity: Entity,
    pub origin: CollisionOrigin,
    /// Actual native corners in counterclockwise order, starting at the smallest `(x, y)`.
    pub vertices: [Vec2; 4],
}

/// A detached capture that remains usable after edits, removal, and owner close.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DebugSnapshot {
    pub shapes: Vec<DebugShape>,
    /// The genuine fixed-input publication admitted for this capture, or None when unavailable.
    pub input_stamp: Option<SpatialSnapshotStamp>,
    /// Whether the native world has been solved and written back for this input publication.
    pub solved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CollisionSpatialSource {
    local: Transform2d,
    parent: Option<Entity>,
    global: Mat3,
}

struct Candidate {
    definition: CollisionDefinition,
    origin: CollisionOrigin,
    scene_source: SceneEntitySource,
    source: CollisionSpatialSource,
    pair: geometry::PreparedPair,
}

impl Candidate {
    fn preserves(&self, record: &GeneratedCollision, solved_same_input: bool) -> bool {
        self.definition == record.definition
            && if self.definition.dynamic {
                self.source.local == record.last_writeback.unwrap_or(record.source.local)
                    || (solved_same_input && self.source == record.source)
            } else {
                self.source == record.source
            }
    }
}

/// Owns one native world; successful inputs are its complete selected collision inventory.
///
/// Unchanged geometry and domain-written poses retain their pair and solver state. External
/// authored shape or pose edits rebuild only that pair. The first successful admission binds the
/// owner to its World. A failed admission withholds stepping and current debug publication.
pub struct CollisionWorld {
    native: Option<PhysicsWorld>,
    world: Option<WorldId>,
    records: BTreeMap<Entity, GeneratedCollision>,
    snapshot: Arc<DebugSnapshot>,
    admission: Option<SpatialSnapshotStamp>,
    last_solve: Option<SpatialSnapshotStamp>,
}

impl Default for CollisionWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl CollisionWorld {
    /// Uses Rapier's default integration parameters and gravity `(0, -9.81)`.
    pub fn new() -> Self {
        Self {
            native: Some(PhysicsWorld::default()),
            world: None,
            records: BTreeMap::new(),
            snapshot: Arc::new(DebugSnapshot::default()),
            admission: None,
            last_solve: None,
        }
    }

    /// Validates every receipt, captured Transform and native shape before native mutation.
    ///
    /// Omitting a previously selected receipt retires all of its output. Passing no inputs unloads
    /// all output. The supplied revision is metadata accepted by the caller; it is not inferred
    /// from ECS state or a materialized entity ID. Every call requires the current fixed-input
    /// role, including an empty inventory. Input rejection preserves native pairs and revokes
    /// admission; a native failure after activation does not promise rollback.
    pub fn reconcile(
        &mut self,
        world: &World,
        inputs: &[CollisionInput<'_>],
    ) -> Result<(), Box<DiagnosticReport>> {
        self.admission = None;
        self.snapshot = Arc::new(DebugSnapshot::default());
        self.native.as_ref().ok_or_else(diagnostics::closed)?;
        if self.world.is_some_and(|bound| bound != world.id()) {
            let mut diagnostic = diagnostics::error(
                "fixture.motion.collision.world-mismatch",
                "Collision ownership is bound to a different World",
            );
            if let Some(input) = inputs.first() {
                diagnostic = diagnostics::with_input(diagnostic, input, 0);
            }
            return Err(diagnostics::single(diagnostic));
        }
        let spatial = fixed_input_spatial_snapshot(world)
            .map_err(|error| diagnostics::single(diagnostics::spatial_input(error)))?;
        let stamp = spatial
            .stamp()
            .map_err(|error| diagnostics::single(diagnostics::spatial_input(error)))?;
        let candidates = validate_inputs(world, inputs, &spatial)?;
        let solved_same_input = self.last_solve == Some(stamp);
        let retired: Vec<_> = self
            .records
            .iter()
            .filter(|(entity, record)| {
                candidates
                    .get(*entity)
                    .is_none_or(|candidate| !candidate.preserves(record, solved_same_input))
            })
            .map(|(entity, _)| *entity)
            .collect();
        let native = self.native.as_mut().ok_or_else(diagnostics::closed)?;

        for entity in retired {
            if let Some(record) = self.records.remove(&entity) {
                native.remove_body(record.body);
            }
        }
        for (entity, candidate) in candidates {
            if let Some(record) = self.records.get_mut(&entity) {
                record.origin = candidate.origin;
                record.scene_source = candidate.scene_source;
                record.source = candidate.source;
                continue;
            }
            let (body, collider) = native.insert(candidate.pair.body, candidate.pair.collider);
            self.records.insert(
                entity,
                GeneratedCollision {
                    body,
                    collider,
                    origin: candidate.origin,
                    definition: candidate.definition,
                    scene_source: candidate.scene_source,
                    source: candidate.source,
                    last_writeback: None,
                },
            );
        }
        self.snapshot = Arc::new(capture(native, &self.records, stamp, false)?);
        self.world = Some(world.id());
        self.admission = Some(stamp);
        Ok(())
    }

    /// Solves the most recently admitted current input once and batches dynamic root poses.
    ///
    /// All writeback targets are validated before solving. Native failure after solving begins
    /// withholds publication and consumes the cut; it does not claim native rollback. Call this
    /// before normal fixed Resolve, which publishes the original Transform owner's completed cut.
    pub fn step_and_writeback(&mut self, world: &mut World) -> Result<(), Box<DiagnosticReport>> {
        let admission = self.admission.take();
        self.snapshot = Arc::new(DebugSnapshot::default());
        self.native.as_ref().ok_or_else(diagnostics::closed)?;
        let spatial = fixed_input_spatial_snapshot(world)
            .map_err(|error| diagnostics::single(diagnostics::spatial_input(error)))?;
        let stamp = spatial
            .stamp()
            .map_err(|error| diagnostics::single(diagnostics::spatial_input(error)))?;
        if self.last_solve == Some(stamp) {
            return Err(diagnostics::single(diagnostics::error(
                "fixture.motion.collision.already-solved",
                "Collision input has already been consumed by a native solve",
            )));
        }
        if self.world != Some(world.id()) || admission != Some(stamp) {
            return Err(diagnostics::single(diagnostics::error(
                "fixture.motion.collision.step-admission",
                "Collision stepping requires successful admission of this exact current input",
            )));
        }
        validate_writeback_targets(world, &self.records)?;
        let native = self.native.as_mut().ok_or_else(diagnostics::closed)?;
        let _ = capture(native, &self.records, stamp, false)?;
        self.last_solve = Some(stamp);
        native.step();
        let mut writebacks = Vec::new();
        for (entity, record) in &self.records {
            let body = native
                .bodies
                .get(record.body)
                .ok_or_else(|| diagnostics::native_state(*entity, &record.origin))?;
            let pose = geometry::body_transform(body).ok_or_else(|| {
                diagnostics::single(diagnostics::with_origin(
                    diagnostics::error(
                        "fixture.motion.collision.nonfinite-native-pose",
                        "Native solving produced a nonfinite pose and cannot be rolled back",
                    ),
                    *entity,
                    &record.origin,
                ))
            })?;
            if record.definition.dynamic {
                writebacks.push((*entity, pose));
            }
        }
        let snapshot = capture(native, &self.records, stamp, true)?;
        for (entity, pose) in writebacks {
            *world
                .get_mut::<Transform2d>(entity)
                .expect("writeback targets were validated before the ECS-free native solve") = pose;
            self.records
                .get_mut(&entity)
                .expect("owned records cannot change during native solving")
                .last_writeback = Some(pose);
        }
        self.snapshot = Arc::new(snapshot);
        Ok(())
    }

    pub fn records(&self) -> impl ExactSizeIterator<Item = (&Entity, &GeneratedCollision)> {
        self.records.iter()
    }

    pub fn body(&self, handle: RigidBodyHandle) -> Option<&RigidBody> {
        self.native.as_ref()?.bodies.get(handle)
    }

    pub fn collider(&self, handle: ColliderHandle) -> Option<&Collider> {
        self.native.as_ref()?.colliders.get(handle)
    }

    /// Returns `(rigid_bodies, colliders)` from the actual native sets.
    pub fn counts(&self) -> (usize, usize) {
        self.native.as_ref().map_or((0, 0), |native| {
            (native.bodies.len(), native.colliders.len())
        })
    }

    /// Reports active contacts from the last solve whose two collider handles are still live.
    pub fn contacts(&self) -> bool {
        self.native.as_ref().is_some_and(|native| {
            native.contact_pairs().any(|pair| {
                pair.has_any_active_contact()
                    && native.colliders.get(pair.collider1).is_some()
                    && native.colliders.get(pair.collider2).is_some()
            })
        })
    }

    /// Reports active contact for exactly the two entities' currently owned native colliders.
    pub fn contact_between(&self, first: Entity, second: Entity) -> bool {
        let (Some(native), Some(first), Some(second)) = (
            self.native.as_ref(),
            self.records.get(&first),
            self.records.get(&second),
        ) else {
            return false;
        };
        native.colliders.get(first.collider).is_some()
            && native.colliders.get(second.collider).is_some()
            && native
                .contact_pair(first.collider, second.collider)
                .is_some_and(|pair| pair.has_any_active_contact())
    }

    pub fn debug_snapshot(&self) -> Arc<DebugSnapshot> {
        Arc::clone(&self.snapshot)
    }

    /// Immediately drops all native state and permanently rejects reconciliation and stepping.
    ///
    /// This is synchronous solver cleanup, independent of any Host or device-thread close.
    pub fn close(&mut self) {
        self.native = None;
        self.records.clear();
        self.snapshot = Arc::new(DebugSnapshot::default());
        self.admission = None;
    }
}

fn validate_inputs(
    world: &World,
    inputs: &[CollisionInput<'_>],
    spatial: &SpatialSnapshot2d<'_>,
) -> Result<BTreeMap<Entity, Candidate>, Box<DiagnosticReport>> {
    let mut report = diagnostics::new_report();
    let mut selected_instances = BTreeSet::new();
    let mut selected_entities = BTreeSet::new();
    let mut selected_occurrences = BTreeSet::new();
    let mut candidates = BTreeMap::new();
    let identity = world.get_resource::<WorldIdentityDomain>();

    for (index, input) in inputs.iter().enumerate() {
        if !selected_instances.insert((input.instance.domain_id(), input.instance.instance_id())) {
            report.push(diagnostics::with_input(
                diagnostics::error(
                    "fixture.motion.collision.duplicate-input",
                    "Collision inputs contain the same Scene instance more than once",
                ),
                input,
                index,
            ));
            continue;
        }
        let current = identity.and_then(|identity| {
            identity
                .active_scene_instance(world, input.instance.instance_id())
                .ok()
        });
        if current.as_ref() != Some(input.instance) {
            report.push(diagnostics::with_input(
                diagnostics::error(
                    "fixture.motion.collision.invalid-receipt",
                    "Collision input is not the complete current receipt in this World",
                ),
                input,
                index,
            ));
        }
        let mut context: Option<&SceneEntitySource> = None;
        for entity_id in input.instance.entity_ids() {
            let EntityLookup::Resolved(entity) = input.instance.resolve(world, entity_id) else {
                report.push(diagnostics::with_entity(
                    diagnostics::with_input(
                        diagnostics::error(
                            "fixture.motion.collision.invalid-receipt",
                            "A collision input member does not resolve in this World",
                        ),
                        input,
                        index,
                    ),
                    entity_id,
                    None,
                    None,
                ));
                continue;
            };
            let source = world.get::<SceneEntitySource>(entity);
            let entity_diagnostic = |code, summary| {
                diagnostics::with_entity(
                    diagnostics::with_input(diagnostics::error(code, summary), input, index),
                    entity_id,
                    Some(entity),
                    source,
                )
            };
            if !selected_entities.insert(entity) {
                report.push(entity_diagnostic(
                    "fixture.motion.collision.duplicate-entity",
                    "Collision inputs resolve to the same runtime Entity more than once",
                ));
                continue;
            }
            let Some(source) = source.filter(|source| {
                source.instance_id == input.instance.instance_id() && source.entity_id == *entity_id
            }) else {
                report.push(entity_diagnostic(
                    "fixture.motion.collision.source-mismatch",
                    "A Scene entity source is missing or does not match its exact receipt member",
                ));
                continue;
            };
            if let Some(context) = context {
                if source.containing_source() != context.containing_source()
                    || source.occurrence() != context.occurrence()
                {
                    report.push(entity_diagnostic(
                        "fixture.motion.collision.source-mismatch",
                        "Scene instance members disagree on their containing source or occurrence",
                    ));
                }
            } else {
                context = Some(source);
                if !selected_occurrences.insert(source.occurrence().clone()) {
                    report.push(entity_diagnostic(
                        "fixture.motion.collision.duplicate-occurrence",
                        "Collision inputs contain the same logical Scene occurrence more than once",
                    ));
                }
            }
            let Some(definition) = world.get::<CollisionDefinition>(entity).copied() else {
                continue;
            };
            let invalid_fields = [
                (
                    "center",
                    !definition.center.is_finite()
                        || definition.center.abs().max_element() > MAX_CENTER_COORDINATE,
                ),
                (
                    "half_width",
                    !definition.half_width.is_finite()
                        || !(MIN_HALF_EXTENT..=MAX_HALF_EXTENT).contains(&definition.half_width),
                ),
                (
                    "half_height",
                    !definition.half_height.is_finite()
                        || !(MIN_HALF_EXTENT..=MAX_HALF_EXTENT).contains(&definition.half_height),
                ),
            ];
            let mut invalid_definition = false;
            for (field, invalid) in invalid_fields {
                if invalid {
                    invalid_definition = true;
                    report.push(diagnostics::with_property(
                        entity_diagnostic(
                            "fixture.motion.collision.invalid-geometry",
                            "Collision geometry must be finite and within its admitted bounds",
                        ),
                        field,
                    ));
                }
            }
            let (Ok(local), Ok(parent), Ok(global)) = (
                spatial.local(entity),
                spatial.parent(entity),
                spatial.global(entity),
            ) else {
                report.push(entity_diagnostic(
                    "fixture.motion.collision.missing-transform",
                    "Every collider requires an actual captured Transform in the fixed input",
                ));
                continue;
            };
            if invalid_definition {
                continue;
            }
            let source_spatial = CollisionSpatialSource {
                local,
                parent,
                global: global.matrix(),
            };
            let pair = match geometry::prepare(definition, source_spatial) {
                Ok(pair) => pair,
                Err(error) => {
                    report.push(diagnostics::with_property(
                        entity_diagnostic(error.code, error.summary),
                        error.field,
                    ));
                    continue;
                }
            };
            candidates.insert(
                entity,
                Candidate {
                    definition,
                    origin: CollisionOrigin::from_source(source, input.revision),
                    scene_source: source.clone(),
                    source: source_spatial,
                    pair,
                },
            );
        }
    }
    if report.has_errors() {
        Err(Box::new(report))
    } else {
        Ok(candidates)
    }
}

fn validate_writeback_targets(
    world: &World,
    records: &BTreeMap<Entity, GeneratedCollision>,
) -> Result<(), Box<DiagnosticReport>> {
    let mut report = diagnostics::new_report();
    for (entity, record) in records {
        if !record.definition.dynamic {
            continue;
        }
        if record.source.parent.is_some()
            || world.get::<nara_hierarchy::Parent>(*entity).is_some()
            || record.source.local.scale != Vec2::ONE
            || world.get::<Transform2d>(*entity) != Some(&record.source.local)
            || world.get::<CollisionDefinition>(*entity) != Some(&record.definition)
            || world.get::<SceneEntitySource>(*entity) != Some(&record.scene_source)
        {
            report.push(diagnostics::with_origin(
                diagnostics::error(
                    "fixture.motion.collision.writeback-target",
                    "A dynamic root writeback target no longer matches its admitted source",
                ),
                *entity,
                &record.origin,
            ));
        }
    }
    if report.has_errors() {
        Err(Box::new(report))
    } else {
        Ok(())
    }
}

fn capture(
    native: &PhysicsWorld,
    records: &BTreeMap<Entity, GeneratedCollision>,
    stamp: SpatialSnapshotStamp,
    solved: bool,
) -> Result<DebugSnapshot, Box<DiagnosticReport>> {
    let mut shapes = Vec::with_capacity(records.len());
    for (entity, record) in records {
        let native_error = || diagnostics::native_state(*entity, &record.origin);
        let collider = native
            .colliders
            .get(record.collider)
            .ok_or_else(native_error)?;
        if native
            .bodies
            .get(record.body)
            .and_then(geometry::body_transform)
            .is_none()
            || collider.parent() != Some(record.body)
        {
            return Err(native_error());
        }
        let vertices = geometry::native_vertices(collider).ok_or_else(native_error)?;
        shapes.push(DebugShape {
            entity: *entity,
            origin: record.origin.clone(),
            vertices,
        });
    }
    Ok(DebugSnapshot {
        shapes,
        input_stamp: Some(stamp),
        solved,
    })
}
