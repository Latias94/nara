use std::{collections::BTreeSet, time::Duration};

use nara_app::{App, RuntimeInstance, StartupStage};
use nara_asset::AssetRef;
use nara_input::{InputPointerId, InputSeatId, InputWindowId};
use nara_reflect::{ComponentRegistry, ComponentRegistryPlugin, component_registry};
use nara_scene::{
    ExpandedSceneCandidate, InMemoryPrefabSourceResolver, PrefabAddition, PrefabDocument,
    PrefabEntityTarget, PrefabInstance, PrefabOverlay, PrefabPlacement, PrefabPlacementParent,
    PrefabPlacementSlot, PrefabProjectionStep, PrefabSourceResolver, SceneDocument, SceneEntityId,
    SceneEntityRecord, SceneEntitySource, SourceDocumentKey,
};
use nara_ui::{
    UiAdapterDiagnostics, UiNode, UiPlugin, UiRoot, UiStyle, UiSurfaceEndpoint, UiSurfaceSlot,
    install_ui_surface, latest_ui_frame,
};
use nara_ui_kernel::{
    UiLayer, UiRootKey, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform, UiSurfaceId,
};

#[test]
fn source_and_occurrence_keep_equal_document_local_ui_roots_independent() {
    for (second_root, second_source) in [
        ("different-root", "second-document"),
        ("root", "second-document"),
        ("root", "first-document"),
    ] {
        let (mut app, surface) = source_identity_app();
        app.add_systems(StartupStage::Scene, move |world: &mut nara_ecs::World| {
            let registry = ComponentRegistry::from_snapshot(
                component_registry(world).unwrap().snapshot().unwrap(),
            );

            let mut instances = Vec::new();
            for (root, marker) in [("root", "first-document"), (second_root, "second-document")] {
                let root = SceneEntityId::new(root).unwrap();
                let document = SceneDocument::from_ordered_roots([
                    SceneEntityRecord::new(root.clone()),
                    SceneEntityRecord::new(SceneEntityId::new(marker).unwrap()),
                ]);
                let report = nara_scene::spawn_scene(
                    world,
                    &registry,
                    &nara_scene::ExpandedSceneCandidate::plain(
                        scene_source_key(if marker == "first-document" {
                            marker
                        } else {
                            second_source
                        }),
                        std::sync::Arc::new((document).clone()),
                    ),
                    &scene_occurrence(marker),
                    None,
                );
                assert!(!report.diagnostics.has_errors());
                let instance = report.instance.unwrap();
                let root_entity = world
                    .iter_entities()
                    .find(|entity| {
                        entity.get::<SceneEntitySource>().is_some_and(|source| {
                            source.instance_id == instance.instance_id() && source.entity_id == root
                        })
                    })
                    .unwrap()
                    .id();
                world
                    .entity_mut(root_entity)
                    .insert((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())));
                instances.push(instance);
            }
            assert_ne!(instances[0].instance_id(), instances[1].instance_id());
        })
        .unwrap();

        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let frame = latest_ui_frame(runtime.world(), surface).unwrap();
        let diagnostics = runtime
            .world()
            .resource::<UiAdapterDiagnostics>()
            .as_slice()
            .to_vec();
        runtime
            .finish()
            .expect("the window-free UI runtime must retire");
        let frame = frame.expect("complete source/occurrence coordinates must distinguish roots");
        assert_eq!(frame.root_outcomes().len(), 2);
        assert_ne!(
            frame.root_outcomes()[0].key(),
            frame.root_outcomes()[1].key()
        );
        assert!(diagnostics.is_empty());
    }
}

fn scene_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

