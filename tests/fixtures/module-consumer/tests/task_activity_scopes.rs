use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nara_engine::{
    app::{App, RuntimeControl, RuntimeInstance, RuntimeState},
    core::ItemLimit,
    tasks::{
        TaskDomainKey, TaskPlugin, TaskPoolConfig, TaskPoolKind, TaskPools, TaskScopeCloseReport,
        TaskShutdownReport, TaskSpawnRequest, TaskTerminal,
    },
};

fn stop(runtime: &mut RuntimeInstance) {
    runtime.request_control(RuntimeControl::Stop);
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.state() != RuntimeState::Stopped {
        assert!(Instant::now() < deadline, "activity close did not complete");
        runtime.drive(Duration::ZERO).unwrap();
        std::thread::yield_now();
    }
}

#[test]
fn renamed_root_and_direct_tasks_share_facilities_with_independent_runtime_custody() {
    // The renamed root and direct module expose the same physical owner type.
    let facilities: nara_engine::advanced_prelude::TaskFacilities =
        nara_tasks::TaskFacilities::inline_for_tests(
            TaskPoolConfig::threaded(ItemLimit::ONE, ItemLimit::ONE, ItemLimit::ONE).unwrap(),
        )
        .unwrap();
    let facilities = Arc::new(facilities);
    let mut first = App::new();
    first
        .add_plugin(TaskPlugin::shared(facilities.clone()))
        .unwrap();
    let mut second = App::new();
    second
        .add_plugin(TaskPlugin::shared(facilities.clone()))
        .unwrap();
    let mut first = RuntimeInstance::start(first).unwrap();
    let mut second = RuntimeInstance::start(second).unwrap();
    let request = TaskSpawnRequest::new(1, TaskDomainKey::new(4));
    let mut pending = first
        .world()
        .resource::<TaskPools>()
        .spawn(TaskPoolKind::Io, request, |_| 17)
        .into_handle()
        .unwrap();
    stop(&mut first);
    assert!(matches!(
        pending.try_take(),
        Some(TaskTerminal::Cancelled(_))
    ));
    assert!(
        first
            .world()
            .resource::<TaskScopeCloseReport>()
            .is_complete()
    );
    assert!(!first.world().contains_resource::<TaskPools>());
    assert!(!first.world().contains_resource::<TaskShutdownReport>());
    let mut sibling = second
        .world()
        .resource::<TaskPools>()
        .spawn(TaskPoolKind::Io, request, |_| 29)
        .into_handle()
        .unwrap();
    assert_eq!(facilities.run_pending_for_tests().executed, 1);
    assert_eq!(sibling.try_take(), Some(TaskTerminal::Completed(29)));
    assert!(sibling.admission_to_start().is_some());
    assert!(pending.admission_to_start().is_none());
    stop(&mut second);
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn lazy_facilities_expose_first_use_without_allocating_unused_worker_kinds() {
    let facilities = Arc::new(
        nara_engine::tasks::TaskFacilities::try_new_lazy(
            TaskPoolConfig::threaded(ItemLimit::ONE, ItemLimit::ONE, ItemLimit::ONE).unwrap(),
        )
        .unwrap(),
    );
    let mut app = App::new();
    app.add_plugin(nara_tasks::TaskPlugin::shared(facilities.clone()))
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    for kind in TaskPoolKind::ALL {
        assert_eq!(facilities.stats().for_kind(kind).live_workers, 0);
    }
    let mut task = runtime
        .world()
        .resource::<TaskPools>()
        .spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(2, TaskDomainKey::new(7)),
            |_| 41,
        )
        .into_handle()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while task.return_observer().state() == nara_engine::tasks::TaskReturnState::Pending {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(task.try_take(), Some(TaskTerminal::Completed(41)));
    assert!(task.admission_to_start().is_some());
    assert_eq!(
        facilities.stats().for_kind(TaskPoolKind::Io).live_workers,
        1
    );
    assert_eq!(
        facilities
            .stats()
            .for_kind(TaskPoolKind::Compute)
            .live_workers,
        0
    );
    stop(&mut runtime);
    let report = facilities.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert_eq!(report.for_kind(TaskPoolKind::Io).joined_workers, 1);
    assert_eq!(report.for_kind(TaskPoolKind::Compute).joined_workers, 0);
}
