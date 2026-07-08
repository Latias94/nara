//! Application lifecycle and plugin orchestration for nara.

use std::{
    any::type_name,
    collections::{BTreeMap, HashSet},
    time::Duration,
};

use nara_ecs::{
    Resource, World,
    schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel},
    system::ScheduleSystem,
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ScheduleLabel)]
pub enum StartupStage {
    Core,
    Platform,
    Runtime,
    Scene,
    Tooling,
}

impl StartupStage {
    pub const ALL: [Self; 5] = [
        Self::Core,
        Self::Platform,
        Self::Runtime,
        Self::Scene,
        Self::Tooling,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ScheduleLabel)]
pub enum CoreStage {
    First,
    PreUpdate,
    FixedUpdate,
    Update,
    PostUpdate,
    Extract,
    Prepare,
    Queue,
    Sort,
    Render,
    Cleanup,
    Last,
}

impl CoreStage {
    pub const ALL: [Self; 12] = [
        Self::First,
        Self::PreUpdate,
        Self::FixedUpdate,
        Self::Update,
        Self::PostUpdate,
        Self::Extract,
        Self::Prepare,
        Self::Queue,
        Self::Sort,
        Self::Render,
        Self::Cleanup,
        Self::Last,
    ];
}

pub trait Plugin: Send + Sync + 'static {
    fn build(&self, app: &mut App);

    fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn cleanup(&self, _app: &mut App) {}

    fn name(&self) -> &'static str {
        type_name::<Self>()
    }

    fn is_unique(&self) -> bool {
        true
    }
}

