use nara_ecs::{
    Commands, Component, Entity, QueryState, ResMut, Resource, World,
    bevy_ecs::{
        entity_disabling::Disabled, relationship::RelationshipHookMode, world::CommandQueue,
    },
    schedule::IntoScheduleConfigs,
};

use super::*;

const DEEP_HIERARCHY_DEPTH: usize = 4_096;

fn root_last() -> HierarchyPlacement {
    HierarchyPlacement::last(HierarchyPlacementOwner::Root)
}

fn child_last(parent: Entity) -> HierarchyPlacement {
    HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent })
}

fn place_root(world: &mut World, entity: Entity) {
    HierarchyWriter::new(world)
        .place(entity, root_last())
        .expect("root placement should succeed");
}

#[test]
fn generation_resource_insertion_is_admitted_before_hierarchy_publication() {
    use nara_ecs::bevy_ecs::{lifecycle::Add, observer::On};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let hits = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&hits);
    world.add_observer(move |_: On<Add<HierarchyGenerationState>>| {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    world.flush();
    let result = HierarchyWriter::new(&mut world).place(entity, root_last());
    assert_eq!(result, Err(HierarchyError::LifecycleWork));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert!(!world.contains_resource::<HierarchyGenerationState>());
    assert!(!world.contains_resource::<HierarchyRoots>());
    assert!(world.get::<ChildOf>(entity).is_none());
    assert_eq!(world.entity(entity).archetype().components().len(), 0);
}

#[test]
fn cached_resource_targets_are_admitted_before_hierarchy_publication() {
    use nara_ecs::bevy_ecs::{
        lifecycle::Add,
        observer::{Observer, On},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    for generation in [false, true] {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let resource = if generation {
            world.init_resource::<HierarchyGenerationState>();
            let resource = world.component_id::<HierarchyGenerationState>().unwrap();
            world.remove_resource::<HierarchyGenerationState>().unwrap();
            resource
        } else {
            world.insert_resource(HierarchyRoots::new());
            let resource = world.component_id::<HierarchyRoots>().unwrap();
            world.remove_resource::<HierarchyRoots>().unwrap();
            resource
        };
        let cached = world.resource_entities().get(resource).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&hits);
        world.spawn(
            Observer::new(move |_: On<Add<()>>| {
                observed.fetch_add(1, Ordering::SeqCst);
            })
            .with_entity(cached)
            .with_component(resource),
        );
        world.flush();
        assert_eq!(
            HierarchyWriter::new(&mut world).place(entity, root_last()),
            Err(HierarchyError::LifecycleWork)
        );
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert!(!world.contains_resource::<HierarchyRoots>());
        assert!(!world.contains_resource::<HierarchyGenerationState>());
        assert_eq!(world.entity(entity).archetype().components().len(), 0);
    }
}

#[test]
fn ordered_batch_publishes_explicit_roots_and_children() {
    let mut world = World::new();
    let second_child = world.spawn_empty().id();
    let unrelated = world.spawn_empty().id();
    let first_root = world.spawn_empty().id();
    let first_child = world.spawn_empty().id();
    let second_root = world.spawn_empty().id();

    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: first_root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: second_root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: first_child,
                placement: child_last(first_root),
            },
            HierarchyOperation::Place {
                entity: second_child,
                placement: child_last(first_root),
            },
        ])
        .unwrap();

    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        [first_root, second_root]
    );
    assert_eq!(
        &world.get::<Children>(first_root).unwrap()[..],
        [first_child, second_child]
    );
    assert_eq!(
        world.get::<ChildOf>(first_child).map(ChildOf::parent),
        Some(first_root)
    );
    assert!(HierarchyPlacement::current(&world, unrelated).is_none());
    validate_hierarchy(&world).unwrap();
}

#[test]
fn deferred_batch_uses_the_same_ordered_transaction() {
    let mut world = World::new();
    let child = world.spawn_empty().id();
    let root = world.spawn_empty().id();
    let mut queue = CommandQueue::default();

    Commands::new(&mut queue, &world).apply_hierarchy_batch(vec![
        HierarchyOperation::Place {
            entity: root,
            placement: root_last(),
        },
        HierarchyOperation::Place {
            entity: child,
            placement: child_last(root),
        },
    ]);
    queue.apply(&mut world);

    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), [root]);
    assert_eq!(&world.get::<Children>(root).unwrap()[..], [child]);
    assert!(
        world
            .resource::<HierarchyGenerationState>()
            .needs_validation()
    );
}

#[test]
fn unrelated_invalid_deferred_write_retains_the_bare_world_error_behavior() {
    #[derive(Resource)]
    struct LaterCommandRan;

    let mut world = World::new();
    let missing = world.spawn_empty().id();
    assert!(world.despawn(missing));
    let mut queue = CommandQueue::default();
    Commands::new(&mut queue, &world)
        .apply_hierarchy_operation(HierarchyOperation::Place {
            entity: missing,
            placement: root_last(),
        })
        .insert_resource(LaterCommandRan);

    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| queue.apply(&mut world)));
    assert!(outcome.is_err());
    assert!(!world.contains_resource::<LaterCommandRan>());
}

#[test]
fn invalid_batch_rejects_before_any_topology_mutation() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    let missing = world.spawn_empty().id();
    assert!(world.despawn(missing));

    let error = HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: HierarchyPlacement::before(
                    HierarchyPlacementOwner::Parent { parent: root },
                    missing,
                ),
            },
        ])
        .unwrap_err();

    assert_eq!(
        error,
        HierarchyError::AnchorMissing {
            entity: child,
            anchor: missing,
        }
    );
    assert!(!world.contains_resource::<HierarchyRoots>());
    assert!(world.get::<ChildOf>(child).is_none());
    assert!(world.get::<Children>(root).is_none());
    assert!(!world.contains_resource::<HierarchyGenerationState>());
}

