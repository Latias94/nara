use super::*;
use crate::{__private, HierarchyOrder, validate_dirty_hierarchy};

#[test]
fn parallel_root_replacements_preserve_unmanaged_siblings_and_exact_anchors() {
    const ROOT_COUNT: usize = 4_096;
    let mut world = World::new();
    let [head, middle, tail, unmanaged_child, unplaced] =
        std::array::from_fn(|_| world.spawn_empty().id());
    let root_owner = HierarchyPlacementOwner::Root;
    let mut setup = [head, middle, tail]
        .map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(root_owner),
        })
        .to_vec();
    setup.push(HierarchyOperation::Place {
        entity: unmanaged_child,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: head }),
    });
    HierarchyWriter::new(&mut world)
        .apply_batch(&setup)
        .unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();

    let mut old = Vec::new();
    let mut retired = Vec::new();
    for epoch in 0..4 {
        let mut next = (0..ROOT_COUNT)
            .map(|_| world.spawn_empty().id())
            .collect::<Vec<_>>();
        if epoch % 2 != 0 {
            next.reverse();
        }
        let retirement_input = old.iter().rev().copied().collect::<Vec<_>>();
        let mut operations = __private::retirement_operations(&world, &retirement_input).unwrap();
        for (index, entity) in next.iter().copied().enumerate() {
            let previous = match index {
                0 => head,
                index if index == ROOT_COUNT / 2 => middle,
                index => next[index - 1],
            };
            operations.push(HierarchyOperation::Place {
                entity,
                placement: HierarchyPlacement::after(root_owner, previous),
            });
        }

        let before = world.resource::<HierarchyRoots>().as_slice().to_vec();
        let epoch_before = world.resource::<HierarchyRoots>().epoch();
        let generation_before = world
            .resource::<HierarchyGenerationState>()
            .topology_generation;
        let prepared = __private::prepare_construction_batch(&world, &operations).unwrap();
        __private::validate_retirement_despawn(&world, &old, &old).unwrap();
        assert_eq!(world.resource::<HierarchyRoots>().as_slice(), before);
        assert_eq!(world.resource::<HierarchyRoots>().epoch(), epoch_before);
        prepared.commit(&mut world);

        let mut expected = vec![head];
        expected.extend_from_slice(&next[..ROOT_COUNT / 2]);
        expected.push(middle);
        expected.extend_from_slice(&next[ROOT_COUNT / 2..]);
        expected.push(tail);
        assert_eq!(world.resource::<HierarchyRoots>().as_slice(), expected);
        assert_ne!(world.resource::<HierarchyRoots>().epoch(), epoch_before);
        assert!(
            world
                .resource::<HierarchyGenerationState>()
                .topology_generation
                > generation_before
        );
        assert!(__private::completed_topology_generation(&world).is_none());
        assert!(hierarchy_baseline_is_valid(&world));
        for entity in old.iter().copied() {
            assert!(!world.resource::<HierarchyRoots>().contains(entity));
            assert!(world.get::<HierarchyMember>(entity).is_none());
            assert!(world.get::<Parent>(entity).is_none());
            assert!(world.get::<Children>(entity).is_none());
            assert!(world.despawn(entity));
            retired.push(entity);
        }
        for entity in retired.iter().copied() {
            assert!(!world.resource::<HierarchyRoots>().contains(entity));
        }
        for entity in next.iter().copied() {
            assert!(world.resource::<HierarchyRoots>().contains(entity));
            assert!(world.get::<HierarchyMember>(entity).is_some());
            assert!(world.get::<Parent>(entity).is_none());
        }
        assert_eq!(
            world.get::<Children>(head).unwrap().as_slice(),
            [unmanaged_child]
        );
        assert_eq!(
            world.get::<Parent>(unmanaged_child).map(Parent::parent),
            Some(head)
        );
        assert!(world.get::<HierarchyMember>(unplaced).is_none());
        validate_dirty_hierarchy(&mut world).unwrap();
        assert_eq!(world.resource::<HierarchyOrder>().roots(), expected);
        old = next;
    }

    let operations = __private::retirement_operations(&world, &old).unwrap();
    __private::validate_retirement_despawn(&world, &old, &old).unwrap();
    __private::prepare_construction_batch(&world, &operations)
        .unwrap()
        .commit(&mut world);
    for entity in old {
        assert!(world.despawn(entity));
        assert!(!world.resource::<HierarchyRoots>().contains(entity));
    }
    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        [head, middle, tail]
    );
    assert_eq!(
        world.get::<Children>(head).unwrap().as_slice(),
        [unmanaged_child]
    );
    validate_hierarchy(&world).unwrap();
}