#[test]
fn canonical_prefab_alias_rebuild_preserves_keys_but_a_containing_source_switch_does_not() {
    use nara_ui::UiText;

    let local = |name| SceneEntityId::new(name).unwrap();
    let prefab = PrefabDocument::new(
        [local("root")],
        [
            SceneEntityRecord::new(local("root")).with_children([local("label")]),
            SceneEntityRecord::new(local("label")),
        ],
    );
    let first_request = AssetRef::path("prefabs/first.ron").unwrap();
    let alias = AssetRef::path("prefabs/alias.ron").unwrap();
    let canonical = |name| {
        nara_scene::SourceDocumentKey::named(
            "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
            name,
            nara_scene::SourceDocumentKind::Prefab,
        )
        .unwrap()
    };
    let middle = AssetRef::path("prefabs/middle.ron").unwrap();
    let leaf = AssetRef::path("prefabs/leaf.ron").unwrap();
    let nested = |name, request: AssetRef| {
        let mut anchor = SceneEntityRecord::new(local(name));
        anchor.prefab = Some(PrefabInstance {
            source: request,
            overrides: None,
        });
        anchor
    };
    let outer = PrefabDocument::from_ordered_roots([
        nested("left", middle.clone()),
        nested("right", middle.clone()),
    ]);
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(leaf.clone(), canonical("panel"), prefab)
        .with_prefab(
            middle,
            canonical("middle"),
            PrefabDocument::from_ordered_roots([nested("inner", leaf)]),
        )
        .with_prefab(first_request.clone(), canonical("outer"), outer.clone())
        .with_prefab(alias.clone(), canonical("outer"), outer);
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let candidates = [
        (first_request, "first"),
        (alias.clone(), "first"),
        (alias, "other"),
    ]
    .into_iter()
    .map(|(request, containing)| {
        let mut anchor = SceneEntityRecord::new(local("anchor"));
        anchor.prefab = Some(PrefabInstance {
            source: request,
            overrides: None,
        });
        let expanded =
            SceneDocument::from_ordered_roots([anchor]).expand_prefabs(&registry, &resolver);
        assert!(!expanded.diagnostics.has_errors());
        expanded
            .candidate
            .unwrap()
            .bind_source(scene_source_key(containing))
    })
    .collect::<Vec<_>>();
    let (mut app, surface) = source_identity_app();
    let mut current = None;
    let mut index = 0;
    app.add_systems(
        nara_app::CoreStage::Update,
        move |world: &mut nara_ecs::World| {
            let Some(candidate) = candidates.get(index) else {
                return;
            };
            let registry = ComponentRegistry::from_snapshot(
                component_registry(world).unwrap().snapshot().unwrap(),
            );
            let prepared = match current.as_ref() {
                Some(current) => nara_scene::prepare_scene_replacement(
                    world, &registry, candidate, current, None,
                ),
                None => nara_scene::prepare_scene(
                    world,
                    &registry,
                    candidate,
                    &scene_occurrence("alias"),
                    None,
                ),
            }
            .unwrap();
            let report = prepared.commit(world);
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let instance = report.instance.unwrap();
            let entities = world
                .iter_entities()
                .filter_map(|entity| {
                    entity
                        .get::<SceneEntitySource>()
                        .filter(|source| source.instance_id == instance.instance_id())
                        .map(|source| (entity.id(), source.provenance().local_entity_id().clone()))
                })
                .collect::<Vec<_>>();
            for (entity, source) in entities {
                if source.as_str() == "root" {
                    world
                        .entity_mut(entity)
                        .insert((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())));
                } else if source.as_str() == "label" {
                    world.entity_mut(entity).insert((
                        UiNode::new(UiStyle::absolute(0.0, 0.0, 80.0, 30.0)),
                        UiText::new("Panel", 16.0).unwrap(),
                    ));
                }
            }
            current = Some(instance);
            index += 1;
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let mut observations = Vec::new();
    for _ in 0..3 {
        runtime.drive(Duration::ZERO).unwrap();
        let frame = latest_ui_frame(runtime.world(), surface).unwrap();
        let source = runtime
            .world()
            .iter_entities()
            .filter_map(|entity| entity.get::<SceneEntitySource>())
            .find(|source| source.provenance().local_entity_id().as_str() == "root")
            .unwrap();
        let path = source.provenance().projection_path().unwrap();
        let PrefabProjectionStep::SourceEntity {
            source_document, ..
        } = &path.steps()[0]
        else {
            panic!("the inherited outer anchor must retain its source request");
        };
        observations.push((
            frame,
            source.instance_id,
            source_document.clone(),
            path.steps().len(),
        ));
    }
    runtime.finish().unwrap();
    let keys = |frame: &Option<std::sync::Arc<nara_ui_kernel::UiFrame>>| {
        frame
            .as_ref()
            .expect("the source-bound UI must compile")
            .layouts()
            .iter()
            .filter_map(|layout| layout.key().cloned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(observations[0].0.as_ref().unwrap().root_outcomes().len(), 2);
    assert_eq!(keys(&observations[0].0).len(), 4);
    assert_eq!(keys(&observations[0].0), keys(&observations[1].0));
    assert_ne!(keys(&observations[1].0), keys(&observations[2].0));
    assert_ne!(observations[0].1, observations[1].1);
    assert_ne!(observations[0].2, observations[1].2);
    assert!(observations.iter().all(|observation| observation.3 == 3));
}

fn scene_occurrence(slot: &str) -> nara_scene::SceneOccurrenceKey {
    nara_scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}

#[test]
fn direct_overlay_additions_do_not_alias_inherited_ui_keys_or_inherit_the_base_source() {
    let source = AssetRef::path("prefabs/base.ron").unwrap();
    let other_source = AssetRef::path("prefabs/other.ron").unwrap();
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(local_id("shared"))]);
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(source.clone(), prefab_source_key("base"), prefab.clone())
        .with_prefab(other_source.clone(), prefab_source_key("other"), prefab);
    let additions = |request: &AssetRef| {
        replace_roots_with_additions(
            resolver.resolve_prefab(request).unwrap().key().clone(),
            [SceneEntityRecord::new(local_id("shared"))],
        )
    };
    let observations = observe_shared_ui_roots(vec![
        expand_ui_scene(repeated_placements(source.clone(), None), &resolver),
        expand_ui_scene(
            repeated_placements(source.clone(), Some(additions(&source))),
            &resolver,
        ),
        expand_ui_scene(
            repeated_placements(other_source.clone(), Some(additions(&other_source))),
            &resolver,
        ),
    ]);

    assert_eq!(observations[0].keys.len(), 2);
    assert!(observations[0].keys.is_disjoint(&observations[1].keys));
    assert_eq!(observations[1].keys, observations[2].keys);
    for source in &observations[0].sources {
        let path = source.provenance().projection_path().unwrap();
        assert!(
            matches!(path.steps(), [PrefabProjectionStep::SourceEntity { entity, .. }]
            if entity == &local_id("shared"))
        );
    }
    for observation in &observations[1..] {
        assert_repeated_anchors(observation);
        for source in &observation.sources {
            let path = source.provenance().projection_path().unwrap();
            assert_eq!(source.provenance().local_entity_id(), &local_id("shared"));
            assert!(
                matches!(path.steps(), [PrefabProjectionStep::InstanceAddition { entity }]
                if entity == &local_id("shared"))
            );
        }
    }
}

#[test]
fn nested_overlay_additions_do_not_alias_inherited_ui_keys_across_repeated_placements() {
    let outer_source = AssetRef::path("prefabs/outer.ron").unwrap();
    let leaf_source = AssetRef::path("prefabs/leaf.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        leaf_source.clone(),
        prefab_source_key("leaf"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(local_id("shared"))]),
    );
    let candidates = [
        None,
        Some(replace_roots_with_additions(
            resolver.resolve_prefab(&leaf_source).unwrap().key().clone(),
            [SceneEntityRecord::new(local_id("shared"))],
        )),
    ]
    .into_iter()
    .map(|overrides| {
        let resolver = resolver.clone().with_prefab(
            outer_source.clone(),
            prefab_source_key("outer"),
            PrefabDocument::from_ordered_roots([prefab_anchor(
                "inner",
                leaf_source.clone(),
                overrides,
            )]),
        );
        expand_ui_scene(repeated_placements(outer_source.clone(), None), &resolver)
    })
    .collect();
    let observations = observe_shared_ui_roots(candidates);

    assert_eq!(observations[0].keys.len(), 2);
    assert!(observations[0].keys.is_disjoint(&observations[1].keys));
    for (index, observation) in observations.iter().enumerate() {
        assert_repeated_anchors(observation);
        for source in &observation.sources {
            let path = source.provenance().projection_path().unwrap();
            let [
                PrefabProjectionStep::SourceEntity {
                    canonical_source,
                    entity,
                    ..
                },
                terminal,
            ] = path.steps()
            else {
                panic!("the nested leaf must retain its containing Prefab anchor");
            };
            assert_eq!(canonical_source, &prefab_source_key("outer"));
            assert_eq!(entity, &local_id("inner"));
            assert_eq!(terminal.entity_id(), &local_id("shared"));
            if index == 0 {
                assert!(matches!(
                    terminal,
                    PrefabProjectionStep::SourceEntity { .. }
                ));
            } else {
                assert!(matches!(
                    terminal,
                    PrefabProjectionStep::InstanceAddition { .. }
                ));
            }
        }
    }
}

