use std::{fs, path::Path, time::Duration};

use nara::project_tooling::cargo_build::CargoBuildTools;
use serde_json::{Value, json};
use two_level_game::levels::LevelId;

use crate::cooked_delivery::{inspect_pack, native::assert_payload};

use super::{
    BIN, Custody, Fixture, assert_regions, assert_sources_gone, assert_success, cook_command,
    read_bounded, record, remove_owned_tree, service,
};

pub(crate) fn accept(cooker: &Path) {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let mut custody = Custody::new(Fixture::new());
    let fixture = custody.fixture();
    let target = fixture.repository.join("target").canonicalize().unwrap();
    let tools = CargoBuildTools::discover_in(&fixture.repository).unwrap();
    let triple = fixture.request().selection().target_triple().to_owned();
    let output_parent = fixture.directory("cook-output");
    let relocated = fixture.directory("relocated-installation");
    let manifest = fixture.read("nara.toml");
    let command = cook_command(cooker, fixture, &tools, &target, &triple, &output_parent);
    let executable = format!("{BIN}{}", std::env::consts::EXE_SUFFIX);

    let [half, double, zero, long_tick, fast, paused, fast_edited] = [
        (0.5, "TwoLevelServerHalf", ""),
        (2.0, "TwoLevelServerDouble", ""),
        (0.0, "TwoLevelServerZero", ""),
        (
            1.0,
            "TwoLevelServerLongTick",
            "fixed_timestep_seconds = 20.0\nmax_delta_seconds = 60.0\n",
        ),
        (8.0, "TwoLevelServerFast", ""),
        (1.0, "TwoLevelServerPaused", "paused = true\n"),
        (8.0, "TwoLevelServerFastEdited", "paused = true\n"),
    ]
    .map(|(scale, name, timing)| {
        custody.fixture().write(
            "nara.toml",
            &format!("{manifest}\n[profiles.server.runtime]\ntime_scale = {scale:.1}\n{timing}"),
        );
        if name == "TwoLevelServerFastEdited" {
            add_valid_wall(custody.fixture());
        }
        let cooked = custody.tool(
            &command.clone().arguments(["--name", name]),
            Duration::from_secs(30 * 60),
        );
        assert_eq!(
            cooked.receipt.exit().code(),
            Some(0),
            "{scale}x Server Cook failed\nstdout: {}\nstderr: {}",
            cooked.stdout,
            cooked.stderr
        );
        let publication: Value = record(&cooked.stdout, "TWO_LEVEL_PACKAGE_PUBLISHED ").unwrap();
        assert_eq!(publication["runtime"], BIN, "{publication}");
        assert_eq!(publication["executable"], executable, "{publication}");
        assert_eq!(
            publication["postpublication_error"],
            Value::Null,
            "{publication}"
        );

        let published = output_parent.join(name);
        assert_payload(&published, &executable);
        let pack = read_bounded(&published.join("content.pack"), 32 * 1024 * 1024);
        let catalog = inspect_pack(&pack).unwrap().catalog_bytes;
        let catalog: Value = serde_json::from_slice(&pack[catalog]).unwrap();
        assert_eq!(
            catalog["payload"]["configuration"]["runtime"]["time_scale"].as_f64(),
            Some(scale),
            "{catalog}"
        );
        if name == "TwoLevelServerLongTick" {
            assert_eq!(
                catalog["payload"]["configuration"]["runtime"]["fixed_timestep"],
                json!({"secs": 20, "nanos": 0}),
                "{catalog}"
            );
        }
        assert_eq!(
            catalog["payload"]["configuration"]["runtime"]["paused"],
            matches!(name, "TwoLevelServerPaused" | "TwoLevelServerFastEdited")
        );
        let installed = relocated.join(name);
        fs::rename(&published, &installed).unwrap();
        installed.join(&executable)
    });

    let fixture = custody.fixture();
    let project = &fixture.project;
    let cache = output_parent
        .join("two-level-cook-cache")
        .canonicalize()
        .unwrap();
    remove_owned_tree(fixture, project, &target);
    remove_owned_tree(fixture, &cache, &target);
    assert_sources_gone(project, &cache);

    for (server, name) in [
        (&half, "server-regions-half"),
        (&double, "server-regions-double"),
        (&fast, "server-regions-fast"),
        (&paused, "server-regions-paused"),
    ] {
        let regions = custody.player(server, vec!["--verify-regions".into()], name);
        let regions = assert_success(&regions, "regions", LevelId::Cavern, 0, &[]);
        assert_regions(&regions);
    }
    for (server, name) in [
        (&fast, "server-travel-fast"),
        (&paused, "server-travel-paused"),
    ] {
        let travel = custody.player(server, vec!["--verify-travel".into()], name);
        assert_success(&travel, "travel", LevelId::Meadow, 30, &[1]);
    }
    service::assert_horizon_drain(&mut custody, &half, "server-half-shutdown", false);
    service::assert_horizon_drain(&mut custody, &half, "server-half-eof", true);
    service::assert_horizon_drain(&mut custody, &double, "server-double-shutdown", false);
    service::assert_zero_scale_checkpoint(&mut custody, &zero);
    service::assert_stalled_drain(&mut custody, &zero);
    service::assert_long_tick_drain(&mut custody, &long_tick);
    service::assert_fast_silent_server(&mut custody, &fast);
    service::assert_fast_edited_server(&mut custody, &fast_edited);
}

fn add_valid_wall(fixture: &Fixture) {
    let path = "scenes/meadow.scene.json";
    let mut scene: Value = serde_json::from_str(&fixture.read(path)).unwrap();
    let entities = scene["payload"]["entities"].as_array_mut().unwrap();
    let mut wall = entities
        .iter()
        .find(|entity| entity["id"] == "wall-inner")
        .unwrap()
        .clone();
    wall["id"] = json!("alcove-post");
    let translation = &mut wall["components"]["nara.transform.Transform2d"]["value"]["value"]["translation"]
        ["value"];
    translation["x"]["value"] = json!(110);
    translation["y"]["value"] = json!(90);
    entities.push(wall);
    scene["payload"]["placements"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "target": {"through": [], "entity": {"kind": "source_entity", "entity": "alcove-post"}},
            "placement": {"parent": {"kind": "root"}, "slot": {"kind": "last"}}
        }));
    fixture.write(path, &serde_json::to_string(&scene).unwrap());
}
