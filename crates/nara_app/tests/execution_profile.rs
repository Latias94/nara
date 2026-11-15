use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use nara_app::{
    App, AppConfigurationFingerprint, CoreStage, ExecutionProfile, ExecutionProfileError,
    FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginError, PluginId,
    PluginPlan, PluginPlanFingerprint, RealTime, RuntimeInstance, StartupStage,
};
use nara_ecs::{
    Commands, Res, Resource, bevy_ecs::schedule::ApplyDeferred, schedule::IntoScheduleConfigs,
};

#[path = "execution_profile/support.rs"]
mod support;

use support::*;

#[cfg(all(feature = "parallel-executor", not(target_arch = "wasm32")))]
#[path = "execution_profile/parallel.rs"]
mod parallel;

#[derive(Default)]
struct IdentityPlugin;

static IDENTITY_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("nara.test.execution-profile.identity"),
    PluginCategory::Runtime,
);

impl Plugin for IdentityPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &IDENTITY_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[test]
fn configuration_identity_includes_profile_and_preserves_the_original_plugin_plan() {
    let definition = PluginDefinition::for_default::<IdentityPlugin>();
    let plan = PluginPlan::resolve(definition.clone()).unwrap();
    let original_plan: PluginPlanFingerprint = plan.fingerprint();
    let empty_serial: AppConfigurationFingerprint = App::new().configuration_fingerprint();

    let mut serial = App::new();
    serial.add_plugins(definition.clone()).unwrap();
    let serial_identity: AppConfigurationFingerprint = serial.configuration_fingerprint();
    let serial_digest: [u8; 32] = serial_identity.as_bytes();
    assert_ne!(serial_digest, empty_serial.as_bytes());
    assert_eq!(serial.plugin_plan_fingerprint(), original_plan);

    let mut explicit_serial = App::new();
    explicit_serial
        .set_execution_profile(ExecutionProfile::Serial)
        .unwrap()
        .add_plugins(definition.clone())
        .unwrap();
    assert_eq!(explicit_serial.configuration_fingerprint(), serial_identity);

    let mut selected_first = App::new();
    selected_first
        .set_execution_profile(ExecutionProfile::Parallel)
        .unwrap()
        .add_plugins(definition.clone())
        .unwrap();
    let mut selected_last = App::new();
    selected_last
        .add_plugins(definition)
        .unwrap()
        .set_execution_profile(ExecutionProfile::Parallel)
        .unwrap();
    let parallel_identity: AppConfigurationFingerprint = selected_first.configuration_fingerprint();
    assert_eq!(
        selected_first.execution_profile(),
        ExecutionProfile::Parallel
    );
    assert_eq!(selected_first.plugin_plan_fingerprint(), original_plan);
    assert_eq!(selected_last.plugin_plan_fingerprint(), original_plan);
    assert_eq!(selected_last.configuration_fingerprint(), parallel_identity);
    assert_ne!(parallel_identity.as_bytes(), serial_digest);

    let mut empty_parallel = App::new();
    empty_parallel
        .set_execution_profile(ExecutionProfile::Parallel)
        .unwrap();
    assert_ne!(
        empty_parallel.configuration_fingerprint(),
        parallel_identity
    );
    assert_ne!(empty_parallel.configuration_fingerprint(), empty_serial);

    let sealed = serial.seal().unwrap();
    assert_eq!(sealed.execution_profile(), ExecutionProfile::Serial);
    assert_eq!(sealed.configuration_fingerprint(), serial_identity);
    assert_eq!(sealed.plugin_plan_fingerprint(), original_plan);
    let replayed = plan.instantiate().unwrap();
    assert_eq!(replayed.execution_profile(), ExecutionProfile::Serial);
    assert_eq!(replayed.configuration_fingerprint(), serial_identity);
    assert_eq!(replayed.plugin_plan_fingerprint(), original_plan);
}

struct FinishObservation {
    rejection: Result<(), PluginError>,
    profile: ExecutionProfile,
    identity: AppConfigurationFingerprint,
}

struct FinishMutationProbe(Arc<Mutex<Option<FinishObservation>>>);

static FINISH_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("nara.test.execution-profile.finish-mutation"),
    PluginCategory::Runtime,
);

impl Plugin for FinishMutationProbe {
    fn declaration() -> &'static PluginDeclaration {
        &FINISH_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        let rejection = app
            .set_execution_profile(ExecutionProfile::Parallel)
            .map(|_| ());
        *self.0.lock().unwrap() = Some(FinishObservation {
            rejection,
            profile: app.execution_profile(),
            identity: app.configuration_fingerprint(),
        });
        Ok(())
    }
}

