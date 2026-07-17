use std::time::Duration;

use nara_app::{App, AppScheduleRunError, CoreStage, PluginLifecycleState, StartupStage};
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
fn custom_typed_schedule_is_inspectable_inert_and_explicitly_driven() {
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
    app.run_once(Duration::ZERO).unwrap();
    assert!(app.world().resource::<DomainTrace>().0.is_empty());

    app.run_schedule(DomainSchedule).unwrap();
    assert_eq!(
        app.world().resource::<DomainTrace>().0,
        ["prepare", "simulate"]
    );
    assert_eq!(
        app.run_schedule(MissingSchedule),
        Err(AppScheduleRunError::MissingSchedule)
    );
    assert!(matches!(
        app.get_schedule_mut(DomainSchedule),
        Err(nara_app::PluginError::AppSealed)
    ));
}

#[test]
fn custom_schedule_seals_before_it_executes() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.init_schedule(DomainSchedule).unwrap();
    app.add_systems(DomainSchedule, record_prepare).unwrap();

    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );

    app.run_schedule(DomainSchedule).unwrap();

    assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Ready);
    assert_eq!(app.world().resource::<DomainTrace>().0, ["prepare"]);
}

#[test]
fn missing_custom_schedule_does_not_seal_the_app() {
    let mut app = App::new();

    assert_eq!(
        app.run_schedule(MissingSchedule),
        Err(AppScheduleRunError::MissingSchedule)
    );
    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );
}

#[test]
fn generic_schedule_entry_rejects_builtin_stages_without_consuming_startup() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.add_systems(StartupStage::Core, record_prepare).unwrap();
    app.add_systems(CoreStage::Update, record_simulate).unwrap();

    for _ in 0..2 {
        assert_eq!(
            app.run_schedule(StartupStage::Core),
            Err(AppScheduleRunError::BuiltInSchedule)
        );
        assert_eq!(
            app.run_schedule(CoreStage::Update),
            Err(AppScheduleRunError::BuiltInSchedule)
        );
    }

    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );
    assert!(app.world().resource::<DomainTrace>().0.is_empty());

    app.run_once(Duration::ZERO).unwrap();
    assert_eq!(
        app.world().resource::<DomainTrace>().0,
        ["prepare", "simulate"]
    );
}

fn record_prepare(mut trace: ResMut<DomainTrace>) {
    trace.0.push("prepare");
}

fn record_simulate(mut trace: ResMut<DomainTrace>) {
    trace.0.push("simulate");
}
