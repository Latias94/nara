use std::{
    ffi::OsString,
    num::NonZeroUsize,
    thread,
    time::{Duration, Instant},
};

use nara::project_tooling::process::SupervisedProcessCommand;
use serde_json::{Value, json};
use two_level_game::{
    levels::{LevelId, LevelKey},
    save::{Checkpoint, MAX_CHECKPOINT_BYTES, decode_checkpoint},
};

use super::{
    OUTPUT_BYTES, REPORT, Request, evidence,
    process::{NativeProcess, OBSERVATION, POLL, READY, RESULT, read_bounded},
    window::{self, Input},
};

const STEP_TIMEOUT: Duration = Duration::from_secs(15);
const BLOCKED_HOLD: Duration = Duration::from_millis(350);
const CONTACT_TOLERANCE: f32 = 0.03;

struct Session<'a> {
    input: Input,
    native: NativeProcess,
    request: &'a Request,
    latest: Option<Value>,
    result: Option<Value>,
    sequence: u64,
    ready: bool,
    fallback: bool,
}

impl<'a> Session<'a> {
    fn spawn(request: &'a Request, fallback: bool, finite: bool) -> Self {
        let input = Input::new(&request.title);
        let arguments: Vec<OsString> = if finite {
            vec!["--observe".into(), "--verify-first-present".into()]
        } else {
            vec![
                "--save".into(),
                request.save.as_os_str().to_owned(),
                "--observe".into(),
            ]
        };
        let command = player_command(request, arguments, fallback);
        let native = NativeProcess::spawn(&command, Some(request.title.clone()));
        Self {
            input,
            native,
            request,
            latest: None,
            result: None,
            sequence: 0,
            ready: false,
            fallback,
        }
    }

    fn collect(&mut self) {
        for (kind, record) in self.native.records() {
            assert!(
                self.result.is_none(),
                "evidence followed the terminal result"
            );
            assert_eq!(evidence::number(&record, "sequence"), self.sequence + 1);
            self.sequence += 1;
            assert!(self.sequence <= 1024, "bounded optional observation stream");
            evidence::presentation(
                &record,
                self.latest.as_ref(),
                self.fallback,
                kind == OBSERVATION,
            );
            assert_eq!(record["success"], true, "{record}");
            assert_eq!(record["scenario"], "interactive");
            assert_eq!(record["source_free"], true);
            assert_eq!(record["processed_runtime"], true);
            assert_eq!(record["catalog_hash"], self.request.catalog_digest["hash"]);
            assert_eq!(
                record["catalog_length"],
                self.request.catalog_digest["length"]
            );
            assert_eq!(record["content_target"], self.request.target);
            if running(&record) {
                evidence::runtime(&record, &self.request.images);
            }
            match kind {
                READY => {
                    assert!(!self.ready, "one original READY record");
                    assert_eq!(record["ready"], true);
                    assert_eq!(record["launch_accepted"], true);
                    assert_eq!(record["resume_applied"], true);
                    self.ready = true;
                }
                RESULT => {
                    for field in [
                        "cleanup_complete",
                        "original_runtime_retired",
                        "original_pump_returned",
                        "first_present_verified",
                    ] {
                        assert_eq!(
                            record[field], true,
                            "terminal success preceded original retirement: {record}"
                        );
                    }
                    self.result = Some(record.clone());
                }
                OBSERVATION => {}
                _ => unreachable!(),
            }
            if kind != RESULT {
                for field in [
                    "cleanup_complete",
                    "original_runtime_retired",
                    "original_pump_returned",
                ] {
                    assert_eq!(
                        record[field], false,
                        "nonterminal presentation claimed retirement"
                    );
                }
            }
            self.latest = Some(record);
        }
    }

    fn poll(&mut self) {
        self.native.poll();
        self.collect();
    }

    fn snapshot(&self) -> Value {
        self.latest
            .as_ref()
            .expect("actual presented game observation")
            .clone()
    }