#[test]
fn invalid_tail_of_large_replacement_preserves_the_entire_completed_topology() {
    const ROOT_COUNT: usize = 2_048;
    let mut world = World::new();
    let head = world.spawn_empty().id();
    let root_owner = HierarchyPlacementOwner::Root;
    let mut setup = vec![HierarchyOperation::Place {
        entity: head,
        placement: HierarchyPlacement::last(root_owner),
    }];
    let mut old = Vec::new();
    for _ in 0..ROOT_COUNT {
        let [root, first, last] = std::array::from_fn(|_| world.spawn_empty().id());
        setup.push(HierarchyOperation::Place {
            entity: root,
            placement: HierarchyPlacement::last(root_owner),
        });
        setup.extend([first, last].map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: root }),
        }));
        old.extend([root, first, last]);
    }
    HierarchyWriter::new(&mut world)
        .apply_batch(&setup)
        .unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();
    let roots_before = world.resource::<HierarchyRoots>().as_slice().to_vec();
    let order_before = world.resource::<HierarchyOrder>().as_slice().to_vec();
    let epoch_before = world.resource::<HierarchyRoots>().epoch();
    let generation_before = world
        .resource::<HierarchyGenerationState>()
        .topology_generation;
    let publication_before = __private::completed_topology_publication(&world);
    let topology_before = old
        .iter()
        .copied()
        .chain([head])
        .map(|entity| {
            (
                entity,
                world.get::<Parent>(entity).map(Parent::parent),
                world
                    .get::<Children>(entity)
                    .map(|children| children.as_slice().to_vec()),
                world.get::<HierarchyMember>(entity).is_some(),
            )
        })
        .collect::<Vec<_>>();

    let next = (0..ROOT_COUNT)
        .map(|_| world.spawn_empty().id())
        .collect::<Vec<_>>();
    let missing_anchor = world.spawn_empty().id();
    let mut operations = __private::retirement_operations(&world, &old).unwrap();
    let mut previous = head;
    for entity in next.iter().copied() {
        operations.push(HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::after(root_owner, previous),
        });
        previous = entity;
    }
    operations.push(HierarchyOperation::Move {
        entity: next[0],
        from: HierarchyPlacement::after(root_owner, head),
        to: HierarchyPlacement::before(root_owner, missing_anchor),
    });

    let result = HierarchyWriter::new(&mut world).apply_batch(&operations);

    assert_eq!(
        result,
        Err(HierarchyError::AnchorMissing {
            entity: next[0],
            anchor: missing_anchor,
        })
    );
    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), roots_before);
    assert!(
        roots_before
            .iter()
            .all(|entity| world.resource::<HierarchyRoots>().contains(*entity))
    );
    assert_eq!(world.resource::<HierarchyOrder>().as_slice(), order_before);
    assert_eq!(world.resource::<HierarchyRoots>().epoch(), epoch_before);
    assert_eq!(
        world
            .resource::<HierarchyGenerationState>()
            .topology_generation,
        generation_before
    );
    assert_eq!(
        __private::completed_topology_publication(&world),
        publication_before
    );
    assert!(hierarchy_baseline_is_valid(&world));
    for (entity, parent, children, member) in topology_before {
        assert_eq!(world.get::<Parent>(entity).map(Parent::parent), parent);
        assert_eq!(
            world.get::<Children>(entity).map(Children::as_slice),
            children.as_deref()
        );
        assert_eq!(world.get::<HierarchyMember>(entity).is_some(), member);
    }
    for entity in next.into_iter().chain([missing_anchor]) {
        assert!(world.get::<HierarchyMember>(entity).is_none());
        assert!(world.get::<Parent>(entity).is_none());
        assert!(world.get::<Children>(entity).is_none());
        assert!(!world.resource::<HierarchyRoots>().contains(entity));
    }
    validate_hierarchy(&world).unwrap();
}

