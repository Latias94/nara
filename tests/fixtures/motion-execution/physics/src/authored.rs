//! Finite authored boxes reconciled into one concrete Rapier 0.33 world.

mod diagnostics;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_core::Vec2;
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
use rapier2d::{
    math::Vector,
    prelude::{
        Collider, ColliderBuilder, ColliderHandle, PhysicsWorld, RigidBody, RigidBodyBuilder,
        RigidBodyHandle,
    },
};

const COMPONENT_ID: &str = "fixture.motion.CollisionDefinition";
const PROPERTY_IDS: [&str; 4] = ["center", "half_width", "half_height", "dynamic"];
const MIN_HALF_EXTENT: f32 = 0.001;
const MAX_HALF_EXTENT: f32 = 64.0;
const MAX_CENTER_COORDINATE: f32 = 4096.0;

/// A translation-only box. Both half extents are finite and within `[0.001, 64]`.
///
/// The native derivation additionally requires each center coordinate to be finite and within
/// `[-4096, 4096]`. This spatial admission bound is independent of Schema's scalar ranges.
#[derive(Debug, Clone, Copy, PartialEq, Component, PersistentComponent)]
#[nara(
    id = "fixture.motion.CollisionDefinition",
    version = 1,
    alias = "Collision definition",
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
}

/// An immutable numeric capture of one actual native collider in world coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct DebugShape {
    pub entity: Entity,
    pub origin: CollisionOrigin,
    /// Counterclockwise corners, starting at the collider's local bottom-left corner.
    pub vertices: [Vec2; 4],
}

/// A detached capture that remains usable after edits, removal, and owner close.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DebugSnapshot {
    pub shapes: Vec<DebugShape>,
}

struct Candidate {
    definition: CollisionDefinition,
    origin: CollisionOrigin,
}

/// Owns one native world; successful inputs are its complete selected collision inventory.
///
/// Unchanged definitions retain their pair and solver state. Changing any authored field rebuilds
/// that pair and resets its dynamics. The owner binds to the first successfully reconciled World.
pub struct CollisionWorld {
    native: Option<PhysicsWorld>,
    world: Option<WorldId>,
    records: BTreeMap<Entity, GeneratedCollision>,
    snapshot: Arc<DebugSnapshot>,
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
        }
    }

    /// Validates every receipt and definition before changing any native state.
    ///
    /// Omitting a previously selected receipt retires all of its output. Passing no inputs unloads
    /// all output. The supplied revision is metadata accepted by the caller; it is not inferred
    /// from ECS state or a materialized entity ID.
    pub fn reconcile(
        &mut self,
        world: &World,
        inputs: &[CollisionInput<'_>],
    ) -> Result<(), Box<DiagnosticReport>> {
        let native = self.native.as_mut().ok_or_else(diagnostics::closed)?;
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
        let candidates = validate_inputs(world, inputs)?;
        let retired: Vec<_> = self
            .records
            .iter()
            .filter(|(entity, record)| {
                candidates
                    .get(*entity)
                    .is_none_or(|candidate| candidate.definition != record.definition)
            })
            .map(|(entity, _)| *entity)
            .collect();

        for entity in retired {
            if let Some(record) = self.records.remove(&entity) {
                native.remove_body(record.body);
            }
        }
        for (entity, candidate) in candidates {
            if let Some(record) = self.records.get_mut(&entity) {
                record.origin = candidate.origin;
                continue;
            }
            let definition = candidate.definition;
            let body = if definition.dynamic {
                RigidBodyBuilder::dynamic()
            } else {
                RigidBodyBuilder::fixed()
            }
            .translation(Vector::new(definition.center.x, definition.center.y))
            .lock_rotations();
            let (body, collider) = native.insert(
                body,
                ColliderBuilder::cuboid(definition.half_width, definition.half_height)
                    .restitution(0.0),
            );
            self.records.insert(
                entity,
                GeneratedCollision {
                    body,
                    collider,
                    origin: candidate.origin,
                    definition,
                },
            );
        }
        self.snapshot = Arc::new(capture(native, &self.records)?);
        self.world = Some(world.id());
        Ok(())
    }

    /// Advances one default Rapier timestep, then captures the resulting native geometry.
    pub fn step(&mut self) -> Result<(), Box<DiagnosticReport>> {
        let native = self.native.as_mut().ok_or_else(diagnostics::closed)?;
        native.step();
        self.snapshot = Arc::new(capture(native, &self.records)?);
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
    }
}

fn validate_inputs(
    world: &World,
    inputs: &[CollisionInput<'_>],
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
            for (field, invalid) in invalid_fields {
                if invalid {
                    report.push(diagnostics::with_property(
                        entity_diagnostic(
                            "fixture.motion.collision.invalid-geometry",
                            "Collision geometry must be finite and within its admitted bounds",
                        ),
                        field,
                    ));
                }
            }
            candidates.insert(
                entity,
                Candidate {
                    definition,
                    origin: CollisionOrigin::from_source(source, input.revision),
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

fn capture(
    native: &PhysicsWorld,
    records: &BTreeMap<Entity, GeneratedCollision>,
) -> Result<DebugSnapshot, Box<DiagnosticReport>> {
    let mut shapes = Vec::with_capacity(records.len());
    for (entity, record) in records {
        let native_error = || {
            diagnostics::single(diagnostics::with_origin(
                diagnostics::error(
                    "fixture.motion.collision.native-state",
                    "An owned native pair or its supported box shape is unavailable",
                ),
                *entity,
                &record.origin,
            ))
        };
        let collider = native
            .colliders
            .get(record.collider)
            .ok_or_else(native_error)?;
        if native.bodies.get(record.body).is_none() || collider.parent() != Some(record.body) {
            return Err(native_error());
        }
        let extent = collider
            .shape()
            .as_cuboid()
            .ok_or_else(native_error)?
            .half_extents;
        let vertices = [
            Vector::new(-extent.x, -extent.y),
            Vector::new(extent.x, -extent.y),
            Vector::new(extent.x, extent.y),
            Vector::new(-extent.x, extent.y),
        ]
        .map(|vertex| {
            let position = collider.position().transform_point(vertex);
            Vec2::new(position.x, position.y)
        });
        shapes.push(DebugShape {
            entity: *entity,
            origin: record.origin.clone(),
            vertices,
        });
    }
    Ok(DebugSnapshot { shapes })
}
