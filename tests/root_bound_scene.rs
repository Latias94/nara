#![cfg(feature = "runtime-core")]

use std::{f32::consts::FRAC_PI_2, time::Duration};

use nara::{
    app::{App, RuntimeInstance, StartupStage},
    asset::AssetRef,
    core::Vec2,
    ecs::{Entity, Resource, World},
    hierarchy::{Children, HierarchyPlugin, HierarchyRoots, Parent},
    identity::{EntityLookup, SpawnedSceneInstance},
    reflect::{
        ComponentRegistryPlugin, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        component_registry,
    },
    scene::{
        CompiledSceneDocument, InMemoryPrefabSourceResolver, PrefabDocument, PrefabEntityOrigin,
        PrefabEntityTarget, PrefabInstance, PrefabInstanceHop, PrefabProjectionStep,
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityPlacement,
        SceneEntityRecord, SceneEntitySource, SceneOccurrenceKey, ScenePlacement,
        ScenePlacementParent, SourceDocumentKey, SourceDocumentKind, spawn_scene,
    },
    transform::{GlobalTransform2d, Transform2d, TransformPlugin, historical_spatial_snapshot},
};

#[derive(Resource)]
struct Materialized {
    instance: SpawnedSceneInstance,
    children: [[SceneEntityId; 3]; 2],
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "8f9a6f55-835c-463d-8118-c9961f2e72a8".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

fn source(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/{name}.prefab.ron")).unwrap()
}

fn transform_entity(name: &str, transform: Transform2d) -> SceneEntityRecord {
    let number = |value: f32| ComponentValue::f64(f64::from(value)).unwrap();
    let vector =
        |value: Vec2| ComponentValue::map([("x", number(value.x)), ("y", number(value.y))]);
    SceneEntityRecord::new(id(name)).with_component(
        ComponentTypeId::new("nara.transform.Transform2d"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                ("translation", vector(transform.translation)),
                ("rotation", number(transform.rotation)),
                ("scale", vector(transform.scale)),
            ]),
        ),
    )
}

fn child(name: &str, parent: &str) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        PrefabEntityTarget::source(id(name)),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id(parent),
        ))),
    )
}

fn projected_id(
    document: &CompiledSceneDocument,
    anchor: &str,
    base: &str,
    local: &str,
) -> SceneEntityId {
    let anchor = id(anchor);
    let base = key(base, SourceDocumentKind::Prefab);
    let local = id(local);
    let mut matches = document
        .canonical_provenance()
        .iter()
        .filter(|(_, origin)| {
            origin.local_entity_id() == &local && origin.projection_path().is_some_and(|path| {
                path.root_anchor() == &anchor
                    && matches!(
                        path.steps().last(),
                        Some(PrefabProjectionStep::SourceEntity { canonical_source, entity, .. })
                            if canonical_source == &base && entity == &local
                    )
            })
        });
    let (entity, _) = matches
        .next()
        .expect("the authored occurrence must survive");
    assert!(matches.next().is_none(), "the occurrence must be unique");
    entity.clone()
}

fn spawn_fixture(world: &mut World) {
    let registry = component_registry(world).unwrap().clone();
    let leaf = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("leaf-root")),
            child("inherited-first", "leaf-root"),
            child("inherited-last", "leaf-root"),
        ],
        [
            transform_entity(
                "leaf-root",
                Transform2d {
                    translation: Vec2::new(1000.0, -2000.0),
                    rotation: -FRAC_PI_2,
                    scale: Vec2::new(7.0, 11.0),
                },
            ),
            transform_entity(
                "inherited-first",
                Transform2d::from_translation(Vec2::new(2.0, 1.0)),
            ),
            transform_entity(
                "inherited-last",
                Transform2d::from_translation(Vec2::new(-1.0, 4.0)),
            ),
        ],
    );
    let mut branch_root = transform_entity(
        "branch-root",
        Transform2d {
            translation: Vec2::new(20.0, -10.0),
            rotation: 0.0,
            scale: Vec2::new(3.0, 2.0),
        },
    );
    branch_root.prefab = Some(PrefabInstance::root(
        source("leaf"),
        key("leaf", SourceDocumentKind::Prefab),
        id("leaf-root"),
    ));
    let branch = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("branch-root")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("local-middle")),
                ScenePlacement::before(
                    ScenePlacementParent::Entity(PrefabEntityTarget::source(id("branch-root"))),
                    PrefabEntityTarget {
                        through: vec![PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::SourceEntity(id("branch-root")),
                            expected_base: key("leaf", SourceDocumentKind::Prefab),
                        }],
                        entity: PrefabEntityOrigin::SourceEntity(id("inherited-last")),
                    },
                ),
            ),
        ],
        [
            branch_root,
            transform_entity(
                "local-middle",
                Transform2d::from_translation(Vec2::new(3.0, -2.0)),
            ),
        ],
    );
    let mut replaced = transform_entity(
        "z-replaced",
        Transform2d {
            translation: Vec2::new(5.0, 7.0),
            rotation: FRAC_PI_2,
            scale: Vec2::new(2.0, 3.0),
        },
    );
    let instance = PrefabInstance::root(
        source("branch"),
        key("branch", SourceDocumentKind::Prefab),
        id("branch-root"),
    );
    replaced.prefab = Some(instance.clone());
    // This occurrence has no containing Transform record: the bound root must supply it.
    let mut inherited = SceneEntityRecord::new(id("a-inherited"));
    inherited.prefab = Some(instance);
    let scene = SceneDocument::new(
        [
            SceneEntityPlacement::root(id("stage")),
            child("z-replaced", "stage"),
            child("a-inherited", "stage"),
        ],
        [
            transform_entity(
                "stage",
                Transform2d {
                    translation: Vec2::new(10.0, 20.0),
                    rotation: FRAC_PI_2,
                    scale: Vec2::splat(2.0),
                },
            ),
            replaced,
            inherited,
        ],
    );
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            source("leaf"),
            key("leaf", SourceDocumentKind::Prefab),
            leaf,
        )
        .with_prefab(
            source("branch"),
            key("branch", SourceDocumentKind::Prefab),
            branch,
        );
    let report = scene.compile(&registry, &resolver);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    let document = report
        .candidate
        .expect("the actual Transform source compiles");
    assert_eq!(document.entities().len(), 9);
    assert_eq!(document.roots(), &[id("stage")]);
    let children = ["z-replaced", "a-inherited"].map(|anchor| {
        [
            projected_id(&document, anchor, "leaf", "inherited-first"),
            projected_id(&document, anchor, "branch", "local-middle"),
            projected_id(&document, anchor, "leaf", "inherited-last"),
        ]
    });
    let candidate = document.bind_source(key("root-bound-scene", SourceDocumentKind::Scene));
    let report = spawn_scene(
        world,
        &registry,
        &candidate,
        &SceneOccurrenceKey::new("nara.tests", "root-bound-scene").unwrap(),
        None,
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    world.insert_resource(Materialized {
        instance: report.instance.expect("native Transform codecs must spawn"),
        children,
    });
}

