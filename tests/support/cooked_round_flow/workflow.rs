use std::{
    ffi::OsString,
    fs,
    num::NonZeroUsize,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::project_tooling::process::SupervisedProcessCommand;
use serde_json::{Value, json};
use two_level_game::{
    levels::{LevelId, LevelKey},
    save::{CURRENT_FORMAT_VERSION, MAX_CHECKPOINT_BYTES, decode_checkpoint},
};

use super::{
    OUTPUT_BYTES, REPORT, Request, evidence,
    process::{NativeProcess, OBSERVATION, POLL, READY, RESULT, read_bounded},
    window::{self, Input},
};

struct Player<'a> {
    input: Input,
    native: Option<NativeProcess>,
    request: &'a Request,
    sequence: u64,
    ready: Option<Value>,
    latest: Option<Value>,
    result: Option<Value>,
    resume_boundary: Option<Value>,
}

impl<'a> Player<'a> {
    fn spawn(request: &'a Request, executable: &Path, arguments: Vec<OsString>) -> Self {
        assert!(!request.unavailable_source.exists() && !request.unavailable_cache.exists());
        let input = Input::new(&request.title);
        let command = SupervisedProcessCommand::new(executable, &request.cwd)
            .arguments(arguments)
            .environment("PATH", &request.invalid_path)
            .environment("LOCALAPPDATA", &request.local_data)
            .environment("APPDATA", &request.roaming_data)
            .environment("ProgramData", &request.common_data)
            .environment("ALLUSERSPROFILE", &request.common_data)
            .environment("WGPU_BACKEND", "dx12")
            .environment("NARA_WGPU_FORCE_FALLBACK", "0")
            .bounded_stdout(NonZeroUsize::new(OUTPUT_BYTES).unwrap());
        Self {
            input,
            native: Some(NativeProcess::spawn(&command, Some(request.title.clone()))),
            request,
            sequence: 0,
            ready: None,
            latest: None,
            result: None,
            resume_boundary: None,
        }
    }

    fn native(&mut self) -> &mut NativeProcess {
        self.native.as_mut().unwrap()
    }

    fn poll(&mut self) {
        self.native().poll();
        for (kind, record) in self.native().records() {
            assert!(
                self.result.is_none(),
                "evidence followed the original terminal result"
            );
            assert_eq!(evidence::number(&record, "sequence"), self.sequence + 1);
            self.sequence += 1;
            assert!(self.sequence <= 1024);
            if record["success"] == true {
                evidence::presented(&record, self.request, self.latest.as_ref());
            }
            if record["flow"]["phase"] == "Resuming" && self.resume_boundary.is_none() {
                self.resume_boundary = Some(record.clone());
            }
            match kind {
                READY => {
                    assert!(self.ready.is_none(), "one original READY record");
                    assert_eq!(record["ready"], true);
                    self.ready = Some(record.clone());
                }
                RESULT => self.result = Some(record.clone()),
                OBSERVATION => {}
                _ => unreachable!(),
            }
            if kind != RESULT {
                for field in [
                    "cleanup_complete",
                    "original_runtime_retired",
                    "original_pump_returned",
                ] {
                    assert_eq!(record[field], false);
                }
            }
            self.latest = Some(record);
        }
    }

    fn snapshot(&self) -> Value {
        self.latest
            .as_ref()
            .expect("original presented Player state")
            .clone()
    }