    fn start(request: &'a Request) -> Self {
        let mut session = Self::spawn(request, false, false);
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            session.poll();
            if session.ready && session.latest.as_ref().is_some_and(running) {
                break;
            }
            assert!(
                !session.native.complete() && Instant::now() < deadline,
                "desktop never became playable: {}",
                session.native.diagnostics()
            );
            thread::sleep(POLL);
        }
        let window = session.native.bind_window();
        session.input.bind(window);
        let initial = session.snapshot();
        evidence::progress(&initial, main(LevelId::Meadow), 0, &[], 2);
        assert_eq!(
            initial["state"]["held"],
            json!([false, false, false, false])
        );
        assert_eq!(initial["state"]["travel_count"], 0);
        assert_eq!(initial["state"]["saves"], 0);
        assert_eq!(initial["state"]["restores"], 0);
        session
    }

    fn wait(&mut self, basis: &Value, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + STEP_TIMEOUT;
        loop {
            self.input.check();
            self.poll();
            if let Some(record) = &self.latest
                && running(record)
                && predicate(record)
                && evidence::number(&record["presentation"], "app_frame")
                    > evidence::number(&basis["presentation"], "app_frame")
                && evidence::number(&record["presentation"], "render_frame")
                    > evidence::number(&basis["presentation"], "render_frame")
            {
                return record.clone();
            }
            assert!(
                !self.native.complete() && Instant::now() < deadline,
                "physical transition never reached a matching newer presentation; basis={basis}; {}",
                self.native.diagnostics()
            );
            thread::sleep(POLL);
        }
    }

    fn pump_for(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.input.check();
            self.poll();
            assert!(
                !self.native.complete(),
                "ordinary Player exited before WM_CLOSE: {}",
                self.native.diagnostics()
            );
            thread::sleep(POLL);
        }
    }

    fn tap(&mut self, scan: u16, predicate: impl Fn(&Value) -> bool) -> Value {
        let basis = self.snapshot();
        self.input.press(scan);
        self.pump_for(Duration::from_millis(40));
        self.input.release();
        self.wait(&basis, predicate)
    }

    fn travel(
        &mut self,
        scan: u16,
        level: LevelKey,
        score: u32,
        markers: &[u64],
        visible: usize,
    ) -> Value {
        let instance = self.snapshot()["state"]["current_instance"].clone();
        let record = self.tap(scan, |record| {
            record["level"] == json!(level)
                && record["state"]["current_instance"] != instance
                && record["markers"] == json!(markers)
        });
        evidence::progress(&record, level, score, markers, visible);
        assert_eq!(record["state"]["held"], json!([false, false, false, false]));
        record
    }

    fn walk_and_mark(&mut self, level: LevelKey, score: u32, marker: u64) -> [f32; 2] {
        let basis = self.snapshot();
        self.input.press(window::J);
        self.wait(&basis, |record| {
            assert!(
                evidence::number(record, "score") <= u64::from(score),
                "physical movement skipped the required near pickup"
            );
            record["score"] == score
                && record["state"]["held"] == json!([false, true, false, false])
        });
        let pressed = self.snapshot();
        self.input.release();
        let released = self.wait(&pressed, |record| {
            record["state"]["held"] == json!([false, false, false, false])
        });
        evidence::progress(&released, level, score, &[], 1);
        assert_ne!(
            released["state"]["player"], basis["state"]["player"],
            "J did not move the actual player"
        );
        let pose = position(&released);
        self.pump_for(Duration::from_millis(250));
        assert_eq!(
            position(&self.snapshot()),
            pose,
            "released J moved the idle player"
        );
        let marked = self.tap(window::M, |record| record["markers"] == json!([marker]));
        evidence::progress(&marked, level, score, &[marker], 1);
        assert_eq!(
            position(&marked),
            pose,
            "idle pose changed before the next actual presented marker"
        );
        assert_eq!(marked["state"]["held"], json!([false, false, false, false]));
        pose
    }

    fn collision_excursion(&mut self, level: LevelKey, score: u32, marker: u64) -> Value {
        let start = self.snapshot();
        evidence::progress(&start, level, score, &[marker], 1);
        assert_eq!(start["state"]["held"], json!([false, false, false, false]));
        assert_eq!(position(&start)[1], 0.0);
        let inner_x = match level.level() {
            LevelId::Meadow => 0.0,
            LevelId::Cavern => 20.0,
        };
        let expected_x = inner_x - 6.0 - 9.0 - 0.01;
        assert!(position(&start)[0] < expected_x - CONTACT_TOLERANCE);
        self.input.press(window::J);
        let contact = self.wait(&start, |record| {
            evidence::progress(record, level, score, &[marker], 1);
            let [x, y] = position(record);
            assert_eq!(y, 0.0, "J left the original pickup corridor");
            assert!(
                x <= expected_x + CONTACT_TOLERANCE,
                "J crossed the interior wall: {record}"
            );
            (x - expected_x).abs() <= CONTACT_TOLERANCE
                && record["state"]["held"] == json!([false, true, false, false])
        });
        self.pump_for(BLOCKED_HOLD);
        self.input.release();
        let blocked = self.wait(&contact, |record| {
            record["state"]["held"] == json!([false, false, false, false])
        });
        evidence::progress(&blocked, level, score, &[marker], 1);
        assert!((position(&blocked)[0] - expected_x).abs() <= CONTACT_TOLERANCE);
        assert!((position(&blocked)[0] - position(&contact)[0]).abs() <= 0.001);
        assert_eq!(position(&blocked)[1], 0.0);
        assert!(
            evidence::number(&blocked["state"], "tick")
                >= evidence::number(&contact["state"], "tick") + 4
        );
        assert!(
            evidence::number(&blocked["state"], "logical_elapsed_nanos")
                >= evidence::number(&contact["state"], "logical_elapsed_nanos") + 100_000_000
        );
        self.input.press(window::J);
        self.input.press(window::W);
        let sliding = self.wait(&blocked, |record| {
            evidence::progress(record, level, score, &[marker], 1);
            let [x, y] = position(record);
            assert!(
                (x - expected_x).abs() <= CONTACT_TOLERANCE,
                "W+J crossed the wall before its upper end: {record}"
            );
            assert!(
                (0.0..=28.0).contains(&y),
                "W+J skipped the bounded wall-slide sample: {record}"
            );
            y >= 10.0 && record["state"]["held"] == json!([false, true, true, false])
        });
        let cleared = self.wait(&sliding, |record| {
            evidence::progress(record, level, score, &[marker], 1);
            let [x, y] = position(record);
            assert!(
                (expected_x - CONTACT_TOLERANCE..40.0).contains(&x) && (0.0..96.0).contains(&y),
                "wall excursion left its finite pickup-free area: {record}"
            );
            if y <= 33.0 {
                assert!(
                    (x - expected_x).abs() <= CONTACT_TOLERANCE,
                    "W+J crossed the wall before clearing it: {record}"
                );
            }
            y >= 33.5
                && x >= expected_x + 2.0
                && record["state"]["held"] == json!([false, true, true, false])
        });
        self.input.release();
        let released = self.wait(&cleared, |record| {
            record["state"]["held"] == json!([false, false, false, false])
        });
        evidence::progress(&released, level, score, &[marker], 1);
        let [x, y] = position(&released);
        assert!((expected_x + 2.0..40.0).contains(&x) && (33.5..96.0).contains(&y));
        self.pump_for(Duration::from_millis(100));
        assert_eq!(
            position(&self.snapshot()),
            [x, y],
            "released wall excursion moved the idle player"
        );
        for sample in [&contact, &blocked, &sliding, &cleared, &released] {
            assert_eq!(
                sample["state"]["current_instance"],
                start["state"]["current_instance"]
            );
            for field in ["travel_count", "restarts", "saves", "restores"] {
                assert_eq!(sample["state"][field], start["state"][field]);
            }
        }
        json!({
            "level": level,
            "marker": marker,
            "score": score,
            "inner_wall_x": inner_x,
            "expected_contact_x": expected_x,
            "margin": 0.01_f32,
            "held_against_wall_ms": BLOCKED_HOLD.as_millis(),
            "start": collision_sample(&start),
            "contact": collision_sample(&contact),
            "blocked": collision_sample(&blocked),
            "sliding": collision_sample(&sliding),
            "cleared": collision_sample(&cleared),
            "released": collision_sample(&released),
        })
    }

    fn close(mut self) -> (Value, Value) {
        self.input.release();
        self.native.close_window();
        self.native.finish(Duration::from_secs(30));
        self.collect();
        self.native.assert_clean();
        assert!(
            self.native.receipt().exit().success(),
            "{}",
            self.native.diagnostics()
        );
        let result = self
            .result
            .take()
            .expect("one terminal result after normal native close");
        assert!(self.ready);
        evidence::runtime(&result, &self.request.images);
        assert!(
            self.input.restore(),
            "restore the original foreground after normal native retirement"
        );
        (result, self.native.cleanup_json())
    }
}