fn resolve(world: &World, instance: &SpawnedSceneInstance, id: &SceneEntityId) -> Entity {
    match instance.resolve(world, id) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live Scene entity for {id:?}, got {other:?}"),
    }
}

fn assert_global(global: GlobalTransform2d, origin: Vec2, axes: [Vec2; 2]) {
    for (actual, expected) in [
        (global.translation(), origin),
        (global.matrix().transform_vector2(Vec2::X), axes[0]),
        (global.matrix().transform_vector2(Vec2::Y), axes[1]),
    ] {
        assert!(
            actual.abs_diff_eq(expected, 0.0001),
            "expected {expected:?}, got {actual:?}",
        );
    }
}

#[test]
fn nested_root_bound_prefabs_propagate_real_transforms_without_wrappers() {
    let mut app = App::new();
    app.add_plugins((ComponentRegistryPlugin, HierarchyPlugin, TransformPlugin))
        .unwrap();
    app.add_systems(StartupStage::Scene, spawn_fixture).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let world = runtime.world();
    let materialized = world.resource::<Materialized>();
    assert_eq!(materialized.instance.len(), 9);
    assert_eq!(
        world
            .iter_entities()
            .filter(|entity| entity.contains::<SceneEntitySource>())
            .count(),
        9,
        "two nested root bindings per occurrence must not create wrapper entities",
    );
    let stage = resolve(world, &materialized.instance, &id("stage"));
    let anchors =
        ["z-replaced", "a-inherited"].map(|name| resolve(world, &materialized.instance, &id(name)));
    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), &[stage]);
    assert!(world.get::<Parent>(stage).is_none());
    assert_eq!(world.get::<Children>(stage).unwrap().as_slice(), anchors);
    assert_eq!(
        world.get::<Transform2d>(anchors[0]),
        Some(&Transform2d {
            translation: Vec2::new(5.0, 7.0),
            rotation: FRAC_PI_2,
            scale: Vec2::new(2.0, 3.0),
        }),
    );
    assert_eq!(
        world.get::<Transform2d>(anchors[1]),
        Some(&Transform2d {
            translation: Vec2::new(20.0, -10.0),
            rotation: 0.0,
            scale: Vec2::new(3.0, 2.0),
        }),
    );

    let snapshot = historical_spatial_snapshot(world).unwrap();
    assert_eq!(snapshot.order().unwrap().len(), 9);
    assert_global(
        snapshot.global(stage).unwrap(),
        Vec2::new(10.0, 20.0),
        [Vec2::new(0.0, 2.0), Vec2::new(-2.0, 0.0)],
    );
    // Hand-calculated world origins and basis vectors. Neither source-root transform is
    // multiplied as an extra frame; each root binding contributes to its containing anchor.
    for (index, (origins, axes)) in [
        (
            [
                Vec2::new(-4.0, 30.0),
                Vec2::new(-12.0, 24.0),
                Vec2::new(-16.0, 42.0),
                Vec2::new(0.0, 6.0),
            ],
            [Vec2::new(-4.0, 0.0), Vec2::new(0.0, -6.0)],
        ),
        (
            [
                Vec2::new(30.0, 60.0),
                Vec2::new(26.0, 72.0),
                Vec2::new(38.0, 78.0),
                Vec2::new(14.0, 54.0),
            ],
            [Vec2::new(0.0, 6.0), Vec2::new(-4.0, 0.0)],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let anchor = anchors[index];
        let children = materialized.children[index]
            .each_ref()
            .map(|id| resolve(world, &materialized.instance, id));
        assert_eq!(world.get::<Parent>(anchor).unwrap().parent(), stage);
        assert_eq!(world.get::<Children>(anchor).unwrap().as_slice(), children);
        let source = world.get::<SceneEntitySource>(anchor).unwrap();
        assert_eq!(source.entity_id, id(["z-replaced", "a-inherited"][index]));
        assert!(source.provenance().projection_path().is_none());
        for child in children {
            assert_eq!(world.get::<Parent>(child).unwrap().parent(), anchor);
            assert!(
                world
                    .get::<Children>(child)
                    .is_none_or(|children| children.as_slice().is_empty())
            );
        }
        for (entity, origin) in [anchor, children[0], children[1], children[2]]
            .into_iter()
            .zip(origins)
        {
            assert_global(snapshot.global(entity).unwrap(), origin, axes);
        }
    }
    runtime.finish().unwrap();
}
