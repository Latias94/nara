//! Application lifecycle and plugin orchestration for nara.

use std::{
    any::type_name,
    collections::{BTreeMap, HashSet},
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
    PreUpdate,
    Update,
    PostUpdate,
    Extract,
    Render,
    Last,
}

impl CoreStage {
    pub const ALL: [Self; 6] = [
        Self::PreUpdate,
        Self::Update,
        Self::PostUpdate,
        Self::Extract,
        Self::Render,
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

pub struct App {
    world: World,
    startup_schedules: BTreeMap<StartupStage, Schedule>,
    schedules: BTreeMap<CoreStage, Schedule>,
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
        let startup_schedules = StartupStage::ALL
            .into_iter()
            .map(|stage| (stage, Schedule::new(stage)))
            .collect();
        let schedules = CoreStage::ALL
            .into_iter()
            .map(|stage| (stage, Schedule::new(stage)))
            .collect();

        Self {
            world: World::new(),
            startup_schedules,
            schedules,
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

    pub fn try_update(&mut self) -> Result<(), PluginError> {
        self.finish_plugins()?;

        if !self.started {
            for stage in StartupStage::ALL {
                if let Some(schedule) = self.startup_schedules.get_mut(&stage) {
                    schedule.run(&mut self.world);
                }
            }
            self.started = true;
        }

        for stage in CoreStage::ALL {
            if let Some(schedule) = self.schedules.get_mut(&stage) {
                schedule.run(&mut self.world);
            }
        }

        Ok(())
    }

    pub fn update(&mut self) {
        self.try_update()
            .expect("app update failed while finishing plugins");
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

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::{Commands, Component, ResMut, Resource};

    #[derive(Debug, Default, Resource)]
    struct Frames(u32);

    #[derive(Debug, Component)]
    struct Spawned;

    fn spawn_entity(mut commands: Commands) {
        commands.spawn(Spawned);
    }

    fn count_frame(mut frames: ResMut<Frames>) {
        frames.0 += 1;
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
}
