#[path = "../src/bin/player/arguments.rs"]
mod arguments;
#[path = "../src/bin/player/output.rs"]
#[allow(dead_code)]
mod output;

#[test]
fn result_contract_uses_lowercase_scenarios_and_reports_broken_output_without_panicking() {
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
    let result = output::failure(Some(two_level_game::Scenario::Travel), "original failure");
    assert_eq!(result["scenario"], "travel");
    assert!(output::emit_to(&mut Broken, &mut Vec::new(), &result).is_err());
    assert!(output::diagnostic_to(&mut Broken, "Runtime failure diagnostics").is_err());
    let mut record = Vec::new();
    assert!(output::emit_to(&mut record, &mut Broken, &result).is_err());
    assert!(
        String::from_utf8(record)
            .unwrap()
            .contains("TWO_LEVEL_PLAYER_RESULT ")
    );
}

#[cfg(all(feature = "cooked-player", not(nara_cooked_player)))]
#[test]
fn ungenerated_player_refuses_execution_with_a_truthful_result() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_player"))
        .output()
        .expect("run the ordinary ungenerated Player");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("without a generated cooked-content expectation")
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let markers = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("TWO_LEVEL_PLAYER_RESULT "))
        .collect::<Vec<_>>();
    assert_eq!(markers.len(), 1);
    let result: serde_json::Value = serde_json::from_str(markers[0]).unwrap();
    assert_eq!(result["success"], false);
    assert_eq!(result["scenario"], "interactive");
    assert_eq!(result["source_free"], true);
    assert_eq!(result["cleanup_complete"], true);
    assert!(result["startup_receipt"].is_null());
    assert!(result["current_receipt"].is_null());
    assert!(
        result["observed_image_products"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
