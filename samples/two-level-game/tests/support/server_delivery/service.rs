use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use two_level_game::command::CommandKey;

use nara::project_tooling::process::{
    SupervisedBootstrapChannel, SupervisedProcessCleanupReceipt,
    SupervisedProductEnvironmentPolicy, SupervisedProductProcessCommand,
};
use serde_json::{Value, json};
use two_level_game::{
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
    save::{MAX_CHECKPOINT_BYTES, decode_checkpoint},
};

use super::{Custody, RESULT, assert_complete, read_bounded};

const READY: &str = "TWO_LEVEL_SERVER_READY ";
const REPLY: &str = "TWO_LEVEL_SERVER_REPLY ";
const SOURCE: &str = "two-level-game.server";
pub(super) const TIMESTEP_NANOS: u64 = 20_000_000;
const LONG_TIMESTEP_NANOS: u64 = 20_000_000_000;
const POLL: Duration = Duration::from_millis(10);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(65);
const HORIZON_DRAIN_TIMEOUT: Duration = Duration::from_secs(40);
pub(super) const RELEASED: [bool; 4] = [false; 4];
pub(super) const ANCHOR_POSITION: [f32; 2] = [-120.0, 0.0];

pub(super) fn accept(custody: &mut Custody, executable: &Path) {
    let meadow = LevelKey::main(LevelId::Meadow);
    let cavern = LevelKey::main(LevelId::Cavern);
    let instance = LevelKey::new(LevelId::Cavern, 1).unwrap();
    let path = custody
        .fixture()
        .directory("server-service-checkpoint")
        .join("checkpoint.json");
    let mut server = Service::start(custody, executable, Some(&path), "server-service");

    let before = server.observe();
    thread::sleep(Duration::from_nanos(8 * TIMESTEP_NANOS));
    let after = server.observe();
    assert!(
        tick(&after["state"]) >= tick(&before["state"]) + 3,
        "{before}\n{after}"
    );
    assert!(
        number(&after["state"], "logical_tick") > number(&before["state"], "logical_tick"),
        "{before}\n{after}"
    );
    assert!(number(&after["time"], "elapsed_nanos") > number(&before["time"], "elapsed_nanos"));
    assert!(
        number(&after["time"], "frames") >= number(&before["time"], "frames") + 2,
        "{before}\n{after}"
    );
    assert_eq!(after["state"]["held"], json!(RELEASED));
    assert_eq!(player(&after["state"]), player(&before["state"]));

    server.write("{\"request\":\"command\",\n");
    server.reject();
    server.send(&command(json!({
        "operation": "move", "direction": 4, "pressed": true
    })));
    server.reject();
    let refused = server.observe();
    assert_eq!(refused["state"]["held"], json!(RELEASED));
    assert_eq!(player(&refused["state"]), player(&before["state"]));
    assert_progress(&refused["state"], meadow, 0, &[]);

    server.admit(&request("side_room"));
    server.until("the authored Side Room to open", |state| {
        state["side_room"] == "open"
    });
    server.admit(&request("lookout"));
    let open = server.until("both authored rooms to open", |state| {
        state["side_room"] == "open" && state["lookout"] == "open"
    });
    assert_progress(&open, meadow, 0, &[]);
    let meadow_marker = server.collect_and_mark(10, 1);
    assert_eq!(meadow_marker["side_room"], "open");
    assert_eq!(meadow_marker["lookout"], "open");

    server.admit(&action("travel"));
    let travelled = server.until_level(cavern);
    assert_eq!(travelled["travel_count"], 1);
    assert_eq!(travelled["held"], json!(RELEASED));
    assert_eq!(travelled["side_room"], "closed");
    assert_eq!(travelled["lookout"], "closed");
    assert_progress(&travelled, cavern, 10, &[]);
    let cavern_marker = server.collect_and_mark(20, 2);

    server.admit(&action("instance"));
    let instanced = server.until_level(instance);
    assert_eq!(instanced["travel_count"], 2);
    assert_eq!(instanced["visible_pickups"], 2);
    assert_progress(&instanced, instance, 20, &[]);
    let instance_marker = server.collect_and_mark(30, 3);

    server.admit(&action("retry"));
    let retried = server.until("the Cavern instance to restart", |state| {
        state["restarts"] == 1 && state["phase"] == "Running"
    });
    assert_progress(&retried, instance, 30, &[3]);
    assert_eq!(player(&retried), ANCHOR_POSITION);

    assert!(!path.exists());
    server.admit(&request("save"));
    let saved = server.until("the checkpoint to publish", |state| state["saves"] == 1);
    assert_progress(&saved, instance, 30, &[3]);
    let bytes = read_bounded(&path, MAX_CHECKPOINT_BYTES);
    let decoded = decode_checkpoint(&bytes).unwrap();
    assert!(decoded.migrated_from.is_none());
    let checkpoint = decoded.checkpoint;
    assert_eq!(checkpoint.location.level, instance);
    assert_eq!(checkpoint.location.anchor.as_str(), "checkpoint");
    assert_eq!(checkpoint.score, 30);
    let pickups: Vec<_> = checkpoint
        .collected
        .iter()
        .map(|pickup| (pickup.level, pickup.entity.as_str()))
        .collect();
    assert_eq!(
        pickups,
        [
            (meadow, "gem-near"),
            (cavern, "gem-near"),
            (instance, "gem-near")
        ]
    );
    let markers: Vec<_> = checkpoint
        .markers
        .iter()
        .map(|marker| (marker.id.get(), marker.level, marker.position))
        .collect();
    assert_eq!(
        markers,
        [
            (1, meadow, player(&meadow_marker)),
            (2, cavern, player(&cavern_marker)),
            (3, instance, player(&instance_marker)),
        ]
    );
    assert_eq!(checkpoint.next_marker_id.get(), 4);

    server.write(&format!("{}\n{}\n", action("marker"), request("shutdown")));
    server.read_admission();
    let shutdown = server.finish("shutdown", None);
    assert_progress(&shutdown["state"], instance, 30, &[3, 4]);
    assert_eq!(
        shutdown["visited_levels"],
        json!(LevelId::ALL),
        "{shutdown}"
    );
    assert_eq!(shutdown["state"]["saves"], 1);
    assert_eq!(read_bounded(&path, MAX_CHECKPOINT_BYTES), bytes);

    let mut fresh = Service::start(custody, executable, Some(&path), "server-service-restore");
    fresh.admit(&request("restore"));
    let restored = fresh.until("the saved instance to restore", |state| {
        state["restores"] == 1 && state["phase"] == "Running" && state["restoring"] == false
    });
    assert_progress(&restored, instance, 30, &[3]);
    assert_eq!(restored["visible_pickups"], 1);
    assert_eq!(restored["held"], json!(RELEASED));
    assert_eq!(restored["saves"], 0);
    assert_eq!(player(&restored), ANCHOR_POSITION);

    fresh.admit(&request("save"));
    fresh.until("the restored checkpoint to publish", |state| {
        state["saves"] == 1
    });
    assert_eq!(read_bounded(&path, MAX_CHECKPOINT_BYTES), bytes);
    fresh.admit(&action("marker"));
    fresh.close_input();
    let eof = fresh.finish("stdin_eof", None);
    assert_progress(&eof["state"], instance, 30, &[3, 4]);
    assert_eq!(eof["visited_levels"], json!(LevelId::ALL), "{eof}");
    assert_eq!(eof["state"]["restores"], 1);
    assert_eq!(read_bounded(&path, MAX_CHECKPOINT_BYTES), bytes);

    assert_refused_startup(custody, executable);
    assert_terminal_input(custody, executable);
    assert_gameplay_rejection(custody, executable);

    println!(
        "TWO_LEVEL_SERVER_SERVICE_DELIVERY {}",
        json!({
            "idle_before": before,
            "idle_after": after,
            "checkpoint_bytes": bytes.len(),
            "checkpoint": checkpoint,
            "restored_state": restored,
            "shutdown": shutdown["server_evidence"],
            "eof": eof["server_evidence"],
        })
    );
}