pub fn run(request: &Request) {
    let mut first = Session::start(request);
    let meadow = main(LevelId::Meadow);
    let cavern = main(LevelId::Cavern);
    let independent = LevelKey::new(LevelId::Cavern, 1).unwrap();
    let refused = first.tap(window::L, |record| record["gameplay_error"].is_string());
    assert_eq!(refused["success"], true);
    assert_eq!(refused["level"], json!(meadow));
    assert_eq!(refused["score"], 0);
    assert_eq!(refused["state"]["phase"], "Running");
    assert!(
        !request.save.exists(),
        "refused Load must not publish a save"
    );
    first.travel(window::R, meadow, 0, &[], 2);
    let one = first.walk_and_mark(meadow, 10, 1);
    let meadow_collision = first.collision_excursion(meadow, 10, 1);
    first.travel(window::E, cavern, 10, &[], 2);
    let two = first.walk_and_mark(cavern, 20, 2);
    let cavern_collision = first.collision_excursion(cavern, 20, 2);
    first.travel(window::Q, independent, 20, &[], 2);
    let three = first.walk_and_mark(independent, 30, 3);
    first.travel(window::Q, cavern, 30, &[2], 1);
    let restarts = evidence::number(&first.snapshot()["state"], "restarts");
    let restarted = first.travel(window::R, cavern, 30, &[2], 1);
    assert_eq!(
        evidence::number(&restarted["state"], "restarts"),
        restarts + 1
    );
    first.travel(window::E, meadow, 30, &[1], 1);
    first.travel(window::E, cavern, 30, &[2], 1);
    let saved = first.tap(window::K, |record| record["state"]["saves"] == 1);
    evidence::progress(&saved, cavern, 30, &[2], 1);
    let bytes = read_bounded(&request.save, MAX_CHECKPOINT_BYTES);
    let decoded = decode_checkpoint(&bytes).unwrap();
    assert!(decoded.migrated_from.is_none());
    assert_checkpoint(
        &decoded.checkpoint,
        [(meadow, one), (cavern, two), (independent, three)],
    );
    let first_pid = first.native.pid();
    let (saved, first_cleanup) = first.close();
    evidence::scene_sequence(
        &saved,
        &[
            LevelId::Meadow,
            LevelId::Meadow,
            LevelId::Cavern,
            LevelId::Cavern,
            LevelId::Cavern,
            LevelId::Cavern,
            LevelId::Meadow,
            LevelId::Cavern,
        ],
    );
    assert_eq!(read_bounded(&request.save, MAX_CHECKPOINT_BYTES), bytes);

    let mut fresh = Session::start(request);
    assert_ne!(
        fresh.native.pid(),
        first_pid,
        "restore must use a fresh native process"
    );
    let restored = fresh.travel(window::L, cavern, 30, &[2], 1);
    assert_eq!(restored["state"]["restores"], 1);
    assert_eq!(restored["state"]["saves"], 0);
    assert_eq!(restored["image_proofs"].as_array().unwrap().len(), 3);
    let continued = fresh.tap(window::M, |record| record["markers"] == json!([2, 4]));
    evidence::progress(&continued, cavern, 30, &[2, 4], 1);
    fresh.travel(window::Q, independent, 30, &[3], 1);
    fresh.travel(window::E, meadow, 30, &[1], 1);
    evidence::scene_sequence(
        &fresh.snapshot(),
        &[
            LevelId::Meadow,
            LevelId::Cavern,
            LevelId::Cavern,
            LevelId::Meadow,
        ],
    );
    for visit in 0..20 {
        let (level, markers) = if visit % 2 == 0 {
            (cavern, &[2, 4][..])
        } else {
            (meadow, &[1][..])
        };
        fresh.travel(window::E, level, 30, markers, 1);
    }
    let (continued, fresh_cleanup) = fresh.close();
    assert_eq!(continued["scene_receipts_sampled"], true);
    assert_eq!(continued["scene_receipts"].as_array().unwrap().len(), 16);
    evidence::progress(&continued, meadow, 30, &[1], 1);
    assert_eq!(
        read_bounded(&request.save, MAX_CHECKPOINT_BYTES),
        bytes,
        "continued play must leave stored checkpoint bytes unchanged"
    );
    evidence::assert_payload(request.executable.parent().unwrap());

    let mut finite = Session::spawn(request, true, true);
    finite.native.finish(Duration::from_secs(60));
    finite.collect();
    finite.native.assert_clean();
    assert!(
        finite.native.receipt().exit().success(),
        "{}",
        finite.native.diagnostics()
    );
    let finite_result = finite
        .result
        .take()
        .expect("finite first-present terminal result");
    assert_eq!(finite_result["first_present_verified"], true);
    assert!(
        finite.input.restore(),
        "restore foreground after finite first-present control"
    );
    let (refused, refused_cleanup) = rejected_initial_save(request);
    let scripted = scripted_controls(request);
    println!(
        "\n{REPORT}{}",
        json!({"success": true, "authored_key": "j", "ordinary_processes": 2, "finite_processes": 1,
        "refused_processes": 1, "refused_initial_save": refused,
        "scripted_controls": scripted,
        "collision_excursions": [meadow_collision, cavern_collision],
        "saved": saved, "continued": continued, "finite": finite_result,
        "native_cleanup": [first_cleanup, fresh_cleanup, finite.native.cleanup_json(), refused_cleanup],
        "checkpoint": decoded.checkpoint, "stored_bytes_unchanged": true,
        "recoverable_load_refusal": true, "additional_interactive_travels": 20})
    );
}

