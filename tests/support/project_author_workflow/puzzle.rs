use std::{fs, path::Path, time::Instant};

#[path = "puzzle/workflow/mod.rs"]
mod workflow;

use super::process::Output;
use workflow::{Project, editor, source};

const SCENE: &str = "scenes/startup.scene.json";
const DESKTOP_UNSUPPORTED_KEY: char = 'é';
const RETAINED_STARTER_FILES: [&str; 9] = [
    "nara.toml",
    "Cargo.toml",
    "Cargo.lock",
    "README.md",
    "scenes/startup.scene.json.meta",
    "build.rs",
    "src/bin/desktop.rs",
    "src/bin/cook.rs",
    "src/bin/player.rs",
];
const PUZZLE_FILES: [&str; 6] = [
    SCENE,
    source::SCHEMA,
    "src/lib.rs",
    "src/gameplay.rs",
    "src/proof.rs",
    "src/bin/headless.rs",
];

pub fn accept() {
    let mut project = Project::create();
    let lockfile = project.root.join("Cargo.lock");
    assert!(!lockfile.exists());
    project.cli("check", &[]).assert_success();
    assert!(lockfile.is_file());
    let starter_headless = project.build("headless", "starter-headless", &[]);
    let starter = project.run(&starter_headless, |command| command);
    assert_line(&starter, "starter_headless: ok fixed_ticks=1");
    assert!(source::observed_snapshots(&starter.stdout).is_empty());

    let probe_path = project.root.join("src/bin/starter_probe.rs");
    assert!(!probe_path.exists());
    fs::write(&probe_path, include_bytes!("puzzle/game/starter_probe.rs")).unwrap();
    let movement_probe = project.build("starter_probe", "starter-probe", &[]);
    let movement = project.run(&movement_probe, |command| command);
    assert_line(
        &movement,
        "STARTER_MOVEMENT_OK continuous_ticks=2,3 held_then_released=true confirm_resets=true",
    );
    fs::remove_file(probe_path).unwrap();

    let starter_files = snapshot(&project.root, RETAINED_STARTER_FILES);
    let initial_input = fs::read(project.root.join(source::INPUT_MAP)).unwrap();
    source::assert_default_directions(&initial_input);
    let initial_schema = fs::read(project.root.join(source::SCHEMA)).unwrap();

    let game_edit = Instant::now();
    source::install_puzzle(&project.root);
    let stale = project.cli("schema", &["--check"]);
    assert!(
        !stale.receipt.exit().success() && stale.stderr.contains("missing or stale"),
        "stdout={}\nstderr={}",
        stale.stdout,
        stale.stderr
    );
    assert_eq!(
        fs::read(project.root.join(source::SCHEMA)).unwrap(),
        initial_schema
    );
    project.cli("schema", &["--write"]).assert_success();
    project.cli("schema", &["--check"]).assert_success();
    project.cli("check", &[]).assert_success();
    source::assert_puzzle_schema(&project.root);

    let puzzle_headless = project.build("headless", "puzzle-headless", &[]);
    let baseline = run_source(&mut project, &puzzle_headless, "keyboard");
    source::assert_snapshots(&baseline.stdout, Some(2), 4);
    let game_edit_to_result = game_edit.elapsed();

    let scene_edit = editor::move_pressure_plate(&project.root);
    let edited = run_source(&mut project, &puzzle_headless, "keyboard");
    source::assert_snapshots(&edited.stdout, Some(6), 4);
    let scene_edit_to_result = scene_edit.elapsed();
    assert_eq!(
        fs::read(project.root.join(source::INPUT_MAP)).unwrap(),
        initial_input
    );
    let scene_path = project.root.join(SCENE);
    let saved_scene = fs::read(&scene_path).unwrap();
    fs::write(&scene_path, source::blocked_door_scene(&saved_scene)).unwrap();
    let blocked = run_source(&mut project, &puzzle_headless, "keyboard");
    source::assert_snapshots(&blocked.stdout, None, 2);
    fs::write(&scene_path, &saved_scene).unwrap();

    let input_path = project.root.join(source::INPUT_MAP);
    fs::write(
        &input_path,
        source::bind_keyboard_right_to_up(&initial_input),
    )
    .unwrap();
    let simultaneous = run_source(&mut project, &puzzle_headless, "keyboard");
    assert_line(
        &simultaneous,
        "push_proof tick=2 actor=(1,0) box=(2,0) door_open=false won=false",
    );
    assert_line(
        &simultaneous,
        "push_proof tick=3 actor=(1,1) box=(2,0) door_open=false won=false",
    );
    fs::write(&input_path, &initial_input).unwrap();

    let puzzle_files = snapshot(&project.root, PUZZLE_FILES);
    let input_edit = source::remove_keyboard_right(&project.root);
    let rebound = run_source(&mut project, &puzzle_headless, "gamepad");
    assert_eq!(
        source::observed_snapshots(&rebound.stdout),
        source::observed_snapshots(&edited.stdout)
    );
    let input_edit_to_result = input_edit.elapsed();
    let unbound = run_source(&mut project, &puzzle_headless, "keyboard");
    source::assert_snapshots(&unbound.stdout, None, 0);
    project.cli("check", &[]).assert_success();
    assert_eq!(snapshot(&project.root, PUZZLE_FILES), puzzle_files);
    assert_eq!(
        snapshot(&project.root, RETAINED_STARTER_FILES),
        starter_files
    );

    fs::write(
        &input_path,
        source::rebind_keyboard_right(&initial_input, DESKTOP_UNSUPPORTED_KEY),
    )
    .unwrap();
    let unicode_keyboard = run_source(&mut project, &puzzle_headless, "unicode-keyboard");
    assert_eq!(
        source::observed_snapshots(&unicode_keyboard.stdout),
        source::observed_snapshots(&rebound.stdout)
    );

    let player = project.cook_and_relocate();
    let pack = player.parent().unwrap().join("content.pack");
    let packed_bytes = fs::read(&pack).unwrap();
    let played = run_player(&mut project, &player, "gamepad");
    assert_eq!(
        source::observed_snapshots(&played.stdout),
        source::observed_snapshots(&rebound.stdout)
    );
    let cooked_unbound = run_player(&mut project, &player, "keyboard");
    source::assert_snapshots(&cooked_unbound.stdout, None, 0);
    let cooked_unicode = run_player(&mut project, &player, "unicode-keyboard");
    assert_eq!(
        source::observed_snapshots(&cooked_unicode.stdout),
        source::observed_snapshots(&unicode_keyboard.stdout)
    );

    let exact = project.run_exact_player(&player);
    assert_line(
        &exact,
        "starter_player: ok frame_limit=4096 fixed_ticks=4096",
    );
    assert!(source::observed_snapshots(&exact.stdout).is_empty());
    assert_eq!(fs::read(pack).unwrap(), packed_bytes);
    println!(
        "PUSH_BOX_WORKFLOW_OK provider=push-box.components plugin=push-box.gameplay action=move.right command_transcodes=0 gameplay_edit_to_result_ms={} scene_edit_to_result_ms={} input_edit_to_result_ms={} source_gate_open_tick=2->6 closed_gate_blocks=true double_tap_fixed_ticks=10,11 multibinding_order=true keyboard_unbound=true cooked_matches_source=true cooked_keyboard_unbound=true exact_source_free=true unicode_keyboard_matches_source=true",
        game_edit_to_result.as_millis(),
        scene_edit_to_result.as_millis(),
        input_edit_to_result.as_millis(),
    );
}

