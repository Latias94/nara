use std::{ffi::OsString, fs, num::NonZeroUsize, path::Path, time::Duration};

use nara::{
    project_tooling::{cargo_build::CargoBuildTools, process::SupervisedProcessCommand},
    project_workflow::cargo::CargoBuildSelection,
    runtime_content::RuntimeContentError,
};
use serde_json::{Value, json};
use two_level_game::{
    levels::{LevelId, LevelKey},
    save::{CURRENT_FORMAT_VERSION, Checkpoint, MAX_CHECKPOINT_BYTES, decode_checkpoint},
};

use crate::{
    continuity::Fixture,
    cooked_delivery::{
        native::{Custody, Output, assert_sources_gone, read_bounded, remove_owned_tree},
        record,
    },
};

mod checkpoint_handoff;
mod continuation;
mod graph;
mod package;
mod service;

const GAME: &str = "two-level-game";
const BIN: &str = "server";
const FEATURE: &str = "cooked-server";
const PROFILE: &str = "release";
const CONTENT_PROFILE: &str = "two-level-server-shipping";
const INSTALLATION: &str = "TwoLevelServer";
const RESULT: &str = "TWO_LEVEL_SERVER_RESULT ";

pub(super) fn accept(cooker: &Path) {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let mut custody = Custody::new(Fixture::new());
    let fixture = custody.fixture();
    let target = fixture.repository.join("target").canonicalize().unwrap();
    let tools = CargoBuildTools::discover_in(&fixture.repository).unwrap();
    let triple = fixture.request().selection().target_triple().to_owned();
    let selection = CargoBuildSelection::new(GAME, BIN, PROFILE, &triple, [FEATURE], false)
        .expect("the explicit shipping selection");
    let lock_path = fixture.project.join("Cargo.lock");
    let lock = read_bounded(&lock_path, 1024 * 1024);
    let output_parent = fixture.directory("cook-output");
    let command = SupervisedProcessCommand::new(cooker, &fixture.root)
        .arguments([
            OsString::from("--runtime"),
            OsString::from(BIN),
            OsString::from("--project-root"),
            fixture.project.as_os_str().to_owned(),
            OsString::from("--output-parent"),
            output_parent.as_os_str().to_owned(),
            OsString::from("--name"),
            OsString::from(INSTALLATION),
            OsString::from("--target-dir"),
            target.as_os_str().to_owned(),
            OsString::from("--target"),
            OsString::from(&triple),
            OsString::from("--cargo"),
            tools.cargo_path().as_os_str().to_owned(),
            OsString::from("--rustc"),
            tools.rustc_path().as_os_str().to_owned(),
            OsString::from("--profile"),
            OsString::from(PROFILE),
        ])
        .environment("CARGO_BUILD_JOBS", "1")
        .environment("CARGO_NET_OFFLINE", "true")
        .environment("CARGO_TARGET_DIR", &target)
        .environment("RUSTUP_TOOLCHAIN", "1.95.0")
        .environment("RUSTUP_AUTO_INSTALL", "0")
        .environment("CARGO", tools.cargo_path())
        .environment("RUSTC", tools.rustc_path())
        .bounded_stdout(NonZeroUsize::new(2 * 1024 * 1024).unwrap());
    let (tree, graph_before) = graph::audit(&mut custody, &tools, &target, &selection);
    let cooked = custody.tool(&command, Duration::from_secs(30 * 60));
    assert_eq!(
        cooked.receipt.exit().code(),
        Some(0),
        "server Cook failed\nstdout: {}\nstderr: {}",
        cooked.stdout,
        cooked.stderr
    );
    assert_eq!(
        graph::read(&mut custody, &tree),
        graph_before,
        "the locked server normal/build Cargo graph changed across Cook"
    );
    assert!(
        read_bounded(&lock_path, 1024 * 1024) == lock,
        "the copied sample Cargo.lock changed across Cook and the graph probes"
    );
    let publication: Value = record(&cooked.stdout, "TWO_LEVEL_PACKAGE_PUBLISHED ").unwrap();
    let published = output_parent.join(INSTALLATION);
    let executable_name = format!("{BIN}{}", std::env::consts::EXE_SUFFIX);
    assert_eq!(publication["runtime"], BIN);
    assert_eq!(publication["executable"], executable_name);
    assert_eq!(publication["postpublication_error"], Value::Null);
    assert_eq!(
        Path::new(publication["directory"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        published.canonicalize().unwrap()
    );

    let package = package::inspect(&published, &executable_name, &selection, &lock);

    let fixture = custody.fixture();
    let relocated = fixture
        .directory("relocated-installation")
        .join(INSTALLATION);
    fs::rename(&published, &relocated).unwrap();
    assert!(!published.exists());
    package.assert_unchanged(&relocated);
    let mismatched = fixture.directory("wrong-server-content");
    package.install_mismatched(&relocated, &mismatched);

    let mut candidate = Custody::new(Fixture::new());
    let candidate_project = candidate.fixture().project.clone();
    let candidate_cache = candidate
        .fixture()
        .root
        .join("cook-output/two-level-cook-cache");
    let candidate_installation = candidate.fixture().directory("relocated-installation");
    for name in [executable_name.as_str(), "content.pack", "package.json"] {
        fs::copy(relocated.join(name), candidate_installation.join(name)).unwrap();
    }
    package.assert_unchanged(&candidate_installation);
    remove_owned_tree(candidate.fixture(), &candidate_project, &target);
    assert_sources_gone(&candidate_project, &candidate_cache);

    let project = fixture.project.clone();
    let cache = output_parent
        .join("two-level-cook-cache")
        .canonicalize()
        .unwrap();
    remove_owned_tree(fixture, &project, &target);
    remove_owned_tree(fixture, &cache, &target);
    assert_sources_gone(&project, &cache);

    let server = relocated.join(&executable_name);
    let travel = custody.player(&server, vec!["--verify-travel".into()], "server-travel");
    let travel = assert_success(&travel, "travel", LevelId::Meadow, 30, &[1]);
    assert_eq!(travel["state"]["travel_count"], 4, "{travel}");
    assert_eq!(travel["state"]["restarts"], 1, "{travel}");

    let regions = custody.player(&server, vec!["--verify-regions".into()], "server-regions");
    let regions = assert_success(&regions, "regions", LevelId::Cavern, 0, &[]);
    assert_regions(&regions);

    let save_parent = custody.fixture().directory("server-checkpoint");
    let checkpoint_path = save_parent.join("checkpoint.json");
    let saved = custody.player(
        &server,
        vec![
            "--verify-save".into(),
            checkpoint_path.as_os_str().to_owned(),
        ],
        "server-save",
    );
    let saved = assert_success(&saved, "save", LevelId::Cavern, 30, &[2]);
    assert_eq!(saved["state"]["saves"], 1, "{saved}");
    let saved_bytes = read_bounded(&checkpoint_path, MAX_CHECKPOINT_BYTES);
    let decoded = decode_checkpoint(&saved_bytes).unwrap();
    assert!(decoded.migrated_from.is_none());
    let checkpoint = decoded.checkpoint;
    assert_eq!(checkpoint.location.level, LevelKey::main(LevelId::Cavern));
    assert_eq!(checkpoint.score, 30);
    assert_eq!(checkpoint.collected.len(), 3);
    assert_eq!(checkpoint.markers.len(), 3);
    assert_eq!(checkpoint.next_marker_id.get(), 4);
    assert_checkpoint_receipt(&saved, "save", &checkpoint_path, &checkpoint);

    let restored = custody.player(
        &server,
        vec![
            "--verify-restore".into(),
            checkpoint_path.as_os_str().to_owned(),
        ],
        "server-restore",
    );
    let restored = assert_success(&restored, "restore", LevelId::Meadow, 30, &[1]);
    assert_checkpoint_receipt(&restored, "restore", &checkpoint_path, &checkpoint);
    assert_eq!(restored["state"]["restores"], 1);
    let restored_state = &restored["checkpoint"]["restored_state"];
    assert_eq!(restored_state["restores"], 1, "{restored}");
    assert_eq!(
        restored_state["level"],
        json!(checkpoint.location.level),
        "{restored}"
    );
    assert_eq!(restored_state["score"], checkpoint.score, "{restored}");
    assert_eq!(restored_state["markers"], json!([2]), "{restored}");
    assert!(positive_tick(restored_state) < positive_tick(&restored["state"]));
    assert_eq!(
        read_bounded(&checkpoint_path, MAX_CHECKPOINT_BYTES),
        saved_bytes
    );

    let refused = custody.player(
        &mismatched.join(&executable_name),
        vec!["--verify-travel".into()],
        "server-refusal",
    );
    refused.assert_player_exit(false);
    let rejected: Value = record(&refused.stdout, RESULT).unwrap();
    assert_eq!(rejected["status"], "rejected", "{rejected}");
    assert_eq!(rejected["success"], false, "{rejected}");
    assert_eq!(rejected["scenario"], "travel", "{rejected}");
    assert_eq!(rejected["stage"], "open_pack", "{rejected}");
    assert_eq!(rejected["ready"], false, "{rejected}");
    assert_eq!(rejected["initialized"], false, "{rejected}");
    assert_eq!(rejected["close"], "not_started", "{rejected}");
    assert_eq!(rejected["cleanup_complete"], true, "{rejected}");
    assert!(rejected["state"].is_null(), "{rejected}");
    assert_eq!(rejected["visited_levels"], json!([]), "{rejected}");
    let message = RuntimeContentError::CatalogDigestMismatch.to_string();
    assert_eq!(rejected["message"], message, "{rejected}");
    assert!(refused.stderr.contains(&message), "{}", refused.stderr);

    service::accept(&mut custody, &server);
    let candidate_server = candidate_installation.join(&executable_name);
    continuation::accept(&mut custody, &server, &mut candidate, &candidate_server);
    continuation::accept_open_rooms(&mut custody, &server, &mut candidate, &candidate_server);
    checkpoint_handoff::accept(&mut custody, &server, &mut candidate, &candidate_server);
    package.assert_unchanged(&relocated);
    package.assert_unchanged(&candidate_installation);
    assert_sources_gone(&project, &cache);
    assert_sources_gone(&candidate_project, &candidate_cache);
    println!(concat!(
        "TWO_LEVEL_SERVER_DELIVERY travel=complete regions=complete save=complete ",
        "fresh_restore=complete wrong_content=open_pack_refused native_cleanup=normal"
    ));
}

fn assert_success(
    output: &Output,
    scenario: &str,
    level: LevelId,
    score: u32,
    markers: &[u64],
) -> Value {
    output.assert_player_exit(true);
    let result: Value = record(&output.stdout, RESULT).unwrap();
    assert_complete(&result, scenario);
    assert_eq!(result["visited_levels"], json!(LevelId::ALL), "{result}");
    let state = &result["state"];
    positive_tick(state);
    assert_eq!(state["phase"], "Running", "{result}");
    assert_eq!(state["level"], json!(LevelKey::main(level)), "{result}");
    assert_eq!(state["score"], score, "{result}");
    assert_eq!(state["markers"], json!(markers), "{result}");
    result
}

fn assert_complete(result: &Value, scenario: &str) {
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["success"], true, "{result}");
    assert_eq!(result["scenario"], scenario, "{result}");
    assert_eq!(result["stage"], "complete", "{result}");
    assert_eq!(result["ready"], true, "{result}");
    assert_eq!(result["source_free"], true, "{result}");
    assert_eq!(result["processed_runtime"], true, "{result}");
    assert_eq!(result["initialized"], true, "{result}");
    assert_eq!(result["close"], "complete", "{result}");
    assert_eq!(result["cleanup_complete"], true, "{result}");
}

fn positive_tick(state: &Value) -> u64 {
    state["tick"]
        .as_u64()
        .filter(|tick| *tick > 0)
        .unwrap_or_else(|| panic!("expected an observed fixed tick: {state}"))
}

fn assert_regions(result: &Value) {
    let state = &result["state"];
    let evidence = &result["server_evidence"];
    assert_eq!(evidence["source"], "two-level-game.server", "{result}");
    assert_eq!(evidence["commands_consumed"], 11, "{result}");
    assert_eq!(evidence["services_absent"], true, "{result}");
    for room in ["side_room", "lookout"] {
        assert_eq!(evidence["rooms"][room]["phase"], "relic_found", "{result}");
        assert_eq!(evidence["rooms"][room]["closed"], true, "{result}");
        assert_eq!(state[room], "closed", "{result}");
    }
    assert_eq!(state["travel_count"], 1, "{result}");
    let collision = &evidence["collision"];
    assert_eq!(collision["blocked"], true, "{result}");
    assert_eq!(collision["wall"], "wall-west", "{result}");
    assert!(
        collision["contact_x"]
            .as_f64()
            .is_some_and(|x| (-280.1..=-279.8).contains(&x)),
        "{result}"
    );
    assert!(
        collision["held_tick"]
            .as_u64()
            .is_some_and(|tick| tick > 0 && tick <= positive_tick(state)),
        "{result}"
    );
}

fn assert_checkpoint_receipt(
    result: &Value,
    operation: &str,
    path: &Path,
    checkpoint: &Checkpoint,
) {
    let receipt = &result["checkpoint"];
    assert_eq!(receipt["operation"], operation, "{result}");
    assert_eq!(Path::new(receipt["path"].as_str().unwrap()), path);
    assert_eq!(
        receipt["format_version"], CURRENT_FORMAT_VERSION,
        "{result}"
    );
    assert!(receipt["migrated_from"].is_null(), "{result}");
    assert_eq!(receipt["score"], checkpoint.score, "{result}");
    assert_eq!(
        receipt["level"],
        json!(checkpoint.location.level),
        "{result}"
    );
    assert_eq!(receipt["markers"], json!(checkpoint.markers), "{result}");
    assert_eq!(receipt["collected"], checkpoint.collected.len(), "{result}");
    assert_eq!(
        receipt["next_marker_id"],
        checkpoint.next_marker_id.get(),
        "{result}"
    );
}