fn player_command(
    request: &Request,
    arguments: Vec<OsString>,
    fallback: bool,
) -> SupervisedProcessCommand {
    SupervisedProcessCommand::new(&request.executable, &request.cwd)
        .arguments(arguments)
        .environment("PATH", &request.invalid_path)
        .environment("LOCALAPPDATA", &request.local_data)
        .environment("APPDATA", &request.roaming_data)
        .environment("ProgramData", &request.common_data)
        .environment("ALLUSERSPROFILE", &request.common_data)
        .environment("WGPU_BACKEND", "dx12")
        .environment("NARA_WGPU_FORCE_FALLBACK", if fallback { "1" } else { "0" })
        .bounded_stdout(NonZeroUsize::new(OUTPUT_BYTES).unwrap())
}

fn rejected_initial_save(request: &Request) -> (Value, Value) {
    let path = request
        .local_data
        .join("missing-save-parent")
        .join("checkpoint.json");
    assert!(!path.parent().unwrap().exists());
    let mut input = Input::new(&request.title);
    let command = player_command(
        request,
        vec!["--save".into(), path.as_os_str().to_owned()],
        false,
    );
    let mut native = NativeProcess::spawn(&command, Some(request.title.clone()));
    native.finish(Duration::from_secs(60));
    native.assert_clean();
    assert!(
        !native.receipt().exit().success(),
        "invalid first-tick configuration must be refused"
    );
    let result = super::process::record(&native.stdout(), RESULT);
    assert_eq!(result["success"], false);
    assert_eq!(result["launch_admitted"], true);
    assert_eq!(result["launch_accepted"], false);
    assert_eq!(result["ready"], false);
    assert!(
        result["message"]
            .as_str()
            .unwrap()
            .contains("first-tick desktop launch was rejected")
    );
    for field in [
        "cleanup_complete",
        "original_runtime_retired",
        "original_pump_returned",
        "first_present_verified",
    ] {
        assert_eq!(result[field], true, "{result}");
    }
    assert!(!path.exists() && !path.parent().unwrap().exists());
    assert!(
        input.restore(),
        "restore foreground after refused initial save configuration"
    );
    (result, native.cleanup_json())
}