    fn wait_ready(&mut self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            self.poll();
            if let Some(ready) = &self.ready {
                return ready.clone();
            }
            assert!(
                !self.native().complete() && Instant::now() < deadline,
                "Player never presented its initial Title: {}",
                self.native().diagnostics()
            );
            thread::sleep(POLL);
        }
    }

    fn wait_focused(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            self.poll();
            assert!(
                !self.native().complete() && Instant::now() < deadline,
                "the original Player never became visible and acknowledged foreground Input: {}",
                self.native().diagnostics()
            );
            if self.input.try_focus()
                && self
                    .latest
                    .as_ref()
                    .is_some_and(|record| record["flow"]["input_focused"] == true)
            {
                self.input.check();
                return;
            }
            thread::sleep(POLL);
        }
    }

    #[track_caller]
    fn wait_after(&mut self, basis: &Value, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.poll();
            if let Some(result) = &self.result {
                panic!(
                    "original Player exited before the expected round transition: success={} message={} gameplay_error={}",
                    result["success"], result["message"], result["gameplay_error"],
                );
            }
            if let Some(record) = &self.latest
                && predicate(record)
                && ["app_frame", "render_frame"].into_iter().all(|field| {
                    evidence::number(&record["presentation"], field)
                        > evidence::number(&basis["presentation"], field)
                })
            {
                return record.clone();
            }
            if self.native().complete() || Instant::now() >= deadline {
                let diagnostics = self.native().diagnostics();
                let latest = self.latest.as_ref().unwrap_or(basis);
                panic!(
                    "physical round transition never reached a newer original presentation; basis_frame={} latest_frame={} latest_phase={} latest_round={} latest_tick={} input_focused={}; {diagnostics}",
                    basis["presentation"]["app_frame"],
                    latest["presentation"]["app_frame"],
                    latest["flow"]["phase"],
                    latest["flow"]["round"],
                    latest["state"]["tick"],
                    latest["flow"]["input_focused"],
                );
            }
            thread::sleep(POLL);
        }
    }

    fn pump_for(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.input.check();
            self.poll();
            assert!(
                !self.native().complete(),
                "ordinary Player exited before physical X: {}",
                self.native().diagnostics()
            );
            thread::sleep(POLL);
        }
    }

    #[track_caller]
    fn tap(&mut self, scan: u16, predicate: impl Fn(&Value) -> bool) -> Value {
        let basis = self.snapshot();
        self.input.press(scan);
        let presented = self.wait_after(&basis, predicate);
        self.input.release(scan);
        presented
    }

    fn finish(&mut self, success: bool) -> (Value, Value) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            self.poll();
            if self.native().complete() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "original Player did not physically retire: {}",
                self.native().diagnostics()
            );
            thread::sleep(POLL);
        }
        self.input.finish().unwrap();
        self.native().assert_clean();
        assert_eq!(
            self.native().receipt().exit().success(),
            success,
            "{}",
            self.native().diagnostics()
        );
        let result = self
            .result
            .clone()
            .expect("one original terminal Player record");
        let cleanup = self.native().cleanup_json();
        self.native.take();
        (result, cleanup)
    }
}

impl Drop for Player<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.input.finish() {
            eprintln!("round input retirement: {error}");
        }
        if let Some(native) = self.native.as_mut()
            && !native.retire_normally(Duration::from_secs(30))
        {
            let message = native.diagnostics().chars().take(8192).collect::<String>();
            if let Err(error) = fs::write(&self.request.unresolved_cleanup, &message) {
                eprintln!("could not preserve unresolved original cleanup marker: {error}");
            }
            eprintln!("retaining original round Player custody: {message}");
            std::mem::forget(self.native.take());
        }
    }
}

fn interactive_arguments(request: &Request) -> Vec<OsString> {
    vec![
        "--observe".into(),
        "--save".into(),
        request.save.as_os_str().to_owned(),
    ]
}