#[test]
fn placement_rejects_missing_self_and_unplaced_owners() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let owner = world.spawn_empty().id();
    let missing = world.spawn_empty().id();
    assert!(world.despawn(missing));

    assert_eq!(
        HierarchyWriter::new(&mut world)
            .place(entity, child_last(missing))
            .unwrap_err(),
        HierarchyError::MissingParent {
            child: entity,
            parent: missing,
        }
    );
    assert_eq!(
        HierarchyWriter::new(&mut world)
            .place(entity, child_last(entity))
            .unwrap_err(),
        HierarchyError::SelfParent { entity }
    );
    assert_eq!(
        HierarchyWriter::new(&mut world)
            .place(entity, child_last(owner))
            .unwrap_err(),
        HierarchyError::OwnerUnplaced {
            entity,
            parent: owner,
        }
    );
}

#[test]
fn placement_resolves_after_pending_entity_despawn() {
    let mut world = World::new();
    let existing = world.spawn_empty().id();
    place_root(&mut world, existing);
    let pending = world.spawn_empty().id();
    world.commands().entity(pending).despawn();

    assert_eq!(
        HierarchyWriter::new(&mut world).place(pending, root_last()),
        Err(HierarchyError::MissingEntity { entity: pending })
    );
    assert!(world.get_entity(pending).is_err());
    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), [existing]);
}

#[test]
fn later_batch_operations_can_anchor_to_earlier_candidate_changes() {
    let mut world = World::new();
    let original = world.spawn_empty().id();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    place_root(&mut world, original);

    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: first,
                placement: HierarchyPlacement::first(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: second,
                placement: HierarchyPlacement::after(HierarchyPlacementOwner::Root, first),
            },
        ])
        .unwrap();

    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        [first, second, original]
    );
}

#[test]
fn moves_preserve_exact_same_parent_cross_parent_and_root_order() {
    let mut world = World::new();
    let left = world.spawn_empty().id();
    let right = world.spawn_empty().id();
    let loose = world.spawn_empty().id();
    let a = world.spawn_empty().id();
    let b = world.spawn_empty().id();
    let c = world.spawn_empty().id();

    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: left,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: right,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: loose,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: a,
                placement: child_last(left),
            },
            HierarchyOperation::Place {
                entity: b,
                placement: child_last(left),
            },
            HierarchyOperation::Place {
                entity: c,
                placement: child_last(left),
            },
        ])
        .unwrap();

    let operations = [
        HierarchyOperation::Move {
            entity: c,
            from: HierarchyPlacement::current(&world, c).unwrap(),
            to: HierarchyPlacement::first(HierarchyPlacementOwner::Parent { parent: left }),
        },
        HierarchyOperation::Move {
            entity: b,
            from: HierarchyPlacement::after(HierarchyPlacementOwner::Parent { parent: left }, a),
            to: child_last(right),
        },
        HierarchyOperation::Move {
            entity: loose,
            from: HierarchyPlacement::current(&world, loose).unwrap(),
            to: HierarchyPlacement::before(HierarchyPlacementOwner::Root, right),
        },
    ];
    HierarchyWriter::new(&mut world)
        .apply_batch(&operations)
        .unwrap();

    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        [left, loose, right]
    );
    assert_eq!(&world.get::<Children>(left).unwrap()[..], [c, a]);
    assert_eq!(&world.get::<Children>(right).unwrap()[..], [b]);
    validate_hierarchy(&world).unwrap();
}

#[test]
fn stale_source_and_cycle_reject_the_complete_move_batch() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    let sibling = world.spawn_empty().id();
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: child_last(root),
            },
            HierarchyOperation::Place {
                entity: sibling,
                placement: root_last(),
            },
        ])
        .unwrap();
    let before_roots = world.resource::<HierarchyRoots>().as_slice().to_vec();

    let stale = HierarchyWriter::new(&mut world)
        .move_entity(
            sibling,
            HierarchyPlacement::first(HierarchyPlacementOwner::Root),
            child_last(root),
        )
        .unwrap_err();
    assert_eq!(
        stale,
        HierarchyError::PlacementContextMismatch { entity: sibling }
    );

    let cycle = HierarchyWriter::new(&mut world)
        .move_entity(
            root,
            HierarchyPlacement::first(HierarchyPlacementOwner::Root),
            child_last(child),
        )
        .unwrap_err();
    assert_eq!(cycle, HierarchyError::Cycle { entity: root });
    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), before_roots);
    assert_eq!(world.get::<ChildOf>(child).map(ChildOf::parent), Some(root));
}

#[test]
fn removal_requires_a_childless_exact_placement() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: child_last(root),
            },
        ])
        .unwrap();

    assert_eq!(
        HierarchyWriter::new(&mut world)
            .remove(
                root,
                HierarchyPlacement::first(HierarchyPlacementOwner::Root)
            )
            .unwrap_err(),
        HierarchyError::HasChildren { entity: root }
    );
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Remove {
                entity: child,
                from: HierarchyPlacement::first(HierarchyPlacementOwner::Parent { parent: root }),
            },
            HierarchyOperation::Remove {
                entity: root,
                from: HierarchyPlacement::first(HierarchyPlacementOwner::Root),
            },
        ])
        .unwrap();

    assert!(world.resource::<HierarchyRoots>().is_empty());
    assert!(world.get::<ChildOf>(child).is_none());
    assert!(world.get::<Children>(root).is_none());
    validate_hierarchy(&world).unwrap();
}

#[test]
fn candidate_keeps_empty_child_overrides_after_remove_and_replace() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let old_child = world.spawn_empty().id();
    let new_child = world.spawn_empty().id();
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: old_child,
                placement: child_last(root),
            },
        ])
        .unwrap();

    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Remove {
                entity: old_child,
                from: child_last(root),
            },
            HierarchyOperation::Remove {
                entity: root,
                from: root_last(),
            },
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: new_child,
                placement: child_last(root),
            },
        ])
        .unwrap();

    assert_eq!(&world.get::<Children>(root).unwrap()[..], [new_child]);
    validate_hierarchy(&world).unwrap();
}

