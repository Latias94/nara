use std::{
    f64::consts::FRAC_PI_2,
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use collect_gameplay::{
    Collectible, Direction, GameSnapshot, PlayerConfig, demonstration_commands, movement, reset,
};
use nara::{
    core::Vec2,
    fs::DirectoryCapability,
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick,
    },
    project_host::{HeadlessRun, HeadlessRunOutcome},
    reflect::{ComponentTypeId, ComponentValue, PersistentComponentProvider},
    scene::{
        PrefabDocument, PrefabDocumentCandidate, PrefabEntityTarget, SceneDocument,
        SceneDocumentCandidate, SceneEntityId, SceneEntityPlacement, ScenePlacement,
        ScenePlacementParent,
    },
};

const TRANSFORM: &str = "nara.transform.Transform2d";

#[test]
fn custom_creation_templates_keep_the_current_float_wire_contract() {
    for (schema, field, expected) in [
        (PlayerConfig::persistent_component_schema(), "speed", 240.0),
        (Collectible::persistent_component_schema(), "radius", 20.0),
    ] {
        assert_eq!(schema.version().get(), 2);
        let value = schema
            .authoring_initial_value()
            .expect("the sample publishes complete scalar creation data");
        assert_eq!(
            value,
            &ComponentValue::map([(
                field,
                ComponentValue::f64(expected).expect("the creation value is finite"),
            )])
        );
    }
}

fn run(ticks: u32, commands: Vec<GameplayCommandSubmission>) -> GameSnapshot {
    run_project(
        nara_game::open_project_root().expect("open sample source"),
        ticks,
        commands,
    )
}

fn run_project(
    project_root: DirectoryCapability,
    ticks: u32,
    commands: Vec<GameplayCommandSubmission>,
) -> GameSnapshot {
    let mut run = HeadlessRun::<GameSnapshot>::from_recipe(
        project_root,
        nara_game::product_recipe().expect("construct sample product recipe"),
        NonZeroU32::new(ticks).unwrap(),
        commands,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(snapshot) => return snapshot.clone(),
            HeadlessRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now()
            }
            _ => panic!("game run failed: {:?}", report.diagnostics()),
        }
    }
}

struct SourceProject(PathBuf);

impl SourceProject {
    fn new(edit_scene: impl FnOnce(&mut SceneDocument)) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "nara-collect-game-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).expect("create temporary sample root");
        let project = Self(root);
        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        for file in [
            "nara.toml",
            "scenes/startup.scene.json",
            "prefabs/player.prefab.json",
            "assets/textures/token.png",
            "assets/textures/token.png.meta",
            "schema/authoring.bundle.json",
        ] {
            let destination = project.0.join(file);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source.join(file), destination).unwrap();
        }

        let path = project.0.join("scenes/startup.scene.json");
        let mut scene = SceneDocumentCandidate::decode_json_bytes(&fs::read(&path).unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        edit_scene(&mut scene);
        fs::write(path, scene.to_json_string().unwrap()).unwrap();
        project
    }

    fn edit_player_prefab(&self, edit: impl FnOnce(&mut PrefabDocument)) {
        let path = self.0.join("prefabs/player.prefab.json");
        let mut prefab = PrefabDocumentCandidate::decode_json_bytes(&fs::read(&path).unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        edit(&mut prefab);
        fs::write(path, prefab.to_json_string().unwrap()).unwrap();
    }

    fn run(
        &self,
        ticks: u32,
        drafts: impl IntoIterator<Item = (u64, GameplayCommandDraft)>,
    ) -> GameSnapshot {
        let commands = drafts
            .into_iter()
            .enumerate()
            .map(|(index, (tick, draft))| {
                GameplayCommandSubmission::new(
                    GameplayCommandTick::new(tick).unwrap(),
                    GameplayCommandIngressSource::test("sample.source-fixture").unwrap(),
                    GameplayCommandSourceSequence::new(index as u64 + 1).unwrap(),
                    draft,
                )
            })
            .collect();
        run_project(
            nara_game::open_explicit_project_root(&self.0).expect("open temporary sample source"),
            ticks,
            commands,
        )
    }
}

