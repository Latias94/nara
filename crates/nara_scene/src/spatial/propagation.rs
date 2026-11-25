use bevy_ecs::{
    change_detection::{ContiguousRef, DetectChanges, DetectChangesMut, Ref, Tick},
    component::Component,
    entity::{Entity, EntityHashMap, EntityHashSet},
    entity_disabling::Disabled,
    lifecycle::{RemovedComponentReader, RemovedComponents},
    query::{Allow, Has, Or, QueryData, With},
    resource::Resource,
    system::{Local, Query, ResMut, SystemChangeTick, SystemParam},
    world::World,
};

use super::{ChildOf, Children, GlobalTransform2d, InheritedVisibility, Transform2d, Visibility};

/// Installs the world-owned change cursors for [`propagate_spatial`].
///
/// Register once per world before running propagation. Repeated registration is safe, and
/// the first propagation also initializes spatial entities already in the world.
pub fn register_spatial(world: &mut World) {
    world.init_resource::<DirtySpatial>();
}

/// System parameter for [`propagate_spatial`].
#[derive(SystemParam)]
pub struct SpatialPropagation<'w, 's> {
    transforms: SpatialEdits<'w, 's, Transform2d>,
    visibility: SpatialEdits<'w, 's, Visibility>,
    parents: SpatialEdits<'w, 's, ChildOf>,
    removed_transforms: RemovedComponents<'w, 's, Transform2d>,
    removed_visibility: RemovedComponents<'w, 's, Visibility>,
    removed_parents: RemovedComponents<'w, 's, ChildOf>,
    outputs: Query<
        'w,
        's,
        Entity,
        (
            Or<(With<GlobalTransform2d>, With<InheritedVisibility>)>,
            Allow<Disabled>,
        ),
    >,
    nodes: Query<'w, 's, SpatialNode, Allow<Disabled>>,
    derived: Query<'w, 's, DerivedSpatial, Allow<Disabled>>,
    dirty: ResMut<'w, DirtySpatial>,
    ticks: SystemChangeTick,
    work: Local<'s, PropagationWork>,
}

/// Updates derived spatial values for changed branches of the native Bevy hierarchy.
///
/// Run after local writes and any deferred hierarchy commands, and before systems that read the
/// resulting globals. Multiple schedules may run this system on the same world. Entities without
/// a local transform or visibility pass through the corresponding parent value. Cyclic branches
/// keep their previous results until their relationships are repaired.
pub fn propagate_spatial(mut spatial: SpatialPropagation) {
    let now = spatial.ticks.this_run();
    let state = &mut *spatial.dirty;
    let dirty = &mut state.entities;
    let missed = queue_removed(
        &spatial.removed_transforms,
        &mut state.removed_transforms,
        dirty,
    ) | queue_removed(
        &spatial.removed_visibility,
        &mut state.removed_visibility,
        dirty,
    ) | queue_removed(&spatial.removed_parents, &mut state.removed_parents, dirty);
    if missed {
        dirty.extend(spatial.outputs.iter());
    }
    let since = state.last_pass.replace(now);
    queue_changed(&spatial.transforms, since, now, dirty);
    queue_changed(&spatial.visibility, since, now, dirty);
    queue_changed(&spatial.parents, since, now, dirty);
    if dirty.is_empty() {
        return;
    }

    spatial.work.find_branches(dirty, &spatial.nodes);
    dirty.clear();

    while let Some((entity, parent)) = spatial.work.branches.pop() {
        let Ok(node) = spatial.nodes.get(entity) else {
            continue;
        };
        let current = parent.with_local(node.transform, node.visibility);
        if let Ok(mut derived) = spatial.derived.get_mut(entity) {
            if let Some(global) = &mut derived.global {
                global.set_if_neq(current.global);
            }
            if let Some(visibility) = &mut derived.visibility {
                visibility.set_if_neq(current.visibility);
            }
        }
        if let Some(children) = node.children {
            spatial
                .work
                .branches
                .extend(children.iter().map(|&child| (child, current)));
        }
    }
}

#[derive(Resource, Default)]
struct DirtySpatial {
    entities: EntityHashSet,
    last_pass: Option<Tick>,
    removed_transforms: RemovedComponentReader<Transform2d>,
    removed_visibility: RemovedComponentReader<Visibility>,
    removed_parents: RemovedComponentReader<ChildOf>,
}