#[test]
fn same_owner_anchor_positions_match_removed_entity_order() {
    for actor_index in 0..4 {
        for anchor_index in 0..4 {
            if actor_index == anchor_index {
                continue;
            }
            for insert_after in [false, true] {
                let mut world = World::new();
                let root = world.spawn_empty().id();
                let children = (0..4).map(|_| world.spawn_empty().id()).collect::<Vec<_>>();
                let mut operations = vec![HierarchyOperation::Place {
                    entity: root,
                    placement: root_last(),
                }];
                operations.extend(children.iter().map(|entity| HierarchyOperation::Place {
                    entity: *entity,
                    placement: child_last(root),
                }));
                HierarchyWriter::new(&mut world)
                    .apply_batch(&operations)
                    .unwrap();
                let owner = HierarchyPlacementOwner::Parent { parent: root };
                let actor = children[actor_index];
                let anchor = children[anchor_index];
                let from = HierarchyPlacement::current(&world, actor).unwrap();
                let target = if insert_after {
                    HierarchyPlacement::after(owner, anchor)
                } else {
                    HierarchyPlacement::before(owner, anchor)
                };
                let mut expected = children;
                expected.remove(actor_index);
                let insertion = expected
                    .iter()
                    .position(|entity| *entity == anchor)
                    .unwrap()
                    + usize::from(insert_after);
                expected.insert(insertion, actor);

                HierarchyWriter::new(&mut world)
                    .move_entity(actor, from, target)
                    .unwrap();

                assert_eq!(&world.get::<Children>(root).unwrap()[..], expected);
                validate_hierarchy(&world).unwrap();
            }
        }
    }
}

#[test]
fn candidate_cycle_uses_earlier_moves_without_publishing_them() {
    let mut world = World::new();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    place_root(&mut world, first);
    place_root(&mut world, second);
    validate_dirty_hierarchy(&mut world).unwrap();
    let completed = crate::hierarchy::completed_topology_generation(&world);

    let result = HierarchyWriter::new(&mut world).apply_batch(&[
        HierarchyOperation::Move {
            entity: first,
            from: HierarchyPlacement::first(HierarchyPlacementOwner::Root),
            to: child_last(second),
        },
        HierarchyOperation::Move {
            entity: second,
            from: root_last(),
            to: child_last(first),
        },
    ]);

    assert_eq!(result, Err(HierarchyError::Cycle { entity: second }));
    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        [first, second]
    );
    assert!(world.get::<ChildOf>(first).is_none());
    assert_eq!(
        crate::hierarchy::completed_topology_generation(&world),
        completed
    );
    validate_hierarchy(&world).unwrap();
}

#[test]
fn valid_baseline_cannot_bypass_new_lifecycle_observers() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    place_root(&mut world, root);
    world.add_observer(
        |_: nara_ecs::bevy_ecs::observer::On<nara_ecs::bevy_ecs::lifecycle::Insert<ChildOf>>| {},
    );

    let result = HierarchyWriter::new(&mut world).place(child, child_last(root));

    assert_eq!(result, Err(HierarchyError::LifecycleWork));
    assert!(world.get::<ChildOf>(child).is_none());
    assert!(world.get::<Children>(root).is_none());
    validate_hierarchy(&world).unwrap();
}

#[test]
fn deep_and_wide_ordered_batches_are_iterative() {
    let mut deep_world = World::new();
    let deep_entities = (0..DEEP_HIERARCHY_DEPTH)
        .map(|_| deep_world.spawn_empty().id())
        .collect::<Vec<_>>();
    let mut deep_operations = Vec::with_capacity(deep_entities.len());
    deep_operations.push(HierarchyOperation::Place {
        entity: deep_entities[0],
        placement: root_last(),
    });
    deep_operations.extend(
        (1..deep_entities.len()).map(|index| HierarchyOperation::Place {
            entity: deep_entities[index],
            placement: child_last(deep_entities[index - 1]),
        }),
    );
    HierarchyWriter::new(&mut deep_world)
        .apply_batch(&deep_operations)
        .unwrap();
    assert_eq!(
        deep_world.resource::<HierarchyRoots>().as_slice(),
        [deep_entities[0]]
    );
    validate_hierarchy(&deep_world).unwrap();

    let mut wide_world = World::new();
    let root = wide_world.spawn_empty().id();
    let children = (0..DEEP_HIERARCHY_DEPTH)
        .map(|_| wide_world.spawn_empty().id())
        .collect::<Vec<_>>();
    let mut wide_operations = Vec::with_capacity(children.len() + 1);
    wide_operations.push(HierarchyOperation::Place {
        entity: root,
        placement: root_last(),
    });
    wide_operations.extend(
        children
            .iter()
            .copied()
            .map(|entity| HierarchyOperation::Place {
                entity,
                placement: child_last(root),
            }),
    );
    HierarchyWriter::new(&mut wide_world)
        .apply_batch(&wide_operations)
        .unwrap();
    assert_eq!(&wide_world.get::<Children>(root).unwrap()[..], children);
    validate_hierarchy(&wide_world).unwrap();
}

#[test]
fn unchanged_generation_skips_full_validation_scans() {
    let mut world = World::new();
    world.init_resource::<HierarchyGenerationState>();
    world.init_resource::<RetainedHierarchyValidationScratch>();
    world.insert_resource(HierarchyRoots::new());

    validate_dirty_hierarchy(&mut world).unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();
    assert_eq!(
        world
            .resource::<HierarchyGenerationState>()
            .validation_scans,
        1
    );

    let root = world.spawn_empty().id();
    place_root(&mut world, root);
    validate_dirty_hierarchy(&mut world).unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();
    let state = world.resource::<HierarchyGenerationState>();
    assert_eq!(state.topology_generation, 1);
    assert_eq!(state.completed_generation, Some(1));
    assert_eq!(state.validation_scans, 2);
}

#[test]
fn dirty_generations_reuse_retained_validation_storage() {
    let mut world = World::new();
    world.init_resource::<HierarchyGenerationState>();
    world.init_resource::<RetainedHierarchyValidationScratch>();
    world.insert_resource(HierarchyRoots::new());
    let root = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: child_last(root),
            },
        ])
        .unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();

    let capacities = world
        .resource::<RetainedHierarchyValidationScratch>()
        .0
        .capacities();
    assert!(capacities.into_iter().all(|capacity| capacity > 0));
    mark_topology_dirty(&mut world);
    validate_dirty_hierarchy(&mut world).unwrap();
    assert_eq!(
        world
            .resource::<RetainedHierarchyValidationScratch>()
            .0
            .capacities(),
        capacities
    );
}