fn scripted_controls(request: &Request) -> Vec<Value> {
    let save = request.local_data.join("scripted-checkpoint.json");
    assert!(!save.exists());
    let mut results = Vec::new();
    for scenario in ["travel", "save", "restore"] {
        let mut input = Input::new(&request.title);
        let mut arguments = vec![format!("--verify-{scenario}").into()];
        if scenario != "travel" {
            arguments.push(save.as_os_str().to_owned());
        }
        let command = player_command(request, arguments, false);
        let mut native = NativeProcess::spawn(&command, Some(request.title.clone()));
        native.finish(Duration::from_secs(90));
        native.assert_clean();
        assert!(
            native.receipt().exit().success(),
            "{scenario}: {}",
            native.diagnostics()
        );
        let result = super::process::record(&native.stdout(), RESULT);
        assert_eq!(result["scenario"], scenario);
        assert_eq!(result["state"]["verified"], true);
        assert_eq!(result["launch_admitted"], true);
        assert_eq!(result["launch_accepted"], true);
        assert_eq!(result["ready"], true);
        for field in [
            "cleanup_complete",
            "original_runtime_retired",
            "original_pump_returned",
            "first_present_verified",
        ] {
            assert_eq!(result[field], true, "{result}");
        }
        evidence::runtime(&result, &request.images);
        let (level, marker) = if scenario == "save" {
            (LevelId::Cavern, 2)
        } else {
            (LevelId::Meadow, 1)
        };
        evidence::progress(&result, main(level), 30, &[marker], 1);
        assert!(
            input.restore(),
            "restore foreground after scripted {scenario}"
        );
        results.push(json!({"scenario": scenario, "result": result, "native_cleanup": native.cleanup_json()}));
    }
    results
}

