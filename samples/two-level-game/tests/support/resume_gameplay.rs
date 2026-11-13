use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    ProductRecipe,
    app::{FixedTime, RuntimeState},
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandPayload,
        GameplayCommandQueue, GameplayCommandQueueStats, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
        GameplayCommandValue,
    },
    identity::{SceneInstanceId, WorldIdentityDomain, WorldIdentityDomainId},
    project_host::{HeadlessSession, HeadlessSessionClose},
    tasks::{TaskPoolKind, TaskPools},
};
use two_level_game::{
    GamePhase, GameSnapshot, capture_resume, open_explicit_project_root, open_project_root,
    product_recipe,
    resume::{ResumeState, encode_resume},
};

pub const DEADLINE: Duration = Duration::from_secs(10);
pub const MAX_WALK_STEPS: usize = 128;
pub const TRAVEL: &str = "two-level-game.travel";
pub const INSTANCE: &str = "two-level-game.instance";
pub const MARKER: &str = "two-level-game.marker";

/// The original public owner is closed even when a gameplay assertion unwinds.
pub struct Game {
    pub session: HeadlessSession,
    sequence: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FrozenGame {
    pub bytes: Vec<u8>,
    pub tick: u64,
    pub generation: u64,
    pub instance: SceneInstanceId,
    pub domain: WorldIdentityDomainId,
    pub queue: GameplayCommandQueueStats,
}

impl Game {
    pub fn source() -> Self {
        Self::with_recipe(product_recipe())
    }

    pub fn with_recipe(recipe: ProductRecipe) -> Self {
        Self::start(HeadlessSession::new(open_project_root().unwrap(), recipe))
    }

    pub fn from_project(root: &Path) -> Self {
        Self::from_project_with_recipe(root, product_recipe())
    }

    pub fn from_project_with_recipe(root: &Path, recipe: ProductRecipe) -> Self {
        Self::start(HeadlessSession::new(
            open_explicit_project_root(root).unwrap(),
            recipe,
        ))
    }

    fn start(session: HeadlessSession) -> Self {
        let mut game = Self {
            session,
            sequence: 0,
        };
        game.session.start().expect("ordinary product startup");
        assert_eq!(game.session.pause().unwrap(), None);
        let initial = game.settle();
        assert_eq!(game.tick(), 0, "startup must not simulate the candidate");
        assert_eq!(initial.tick, 0);
        assert_eq!(initial.logical_tick, 0);
        assert_eq!(initial.logical_elapsed_nanos, 0);
        assert_eq!(initial.held, [false; 4]);
        assert_eq!(initial.scene_members, 6);
        assert!(initial.current_instance.is_some());
        game
    }

    pub fn tick(&self) -> u64 {
        self.session
            .observe(|world| world.resource::<FixedTime>().tick())
            .unwrap()
    }

    pub fn snapshot(&self) -> GameSnapshot {
        let snapshot = self
            .session
            .observe(|world| world.resource::<GameSnapshot>().clone())
            .unwrap();
        assert_eq!(snapshot.tick, self.tick());
        snapshot
    }

    pub fn queue(&self) -> GameplayCommandQueueStats {
        self.session
            .observe(|world| world.resource::<GameplayCommandQueue>().stats())
            .unwrap()
    }

    pub fn capture(&self) -> ResumeState {
        assert_eq!(self.session.state(), Some(RuntimeState::Paused));
        let state = capture_resume(&self.session).expect("healthy completed game capture");
        let snapshot = self.snapshot();
        assert_eq!(snapshot.phase, GamePhase::Running);
        assert!(!snapshot.restoring);
        assert!(snapshot.error.is_none(), "{snapshot:?}");
        assert_eq!(snapshot.level, Some(state.progress.location.level));
        assert_eq!(snapshot.player.to_array(), state.player.translation);
        assert_eq!(snapshot.score, state.progress.score);
        assert_eq!(snapshot.collected, state.progress.collected.len());
        assert_eq!(snapshot.held, state.held);
        assert_eq!(snapshot.logical_tick, state.clock.ticks);
        assert_eq!(snapshot.logical_elapsed_nanos, state.clock.elapsed_nanos);
        assert_eq!(snapshot.travel_count, state.counters.travel_count);
        assert_eq!(snapshot.restarts, state.counters.restarts);
        assert_eq!(snapshot.saves, state.counters.saves);
        assert_eq!(snapshot.restores, state.counters.restores);
        state
    }