#[test]
fn native_linked_children_are_detached_before_exact_retirement() {
    let mut world = World::new();
    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: parent,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: child_last(parent),
            },
        ])
        .unwrap();
    HierarchyWriter::new(&mut world)
        .move_entity(
            child,
            HierarchyPlacement::first(HierarchyPlacementOwner::Parent { parent }),
            HierarchyPlacement::first(HierarchyPlacementOwner::Root),
        )
        .unwrap();
    HierarchyWriter::new(&mut world)
        .remove(
            parent,
            HierarchyPlacement::after(HierarchyPlacementOwner::Root, child),
        )
        .unwrap();
    assert!(world.despawn(parent));

    assert!(world.get_entity(child).is_ok());
    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), [child]);
    validate_hierarchy(&world).unwrap();

    let unmanaged_parent = world.spawn_empty().id();
    let unmanaged_child = world.spawn(ChildOf(unmanaged_parent)).id();
    assert_eq!(validate_hierarchy(&world), Ok(()));
    assert!(world.despawn(unmanaged_parent));
    assert!(world.get_entity(unmanaged_child).is_err());
}

#[test]
fn writer_rejects_placing_a_native_root_under_its_own_descendant() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let child = world.spawn(ChildOf(root)).id();
    assert_eq!(
        HierarchyWriter::new(&mut world).place(root, child_last(child)),
        Err(HierarchyError::Cycle { entity: root }),
    );
    assert!(world.get::<ChildOf>(root).is_none());
    assert_eq!(&world.get::<Children>(root).unwrap()[..], &[child]);
    assert_eq!(validate_hierarchy(&world), Ok(()));
}

#[test]
fn retirement_promotes_surviving_frontier_in_original_structural_order() {
    let mut world = World::new();
    let survivor_after = world.spawn_empty().id();
    let trailing_root = world.spawn_empty().id();
    let retiring_child_after = world.spawn_empty().id();
    let leading_root = world.spawn_empty().id();
    let nested_survivor = world.spawn_empty().id();
    let retiring_root = world.spawn_empty().id();
    let direct_survivor = world.spawn_empty().id();
    let retiring_child_before = world.spawn_empty().id();

    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: leading_root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: retiring_root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: trailing_root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: retiring_child_before,
                placement: child_last(retiring_root),
            },
            HierarchyOperation::Place {
                entity: nested_survivor,
                placement: child_last(retiring_child_before),
            },
            HierarchyOperation::Place {
                entity: direct_survivor,
                placement: child_last(retiring_root),
            },
            HierarchyOperation::Place {
                entity: retiring_child_after,
                placement: child_last(retiring_root),
            },
            HierarchyOperation::Place {
                entity: survivor_after,
                placement: child_last(retiring_child_after),
            },
        ])
        .unwrap();

    let operations = crate::hierarchy::retirement_operations(
        &world,
        &[retiring_child_after, retiring_root, retiring_child_before],
    )
    .unwrap();
    HierarchyWriter::new(&mut world)
        .apply_batch(&operations)
        .unwrap();

    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        [
            leading_root,
            trailing_root,
            nested_survivor,
            direct_survivor,
            survivor_after,
        ]
    );
    assert!(world.get::<ChildOf>(nested_survivor).is_none());
    assert!(world.get::<ChildOf>(direct_survivor).is_none());
    assert!(world.get::<ChildOf>(survivor_after).is_none());
    assert!(world.get::<Children>(retiring_root).is_none());
    assert!(world.get::<Children>(retiring_child_before).is_none());
    assert!(world.get::<Children>(retiring_child_after).is_none());
    validate_hierarchy(&world).unwrap();
}

#[test]
fn direct_forward_and_reverse_corruption_fails_closed() {
    let mut world = World::new();
    let first_root = world.spawn_empty().id();
    let second_root = world.spawn_empty().id();
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: first_root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: second_root,
                placement: root_last(),
            },
        ])
        .unwrap();
    world.entity_mut(first_root).insert(ChildOf(second_root));
    world.flush();
    assert_eq!(
        validate_hierarchy(&world),
        Err(HierarchyError::RootParented {
            entity: first_root,
            parent: second_root,
        })
    );

    let mut reverse_world = World::new();
    let root = reverse_world.spawn_empty().id();
    let child = reverse_world.spawn_empty().id();
    HierarchyWriter::new(&mut reverse_world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: child_last(root),
            },
        ])
        .unwrap();
    reverse_world.entity_mut(root).detach_all_children();
    reverse_world
        .entity_mut(child)
        .insert_with_relationship_hook_mode(ChildOf(root), RelationshipHookMode::Skip);
    assert_eq!(
        validate_hierarchy(&reverse_world),
        Err(HierarchyError::ReverseMissing {
            parent: root,
            child
        })
    );
}

#[test]
fn complete_validation_does_not_register_missing_hierarchy_state() {
    let world = World::new();
    let component_count = world.components().len();

    assert_eq!(validate_hierarchy(&world), Ok(()));
    assert_eq!(world.components().len(), component_count);
    assert!(world.component_id::<ChildOf>().is_none());
    assert!(world.component_id::<Children>().is_none());
    assert!(!world.contains_resource::<HierarchyRoots>());
}