#[cfg(target_os = "windows")]
pub fn accept_native() {
    let mut project = Project::create();
    assert_desktop_keyboard_check(&mut project);
    let headless = project.build("headless", "headless-with-desktop-feature", &["desktop"]);
    let started = project.run(&headless, |command| command);
    assert_line(&started, "starter_headless: ok fixed_ticks=1");
    let verify_first_present = |project: &mut Project, installed| {
        let desktop = project.build("desktop", installed, &["desktop"]);
        let presented = project.run(&desktop, |command| {
            command.argument("--verify-first-present")
        });
        assert_line(
            &presented,
            "starter_desktop: ok first_present_verified=true",
        );
    };
    verify_first_present(&mut project, "desktop");

    let desktop_entry = project.root.join("src/bin/desktop.rs");
    let generated = fs::read(&desktop_entry).unwrap();
    source::install_puzzle(&project.root);
    project
        .cli("schema", &["--write", "--profile", "desktop"])
        .assert_success();
    source::assert_puzzle_schema(&project.root);
    assert_eq!(fs::read(desktop_entry).unwrap(), generated);
    verify_first_present(&mut project, "puzzle-desktop");
    let exported = project.export_candidate();
    for path in [SCENE, source::INPUT_MAP, source::SCHEMA] {
        assert_eq!(
            fs::read(exported.join("project").join(path)).unwrap(),
            fs::read(project.root.join(path)).unwrap(),
        );
    }
    println!(
        "DESKTOP_2D_V2_NATIVE_OK starter_first_present=true puzzle_first_present=true source_export_observations=2 physical_input_tested=false"
    );
}