#[test]
fn parallel_retirement_keeps_original_postorder_and_promotes_all_surviving_frontiers() {
    const ROOT_COUNT: usize = 1_024;
    let mut world = World::new();
    let root_owner = HierarchyPlacementOwner::Root;
    let [head, middle, tail, unmanaged_child] = std::array::from_fn(|_| world.spawn_empty().id());
    let mut setup = vec![HierarchyOperation::Place {
        entity: head,
        placement: HierarchyPlacement::last(root_owner),
    }];
    setup.push(HierarchyOperation::Place {
        entity: unmanaged_child,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: head }),
    });
    let mut retirements = Vec::new();
    let mut postorder = Vec::new();
    let mut frontiers = Vec::new();
    let mut all_entities = vec![head, middle, tail, unmanaged_child];
    for index in 0..ROOT_COUNT {
        if index == ROOT_COUNT / 2 {
            setup.push(HierarchyOperation::Place {
                entity: middle,
                placement: HierarchyPlacement::last(root_owner),
            });
        }
        let [
            root,
            branch,
            nested,
            inner_retirement,
            inner_survivor,
            direct,
        ] = std::array::from_fn(|_| world.spawn_empty().id());
        setup.push(HierarchyOperation::Place {
            entity: root,
            placement: HierarchyPlacement::last(root_owner),
        });
        for (entity, parent) in [
            (branch, root),
            (nested, branch),
            (inner_retirement, nested),
            (inner_survivor, inner_retirement),
            (direct, root),
        ] {
            setup.push(HierarchyOperation::Place {
                entity,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            });
        }
        all_entities.extend([
            root,
            branch,
            nested,
            inner_retirement,
            inner_survivor,
            direct,
        ]);
        retirements.extend([root, branch, inner_retirement]);
        postorder.extend([inner_retirement, branch, root]);
        frontiers.extend([nested, inner_survivor, direct]);
    }
    setup.push(HierarchyOperation::Place {
        entity: tail,
        placement: HierarchyPlacement::last(root_owner),
    });
    HierarchyWriter::new(&mut world)
        .apply_batch(&setup)
        .unwrap();
    retirements.reverse();

    let operations = __private::retirement_operations(&world, &retirements).unwrap();

    assert_eq!(
        operations
            .iter()
            .filter_map(|operation| match operation {
                HierarchyOperation::Move { entity, .. } => Some(*entity),
                _ => None,
            })
            .collect::<Vec<_>>(),
        frontiers
    );
    assert_eq!(
        operations
            .iter()
            .filter_map(|operation| match operation {
                HierarchyOperation::Remove { entity, .. } => Some(*entity),
                _ => None,
            })
            .collect::<Vec<_>>(),
        postorder
    );
    __private::validate_retirement_despawn(&world, &retirements, &all_entities).unwrap();
    __private::prepare_construction_batch(&world, &operations)
        .unwrap()
        .commit(&mut world);
    let mut expected = vec![head, middle, tail];
    expected.extend_from_slice(&frontiers);
    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), expected);
    for entity in frontiers {
        assert!(world.get::<Parent>(entity).is_none());
        assert!(world.get::<Children>(entity).is_none());
        assert!(world.get::<HierarchyMember>(entity).is_some());
    }
    for entity in retirements {
        assert!(world.get::<HierarchyMember>(entity).is_none());
        assert!(world.get::<Parent>(entity).is_none());
        assert!(world.get::<Children>(entity).is_none());
        assert!(world.despawn(entity));
    }
    assert_eq!(
        world.get::<Children>(head).unwrap().as_slice(),
        [unmanaged_child]
    );
    validate_dirty_hierarchy(&mut world).unwrap();
    assert_eq!(world.resource::<HierarchyOrder>().roots(), expected);
}