#[test]
fn managed_mutation_preparation_rejects_missing_roots_without_blocking_empty_retirement() {
    let mut app = App::new();
    app.add_plugin(HierarchyPlugin).unwrap();
    let world = app.world_mut().unwrap();
    let entity = world.spawn_empty().id();
    let operation = HierarchyOperation::Place {
        entity,
        placement: root_last(),
    };
    crate::hierarchy::prepare_mutation_baseline(world);
    crate::hierarchy::validate_fresh_construction_topology(world, false).unwrap();
    world.remove_resource::<HierarchyRoots>().unwrap();

    assert_eq!(
        crate::hierarchy::validate_fresh_construction_topology(world, false),
        Err(HierarchyError::MissingRoots)
    );
    assert_eq!(
        crate::hierarchy::prepare_construction_batch(world, &[operation]).err(),
        Some(HierarchyError::MissingRoots)
    );
    assert_eq!(
        HierarchyWriter::new(world).apply(operation),
        Err(HierarchyError::MissingRoots)
    );

    let retirement = crate::hierarchy::retirement_operations(world, &[]).unwrap();
    assert!(retirement.is_empty());
    crate::hierarchy::prepare_construction_batch(world, &retirement)
        .unwrap()
        .commit(world);
    assert!(!world.contains_resource::<HierarchyRoots>());
    assert!(world.get::<relation::HierarchyMember>(entity).is_none());
}

#[test]
fn missing_registered_root_is_reported_exactly() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    place_root(&mut world, root);
    assert!(world.despawn(root));

    assert_eq!(
        validate_hierarchy(&world),
        Err(HierarchyError::RootEntityMissing { entity: root })
    );
}

#[derive(Resource)]
struct MoveBeforeCompletion {
    entity: Entity,
    from: HierarchyPlacement,
    to: HierarchyPlacement,
}

fn move_before_completion(world: &mut World) {
    let operation = world.remove_resource::<MoveBeforeCompletion>().unwrap();
    HierarchyWriter::new(world)
        .move_entity(operation.entity, operation.from, operation.to)
        .unwrap();
}

#[test]
fn plugin_completes_the_latest_ordered_generation() {
    let mut app = App::new();
    app.add_plugin(HierarchyPlugin).unwrap();
    let first = app.world_mut().unwrap().spawn_empty().id();
    let second = app.world_mut().unwrap().spawn_empty().id();
    {
        let world = app.world_mut().unwrap();
        HierarchyWriter::new(world)
            .apply_batch(&[
                HierarchyOperation::Place {
                    entity: first,
                    placement: root_last(),
                },
                HierarchyOperation::Place {
                    entity: second,
                    placement: root_last(),
                },
            ])
            .unwrap();
    }
    app.insert_resource(MoveBeforeCompletion {
        entity: second,
        from: HierarchyPlacement::after(HierarchyPlacementOwner::Root, first),
        to: HierarchyPlacement::first(HierarchyPlacementOwner::Root),
    })
    .unwrap();
    app.add_systems(
        CoreStage::PostUpdate,
        move_before_completion.before(HierarchySet::ValidateAndComplete),
    )
    .unwrap();

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    assert_eq!(
        runtime.world().resource::<HierarchyRoots>().as_slice(),
        [second, first]
    );
    assert_eq!(
        crate::hierarchy::completed_topology_generation(runtime.world()),
        Some(
            runtime
                .world()
                .resource::<HierarchyGenerationState>()
                .topology_generation
        )
    );
    runtime.finish().unwrap();
}

#[derive(Component)]
#[component(storage = "SparseSet")]
struct CustomDisabled;

#[derive(Default, Resource)]
struct HierarchyRenderRuns(usize);

fn app_with_hierarchy_edge() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(HierarchyPlugin).unwrap();
    app.init_resource::<HierarchyRenderRuns>().unwrap();
    app.add_systems(
        CoreStage::Render,
        |mut runs: ResMut<HierarchyRenderRuns>| {
            runs.0 += 1;
        },
    )
    .unwrap();
    let world = app.world_mut().unwrap();
    world.register_disabling_component::<CustomDisabled>();
    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    place_root(world, parent);
    HierarchyWriter::new(world)
        .place(child, child_last(parent))
        .unwrap();
    (app, parent, child)
}

#[test]
fn managed_and_public_validation_include_disabled_hierarchy_endpoints() {
    use std::time::Duration;

    use nara_app::{RealTime, RuntimeInstance};

    let (mut app, parent, child) = app_with_hierarchy_edge();
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        match world.resource::<RealTime>().frame {
            2 => {
                world.entity_mut(parent).insert(Disabled);
            }
            3 => {
                world.entity_mut(parent).remove::<Disabled>();
                world.entity_mut(child).insert(CustomDisabled);
            }
            4 => {
                world.entity_mut(parent).insert((Disabled, CustomDisabled));
            }
            _ => return,
        }
        let unrelated_root = world.spawn_empty().id();
        place_root(world, unrelated_root);
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let mut previous_publication = None;
    for (index, [parent_visible, child_visible]) in
        [[true, true], [false, true], [true, false], [false, false]]
            .into_iter()
            .enumerate()
    {
        runtime.drive(Duration::ZERO).unwrap();
        let world = runtime.world();
        assert_eq!(world.resource::<HierarchyRenderRuns>().0, index + 1);
        assert_eq!(validate_hierarchy(world), Ok(()));
        let order = world.resource::<HierarchyOrder>();
        assert_eq!(order.len(), index + 2);
        assert_eq!(order.as_slice()[..2], [parent, child]);
        let publication = crate::hierarchy::completed_topology_publication(world);
        assert!(publication.is_some());
        assert_ne!(publication, previous_publication);
        previous_publication = publication;

        let mut query = QueryState::<Entity>::try_new(world).unwrap();
        assert_eq!(query.get(world, parent).is_ok(), parent_visible);
        assert_eq!(query.get(world, child).is_ok(), child_visible);
    }
    runtime.finish().unwrap();
}