pub(super) fn assert_horizon_drain(
    custody: &mut Custody,
    executable: &Path,
    name: &str,
    eof: bool,
) {
    const DUE_TICK: u64 = 600;

    let mut server = Service::spawn(custody, executable, Launch::Paused(None), name);
    server.unit_time_scale = false;
    let ready = server.ready(true, false);
    assert_eq!(tick(&ready["state"]), 0, "{ready}");
    let mut marker = action("marker");
    marker["after_ticks"] = json!(DUE_TICK);
    assert_eq!(server.admit(&marker), DUE_TICK);

    let resume = request("resume");
    let stop = if eof {
        server.send(&resume);
        server.close_input();
        "stdin_eof"
    } else {
        server.write(&format!("{resume}\n{}\n", request("shutdown")));
        "shutdown"
    };
    let resumed = server.reply("resumed");
    assert_eq!(resumed["paused"], false, "{resumed}");
    assert!(tick(&resumed["state"]) < DUE_TICK, "{resumed}");
    assert_eq!(resumed["state"]["markers"], json!([]), "{resumed}");
    server.check_observation(&resumed["state"], &resumed);

    let result = server.finish_with_timeout(stop, None, HORIZON_DRAIN_TIMEOUT);
    assert!(tick(&result["state"]) >= DUE_TICK, "{result}");
    assert_progress(&result["state"], LevelKey::main(LevelId::Meadow), 0, &[1]);
    assert_eq!(
        result["visited_levels"],
        json!([LevelId::Meadow]),
        "{result}"
    );
    print_scaled_service_evidence(name, &result);
}