impl Drop for SourceProject {
    fn drop(&mut self) {
        let result = fs::remove_dir_all(&self.0);
        if !std::thread::panicking() {
            result.expect("remove temporary sample source after runtime cleanup");
        }
    }
}

fn set_scene_field(
    scene: &mut SceneDocument,
    entity_id: &str,
    component_id: &str,
    field: &str,
    value: ComponentValue,
) {
    let entity = scene
        .entities
        .iter_mut()
        .find(|entity| entity.id.as_str() == entity_id)
        .unwrap_or_else(|| panic!("missing authored entity {entity_id}"));
    let component = entity
        .components
        .get_mut(&ComponentTypeId::new(component_id))
        .unwrap_or_else(|| panic!("missing authored component {entity_id}/{component_id}"));
    let ComponentValue::Map(fields) = component.value_mut() else {
        panic!("expected a map for {entity_id}/{component_id}");
    };
    let field_value = fields
        .get_mut(field)
        .unwrap_or_else(|| panic!("missing authored field {entity_id}/{component_id}/{field}"));
    *field_value = value;
}

fn vec2_value(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([
        ("x", ComponentValue::f64(x).unwrap()),
        ("y", ComponentValue::f64(y).unwrap()),
    ])
}

fn set_narrow_pickup(scene: &mut SceneDocument, entity: &str, x: f64, y: f64) {
    set_scene_field(
        scene,
        entity,
        "collect-game.Collectible",
        "radius",
        ComponentValue::f64(1.0).unwrap(),
    );
    set_scene_field(scene, entity, TRANSFORM, "translation", vec2_value(x, y));
}

fn source_target(entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget::source(SceneEntityId::new(entity).unwrap())
}

fn nest_player_anchor(
    scene: &mut SceneDocument,
    scale_y: f64,
    outer_angle: f64,
    anchor_angle: f64,
) {
    let mut parent = scene
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "player-anchor")
        .unwrap()
        .clone();
    parent.id = SceneEntityId::new("outer-parent").unwrap();
    parent.prefab = None;
    scene
        .placements
        .insert(0, SceneEntityPlacement::root(parent.id.clone()));
    scene.entities.push(parent);
    let anchor = scene
        .placements
        .iter_mut()
        .find(|placement| placement.target == source_target("player-anchor"))
        .unwrap();
    anchor.placement =
        ScenePlacement::last(ScenePlacementParent::Entity(source_target("outer-parent")));
    set_scene_field(
        scene,
        "outer-parent",
        TRANSFORM,
        "scale",
        vec2_value(1.0, scale_y),
    );
    for (name, angle) in [
        ("outer-parent", outer_angle),
        ("player-anchor", anchor_angle),
    ] {
        set_scene_field(
            scene,
            name,
            TRANSFORM,
            "rotation",
            ComponentValue::f64(angle).unwrap(),
        );
    }
}

#[test]
fn right_moves_four_world_units_under_transformed_prefab_anchors() {
    for (scale, rotation, authored_start) in [
        ((2.0, 2.0), 0.0, Vec2::new(-240.0, 0.0)),
        ((1.0, 1.0), FRAC_PI_2, Vec2::new(0.0, -120.0)),
        ((-2.0, 0.5), 0.0, Vec2::new(240.0, 0.0)),
    ] {
        let project = SourceProject::new(|scene| {
            set_scene_field(
                scene,
                "player-anchor",
                TRANSFORM,
                "scale",
                vec2_value(scale.0, scale.1),
            );
            set_scene_field(
                scene,
                "player-anchor",
                TRANSFORM,
                "rotation",
                ComponentValue::f64(rotation).unwrap(),
            );
        });
        let start = project.run(1, []).player;
        assert!(
            start.distance(authored_start) < 0.001,
            "scale {scale:?}, angle {rotation}: expected start {authored_start:?}, got {start:?}",
        );
        let moved = project.run(1, [(1, movement(Direction::Right, true))]);
        let displacement = moved.player - start;
        assert!(
            displacement.distance(Vec2::new(4.0, 0.0)) < 0.001,
            "scale {scale:?}, angle {rotation}: expected (4, 0), moved {displacement:?}",
        );
        let bounded = project.run(50, [(1, movement(Direction::Down, true))]);
        let world_limit = Vec2::new(authored_start.x, -160.0);
        assert!(
            bounded.player.distance(world_limit) < 0.001,
            "scale {scale:?}, angle {rotation}: expected world limit {world_limit:?}, got {:?}",
            bounded.player,
        );
    }
}

