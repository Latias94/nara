//! A finite game script issuing the same semantic commands as keys. It never edits World state.

use std::time::{Duration, Instant};

use nara::{
    gameplay::LocalGameplayCommands,
    prelude::{Res, ResMut, Resource},
};

use super::{GamePhase, GameSnapshot, Scenario, commands::*};
use crate::levels::{LevelId, LevelKey};

#[derive(Debug)]
enum Step {
    Walk(u32),
    Marker(u64),
    Level {
        command: &'static str,
        key: LevelKey,
        score: u32,
        visible: usize,
        markers: &'static [u64],
    },
    Save,
}

#[derive(Resource)]
pub(super) struct Verification {
    steps: Vec<Step>,
    index: usize,
    sent: bool,
    prior: u32,
    pub complete: bool,
    pub failure: Option<String>,
    previous_instance: Option<nara::identity::SceneInstanceId>,
    driver_calls: u64,
    last_progress_at: Option<Instant>,
}

impl Verification {
    pub fn new(scenario: Scenario) -> Self {
        use LevelId::{Cavern, Meadow};
        let level =
            |command, source, instance, score, visible, markers: &'static [u64]| Step::Level {
                command,
                key: LevelKey::new(source, instance).unwrap(),
                score,
                visible,
                markers,
            };
        let steps = match scenario {
            Scenario::Interactive => Vec::new(),
            Scenario::Restore => vec![
                level(RESTORE, Cavern, 0, 30, 1, &[2]),
                Step::Marker(4),
                level(INSTANCE, Cavern, 1, 30, 1, &[3]),
                level(TRAVEL, Meadow, 0, 30, 1, &[1]),
            ],
            Scenario::Travel | Scenario::Save => {
                let mut steps = vec![
                    Step::Walk(10),
                    Step::Marker(1),
                    level(TRAVEL, Cavern, 0, 10, 2, &[]),
                    Step::Walk(20),
                    Step::Marker(2),
                    level(INSTANCE, Cavern, 1, 20, 2, &[]),
                    Step::Walk(30),
                    Step::Marker(3),
                    level(INSTANCE, Cavern, 0, 30, 1, &[2]),
                    level(RETRY, Cavern, 0, 30, 1, &[2]),
                ];
                if scenario == Scenario::Save {
                    steps.push(Step::Save);
                } else {
                    steps.push(level(TRAVEL, Meadow, 0, 30, 1, &[1]));
                }
                steps
            }
        };
        Self {
            steps,
            index: 0,
            sent: false,
            prior: 0,
            complete: false,
            failure: None,
            previous_instance: None,
            driver_calls: 0,
            last_progress_at: None,
        }
    }
}

pub(super) fn drive(
    mut verifier: ResMut<Verification>,
    snapshot: Res<GameSnapshot>,
    mut commands: ResMut<LocalGameplayCommands>,
) {
    verifier.driver_calls = verifier.driver_calls.saturating_add(1);
    if verifier.complete
        || verifier.failure.is_some()
        || !snapshot.initialized
        || snapshot.error.is_some()
        || verifier.steps.is_empty()
    {
        return;
    }
    let Some(step) = verifier.steps.get(verifier.index) else {
        verifier.complete = true;
        return;
    };
    let done = if !verifier.sent {
        if snapshot.phase != GamePhase::Running {
            return;
        }
        let command = match step {
            Step::Walk(_) => movement(1, true),
            Step::Marker(_) => action(MARKER),
            Step::Level { command, .. } => action(command),
            Step::Save => action(SAVE),
        };
        println!(
            "two_level_verify: dispatch stage={} step={step:?} snapshot={snapshot:?}",
            verifier.index
        );
        if let Err(error) = commands.push(command) {
            verifier.failure = Some(error.to_string());
            return;
        }
        verifier.prior = match step {
            Step::Save => snapshot.saves,
            Step::Level { command: RETRY, .. } => snapshot.restarts,
            _ => snapshot.travel_count,
        };
        verifier.previous_instance = snapshot.current_instance;
        verifier.sent = true;
        verifier.last_progress_at = Some(Instant::now());
        false
    } else {
        match step {
            Step::Walk(score) if snapshot.score >= *score => {
                if snapshot.score != *score {
                    verifier.failure =
                        Some("walk collected an unexpected number of pickups".into());
                    return;
                }
                if let Err(error) = commands.push(movement(1, false)) {
                    verifier.failure = Some(error.to_string());
                    return;
                }
                true
            }
            Step::Marker(id) => snapshot.markers.iter().any(|marker| marker.get() == *id),
            Step::Level {
                command,
                key,
                score,
                visible,
                markers,
            } if (if *command == RETRY {
                snapshot.restarts
            } else {
                snapshot.travel_count
            }) > verifier.prior
                && snapshot.phase == GamePhase::Running =>
            {
                if snapshot.current_instance == verifier.previous_instance
                    || snapshot.scene_members != 6
                    || snapshot.level != Some(*key)
                    || snapshot.score != *score
                    || snapshot.visible_pickups != *visible
                    || snapshot
                        .markers
                        .iter()
                        .map(|id| id.get())
                        .collect::<Vec<_>>()
                        .as_slice()
                        != *markers
                {
                    verifier.failure = Some(format!(
                        "level reconstruction disagreed with saved logical facts: {snapshot:?}"
                    ));
                    return;
                }
                true
            }
            Step::Save => snapshot.saves > verifier.prior,
            _ => false,
        }
    };
    if done {
        println!(
            "two_level_verify: completed stage={} snapshot={snapshot:?}",
            verifier.index
        );
        verifier.index += 1;
        verifier.sent = false;
        verifier.last_progress_at = Some(Instant::now());
    }
    // Yield only in this optional headless verification driver; no blocking content runs here.
    std::thread::yield_now();
}

/// A real-time diagnostic bound for this finite script, independent of virtual frame speed.
pub(super) fn observe_progress(snapshot: Res<GameSnapshot>, mut verifier: ResMut<Verification>) {
    const MAX_IDLE: Duration = Duration::from_secs(30);
    if verifier.steps.is_empty() || verifier.complete || verifier.failure.is_some() {
        return;
    }
    let since = *verifier.last_progress_at.get_or_insert_with(Instant::now);
    if since.elapsed() < MAX_IDLE {
        return;
    }
    let detail = format!(
        "verification stalled for {MAX_IDLE:?}: stage={} step={:?} dispatched={} PreUpdate_driver_calls={} snapshot={snapshot:?}",
        verifier.index,
        verifier.steps.get(verifier.index),
        verifier.sent,
        verifier.driver_calls
    );
    eprintln!("two_level_verify: {detail}");
    verifier.failure = Some(detail);
}
