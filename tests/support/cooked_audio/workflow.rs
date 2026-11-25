use std::{
    fs,
    num::NonZeroUsize,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::project_tooling::process::SupervisedProcessCommand;
use serde_json::{Value, json};
use two_level_game::levels::{LevelId, LevelKey};

use super::{
    OUTPUT_BYTES, REPORT, Request, evidence,
    process::{NativeProcess, OBSERVATION, POLL, READY, RESULT},
    window::{self, Input},
};

struct Player<'a> {
    request: &'a Request,
    native: Option<NativeProcess>,
    sequence: u64,
    ready: Option<Value>,
    latest: Option<Value>,
    result: Option<Value>,
}

impl<'a> Player<'a> {
    fn spawn(request: &'a Request, executable: &Path, arguments: &[&str]) -> Self {
        let command = SupervisedProcessCommand::new(executable, &request.cwd)
            .arguments(arguments.iter().copied())
            .environment("PATH", &request.invalid_path)
            .environment("LOCALAPPDATA", &request.local_data)
            .environment("APPDATA", &request.roaming_data)
            .environment("ProgramData", &request.common_data)
            .environment("ALLUSERSPROFILE", &request.common_data)
            .environment("WGPU_BACKEND", "dx12")
            .environment("NARA_WGPU_FORCE_FALLBACK", "0")
            .bounded_stdout(NonZeroUsize::new(OUTPUT_BYTES).unwrap());
        Self {
            request,
            native: Some(NativeProcess::spawn(&command, Some(request.title.clone()))),
            sequence: 0,
            ready: None,
            latest: None,
            result: None,
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
                evidence::selection(&record, self.request);
            }
            match kind {
                READY => {
                    assert!(self.ready.is_none());
                    self.ready = Some(record.clone());
                }
                OBSERVATION => {}
                RESULT => self.result = Some(record.clone()),
                _ => unreachable!(),
            }
            self.latest = Some(record);
        }
    }

    fn wait(&mut self, timeout: Duration, mut predicate: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + timeout;
        loop {
            self.poll();
            if predicate(self) {
                return;
            }
            assert!(
                !self.native().complete() && Instant::now() < deadline,
                "original cooked-audio Player did not reach its expected state: {}",
                self.native().diagnostics()
            );
            thread::sleep(POLL);
        }
    }

    fn ready(&mut self, input: &mut Input) {
        self.wait(Duration::from_secs(60), |player| player.ready.is_some());
        evidence::presented(self.ready.as_ref().unwrap(), self.request);
        assert!(
            !self.native().complete(),
            "ordinary original window must still be live"
        );
        input.bind(self.native().bind_window());
    }

    fn finish(&mut self, success: bool) -> (Value, Value) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            self.poll();
            if self.native().complete() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "original Player did not physically finish: {}",
                self.native().diagnostics()
            );
            thread::sleep(POLL);
        }
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
        (result, self.native().cleanup_json())
    }
}

impl Drop for Player<'_> {
    fn drop(&mut self) {
        if let Some(native) = self.native.as_mut()
            && !native.retire_normally(Duration::from_secs(30))
        {
            let diagnostic = native.diagnostics();
            let message = diagnostic.chars().take(8192).collect::<String>();
            if let Err(error) = fs::write(&self.request.unresolved_cleanup, message.as_bytes()) {
                eprintln!("unresolved original Player custody marker failed: {error}");
            }
            eprintln!("Player cleanup remained incomplete: {message}");
            drop(self.native.take());
        }
    }
}

fn verified(request: &Request) -> Value {
    let input = Input::new(&request.title);
    let mut player = Player::spawn(request, &request.executable, &["--verify"]);
    let mut input = input;
    player.ready(&mut input);
    let (result, cleanup) = player.finish(true);
    evidence::verified(&result, request);
    assert!(
        input.restore(),
        "restore original foreground after the complete Travel script"
    );
    json!({"result": result, "native_cleanup": cleanup})
}