#[test]
fn an_outside_spawn_walks_toward_the_world_bounds_at_the_configured_speed() {
    let project = SourceProject::new(|scene| {
        set_scene_field(
            scene,
            "player-anchor",
            TRANSFORM,
            "scale",
            vec2_value(3.0, 1.0),
        );
    });
    let start = Vec2::new(-360.0, 0.0);
    assert_eq!(project.run(1, []).player, start);
    assert_eq!(
        project
            .run(1, [(1, movement(Direction::Left, true))])
            .player,
        start
    );
    let inward = project.run(1, [(1, movement(Direction::Right, true))]);
    assert!(inward.player.distance(Vec2::new(-356.0, 0.0)) < 0.001);
}

#[test]
fn a_movement_step_collects_a_narrow_pickup_crossed_between_endpoints() {
    for (x, y, collected_before, collected_after) in
        [(-40.0, 0.0, 1, 1), (-38.0, 0.0, 0, 1), (-38.0, 1.25, 0, 0)]
    {
        let project = SourceProject::new(|scene| set_narrow_pickup(scene, "gem-left", x, y));
        let steps = [(20, -40.0, collected_before), (21, -36.0, collected_after)];
        for (tick, player_x, collected) in steps {
            let snapshot = project.run(tick, [(1, movement(Direction::Right, true))]);
            assert_eq!(snapshot.player, Vec2::new(player_x, 0.0));
            assert_eq!(
                snapshot.collected, collected,
                "pickup at ({x}, {y}), tick {tick}",
            );
            assert_eq!(snapshot.visible_targets, 3 - collected);
        }
    }
}

#[test]
fn reset_sweeps_only_movement_issued_after_it() {
    let project = SourceProject::new(|scene| {
        set_narrow_pickup(scene, "gem-left", -38.0, 0.0);
        set_narrow_pickup(scene, "gem-middle", -122.0, 0.0);
    });

    let returned = project.run(22, [(1, movement(Direction::Right, true)), (22, reset())]);
    assert_eq!(returned.player, Vec2::new(-120.0, 0.0));
    assert_eq!(returned.resets, 1);
    assert_eq!(returned.collected, 0);
    assert_eq!(returned.visible_targets, 3);

    let moved = project.run(
        22,
        [
            (1, movement(Direction::Right, true)),
            (22, reset()),
            (22, movement(Direction::Left, true)),
        ],
    );
    assert_eq!(moved.player, Vec2::new(-124.0, 0.0));
    assert_eq!(moved.resets, 1);
    assert_eq!(moved.collected, 1);
    assert_eq!(moved.visible_targets, 2);
}

#[test]
fn singular_prefab_anchor_keeps_the_player_at_its_world_position() {
    let project = SourceProject::new(|scene| {
        set_scene_field(
            scene,
            "player-anchor",
            TRANSFORM,
            "scale",
            vec2_value(0.0, 1.0),
        );
        set_scene_field(
            scene,
            "player-anchor",
            TRANSFORM,
            "translation",
            vec2_value(0.0, 32.0),
        );
    });
    let snapshot = project.run(
        1,
        [
            (1, movement(Direction::Right, true)),
            (1, movement(Direction::Up, true)),
        ],
    );
    assert_eq!(snapshot.player, Vec2::new(0.0, 32.0));
}

#[test]
fn nested_singular_parent_does_not_turn_roundoff_into_movement() {
    let project = SourceProject::new(|scene| nest_player_anchor(scene, 0.0, 0.3, 0.4));
    let start = project.run(1, []).player;
    assert!(start.distance(Vec2::new(-105.59078, -32.663055)) < 0.01);
    let moved = project.run(1, [(1, movement(Direction::Right, true))]);
    assert_eq!(moved.player, start);
}

