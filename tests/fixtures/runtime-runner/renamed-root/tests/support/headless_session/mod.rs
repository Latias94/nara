use std::{
    fs::File,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use engine::{
    ProductRecipe,
    app::{
        App, AppExitRequests, CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration,
        PluginError, PluginId, PluginShutdownObligationId, RealTime, RuntimeCloseContext,
        RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
        RuntimeCloseProgress, RuntimeFaultDetail, RuntimeGeneration, RuntimeTimeSettings,
        StartupStage,
    },
    ecs::{Commands, Res, ResMut, Resource, World, schedule::IntoScheduleConfigs},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    gameplay::{
        GameplayCommandBatch, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandKey, GameplayCommandPayload, GameplayCommandSet,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTick,
        GameplayCommandTypeId, GameplayCommandValue,
    },
    project_host::{HeadlessSession, HeadlessSessionClose},
};

pub const FIXED_DELTA: Duration = Duration::from_millis(20);
pub const STARTUP_FAILURE: &str = "fixture.headless-session.startup-failed";
pub const EXECUTION_FAILURE: &str = "fixture.headless-session.execution-failed";
const ADJUST: &str = "fixture.headless-session.adjust";
const EXIT: &str = "fixture.headless-session.exit";
const PLUGIN: PluginId = PluginId::new("fixture.headless-session");
const SHUTDOWN: PluginShutdownObligationId =
    PluginShutdownObligationId::new("fixture.headless-session.shutdown");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub generation: u64,
    pub tick: u64,
    pub elapsed: Duration,
    pub frame: u64,
    pub last_regular_frame: u64,
    pub last_observed_tick: u64,
    pub task_update_ticks: Vec<u64>,
    pub value: i64,
    pub simulated_ticks: Vec<u64>,
    pub deferred_ticks: Vec<u64>,
    pub consumed: Vec<GameplayCommandKey>,
}

#[derive(Default, Resource)]
struct GameState {
    last_regular_frame: u64,
    last_observed_tick: u64,
    task_update_ticks: Vec<u64>,
    value: i64,
    simulated_ticks: Vec<u64>,
    deferred_ticks: Vec<u64>,
    consumed: Vec<GameplayCommandKey>,
}

pub fn snapshot(world: &World) -> Snapshot {
    let state = world.resource::<GameState>();
    let time = world.resource::<FixedTime>();
    Snapshot {
        generation: world.resource::<RuntimeGeneration>().get(),
        tick: time.tick(),
        elapsed: time.elapsed(),
        frame: world.resource::<RealTime>().frame,
        last_regular_frame: state.last_regular_frame,
        last_observed_tick: state.last_observed_tick,
        task_update_ticks: state.task_update_ticks.clone(),
        value: state.value,
        simulated_ticks: state.simulated_ticks.clone(),
        deferred_ticks: state.deferred_ticks.clone(),
        consumed: state.consumed.clone(),
    }
}

pub struct CleanupObservation {
    pub ready: AtomicBool,
    pub fail_release: AtomicBool,
    pub begins: AtomicUsize,
    pub polls: AtomicUsize,
    pub released: AtomicBool,
    pub drops: AtomicUsize,
}

impl Default for CleanupObservation {
    fn default() -> Self {
        Self {
            ready: AtomicBool::new(true),
            fail_release: AtomicBool::new(false),
            begins: AtomicUsize::new(0),
            polls: AtomicUsize::new(0),
            released: AtomicBool::new(false),
            drops: AtomicUsize::new(0),
        }
    }
}

#[derive(Resource)]
struct CleanupService(Arc<CleanupObservation>);

pub fn cleanup(world: &World) -> Arc<CleanupObservation> {
    Arc::clone(&world.resource::<CleanupService>().0)
}

struct CloseParticipant(Arc<CleanupObservation>);

impl RuntimeCloseParticipant for CloseParticipant {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.begins.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.polls.fetch_add(1, Ordering::SeqCst);
        if !self.0.ready.load(Ordering::SeqCst) {
            return Ok(RuntimeCloseProgress::Pending);
        }
        self.0.released.store(true, Ordering::SeqCst);
        Ok(if self.0.fail_release.load(Ordering::SeqCst) {
            RuntimeCloseProgress::CompleteWithError {
                code: "fixture.headless-session.release-failed",
            }
        } else {
            RuntimeCloseProgress::Complete
        })
    }
}

impl Drop for CloseParticipant {
    fn drop(&mut self) {
        self.0.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct ProbePlugin;

impl Plugin for ProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PLUGIN, PluginCategory::Runtime)
                .requires_plugins(&[engine::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID])
                .shutdown_obligations(&[SHUTDOWN]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let observation = Arc::new(CleanupObservation::default());
        app.init_resource::<GameState>()?;
        app.insert_resource(CleanupService(Arc::clone(&observation)))?;
        app.register_plugin_runtime_close_participant(
            SHUTDOWN,
            RuntimeCloseParticipantId::new("fixture.headless-session.close"),
            CloseParticipant(observation),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            consume.in_set(GameplayCommandSet::Consume),
        )?;
        app.add_systems(CoreStage::TaskUpdate, record_task_update)?;
        app.add_systems(CoreStage::Last, complete_regular_frame)?;
        Ok(())
    }
}