pub(super) fn assert_stalled_drain(custody: &mut Custody, executable: &Path) {
    let name = "server-zero-stall";
    let mut server = Service::spawn(custody, executable, Launch::Paced(None), name);
    server.unit_time_scale = false;
    let ready = server.ready(false, false);
    assert_eq!(tick(&ready["state"]), 0, "{ready}");
    assert_eq!(server.admit(&action("marker")), 1);
    server.send(&request("shutdown"));

    let result = server.finish_with_timeout(
        "shutdown",
        Some("Server made no progress settling admitted work during shutdown"),
        CONTROL_TIMEOUT,
    );
    assert_eq!(tick(&result["state"]), 0, "{result}");
    assert_progress(&result["state"], LevelKey::main(LevelId::Meadow), 0, &[]);
    assert_eq!(
        result["visited_levels"],
        json!([LevelId::Meadow]),
        "{result}"
    );
    let evidence = &result["server_evidence"];
    assert_eq!(evidence["paused"], false, "{result}");
    assert_eq!(evidence["commands"]["retained"], 1, "{result}");
    let time = &evidence["time"];
    assert!(
        number(time, "frames") > number(&ready["time"], "frames"),
        "{result}"
    );
    assert!(
        number(time, "elapsed_nanos") > number(&ready["time"], "elapsed_nanos"),
        "{result}"
    );
    print_scaled_service_evidence(name, &result);
}

pub(super) fn assert_zero_scale_checkpoint(custody: &mut Custody, executable: &Path) {
    let name = "server-zero-paused-checkpoint";
    let checkpoint = custody
        .fixture()
        .directory("server-zero-checkpoint-save")
        .join("checkpoint.json");
    let mut server = Service::spawn(
        custody,
        executable,
        Launch::PausedCheckpoint {
            save: &checkpoint,
            resume: None,
        },
        name,
    );
    server.unit_time_scale = false;
    let ready = server.ready(true, true);
    assert_eq!(tick(&ready["state"]), 1, "{ready}");
    assert_eq!(ready["commands"]["acknowledged"], 1, "{ready}");
    assert_eq!(ready["commands"]["pending"], 0, "{ready}");
    let observed = server.observe();
    assert_eq!(tick(&observed["state"]), 1, "{observed}");
    server.send(&request("shutdown"));
    let result = server.finish("shutdown", None);
    assert_eq!(tick(&result["state"]), 1, "{result}");
    assert!(!checkpoint.exists());
    print_scaled_service_evidence(name, &result);
}

pub(super) fn assert_running_slow_checkpoint(custody: &mut Custody, executable: &Path) {
    let name = "server-quarter-speed-checkpoint";
    let checkpoint = custody
        .fixture()
        .directory("server-quarter-speed-save")
        .join("checkpoint.json");
    let mut server =
        Service::spawn_scaled(custody, executable, Launch::Paced(Some(&checkpoint)), name);
    server.timestep_nanos = LONG_TIMESTEP_NANOS;
    let ready = server.ready(false, true);
    assert_eq!(tick(&ready["state"]), 1, "{ready}");
    assert_eq!(ready["commands"]["acknowledged"], 1, "{ready}");
    assert_eq!(ready["commands"]["pending"], 0, "{ready}");
    server.send(&request("shutdown"));
    let result = server.finish("shutdown", None);
    assert_eq!(tick(&result["state"]), 1, "{result}");
    assert!(!checkpoint.exists());
    print_scaled_service_evidence(name, &result);
}

pub(super) fn assert_long_tick_resume_control(custody: &mut Custody, executable: &Path) {
    let name = "server-long-tick-resume-pause";
    let mut server = Service::spawn(custody, executable, Launch::Paused(None), name);
    server.timestep_nanos = LONG_TIMESTEP_NANOS;
    let ready = server.ready(true, false);
    assert_eq!(tick(&ready["state"]), 0, "{ready}");
    assert_eq!(server.admit(&action("marker")), 1);
    server.write(&format!("{}\n{}\n", request("resume"), request("pause")));

    let resumed = server.reply("resumed");
    server.check_observation(&resumed["state"], &resumed);
    assert_eq!(resumed["paused"], false, "{resumed}");
    assert_eq!(resumed["time"]["elapsed_nanos"], 0, "{resumed}");
    let paused = server.reply("paused");
    server.check_observation(&paused["state"], &paused);
    assert_eq!(paused["paused"], true, "{paused}");
    assert_eq!(tick(&paused["state"]), 0, "{paused}");
    assert_eq!(paused["state"]["markers"], json!([]), "{paused}");
    assert_eq!(paused["commands"]["pending"], 1, "{paused}");

    let advanced = server.advance(LONG_TIMESTEP_NANOS);
    assert_eq!(tick(&advanced["state"]), 1, "{advanced}");
    assert_eq!(advanced["state"]["markers"], json!([1]), "{advanced}");
    assert_eq!(advanced["commands"]["pending"], 0, "{advanced}");
    server.send(&request("shutdown"));
    let result = server.finish("shutdown", None);
    assert_eq!(result["state"], advanced["state"], "{result}");
    print_scaled_service_evidence(name, &result);

    let name = "server-long-tick-resume-eof";
    let mut server = Service::spawn(custody, executable, Launch::Paused(None), name);
    server.timestep_nanos = LONG_TIMESTEP_NANOS;
    let ready = server.ready(true, false);
    assert_eq!(tick(&ready["state"]), 0, "{ready}");
    server.send(&request("resume"));
    server.close_input();

    let resumed = server.reply("resumed");
    server.check_observation(&resumed["state"], &resumed);
    assert_eq!(resumed["paused"], false, "{resumed}");
    let result = server.finish("stdin_eof", None);
    assert_eq!(tick(&result["state"]), 0, "{result}");
    assert_eq!(
        result["server_evidence"]["commands"]["accepted"], 0,
        "{result}"
    );
    print_scaled_service_evidence(name, &result);
}