#[test]
fn parented_movement_requires_a_representable_local_landing() {
    for (angle, expected_start_x, expected_step_y) in [
        (0.0, -120.0, 4.0),
        (std::f64::consts::FRAC_PI_4, -84.852_81, 0.0),
    ] {
        let project = SourceProject::new(|scene| {
            nest_player_anchor(scene, 1e-8, 0.0, angle);
            set_narrow_pickup(scene, "gem-left", -90.0, 2.0);
        });
        let start = project.run(1, []).player;
        assert!(start.distance(Vec2::new(expected_start_x, 0.0)) < 0.001);
        let moved = project.run(1, [(1, movement(Direction::Up, true))]);
        assert_eq!(
            moved.collected, 0,
            "scene angle {angle}: {:?}",
            moved.player
        );
        assert!(
            moved
                .player
                .distance(start + Vec2::new(0.0, expected_step_y))
                < 0.001,
            "scene angle {angle}: {:?} → {:?}",
            start,
            moved.player
        );
    }
}

#[test]
fn a_gem_carried_by_the_player_does_not_cross_its_pickup_circle() {
    for (distance, expected) in [(-2.0, 0), (-0.5, 1)] {
        let mut carried = None;
        let project = SourceProject::new(|scene| {
            set_narrow_pickup(scene, "gem-left", distance, 0.0);
            let index = scene
                .entities
                .iter()
                .position(|entity| entity.id.as_str() == "gem-left")
                .unwrap();
            carried = Some(scene.entities.remove(index));
            scene
                .placements
                .retain(|placement| placement.target != source_target("gem-left"));
        });
        project.edit_player_prefab(|prefab| {
            prefab.entities.push(carried.unwrap());
            prefab.placements.push(SceneEntityPlacement::new(
                source_target("gem-left"),
                ScenePlacement::last(ScenePlacementParent::Entity(source_target("player"))),
            ));
        });
        let moved = project.run(1, [(1, movement(Direction::Right, true))]);
        assert_eq!(moved.player, Vec2::new(-116.0, 0.0));
        assert_eq!(moved.collected, expected);
        assert_eq!(moved.visible_targets, 3 - expected);
    }
}

#[test]
fn movement_collects_all_three_authored_targets_and_changes_the_background() {
    let initial = run(1, Vec::new());
    assert_eq!(initial.player, nara::core::Vec2::new(-120.0, 0.0));
    assert_eq!(initial.visible_targets, 3);
    assert_eq!(initial.collected, 0);
    assert!(!initial.won);

    let completed = run(62, demonstration_commands(false));
    assert!(completed.player.x > initial.player.x);
    assert_eq!(completed.collected, 3);
    assert_eq!(completed.target_count, 3);
    assert_eq!(completed.visible_targets, 0);
    assert!(completed.won);
    assert!(completed.victory_color);
}

#[test]
fn reset_restores_positions_targets_and_color_without_removing_authored_entities() {
    let initial = run(1, Vec::new());
    let reset = run(66, demonstration_commands(true));
    assert_eq!(reset.player, initial.player);
    assert_eq!(reset.target_count, initial.target_count);
    assert_eq!(reset.visible_targets, initial.visible_targets);
    assert_eq!(reset.collected, 0);
    assert_eq!(reset.resets, 1);
    assert!(!reset.won);
    assert!(!reset.victory_color);
}

#[test]
fn releasing_a_direction_stops_movement() {
    let commands = [(1, true), (6, false)]
        .into_iter()
        .enumerate()
        .map(|(index, (tick, pressed))| {
            GameplayCommandSubmission::new(
                GameplayCommandTick::new(tick).unwrap(),
                GameplayCommandIngressSource::test("sample.release").unwrap(),
                GameplayCommandSourceSequence::new(index as u64 + 1).unwrap(),
                movement(Direction::Right, pressed),
            )
        })
        .collect();
    let stopped = run(10, commands);
    assert!((stopped.player.x - (-100.0)).abs() < 0.001);
    assert_eq!(stopped.collected, 0);
}