#[test]
fn managed_and_public_validation_reject_broken_disabled_edges() {
    use std::time::Duration;

    use nara_app::{RealTime, RuntimeFaultKind, RuntimeInstance, RuntimeState};

    for remove_parent in [true, false] {
        let (mut app, parent, child) = app_with_hierarchy_edge();
        app.add_systems(CoreStage::Update, move |world: &mut World| {
            if world.resource::<RealTime>().frame == 2 {
                world.entity_mut(parent).insert(Disabled);
                world.entity_mut(child).insert(CustomDisabled);
                if remove_parent {
                    let other_parent = world.spawn_empty().id();
                    world.entity_mut(child).insert_with_relationship_hook_mode(
                        ChildOf(other_parent),
                        RelationshipHookMode::Skip,
                    );
                } else {
                    world.entity_mut(parent).detach_all_children();
                    world.entity_mut(child).insert_with_relationship_hook_mode(
                        ChildOf(parent),
                        RelationshipHookMode::Skip,
                    );
                }
            }
        })
        .unwrap();

        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert!(crate::hierarchy::completed_topology_publication(runtime.world()).is_some());
        assert_eq!(runtime.world().resource::<HierarchyRenderRuns>().0, 1);

        let error = runtime.drive(Duration::ZERO).unwrap_err();
        assert_eq!(error.fault().kind(), RuntimeFaultKind::System);
        assert_eq!(error.fault().source(), "nara.ecs.fallible-execution");
        assert_eq!(runtime.state(), RuntimeState::Faulted);
        let world = runtime.world();
        assert_eq!(world.resource::<HierarchyRenderRuns>().0, 1);
        assert!(crate::hierarchy::completed_topology_publication(world).is_none());
        let expected = if remove_parent {
            HierarchyError::ReverseUnexpected {
                parent,
                child,
                actual_parent: world.get::<ChildOf>(child).map(ChildOf::parent),
            }
        } else {
            HierarchyError::ReverseMissing { parent, child }
        };
        assert_eq!(validate_hierarchy(world), Err(expected));
        runtime.finish().unwrap();
    }
}

#[test]
fn public_validation_rejects_disabled_member_without_placement() {
    let mut world = World::new();
    world.register_disabling_component::<CustomDisabled>();
    let entity = world
        .spawn((relation::HierarchyMember, CustomDisabled))
        .id();

    assert_eq!(
        validate_hierarchy(&world),
        Err(HierarchyError::PlacementMissing { entity })
    );
}

#[test]
fn removing_empty_hierarchy_roots_faults_runtime_drive_and_allows_stop() {
    use std::time::Duration;

    use nara_app::{
        RealTime, RuntimeControl, RuntimeControlRequestResult, RuntimeControlStatus,
        RuntimeFaultKind, RuntimeInstance, RuntimeState,
    };

    let mut app = App::new();
    app.add_plugin(HierarchyPlugin).unwrap();
    app.add_systems(
        CoreStage::PostUpdate,
        (|world: &mut World| {
            if world.resource::<RealTime>().frame == 2 {
                world.remove_resource::<HierarchyRoots>().unwrap();
            }
        })
        .before(HierarchySet::ValidateAndComplete),
    )
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<HierarchyRoots>().is_empty());
    assert!(crate::hierarchy::completed_topology_publication(runtime.world()).is_some());

    let error = runtime.drive(Duration::ZERO).unwrap_err();
    let fault = error.fault();
    assert_eq!(fault.kind(), RuntimeFaultKind::System);
    assert_eq!(fault.source(), "nara.ecs.fallible-execution");
    let detail = fault.detail().unwrap();
    assert_eq!(detail.code(), "nara.hierarchy.missing-roots");
    assert_eq!(detail.origin(), "nara.hierarchy");
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(runtime.fault(), Some(fault));
    assert!(!runtime.world().contains_resource::<HierarchyRoots>());
    assert!(
        !runtime
            .world()
            .contains_resource::<crate::hierarchy::CompletedHierarchyProjection>()
    );
    assert_eq!(
        crate::hierarchy::completed_topology_publication(runtime.world()),
        None
    );
    assert_eq!(runtime.drive(Duration::ZERO).unwrap_err().fault(), fault);

    let RuntimeControlRequestResult::Accepted(stop) = runtime.request_control(RuntimeControl::Stop)
    else {
        panic!("faulted runtime should accept Stop");
    };
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(
        runtime.control_status(stop),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(runtime.fault(), Some(fault));
    runtime.finish().unwrap();
}

#[test]
fn deferred_placement_cannot_recreate_managed_roots_before_completion() {
    use std::time::Duration;

    use nara_app::{RealTime, RuntimeFaultKind, RuntimeInstance};

    let mut app = App::new();
    app.add_plugin(HierarchyPlugin).unwrap();
    let entity = app.world_mut().unwrap().spawn_empty().id();
    app.add_systems(
        CoreStage::PostUpdate,
        (move |world: &mut World| {
            if world.resource::<RealTime>().frame == 2 {
                world.remove_resource::<HierarchyRoots>().unwrap();
                world
                    .commands()
                    .apply_hierarchy_operation(HierarchyOperation::Place {
                        entity,
                        placement: root_last(),
                    });
                world.flush();
            }
        })
        .before(HierarchySet::ValidateAndComplete),
    )
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let error = runtime.drive(Duration::ZERO).unwrap_err();
    assert_eq!(error.fault().kind(), RuntimeFaultKind::Command);
    assert_eq!(
        error.fault().detail().unwrap().code(),
        "nara.hierarchy.missing-roots"
    );
    assert!(!runtime.world().contains_resource::<HierarchyRoots>());
    assert!(HierarchyPlacement::current(runtime.world(), entity).is_none());
    assert!(crate::hierarchy::completed_topology_publication(runtime.world()).is_none());
    runtime.finish().unwrap();
}

#[test]
fn completed_order_is_depth_first_and_allocation_independent() {
    let mut app = App::new();
    app.add_plugin(HierarchyPlugin).unwrap();
    let trailing_child = app.world_mut().unwrap().spawn_empty().id();
    let unrelated = app.world_mut().unwrap().spawn_empty().id();
    let first_root = app.world_mut().unwrap().spawn_empty().id();
    let leading_child = app.world_mut().unwrap().spawn_empty().id();
    let nested_child = app.world_mut().unwrap().spawn_empty().id();
    let second_root = app.world_mut().unwrap().spawn_empty().id();
    {
        let world = app.world_mut().unwrap();
        HierarchyWriter::new(world)
            .apply_batch(&[
                HierarchyOperation::Place {
                    entity: first_root,
                    placement: root_last(),
                },
                HierarchyOperation::Place {
                    entity: second_root,
                    placement: root_last(),
                },
                HierarchyOperation::Place {
                    entity: leading_child,
                    placement: child_last(first_root),
                },
                HierarchyOperation::Place {
                    entity: trailing_child,
                    placement: child_last(first_root),
                },
                HierarchyOperation::Place {
                    entity: nested_child,
                    placement: child_last(leading_child),
                },
            ])
            .unwrap();
    }

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let order = runtime.world().resource::<HierarchyOrder>();
    assert_eq!(order.roots(), [first_root, second_root]);
    assert_eq!(
        order.as_slice(),
        [
            first_root,
            leading_child,
            nested_child,
            trailing_child,
            second_root,
        ]
    );
    assert_eq!(order.index(trailing_child), Some(3));
    assert_eq!(order.index(unrelated), None);
    runtime.finish().unwrap();
}