#[test]
fn final_projection_elision_cannot_hide_intermediate_lifecycle_targets() {
    use nara_ecs::bevy_ecs::{
        lifecycle::Insert,
        observer::{Observer, On},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    for observed_component in 0..3 {
        let mut world = World::new();
        let [left, right, child, transient] = std::array::from_fn(|_| world.spawn_empty().id());
        let roots = HierarchyPlacementOwner::Root;
        let left_owner = HierarchyPlacementOwner::Parent { parent: left };
        let right_owner = HierarchyPlacementOwner::Parent { parent: right };
        HierarchyWriter::new(&mut world)
            .apply_batch(&[
                HierarchyOperation::Place {
                    entity: left,
                    placement: HierarchyPlacement::last(roots),
                },
                HierarchyOperation::Place {
                    entity: right,
                    placement: HierarchyPlacement::last(roots),
                },
                HierarchyOperation::Place {
                    entity: child,
                    placement: HierarchyPlacement::last(left_owner),
                },
            ])
            .unwrap();
        validate_dirty_hierarchy(&mut world).unwrap();
        let publication = __private::completed_topology_publication(&world);
        let epoch = world.resource::<HierarchyRoots>().epoch();
        let generation = world
            .resource::<HierarchyGenerationState>()
            .topology_generation;
        let hits = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&hits);
        match observed_component {
            0 => {
                world.spawn(
                    Observer::new(move |_: On<Insert<Parent>>| {
                        observed.fetch_add(1, Ordering::SeqCst);
                    })
                    .with_entity(child),
                );
            }
            1 => {
                world.spawn(
                    Observer::new(move |_: On<Insert<Children>>| {
                        observed.fetch_add(1, Ordering::SeqCst);
                    })
                    .with_entity(right),
                );
            }
            _ => {
                world.spawn(
                    Observer::new(move |_: On<Insert<HierarchyMember>>| {
                        observed.fetch_add(1, Ordering::SeqCst);
                    })
                    .with_entity(transient),
                );
            }
        }
        world.flush();
        let operations = [
            HierarchyOperation::Move {
                entity: child,
                from: HierarchyPlacement::last(left_owner),
                to: HierarchyPlacement::last(right_owner),
            },
            HierarchyOperation::Move {
                entity: child,
                from: HierarchyPlacement::last(right_owner),
                to: HierarchyPlacement::last(left_owner),
            },
            HierarchyOperation::Place {
                entity: transient,
                placement: HierarchyPlacement::last(roots),
            },
            HierarchyOperation::Remove {
                entity: transient,
                from: HierarchyPlacement::last(roots),
            },
        ];

        assert_eq!(
            __private::prepare_construction_batch(&world, &operations).err(),
            Some(HierarchyError::LifecycleWork),
        );
        assert_eq!(
            HierarchyWriter::new(&mut world).apply_batch(&operations),
            Err(HierarchyError::LifecycleWork),
        );
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert_eq!(world.resource::<HierarchyRoots>().as_slice(), [left, right]);
        assert_eq!(world.resource::<HierarchyRoots>().epoch(), epoch);
        assert_eq!(
            world
                .resource::<HierarchyGenerationState>()
                .topology_generation,
            generation
        );
        assert_eq!(
            __private::completed_topology_publication(&world),
            publication
        );
        assert_eq!(world.get::<Children>(left).unwrap().as_slice(), [child]);
        assert_eq!(world.get::<Parent>(child).map(Parent::parent), Some(left));
        assert!(world.get::<Children>(right).is_none());
        assert!(world.get::<HierarchyMember>(transient).is_none());
        validate_hierarchy(&world).unwrap();
    }
}

#[test]
fn restored_entities_and_anchors_follow_their_sequential_candidate_owners() {
    let mut world = World::new();
    let [parent, anchor, tail, leaf] = std::array::from_fn(|_| world.spawn_empty().id());
    let roots = HierarchyPlacementOwner::Root;
    let children = HierarchyPlacementOwner::Parent { parent };
    let mut setup = [parent, anchor, tail]
        .map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(roots),
        })
        .to_vec();
    setup.push(HierarchyOperation::Place {
        entity: leaf,
        placement: HierarchyPlacement::last(children),
    });
    HierarchyWriter::new(&mut world)
        .apply_batch(&setup)
        .unwrap();

    HierarchyWriter::new(&mut world)
        .apply_batch(&[
            HierarchyOperation::Remove {
                entity: leaf,
                from: HierarchyPlacement::last(children),
            },
            HierarchyOperation::Remove {
                entity: parent,
                from: HierarchyPlacement::first(roots),
            },
            HierarchyOperation::Remove {
                entity: anchor,
                from: HierarchyPlacement::first(roots),
            },
            HierarchyOperation::Place {
                entity: anchor,
                placement: HierarchyPlacement::last(roots),
            },
            HierarchyOperation::Place {
                entity: parent,
                placement: HierarchyPlacement::before(roots, anchor),
            },
            HierarchyOperation::Place {
                entity: leaf,
                placement: HierarchyPlacement::last(children),
            },
            HierarchyOperation::Move {
                entity: leaf,
                from: HierarchyPlacement::first(children),
                to: HierarchyPlacement::after(roots, parent),
            },
            HierarchyOperation::Move {
                entity: anchor,
                from: HierarchyPlacement::last(roots),
                to: HierarchyPlacement::last(children),
            },
            HierarchyOperation::Move {
                entity: leaf,
                from: HierarchyPlacement::after(roots, parent),
                to: HierarchyPlacement::before(children, anchor),
            },
            HierarchyOperation::Move {
                entity: parent,
                from: HierarchyPlacement::last(roots),
                to: HierarchyPlacement::first(roots),
            },
        ])
        .unwrap();

    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        [parent, tail]
    );
    assert_eq!(
        world.get::<Children>(parent).unwrap().as_slice(),
        [leaf, anchor]
    );
    assert!(!world.resource::<HierarchyRoots>().contains(anchor));
    assert!(!world.resource::<HierarchyRoots>().contains(leaf));
    assert_eq!(
        HierarchyPlacement::current(&world, parent),
        Some(HierarchyPlacement::first(roots))
    );
    assert_eq!(
        HierarchyPlacement::current(&world, leaf),
        Some(HierarchyPlacement::first(children))
    );
    assert_eq!(
        HierarchyPlacement::current(&world, anchor),
        Some(HierarchyPlacement::after(children, leaf))
    );
    validate_hierarchy(&world).unwrap();
}