pub(super) fn assert_long_tick_drain(custody: &mut Custody, executable: &Path) {
    let name = "server-long-tick-shutdown";
    let mut server = Service::spawn(custody, executable, Launch::Paced(None), name);
    server.timestep_nanos = LONG_TIMESTEP_NANOS;
    let ready = server.ready(false, false);
    assert_eq!(tick(&ready["state"]), 0, "{ready}");
    assert_eq!(server.admit(&action("travel")), 1);
    server.send(&request("shutdown"));

    let result = server.finish_with_timeout("shutdown", None, HORIZON_DRAIN_TIMEOUT);
    assert_progress(&result["state"], LevelKey::main(LevelId::Cavern), 0, &[]);
    assert_eq!(result["state"]["travel_count"], 1, "{result}");
    assert_eq!(result["visited_levels"], json!(LevelId::ALL), "{result}");
    print_scaled_service_evidence(name, &result);
}

pub(super) fn assert_fast_silent_server(custody: &mut Custody, executable: &Path) {
    let name = "server-fast-silent-stdin";
    let mut server = Service::spawn(custody, executable, Launch::Paced(None), name);
    server.unit_time_scale = false;
    server.ready(false, false);
    let mut marker = action("marker");
    marker["after_ticks"] = json!(600);
    let due = server.admit(&marker);

    thread::sleep(Duration::from_secs(3));
    let observed = server.observe();
    assert!(tick(&observed["state"]) >= due, "{observed}");
    assert_progress(&observed["state"], LevelKey::main(LevelId::Meadow), 0, &[1]);
    assert_eq!(observed["commands"]["pending"], 0, "{observed}");

    server.send(&request("shutdown"));
    let result = server.finish("shutdown", None);
    assert_progress(&result["state"], LevelKey::main(LevelId::Meadow), 0, &[1]);
    print_scaled_service_evidence(name, &result);
}

pub(super) fn assert_fast_edited_server(custody: &mut Custody, executable: &Path) {
    let checkpoint = custody
        .fixture()
        .directory("server-fast-save")
        .join("checkpoint.json");
    for (eof, name) in [(false, "server-fast-shutdown"), (true, "server-fast-eof")] {
        let mut server = Service::spawn(
            custody,
            executable,
            Launch::PausedCheckpoint {
                save: &checkpoint,
                resume: None,
            },
            name,
        );
        server.unit_time_scale = false;
        let ready = server.ready(true, true);
        assert_eq!(tick(&ready["state"]), 1, "{ready}");
        assert_eq!(
            ready["state"]["scene_members"],
            AUTHORED_LEVEL_MEMBERS + 1,
            "{ready}"
        );
        server.exchange_observation(&request("resume"), "resumed");
        let mut marker = action("marker");
        marker["after_ticks"] = json!(201);
        let due = server.admit(&marker);
        let stop = if eof {
            server.close_input();
            "stdin_eof"
        } else {
            server.send(&request("shutdown"));
            "shutdown"
        };
        let result = server.finish_with_timeout(stop, None, HORIZON_DRAIN_TIMEOUT);
        assert!(tick(&result["state"]) >= due, "{result}");
        assert_eq!(result["state"]["markers"], json!([1]), "{result}");
        assert_eq!(
            result["state"]["scene_members"],
            AUTHORED_LEVEL_MEMBERS + 1,
            "{result}"
        );
        assert_eq!(
            result["visited_levels"],
            json!([LevelId::Meadow]),
            "{result}"
        );
        print_scaled_service_evidence(name, &result);
    }
}

fn print_scaled_service_evidence(case: &str, result: &Value) {
    let evidence = &result["server_evidence"];
    let commands = &evidence["commands"];
    println!(
        "TWO_LEVEL_SERVER_SCALED_SERVICE {}",
        json!({
            "case": case,
            "stop": evidence["stop"],
            "accepted": commands["accepted"],
            "acknowledged": commands["acknowledged"],
            "pending": commands["pending"],
            "retained": commands["retained"],
            "tick": result["state"]["tick"],
            "elapsed_nanos": evidence["time"]["elapsed_nanos"],
        })
    );
}