#[test]
fn advanced_component_or_root_removal_invalidates_completion_immediately() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: child_last(root),
            },
        ])
        .unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();
    assert!(crate::hierarchy::completed_topology_generation(&world).is_some());

    world.entity_mut(child).remove::<ChildOf>();
    assert_eq!(
        crate::hierarchy::completed_topology_generation(&world),
        None
    );
    validate_dirty_hierarchy(&mut world).unwrap();
    assert_eq!(world.resource::<HierarchyOrder>().as_slice(), &[root]);

    let mut roots_world = World::new();
    let root = roots_world.spawn_empty().id();
    place_root(&mut roots_world, root);
    validate_dirty_hierarchy(&mut roots_world).unwrap();
    assert!(crate::hierarchy::completed_topology_generation(&roots_world).is_some());
    roots_world.remove_resource::<HierarchyRoots>();
    assert_eq!(
        crate::hierarchy::completed_topology_generation(&roots_world),
        None
    );
}

#[test]
fn replacing_root_resource_cannot_reuse_completed_generation() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    place_root(&mut world, root);
    validate_dirty_hierarchy(&mut world).unwrap();
    assert!(crate::hierarchy::completed_topology_generation(&world).is_some());

    let _removed = world.remove_resource::<HierarchyRoots>().unwrap();
    world.insert_resource(HierarchyRoots::new());

    assert_eq!(
        crate::hierarchy::completed_topology_generation(&world),
        None
    );
    assert!(validate_dirty_hierarchy(&mut world).is_err());
}

#[test]
fn writer_revalidates_after_raw_relation_mutation() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    let replacement = world.spawn_empty().id();
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: child_last(root),
            },
        ])
        .unwrap();

    world.entity_mut(root).insert(ChildOf(child));

    assert!(
        HierarchyWriter::new(&mut world)
            .place(replacement, root_last())
            .is_err()
    );
    assert_eq!(world.get::<ChildOf>(child).map(ChildOf::parent), Some(root));
    assert_eq!(&world.get::<Children>(root).unwrap()[..], [child]);
}

#[test]
fn root_resource_modified_in_another_world_cannot_restore_an_old_baseline() {
    let mut original = World::new();
    let mut foreign = World::new();
    let original_entities = (0..3)
        .map(|_| original.spawn_empty().id())
        .collect::<Vec<_>>();
    let foreign_entities = (0..3)
        .map(|_| foreign.spawn_empty().id())
        .collect::<Vec<_>>();
    assert_eq!(original_entities, foreign_entities);
    place_root(&mut original, original_entities[0]);
    place_root(&mut foreign, foreign_entities[0]);

    let original_roots = original.remove_resource::<HierarchyRoots>().unwrap();
    foreign.insert_resource(original_roots);
    place_root(&mut foreign, foreign_entities[1]);
    let modified_roots = foreign.remove_resource::<HierarchyRoots>().unwrap();
    original.insert_resource(modified_roots);

    assert_eq!(
        HierarchyWriter::new(&mut original).place(original_entities[2], root_last()),
        Err(HierarchyError::MembershipMissing {
            entity: original_entities[1],
        })
    );
    assert!(
        !original
            .resource::<HierarchyRoots>()
            .contains(original_entities[2])
    );
}

#[test]
fn foreign_completed_order_cannot_replace_the_worlds_structural_order() {
    let mut original = World::new();
    let mut foreign = World::new();
    let original_entities = (0..3)
        .map(|_| original.spawn_empty().id())
        .collect::<Vec<_>>();
    let foreign_entities = (0..3)
        .map(|_| foreign.spawn_empty().id())
        .collect::<Vec<_>>();
    assert_eq!(original_entities, foreign_entities);
    for (world, entities, reverse) in [
        (&mut original, &original_entities, false),
        (&mut foreign, &foreign_entities, true),
    ] {
        let ordered_children = if reverse {
            [entities[2], entities[1]]
        } else {
            [entities[1], entities[2]]
        };
        let mut operations = vec![HierarchyOperation::Place {
            entity: entities[0],
            placement: root_last(),
        }];
        operations.extend(ordered_children.map(|entity| HierarchyOperation::Place {
            entity,
            placement: child_last(entities[0]),
        }));
        HierarchyWriter::new(world)
            .apply_batch(&operations)
            .unwrap();
        validate_dirty_hierarchy(world).unwrap();
    }
    let foreign_order = foreign.remove_resource::<HierarchyOrder>().unwrap();
    original.insert_resource(foreign_order);

    assert_eq!(
        crate::hierarchy::completed_topology_generation(&original),
        None
    );
    validate_dirty_hierarchy(&mut original).unwrap();
    assert_eq!(
        original.resource::<HierarchyOrder>().as_slice(),
        original_entities
    );
    assert!(crate::hierarchy::completed_topology_generation(&original).is_some());
}

