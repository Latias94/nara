#[path = "support/manual_raw_app_boot.rs"]
mod manual_raw_app_boot;
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use manual_raw_app_boot::{ManualRawAppFault, run_manual_raw_app_boot, run_manual_raw_app_fault};
use nara::prelude::Vec2;

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

    assert_eq!(report.plugin_plan_fingerprint.len(), 64);
    assert_eq!(report.schema_fingerprint.len(), 64);
    assert_eq!(report.content_revision.len(), 64);
    assert_eq!(report.content_digest.len(), 64);
    assert_eq!(report.command_digest.len(), 64);
    assert!(!report.task_shutdown.timed_out());
    assert!(report.joined_task_workers > 0);
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