#[test]
fn added_prefab_instances_retain_addition_ancestry_in_descendant_ui_keys() {
    let outer_source = AssetRef::path("prefabs/outer.ron").unwrap();
    let leaf_source = AssetRef::path("prefabs/leaf.ron").unwrap();
    let anchors = ["first", "second"].map(|name| prefab_anchor(name, leaf_source.clone(), None));
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            leaf_source,
            prefab_source_key("leaf"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(local_id("shared"))]),
        )
        .with_prefab(
            outer_source.clone(),
            prefab_source_key("outer"),
            PrefabDocument::from_ordered_roots(anchors.clone()),
        );
    let observations = observe_shared_ui_roots(vec![
        expand_ui_scene(repeated_placements(outer_source.clone(), None), &resolver),
        expand_ui_scene(
            repeated_placements(
                outer_source.clone(),
                Some(replace_roots_with_additions(
                    resolver
                        .resolve_prefab(&outer_source)
                        .unwrap()
                        .key()
                        .clone(),
                    anchors,
                )),
            ),
            &resolver,
        ),
    ]);

    assert_eq!(observations[0].keys.len(), 4);
    assert_eq!(observations[1].keys.len(), 4);
    assert!(observations[0].keys.is_disjoint(&observations[1].keys));
    for (index, observation) in observations.iter().enumerate() {
        assert_repeated_anchors(observation);
        let mut placements = BTreeSet::new();
        for source in &observation.sources {
            let path = source.provenance().projection_path().unwrap();
            let [
                anchor,
                PrefabProjectionStep::SourceEntity {
                    canonical_source,
                    entity,
                    ..
                },
            ] = path.steps()
            else {
                panic!("the leaf must retain both its instance anchor and source entity");
            };
            assert_eq!(canonical_source, &prefab_source_key("leaf"));
            assert_eq!(entity, &local_id("shared"));
            if index == 0 {
                assert!(matches!(anchor, PrefabProjectionStep::SourceEntity { .. }));
            } else {
                assert!(matches!(
                    anchor,
                    PrefabProjectionStep::InstanceAddition { .. }
                ));
            }
            placements.insert((path.root_anchor().clone(), anchor.entity_id().clone()));
        }
        assert_eq!(
            placements,
            ["left", "right"]
                .into_iter()
                .flat_map(|root| {
                    ["first", "second"].map(|anchor| (local_id(root), local_id(anchor)))
                })
                .collect()
        );
    }
}

