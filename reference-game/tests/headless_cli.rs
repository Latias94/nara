use std::process::Command;

#[test]
fn bundled_cli_emits_one_stable_json_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_headless"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        concat!(
            "{\"schema\":\"nara-reference-game.wave-summary-v1\",",
            "\"outcome\":\"completed\",\"tick\":50,\"score\":300,",
            "\"player_hit_points\":20,\"enemies_remaining\":0,",
            "\"projectiles_remaining\":4}\n"
        )
    );
}

#[test]
fn opt_in_startup_marker_precedes_the_terminal_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_headless"))
        .env("NARA_REFERENCE_GAME_STARTUP_MARKER", "1")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "{\"schema\":\"nara-reference-game.startup-marker-v1\",",
            "\"event\":\"headless_first_authoritative_tick\"}\n",
            "{\"schema\":\"nara-reference-game.wave-summary-v1\",",
            "\"outcome\":\"completed\",\"tick\":50,\"score\":300,",
            "\"player_hit_points\":20,\"enemies_remaining\":0,",
            "\"projectiles_remaining\":4}\n"
        )
    );
}

#[test]
fn tick_limit_is_a_failure_without_a_success_snapshot() {
    let output = Command::new(env!("CARGO_BIN_EXE_headless"))
        .args(["--max-ticks", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "project.run.tick-limit: Headless project run reached its fixed-tick limit\n"
    );
}

#[test]
fn cli_rejects_external_scenario_paths_without_echoing_them() {
    let canary = "C:/private/credential-scenario.json";
    let output = Command::new(env!("CARGO_BIN_EXE_headless"))
        .args(["--scenario", canary])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr,
        "reference-game.cli.invalid-arguments: Headless arguments are invalid\n"
    );
    assert!(!stderr.contains(canary));
}

#[test]
fn cli_rejects_a_tick_limit_above_its_product_work_budget() {
    let output = Command::new(env!("CARGO_BIN_EXE_headless"))
        .args(["--max-ticks", "257"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "reference-game.cli.invalid-arguments: Headless arguments are invalid\n"
    );
}
