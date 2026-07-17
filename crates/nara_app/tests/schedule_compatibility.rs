use std::panic::{AssertUnwindSafe, catch_unwind};

use nara_app::{App, CoreStage, FixedTime, PluginError, ScheduleCompatibilityError, StartupStage};
use nara_ecs::{Resource, schedule::IntoScheduleConfigs, system::Commands};

#[derive(Resource)]
struct FinalDeferredApplied;

fn insert_final_deferred_marker(mut commands: Commands) {
    commands.insert_resource(FinalDeferredApplied);
}

#[test]
fn seal_rejects_disabled_automatic_deferred_insertion() {
    let mut app = App::new();
    let mut settings = app
        .get_schedule(CoreStage::FixedUpdate)
        .expect("FixedUpdate is an engine-owned schedule")
        .get_build_settings();
    settings.auto_insert_apply_deferred = false;
    app.set_schedule_build_settings(CoreStage::FixedUpdate, settings)
        .unwrap();

    assert_eq!(
        app.seal().unwrap_err(),
        PluginError::ScheduleCompatibility(
            ScheduleCompatibilityError::AutomaticDeferredInsertionDisabled {
                schedule: CoreStage::FixedUpdate,
            }
        )
    );
}

#[test]
fn seal_rejects_invalid_public_anchor_graph_without_panicking() {
    let mut app = App::new();
    app.configure_sets(
        CoreStage::FixedUpdate,
        nara_app::FixedUpdateSet::Prepare.after(nara_app::FixedUpdateSet::Finalize),
    )
    .unwrap();

    let sealed = catch_unwind(AssertUnwindSafe(|| app.seal()));

    assert!(sealed.is_ok(), "schedule validation panicked");
    let error = sealed.unwrap().unwrap_err();
    assert!(matches!(
        error,
        PluginError::ScheduleCompatibility(ScheduleCompatibilityError::BuildFailed {
            schedule: CoreStage::FixedUpdate,
            ..
        })
    ));
}

#[test]
fn seal_reasserts_final_deferred_application() {
    let mut app = App::new();
    app.add_systems(CoreStage::FixedUpdate, insert_final_deferred_marker)
        .unwrap();
    app.set_schedule_apply_final_deferred(CoreStage::FixedUpdate, false)
        .unwrap();

    app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let final_deferred_applied = app.world().contains_resource::<FinalDeferredApplied>();

    assert!(final_deferred_applied);
}

#[test]
fn raw_built_in_schedule_mutation_is_rejected() {
    let mut app = App::new();

    for stage in StartupStage::ALL {
        assert!(matches!(
            app.get_schedule_mut(stage),
            Err(PluginError::RawBuiltInScheduleMutationForbidden)
        ));
    }
    for stage in CoreStage::ALL {
        assert!(matches!(
            app.get_schedule_mut(stage),
            Err(PluginError::RawBuiltInScheduleMutationForbidden)
        ));
    }
}