struct SourceIdentityObservation {
    keys: BTreeSet<UiRootKey>,
    sources: Vec<SceneEntitySource>,
}

fn observe_shared_ui_roots(
    candidates: Vec<ExpandedSceneCandidate>,
) -> Vec<SourceIdentityObservation> {
    let (mut app, surface) = source_identity_app();
    let count = candidates.len();
    let mut current = None;
    let mut index = 0;
    app.add_systems(
        nara_app::CoreStage::Update,
        move |world: &mut nara_ecs::World| {
            let Some(candidate) = candidates.get(index) else {
                return;
            };
            let registry = ComponentRegistry::from_snapshot(
                component_registry(world).unwrap().snapshot().unwrap(),
            );
            let prepared = match current.as_ref() {
                Some(current) => nara_scene::prepare_scene_replacement(
                    world, &registry, candidate, current, None,
                ),
                None => nara_scene::prepare_scene(
                    world,
                    &registry,
                    candidate,
                    &scene_occurrence("overlay"),
                    None,
                ),
            }
            .unwrap();
            let report = prepared.commit(world);
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let instance = report.instance.unwrap();
            let entities = world
                .iter_entities()
                .filter_map(|entity| {
                    entity
                        .get::<SceneEntitySource>()
                        .filter(|source| {
                            source.instance_id == instance.instance_id()
                                && source.provenance().local_entity_id().as_str() == "shared"
                        })
                        .map(|_| entity.id())
                })
                .collect::<Vec<_>>();
            for entity in entities {
                world
                    .entity_mut(entity)
                    .insert((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())));
            }
            current = Some(instance);
            index += 1;
        },
    )
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let mut observations = Vec::new();
    for _ in 0..count {
        runtime.drive(Duration::ZERO).unwrap();
        let diagnostics = runtime
            .world()
            .resource::<UiAdapterDiagnostics>()
            .as_slice();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let frame = latest_ui_frame(runtime.world(), surface)
            .unwrap()
            .expect("complete source/addition provenance must compile without key aliases");
        let sources = runtime
            .world()
            .iter_entities()
            .filter_map(|entity| {
                entity
                    .get::<SceneEntitySource>()
                    .filter(|source| source.provenance().local_entity_id().as_str() == "shared")
                    .cloned()
            })
            .collect::<Vec<_>>();
        let keys = frame
            .root_outcomes()
            .iter()
            .map(|outcome| outcome.key().clone())
            .collect::<BTreeSet<_>>();
        assert!(!sources.is_empty());
        assert_eq!(frame.root_outcomes().len(), sources.len());
        assert_eq!(frame.layouts().len(), sources.len());
        assert_eq!(
            keys.len(),
            sources.len(),
            "every repeated placement needs its own UI key"
        );
        observations.push(SourceIdentityObservation { keys, sources });
    }
    runtime.finish().unwrap();
    observations
}