fn consume(
    fixed: Res<FixedTime>,
    batch: Res<GameplayCommandBatch>,
    mut state: ResMut<GameState>,
    mut commands: Commands,
    mut exits: ResMut<AppExitRequests>,
) {
    let tick = fixed.tick();
    assert_eq!(batch.active_tick().unwrap().get(), tick);
    state.simulated_ticks.push(tick);
    for command in batch.iter() {
        match command.command_type().as_str() {
            ADJUST => {
                let Some(GameplayCommandValue::I64(amount)) = command.payload().get("amount")
                else {
                    panic!("the public consumer submitted an invalid adjustment payload");
                };
                state.value += *amount;
            }
            EXIT => exits.request_success(),
            other => panic!("unexpected gameplay command in the source fixture: {other}"),
        }
        state.consumed.push(command.key().clone());
    }
    commands.queue(move |world: &mut World| {
        world.resource_mut::<GameState>().deferred_ticks.push(tick);
    });
}

fn record_task_update(fixed: Res<FixedTime>, mut commands: Commands) {
    let tick = fixed.tick();
    commands.queue(move |world: &mut World| {
        world
            .resource_mut::<GameState>()
            .task_update_ticks
            .push(tick);
    });
}

fn complete_regular_frame(real: Res<RealTime>, fixed: Res<FixedTime>, mut commands: Commands) {
    let frame = real.frame;
    let tick = fixed.tick();
    commands.queue(move |world: &mut World| {
        let mut state = world.resource_mut::<GameState>();
        state.last_regular_frame = frame;
        state.last_observed_tick = tick;
    });
}

#[derive(Default)]
pub struct StartPaused;

impl Plugin for StartPaused {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("fixture.headless-session.start-paused"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[PLUGIN]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            StartupStage::Core,
            |mut settings: ResMut<RuntimeTimeSettings>| {
                settings.set_paused(true);
            },
        )?;
        Ok(())
    }
}

#[derive(Default, Resource)]
struct Readiness(bool);

pub fn simulation_ready(world: &World) -> bool {
    world.resource::<Readiness>().0
}

#[derive(Default)]
pub struct ReadyAfterTaskUpdate;

impl Plugin for ReadyAfterTaskUpdate {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("fixture.headless-session.delayed-readiness"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[PLUGIN]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Readiness>()?;
        app.add_simulation_condition(simulation_ready)?;
        app.add_systems(CoreStage::TaskUpdate, |mut readiness: ResMut<Readiness>| {
            readiness.0 = true;
        })?;
        Ok(())
    }
}

#[derive(Default)]
pub struct FailStartup;

impl Plugin for FailStartup {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("fixture.headless-session.fail-startup"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[PLUGIN]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Runtime, fail_candidate_startup)?;
        Ok(())
    }
}

fn fail_candidate_startup() -> Result<(), engine::ecs::bevy_ecs::error::BevyError> {
    Err(RuntimeFaultDetail::new(
        STARTUP_FAILURE,
        "The candidate deliberately failed during Startup",
        "fixture.headless-session.candidate",
    )
    .unwrap()
    .into_bevy_error())
}

#[derive(Default)]
pub struct FailFrame;

impl Plugin for FailFrame {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("fixture.headless-session.fail-frame"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[PLUGIN]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(CoreStage::Last, fail_frame)?;
        Ok(())
    }
}

fn fail_frame() -> Result<(), engine::ecs::bevy_ecs::error::BevyError> {
    Err(RuntimeFaultDetail::new(
        EXECUTION_FAILURE,
        "The retained game deliberately failed during a complete frame",
        "fixture.headless-session.frame",
    )
    .unwrap()
    .into_bevy_error())
}

pub fn recipe() -> ProductRecipe {
    ProductRecipe::new().add_plugin::<ProbePlugin>().unwrap()
}

pub fn session() -> HeadlessSession {
    HeadlessSession::new(project_root(), recipe())
}

pub fn adjust(tick: u64, sequence: u64, amount: i64) -> GameplayCommandSubmission {
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("amount", GameplayCommandValue::I64(amount))
        .unwrap();
    submission(tick, sequence, ADJUST, payload)
}

pub fn exit(tick: u64, sequence: u64) -> GameplayCommandSubmission {
    submission(tick, sequence, EXIT, GameplayCommandPayload::new())
}

fn submission(
    tick: u64,
    sequence: u64,
    command: &str,
    payload: GameplayCommandPayload,
) -> GameplayCommandSubmission {
    GameplayCommandSubmission::new(
        GameplayCommandTick::new(tick).unwrap(),
        GameplayCommandIngressSource::external("fixture.headless-session.driver").unwrap(),
        GameplayCommandSourceSequence::new(sequence).unwrap(),
        GameplayCommandDraft::new(GameplayCommandTypeId::new(command).unwrap())
            .with_payload(payload),
    )
}

pub fn close(session: &mut HeadlessSession) -> HeadlessSessionClose {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match session.close() {
            HeadlessSessionClose::Pending => {
                assert!(
                    Instant::now() < deadline,
                    "the original source Host did not finish closing"
                );
                std::thread::yield_now();
            }
            terminal => return terminal,
        }
    }
}

pub fn project_root() -> DirectoryCapability {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("project");
    let file = open_directory(&path).unwrap();
    let trust = if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    };
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, trust),
    )
    .unwrap()
}

#[cfg(windows)]
fn open_directory(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn open_directory(path: &Path) -> std::io::Result<File> {
    File::open(path)
}
