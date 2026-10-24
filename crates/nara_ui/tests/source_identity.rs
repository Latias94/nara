use std::time::Duration;

use nara_app::{App, RuntimeInstance, StartupStage};
use nara_input::{InputPointerId, InputSeatId, InputWindowId};
use nara_reflect::{ComponentRegistry, ComponentRegistryPlugin, component_registry};
use nara_scene::{SceneDocument, SceneEntityId, SceneEntityRecord, SceneEntitySource};
use nara_ui::{
    UiAdapterDiagnostics, UiNode, UiPlugin, UiRoot, UiStyle, UiSurfaceEndpoint, UiSurfaceSlot,
    install_ui_surface, latest_ui_frame,
};
use nara_ui_kernel::{UiLayer, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform, UiSurfaceId};

#[test]
fn source_and_occurrence_keep_equal_document_local_ui_roots_independent() {
    for (second_root, second_source) in [
        ("different-root", "second-document"),
        ("root", "second-document"),
        ("root", "first-document"),
    ] {
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
        let slot = UiSurfaceSlot::new(
            surface,
            UiSurfaceEndpoint::new(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            UiSurfaceContentTransform::new(size, size, UiSafeAreaInsets::default(), 1.0).unwrap(),
        )
        .unwrap();
        install_ui_surface(app.world_mut().unwrap(), slot).unwrap();
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
    use nara_asset::AssetRef;
    use nara_scene::{
        InMemoryPrefabSourceResolver, PrefabDocument, PrefabInstance, ScenePatchDocument,
    };
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
            overrides: ScenePatchDocument::default(),
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
            overrides: ScenePatchDocument::default(),
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
                        .map(|source| (entity.id(), source.source_entity_id.clone()))
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
            .find(|source| source.source_entity_id.as_str() == "root")
            .unwrap();
        observations.push((
            frame,
            source.instance_id,
            source.projection_path.as_ref().unwrap().source_chain()[0]
                .source_document()
                .clone(),
            source
                .projection_path
                .as_ref()
                .unwrap()
                .source_chain()
                .len(),
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