fn assert_repeated_anchors(observation: &SourceIdentityObservation) {
    assert_eq!(
        observation
            .sources
            .iter()
            .map(|source| {
                source
                    .provenance()
                    .projection_path()
                    .unwrap()
                    .root_anchor()
                    .clone()
            })
            .collect::<BTreeSet<_>>(),
        [local_id("left"), local_id("right")].into_iter().collect()
    );
}

fn local_id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn prefab_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn prefab_anchor(
    id: &str,
    source: AssetRef,
    overrides: Option<PrefabOverlay>,
) -> SceneEntityRecord {
    let mut anchor = SceneEntityRecord::new(local_id(id));
    anchor.prefab = Some(PrefabInstance { source, overrides });
    anchor
}

fn repeated_placements(source: AssetRef, overrides: Option<PrefabOverlay>) -> SceneDocument {
    SceneDocument::from_ordered_roots(
        ["left", "right"].map(|name| prefab_anchor(name, source.clone(), overrides.clone())),
    )
}

fn replace_roots_with_additions(
    base: SourceDocumentKey,
    entities: impl IntoIterator<Item = SceneEntityRecord>,
) -> PrefabOverlay {
    let mut overlay = PrefabOverlay::new(base);
    for entity in entities {
        assert!(
            entity.children.is_empty(),
            "fixture additions are flat roots"
        );
        overlay
            .suppressions
            .push(PrefabEntityTarget::source(entity.id.clone()));
        overlay.placements.push(PrefabPlacement {
            target: PrefabEntityTarget::addition(entity.id.clone()),
            parent: PrefabPlacementParent::Root,
            slot: PrefabPlacementSlot::Last,
        });
        overlay.additions.push(PrefabAddition {
            id: entity.id,
            components: entity.components,
            prefab: entity.prefab,
            bindings: Default::default(),
        });
    }
    overlay
}

fn expand_ui_scene(
    document: SceneDocument,
    resolver: &InMemoryPrefabSourceResolver,
) -> ExpandedSceneCandidate {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let expansion = document.expand_prefabs(&registry, resolver);
    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics
    );
    expansion
        .candidate
        .unwrap()
        .bind_source(scene_source_key("overlay"))
}

fn source_identity_app() -> (App, UiSurfaceId) {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_input::InputPlugin,
        UiPlugin::default(),
    ))
    .unwrap();
    let surface = UiSurfaceId::new(7).unwrap();
    let size = UiSize::new(200.0, 100.0).unwrap();
    install_ui_surface(
        app.world_mut().unwrap(),
        UiSurfaceSlot::new(
            surface,
            UiSurfaceEndpoint::new(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            UiSurfaceContentTransform::new(size, size, UiSafeAreaInsets::default(), 1.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    (app, surface)
}
