#![forbid(unsafe_code)]

#[path = "support/resume_gameplay.rs"]
#[allow(dead_code)]
mod support;

use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::CurrentScene,
    app::RuntimeState,
    asset::Assets,
    ecs::Entity,
    gameplay::GameplayCommandQueue,
    hierarchy::{Children, HierarchyRoots, Parent},
    identity::{EntityLookup, EntityReference, SpawnedSceneInstance, WorldIdentityDomain},
    image::{ImageAsset, ImageColorSpace, ImageExtent},
    material::SamplerDescriptor,
    prelude::{Color, Transform2d, Vec2},
    project_host::{
        HeadlessSession, HeadlessSessionClose, HeadlessSessionError,
        inspect_current_scene_dependency,
    },
    reflect::ComponentTypeId,
    scene::SceneEntitySource,
    sprite::Sprite,
    tasks::{TaskPools, TaskReturnState},
};
use support::{DEADLINE, Game, INSTANCE, MARKER, ProjectCopy, TRAVEL, action, movement};
use two_level_game::{
    GamePhase,
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
    markers::{MarkerLink, MarkerPalette, marker_image_product, marker_product},
    open_explicit_project_root, product_recipe, request_resume,
    resume::encode_resume,
    save::{Checkpoint, MarkerId},
};

#[derive(Clone, Copy)]
struct Appearance {
    root_size: Vec2,
    root_tint: Color,
    child_translation: Vec2,
    badge_extent: ImageExtent,
    badge_texel: (usize, usize, [u8; 4]),
}

