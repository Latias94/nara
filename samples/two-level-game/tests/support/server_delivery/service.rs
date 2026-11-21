use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use nara::{
    gameplay::GameplayCommandKey,
    project_tooling::process::{
        SupervisedBootstrapChannel, SupervisedProcessCleanupReceipt,
        SupervisedProductEnvironmentPolicy, SupervisedProductProcessCommand,
    },
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
const TIMESTEP_NANOS: u64 = 20_000_000;
const POLL: Duration = Duration::from_millis(10);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(65);
const RELEASED: [bool; 4] = [false; 4];
const ANCHOR_POSITION: [f32; 2] = [-120.0, 0.0];

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

fn assert_refused_startup(custody: &mut Custody, executable: &Path) {
    let parent = custody
        .fixture()
        .root
        .join("server-service-missing-save-parent");
    assert!(!parent.exists());
    let mut server = Service::spawn(
        custody,
        executable,
        Some(&parent.join("checkpoint.json")),
        "server-service-refused-startup",
    );
    let result = server.read(RESULT, STARTUP_TIMEOUT);
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
            .is_some_and(|message| !message.is_empty())
    );
    server.assert_native_exit(1, &result);
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
    let mut server = Service::start(
        custody,
        executable,
        Some(&missing),
        "server-service-rejection",
    );
    for attempt in 1..=8 {
        server.write(&format!("{}\n{}\n", request("restore"), action("marker")));
        let restore_tick = server.read_admission();
        let marker_tick = server.read_admission();
        let deadline = Instant::now() + CONTROL_TIMEOUT;
        let settled = loop {
            server.send(&request("observe"));
            let observation = server.reply("observed");
            if tick(&observation["state"]) >= marker_tick {
                break observation;
            }
            assert!(Instant::now() < deadline, "{observation}");
            thread::sleep(POLL);
        };
        let expected_markers: Vec<u64> = (1..=attempt).collect();
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
        assert_eq!(rejections["count"], attempt, "{settled}");
        let latest = &rejections["latest"];
        let key: GameplayCommandKey = serde_json::from_value(latest["key"].clone()).unwrap();
        assert_eq!(key.tick().get(), restore_tick);
        assert_eq!(key.source_sequence().get(), attempt * 2 - 1);
        assert_eq!(key.source().id().unwrap().as_str(), SOURCE);
        assert!(
            latest["message"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        if restore_tick == marker_tick {
            server.send(&request("shutdown"));
            let closed = server.finish("shutdown", None);
            assert_eq!(
                closed["server_evidence"]["gameplay_rejections"],
                *rejections
            );
            return;
        }
    }
    panic!("no Restore and Marker pair reached the same admitted fixed tick");
}

struct Service<'a> {
    custody: &'a mut Custody,
    cwd: PathBuf,
    input: Option<SupervisedBootstrapChannel>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    cursor: usize,
    receipt: Option<SupervisedProcessCleanupReceipt>,
    startup_commands: u64,
    last_tick: u64,
    admitted_ticks: Vec<u64>,
}

impl<'a> Service<'a> {
    fn start(custody: &'a mut Custody, executable: &Path, path: Option<&Path>, name: &str) -> Self {
        let mut service = Self::spawn(custody, executable, path, name);
        let ready = service.read(READY, STARTUP_TIMEOUT);
        assert_eq!(ready["version"], 1, "{ready}");
        assert_eq!(ready["timestep_nanos"], TIMESTEP_NANOS, "{ready}");
        assert_eq!(ready["checkpoint_configured"], path.is_some(), "{ready}");
        let state = &ready["state"];
        service.last_tick = tick(state);
        assert!(service.last_tick >= service.startup_commands, "{ready}");
        assert_progress(state, LevelKey::main(LevelId::Meadow), 0, &[]);
        assert_eq!(state["travel_count"], 0);
        assert_eq!(state["saves"], 0);
        assert_eq!(state["restores"], 0);
        assert_eq!(state["held"], json!(RELEASED));
        assert_eq!(player(state), ANCHOR_POSITION);
        service
    }

    fn spawn(custody: &'a mut Custody, executable: &Path, path: Option<&Path>, name: &str) -> Self {
        let cwd = custody.fixture().directory(name);
        let mut arguments = vec![OsString::from("--serve")];
        if let Some(path) = path {
            assert!(path.is_absolute());
            arguments.extend([OsString::from("--save"), path.as_os_str().to_owned()]);
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
            startup_commands: u64::from(path.is_some()),
            last_tick: 0,
            admitted_ticks: Vec::new(),
        }
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

    fn until(&mut self, description: &str, reached: impl Fn(&Value) -> bool) -> Value {
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

    fn admit(&mut self, request: &Value) {
        self.send(request);
        self.read_admission();
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

    fn observe(&mut self) -> Value {
        self.send(&request("observe"));
        let observed = self.reply("observed");
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
        assert_eq!(commands["retained"], accepted - acknowledged, "{evidence}");

        let time = &evidence["time"];
        assert!(number(time, "frames") > 0, "{evidence}");
        let accounted = state_tick * TIMESTEP_NANOS + number(time, "fixed_debt_nanos");
        assert!(
            (accounted..accounted + TIMESTEP_NANOS).contains(&number(time, "elapsed_nanos")),
            "{state}\n{time}"
        );
    }

    fn send(&mut self, request: &Value) {
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

    fn finish(mut self, stop: &str, error: Option<&str>) -> Value {
        let result = self.read(RESULT, CONTROL_TIMEOUT);
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
        assert_eq!(evidence["commands"]["retained"], 0, "{result}");
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
            "{result}"
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

fn tick(state: &Value) -> u64 {
    number(state, "tick")
}

fn number(object: &Value, field: &str) -> u64 {
    object[field]
        .as_u64()
        .unwrap_or_else(|| panic!("expected numeric {field}: {object}"))
}

fn player(state: &Value) -> [f32; 2] {
    serde_json::from_value(state["player"].clone()).expect("observed explorer position")
}

fn assert_progress(state: &Value, level: LevelKey, score: u64, markers: &[u64]) {
    assert_eq!(state["initialized"], true, "{state}");
    assert_eq!(state["phase"], "Running", "{state}");
    assert_eq!(state["restoring"], false, "{state}");
    assert_eq!(state["level"], json!(level), "{state}");
    assert_eq!(state["scene_members"], AUTHORED_LEVEL_MEMBERS, "{state}");
    assert_eq!(state["score"], score, "{state}");
    assert_eq!(state["collected"], score / 10, "{state}");
    assert_eq!(state["markers"], json!(markers), "{state}");
}

fn request(name: &str) -> Value {
    json!({"request": name})
}

fn command(body: Value) -> Value {
    json!({"request": "command", "command": body})
}

fn action(operation: &str) -> Value {
    command(json!({"operation": operation}))
}

fn movement(pressed: bool) -> Value {
    command(json!({"operation": "move", "direction": 1, "pressed": pressed}))
}
