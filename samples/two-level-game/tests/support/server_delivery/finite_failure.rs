use std::{fs, path::Path, time::Duration};

use nara::{identity::SceneEntityId, project_tooling::cargo_build::CargoBuildTools};
use serde_json::{Value, json};
use two_level_game::{
    levels::{LevelId, LevelKey},
    save::{
        Checkpoint, CheckpointLocation, CollectedPickup, MAX_CHECKPOINT_BYTES, encode_checkpoint,
    },
};

use crate::cooked_delivery::native::assert_payload;

use super::{
    BIN, Custody, Fixture, INSTALLATION, RESULT, assert_sources_gone, assert_success, cook_command,
    positive_tick, read_bounded, record, remove_owned_tree,
};

pub(crate) fn accept(cooker: &Path) {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let mut custody = Custody::new(Fixture::new());
    let fixture = custody.fixture();
    let cavern_members = add_cavern_waypoint(fixture);
    let target = fixture.repository.join("target").canonicalize().unwrap();
    let tools = CargoBuildTools::discover_in(&fixture.repository).unwrap();
    let triple = fixture.request().selection().target_triple().to_owned();
    let output_parent = fixture.directory("cook-output");
    let command = cook_command(cooker, fixture, &tools, &target, &triple, &output_parent)
        .arguments(["--name", INSTALLATION]);

    let cooked = custody.tool(&command, Duration::from_secs(30 * 60));
    assert_eq!(
        cooked.receipt.exit().code(),
        Some(0),
        "server Cook failed\nstdout: {}\nstderr: {}",
        cooked.stdout,
        cooked.stderr
    );
    let publication: Value = record(&cooked.stdout, "TWO_LEVEL_PACKAGE_PUBLISHED ").unwrap();
    let executable = format!("{BIN}{}", std::env::consts::EXE_SUFFIX);
    let published = output_parent.join(INSTALLATION);
    assert_eq!(publication["runtime"], BIN, "{publication}");
    assert_eq!(publication["executable"], executable, "{publication}");
    assert_eq!(
        publication["postpublication_error"],
        Value::Null,
        "{publication}"
    );
    assert_eq!(
        Path::new(publication["directory"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        published.canonicalize().unwrap()
    );

    let fixture = custody.fixture();
    let relocated = fixture
        .directory("relocated-installation")
        .join(INSTALLATION);
    fs::rename(&published, &relocated).unwrap();
    assert!(!published.exists());
    assert_payload(&relocated, &executable);

    let project = fixture.project.clone();
    let cache = output_parent
        .join("two-level-cook-cache")
        .canonicalize()
        .unwrap();
    remove_owned_tree(fixture, &project, &target);
    remove_owned_tree(fixture, &cache, &target);
    assert_sources_gone(&project, &cache);

    let checkpoint_path = fixture
        .directory("checkpoint-input")
        .join("checkpoint.json");
    let cavern = LevelKey::main(LevelId::Cavern);
    let mut checkpoint = Checkpoint::new(CheckpointLocation {
        level: cavern,
        anchor: SceneEntityId::new("checkpoint").unwrap(),
    });
    checkpoint.score = 10;
    checkpoint.collected.push(CollectedPickup {
        level: cavern,
        entity: SceneEntityId::new("gem-near").unwrap(),
    });
    let checkpoint_bytes = encode_checkpoint(&checkpoint).unwrap();
    fs::write(&checkpoint_path, &checkpoint_bytes).unwrap();

    let server = relocated.join(&executable);
    let refused = custody.player(
        &server,
        vec![
            "--verify-restore".into(),
            checkpoint_path.as_os_str().to_owned(),
        ],
        "server-finite-refusal",
    );
    refused.assert_player_exit(false);
    let result: Value = record(&refused.stdout, RESULT).unwrap();
    assert_eq!(result["status"], "rejected", "{result}");
    assert_eq!(result["success"], false, "{result}");
    assert_eq!(result["scenario"], "restore", "{result}");
    assert_eq!(result["stage"], "scenario", "{result}");
    assert_eq!(result["ready"], true, "{result}");
    assert_eq!(result["initialized"], true, "{result}");
    assert_eq!(result["source_free"], true, "{result}");
    assert_eq!(result["processed_runtime"], true, "{result}");
    assert_eq!(result["close"], "complete", "{result}");
    assert_eq!(result["cleanup_complete"], true, "{result}");
    assert_eq!(
        result["visited_levels"],
        json!([LevelId::Meadow, LevelId::Cavern]),
        "{result}"
    );
    assert!(result["checkpoint"].is_null(), "{result}");
    assert!(result["server_evidence"].is_null(), "{result}");

    let state = &result["state"];
    positive_tick(state);
    assert_eq!(state["phase"], "Running", "{result}");
    assert_eq!(state["level"], json!(cavern), "{result}");
    assert_eq!(state["score"], 10, "{result}");
    assert_eq!(state["collected"], 1, "{result}");
    assert_eq!(state["visible_pickups"], 1, "{result}");
    assert_eq!(state["scene_members"], cavern_members, "{result}");
    assert_eq!(state["travel_count"], 1, "{result}");
    assert_eq!(state["restores"], 1, "{result}");
    assert_eq!(state["verified"], false, "{result}");
    let message = result["message"].as_str().unwrap();
    let gameplay_error = state["error"].as_str().unwrap();
    assert!(
        gameplay_error.starts_with("level reconstruction disagreed with saved logical facts: "),
        "{result}"
    );
    assert_eq!(
        message.strip_prefix("gameplay rejected the scenario: "),
        Some(gameplay_error),
        "{result}"
    );
    assert!(refused.stderr.contains(message), "{}", refused.stderr);
    assert_eq!(
        read_bounded(&checkpoint_path, MAX_CHECKPOINT_BYTES),
        checkpoint_bytes
    );

    let regions = custody.player(
        &server,
        vec!["--verify-regions".into()],
        "server-finite-control",
    );
    let regions = assert_success(&regions, "regions", LevelId::Cavern, 0, &[]);
    assert_eq!(
        regions["state"]["scene_members"], cavern_members,
        "{regions}"
    );
    assert_eq!(regions["state"]["visible_pickups"], 2, "{regions}");
    assert_sources_gone(&project, &cache);
}

fn add_cavern_waypoint(fixture: &Fixture) -> usize {
    let path = "scenes/cavern.scene.json";
    let mut scene: Value = serde_json::from_str(&fixture.read(path)).unwrap();
    let entities = scene["payload"]["entities"].as_array_mut().unwrap();
    let gem = entities
        .iter()
        .find(|entity| entity["id"] == "gem-near")
        .unwrap();
    let transform = gem["components"]["nara.transform.Transform2d"].clone();
    entities.push(json!({
        "id": "cavern-waypoint",
        "components": { "nara.transform.Transform2d": transform },
    }));
    let members = entities.len();
    scene["payload"]["placements"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "target": {
                "through": [],
                "entity": { "kind": "source_entity", "entity": "cavern-waypoint" },
            },
            "placement": { "parent": { "kind": "root" }, "slot": { "kind": "last" } },
        }));
    fixture.write(path, &serde_json::to_string(&scene).unwrap());
    members
}
