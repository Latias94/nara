use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use two_level_game::{
    development::{
        ActiveGeneration, DevelopmentHost, ProbeStep, RetirementRole, RetirementStatus,
        protocol::{Observation, SemanticCommand},
    },
    resume::{ResumeState, encode_resume},
    save::{MarkerId, MarkerRecord},
};

const POLL: Duration = Duration::from_millis(10);
const RETIRE_TIMEOUT: Duration = Duration::from_secs(30);
const TEMP_PREFIX: &str = "nara-development-host-acceptance-";

/// Filesystem-only tests need a trusted parent without the actual-build fixture's rustc probe.
pub struct WorkParent {
    parent: PathBuf,
    root: PathBuf,
}

impl WorkParent {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = parent.join(format!(
            "{TEMP_PREFIX}{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        assert_eq!(root.parent(), Some(parent.as_path()));
        Self { parent, root }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for WorkParent {
    fn drop(&mut self) {
        let Ok(root) = self.root.canonicalize() else {
            return;
        };
        if root != self.root
            || root.parent() != Some(self.parent.as_path())
            || !root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(TEMP_PREFIX)
        {
            eprintln!(
                "refusing to remove an unverified Host test parent: {}",
                root.display(),
            );
            return;
        }
        if let Err(error) = fs::remove_dir_all(&root) {
            eprintln!(
                "Host test parent cleanup failed at {}: {error}",
                root.display(),
            );
        }
    }
}

pub fn movement(pressed: bool) -> SemanticCommand {
    SemanticCommand::Move {
        direction: 1,
        pressed,
    }
}

pub fn walk_to_score(host: &mut DevelopmentHost, expected: u32) {
    let mut observation = host.step(vec![movement(true)]).unwrap();
    for _ in 0..100 {
        if observation.score == expected {
            assert_eq!(observation.collected, expected as usize / 10);
            assert_eq!(observation.held, [false, true, false, false]);
            assert_observation(&observation, &host.capture().unwrap());
            return;
        }
        assert!(
            observation.score < expected,
            "unexpected pickup: {observation:?}",
        );
        observation = host.step(Vec::new()).unwrap();
    }
    panic!("ordinary held movement did not reach the expected pickup: {observation:?}");
}

pub fn assert_preserved(
    host: &mut DevelopmentHost,
    active: &ActiveGeneration,
    state: &ResumeState,
) {
    assert_eq!(host.active().as_ref(), Some(active));
    let actual = host.capture().unwrap();
    assert_eq!(
        encode_resume(&actual).unwrap(),
        encode_resume(state).unwrap(),
        "complete canonical authoritative game state: actual={actual:?}, expected={state:?}",
    );
}

pub fn assert_observation(observation: &Observation, state: &ResumeState) {
    state.validate().unwrap();
    assert_eq!(observation.phase, "running", "{observation:?}");
    assert!(!observation.restoring, "{observation:?}");
    assert!(observation.error.is_none(), "{observation:?}");
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
            .collect::<Vec<_>>(),
    );
}

fn advance_clock(state: &mut ResumeState) {
    state.clock.ticks += 1;
    state.clock.elapsed_nanos += state.clock.timestep_nanos;
}

/// A right-held ordinary tick with no new pickup nearby. This is an exact f32 expectation
/// derived from the captured fixed duration, not from any candidate observation or snapshot.
pub fn expected_held_tick(before: &ResumeState, speed: f32) -> ResumeState {
    assert_eq!(before.held, [false, true, false, false]);
    let mut after = before.clone();
    let delta = Duration::from_nanos(before.clock.timestep_nanos).as_secs_f32();
    after.player.translation[0] =
        (before.player.translation[0] + speed * delta).clamp(-280.0, 280.0);
    advance_clock(&mut after);
    after.validate().unwrap();
    after
}

pub fn expected_marker_tick(before: &ResumeState, speed: f32) -> ResumeState {
    let mut marked = before.clone();
    let id = marked.progress.next_marker_id;
    marked.progress.markers.push(MarkerRecord {
        id,
        level: before.progress.location.level,
        position: before.player.translation,
    });
    marked.progress.next_marker_id = MarkerId::new(id.get() + 1).unwrap();
    expected_held_tick(&marked, speed)
}

/// Three complete, finite expectations: move, place a marker then move, release and tick.
/// The final probe deliberately differs from the original capture in position, progress,
/// marker allocator, held intention and clock; a fresh adopter must restore the original.
pub fn probe_steps(base: &ResumeState, speed: f32) -> Vec<ProbeStep> {
    let moved = expected_held_tick(base, speed);
    let marked = expected_marker_tick(&moved, speed);
    let mut released = marked.clone();
    released.held = [false; 4];
    advance_clock(&mut released);
    released.validate().unwrap();
    vec![
        ProbeStep {
            commands: Vec::new(),
            expected: moved,
        },
        ProbeStep {
            commands: vec![SemanticCommand::Marker],
            expected: marked,
        },
        ProbeStep {
            commands: vec![movement(false)],
            expected: released,
        },
    ]
}

pub fn assert_clean_retirement(host: &DevelopmentHost, generation: ActiveGeneration) {
    let retired = host
        .retirements()
        .iter()
        .find(|report| report.generation == generation)
        .expect("the exact original process must publish its physical retirement report");
    assert!(retired.game_closed, "{retired:?}");
    assert!(
        !retired.terminated,
        "fallback termination is not clean game Close: {retired:?}"
    );
    assert!(retired.native.exit().success(), "{retired:?}");
    assert!(retired.native.containment_empty(), "{retired:?}");
    assert!(retired.native.readers_stopped(), "{retired:?}");
    assert!(retired.native.output_drained(), "{retired:?}");
}

pub fn assert_probe_was_discarded(host: &DevelopmentHost, adopter: ActiveGeneration) {
    let probe = host
        .retirements()
        .iter()
        .rev()
        .find(|report| report.role == RetirementRole::Probe)
        .expect("a successful preparation must retire its actual probe");
    assert_ne!(probe.generation.sequence, adopter.sequence);
    assert_eq!(probe.generation.executable, adopter.executable);
    assert_eq!(probe.generation.provenance, adopter.provenance);
    assert_clean_retirement(host, probe.generation);
}

pub fn finish_retirements(host: &mut DevelopmentHost, mut status: RetirementStatus) {
    let deadline = Instant::now() + RETIRE_TIMEOUT;
    while matches!(status, RetirementStatus::Pending) {
        assert!(
            Instant::now() < deadline,
            "Host still owns incomplete physical retirement",
        );
        std::thread::sleep(POLL);
        status = host.retry_retirement();
    }
    assert!(matches!(status, RetirementStatus::Complete));
}

/// Return pending truthfully so the caller can retain both the Host and its source/work tree.
pub fn close_to_completion(host: &mut DevelopmentHost) -> bool {
    let deadline = Instant::now() + RETIRE_TIMEOUT;
    loop {
        if matches!(host.close(), RetirementStatus::Complete) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
}
