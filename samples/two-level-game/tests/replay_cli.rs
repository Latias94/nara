#![cfg(feature = "development")]

use std::{
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use two_level_game::{
    levels::{LevelId, LevelKey},
    replay::{decode_program, decode_recording},
};

#[path = "support/child.rs"]
mod child;

const RESULT_PREFIX: &str = "TWO_LEVEL_REPLAY_RESULT ";
const PROGRAM: &[u8] = include_bytes!("../replays/two-level.program.json");

struct Fixture {
    temporary_parent: PathBuf,
    directory: PathBuf,
    project: PathBuf,
    program: PathBuf,
    recording: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary_parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let directory = temporary_parent.join(format!(
            "two-level-replay-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let directory = fs::canonicalize(directory).unwrap();
        assert_eq!(directory.parent(), Some(temporary_parent.as_path()));
        assert!(!directory.starts_with(fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap()));
        let fixture = Self {
            project: directory.join("project"),
            program: directory.join("program.json"),
            recording: directory.join("recording.json"),
            temporary_parent,
            directory,
        };
        for relative in [
            "nara.toml",
            "input/explorer.actions.json",
            "scenes/meadow.scene.json",
            "scenes/cavern.scene.json",
            "scenes/marker.scene.json",
            "scenes/marker.scene.json.meta",
            "scenes/side-room.scene.json",
            "scenes/side-room.scene.json.meta",
            "prefabs/marker.prefab.json",
            "assets/textures/meadow.png",
            "assets/textures/meadow.png.meta",
            "assets/textures/cavern.png",
            "assets/textures/cavern.png.meta",
            "assets/textures/explorer.png",
            "assets/textures/explorer.png.meta",
            "assets/textures/marker.png",
            "assets/textures/marker.png.meta",
            "assets/textures/side-room.png",
            "assets/textures/side-room.png.meta",
            "assets/animations/explorer.sprite-animations.json",
            "assets/animations/explorer.sprite-animations.json.meta",
            "assets/audio/game.soundbank.json",
            "assets/audio/game.soundbank.json.meta",
            "assets/audio/pickup.wav",
            "assets/audio/travel.wav",
            "schema/authoring.bundle.json",
            "prefabs/README.md",
        ] {
            let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
            let target = fixture.project.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(source, target).unwrap();
        }
        fs::write(&fixture.program, PROGRAM).unwrap();
        fixture
    }

    fn command(&self, operation: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_replay"));
        command
            .arg(operation)
            .arg("--project-root")
            .arg(&self.project)
            .current_dir(&self.directory);
        command
    }

    fn record_command(&self) -> Command {
        let mut command = self.command("record");
        command
            .arg("--program")
            .arg(&self.program)
            .arg("--output")
            .arg(&self.recording);
        command
    }

    fn play_command(&self) -> Command {
        let mut command = self.command("play");
        command.arg("--recording").arg(&self.recording);
        command
    }

    fn invoke(&self, command: Command) -> (Output, Value) {
        let output = child::run(command, &self.directory, "source-game replay CLI");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let results = stdout
            .lines()
            .filter_map(|line| line.strip_prefix(RESULT_PREFIX))
            .collect::<Vec<_>>();
        assert_eq!(
            results.len(),
            1,
            "status={}; stdout={stdout}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_str(results.last().unwrap()).unwrap();
        assert_eq!(result["ok"], output.status.success(), "{result}");
        assert_eq!(result["cleanup_pending"], false, "{result}");
        if !output.status.success() {
            assert!(result.get("report").is_none(), "{result}");
            assert!(
                result["error"]["message"]
                    .as_str()
                    .is_some_and(|text| !text.is_empty())
            );
        }
        (output, result)
    }

    fn record(&self) -> Value {
        let (output, result) = self.invoke(self.record_command());
        assert!(output.status.success(), "{result}");
        result
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let Ok(directory) = fs::canonicalize(&self.directory) else {
            return;
        };
        if directory == self.directory
            && directory.parent() == Some(self.temporary_parent.as_path())
        {
            let _ = fs::remove_dir_all(directory);
        }
    }
}

#[test]
fn ordinary_process_records_gameplay_and_a_fresh_process_replays_every_result() {
    let fixture = Fixture::new();
    decode_program(PROGRAM).expect("the shipped finite program must pass production admission");
    let recorded = fixture.record();
    let recording = decode_recording(&fs::read(&fixture.recording).unwrap()).unwrap();
    let initial = &recording.initial;
    assert_eq!(
        initial.progress.location.level,
        LevelKey::main(LevelId::Meadow)
    );
    assert_eq!(initial.player.translation, [-120.0, 0.0]);
    assert_eq!(initial.progress.score, 0);
    assert_eq!(initial.held, [false; 4]);
    assert_eq!(initial.clock.ticks, 0);

    let steps = &recording.steps;
    assert_eq!(steps.len(), 67);
    let command_count = steps.iter().map(|step| step.commands.len()).sum::<usize>();
    assert_eq!(command_count, 20);
    for direction in 0..4 {
        assert!(steps.iter().any(|step| step.after.held[direction]));
    }
    let cavern = LevelKey::main(LevelId::Cavern);
    let instance = LevelKey::new(LevelId::Cavern, 1).unwrap();
    for key in [cavern, instance] {
        assert!(
            steps
                .iter()
                .any(|step| step.after.progress.location.level == key)
        );
    }
    let final_state = &steps.last().unwrap().after;
    assert_eq!(
        final_state.progress.location.level,
        LevelKey::main(LevelId::Meadow)
    );
    assert_ne!(final_state.player.translation, initial.player.translation);
    assert_eq!(final_state.progress.score, 30);
    assert_eq!(final_state.progress.collected.len(), 3);
    assert_eq!(final_state.progress.markers.len(), 3);
    for (index, key) in [LevelKey::main(LevelId::Meadow), cavern, instance]
        .into_iter()
        .enumerate()
    {
        let marker = &final_state.progress.markers[index];
        assert_eq!(marker.id.get(), index as u64 + 1);
        assert_eq!(marker.level, key);
        assert!(
            final_state
                .progress
                .collected
                .iter()
                .any(|pickup| pickup.level == key)
        );
    }
    assert_eq!(final_state.held, [false; 4]);
    assert_eq!(final_state.clock.ticks, steps.len() as u64 - 4);
    assert_eq!(final_state.counters.travel_count, 4);
    assert_eq!(final_state.counters.restarts, 1);
    assert_eq!(final_state.counters.saves, 0);
    assert_eq!(final_state.counters.restores, 0);
    assert_eq!(recorded["report"]["steps"], steps.len());
    assert_eq!(recorded["report"]["commands"], command_count);
    assert_eq!(
        recorded["report"]["final_state"],
        serde_json::to_value(final_state).unwrap()
    );

    let (output, replayed) = fixture.invoke(fixture.play_command());
    assert!(output.status.success(), "{replayed}");
    assert_eq!(replayed["operation"], "play");
    assert_eq!(replayed["report"], recorded["report"]);
}

#[test]
fn a_valid_recording_with_a_tampered_result_fails_playback_without_a_success_report() {
    let fixture = Fixture::new();
    fixture.record();
    let mut tampered: Value =
        serde_json::from_slice(&fs::read(&fixture.recording).unwrap()).unwrap();
    let x = &mut tampered["steps"][0]["after"]["player"]["translation"][0];
    *x = json!(x.as_f64().unwrap() + 1.0);
    let bytes = serde_json::to_vec(&tampered).unwrap();
    decode_recording(&bytes).expect("the tamper must reach actual playback, beyond wire admission");
    fs::write(&fixture.recording, bytes).unwrap();

    let (output, result) = fixture.invoke(fixture.play_command());
    assert!(!output.status.success(), "{result}");
    assert_eq!(result["operation"], "play");
    assert_eq!(result["error"]["stage"], "play");
}

#[test]
fn malformed_and_unsupported_programs_refuse_before_publishing_output() {
    let fixture = Fixture::new();
    for bytes in [b"{".to_vec(), vec![0xff], b"null".to_vec()] {
        fs::write(&fixture.program, bytes).unwrap();
        let (output, result) = fixture.invoke(fixture.record_command());
        assert!(!output.status.success(), "{result}");
        assert_eq!(result["error"]["stage"], "decode_program");
        assert!(!fixture.recording.exists());
    }
    for operation in ["save", "restore", "unknown"] {
        let mut program: Value = serde_json::from_slice(PROGRAM).unwrap();
        program["steps"][0] = json!([{ "operation": operation }]);
        fs::write(&fixture.program, serde_json::to_vec(&program).unwrap()).unwrap();
        let (output, result) = fixture.invoke(fixture.record_command());
        assert!(!output.status.success(), "{result}");
        assert_eq!(result["error"]["stage"], "decode_program");
        assert!(!fixture.recording.exists());
    }
    let mut program: Value = serde_json::from_slice(PROGRAM).unwrap();
    program["steps"][0][0]["direction"] = json!(4);
    fs::write(&fixture.program, serde_json::to_vec(&program).unwrap()).unwrap();
    let (output, result) = fixture.invoke(fixture.record_command());
    assert!(!output.status.success(), "{result}");
    assert_eq!(result["error"]["stage"], "decode_program");
    assert!(!fixture.recording.exists());

    fs::write(&fixture.recording, b"{").unwrap();
    let (output, result) = fixture.invoke(fixture.play_command());
    assert!(!output.status.success(), "{result}");
    assert_eq!(result["error"]["stage"], "decode_recording");
    assert_eq!(fs::read(&fixture.recording).unwrap(), b"{");
}

#[test]
fn both_file_inputs_have_a_bounded_sixteen_mebibyte_read() {
    let fixture = Fixture::new();
    let oversized = fixture.directory.join("oversized.json");
    File::create_new(&oversized)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    for operation in ["record", "play"] {
        let mut command = fixture.command(operation);
        if operation == "record" {
            command
                .arg("--program")
                .arg(&oversized)
                .arg("--output")
                .arg(&fixture.recording);
        } else {
            command.arg("--recording").arg(&oversized);
        }
        let (output, result) = fixture.invoke(command);
        assert!(!output.status.success(), "{result}");
        assert_eq!(result["error"]["stage"], "input");
        assert!(
            result["error"]["message"]
                .as_str()
                .unwrap()
                .contains("16 MiB")
        );
        assert!(!fixture.recording.exists());
    }
}

#[test]
fn all_paths_are_explicit_absolute_and_command_options_are_unambiguous() {
    let fixture = Fixture::new();
    let text = |value: &str| OsString::from(value);
    let root = fixture.project.clone().into_os_string();
    let program = fixture.program.clone().into_os_string();
    let recording = fixture.recording.clone().into_os_string();
    for arguments in [
        vec![],
        vec![
            text("record"),
            text("--program"),
            program.clone(),
            text("--output"),
            recording.clone(),
        ],
        vec![
            text("record"),
            text("--project-root"),
            root.clone(),
            text("--program"),
            program.clone(),
        ],
        vec![
            text("record"),
            text("--project-root"),
            root.clone(),
            text("--program"),
            program.clone(),
            text("--output"),
            text("relative.json"),
        ],
        vec![
            text("play"),
            text("--project-root"),
            text("project"),
            text("--recording"),
            recording.clone(),
        ],
        vec![
            text("play"),
            text("--project-root"),
            root.clone(),
            text("--recording"),
            text("relative.json"),
        ],
        vec![
            text("play"),
            text("--project-root"),
            root.clone(),
            text("--recording"),
            recording.clone(),
            text("--project-root"),
            root.clone(),
        ],
        vec![
            text("play"),
            text("--project-root"),
            root.clone(),
            text("--recording"),
            recording.clone(),
            text("--program"),
            program,
        ],
        vec![
            text("play"),
            text("--project-root"),
            root,
            text("--recording"),
            recording,
            text("extra"),
        ],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_replay"));
        command.args(arguments).current_dir(&fixture.directory);
        let (output, result) = fixture.invoke(command);
        assert!(!output.status.success(), "{result}");
        assert_eq!(result["error"]["stage"], "arguments");
        assert!(!fixture.recording.exists());
    }
}

#[test]
fn recording_never_overwrites_an_existing_output() {
    let fixture = Fixture::new();
    let original = b"retained existing recording\n";
    fs::write(&fixture.recording, original).unwrap();
    let (output, result) = fixture.invoke(fixture.record_command());
    assert!(!output.status.success(), "{result}");
    assert_eq!(result["error"]["stage"], "output");
    assert_eq!(fs::read(&fixture.recording).unwrap(), original);
    assert_eq!(fs::read(&fixture.program).unwrap(), PROGRAM);
}