pub fn run(request: &Request) {
    let mut player = Player::spawn(request, &request.executable, interactive_arguments(request));
    let pid = player.native().pid();
    let title = player.wait_ready();
    evidence::initial_title(&title, true);
    assert!(!request.save.exists());
    let window = player.native().bind_window();
    player.input.bind(window);
    player.wait_focused();
    let started = player.tap(window::ENTER, |record| {
        evidence::shown(record, "Playing", 1) && record["launch_accepted"] == true
    });
    evidence::assert_shown(&started, "Playing", 1, 0, [false, false]);
    assert_ne!(
        started["current_receipt"]["instance_id"],
        title["current_receipt"]["instance_id"]
    );
    assert_eq!(
        started["current_receipt"]["domain_id"],
        title["current_receipt"]["domain_id"]
    );
    assert_eq!(started["state"]["saves"], 0);
    assert!(evidence::number(&started["state"], "tick") > 0);
    player.input.press(window::D);
    let meadow = player.wait_after(&started, |record| {
        evidence::shown(record, "Playing", 1) && record["score"] == 10
    });
    evidence::assert_shown(&meadow, "Playing", 1, 10, [true, false]);
    assert!(position(&meadow)[0] > position(&started)[0]);
    assert_eq!(meadow["state"]["held"], json!([false, true, false, false]));
    let paused = player.tap(window::ESCAPE, |record| {
        evidence::shown(record, "Paused", 1)
    });
    evidence::assert_shown(&paused, "Paused", 1, 10, [true, false]);
    assert_eq!(paused["state"]["held"], json!([false, false, false, false]));
    assert!(player.input.down(window::D));
    player.pump_for(Duration::from_millis(350));
    assert!(player.input.down(window::D));
    player.input.release(window::D);
    player.resume_boundary = None;
    let resumed = player.tap(window::ENTER, |record| {
        evidence::shown(record, "Playing", 1)
    });
    let boundary = player
        .resume_boundary
        .clone()
        .expect("actual first Resuming presentation after the paused hold");
    evidence::frozen(&paused, &boundary);
    evidence::assert_shown(&resumed, "Playing", 1, 10, [true, false]);
    assert_eq!(
        resumed["state"]["held"],
        json!([false, false, false, false])
    );
    assert_eq!(position(&resumed), position(&paused));
    player.pump_for(Duration::from_millis(350));
    let saved = player.tap(window::K, |record| {
        evidence::shown(record, "Playing", 1) && record["state"]["saves"] == 1
    });
    evidence::assert_shown(&saved, "Playing", 1, 10, [true, false]);
    assert_eq!(saved["launch_accepted"], true);
    assert_eq!(
        position(&saved),
        position(&resumed),
        "released D must not move after Resume"
    );
    assert_eq!(saved["state"]["held"], json!([false, false, false, false]));
    assert!(
        evidence::number(&saved["state"], "logical_tick")
            >= evidence::number(&resumed["state"], "logical_tick") + 5
    );
    let checkpoint_bytes = read_bounded(&request.save, MAX_CHECKPOINT_BYTES);
    let decoded = decode_checkpoint(&checkpoint_bytes).unwrap();
    assert!(decoded.migrated_from.is_none());
    let checkpoint = decoded.checkpoint;
    assert_eq!(checkpoint.location.level, LevelKey::main(LevelId::Meadow));
    assert_eq!(checkpoint.location.anchor.as_str(), "checkpoint");
    assert_eq!(checkpoint.score, 10);
    assert_eq!(checkpoint.collected.len(), 1);
    assert_eq!(
        checkpoint.collected[0].level,
        LevelKey::main(LevelId::Meadow)
    );
    assert_eq!(checkpoint.collected[0].entity.as_str(), "gem-near");
    assert!(checkpoint.markers.is_empty());
    assert_eq!(checkpoint.next_marker_id.get(), 1);
    let envelope: Value = serde_json::from_slice(&checkpoint_bytes).unwrap();
    assert_eq!(envelope["format_version"], CURRENT_FORMAT_VERSION);
    player.input.press(window::D);
    let fresh_d = player.wait_after(&saved, |record| {
        evidence::shown(record, "Playing", 1)
            && record["state"]["held"] == json!([false, true, false, false])
            && evidence::number(&record["state"], "logical_tick")
                > evidence::number(&saved["state"], "logical_tick")
    });
    assert!(position(&fresh_d)[0] >= position(&saved)[0]);
    player.input.release(window::D);
    let released = player.wait_after(&fresh_d, |record| {
        record["state"]["held"] == json!([false, false, false, false])
    });
    evidence::assert_shown(&released, "Playing", 1, 10, [true, false]);
    let cavern = player.tap(window::E, |record| {
        evidence::shown(record, "Playing", 1)
            && record["level"] == json!(LevelKey::main(LevelId::Cavern))
            && record["current_receipt"]["instance_id"] != saved["current_receipt"]["instance_id"]
    });
    evidence::assert_shown(&cavern, "Playing", 1, 10, [true, false]);
    assert_eq!(cavern["state"]["travel_count"], 1);
    assert_eq!(cavern["state"]["held"], json!([false, false, false, false]));
    player.input.press(window::D);
    let complete = player.wait_after(&cavern, |record| evidence::shown(record, "Complete", 1));
    evidence::assert_shown(&complete, "Complete", 1, 20, [true, true]);
    assert!(position(&complete)[0] > position(&cavern)[0]);
    player.input.release(window::D);
    let restarted = player.tap(window::ENTER, |record| {
        evidence::shown(record, "Playing", 2)
    });
    evidence::assert_shown(&restarted, "Playing", 2, 0, [false, false]);
    assert_eq!(restarted["level"], json!(LevelKey::main(LevelId::Meadow)));
    assert_ne!(
        restarted["current_receipt"]["instance_id"],
        complete["current_receipt"]["instance_id"]
    );
    assert_eq!(
        restarted["current_receipt"]["domain_id"],
        complete["current_receipt"]["domain_id"]
    );
    for field in ["travel_count", "saves", "restores"] {
        assert_eq!(restarted["state"][field], 0);
    }
    assert_eq!(
        restarted["state"]["held"],
        json!([false, false, false, false])
    );
    let paused_again = player.tap(window::ESCAPE, |record| {
        evidence::shown(record, "Paused", 2)
    });
    evidence::assert_shown(&paused_again, "Paused", 2, 0, [false, false]);
    let returned = player.tap(window::T, |record| evidence::shown(record, "Title", 2));
    evidence::assert_shown(&returned, "Title", 2, 0, [false, false]);
    evidence::frozen(&paused_again, &returned);
    player.input.press(window::X);
    let (result, cleanup) = player.finish(true);
    evidence::terminal(&result, request, true);
    assert_eq!(result["launch_accepted"], true);
    assert_eq!(
        read_bounded(&request.save, MAX_CHECKPOINT_BYTES),
        checkpoint_bytes
    );
    let finite = finite_first_present(request, pid);
    let scripted = refused(
        request,
        &request.executable,
        if request.runtime == "audio-desktop" {
            "--verify"
        } else {
            "--verify-travel"
        },
    );
    assert!(
        scripted["result"]["message"]
            .as_str()
            .unwrap()
            .contains("round")
    );
    let mismatch = refused(request, &request.mismatched_executable, "--observe");
    assert!(
        mismatch["result"]["message"]
            .as_str()
            .unwrap()
            .contains("Runtime catalog differs from the Host expectation")
    );
    assert!(!request.unresolved_cleanup.exists());
    assert_eq!(
        read_bounded(&request.save, MAX_CHECKPOINT_BYTES),
        checkpoint_bytes
    );
    println!(
        "\n{REPORT}{}",
        json!({
            "success": true, "runtime": request.runtime, "physical_round_processes": 1,
            "finite_processes": 1, "refused_processes": 2,
            "checkpoints": {"title": title, "started": started, "meadow_pickup": meadow,
                "paused_while_d_held": paused, "resume_boundary": boundary, "resumed": resumed,
                "stationary_saved": saved, "fresh_d": fresh_d, "cavern": cavern,
                "complete": complete, "restarted": restarted, "returned_to_title": returned},
            "checkpoint": checkpoint, "result": result, "cleanup": cleanup,
            "finite": finite, "scripted_refusal": scripted, "catalog_refusal": mismatch,
            "scope": "original native DX12 and UI submissions, physical keys and original cleanup; no screenshot, scanout, PCM or audibility claim",
        })
    );
}