fn main(level: LevelId) -> LevelKey {
    LevelKey::main(level)
}
fn running(record: &Value) -> bool {
    record["state"]["initialized"] == true
        && record["state"]["phase"] == "Running"
        && record["state"]["restoring"] == false
}
fn position(record: &Value) -> [f32; 2] {
    serde_json::from_value(record["state"]["player"].clone()).unwrap()
}

fn collision_sample(record: &Value) -> Value {
    json!({
        "player": record["state"]["player"],
        "tick": record["state"]["tick"],
        "logical_tick": record["state"]["logical_tick"],
        "logical_elapsed_nanos": record["state"]["logical_elapsed_nanos"],
        "held": record["state"]["held"],
        "current_instance": record["state"]["current_instance"],
        "presentation": record["presentation"],
    })
}

fn assert_checkpoint(checkpoint: &Checkpoint, expected: [(LevelKey, [f32; 2]); 3]) {
    checkpoint.validate().unwrap();
    assert_eq!(checkpoint.location.level, main(LevelId::Cavern));
    assert_eq!(checkpoint.location.anchor.as_str(), "checkpoint");
    assert_eq!(checkpoint.score, 30);
    assert_eq!(checkpoint.collected.len(), 3);
    assert_eq!(checkpoint.markers.len(), 3);
    assert_eq!(checkpoint.next_marker_id.get(), 4);
    for (index, (level, position)) in expected.into_iter().enumerate() {
        let pickups = checkpoint
            .collected
            .iter()
            .filter(|pickup| pickup.level == level)
            .collect::<Vec<_>>();
        assert_eq!(pickups.len(), 1);
        assert_eq!(pickups[0].entity.as_str(), "gem-near");
        let markers = checkpoint
            .markers
            .iter()
            .filter(|marker| marker.level == level)
            .collect::<Vec<_>>();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].id.get(), index as u64 + 1);
        assert_eq!(markers[0].position, position);
    }
}