fn assert_serial_default() {
    let driver = thread::current().id();
    assert_eq!(ExecutionProfile::default(), ExecutionProfile::Serial);
    assert_eq!(App::default().execution_profile(), ExecutionProfile::Serial);
    let (mut app, owner_trace) = tracked_app();
    assert_eq!(app.execution_profile(), ExecutionProfile::Serial);
    let finish_observation = Arc::new(Mutex::new(None));
    app.add_plugins(FinishMutationProbe(Arc::clone(&finish_observation)))
        .unwrap();
    let original_identity = app.configuration_fingerprint();
    let startup = Arc::new(Mutex::new(Vec::new()));
    let frame = Arc::new(Mutex::new(Vec::new()));
    for stage in StartupStage::ALL {
        let startup = Arc::clone(&startup);
        app.add_systems(stage, move || {
            startup
                .lock()
                .unwrap()
                .push((stage, thread::current().id()));
        })
        .unwrap();
    }
    for stage in CoreStage::ALL {
        let frame = Arc::clone(&frame);
        app.add_systems(stage, move || {
            frame.lock().unwrap().push((stage, thread::current().id()));
        })
        .unwrap();
    }

    let mut runtime = RuntimeInstance::start(app).unwrap();
    assert_eq!(runtime.execution_profile(), ExecutionProfile::Serial);
    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 1);
    assert_eq!(runtime.execution_profile(), ExecutionProfile::Serial);
    let evidence = runtime.finish().unwrap();

    assert!(evidence.is_empty());
    let finish = finish_observation.lock().unwrap();
    let observation = finish.as_ref().expect("the Finish hook must have executed");
    assert_eq!(observation.rejection, Err(PluginError::AppSealed));
    assert_eq!(observation.profile, ExecutionProfile::Serial);
    assert_eq!(observation.identity, original_identity);
    assert_eq!(
        *startup.lock().unwrap(),
        StartupStage::ALL.map(|stage| (stage, driver))
    );
    assert_eq!(
        *frame.lock().unwrap(),
        CoreStage::ALL.map(|stage| (stage, driver))
    );
    let trace = owner_trace.lock().unwrap();
    assert_eq!(trace.startup, [driver]);
    assert_eq!(trace.update, [driver]);
    assert_eq!(trace.drops, [driver]);
}

#[derive(Resource)]
struct DeferredPublication(u64);

fn queue_first_update(mut commands: Commands, time: Res<RealTime>) {
    if time.frame == 1 {
        commands.insert_resource(DeferredPublication(time.frame));
    }
}

fn assert_update_final_deferred(profile: ExecutionProfile) {
    for apply_final in [true, false] {
        let mut app = App::new();
        app.set_execution_profile(profile).unwrap();
        if !apply_final {
            app.set_schedule_apply_final_deferred(CoreStage::Update, false)
                .unwrap();
        }
        app.add_systems(
            CoreStage::Update,
            (ApplyDeferred, queue_first_update).chain_ignore_deferred(),
        )
        .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let first = runtime
            .world()
            .get_resource::<DeferredPublication>()
            .map(|value| value.0);
        runtime.drive(Duration::ZERO).unwrap();
        let second = runtime
            .world()
            .get_resource::<DeferredPublication>()
            .map(|value| value.0);
        assert!(runtime.finish().unwrap().is_empty());

        let expected_first = if apply_final { Some(1) } else { None };
        assert_eq!(
            first, expected_first,
            "{profile:?}, apply_final={apply_final}"
        );
        assert_eq!(
            second,
            Some(1),
            "the original first-frame command must apply"
        );
    }
}

#[cfg(not(feature = "parallel-executor"))]
#[test]
fn serial_is_default_and_unbuilt_parallel_refuses_before_startup_with_retryable_custody() {
    assert_serial_default();
    assert_update_final_deferred(ExecutionProfile::Serial);
    assert_parallel_refused_before_startup(ExecutionProfileError::ParallelNotBuilt);
}

#[cfg(all(feature = "parallel-executor", target_arch = "wasm32"))]
#[test]
fn unsupported_parallel_target_refuses_before_startup_with_retryable_custody() {
    assert_serial_default();
    assert_update_final_deferred(ExecutionProfile::Serial);
    assert_parallel_refused_before_startup(ExecutionProfileError::UnsupportedTarget);
}