const AUTHORED: Appearance = Appearance {
    root_size: Vec2::new(10.0, 10.0),
    root_tint: Color::rgba(1.0, 0.3, 0.8, 1.0),
    child_translation: Vec2::new(3.0, 3.0),
    badge_extent: ImageExtent::new(2, 2),
    badge_texel: (0, 0, [255, 192, 64, 255]),
};
const EDITED: Appearance = Appearance {
    root_size: Vec2::new(12.0, 8.0),
    root_tint: Color::rgba(0.0, 1.0, 0.0, 1.0),
    child_translation: Vec2::new(5.0, -2.0),
    badge_extent: ImageExtent::new(64, 16),
    badge_texel: (40, 8, [240, 152, 48, 255]),
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkerGraph {
    receipt: SpawnedSceneInstance,
    root: Entity,
    child: Entity,
    source: SceneEntitySource,
}

fn main_receipt(game: &Game) -> SpawnedSceneInstance {
    game.session
        .observe(|world| CurrentScene::get(world).unwrap().receipt().clone())
        .unwrap()
}

fn marker_graphs(
    game: &Game,
    checkpoint: &Checkpoint,
    appearance: Appearance,
) -> BTreeMap<MarkerId, MarkerGraph> {
    let snapshot = game.snapshot();
    assert_eq!(snapshot.level, Some(checkpoint.location.level));
    assert_eq!(
        snapshot.markers,
        checkpoint
            .markers
            .iter()
            .filter(|record| record.level == checkpoint.location.level)
            .map(|record| record.id)
            .collect::<Vec<_>>()
    );
    game.session
        .observe(|world| {
            let current = CurrentScene::get(world).unwrap();
            let main = current.receipt();
            assert_eq!(AUTHORED_LEVEL_MEMBERS, 11);
            assert_eq!(main.len(), AUTHORED_LEVEL_MEMBERS);
            assert_eq!(current.source().entities().len(), AUTHORED_LEVEL_MEMBERS);
            assert_eq!(
                main.entity_ids(),
                current
                    .source()
                    .entities()
                    .iter()
                    .map(|part| part.id().clone())
                    .collect::<Vec<_>>()
            );
            let palettes = world
                .iter_entities()
                .filter_map(|entity| {
                    entity
                        .get::<MarkerPalette>()
                        .map(|palette| (entity.id(), palette))
                })
                .collect::<Vec<_>>();
            let [(camera, palette)] = palettes.as_slice() else {
                panic!("the active level must have exactly one marker palette");
            };
            let product = marker_product();
            assert!(
                palette.template.as_product() == Some(product)
                    || palette.template.as_stable_id() == Some(product.source())
            );
            let camera_source = world.get::<SceneEntitySource>(*camera).unwrap();
            assert_eq!(camera_source.instance_id, main.instance_id());
            assert_eq!(
                main.resolve(world, &camera_source.entity_id),
                EntityLookup::Resolved(*camera)
            );

            let (root_id, child_id, provenance) =
                inspect_current_scene_dependency(world, main, marker_product(), |document| {
                    assert_eq!(document.entities().len(), 2);
                    let [root_id] = document.roots() else {
                        panic!("the marker wrapper must have one root");
                    };
                    assert_eq!(root_id.as_str(), "marker");
                    let root = document
                        .entities()
                        .iter()
                        .find(|part| part.id() == root_id)
                        .unwrap();
                    let [child_id] = root.children() else {
                        panic!("the authored marker root must have one child");
                    };
                    let child = document
                        .entities()
                        .iter()
                        .find(|part| part.id() == child_id)
                        .unwrap();
                    assert!(child.children().is_empty());
                    let link_id = ComponentTypeId::new("two-level-game.MarkerLink");
                    let link = &root.components()[&link_id];
                    assert_eq!(
                        link.value().field("target").unwrap().as_entity_reference(),
                        Some(&EntityReference::SceneLocal {
                            entity: child_id.clone(),
                        })
                    );
                    (
                        root_id.clone(),
                        child_id.clone(),
                        document.canonical_provenance().clone(),
                    )
                })
                .expect("the level retains its actual Immediate marker Scene");
            assert_ne!(root_id, child_id);
            assert!(!main.contains(&root_id));
            assert!(!main.contains(&child_id));

            let records = checkpoint
                .markers
                .iter()
                .filter(|record| record.level == checkpoint.location.level)
                .collect::<Vec<_>>();
            let roots = world
                .iter_entities()
                .filter(|entity| entity.contains::<MarkerLink>())
                .collect::<Vec<_>>();
            assert_eq!(roots.len(), records.len());
            let domain = world.resource::<WorldIdentityDomain>();
            assert_eq!(domain.stats().active_scene_instances, 1 + records.len());
            assert_eq!(
                domain.stats().active_scene_entities,
                AUTHORED_LEVEL_MEMBERS + 2 * records.len()
            );
            assert_eq!(
                world
                    .iter_entities()
                    .filter(|entity| entity.contains::<SceneEntitySource>())
                    .count(),
                AUTHORED_LEVEL_MEMBERS + 2 * records.len()
            );
            let hierarchy_roots = world.resource::<HierarchyRoots>();
            let images = world.resource::<Assets<ImageAsset>>();
            let mut badges = images
                .iter()
                .filter(|(_, image)| image.product() == Some(marker_image_product()));
            let (_, badge) = badges
                .next()
                .expect("the current level retains its marker's Image");
            assert!(
                badges.next().is_none(),
                "occurrences must share one resident Image"
            );
            assert_eq!(badge.extent(), appearance.badge_extent);
            assert_eq!(badge.color_space(), ImageColorSpace::Srgb);
            assert!(badge.source().is_some());
            let (x, y, color) = appearance.badge_texel;
            let offset = (y * badge.extent().width as usize + x) * 4;
            assert_eq!(badge.pixels()[offset..offset + 4], color);
            let mut graphs = BTreeMap::new();
            for root in roots {
                let source = root.get::<SceneEntitySource>().unwrap();
                assert_eq!(source.entity_id, root_id);
                assert_ne!(source.instance_id, main.instance_id());
                assert_eq!(source.provenance(), &provenance[&root_id]);
                let receipt = domain
                    .active_scene_instance(world, source.instance_id)
                    .unwrap();
                assert_eq!(receipt.len(), 2);
                assert!(receipt.contains(&root_id) && receipt.contains(&child_id));
                assert_eq!(
                    receipt.resolve(world, &root_id),
                    EntityLookup::Resolved(root.id())
                );
                let EntityLookup::Resolved(child) = receipt.resolve(world, &child_id) else {
                    panic!("the complete authored child must resolve in its original occurrence");
                };
                let transform = root.get::<Transform2d>().unwrap();
                let matching_records = records
                    .iter()
                    .filter(|record| Vec2::from_array(record.position) == transform.translation)
                    .collect::<Vec<_>>();
                let [record] = matching_records.as_slice() else {
                    panic!("each live marker root must match exactly one saved placement");
                };
                assert_eq!(
                    *transform,
                    Transform2d::from_translation(Vec2::from_array(record.position))
                );
                let sprite = root.get::<Sprite>().unwrap();
                assert_eq!(sprite.size, appearance.root_size);
                assert_eq!(sprite.material.tint, appearance.root_tint);
                assert!(sprite.material.image.is_none());
                assert!(root.get::<Parent>().is_none());
                assert!(hierarchy_roots.contains(root.id()));
                assert_eq!(root.get::<Children>().unwrap().as_slice(), &[child]);

                let child_source = world.get::<SceneEntitySource>(child).unwrap();
                assert_eq!(child_source.instance_id, source.instance_id);
                assert_eq!(child_source.entity_id, child_id);
                assert_eq!(child_source.provenance(), &provenance[&child_id]);
                assert_eq!(child_source.containing_source(), source.containing_source());
                assert_eq!(child_source.occurrence(), source.occurrence());
                assert_eq!(world.get::<Parent>(child).unwrap().parent(), root.id());
                assert!(!hierarchy_roots.contains(child));
                assert!(
                    world
                        .get::<Children>(child)
                        .is_none_or(|children| children.as_slice().is_empty())
                );
                assert!(world.get::<MarkerLink>(child).is_none());
                assert_eq!(
                    world.get::<Transform2d>(child),
                    Some(&Transform2d::from_translation(appearance.child_translation))
                );
                let child_sprite = world.get::<Sprite>(child).unwrap();
                assert_eq!(child_sprite.size, Vec2::new(4.0, 4.0));
                assert_eq!(child_sprite.material.tint, Color::WHITE);
                assert_eq!(
                    child_sprite.material.sampler,
                    SamplerDescriptor::NEAREST_CLAMP
                );
                let handle = child_sprite
                    .material
                    .image
                    .expect("marker child binds the authored Image");
                assert!(std::ptr::eq(images.get(handle).unwrap(), badge));

                let link = root.get::<MarkerLink>().unwrap();
                assert_eq!(
                    link.target,
                    EntityReference::SceneLocal {
                        entity: child_id.clone(),
                    }
                );
                assert_eq!(
                    domain.resolve_entity_reference(world, &link.target, Some(source.instance_id)),
                    EntityLookup::Resolved(child)
                );
                assert_eq!(
                    domain.resolve_entity_reference(world, &link.target, None),
                    EntityLookup::ContextRequired
                );
                assert_eq!(
                    domain.resolve_entity_reference(world, &link.target, Some(main.instance_id())),
                    EntityLookup::Missing
                );
                assert!(
                    graphs
                        .insert(
                            record.id,
                            MarkerGraph {
                                receipt,
                                root: root.id(),
                                child,
                                source: source.clone(),
                            },
                        )
                        .is_none()
                );
            }
            for (id, graph) in &graphs {
                for (other_id, other) in &graphs {
                    if id != other_id {
                        assert_ne!(graph.receipt.instance_id(), other.receipt.instance_id());
                        assert_ne!(graph.source.occurrence(), other.source.occurrence());
                        assert_ne!(graph.root, other.root);
                        assert_ne!(graph.child, other.child);
                        let link = world.get::<MarkerLink>(graph.root).unwrap();
                        assert_eq!(
                            domain.resolve_entity_reference(
                                world,
                                &link.target,
                                Some(other.receipt.instance_id()),
                            ),
                            EntityLookup::Resolved(other.child)
                        );
                    }
                }
            }
            graphs
        })
        .unwrap()
}

fn assert_retired(game: &Game, receipt: &SpawnedSceneInstance) {
    game.session
        .observe(|world| {
            let domain = world.resource::<WorldIdentityDomain>();
            assert!(
                domain
                    .active_scene_instance(world, receipt.instance_id())
                    .is_err()
            );
            assert!(world.iter_entities().all(|entity| {
                entity
                    .get::<SceneEntitySource>()
                    .is_none_or(|source| source.instance_id != receipt.instance_id())
            }));
            for id in receipt.entity_ids() {
                assert!(matches!(
                    receipt.resolve(world, id),
                    EntityLookup::Tombstoned(_)
                ));
            }
        })
        .unwrap();
}

fn replace_level(
    game: &mut Game,
    command: &str,
    target: LevelKey,
) -> BTreeMap<MarkerId, MarkerGraph> {
    let before = game.capture();
    let main = main_receipt(game);
    let markers = marker_graphs(game, &before.progress, AUTHORED);
    let snapshot = game.command(action(command));
    assert_eq!(snapshot.level, Some(target));
    assert_eq!(snapshot.scene_members, AUTHORED_LEVEL_MEMBERS);
    let after = game.capture();
    assert_eq!(after.progress.markers, before.progress.markers);
    assert_eq!(
        after.progress.next_marker_id,
        before.progress.next_marker_id
    );
    assert_eq!(after.progress.score, before.progress.score);
    assert_eq!(after.progress.collected, before.progress.collected);
    let replacement = main_receipt(game);
    assert_eq!(replacement.domain_id(), main.domain_id());
    assert_ne!(replacement.instance_id(), main.instance_id());
    assert_retired(game, &main);
    for graph in markers.values() {
        assert_retired(game, &graph.receipt);
    }
    marker_graphs(game, &after.progress, AUTHORED)
}

#[test]
fn repeated_authored_markers_remain_isolated_across_levels_instances_and_retry() {
    let project = ProjectCopy::new();
    let mut game = Game::from_project(project.root());
    let initial_main = main_receipt(&game);
    let first = game.command(action(MARKER));
    game.command(movement(1, true));
    game.command(movement(1, false));
    let second = game.command(action(MARKER));
    let meadow = game.capture();
    assert_eq!(meadow.progress.markers.len(), 2);
    assert_eq!(meadow.progress.markers[0].position, first.player.to_array());
    assert_eq!(
        meadow.progress.markers[1].position,
        second.player.to_array()
    );
    assert_ne!(
        meadow.progress.markers[0].position,
        meadow.progress.markers[1].position
    );
    let original = marker_graphs(&game, &meadow.progress, AUTHORED);
    assert_eq!(original.len(), 2);
    assert_eq!(main_receipt(&game), initial_main);

    let cavern = LevelKey::main(LevelId::Cavern);
    assert!(replace_level(&mut game, TRAVEL, cavern).is_empty());
    game.command(action(MARKER));
    let first_cavern = marker_graphs(&game, &game.capture().progress, AUTHORED);
    assert_eq!(
        first_cavern.keys().map(|id| id.get()).collect::<Vec<_>>(),
        [3]
    );
    let second = LevelKey::new(LevelId::Cavern, 1).unwrap();
    assert!(replace_level(&mut game, INSTANCE, second).is_empty());
    game.command(action(MARKER));
    let second_cavern = marker_graphs(&game, &game.capture().progress, AUTHORED);
    assert_eq!(
        second_cavern.keys().map(|id| id.get()).collect::<Vec<_>>(),
        [4]
    );
    let returned_cavern = replace_level(&mut game, INSTANCE, cavern);
    assert_eq!(
        returned_cavern.keys().copied().collect::<Vec<_>>(),
        first_cavern.keys().copied().collect::<Vec<_>>()
    );
    for (id, graph) in &first_cavern {
        assert_ne!(
            returned_cavern[id].receipt.instance_id(),
            graph.receipt.instance_id()
        );
    }

    let meadow_key = LevelKey::main(LevelId::Meadow);
    let returned = replace_level(&mut game, TRAVEL, meadow_key);
    assert_eq!(
        returned.keys().copied().collect::<Vec<_>>(),
        original.keys().copied().collect::<Vec<_>>()
    );
    for (id, graph) in &original {
        assert_retired(&game, &graph.receipt);
        assert_ne!(
            returned[id].receipt.instance_id(),
            graph.receipt.instance_id()
        );
    }
    let retried = replace_level(&mut game, "two-level-game.retry", meadow_key);
    for (id, graph) in &returned {
        assert_ne!(
            retried[id].receipt.instance_id(),
            graph.receipt.instance_id()
        );
    }
    let final_state = game.capture();
    assert_eq!(
        final_state
            .progress
            .markers
            .iter()
            .map(|marker| marker.id.get())
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(final_state.progress.next_marker_id.get(), 5);
    assert_eq!(
        &final_state.progress.markers[..2],
        meadow.progress.markers.as_slice()
    );
    assert_eq!(final_state.counters.travel_count, 4);
    assert_eq!(final_state.counters.restarts, 1);
    game.close();
}

#[test]
fn prefab_edits_reach_actual_markers_resume_and_retained_source_retry() {
    let project = ProjectCopy::new();
    let mut baseline = Game::from_project(project.root());
    baseline.command(action(MARKER));
    let before = baseline.capture();
    marker_graphs(&baseline, &before.progress, AUTHORED);
    let badge = project.root().join("assets/textures/marker.png");
    fs::copy(project.root().join("assets/textures/explorer.png"), &badge).unwrap();
    baseline.command(action("two-level-game.retry"));
    assert_eq!(baseline.capture().compatibility, before.compatibility);
    marker_graphs(&baseline, &before.progress, AUTHORED);
    baseline.close();

    let path = project.root().join("prefabs/marker.prefab.json");
    let mut prefab: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let entities = prefab["payload"]["entities"].as_array_mut().unwrap();
    let root = entities
        .iter_mut()
        .find(|entity| entity["id"] == "marker-root")
        .unwrap();
    let sprite = &mut root["components"]["nara.sprite.Sprite"]["value"]["value"];
    assert_eq!(sprite["size"]["value"]["x"]["value"], serde_json::json!(10));
    assert_eq!(sprite["size"]["value"]["y"]["value"], serde_json::json!(10));
    sprite["size"]["value"]["x"]["value"] = serde_json::json!(12);
    sprite["size"]["value"]["y"]["value"] = serde_json::json!(8);
    let tint = &mut sprite["material"]["value"]["tint"]["value"];
    assert_eq!(tint["r"]["value"], serde_json::json!(1));
    assert_eq!(tint["g"]["value"], serde_json::json!(0.3));
    assert_eq!(tint["b"]["value"], serde_json::json!(0.8));
    tint["r"]["value"] = serde_json::json!(0);
    tint["g"]["value"] = serde_json::json!(1);
    tint["b"]["value"] = serde_json::json!(0);
    let child = entities
        .iter_mut()
        .find(|entity| entity["id"] == "marker-child")
        .unwrap();
    let translation = &mut child["components"]["nara.transform.Transform2d"]["value"]["value"]["translation"]
        ["value"];
    assert_eq!(translation["x"]["value"], serde_json::json!(3));
    assert_eq!(translation["y"]["value"], serde_json::json!(3));
    translation["x"]["value"] = serde_json::json!(5);
    translation["y"]["value"] = serde_json::json!(-2);
    fs::write(&path, serde_json::to_vec_pretty(&prefab).unwrap()).unwrap();

    let mut edited = Game::from_project(project.root());
    edited.command(action(MARKER));
    let state = edited.capture();
    assert_eq!(state.progress, before.progress);
    assert_eq!(state.compatibility.schema, before.compatibility.schema);
    assert_eq!(
        state.compatibility.configuration,
        before.compatibility.configuration
    );
    let old_stamp = before
        .compatibility
        .levels
        .iter()
        .find(|stamp| stamp.level == LevelId::Meadow)
        .unwrap();
    let new_stamp = state
        .compatibility
        .levels
        .iter()
        .find(|stamp| stamp.level == LevelId::Meadow)
        .unwrap();
    assert_eq!(new_stamp.manifest, old_stamp.manifest);
    assert_ne!(new_stamp.content, old_stamp.content);
    assert_ne!(
        encode_resume(&state).unwrap(),
        encode_resume(&before).unwrap()
    );
    let graphs = marker_graphs(&edited, &state.progress, EDITED);
    let retained = edited.frozen();

    let mut resumed = Game::from_project(project.root());
    let fresh = main_receipt(&resumed);
    assert_ne!(fresh.domain_id(), main_receipt(&edited).domain_id());
    request_resume(&mut resumed.session, &state).unwrap();
    resumed.settle();
    assert_eq!(resumed.capture(), state);
    assert_eq!(
        encode_resume(&resumed.capture()).unwrap(),
        encode_resume(&state).unwrap()
    );
    let restored = marker_graphs(&resumed, &state.progress, EDITED);
    assert_eq!(
        restored.keys().copied().collect::<Vec<_>>(),
        graphs.keys().copied().collect::<Vec<_>>()
    );
    assert_retired(&resumed, &fresh);
    assert_eq!(edited.frozen(), retained);
    resumed.close();

    fs::write(&path, b"{ malformed marker Prefab after admission").unwrap();
    fs::write(&badge, b"malformed marker Image after admission").unwrap();
    let old_main = main_receipt(&edited);
    edited.command(action("two-level-game.retry"));
    assert_eq!(edited.capture().progress, state.progress);
    assert_retired(&edited, &old_main);
    let retried = marker_graphs(&edited, &state.progress, EDITED);
    for (id, graph) in &graphs {
        assert_retired(&edited, &graph.receipt);
        assert_ne!(
            retried[id].receipt.instance_id(),
            graph.receipt.instance_id()
        );
    }
    edited.close();
}

#[test]
fn malformed_marker_prefab_rejects_prepared_instance_travel_without_retiring_the_live_game() {
    reject_changed_marker_source(
        "prefabs/marker.prefab.json",
        "project.content.prefab-format-invalid",
    );
}

#[test]
fn damaged_marker_image_rejects_travel_and_retained_markers_remain_playable() {
    reject_changed_marker_source(
        "assets/textures/marker.png",
        "project.content.image-import-failed",
    );
}

fn reject_changed_marker_source(relative: &str, diagnostic: &str) {
    let project = ProjectCopy::new();
    let mut game = Game::from_project(project.root());
    game.command(action(TRAVEL));
    game.command(action(MARKER));
    let before = game.capture();
    assert_eq!(
        before.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    let old_main = main_receipt(&game);
    let graphs = marker_graphs(&game, &before.progress, AUTHORED);
    let tick = game.tick();
    let generation = game.session.runtime_generation();
    fs::write(
        project.root().join(relative),
        b"{ malformed marker dependency",
    )
    .unwrap();
    game.submit(action(INSTANCE));
    assert_eq!(game.session.step().unwrap(), None);
    let deadline = Instant::now() + DEADLINE;
    let rejected = loop {
        let snapshot = game.snapshot();
        let (idle, returned) = game
            .session
            .observe(|world| {
                (
                    world.resource::<GameplayCommandQueue>().is_idle(),
                    world.resource::<TaskPools>().return_state(),
                )
            })
            .unwrap();
        assert_ne!(returned, TaskReturnState::CleanupFailed);
        if snapshot.error.is_some() && idle && returned == TaskReturnState::Returned {
            break snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "prepared travel did not reject cleanly: {snapshot:?}"
        );
        game.maintenance();
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(game.session.state(), Some(RuntimeState::Paused));
    assert_eq!(game.session.runtime_generation(), generation);
    assert_eq!(game.tick(), tick + 1);
    assert_eq!(rejected.phase, GamePhase::Running);
    assert!(rejected.error.as_deref().unwrap().contains(diagnostic));
    assert!(!rejected.restoring);
    assert_eq!(rejected.level, Some(before.progress.location.level));
    assert_eq!(rejected.current_instance, Some(old_main.instance_id()));
    assert_eq!(rejected.travel_count, before.counters.travel_count);
    assert_eq!(rejected.score, before.progress.score);
    assert_eq!(rejected.collected, before.progress.collected.len());
    assert_eq!(rejected.player.to_array(), before.player.translation);
    assert_eq!(main_receipt(&game), old_main);
    assert_eq!(marker_graphs(&game, &before.progress, AUTHORED), graphs);

    game.submit(movement(1, true));
    assert_eq!(game.session.step().unwrap(), None);
    let continued = game.snapshot();
    assert!(continued.player.x > rejected.player.x);
    assert_eq!(continued.error, rejected.error);
    game.submit(movement(1, false));
    assert_eq!(game.session.step().unwrap(), None);
    assert_eq!(game.snapshot().held, [false; 4]);
    assert_eq!(main_receipt(&game), old_main);
    assert_eq!(marker_graphs(&game, &before.progress, AUTHORED), graphs);

    let retried = game.command(action("two-level-game.retry"));
    assert_ne!(retried.current_instance, rejected.current_instance);
    assert_eq!(game.capture().progress, before.progress);
    game.close();
}

#[test]
fn corrupt_immediate_marker_scene_refuses_startup_and_physically_closes_the_session() {
    let project = ProjectCopy::new();
    fs::write(
        project.root().join("scenes/marker.scene.json"),
        b"{ corrupt Immediate marker dependency",
    )
    .unwrap();
    let mut session = HeadlessSession::new(
        open_explicit_project_root(project.root()).unwrap(),
        product_recipe(),
    );
    let startup = session.start();
    let observation = session.observe(|_| ());
    let deadline = Instant::now() + DEADLINE;
    let terminal = loop {
        match session.close() {
            HeadlessSessionClose::Pending if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1));
            }
            HeadlessSessionClose::Pending => {
                panic!(
                    "failed marker startup retained pending cleanup: {:?}",
                    session.diagnostics()
                );
            }
            terminal => break terminal,
        }
    };
    assert!(matches!(startup, Err(HeadlessSessionError::Runtime(report)) if report.has_errors()));
    assert!(matches!(
        observation,
        Err(HeadlessSessionError::Unavailable)
    ));
    assert_eq!(terminal, HeadlessSessionClose::Failed);
    assert!(session.diagnostics().has_errors());
    assert!(session.is_closed());
    assert_eq!(session.state(), None);
    assert_eq!(session.close(), HeadlessSessionClose::Failed);
    assert!(session.observe(|_| ()).is_err());
    assert!(matches!(
        session.start(),
        Err(HeadlessSessionError::Unavailable)
    ));
}