    pub fn frozen(&self) -> FrozenGame {
        FrozenGame {
            bytes: encode_resume(&self.capture()).unwrap(),
            tick: self.tick(),
            generation: self.session.runtime_generation().unwrap(),
            instance: self.snapshot().current_instance.unwrap(),
            domain: self
                .session
                .observe(|world| world.resource::<WorldIdentityDomain>().id())
                .unwrap(),
            queue: self.queue(),
        }
    }

    pub fn maintenance(&mut self) {
        let tick = self.tick();
        assert_eq!(self.session.state(), Some(RuntimeState::Paused));
        assert_eq!(self.session.frame(Duration::ZERO).unwrap(), None);
        assert_eq!(self.session.state(), Some(RuntimeState::Paused));
        assert_eq!(self.tick(), tick, "paused maintenance simulated gameplay");
    }

    pub fn settle(&mut self) -> GameSnapshot {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let snapshot = self.snapshot();
            assert!(snapshot.error.is_none(), "{snapshot:?}");
            assert_eq!(self.session.state(), Some(RuntimeState::Paused));
            let idle = self
                .session
                .observe(|world| {
                    let tasks = world.resource::<TaskPools>().stats();
                    world.resource::<GameplayCommandQueue>().is_idle()
                        && TaskPoolKind::ALL.into_iter().all(|kind| {
                            let stats = tasks.for_kind(kind);
                            stats.queued == 0 && stats.running == 0
                        })
                })
                .unwrap();
            if snapshot.initialized
                && snapshot.phase == GamePhase::Running
                && !snapshot.restoring
                && idle
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "paused game did not settle: {snapshot:?}; queue={:?}",
                self.queue()
            );
            self.maintenance();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub fn submit(&mut self, command: GameplayCommandDraft) {
        self.submit_at(self.tick().checked_add(1).unwrap(), command);
    }

    pub fn submit_at(&mut self, tick: u64, command: GameplayCommandDraft) {
        assert_eq!(self.session.state(), Some(RuntimeState::Paused));
        self.sequence = self.sequence.checked_add(1).unwrap();
        let submission = GameplayCommandSubmission::new(
            GameplayCommandTick::new(tick).unwrap(),
            GameplayCommandIngressSource::external("two-level-game.resume-gameplay-test").unwrap(),
            GameplayCommandSourceSequence::new(self.sequence).unwrap(),
            command,
        );
        let key = submission.key();
        assert_eq!(self.session.submit(submission).unwrap(), key);
    }

    pub fn step(&mut self) -> GameSnapshot {
        let tick = self.tick();
        assert_eq!(self.session.step().unwrap(), None);
        assert_eq!(self.tick(), tick + 1);
        self.settle()
    }

    pub fn command(&mut self, command: GameplayCommandDraft) -> GameSnapshot {
        self.submit(command);
        self.step()
    }

    fn close_terminal(&mut self) -> Option<HeadlessSessionClose> {
        let deadline = Instant::now() + DEADLINE;
        loop {
            match self.session.close() {
                HeadlessSessionClose::Pending if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                HeadlessSessionClose::Pending => return None,
                terminal => return Some(terminal),
            }
        }
    }

