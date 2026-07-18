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
    assert_eq!(report.first_tick.player_position, Vec2::new(1.0, 0.0));
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
        "957780ddaae1597b3a3925cabac17330ca1c99d357246b9a60091f14cd08d701"
    );
    assert_eq!(
        report.schema_fingerprint,
        "bbdd60c2f559a1807d2c3429caed5b151ae5436fe1b6be037013ebd6b4bbb17b"
    );
    assert_eq!(
        report.content_revision,
        "4fe474f07f3a0c1fbf081b2da76da2c39bc9a30b80ca0e8a3165fddafdfac47e"
    );
    assert_eq!(
        report.content_digest,
        "0aab556515aca592942b6955aabddbf6d76bf5deda5ba3bd387fcfc371f62c47"
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
    assert_eq!(report.first_tick.player_position, Vec2::new(1.0, 0.0));
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
