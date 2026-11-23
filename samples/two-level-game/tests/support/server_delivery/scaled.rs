use std::{fs, path::Path, time::Duration};

use nara::project_tooling::cargo_build::CargoBuildTools;
use serde_json::{Value, json};
use two_level_game::{
    levels::{LevelId, LevelKey},
    resume::processed::MAX_PROCESSED_RESUME_BYTES,
};

use crate::cooked_delivery::{inspect_pack, native::assert_payload};

use super::{
    BIN, Custody, Fixture, assert_regions, assert_sources_gone, assert_success, cook_command,
    read_bounded, record, remove_owned_tree,
    service::{
        self, Launch, Service, TIMESTEP_NANOS, action, assert_progress, number, request, tick,
    },
};

const HALF_SPEED_TICK_REAL_NANOS: u64 = 2 * TIMESTEP_NANOS;

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

    let [
        half,
        double,
        zero,
        long_tick,
        slow_checkpoint,
        fast,
        paused,
        fast_edited,
        hz240,
        hz480,
    ] = [
        (0.5, "TwoLevelServerHalf", ""),
        (2.0, "TwoLevelServerDouble", ""),
        (0.0, "TwoLevelServerZero", ""),
        (
            1.0,
            "TwoLevelServerLongTick",
            "fixed_timestep_seconds = 20.0\nmax_delta_seconds = 60.0\n",
        ),
        (
            0.25,
            "TwoLevelServerSlowCheckpoint",
            "fixed_timestep_seconds = 20.0\nmax_delta_seconds = 60.0\n",
        ),
        (8.0, "TwoLevelServerFast", ""),
        (1.0, "TwoLevelServerPaused", "paused = true\n"),
        (8.0, "TwoLevelServerFastEdited", "paused = true\n"),
        (
            1.0,
            "TwoLevelServer240Hz",
            "fixed_timestep_seconds = 0.004166666666666667\n",
        ),
        (
            1.0,
            "TwoLevelServer480Hz",
            "fixed_timestep_seconds = 0.002083333333333333\n",
        ),
    ]
    .map(|(scale, name, timing)| {
        custody.fixture().write(
            "nara.toml",
            &format!("{manifest}\n[profiles.server.runtime]\ntime_scale = {scale:.2}\n{timing}"),
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
        let fixed_timestep = match name {
            "TwoLevelServerLongTick" | "TwoLevelServerSlowCheckpoint" => {
                Some(json!({"secs": 20, "nanos": 0}))
            }
            "TwoLevelServer240Hz" => Some(json!({"secs": 0, "nanos": 4_166_667})),
            "TwoLevelServer480Hz" => Some(json!({"secs": 0, "nanos": 2_083_333})),
            _ => None,
        };
        if let Some(timestep) = fixed_timestep {
            assert_eq!(
                catalog["payload"]["configuration"]["runtime"]["fixed_timestep"], timestep,
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

    service::assert_long_tick_resume_control(&mut custody, &long_tick);
    service::assert_running_slow_checkpoint(&mut custody, &slow_checkpoint);
    let uninterrupted = assert_half_speed_partitions(&mut custody, &half);
    assert_half_speed_continuation(&mut custody, &half, &target, &uninterrupted);

    for (server, name) in [
        (&half, "server-regions-half"),
        (&double, "server-regions-double"),
        (&fast, "server-regions-fast"),
        (&paused, "server-regions-paused"),
        (&hz240, "server-regions-240hz"),
        (&hz480, "server-regions-480hz"),
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

fn assert_half_speed_partitions(custody: &mut Custody, executable: &Path) -> Value {
    let uninterrupted = run_half_speed_partition(
        custody,
        executable,
        "server-half-unsplit",
        &[HALF_SPEED_TICK_REAL_NANOS],
    );
    let partitions: [(&str, &[u64]); 3] = [
        ("server-half-even-frames", &[10_000_000; 4]),
        (
            "server-half-60hz-frames",
            &[16_666_667, 16_666_667, 6_666_666],
        ),
        (
            "server-half-odd-frames",
            &[9_999_999, 9_999_999, 9_999_999, 10_000_003],
        ),
    ];
    for (name, intervals) in partitions {
        let partitioned = run_half_speed_partition(custody, executable, name, intervals);
        assert_eq!(partitioned["state"], uninterrupted["state"], "{name}");
        assert_eq!(partitioned["commands"], uninterrupted["commands"], "{name}");
    }
    uninterrupted
}

fn run_half_speed_partition(
    custody: &mut Custody,
    executable: &Path,
    name: &str,
    intervals: &[u64],
) -> Value {
    let mut server = Service::spawn_scaled(custody, executable, Launch::Paused(None), name);
    server.ready(true, false);
    assert_eq!(server.admit(&action("marker")), 1, "{name}");
    let mut observed = server.observe();
    assert_half_speed_progress(&observed, 0);
    let mut elapsed = 0;
    for &interval in intervals {
        elapsed += interval;
        observed = server.advance(interval);
        assert_half_speed_progress(&observed, elapsed);
    }
    assert_eq!(elapsed, HALF_SPEED_TICK_REAL_NANOS, "{name}");
    server.send(&request("shutdown"));
    assert_paused_close(&server.finish("shutdown", None), &observed);
    observed
}

fn assert_half_speed_continuation(
    custody: &mut Custody,
    executable: &Path,
    target: &Path,
    uninterrupted: &Value,
) {
    let mut candidate_custody = Custody::new(Fixture::new());
    let fixture = candidate_custody.fixture();
    let resume_path = fixture
        .directory("server-half-resume-state")
        .join("resume.json");
    remove_owned_tree(fixture, &fixture.project, target);
    assert_sources_gone(
        &fixture.project,
        &fixture.root.join("cook-output/two-level-cook-cache"),
    );

    let mut incumbent = Service::spawn_scaled(
        custody,
        executable,
        Launch::Paused(None),
        "server-half-incumbent",
    );
    incumbent.ready(true, false);
    let due = incumbent.admit(&action("marker"));
    assert_eq!(due, 1);
    let boundary = incumbent.advance(HALF_SPEED_TICK_REAL_NANOS - 1);
    assert_half_speed_progress(&boundary, HALF_SPEED_TICK_REAL_NANOS - 1);

    let captured = incumbent.exchange(&request("capture"), "captured")["state"].take();
    assert_eq!(captured["format_version"], 4, "{captured}");
    let payload = &captured["payload"];
    let time = &payload["game"]["runtime_time"];
    let remainder = TIMESTEP_NANOS - 1;
    assert_eq!(time["pending_nanos"], remainder, "{captured}");
    assert_eq!(time["time_scale"], json!(0.5), "{captured}");
    assert_eq!(
        time["scaled_remainder"],
        json!([0, 0, 1 << 20]),
        "{captured}"
    );
    let offset = due - tick(&boundary["state"]);
    assert_eq!(
        payload["pending"],
        json!([{"tick_offset": offset, "command": {"gameplay": {"operation": "marker"}}}]),
        "{captured}"
    );
    let bytes = serde_json::to_vec(&captured).unwrap();
    fs::write(&resume_path, &bytes).unwrap();

    let mut candidate = Service::spawn_scaled(
        &mut candidate_custody,
        executable,
        Launch::Paused(Some(&resume_path)),
        "server-half-candidate",
    );
    let ready = candidate.ready(true, false);
    let restored_tick = tick(&ready["state"]);
    assert_eq!(restored_tick, 1, "{ready}");
    candidate.expect_restored_pending(&[offset]);
    let restored = candidate.observe();
    for field in ["state", "commands", "time"] {
        assert_eq!(restored[field], ready[field], "{restored}");
    }
    assert_same_game(&restored["state"], &boundary["state"]);
    assert_half_speed_phase(&restored, 0, remainder);
    let recaptured = candidate.exchange(&request("capture"), "captured");
    assert_eq!(recaptured["state"], captured, "{recaptured}");

    incumbent.send(&request("retire"));
    assert_paused_close(&incumbent.finish("retired", None), &boundary);

    let zero = candidate.advance(0);
    assert_half_speed_phase(&zero, 0, remainder);
    assert_eq!(zero["state"], restored["state"], "{zero}");
    assert_eq!(zero["commands"], restored["commands"], "{zero}");

    let settled = candidate.advance(1);
    assert_half_speed_phase(&settled, 1, 0);
    assert_eq!(tick(&settled["state"]), restored_tick + offset, "{settled}");
    assert_same_game(&settled["state"], &uninterrupted["state"]);
    assert_eq!(
        number(&boundary["time"], "elapsed_nanos") + number(&settled["time"], "elapsed_nanos"),
        number(&uninterrupted["time"], "elapsed_nanos"),
        "{boundary}\n{settled}"
    );
    candidate.send(&request("shutdown"));
    assert_paused_close(&candidate.finish("shutdown", None), &settled);
    assert_eq!(
        read_bounded(&resume_path, MAX_PROCESSED_RESUME_BYTES),
        bytes
    );
}

fn assert_half_speed_progress(observed: &Value, elapsed: u64) {
    let expected_tick = elapsed / HALF_SPEED_TICK_REAL_NANOS;
    let state = &observed["state"];
    assert_eq!(tick(state), expected_tick, "{observed}");
    assert_eq!(number(state, "logical_tick"), expected_tick, "{observed}");
    let markers: &[u64] = if expected_tick == 0 { &[] } else { &[1] };
    assert_progress(state, LevelKey::main(LevelId::Meadow), 0, markers);
    assert_half_speed_phase(observed, elapsed, elapsed / 2 % TIMESTEP_NANOS);
}

fn assert_half_speed_phase(observed: &Value, elapsed: u64, remainder: u64) {
    assert_eq!(observed["paused"], true, "{observed}");
    let time = &observed["time"];
    assert_eq!(time["elapsed_nanos"], elapsed, "{observed}");
    assert_eq!(time["fixed_debt_nanos"], 0, "{observed}");
    assert_eq!(time["fixed_remainder_nanos"], remainder, "{observed}");
    assert_eq!(observed["gameplay_rejections"]["count"], 0, "{observed}");
}

fn assert_same_game(observed: &Value, expected: &Value) {
    for (field, value) in expected.as_object().unwrap() {
        if field != "tick" {
            assert_eq!(&observed[field], value, "{field}: {observed}\n{expected}");
        }
    }
}

fn assert_paused_close(result: &Value, observed: &Value) {
    assert_eq!(result["state"], observed["state"], "{result}");
    for field in ["paused", "commands", "time", "gameplay_rejections"] {
        assert_eq!(
            result["server_evidence"][field], observed[field],
            "{result}"
        );
    }
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