fn physical_close(request: &Request) -> Value {
    let input = Input::new(&request.title);
    let mut player = Player::spawn(request, &request.executable, &["--observe"]);
    let mut input = input;
    player.ready(&mut input);
    assert_eq!(player.ready.as_ref().unwrap()["scenario"], "interactive");
    assert_eq!(player.ready.as_ref().unwrap()["score"], 0);
    input.press(window::D);
    player.wait(Duration::from_secs(15), |player| {
        input.check();
        player
            .latest
            .as_ref()
            .is_some_and(|record| record["score"] == 10)
    });
    input.release();
    player.wait(Duration::from_secs(15), |player| {
        input.check();
        player
            .latest
            .as_ref()
            .is_some_and(|record| record["state"]["held"] == json!([false, false, false, false]))
    });
    let before = player.latest.as_ref().unwrap()["state"]["current_instance"].clone();
    input.press(window::E);
    let deadline = Instant::now() + Duration::from_millis(40);
    while Instant::now() < deadline {
        input.check();
        player.poll();
        assert!(!player.native().complete());
        thread::sleep(POLL);
    }
    input.release();
    player.wait(Duration::from_secs(15), |player| {
        input.check();
        player.latest.as_ref().is_some_and(|record| {
            record["level"] == json!(LevelKey::main(LevelId::Cavern))
                && record["state"]["current_instance"] != before
                && record["state"]["travel_count"] == 1
        })
    });
    evidence::presented(player.latest.as_ref().unwrap(), request);
    player.native().close_window();
    let (result, cleanup) = player.finish(true);
    evidence::retired(&result, request);
    assert_eq!(result["scenario"], "interactive");
    assert_eq!(result["score"], 10);
    assert_eq!(result["collected"], 1);
    assert_eq!(result["state"]["travel_count"], 1);
    assert_eq!(result["state"]["restarts"], 0);
    assert_eq!(result["audio"]["verification_requested"], false);
    assert!(result["audio"]["pcm"].is_null());
    assert_eq!(
        evidence::accepted_causes(&result["audio"]["snapshot"]),
        vec![
            json!({"kind": "pickup", "level": LevelKey::main(LevelId::Meadow), "entity": "gem-near"}),
            json!({"kind": "travel", "level": LevelKey::main(LevelId::Cavern)}),
        ]
    );
    assert!(
        input.restore(),
        "restore original foreground after owned WM_CLOSE"
    );
    json!({"result": result, "native_cleanup": cleanup})
}

fn refused(request: &Request, executable: &Path, reason: &str) -> Value {
    let input = Input::new(&request.title);
    let mut player = Player::spawn(request, executable, &["--verify"]);
    let mut input = input;
    let (result, cleanup) = player.finish(false);
    assert!(player.ready.is_none());
    evidence::refused(&result, reason);
    assert!(
        input.restore(),
        "restore original foreground after pre-publication refusal"
    );
    json!({"case": reason, "result": result, "native_cleanup": cleanup})
}

pub fn run(request: &Request) {
    let mismatched = refused(
        request,
        &request.mismatched_executable,
        "compiled_catalog_expectation_mismatch",
    );
    let corrupt = refused(
        request,
        &request.corrupt_executable,
        "corrupt_sound_bank_member",
    );
    let verified = verified(request);
    let physical = physical_close(request);
    assert_ne!(
        verified["native_cleanup"]["pid"],
        physical["native_cleanup"]["pid"]
    );
    assert!(!request.unresolved_cleanup.exists());
    assert!(!request.unavailable_source.exists() && !request.unavailable_cache.exists());
    println!(
        "\n{REPORT}{}",
        json!({
            "success": true, "verified_processes": 1, "physical_close_processes": 1,
            "refused_processes": 2, "verified": verified, "physical_close": physical,
            "refusals": [mismatched, corrupt], "source_and_cache_paths_unavailable": true,
        })
    );
}