    pub fn close(&mut self) {
        assert_eq!(
            self.close_terminal(),
            Some(HeadlessSessionClose::Complete),
            "original Close did not finish cleanly: {:?}",
            self.session.diagnostics()
        );
        assert!(self.session.is_closed());
        assert_eq!(self.session.close(), HeadlessSessionClose::Complete);
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let terminal = self.close_terminal();
        if std::thread::panicking() {
            if terminal != Some(HeadlessSessionClose::Complete) {
                eprintln!(
                    "resume test cleanup: {terminal:?}; {:?}",
                    self.session.diagnostics()
                );
            }
        } else {
            assert_eq!(terminal, Some(HeadlessSessionClose::Complete));
            assert!(self.session.is_closed());
        }
    }
}

pub fn action(id: &str) -> GameplayCommandDraft {
    GameplayCommandDraft::new(GameplayCommandTypeId::new(id).unwrap())
}

pub fn movement(direction: i64, pressed: bool) -> GameplayCommandDraft {
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("direction", GameplayCommandValue::I64(direction))
        .unwrap();
    payload
        .insert("pressed", GameplayCommandValue::Bool(pressed))
        .unwrap();
    action("two-level-game.move").with_payload(payload)
}

pub struct ProjectCopy {
    temporary_parent: PathBuf,
    directory: PathBuf,
    project: PathBuf,
}

impl ProjectCopy {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary_parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let directory = temporary_parent.join(format!(
            "two-level-resume-{}-{}-{}",
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
        let project = directory.join("project");
        let copy = Self {
            temporary_parent,
            directory,
            project,
        };
        for relative in [
            "nara.toml",
            "scenes/meadow.scene.json",
            "scenes/cavern.scene.json",
            "assets/textures/meadow.png",
            "assets/textures/meadow.png.meta",
            "assets/textures/cavern.png",
            "assets/textures/cavern.png.meta",
            "schema/authoring.bundle.json",
            "prefabs/README.md",
        ] {
            let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
            let destination = copy.project.join(relative);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(&source, &destination).unwrap();
            assert_eq!(fs::read(source).unwrap(), fs::read(destination).unwrap());
        }
        assert_ne!(
            fs::canonicalize(&copy.project).unwrap(),
            fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap()
        );
        copy
    }

    pub fn root(&self) -> &Path {
        &self.project
    }

    pub fn save_path(&self) -> PathBuf {
        self.directory.join("candidate-checkpoint.json")
    }

    fn editable_file(&self, relative: &str) -> PathBuf {
        let target = fs::canonicalize(self.project.join(relative)).unwrap();
        let project = fs::canonicalize(&self.project).unwrap();
        let source = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(target.is_absolute() && target.starts_with(&project));
        assert!(project.starts_with(&self.directory));
        assert!(!target.starts_with(&source));
        target
    }

    pub fn change_fixed_timestep(&self) {
        let target = self.editable_file("nara.toml");
        let manifest = fs::read_to_string(&target).unwrap();
        let before = "fixed_timestep_seconds = 0.02";
        assert_eq!(manifest.matches(before).count(), 1);
        let changed = manifest.replace(before, "fixed_timestep_seconds = 0.01");
        fs::write(&target, changed.as_bytes()).unwrap();
        assert_eq!(fs::read_to_string(target).unwrap(), changed);
    }

    pub fn change_cavern_pickup(&self) {
        let target = self.editable_file("scenes/cavern.scene.json");
        let mut scene: serde_json::Value =
            serde_json::from_slice(&fs::read(&target).unwrap()).unwrap();
        let pickup = scene["payload"]["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entity| entity["id"] == "gem-far")
            .unwrap();
        let x = &mut pickup["components"]["nara.transform.Transform2d"]["value"]["value"]["translation"]
            ["value"]["x"]["value"];
        assert_eq!(x.as_f64(), Some(80.0));
        *x = serde_json::json!(81);
        fs::write(&target, serde_json::to_vec_pretty(&scene).unwrap()).unwrap();
        let changed: serde_json::Value =
            serde_json::from_slice(&fs::read(target).unwrap()).unwrap();
        assert_eq!(changed, scene);
    }
}

impl Drop for ProjectCopy {
    fn drop(&mut self) {
        let Ok(directory) = fs::canonicalize(&self.directory) else {
            return;
        };
        if directory.parent() == Some(self.temporary_parent.as_path())
            && directory == self.directory
            && directory != fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap()
        {
            let _ = fs::remove_dir_all(directory);
        }
    }
}