#[test]
fn prepared_retirement_still_requires_despawn_admission_for_every_entity() {
    use nara_ecs::bevy_ecs::{lifecycle::Discard, observer::On};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(nara_ecs::Component)]
    struct RetirementPayload;

    let mut world = World::new();
    let head = world.spawn_empty().id();
    let first = world.spawn_empty().id();
    let last = world.spawn(RetirementPayload).id();
    let survivor = world.spawn_empty().id();
    let root_owner = HierarchyPlacementOwner::Root;
    let mut setup = [head, first, last]
        .map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(root_owner),
        })
        .to_vec();
    setup.push(HierarchyOperation::Place {
        entity: survivor,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: first }),
    });
    HierarchyWriter::new(&mut world)
        .apply_batch(&setup)
        .unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();
    let publication = __private::completed_topology_publication(&world);
    let epoch = world.resource::<HierarchyRoots>().epoch();
    let hits = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&hits);
    world.add_observer(move |_: On<Discard<RetirementPayload>>| {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    world.flush();
    let operations = __private::retirement_operations(&world, &[first, last]).unwrap();
    let prepared = __private::prepare_construction_batch(&world, &operations).unwrap();

    assert_eq!(
        __private::validate_retirement_despawn(&world, &[first, last], &[first, last, survivor]),
        Err(LifecycleFreeDespawnError::LifecycleWork),
    );
    drop(prepared);

    assert_eq!(
        world.resource::<HierarchyRoots>().as_slice(),
        [head, first, last]
    );
    assert_eq!(world.resource::<HierarchyRoots>().epoch(), epoch);
    assert_eq!(
        __private::completed_topology_publication(&world),
        publication
    );
    assert_eq!(
        world.get::<Parent>(survivor).map(Parent::parent),
        Some(first)
    );
    assert_eq!(world.get::<Children>(first).unwrap().as_slice(), [survivor]);
    assert!(world.get::<HierarchyMember>(first).is_some());
    assert!(world.get::<HierarchyMember>(last).is_some());
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    validate_hierarchy(&world).unwrap();
}

#[test]
fn sparse_repeated_edits_index_only_the_touched_sibling_set() {
    const ROOT_COUNT: usize = 2_048;
    let mut world = World::new();
    let mut setup = Vec::new();
    let mut selected = None;
    for index in 0..ROOT_COUNT {
        let [parent, first, middle, last] = std::array::from_fn(|_| world.spawn_empty().id());
        setup.push(HierarchyOperation::Place {
            entity: parent,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        });
        setup.extend(
            [first, middle, last].map(|entity| HierarchyOperation::Place {
                entity,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            }),
        );
        if index == ROOT_COUNT / 2 {
            selected = Some((parent, [first, middle, last]));
        }
    }
    HierarchyWriter::new(&mut world)
        .apply_batch(&setup)
        .unwrap();
    let (parent, children) = selected.unwrap();
    let owner = HierarchyPlacementOwner::Parent { parent };
    let mut candidate = CandidateTopology::read(&world);
    assert!(candidate.links.is_empty());
    assert!(candidate.children_overrides.is_empty());
    assert!(candidate.roots.is_none());

    for _ in 0..2_048 {
        for to in [
            HierarchyPlacement::last(owner),
            HierarchyPlacement::first(owner),
        ] {
            let from = candidate.current_placement(children[0]).unwrap();
            candidate
                .apply(
                    &world,
                    HierarchyOperation::Move {
                        entity: children[0],
                        from,
                        to,
                    },
                )
                .unwrap();
        }
        assert_eq!(candidate.links.len(), children.len());
        assert_eq!(candidate.children_overrides.len(), 1);
        assert_eq!(candidate.placement_overrides.len(), 1);
        assert!(candidate.roots.is_none());
    }
    let projection = candidate.into_projection();
    assert!(projection.roots.is_none());
    assert_eq!(projection.children, [(parent, children.to_vec())]);
    assert_eq!(projection.placements.len(), 1);
    assert_eq!(world.get::<Children>(parent).unwrap().as_slice(), children);
    assert!(hierarchy_baseline_is_valid(&world));
}
