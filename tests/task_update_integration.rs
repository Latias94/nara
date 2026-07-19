use std::time::Duration;

use nara::{
    app::{App, CoreStage},
    asset::{AssetPlugin, AssetTaskUpdateSet},
    ecs::{ResMut, Resource, schedule::IntoScheduleConfigs},
    tasks::TaskPlugin,
};

#[derive(Debug, Default, Resource)]
struct IntegrationOrder(Vec<&'static str>);

fn record_poll(mut order: ResMut<IntegrationOrder>) {
    order.0.push("poll");
}

fn record_resolve(mut order: ResMut<IntegrationOrder>) {
    order.0.push("resolve");
}

fn record_spawn(mut order: ResMut<IntegrationOrder>) {
    order.0.push("spawn");
}

fn record_apply(mut order: ResMut<IntegrationOrder>) {
    order.0.push("apply");
}

#[test]
fn asset_plugin_owns_the_complete_task_update_chain() {
    let mut app = App::new();
    app.insert_resource(IntegrationOrder::default()).unwrap();
    app.add_plugins((TaskPlugin::default(), AssetPlugin))
        .unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        record_apply.in_set(AssetTaskUpdateSet::ApplyResults),
    )
    .unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        record_spawn.in_set(AssetTaskUpdateSet::SpawnJobs),
    )
    .unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        record_resolve.in_set(AssetTaskUpdateSet::ResolveSourceChanges),
    )
    .unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        record_poll.in_set(AssetTaskUpdateSet::Poll),
    )
    .unwrap();

    app.run_once(Duration::ZERO).unwrap();

    assert_eq!(
        app.world().resource::<IntegrationOrder>().0,
        ["poll", "resolve", "spawn", "apply"]
    );
}

#[test]
fn foundational_app_and_task_modules_do_not_own_asset_phase_vocabulary() {
    let app_source = include_str!("../crates/nara_app/src/lib.rs");
    let task_source = include_str!("../crates/nara_tasks/src/runtime.rs");

    assert!(!app_source.contains("TaskUpdateSet"));
    assert!(!app_source.contains("AssetTaskUpdateSet"));
    assert!(!task_source.contains("AssetTaskUpdateSet"));
    assert!(!task_source.contains("ResolveSourceChanges"));
    assert!(!task_source.contains("SpawnJobs"));
    assert!(!task_source.contains("ApplyResults"));
}