fn assert_refused_startup(custody: &mut Custody, executable: &Path) {
    let parent = custody
        .fixture()
        .root
        .join("server-service-missing-save-parent");
    assert!(!parent.exists());
    let server = Service::spawn(
        custody,
        executable,
        Launch::Paced(Some(&parent.join("checkpoint.json"))),
        "server-service-refused-startup",
    );
    server.startup_refusal();
    assert!(!parent.exists());
}

fn assert_terminal_input(custody: &mut Custody, executable: &Path) {
    for (name, suffix, eof, message) in [
        (
            "server-service-incomplete-input",
            action("marker").to_string(),
            true,
            "ended before the request newline",
        ),
        (
            "server-service-overlong-input",
            " ".repeat(8192),
            false,
            "no newline within",
        ),
    ] {
        let mut server = Service::start(custody, executable, None, name);
        server.write(&format!("{}\n{suffix}", action("marker")));
        if eof {
            server.close_input();
        }
        server.read_admission();
        let result = server.finish("input_error", Some(message));
        assert_progress(&result["state"], LevelKey::main(LevelId::Meadow), 0, &[1]);
    }
}

fn assert_gameplay_rejection(custody: &mut Custody, executable: &Path) {
    let missing = custody
        .fixture()
        .directory("server-service-missing-checkpoint")
        .join("checkpoint.json");
    let mut server = Service::spawn(
        custody,
        executable,
        Launch::PausedCheckpoint {
            save: &missing,
            resume: None,
        },
        "server-service-rejection",
    );
    server.ready(true, true);
    server.write(&format!("{}\n{}\n", request("restore"), action("marker")));
    let restore_tick = server.read_admission();
    let marker_tick = server.read_admission();
    assert_eq!(restore_tick, marker_tick);
    let settled = server.exchange_observation(
        &json!({
            "request": "advance",
            "elapsed_nanos": TIMESTEP_NANOS,
        }),
        "advanced",
    );
    let expected_markers = vec![1];
    server.check_observation(&settled["state"], &settled);
    assert_progress(
        &settled["state"],
        LevelKey::main(LevelId::Meadow),
        0,
        &expected_markers,
    );
    assert_eq!(settled["state"]["restores"], 0);
    assert!(!missing.exists());
    let rejections = &settled["gameplay_rejections"];
    assert_eq!(rejections["count"], 1, "{settled}");
    let latest = &rejections["latest"];
    let key: CommandKey = serde_json::from_value(latest["key"].clone()).unwrap();
    assert_eq!(key.tick().get(), restore_tick);
    assert_eq!(key.source_sequence().get(), 1);
    assert_eq!(key.source().id().unwrap().as_str(), SOURCE);
    assert!(
        latest["message"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    server.send(&request("shutdown"));
    let closed = server.finish("shutdown", None);
    assert_eq!(
        closed["server_evidence"]["gameplay_rejections"],
        *rejections
    );
}

pub(super) enum Launch<'a> {
    Paced(Option<&'a Path>),
    Paused(Option<&'a Path>),
    PausedCheckpoint {
        save: &'a Path,
        resume: Option<&'a Path>,
    },
}

pub(super) struct Service<'a> {
    custody: &'a mut Custody,
    cwd: PathBuf,
    input: Option<SupervisedBootstrapChannel>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    cursor: usize,
    receipt: Option<SupervisedProcessCleanupReceipt>,
    restored: bool,
    unit_time_scale: bool,
    timestep_nanos: u64,
    startup_commands: u64,
    exact_startup_ticks: u64,
    last_tick: u64,
    admitted_ticks: Vec<u64>,
}

impl<'a> Service<'a> {
    fn start(custody: &'a mut Custody, executable: &Path, path: Option<&Path>, name: &str) -> Self {
        let mut service = Self::spawn(custody, executable, Launch::Paced(path), name);
        let ready = service.ready(false, path.is_some());
        let state = &ready["state"];
        assert_progress(state, LevelKey::main(LevelId::Meadow), 0, &[]);
        assert_eq!(state["travel_count"], 0);
        assert_eq!(state["saves"], 0);
        assert_eq!(state["restores"], 0);
        assert_eq!(state["held"], json!(RELEASED));
        assert_eq!(player(state), ANCHOR_POSITION);
        service
    }

    pub(super) fn spawn(
        custody: &'a mut Custody,
        executable: &Path,
        launch: Launch<'_>,
        name: &str,
    ) -> Self {
        let cwd = custody.fixture().directory(name);
        let mut arguments = vec![OsString::from("--serve")];
        let (paused, resume, save) = match launch {
            Launch::Paced(save) => (false, None, save),
            Launch::Paused(resume) => (true, resume, None),
            Launch::PausedCheckpoint { save, resume } => (true, resume, Some(save)),
        };
        if paused {
            arguments.push("--paused".into());
        }
        for (flag, path) in [("--resume-state", resume), ("--save", save)] {
            if let Some(path) = path {
                assert!(path.is_absolute());
                arguments.extend([OsString::from(flag), path.as_os_str().to_owned()]);
            }
        }
        let command =
            SupervisedProductProcessCommand::new(SupervisedProductEnvironmentPolicy::Sealed, &cwd)
                .with_bootstrap()
                .arguments(arguments);
        let input = custody.start_interactive_product(executable, &command);
        Self {
            custody,
            cwd,
            input: Some(input),
            stdout: Vec::new(),
            stderr: Vec::new(),
            cursor: 0,
            receipt: None,
            restored: resume.is_some(),
            unit_time_scale: true,
            timestep_nanos: TIMESTEP_NANOS,
            startup_commands: u64::from(resume.is_some() || save.is_some()),
            exact_startup_ticks: u64::from(resume.is_none() && save.is_some()),
            last_tick: 0,
            admitted_ticks: Vec::new(),
        }
    }

    pub(super) fn spawn_scaled(
        custody: &'a mut Custody,
        executable: &Path,
        launch: Launch<'_>,
        name: &str,
    ) -> Self {
        let mut service = Self::spawn(custody, executable, launch, name);
        service.unit_time_scale = false;
        service
    }

    pub(super) fn ready(&mut self, paused: bool, checkpoint: bool) -> Value {
        let ready = self.read(READY, STARTUP_TIMEOUT);
        assert_eq!(ready["version"], 1, "{ready}");
        assert_eq!(ready["timestep_nanos"], self.timestep_nanos, "{ready}");
        assert_eq!(ready["checkpoint_configured"], checkpoint, "{ready}");
        assert_eq!(
            ready["checkpoint_pending"],
            self.restored && checkpoint,
            "{ready}"
        );
        assert_eq!(ready["paused"], paused, "{ready}");
        assert_eq!(ready["restored"], self.restored, "{ready}");
        self.last_tick = tick(&ready["state"]);
        assert!(self.last_tick >= self.startup_commands, "{ready}");
        if self.exact_startup_ticks != 0 {
            assert_eq!(self.last_tick, self.exact_startup_ticks, "{ready}");
            assert_eq!(ready["time"]["elapsed_nanos"], 0, "{ready}");
        }
        ready
    }

    pub(super) fn expect_restored_pending(&mut self, offsets: &[u64]) {
        assert!(self.restored && self.admitted_ticks.is_empty());
        self.admitted_ticks
            .extend(offsets.iter().map(|offset| self.last_tick + offset));
    }

    pub(super) fn startup_refusal(mut self) -> Value {
        let result = self.read(RESULT, STARTUP_TIMEOUT);
        assert_eq!(result["scenario"], "serve", "{result}");
        assert_eq!(result["status"], "rejected", "{result}");
        assert_eq!(result["success"], false, "{result}");
        assert_eq!(result["stage"], "service_startup", "{result}");
        assert_eq!(result["ready"], false, "{result}");
        assert_eq!(result["close"], "complete", "{result}");
        assert_eq!(result["cleanup_complete"], true, "{result}");
        assert!(
            result["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty()),
            "{result}"
        );
        self.assert_native_exit(1, &result);
        result
    }

    fn close_input(&mut self) {
        drop(self.input.take().expect("open Server stdin"));
    }

    fn collect_and_mark(&mut self, score: u64, marker: u64) -> Value {
        self.admit(&movement(true));
        let moved = self.until("rightward movement to collect the authored gem", |state| {
            number(state, "score") >= score
        });
        assert_eq!(moved["held"], json!([false, true, false, false]));
        assert!(player(&moved)[0] > ANCHOR_POSITION[0], "{moved}");
        assert_eq!(moved["score"], score);
        assert_eq!(moved["visible_pickups"], 1);

        self.admit(&movement(false));
        let released = self.until("movement release to apply", |state| {
            state["held"] == json!(RELEASED)
        });
        self.admit(&action("marker"));
        let marked = self.until("the marker to activate", |state| {
            state["markers"] == json!([marker])
        });
        assert_eq!(player(&marked), player(&released));
        marked
    }

    fn until_level(&mut self, level: LevelKey) -> Value {
        self.until("level travel to commit", |state| {
            state["level"] == json!(level) && state["phase"] == "Running"
        })
    }

    pub(super) fn until(&mut self, description: &str, reached: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + CONTROL_TIMEOUT;
        loop {
            let observation = self.observe();
            let state = &observation["state"];
            if reached(state) {
                return observation["state"].clone();
            }
            assert!(
                Instant::now() < deadline,
                "waiting for {description}: {observation}"
            );
            thread::sleep(POLL);
        }
    }

    pub(super) fn admit(&mut self, request: &Value) -> u64 {
        self.send(request);
        self.read_admission()
    }

    fn read_admission(&mut self) -> u64 {
        let accepted = self.reply("accepted");
        let tick = number(&accepted, "tick");
        assert_eq!(accepted["source"], SOURCE, "{accepted}");
        assert_eq!(
            accepted["sequence"],
            self.admitted_ticks.len() as u64 + 1,
            "{accepted}"
        );
        assert!(
            tick > self.last_tick,
            "{accepted}; last observed tick {}",
            self.last_tick
        );
        assert!(
            self.admitted_ticks
                .last()
                .is_none_or(|prior| tick >= *prior),
            "{accepted}"
        );
        self.admitted_ticks.push(tick);
        tick
    }

    fn reject(&mut self) {
        let rejected = self.reply("rejected");
        assert!(
            rejected["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty()),
            "{rejected}"
        );
    }

    pub(super) fn observe(&mut self) -> Value {
        self.exchange_observation(&request("observe"), "observed")
    }

    pub(super) fn observe_after_capture_maintenance(&mut self, mut before_capture: Value) -> Value {
        let frames = number(&before_capture["time"], "frames");
        before_capture["time"]["frames"] = json!(frames + 1);
        let observed = self.observe();
        assert_eq!(
            observed, before_capture,
            "capture after command admission must complete exactly one zero-duration App frame"
        );
        observed
    }

    pub(super) fn advance(&mut self, elapsed_nanos: u64) -> Value {
        let advanced = self.exchange_observation(
            &json!({"request": "advance", "elapsed_nanos": elapsed_nanos}),
            "advanced",
        );
        assert_eq!(advanced["paused"], true, "{advanced}");
        advanced
    }

    pub(super) fn exchange_observation(&mut self, request: &Value, status: &str) -> Value {
        let observed = self.exchange(request, status);
        self.check_observation(&observed["state"], &observed);
        observed
    }

    fn check_observation(&mut self, state: &Value, evidence: &Value) {
        let state_tick = tick(state);
        assert!(state_tick >= self.last_tick, "{state}");
        self.last_tick = state_tick;
        assert_eq!(state.get("error"), Some(&Value::Null), "{state}");

        let commands = &evidence["commands"];
        let through = number(commands, "acknowledged_through_tick");
        assert_eq!(through, state_tick, "{evidence}");
        let accepted = self.startup_commands + self.admitted_ticks.len() as u64;
        let acknowledged = self.startup_commands
            + self.admitted_ticks.partition_point(|&tick| tick <= through) as u64;
        assert_eq!(commands["accepted"], accepted, "{evidence}");
        assert_eq!(commands["acknowledged"], acknowledged, "{evidence}");
        assert_eq!(commands["pending"], accepted - acknowledged, "{evidence}");
        assert_eq!(commands["retained"], accepted - acknowledged, "{evidence}");

        let time = &evidence["time"];
        assert!(number(time, "frames") > 0, "{evidence}");
        let debt = number(time, "fixed_debt_nanos");
        let remainder = number(time, "fixed_remainder_nanos");
        assert_eq!(debt % self.timestep_nanos, 0, "{evidence}");
        assert!(remainder < self.timestep_nanos, "{evidence}");
        if self.unit_time_scale && !self.restored {
            assert_eq!(
                (state_tick - self.exact_startup_ticks) * self.timestep_nanos + debt + remainder,
                number(time, "elapsed_nanos"),
                "{state}\n{time}"
            );
        }
    }

    pub(super) fn exchange(&mut self, request: &Value, status: &str) -> Value {
        self.send(request);
        self.reply(status)
    }

    pub(super) fn send(&mut self, request: &Value) {
        self.write(&format!("{request}\n"));
    }

    fn write(&mut self, lines: &str) {
        let mut input = self.input.take().expect("open Server stdin");
        let cancel = input.cancellation();
        let lines = lines.to_owned();
        let (sent, finished) = mpsc::sync_channel(1);
        let _writer = thread::spawn(move || {
            let result = input.write_all(lines.as_bytes());
            let _ = sent.send((input, result));
        });
        match finished.recv_timeout(CONTROL_TIMEOUT) {
            Ok((input, result)) => {
                self.input = Some(input);
                result.expect("write complete Server request");
            }
            Err(error) => {
                let cancellation = cancel.cancel();
                panic!("Server stdin write did not finish: {error}; cancel: {cancellation:?}");
            }
        }
    }

    fn reply(&mut self, status: &str) -> Value {
        let reply = self.read(REPLY, CONTROL_TIMEOUT);
        assert_eq!(reply["status"], status, "{reply}");
        reply
    }

    fn read(&mut self, prefix: &str, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            while let Some(newline) = self.stdout[self.cursor..]
                .iter()
                .position(|&byte| byte == b'\n')
            {
                let end = self.cursor + newline;
                let line = std::str::from_utf8(&self.stdout[self.cursor..end])
                    .expect("Server stdout must be UTF-8")
                    .trim_end_matches('\r');
                self.cursor = end + 1;
                if let Some(payload) = line.strip_prefix(prefix) {
                    return serde_json::from_str(payload).expect("valid Server record");
                }
                assert!(
                    !is_protocol_line(line),
                    "expected {prefix}, received {line}"
                );
            }
            assert!(
                self.receipt.is_none() && Instant::now() < deadline,
                "Server did not send {prefix}; native exit: {:?}\nstdout: {}\nstderr: {}",
                self.receipt,
                String::from_utf8_lossy(&self.stdout),
                String::from_utf8_lossy(&self.stderr),
            );
            self.poll();
            thread::sleep(POLL);
        }
    }

    fn poll(&mut self) {
        self.receipt = self.custody.poll_output(&mut self.stdout, &mut self.stderr);
    }

    pub(super) fn finish(self, stop: &str, error: Option<&str>) -> Value {
        let result = self.finish_with_timeout(stop, error, CONTROL_TIMEOUT);
        if stop != "retired" {
            assert_eq!(
                result["server_evidence"]["commands"]["retained"], 0,
                "{result}"
            );
        }
        result
    }

    fn finish_with_timeout(mut self, stop: &str, error: Option<&str>, timeout: Duration) -> Value {
        let result = self.read(RESULT, timeout);
        let code = if let Some(message) = error {
            assert_eq!(result["scenario"], "serve", "{result}");
            assert_eq!(result["status"], "error", "{result}");
            assert_eq!(result["success"], false, "{result}");
            assert_eq!(result["stage"], "service", "{result}");
            assert_eq!(result["ready"], true, "{result}");
            assert_eq!(result["close"], "complete", "{result}");
            assert_eq!(result["cleanup_complete"], true, "{result}");
            assert!(
                result["message"]
                    .as_str()
                    .is_some_and(|text| text.contains(message)),
                "{result}"
            );
            1
        } else {
            assert_complete(&result, "serve");
            0
        };
        assert_eq!(result.get("checkpoint"), Some(&Value::Null));
        let evidence = &result["server_evidence"];
        assert_eq!(evidence["stop"], stop, "{result}");
        assert_eq!(evidence["input_reader_joined"], true, "{result}");
        self.check_observation(&result["state"], evidence);
        self.assert_native_exit(code, &result);
        result
    }

    fn assert_native_exit(&mut self, exit_code: i32, result: &Value) {
        let deadline = Instant::now() + CONTROL_TIMEOUT;
        while self.receipt.is_none() {
            self.poll();
            assert!(
                self.receipt.is_some() || Instant::now() < deadline,
                "Server did not retire: {result}"
            );
            thread::sleep(POLL);
        }
        assert_eq!(
            self.receipt.unwrap().exit().code(),
            Some(exit_code),
            "{result}\nstderr: {}",
            String::from_utf8_lossy(&self.stderr)
        );
        let tail =
            std::str::from_utf8(&self.stdout[self.cursor..]).expect("Server stdout must be UTF-8");
        assert!(
            !tail.lines().any(is_protocol_line),
            "unexpected Server records: {tail}"
        );
        assert!(fs::read_dir(&self.cwd).unwrap().next().is_none());
    }
}

fn is_protocol_line(line: &str) -> bool {
    [READY, REPLY, RESULT]
        .into_iter()
        .any(|prefix| line.starts_with(prefix))
}

pub(super) fn tick(state: &Value) -> u64 {
    number(state, "tick")
}

pub(super) fn number(object: &Value, field: &str) -> u64 {
    object[field]
        .as_u64()
        .unwrap_or_else(|| panic!("expected numeric {field}: {object}"))
}

pub(super) fn player(state: &Value) -> [f32; 2] {
    serde_json::from_value(state["player"].clone()).expect("observed explorer position")
}

pub(super) fn assert_progress(state: &Value, level: LevelKey, score: u64, markers: &[u64]) {
    assert_eq!(state["initialized"], true, "{state}");
    assert_eq!(state["phase"], "Running", "{state}");
    assert_eq!(state["restoring"], false, "{state}");
    assert_eq!(state["level"], json!(level), "{state}");
    assert_eq!(state["scene_members"], AUTHORED_LEVEL_MEMBERS, "{state}");
    assert_eq!(state["score"], score, "{state}");
    assert_eq!(state["collected"], score / 10, "{state}");
    assert_eq!(state["markers"], json!(markers), "{state}");
}

pub(super) fn request(name: &str) -> Value {
    json!({"request": name})
}

fn command(body: Value) -> Value {
    json!({"request": "command", "command": body})
}

pub(super) fn action(operation: &str) -> Value {
    command(json!({"operation": operation}))
}

pub(super) fn movement(pressed: bool) -> Value {
    command(json!({"operation": "move", "direction": 1, "pressed": pressed}))
}