#[test]
fn shared_root_epoch_and_generation_cannot_admit_a_foreign_sibling_order() {
    let mut original = World::new();
    let mut foreign = World::new();
    let original_entities = (0..3)
        .map(|_| original.spawn_empty().id())
        .collect::<Vec<_>>();
    let foreign_entities = (0..3)
        .map(|_| foreign.spawn_empty().id())
        .collect::<Vec<_>>();
    assert_eq!(original_entities, foreign_entities);
    for (world, entities, children) in [
        (&mut original, &original_entities, [1, 2]),
        (&mut foreign, &foreign_entities, [2, 1]),
    ] {
        HierarchyWriter::new(world)
            .apply_batch(&[
                HierarchyOperation::Place {
                    entity: entities[0],
                    placement: root_last(),
                },
                HierarchyOperation::Place {
                    entity: entities[children[0]],
                    placement: child_last(entities[0]),
                },
                HierarchyOperation::Place {
                    entity: entities[children[1]],
                    placement: child_last(entities[0]),
                },
            ])
            .unwrap();
    }
    // Recreate an order through safe resource removal so the original and transferred-root
    // Worlds reach the same generation without modifying their private completion state.
    validate_dirty_hierarchy(&mut original).unwrap();
    let initial_order = original.remove_resource::<HierarchyOrder>().unwrap();
    validate_dirty_hierarchy(&mut original).unwrap();
    let generation = crate::hierarchy::completed_topology_generation(&original).unwrap();
    let original_publication = crate::hierarchy::completed_topology_publication(&original).unwrap();
    let original_root_epoch = original.resource::<HierarchyOrder>().root_epoch();
    let original_order = original.resource::<HierarchyOrder>().as_slice().to_vec();
    assert_eq!(initial_order.generation() + 1, generation);
    let roots = original.remove_resource::<HierarchyRoots>().unwrap();
    foreign.insert_resource(roots);
    validate_dirty_hierarchy(&mut foreign).unwrap();
    let foreign_order = foreign.remove_resource::<HierarchyOrder>().unwrap();
    assert_eq!(foreign_order.generation(), generation);
    assert_eq!(foreign_order.root_epoch(), original_root_epoch);
    assert_eq!(
        foreign_order.roots(),
        original.resource::<HierarchyOrder>().roots()
    );
    assert_ne!(foreign_order.as_slice(), original_order);
    assert_eq!(
        foreign_order.as_slice(),
        &[
            original_entities[0],
            original_entities[2],
            original_entities[1]
        ]
    );
    original.insert_resource(foreign.remove_resource::<HierarchyRoots>().unwrap());
    original.insert_resource(foreign_order);
    assert_eq!(
        original.resource::<HierarchyOrder>().generation(),
        generation
    );
    assert_eq!(
        original.resource::<HierarchyOrder>().root_epoch(),
        original_root_epoch
    );
    assert_eq!(
        crate::hierarchy::completed_topology_publication(&original),
        None
    );
    assert_eq!(
        crate::hierarchy::completed_topology_generation(&original),
        None
    );
    validate_dirty_hierarchy(&mut original).unwrap();
    assert_eq!(
        original.resource::<HierarchyOrder>().as_slice(),
        original_entities
    );
    assert_ne!(
        crate::hierarchy::completed_topology_publication(&original),
        Some(original_publication)
    );
    assert!(crate::hierarchy::completed_topology_generation(&original).is_some());
    assert_eq!(
        original
            .resource::<HierarchyOrder>()
            .index(original_entities[1]),
        Some(1)
    );
    assert_eq!(
        original
            .resource::<HierarchyOrder>()
            .index(original_entities[2]),
        Some(2)
    );
}

#[test]
fn prior_same_world_order_is_not_the_current_publication_even_with_identical_roots() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: root_last(),
            },
            HierarchyOperation::Place {
                entity: first,
                placement: child_last(root),
            },
            HierarchyOperation::Place {
                entity: second,
                placement: child_last(root),
            },
        ])
        .unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();
    let old_order = world.remove_resource::<HierarchyOrder>().unwrap();
    // Republishing unchanged topology still creates a distinct completion/order association.
    validate_dirty_hierarchy(&mut world).unwrap();
    let new_publication = crate::hierarchy::completed_topology_publication(&world).unwrap();
    world.insert_resource(old_order);
    assert_eq!(
        crate::hierarchy::completed_topology_publication(&world),
        None
    );
    validate_dirty_hierarchy(&mut world).unwrap();
    assert_ne!(
        crate::hierarchy::completed_topology_publication(&world),
        Some(new_publication)
    );
    assert_eq!(
        world.resource::<HierarchyOrder>().as_slice(),
        &[root, first, second]
    );
}

#[test]
fn despawning_childless_root_invalidates_completion_immediately() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    place_root(&mut world, root);
    validate_dirty_hierarchy(&mut world).unwrap();
    assert!(crate::hierarchy::completed_topology_generation(&world).is_some());

    assert!(world.despawn(root));

    assert_eq!(
        crate::hierarchy::completed_topology_generation(&world),
        None
    );
    assert!(validate_dirty_hierarchy(&mut world).is_err());
    assert_eq!(
        crate::hierarchy::completed_topology_generation(&world),
        None
    );
    assert_eq!(
        validate_hierarchy(&world),
        Err(HierarchyError::RootEntityMissing { entity: root })
    );
}

#[test]
fn removing_private_membership_cannot_republish_completion() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    place_root(&mut world, root);
    validate_dirty_hierarchy(&mut world).unwrap();

    world.entity_mut(root).remove::<relation::HierarchyMember>();
    assert_eq!(
        crate::hierarchy::completed_topology_generation(&world),
        None
    );
    assert!(validate_dirty_hierarchy(&mut world).is_err());
    assert_eq!(
        crate::hierarchy::completed_topology_generation(&world),
        None
    );
    assert_eq!(
        validate_hierarchy(&world),
        Err(HierarchyError::MembershipMissing { entity: root })
    );
}

#[test]
fn advanced_missing_parent_remains_visible_to_validation() {
    let mut world = World::new();
    let child = world.spawn_empty().id();
    let missing = world.spawn_empty().id();
    assert!(world.despawn(missing));
    world
        .entity_mut(child)
        .insert_with_relationship_hook_mode(ChildOf(missing), RelationshipHookMode::Skip);

    assert_eq!(
        validate_hierarchy(&world),
        Err(HierarchyError::MissingParent {
            child,
            parent: missing,
        })
    );
}
