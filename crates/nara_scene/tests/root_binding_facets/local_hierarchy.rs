use super::*;

#[test]
fn source_local_hierarchy_refuses_routed_targets_parents_and_siblings() {
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance::forest(source("unopened")));
    let scene = SceneDocument::new(
        [
            SceneEntityPlacement::root(id("anchor")),
            child("first", "anchor"),
            SceneEntityPlacement::new(
                target(&[], "second"),
                ScenePlacement::after(
                    ScenePlacementParent::Entity(target(&[], "anchor")),
                    target(&[], "first"),
                ),
            ),
        ],
        [
            anchor,
            SceneEntityRecord::new(id("first")),
            SceneEntityRecord::new(id("second")),
        ],
    );
    let limits = SceneCompilationLimits::default();
    let prefab = PrefabDocument::new(scene.placements.clone(), scene.entities.clone());
    // Source-local observation needs neither a resolver nor a component registry.
    for hierarchy in [
        scene.local_hierarchy(limits).unwrap(),
        prefab.local_hierarchy(limits).unwrap(),
    ] {
        assert_eq!(hierarchy.roots(), &[id("anchor")]);
        assert_eq!(
            hierarchy.children(&id("anchor")),
            Some(&[id("first"), id("second")][..]),
        );
        assert_eq!(hierarchy.children(&id("first")), Some(&[][..]));
        assert_eq!(hierarchy.children(&id("second")), Some(&[][..]));
    }
    for case in ["target", "parent", "before", "after"] {
        let mut routed = scene.clone();
        let projected_first = target(&[("anchor", "unopened")], "first");
        match case {
            "target" => routed.placements.push(SceneEntityPlacement::new(
                projected_first,
                ScenePlacement::last(ScenePlacementParent::Entity(target(&[], "anchor"))),
            )),
            "parent" => {
                routed.placements[1].placement.parent =
                    ScenePlacementParent::Entity(projected_first);
            }
            "before" => {
                routed.placements[2].placement.slot = ScenePlacementSlot::Before(projected_first);
            }
            "after" => {
                routed.placements[2].placement.slot = ScenePlacementSlot::After(projected_first);
            }
            _ => unreachable!(),
        }
        // A same-spelled local first exists in every control. It must never stand in
        // for the routed first, even though no source loading is possible in this API.
        assert_eq!(
            routed.local_hierarchy(limits),
            Err(SceneCompilationFailureKind::MissingTarget),
            "Scene routed {case}",
        );
        let prefab = PrefabDocument::new(routed.placements, routed.entities);
        assert_eq!(
            prefab.local_hierarchy(limits),
            Err(SceneCompilationFailureKind::MissingTarget),
            "Prefab routed {case}",
        );
    }
}
