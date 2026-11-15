#[path = "../src/bin/player_desktop/arguments.rs"]
mod arguments;
#[path = "../src/bin/player_desktop/output.rs"]
#[allow(dead_code)]
mod desktop_output;
#[path = "../src/bin/player/output.rs"]
#[allow(dead_code)]
mod output;

use std::{ffi::OsString, path::PathBuf};

use two_level_game::Scenario;

fn absolute_path() -> PathBuf {
    std::env::temp_dir().join("two-level-desktop-checkpoint.json")
}

#[test]
fn ordinary_entry_remains_interactive_with_optional_observation_and_save_slot() {
    let options = arguments::parse([]).unwrap();
    assert_eq!(options.game.scenario, Scenario::Interactive);
    assert!(options.game.save_path.is_none());
    assert!(!options.observe && !options.verify_first_present);
    let path = absolute_path();
    for args in [
        vec![
            "--observe".into(),
            "--save".into(),
            path.clone().into_os_string(),
        ],
        vec![
            "--save".into(),
            path.clone().into_os_string(),
            "--observe".into(),
        ],
    ] {
        let options = arguments::parse(args).unwrap();
        assert_eq!(options.game.scenario, Scenario::Interactive);
        assert_eq!(options.game.save_path.as_ref(), Some(&path));
        assert!(options.observe && !options.verify_first_present);
    }
}

#[test]
fn finite_modes_are_explicit_and_accept_observation_in_either_order() {
    for args in [
        vec!["--verify-first-present", "--observe"],
        vec!["--observe", "--verify-first-present"],
    ] {
        let options = arguments::parse(args.into_iter().map(OsString::from)).unwrap();
        assert_eq!(options.game.scenario, Scenario::Interactive);
        assert!(options.observe && options.verify_first_present);
    }
    let options = arguments::parse(["--verify-travel".into()]).unwrap();
    assert_eq!(options.game.scenario, Scenario::Travel);
    assert!(!options.observe && !options.verify_first_present);
    for (flag, scenario) in [
        ("--verify-save", Scenario::Save),
        ("--verify-restore", Scenario::Restore),
    ] {
        for before in [true, false] {
            let mut args = vec![flag.into(), absolute_path().into_os_string()];
            args.insert(if before { 0 } else { 2 }, "--observe".into());
            let options = arguments::parse(args).unwrap();
            assert_eq!(options.game.scenario, scenario);
            assert_eq!(options.game.save_path, Some(absolute_path()));
            assert!(options.observe && !options.verify_first_present);
        }
    }
}

#[test]
fn unknown_duplicate_and_conflicting_modes_are_refused() {
    for args in [
        vec!["--pack", "content.pack"],
        vec!["--catalog-hash", "1234"],
        vec!["--project", "."],
        vec!["--help"],
        vec!["--observe", "--observe"],
        vec!["--verify-first-present", "--verify-first-present"],
        vec!["--verify-travel", "--verify-travel"],
        vec!["--verify-first-present", "--verify-travel"],
        vec!["--verify-travel", "--verify-first-present"],
        vec!["--verify-travel", "extra"],
        vec!["--save"],
        vec!["--verify-save"],
        vec!["--verify-restore"],
    ] {
        assert!(arguments::parse(args.into_iter().map(OsString::from)).is_err());
    }
    for args in [
        vec![
            "--save".into(),
            absolute_path().into_os_string(),
            "--verify-travel".into(),
        ],
        vec![
            "--verify-travel".into(),
            "--save".into(),
            absolute_path().into_os_string(),
        ],
        vec![
            "--verify-save".into(),
            absolute_path().into_os_string(),
            "--verify-restore".into(),
            absolute_path().into_os_string(),
        ],
        vec![
            "--save".into(),
            absolute_path().into_os_string(),
            "--save".into(),
            absolute_path().into_os_string(),
        ],
    ] {
        assert!(arguments::parse(args).is_err());
    }
}

#[test]
fn save_paths_require_bounded_absolute_utf8_file_names() {
    let root = absolute_path().ancestors().last().unwrap().to_path_buf();
    for path in [
        PathBuf::from("checkpoint.json"),
        root,
        PathBuf::from(format!("{}\0.json", absolute_path().display())),
        std::env::temp_dir().join(format!("{}.json", "界".repeat(1366))),
    ] {
        for flag in ["--save", "--verify-save", "--verify-restore"] {
            assert!(arguments::parse([flag.into(), path.clone().into_os_string()]).is_err());
        }
    }
    let prefix = std::env::temp_dir().join("two-level-desktop-path-boundary");
    let prefix_bytes = prefix.to_str().unwrap().len() + 1;
    let path = prefix.join(format!("界{}", "a".repeat(4096 - prefix_bytes - 3)));
    assert_eq!(path.to_str().unwrap().len(), 4096);
    assert!(arguments::parse(["--save".into(), path.clone().into_os_string()]).is_ok());
    assert!(arguments::parse(["--save".into(), format!("{}a", path.display()).into()]).is_err());
}

