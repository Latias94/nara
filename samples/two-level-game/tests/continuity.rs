//! R12 qualification through actual Rust edits, public Cargo builds and fresh native processes.

#![cfg(any(windows, target_os = "linux"))]
#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use nara::{fs::ContentDigest, project_tooling::cargo_build::CapturedCargoExecutable};
use two_level_game::{
    levels::{LevelId, LevelKey},
    resume::ResumeState,
};

#[path = "../src/bin/continuity/protocol.rs"]
#[allow(dead_code)]
mod protocol;
#[path = "../../../tests/support/continuity.rs"]
mod support;

use protocol::{Command, Observation, SemanticCommand};
use support::{CapturedState, Executable, Fixture, GAMEPLAY, GameProcess, assert_running};

#[test]
#[ignore = "actual Rust 1.95.0 nested Cargo builds and native children; run serially with jobs=1 and the absolute root CARGO_TARGET_DIR"]
fn rust_body_and_private_state_edits_rebuild_and_reconstruct_the_playable_game() {
    let fixture = Fixture::new();
    let original_source = fixture.read(GAMEPLAY);
    let a = fixture.build();
    let original_bytes = a.bytes().to_vec();
    let original_digest = a.digest();
    let original_provenance = a.provenance().digest();
    let installed_a = Executable::install(&fixture, "executable-a", &a);
    let mut process_a = GameProcess::spawn(&fixture, &installed_a, "process-a").unwrap();
    assert_eq!(process_a.ready.player, [-120.0, 0.0]);
    assert_eq!(process_a.ready.held, [false; 4]);

    walk_to_score(&mut process_a, 10);
    process_a.settled_step(vec![movement(false)]);
    let meadow_marker = process_a.settled_step(vec![SemanticCommand::Marker]);
    assert_eq!(meadow_marker.markers, [1]);
    let travelling = process_a.step(vec![SemanticCommand::Travel]);
    assert_ne!(
        travelling.phase, "running",
        "travel must first prepare a fresh Scene"
    );
    assert!(
        process_a.request(Command::Capture).is_err(),
        "pending travel must refuse exact capture"
    );
    process_a.settle(travelling);
    walk_to_score(&mut process_a, 20);
    process_a.settled_step(vec![movement(false)]);
    assert_eq!(
        process_a
            .settled_step(vec![SemanticCommand::Marker])
            .markers,
        [2]
    );
    process_a.settled_step(vec![SemanticCommand::Instance]);
    walk_to_score(&mut process_a, 30);
    let meaningful = process_a.settled_step(vec![SemanticCommand::Marker]);
    assert_eq!(meaningful.markers, [3]);
    let captured_a = process_a.capture();
    assert_meaningful(&captured_a.state);
    assert_observation(&meaningful, &captured_a.state);

    fixture.write(
        GAMEPLAY,
        &format!(
            "{original_source}\ncompile_error!({:?});\n",
            support::COMPILE_ERROR
        ),
    );
    fixture.fail_compilation();
    captured_a.assert_exact(&process_a.capture());
    assert_retained(
        &a,
        &installed_a,
        &original_bytes,
        original_digest,
        original_provenance,
    );
    let after_failed_build = process_a.settled_step(vec![movement(true)]);
    let handoff_a = process_a.capture();
    assert_one_movement_tick(&captured_a.state, &handoff_a.state, 160.0);
    assert_observation(&after_failed_build, &handoff_a.state);
    fixture.write(GAMEPLAY, &original_source);
    assert_eq!(fixture.read(GAMEPLAY), original_source);

    fixture.edit_body();
    let b = fixture.build();
    assert_new_build(&a, &b, &BTreeSet::from([GAMEPLAY.to_owned()]));
    assert_retained(
        &a,
        &installed_a,
        &original_bytes,
        original_digest,
        original_provenance,
    );
    handoff_a.assert_exact(&process_a.capture());
    let installed_b = Executable::install(&fixture, "executable-b", &b);
    let mut process_b = GameProcess::spawn(&fixture, &installed_b, "process-b").unwrap();
    assert_ne!(
        process_a.pid(),
        process_b.pid(),
        "B must be an independently created native process"
    );
    let restored_b = process_b.restore(&handoff_a).unwrap();
    assert_observation(&restored_b, &handoff_a.state);
    assert!(
        restored_b.engine_tick < after_failed_build.engine_tick,
        "B's Runtime clock must stay fresh"
    );
    handoff_a.assert_exact(&process_b.capture());

    let observation_a = process_a.settled_step(vec![movement(true)]);
    let observation_b = process_b.settled_step(vec![movement(true)]);
    let continued_a = process_a.capture();
    let continued_b = process_b.capture();
    assert_one_movement_tick(&handoff_a.state, &continued_a.state, 160.0);
    assert_one_movement_tick(&handoff_a.state, &continued_b.state, 80.0);
    assert_observation(&observation_a, &continued_a.state);
    assert_observation(&observation_b, &continued_b.state);
    let delta_a = continued_a.state.player.translation[0] - handoff_a.state.player.translation[0];
    let delta_b = continued_b.state.player.translation[0] - handoff_a.state.player.translation[0];
    assert_near(delta_a, 2.0 * delta_b);
    assert_eq!(
        continued_a.state.clock, continued_b.state.clock,
        "only gameplay movement changed"
    );
    let mut same_semantics = continued_b.state.clone();
    same_semantics.player = continued_a.state.player;
    assert_eq!(
        same_semantics, continued_a.state,
        "the body edit must preserve all other authoritative state"
    );
    assert_retained(
        &a,
        &installed_a,
        &original_bytes,
        original_digest,
        original_provenance,
    );

    for incompatibility in ["configuration", "schema", "cavern-content"] {
        let incumbent = process_b.capture();
        let mut incompatible = CapturedState {
            state: incumbent.state.clone(),
            bytes: incumbent.bytes.clone(),
        };
        match incompatibility {
            "configuration" => incompatible.state.compatibility.configuration[0] ^= 1,
            "schema" => incompatible.state.compatibility.schema[0] ^= 1,
            "cavern-content" => {
                let record = incompatible
                    .state
                    .compatibility
                    .levels
                    .iter_mut()
                    .find(|record| record.level == LevelId::Cavern)
                    .unwrap();
                record.content[0] ^= 1;
            }
            _ => unreachable!(),
        }
        incompatible.state.validate().unwrap();
        let label = format!("rejected-{incompatibility}");
        let mut candidate = GameProcess::spawn(&fixture, &installed_b, &label).unwrap();
        assert_ne!(candidate.pid(), process_b.pid());
        let error = candidate
            .request(Command::Restore {
                state: incompatible.state,
            })
            .unwrap_err();
        assert!(
            error.to_ascii_lowercase().contains(match incompatibility {
                "configuration" => "configuration",
                "schema" => "schema",
                _ => "content",
            }),
            "the real compatibility owner must explain {incompatibility}: {error}"
        );
        assert!(
            candidate.request(Command::Capture).is_err(),
            "a rejected candidate must stay ineligible for capture"
        );
        candidate.close();
        incumbent.assert_exact(&process_b.capture());
        continued_a.assert_exact(&process_a.capture());
        process_b.settled_step(vec![movement(true)]);
        assert_one_movement_tick(&incumbent.state, &process_b.capture().state, 80.0);
        installed_b.assert_exact(&b);
        assert_retained(
            &a,
            &installed_a,
            &original_bytes,
            original_digest,
            original_provenance,
        );
    }

    let before_start_failure = process_b.capture();
    let missing_project = fixture.root.join("missing-project");
    assert!(!missing_project.exists());
    assert!(
        GameProcess::spawn_in(&fixture, &installed_b, "startup-failure", &missing_project).is_err()
    );
    before_start_failure.assert_exact(&process_b.capture());
    process_b.settled_step(vec![movement(true)]);
    let handoff_b = process_b.capture();
    assert_one_movement_tick(&before_start_failure.state, &handoff_b.state, 80.0);

    let structural_inputs = fixture.edit_structure();
    let c = fixture.build();
    assert_new_build(&b, &c, &structural_inputs);
    handoff_b.assert_exact(&process_b.capture());
    let installed_c = Executable::install(&fixture, "executable-c", &c);
    for kind in ["session", "generation", "request"] {
        let mut candidate =
            GameProcess::spawn(&fixture, &installed_c, &format!("stale-reply-{kind}")).unwrap();
        let error = candidate
            .restore_with_stale_reply(&handoff_b, kind)
            .unwrap_err();
        assert!(
            error.contains("stale protocol reply"),
            "the reply gate must reject the real candidate's stale {kind}: {error}"
        );
        candidate.close();
        handoff_b.assert_exact(&process_b.capture());
        continued_a.assert_exact(&process_a.capture());
    }
    let mut process_c = GameProcess::spawn(&fixture, &installed_c, "process-c").unwrap();
    assert_ne!(process_c.pid(), process_b.pid());
    assert_ne!(process_c.pid(), process_a.pid());
    let restored_c = process_c.restore(&handoff_b).unwrap();
    assert_observation(&restored_c, &handoff_b.state);
    assert!(restored_c.engine_tick < process_b.observe().engine_tick);
    handoff_b.assert_exact(&process_c.capture());

    process_c.reject_stale_requests();
    handoff_b.assert_exact(&process_c.capture());
    handoff_b.assert_exact(&process_b.capture());
    continued_a.assert_exact(&process_a.capture());

    let first_continued = paired_step(&mut process_b, &mut process_c, || vec![movement(true)]);
    assert_one_movement_tick(&handoff_b.state, &first_continued.state, 80.0);
    paired_step(&mut process_b, &mut process_c, || vec![movement(false)]);
    let next_marker = paired_step(&mut process_b, &mut process_c, || {
        vec![SemanticCommand::Marker]
    });
    assert_eq!(marker_ids(&next_marker.state), [1, 2, 3, 4]);
    assert_eq!(next_marker.state.progress.next_marker_id.get(), 5);
    assert_eq!(
        next_marker.state.progress.location.level,
        LevelKey::new(LevelId::Cavern, 1).unwrap()
    );

    let cavern_zero = paired_step(&mut process_b, &mut process_c, || {
        vec![SemanticCommand::Instance]
    });
    assert_eq!(
        cavern_zero.state.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(process_c.observe().markers, [2]);
    let cavern_one = paired_step(&mut process_b, &mut process_c, || {
        vec![SemanticCommand::Instance]
    });
    assert_eq!(
        cavern_one.state.progress.location.level,
        LevelKey::new(LevelId::Cavern, 1).unwrap()
    );
    assert_eq!(process_c.observe().markers, [3, 4]);
    let meadow_zero = paired_step(&mut process_b, &mut process_c, || {
        vec![SemanticCommand::Travel]
    });
    assert_eq!(
        meadow_zero.state.progress.location.level,
        LevelKey::main(LevelId::Meadow)
    );
    assert_eq!(process_c.observe().markers, [1]);
    let meadow_one = paired_step(&mut process_b, &mut process_c, || {
        vec![SemanticCommand::Instance]
    });
    assert_eq!(
        meadow_one.state.progress.location.level,
        LevelKey::new(LevelId::Meadow, 1).unwrap()
    );
    assert!(process_c.observe().markers.is_empty());

    let mut collection = paired_step(&mut process_b, &mut process_c, || vec![movement(true)]);
    for _ in 0..100 {
        if collection.state.progress.score == 40 {
            break;
        }
        assert_eq!(
            collection.state.progress.score, 30,
            "no unrelated pickup may be collected"
        );
        collection = paired_step(&mut process_b, &mut process_c, Vec::new);
    }
    assert_eq!(collection.state.progress.score, 40);
    assert_eq!(collection.state.progress.collected.len(), 4);
    assert!(
        collection
            .state
            .progress
            .collected
            .iter()
            .any(|pickup| pickup.level == LevelKey::new(LevelId::Meadow, 1).unwrap())
    );
    paired_step(&mut process_b, &mut process_c, || vec![movement(false)]);
    let marker_five = paired_step(&mut process_b, &mut process_c, || {
        vec![SemanticCommand::Marker]
    });
    assert_eq!(marker_ids(&marker_five.state), [1, 2, 3, 4, 5]);
    assert_eq!(marker_five.state.progress.next_marker_id.get(), 6);
    let retry = paired_step(&mut process_b, &mut process_c, || {
        vec![SemanticCommand::Retry]
    });
    assert_eq!(retry.state.progress, marker_five.state.progress);
    assert_eq!(
        retry.state.counters.restarts,
        marker_five.state.counters.restarts + 1
    );
    assert_eq!(retry.state.player.translation, [-120.0, 0.0]);
    assert_eq!(process_c.observe().markers, [5]);
    let final_state = paired_step(&mut process_b, &mut process_c, || {
        vec![SemanticCommand::Travel]
    });
    assert_eq!(
        final_state.state.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(final_state.state.progress.score, 40);
    assert_eq!(
        final_state.state.counters.travel_count,
        handoff_b.state.counters.travel_count + 5
    );
    assert_eq!(process_c.observe().markers, [2]);
    assert!(final_state.state.clock.ticks > handoff_b.state.clock.ticks);

    continued_a.assert_exact(&process_a.capture());
    process_a.settled_step(vec![movement(true)]);
    assert_one_movement_tick(&continued_a.state, &process_a.capture().state, 160.0);
    assert_retained(
        &a,
        &installed_a,
        &original_bytes,
        original_digest,
        original_provenance,
    );
    installed_b.assert_exact(&b);
    installed_c.assert_exact(&c);
    process_c.close();
    process_b.close();
    process_a.close();
}

fn movement(pressed: bool) -> SemanticCommand {
    SemanticCommand::Move {
        direction: 1,
        pressed,
    }
}

fn walk_to_score(process: &mut GameProcess<'_>, expected: u32) {
    let mut observation = process.settled_step(vec![movement(true)]);
    for _ in 0..100 {
        if observation.score == expected {
            assert_eq!(observation.collected, expected as usize / 10);
            assert_eq!(observation.held, [false, true, false, false]);
            return;
        }
        assert!(
            observation.score < expected,
            "unexpected pickup: {observation:?}"
        );
        observation = process.settled_step(Vec::new());
    }
    panic!("ordinary held movement did not collect the expected pickup: {observation:?}");
}

fn paired_step(
    b: &mut GameProcess<'_>,
    c: &mut GameProcess<'_>,
    commands: impl Fn() -> Vec<SemanticCommand>,
) -> CapturedState {
    let observation_b = b.settled_step(commands());
    let observation_c = c.settled_step(commands());
    let state_b = b.capture();
    let state_c = c.capture();
    state_b.assert_exact(&state_c);
    assert_observation(&observation_b, &state_b.state);
    assert_observation(&observation_c, &state_c.state);
    state_c
}

fn assert_meaningful(state: &ResumeState) {
    state.validate().unwrap();
    assert_eq!(
        state.progress.location.level,
        LevelKey::new(LevelId::Cavern, 1).unwrap()
    );
    assert_ne!(
        state.player.translation,
        [-120.0, 0.0],
        "live position must differ from the progress-save anchor"
    );
    assert_eq!(state.held, [false, true, false, false]);
    assert_eq!(state.progress.score, 30);
    assert_eq!(state.progress.collected.len(), 3);
    assert_eq!(marker_ids(state), [1, 2, 3]);
    assert_eq!(state.progress.next_marker_id.get(), 4);
    let levels = state
        .progress
        .collected
        .iter()
        .map(|pickup| pickup.level)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        levels,
        BTreeSet::from([
            LevelKey::main(LevelId::Meadow),
            LevelKey::main(LevelId::Cavern),
            LevelKey::new(LevelId::Cavern, 1).unwrap(),
        ])
    );
    assert_eq!(state.compatibility.levels.len(), 2);
    assert_eq!(state.counters.travel_count, 2);
    assert_eq!(state.counters.saves, 0);
    assert_eq!(state.counters.restores, 0);
    assert!(state.clock.ticks > 0);
    assert_eq!(state.clock.timestep_nanos, 20_000_000);
    assert_eq!(
        state.clock.elapsed_nanos,
        state.clock.ticks * state.clock.timestep_nanos
    );
}

fn assert_observation(observation: &Observation, state: &ResumeState) {
    assert_running(observation);
    assert_eq!(observation.player, state.player.translation);
    assert_eq!(observation.held, state.held);
    assert_eq!(observation.logical_tick, state.clock.ticks);
    assert_eq!(observation.logical_elapsed_nanos, state.clock.elapsed_nanos);
    assert_eq!(observation.score, state.progress.score);
    assert_eq!(observation.collected, state.progress.collected.len());
    assert_eq!(
        observation.markers,
        state
            .progress
            .markers
            .iter()
            .filter(|marker| marker.level == state.progress.location.level)
            .map(|marker| marker.id.get())
            .collect::<Vec<_>>()
    );
}

fn marker_ids(state: &ResumeState) -> Vec<u64> {
    state
        .progress
        .markers
        .iter()
        .map(|marker| marker.id.get())
        .collect()
}

fn assert_one_movement_tick(before: &ResumeState, after: &ResumeState, speed: f32) {
    assert_eq!(before.held, [false, true, false, false]);
    assert_eq!(after.held, before.held);
    assert_eq!(after.clock.ticks, before.clock.ticks + 1);
    assert_eq!(
        after.clock.elapsed_nanos,
        before.clock.elapsed_nanos + before.clock.timestep_nanos
    );
    assert_eq!(after.clock.timestep_nanos, before.clock.timestep_nanos);
    let expected = speed * (before.clock.timestep_nanos as f32 / 1_000_000_000.0);
    let displacement = after.player.translation[0] - before.player.translation[0];
    assert!(displacement > 0.0);
    assert_near(displacement, expected);
    assert_eq!(after.player.translation[1], before.player.translation[1]);
    let mut unchanged = after.clone();
    unchanged.player.translation = before.player.translation;
    unchanged.clock = before.clock;
    assert_eq!(
        unchanged, *before,
        "one held tick changes only position and logical game time"
    );
}

fn assert_near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.0001,
        "movement differs: actual={actual}, expected={expected}"
    );
}