fn finite_first_present(request: &Request, previous_pid: u32) -> Value {
    let desktop = request.runtime == "desktop";
    let arguments = if desktop {
        vec!["--observe".into(), "--verify-first-present".into()]
    } else {
        interactive_arguments(request)
    };
    let mut player = Player::spawn(request, &request.executable, arguments);
    assert_ne!(player.native().pid(), previous_pid);
    let ready = player.wait_ready();
    evidence::initial_title(&ready, !desktop);
    if !desktop {
        player.native().bind_window();
        player.native().close_window();
    }
    let (result, cleanup) = player.finish(true);
    evidence::initial_title(&result, !desktop);
    evidence::terminal(&result, request, false);
    json!({"ready": ready, "result": result, "cleanup": cleanup,
        "mechanism": if desktop { "--verify-first-present" } else { "initial Title followed by owned WM_CLOSE" }})
}

fn refused(request: &Request, executable: &Path, argument: &str) -> Value {
    let mut player = Player::spawn(request, executable, vec![argument.into()]);
    let (result, cleanup) = player.finish(false);
    assert!(player.ready.is_none());
    assert_eq!(player.sequence, 1);
    evidence::refused(&result);
    json!({"argument": argument, "result": result, "cleanup": cleanup})
}

fn position(record: &Value) -> [f32; 2] {
    serde_json::from_value(record["state"]["player"].clone()).unwrap()
}