#[derive(QueryData)]
struct SpatialNode {
    entity: Entity,
    parent: Option<&'static ChildOf>,
    children: Option<&'static Children>,
    transform: Option<&'static Transform2d>,
    visibility: Option<&'static Visibility>,
}

#[derive(QueryData)]
#[query_data(mutable)]
struct DerivedSpatial {
    global: Option<&'static mut GlobalTransform2d>,
    visibility: Option<&'static mut InheritedVisibility>,
}

type SpatialEdits<'w, 's, T> = Query<'w, 's, (Entity, Ref<'static, T>, Has<Disabled>)>;

fn queue_removed<T: Component>(
    removed: &RemovedComponents<T>,
    reader: &mut RemovedComponentReader<T>,
    dirty: &mut EntityHashSet,
) -> bool {
    if let Some(messages) = removed.messages() {
        let missed = reader.missed_messages(messages) != 0;
        dirty.extend(reader.read(messages).cloned().map(Entity::from));
        missed
    } else {
        false
    }
}

fn queue_changed<T: Component>(
    query: &SpatialEdits<T>,
    since: Option<Tick>,
    now: Tick,
    dirty: &mut EntityHashSet,
) {
    let needs_update = |changed: Tick| since.is_none_or(|since| changed.is_newer_than(since, now));

    if let Ok(tables) = query.contiguous_iter() {
        for (entities, values, _) in tables {
            let (_, ticks) = ContiguousRef::split(values);
            // This system's previous run is no newer than the last pass in this world.
            // A clean table for the query therefore also contains no changes for the shared pass.
            if since.is_some() && ticks.summary_tick_is_changed() == Some(false) {
                continue;
            }
            dirty.extend(
                entities
                    .iter()
                    .zip(ticks.changed())
                    .filter_map(|(&entity, &changed)| needs_update(changed).then_some(entity)),
            );
        }
    } else {
        dirty.extend(
            query.iter().filter_map(|(entity, value, _)| {
                needs_update(value.last_changed()).then_some(entity)
            }),
        );
    }
}

#[derive(Clone, Copy, Default)]
struct SpatialValue {
    global: GlobalTransform2d,
    visibility: InheritedVisibility,
}

impl SpatialValue {
    fn with_local(self, transform: Option<&Transform2d>, visibility: Option<&Visibility>) -> Self {
        let global = transform.map_or(self.global, |local| {
            GlobalTransform2d(self.global.0 * local.affine(), self.global.1 + local.z)
        });
        let visibility = match visibility.copied().unwrap_or_default() {
            Visibility::Inherited => self.visibility,
            Visibility::Visible => InheritedVisibility::VISIBLE,
            Visibility::Hidden => InheritedVisibility::HIDDEN,
        };
        Self { global, visibility }
    }
}

#[derive(Clone, Copy)]
enum AncestorState {
    Visiting,
    Clean(SpatialValue),
    Dirty,
    Cyclic,
}

#[derive(Default)]
struct PropagationWork {
    ancestors: EntityHashMap<AncestorState>,
    path: Vec<(Entity, Option<Transform2d>, Option<Visibility>)>,
    branches: Vec<(Entity, SpatialValue)>,
}

impl PropagationWork {
    fn find_branches(
        &mut self,
        dirty: &EntityHashSet,
        nodes: &Query<SpatialNode, Allow<Disabled>>,
    ) {
        for &entity in dirty {
            let mut cursor = Some(entity);
            let mut state = loop {
                let Some(ancestor) = cursor else {
                    break AncestorState::Clean(SpatialValue::default());
                };
                if let Some(&state) = self.ancestors.get(&ancestor) {
                    break match state {
                        AncestorState::Visiting => AncestorState::Cyclic,
                        state => state,
                    };
                }
                let Ok(node) = nodes.get(ancestor) else {
                    break AncestorState::Clean(SpatialValue::default());
                };
                self.ancestors.insert(ancestor, AncestorState::Visiting);
                self.path
                    .push((ancestor, node.transform.copied(), node.visibility.copied()));
                cursor = node.parent.map(ChildOf::parent);
            };

            for (ancestor, transform, visibility) in self.path.drain(..).rev() {
                if let AncestorState::Clean(parent) = state {
                    state = if dirty.contains(&ancestor) {
                        self.branches.push((ancestor, parent));
                        AncestorState::Dirty
                    } else {
                        AncestorState::Clean(
                            parent.with_local(transform.as_ref(), visibility.as_ref()),
                        )
                    };
                }
                self.ancestors.insert(ancestor, state);
            }
        }
        self.ancestors.clear();
    }
}
