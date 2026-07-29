#[path = "support/manual_raw_app_boot.rs"]
mod manual_raw_app_boot;
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use manual_raw_app_boot::{
    ManualRawAppBootError, ManualRawAppFault, ManualRetirementCustody, run_manual_raw_app_boot,
    run_manual_raw_app_fault, run_manual_raw_app_incomplete_retirement,
    run_manual_raw_app_pre_owner_failure,
};
use nara::prelude::Vec2;
use nara::tasks::{TaskPoolKind, TaskShutdownPhase};

use crate::project_content_fixture::ProjectContentFixtureError;

#[test]
fn committed_manual_raw_app_task_reaches_the_frozen_first_tick_and_retires_owners() {
    let report = run_manual_raw_app_boot().unwrap();

    assert_eq!(report.first_tick.tick, 1);
    assert_eq!(report.first_tick.player_position, Vec2::ZERO);
    assert_eq!(report.first_tick.player_hit_points, 20);
    assert_eq!(report.first_tick.enemy_position, Vec2::new(4.5, 0.0));
    assert_eq!(report.first_tick.enemy_hit_points, 10);
    assert_eq!(report.first_tick.weapon_remaining_ticks, 2);

    assert_eq!(report.command_stats.accepted, 1);
    assert_eq!(report.command_stats.admitted, 1);
    assert_eq!(report.command_stats.acknowledged, 1);
    assert_eq!(report.command_stats.retained_commands, 0);
    assert!(report.command_queue_idle);

    assert_eq!(
        report.plugin_plan_fingerprint,
        "c822c54168160ed7c23c89ceb2fd0f23d87d0012cc2607388db95fbbaea5ecb2"
    );
    assert_eq!(
        report.schema_fingerprint,
        "9ff58034629ae8637edcef3aa99eacde21c5bce1a9ffaade008d1a3b203904ab"
    );
    assert_eq!(
        report.content_revision,
        "60d9ff54b0aadf73d19142b977ebe24a73a7967d5fe5ad9cfbce554e9f02e7d4"
    );
    assert_eq!(
        report.content_digest,
        "962a016b5eb275fd8bff78be1d8d15dfb4d35a8491ceb20cfe0e1b8fcabbb996"
    );
    assert_eq!(
        report.command_digest,
        "42d5de0e3c23bd3d731f1d2a116f767adb43fa4df7f2722692058f480a888c2b"
    );
    assert!(!report.task_shutdown.timed_out());
    assert_eq!(report.joined_task_workers, report.expected_task_workers);
    for kind in TaskPoolKind::ALL {
        assert_eq!(report.task_shutdown.for_kind(kind).panicked_workers, 0);
    }
}

#[test]
fn late_persistent_hook_rejects_before_scene_allocation() {
    let rejection = run_manual_raw_app_fault(ManualRawAppFault::LatePersistentHook).unwrap();

    assert_eq!(
        rejection.diagnostic_code,
        "scene.persistent-apply-ineligible"
    );
    assert_eq!(rejection.persistent_apply_reason, "lifecycle-hook");
    assert_eq!(rejection.lifecycle_event, "add");
    assert_eq!(rejection.hook_calls, 0);
    assert_eq!(rejection.entities_before, rejection.entities_after);
    assert!(!rejection.scene_published);
    assert!(!rejection.task_shutdown.timed_out());
}

#[test]
fn project_content_failure_precedes_manual_app_ownership() {
    let failure = run_manual_raw_app_pre_owner_failure().unwrap_err();

    assert_eq!(
        failure.primary,
        ManualRawAppBootError::ProjectContent(ProjectContentFixtureError::OpenManifest)
    );
    assert!(failure.retirement.is_none());
}

#[test]
fn stalled_required_task_retains_app_custody_until_retry_completes() {
    let report = run_manual_raw_app_incomplete_retirement().unwrap();

    assert_eq!(report.diagnostic_class, ManualRawAppBootError::TaskShutdown);
    assert_eq!(report.first_tick.tick, 1);
    assert_eq!(report.first_tick.player_position, Vec2::ZERO);
    assert_eq!(report.first_tick.enemy_hit_points, 10);
    assert!(report.scene_published);
    assert!(!report.runtime_published);
    assert!(report.incomplete_task_shutdown.timed_out());
    assert!(
        report
            .incomplete_task_shutdown
            .for_kind(TaskPoolKind::Io)
            .timed_out()
    );
    assert_eq!(report.incomplete_phase, TaskShutdownPhase::Join);
    assert_eq!(report.custody, ManualRetirementCustody::AppWorld);
    assert_eq!(report.joined_task_workers, report.expected_task_workers);
    for kind in TaskPoolKind::ALL {
        assert_eq!(
            report
                .completed_task_shutdown
                .for_kind(kind)
                .panicked_workers,
            0
        );
    }
    assert!(report.plugin_shutdown_complete);
}
