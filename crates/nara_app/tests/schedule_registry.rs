use std::time::Duration;

use nara_app::{App, CoreStage, PluginLifecycleState, RuntimeInstance, StartupStage};
use nara_ecs::{
    ResMut, Resource,
    schedule::{IntoScheduleConfigs, ScheduleLabel, SystemSet},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct DomainSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct MissingSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
enum DomainSet {
    Prepare,
    Simulate,
}

#[derive(Debug, Default, Resource)]
struct DomainTrace(Vec<&'static str>);

#[test]
fn custom_typed_schedule_is_inspectable_and_inert_during_managed_frames() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.init_schedule(DomainSchedule).unwrap();
    app.configure_sets(
        DomainSchedule,
        (DomainSet::Prepare, DomainSet::Simulate).chain(),
    )
    .unwrap();
    app.add_systems(
        DomainSchedule,
        (
            record_prepare.in_set(DomainSet::Prepare),
            record_simulate.in_set(DomainSet::Simulate),
        ),
    )
    .unwrap();

    assert!(app.get_schedule(DomainSchedule).is_some());
    assert!(matches!(app.get_schedule_mut(DomainSchedule), Ok(Some(_))));
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<DomainTrace>().0.is_empty());
}

#[test]
fn custom_schedule_configuration_is_frozen_by_seal() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.init_schedule(DomainSchedule).unwrap();
    app.add_systems(DomainSchedule, record_prepare).unwrap();

    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );

    let sealed = app.seal().unwrap();
    assert!(sealed.world().resource::<DomainTrace>().0.is_empty());
}

#[test]
fn missing_custom_schedule_is_observed_without_sealing_the_app() {
    let app = App::new();

    assert!(app.get_schedule(MissingSchedule).is_none());
    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );
}

#[test]
fn managed_startup_runs_once_and_the_frame_uses_engine_stage_order() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.add_systems(StartupStage::Core, record_prepare).unwrap();
    app.add_systems(CoreStage::Update, record_simulate).unwrap();

    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );
    assert!(app.world().resource::<DomainTrace>().0.is_empty());

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<DomainTrace>().0,
        ["prepare", "simulate"]
    );
}

fn record_prepare(mut trace: ResMut<DomainTrace>) {
    trace.0.push("prepare");
}

fn record_simulate(mut trace: ResMut<DomainTrace>) {
    trace.0.push("simulate");
}