impl<T> Plugin for T
where
    T: Fn(&mut App) + Send + Sync + 'static,
{
    fn build(&self, app: &mut App) {
        self(app);
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginError {
    #[error("duplicate plugin: {name}")]
    Duplicate { name: &'static str },
    #[error("plugins cannot be added after plugin finishing has started: {name}")]
    AddedAfterFinish { name: &'static str },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AppRunError {
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error("app runner failed: {message}")]
    Runner { message: String },
}

impl AppRunError {
    #[must_use]
    pub fn runner(message: impl Into<String>) -> Self {
        Self::Runner {
            message: message.into(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AppExit {
    #[default]
    Success,
    Requested,
}

pub type RunnerFn = Box<dyn FnOnce(App) -> Result<AppExit, AppRunError>>;

#[derive(Clone, Copy, Debug, PartialEq, Resource)]
pub struct Time {
    pub delta_seconds: f32,
    pub elapsed_seconds: f64,
    pub frame: u64,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta_seconds: 0.0,
            elapsed_seconds: 0.0,
            frame: 0,
        }
    }
}

impl Time {
    pub fn advance(&mut self, delta: Duration) {
        self.delta_seconds = delta.as_secs_f32();
        self.elapsed_seconds += delta.as_secs_f64();
        self.frame += 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct FixedTime {
    timestep: Duration,
    max_steps_per_frame: u32,
    accumulated: Duration,
    overstep: Duration,
    steps_this_frame: u32,
}

impl Default for FixedTime {
    fn default() -> Self {
        Self {
            timestep: Self::DEFAULT_TIMESTEP,
            max_steps_per_frame: Self::DEFAULT_MAX_STEPS_PER_FRAME,
            accumulated: Duration::ZERO,
            overstep: Duration::ZERO,
            steps_this_frame: 0,
        }
    }
}

impl FixedTime {
    pub const DEFAULT_TIMESTEP: Duration = Duration::from_nanos(16_666_667);
    pub const DEFAULT_MAX_STEPS_PER_FRAME: u32 = 5;

    #[must_use]
    pub fn new(timestep: Duration) -> Self {
        Self {
            timestep,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_max_steps_per_frame(mut self, max_steps_per_frame: u32) -> Self {
        self.max_steps_per_frame = max_steps_per_frame;
        self
    }

    #[must_use]
    pub fn timestep(&self) -> Duration {
        self.timestep
    }

    #[must_use]
    pub fn max_steps_per_frame(&self) -> u32 {
        self.max_steps_per_frame
    }

    #[must_use]
    pub fn accumulated(&self) -> Duration {
        self.accumulated
    }

    #[must_use]
    pub fn overstep(&self) -> Duration {
        self.overstep
    }

    #[must_use]
    pub fn steps_this_frame(&self) -> u32 {
        self.steps_this_frame
    }

    fn begin_frame(&mut self, delta: Duration) -> u32 {
        self.accumulated = self.accumulated.checked_add(delta).unwrap_or(Duration::MAX);

        let mut steps = 0;
        while self.accumulated >= self.timestep && steps < self.max_steps_per_frame {
            self.accumulated -= self.timestep;
            steps += 1;
        }

        self.steps_this_frame = steps;
        self.overstep = self.accumulated;
        steps
    }
}

pub struct App {
    world: World,
    startup_schedules: BTreeMap<StartupStage, Schedule>,
    schedules: BTreeMap<CoreStage, Schedule>,
    runner: Option<RunnerFn>,
    plugins: Vec<Box<dyn Plugin>>,
    plugin_names: HashSet<&'static str>,
    plugins_finished: bool,
    started: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let plugins = std::mem::take(&mut self.plugins);
        for plugin in &plugins {
            plugin.cleanup(self);
        }
        self.plugins = plugins;
    }
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.insert_resource(FixedTime::default());

        let startup_schedules = StartupStage::ALL
            .into_iter()
            .map(|stage| (stage, Schedule::new(stage)))
            .collect();
        let schedules = CoreStage::ALL
            .into_iter()
            .map(|stage| (stage, Schedule::new(stage)))
            .collect();

        Self {
            world,
            startup_schedules,
            schedules,
            runner: Some(Box::new(default_runner)),
            plugins: Vec::new(),
            plugin_names: HashSet::new(),
            plugins_finished: false,
            started: false,
        }
    }

    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> &mut Self {
        self.world.insert_resource(resource);
        self
    }

    pub fn init_resource<R>(&mut self) -> &mut Self
    where
        R: Resource + nara_ecs::world::FromWorld,
    {
        self.world.init_resource::<R>();
        self
    }

    pub fn add_startup_systems<M>(
        &mut self,
        stage: StartupStage,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.startup_schedule_mut(stage).add_systems(systems);
        self
    }

    pub fn add_systems<M>(
        &mut self,
        stage: CoreStage,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.schedule_mut(stage).add_systems(systems);
        self
    }

    pub fn set_runner(
        &mut self,
        runner: impl FnOnce(App) -> Result<AppExit, AppRunError> + 'static,
    ) -> &mut Self {
        self.runner = Some(Box::new(runner));
        self
    }

    pub fn add_plugin(&mut self, plugin: impl Plugin) -> Result<&mut Self, PluginError> {
        let name = plugin.name();
        if self.plugins_finished {
            return Err(PluginError::AddedAfterFinish { name });
        }
        if plugin.is_unique() && self.plugin_names.contains(name) {
            return Err(PluginError::Duplicate { name });
        }

        plugin.build(self);
        self.plugin_names.insert(name);
        self.plugins.push(Box::new(plugin));
        Ok(self)
    }

    pub fn finish_plugins(&mut self) -> Result<&mut Self, PluginError> {
        if self.plugins_finished {
            return Ok(self);
        }

        self.plugins_finished = true;
        let plugins = std::mem::take(&mut self.plugins);
        for plugin in &plugins {
            plugin.finish(self)?;
        }
        self.plugins = plugins;
        Ok(self)
    }

    pub fn run(mut self) -> Result<AppExit, AppRunError> {
        self.finish_plugins()?;
        let runner = self
            .runner
            .take()
            .unwrap_or_else(|| Box::new(default_runner));
        runner(self)
    }

    pub fn run_once(&mut self, delta: Duration) -> Result<(), AppRunError> {
        self.finish_plugins()?;

        if !self.started {
            for stage in StartupStage::ALL {
                if let Some(schedule) = self.startup_schedules.get_mut(&stage) {
                    schedule.run(&mut self.world);
                }
            }
            self.started = true;
        }

        self.world.resource_mut::<Time>().advance(delta);
        let fixed_steps = self.world.resource_mut::<FixedTime>().begin_frame(delta);

        for stage in CoreStage::ALL {
            if let Some(schedule) = self.schedules.get_mut(&stage) {
                if stage == CoreStage::FixedUpdate {
                    for _ in 0..fixed_steps {
                        schedule.run(&mut self.world);
                    }
                } else {
                    schedule.run(&mut self.world);
                }
            }
        }

        Ok(())
    }

    pub fn try_update(&mut self) -> Result<(), AppRunError> {
        self.run_once(Duration::ZERO)
    }

    pub fn update(&mut self) {
        self.try_update()
            .expect("app update failed while running one frame");
    }

    fn startup_schedule_mut(&mut self, stage: StartupStage) -> &mut Schedule {
        self.startup_schedules
            .entry(stage)
            .or_insert_with(|| Schedule::new(stage))
    }

    fn schedule_mut(&mut self, stage: CoreStage) -> &mut Schedule {
        self.schedules
            .entry(stage)
            .or_insert_with(|| Schedule::new(stage))
    }
}

fn default_runner(mut app: App) -> Result<AppExit, AppRunError> {
    app.run_once(Duration::ZERO)?;
    Ok(AppExit::Success)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::{Commands, Component, ResMut, Resource};

    #[derive(Debug, Default, Resource)]
    struct Frames(u32);

    #[derive(Debug, Component)]
    struct Spawned;

    #[derive(Debug, Default, Resource)]
    struct Order(Vec<&'static str>);

    fn spawn_entity(mut commands: Commands) {
        commands.spawn(Spawned);
    }

    fn count_frame(mut frames: ResMut<Frames>) {
        frames.0 += 1;
    }

    fn push_first(mut order: ResMut<Order>) {
        order.0.push("first");
    }

    fn push_pre_update(mut order: ResMut<Order>) {
        order.0.push("pre_update");
    }

    fn push_fixed_update(mut order: ResMut<Order>) {
        order.0.push("fixed_update");
    }

    fn push_update(mut order: ResMut<Order>) {
        order.0.push("update");
    }

    fn push_extract(mut order: ResMut<Order>) {
        order.0.push("extract");
    }

    fn push_prepare(mut order: ResMut<Order>) {
        order.0.push("prepare");
    }

    fn push_queue(mut order: ResMut<Order>) {
        order.0.push("queue");
    }

    fn push_sort(mut order: ResMut<Order>) {
        order.0.push("sort");
    }

    fn push_render(mut order: ResMut<Order>) {
        order.0.push("render");
    }

    fn push_cleanup(mut order: ResMut<Order>) {
        order.0.push("cleanup");
    }

    fn push_last(mut order: ResMut<Order>) {
        order.0.push("last");
    }

    #[test]
    fn update_runs_startup_once_and_update_every_frame() {
        let mut app = App::new();
        app.insert_resource(Frames::default())
            .add_startup_systems(StartupStage::Core, spawn_entity)
            .add_systems(CoreStage::Update, (spawn_entity, count_frame));

        app.update();
        app.update();

        let spawned_count = {
            let world = app.world_mut();
            let mut query = world.query::<&Spawned>();
            query.iter(world).count()
        };

        assert_eq!(spawned_count, 3);
        assert_eq!(app.world().resource::<Frames>().0, 2);
    }

    #[test]
    fn run_once_advances_time_and_runs_fixed_update_when_due() {
        let mut app = App::new();
        app.insert_resource(Frames::default())
            .add_systems(CoreStage::FixedUpdate, count_frame);

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        let time = app.world().resource::<Time>();
        assert_eq!(time.frame, 1);
        assert_eq!(
            time.delta_seconds,
            FixedTime::DEFAULT_TIMESTEP.as_secs_f32()
        );
        assert_eq!(app.world().resource::<Frames>().0, 1);
        assert_eq!(app.world().resource::<FixedTime>().steps_this_frame(), 1);
    }

    #[test]
    fn fixed_update_does_not_run_until_accumulator_is_due() {
        let mut app = App::new();
        app.insert_resource(Frames::default())
            .add_systems(CoreStage::FixedUpdate, count_frame);

        app.run_once(FixedTime::DEFAULT_TIMESTEP / 2).unwrap();

        assert_eq!(app.world().resource::<Frames>().0, 0);
        assert_eq!(app.world().resource::<FixedTime>().steps_this_frame(), 0);
    }

    #[test]
    fn fixed_update_limits_catch_up_ticks() {
        let mut app = App::new();
        app.insert_resource(Frames::default())
            .insert_resource(FixedTime::default().with_max_steps_per_frame(2))
            .add_systems(CoreStage::FixedUpdate, count_frame);

        app.run_once(FixedTime::DEFAULT_TIMESTEP * 5).unwrap();

        assert_eq!(app.world().resource::<Frames>().0, 2);
        assert_eq!(app.world().resource::<FixedTime>().steps_this_frame(), 2);
        assert!(app.world().resource::<FixedTime>().accumulated() >= FixedTime::DEFAULT_TIMESTEP);
    }

    #[test]
    fn first_pre_fixed_and_update_run_in_order() {
        let mut app = App::new();
        app.insert_resource(Order::default())
            .add_systems(CoreStage::First, push_first)
            .add_systems(CoreStage::PreUpdate, push_pre_update)
            .add_systems(CoreStage::FixedUpdate, push_fixed_update)
            .add_systems(CoreStage::Update, push_update);

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            app.world().resource::<Order>().0,
            ["first", "pre_update", "fixed_update", "update"]
        );
    }

    #[test]
    fn render_pipeline_stages_run_in_order() {
        let mut app = App::new();
        app.insert_resource(Order::default())
            .add_systems(CoreStage::Extract, push_extract)
            .add_systems(CoreStage::Prepare, push_prepare)
            .add_systems(CoreStage::Queue, push_queue)
            .add_systems(CoreStage::Sort, push_sort)
            .add_systems(CoreStage::Render, push_render)
            .add_systems(CoreStage::Cleanup, push_cleanup)
            .add_systems(CoreStage::Last, push_last);

        app.run_once(Duration::ZERO).unwrap();

        assert_eq!(
            app.world().resource::<Order>().0,
            [
                "extract", "prepare", "queue", "sort", "render", "cleanup", "last"
            ]
        );
    }

    #[test]
    fn run_consumes_app_with_custom_runner() {
        let mut app = App::new();
        app.set_runner(|mut app| {
            app.run_once(Duration::ZERO)?;
            Ok(AppExit::Requested)
        });

        assert_eq!(app.run().unwrap(), AppExit::Requested);
    }

    #[test]
    fn runner_failure_is_reported_without_panic() {
        let mut app = App::new();
        app.set_runner(|_app| Err(AppRunError::runner("window creation failed")));

        assert_eq!(
            app.run().unwrap_err(),
            AppRunError::runner("window creation failed")
        );
    }
}
