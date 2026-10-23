use nara_ecs::{
    Commands, Entity, Resource, World, bevy_ecs::world::CommandQueue, schedule::IntoScheduleConfigs,
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
        world.get::<Children>(first_root).unwrap().as_slice(),
        [first_child, second_child]
    );
    assert_eq!(
        world.get::<Parent>(first_child).map(Parent::parent),
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
    assert_eq!(world.get::<Children>(root).unwrap().as_slice(), [child]);
    assert!(
        world
            .resource::<HierarchyGenerationState>()
            .needs_validation()
    );
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
    assert!(world.get::<Parent>(child).is_none());
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
    assert_eq!(world.get::<Children>(left).unwrap().as_slice(), [c, a]);
    assert_eq!(world.get::<Children>(right).unwrap().as_slice(), [b]);
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
    assert_eq!(world.get::<Parent>(child).map(Parent::parent), Some(root));
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
    assert!(world.get::<Parent>(child).is_none());
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

    assert_eq!(world.get::<Children>(root).unwrap().as_slice(), [new_child]);
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

                assert_eq!(world.get::<Children>(root).unwrap().as_slice(), expected);
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
    let completed = __private::completed_topology_generation(&world);

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
    assert!(world.get::<Parent>(first).is_none());
    assert_eq!(__private::completed_topology_generation(&world), completed);
    validate_hierarchy(&world).unwrap();
}

#[test]
fn valid_baseline_cannot_bypass_new_lifecycle_observers() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    place_root(&mut world, root);
    world.add_observer(
        |_: nara_ecs::bevy_ecs::observer::On<nara_ecs::bevy_ecs::lifecycle::Insert<Parent>>| {},
    );

    let result = HierarchyWriter::new(&mut world).place(child, child_last(root));

    assert_eq!(result, Err(HierarchyError::LifecycleWork));
    assert!(world.get::<Parent>(child).is_none());
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
    assert_eq!(
        wide_world.get::<Children>(root).unwrap().as_slice(),
        children
    );
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
fn relation_is_non_linked_and_writer_can_detach_before_parent_retirement() {
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

    let operations = __private::retirement_operations(
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
    assert!(world.get::<Parent>(nested_survivor).is_none());
    assert!(world.get::<Parent>(direct_survivor).is_none());
    assert!(world.get::<Parent>(survivor_after).is_none());
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
    let mut second_children = Children::new();
    second_children.insert(0, first_root);
    world.entity_mut(second_root).insert(second_children);
    world
        .entity_mut(first_root)
        .insert(Parent::new(second_root));
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
    reverse_world.entity_mut(root).insert(Children::new());
    assert_eq!(
        validate_hierarchy(&reverse_world),
        Err(HierarchyError::ReverseEmpty { parent: root })
    );
}

#[test]
fn complete_validation_does_not_register_missing_hierarchy_state() {
    let world = World::new();
    let component_count = world.components().len();

    assert_eq!(validate_hierarchy(&world), Ok(()));
    assert_eq!(world.components().len(), component_count);
    assert!(world.component_id::<Parent>().is_none());
    assert!(world.component_id::<Children>().is_none());
    assert!(!world.contains_resource::<HierarchyRoots>());
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
        __private::completed_topology_generation(runtime.world()),
        Some(
            runtime
                .world()
                .resource::<HierarchyGenerationState>()
                .topology_generation
        )
    );
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
    assert!(__private::completed_topology_generation(&world).is_some());

    world.entity_mut(child).remove::<Parent>();
    assert_eq!(__private::completed_topology_generation(&world), None);

    let mut roots_world = World::new();
    let root = roots_world.spawn_empty().id();
    place_root(&mut roots_world, root);
    validate_dirty_hierarchy(&mut roots_world).unwrap();
    assert!(__private::completed_topology_generation(&roots_world).is_some());
    roots_world.remove_resource::<HierarchyRoots>();
    assert_eq!(__private::completed_topology_generation(&roots_world), None);
}

#[test]
fn replacing_root_resource_cannot_reuse_completed_generation() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    place_root(&mut world, root);
    validate_dirty_hierarchy(&mut world).unwrap();
    assert!(__private::completed_topology_generation(&world).is_some());

    let _removed = world.remove_resource::<HierarchyRoots>().unwrap();
    world.insert_resource(HierarchyRoots::new());

    assert_eq!(__private::completed_topology_generation(&world), None);
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

    world.entity_mut(child).remove::<Parent>();

    assert!(
        HierarchyWriter::new(&mut world)
            .place(replacement, root_last())
            .is_err()
    );
    assert!(world.get::<Parent>(child).is_none());
    assert_eq!(world.get::<Children>(root).unwrap().as_slice(), [child]);
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

    assert_eq!(__private::completed_topology_generation(&original), None);
    validate_dirty_hierarchy(&mut original).unwrap();
    assert_eq!(
        original.resource::<HierarchyOrder>().as_slice(),
        original_entities
    );
    assert!(__private::completed_topology_generation(&original).is_some());
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
    let generation = __private::completed_topology_generation(&original).unwrap();
    let original_publication = __private::completed_topology_publication(&original).unwrap();
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
    assert_eq!(__private::completed_topology_publication(&original), None);
    assert_eq!(__private::completed_topology_generation(&original), None);
    validate_dirty_hierarchy(&mut original).unwrap();
    assert_eq!(
        original.resource::<HierarchyOrder>().as_slice(),
        original_entities
    );
    assert_ne!(
        __private::completed_topology_publication(&original),
        Some(original_publication)
    );
    assert!(__private::completed_topology_generation(&original).is_some());
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
    let new_publication = __private::completed_topology_publication(&world).unwrap();
    world.insert_resource(old_order);
    assert_eq!(__private::completed_topology_publication(&world), None);
    validate_dirty_hierarchy(&mut world).unwrap();
    assert_ne!(
        __private::completed_topology_publication(&world),
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
    assert!(__private::completed_topology_generation(&world).is_some());

    assert!(world.despawn(root));

    assert_eq!(__private::completed_topology_generation(&world), None);
    assert!(validate_dirty_hierarchy(&mut world).is_err());
    assert_eq!(__private::completed_topology_generation(&world), None);
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
    assert_eq!(__private::completed_topology_generation(&world), None);
    assert!(validate_dirty_hierarchy(&mut world).is_err());
    assert_eq!(__private::completed_topology_generation(&world), None);
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
    world.entity_mut(child).insert(Parent::new(missing));

    assert_eq!(
        validate_hierarchy(&world),
        Err(HierarchyError::MissingParent {
            child,
            parent: missing,
        })
    );
}