#[test]
fn desktop_records_are_sequenced_and_output_failure_is_fallible() {
    use std::io::{self, Write};

    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    let mut emitter = desktop_output::Emitter::default();
    let result = output::failure(Some(Scenario::Interactive), "original failure");
    assert!(
        emitter
            .emit_to(
                &mut Broken,
                desktop_output::Record::Observation,
                result.clone()
            )
            .is_err()
    );
    let mut bytes = Vec::new();
    for kind in [
        desktop_output::Record::Ready,
        desktop_output::Record::Result,
    ] {
        emitter.emit_to(&mut bytes, kind, result.clone()).unwrap();
    }
    let text = String::from_utf8(bytes).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    for (index, prefix) in ["TWO_LEVEL_DESKTOP_READY ", "TWO_LEVEL_DESKTOP_RESULT "]
        .into_iter()
        .enumerate()
    {
        let record: serde_json::Value =
            serde_json::from_str(lines[index].strip_prefix(prefix).unwrap()).unwrap();
        assert_eq!(record["sequence"], index + 1);
        assert_eq!(record["message"], "original failure");
    }
}

#[test]
fn observation_limits_reserve_a_final_result() {
    let mut emitter = desktop_output::Emitter::default();
    let record = serde_json::json!({"success": true});
    let mut writer = Vec::new();
    for _ in 0..desktop_output::MAX_RECORDS * 2 {
        emitter
            .emit_to(
                &mut writer,
                desktop_output::Record::Observation,
                record.clone(),
            )
            .unwrap();
    }
    emitter
        .emit_to(&mut writer, desktop_output::Record::Ready, record.clone())
        .unwrap();
    emitter
        .emit_to(&mut writer, desktop_output::Record::Result, record)
        .unwrap();
    let text = String::from_utf8(writer).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), desktop_output::MAX_RECORDS);
    for (index, line) in lines.iter().enumerate() {
        let (_, json) = line.split_once(' ').unwrap();
        let record: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(record["sequence"], index + 1);
        if index >= lines.len() - 2 {
            assert_eq!(record["observations_truncated"], true);
        }
    }
    let oversized = serde_json::json!({"message": "a".repeat(desktop_output::MAX_RECORD_BYTES)});
    assert!(
        desktop_output::Emitter::default()
            .emit_to(
                &mut Vec::new(),
                desktop_output::Record::Observation,
                oversized
            )
            .is_err()
    );
}

#[test]
fn observation_byte_allowance_leaves_ready_and_terminal_output_usable() {
    let mut emitter = desktop_output::Emitter::default();
    let mut writer = Vec::new();
    let record = serde_json::json!({"message": "x".repeat(desktop_output::MAX_RECORD_BYTES / 2)});
    for _ in 0..100 {
        emitter
            .emit_to(
                &mut writer,
                desktop_output::Record::Observation,
                record.clone(),
            )
            .unwrap();
    }
    assert!(writer.len() < desktop_output::MAX_STREAM_BYTES);
    emitter
        .emit_to(
            &mut writer,
            desktop_output::Record::Ready,
            serde_json::json!({}),
        )
        .unwrap();
    emitter
        .emit_to(
            &mut writer,
            desktop_output::Record::Result,
            serde_json::json!({}),
        )
        .unwrap();
    assert!(writer.len() <= desktop_output::MAX_STREAM_BYTES);
    let text = String::from_utf8(writer).unwrap();
    assert!(text.lines().count() < desktop_output::MAX_RECORDS);
    let terminal = text
        .lines()
        .last()
        .unwrap()
        .strip_prefix("TWO_LEVEL_DESKTOP_RESULT ")
        .unwrap();
    let record: serde_json::Value = serde_json::from_str(terminal).unwrap();
    assert_eq!(record["observations_truncated"], true);
}

#[cfg(all(
    feature = "desktop",
    feature = "cooked-player",
    not(nara_cooked_player)
))]
#[test]
fn ungenerated_desktop_player_refuses_without_claiming_native_presentation() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_player_desktop"))
        .output()
        .expect("run the ungenerated desktop Player");
    assert!(!result.status.success());
    let stdout = String::from_utf8(result.stdout).unwrap();
    let records = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("TWO_LEVEL_DESKTOP_RESULT "))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    let record: serde_json::Value = serde_json::from_str(records[0]).unwrap();
    assert_eq!(record["success"], false);
    assert_eq!(record["scenario"], "interactive");
    assert_eq!(record["first_present_verified"], false);
    assert_eq!(record["original_runtime_retired"], false);
    assert_eq!(record["original_pump_returned"], false);
    assert_eq!(record["presented_frames"], 0);
    assert!(record["presentation"].is_null());
    assert!(
        record["message"]
            .as_str()
            .unwrap()
            .contains("without a generated cooked-content expectation")
    );
}