#[cfg(target_os = "windows")]
fn assert_desktop_keyboard_check(project: &mut Project) {
    let input_path = project.root.join(source::INPUT_MAP);
    let initial_input = fs::read(&input_path).unwrap();
    for key in [';', ']'] {
        fs::write(
            &input_path,
            source::rebind_keyboard_right(&initial_input, key),
        )
        .unwrap();
        project
            .cli("check", &["--profile", "desktop"])
            .assert_success();
    }

    fs::write(
        &input_path,
        source::rebind_keyboard_right(&initial_input, DESKTOP_UNSUPPORTED_KEY),
    )
    .unwrap();
    let rejected = project.cli("check", &["--profile", "desktop"]);
    assert_eq!(
        rejected.receipt.exit().code(),
        Some(1),
        "stdout={}\nstderr={}",
        rejected.stdout,
        rejected.stderr
    );
    let location = rejected
        .stderr
        .lines()
        .skip_while(|line| {
            !line.starts_with("project.content.input-unsupported-keyboard-binding: ")
        })
        .nth(1);
    let expected_location = format!("  at {}", source::INPUT_MAP);
    assert_eq!(
        location,
        Some(expected_location.as_str()),
        "{}",
        rejected.stderr
    );
    project.cli("check", &[]).assert_success();
    fs::write(input_path, initial_input).unwrap();
}

fn snapshot<const N: usize>(root: &Path, files: [&str; N]) -> [Vec<u8>; N] {
    files.map(|file| fs::read(root.join(file)).unwrap())
}

fn run_source(project: &mut Project, executable: &Path, input: &str) -> Output {
    let output = project.run(executable, |command| {
        command.environment(source::PROOF_INPUT, input)
    });
    assert_line(&output, "starter_headless: ok fixed_ticks=17");
    output
}

fn run_player(project: &mut Project, executable: &Path, input: &str) -> Output {
    let output = project.run(executable, |command| {
        command
            .arguments(["--frames", source::PROOF_TICKS])
            .environment(source::PROOF_INPUT, input)
    });
    assert_line(&output, "starter_player: ok frame_limit=17 fixed_ticks=17");
    output
}

fn assert_line(output: &Output, expected: &str) {
    assert_eq!(
        output
            .stdout
            .lines()
            .filter(|line| *line == expected)
            .count(),
        1,
        "expected {expected:?}\nstdout={}\nstderr={}",
        output.stdout,
        output.stderr
    );
}