fn assert_new_build(
    before: &CapturedCargoExecutable,
    after: &CapturedCargoExecutable,
    edited: &BTreeSet<String>,
) {
    assert_ne!(
        before.digest(),
        after.digest(),
        "a real Rust change must produce a different executable"
    );
    assert_ne!(before.provenance().digest(), after.provenance().digest());
    assert_eq!(
        before.provenance().selection(),
        after.provenance().selection()
    );
    let actual = after
        .provenance()
        .declared_inputs()
        .iter()
        .filter_map(|input| {
            let previous = before
                .provenance()
                .declared_inputs()
                .iter()
                .find(|old| old.path() == input.path())
                .expect("every edited source must remain in the actual Cargo input inventory");
            if previous.digest() != input.digest() {
                Some(input.path().to_owned())
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual, *edited,
        "provenance must name every actual edit, including private Session consumers"
    );
}

fn assert_retained(
    captured: &CapturedCargoExecutable,
    installed: &Executable,
    bytes: &[u8],
    digest: ContentDigest,
    provenance: ContentDigest,
) {
    assert_eq!(
        captured.bytes(),
        bytes,
        "A's exact native bytes remain retained"
    );
    assert_eq!(captured.digest(), digest);
    assert_eq!(ContentDigest::of_bytes(captured.bytes()), digest);
    assert_eq!(captured.provenance().digest(), provenance);
    installed.assert_exact(captured);
}
