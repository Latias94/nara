use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_manifest(repository: &Path) -> PathBuf {
    repository.join("tests/fixtures/schedule-extension/renamed-root/Cargo.toml")
}

#[test]
fn fixture_depends_only_on_the_renamed_public_root_package() {
    let repository = repository();
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
            "--offline",
            "--manifest-path",
        ])
        .arg(fixture_manifest(&repository))
        .output()
        .expect("Cargo metadata must be executable");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    assert_eq!(packages.len(), 1);
    let dependencies = packages[0]["dependencies"].as_array().unwrap();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0]["name"], "nara");
    assert_eq!(dependencies[0]["rename"], "engine");
    assert_eq!(dependencies[0]["uses_default_features"], false);
    assert_eq!(
        dependencies[0]["features"],
        serde_json::json!(["runtime-core"])
    );
    assert_eq!(
        Path::new(dependencies[0]["path"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        repository.canonicalize().unwrap()
    );
}

#[test]
fn renamed_root_fixture_observes_the_public_anchor_contract() {
    let repository = repository();
    let output = Command::new(env!("CARGO"))
        .args(["test", "--manifest-path"])
        .arg(fixture_manifest(&repository))
        .args([
            "--locked",
            "--offline",
            "--jobs",
            "1",
            "--",
            "--test-threads=1",
        ])
        .env("CARGO_TARGET_DIR", repository.join("target"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("the independent schedule fixture must run");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(feature = "runtime-core")]
#[test]
fn public_root_consumer_can_execute_an_installed_custom_schedule() {
    use nara::{
        app::{App, CoreStage, FixedTime, RealTime, RuntimeInstance},
        ecs::{ResMut, Resource, ScheduleLabel},
    };
    use std::time::Duration;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
    struct DomainSchedule;
    #[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
    struct OtherSchedule;
    #[derive(Default, Resource)]
    struct Runs(Vec<&'static str>);

    let mut app = App::new();
    app.init_resource::<Runs>().unwrap();
    app.add_systems(CoreStage::Update, |mut runs: ResMut<Runs>| {
        runs.0.push("update")
    })
    .unwrap();
    app.add_systems(DomainSchedule, |mut runs: ResMut<Runs>| {
        runs.0.push("custom")
    })
    .unwrap();
    app.add_systems(OtherSchedule, |mut runs: ResMut<Runs>| runs.0.push("other"))
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let frame = runtime.world().resource::<RealTime>().frame;
    let tick = runtime.world().resource::<FixedTime>().tick();

    runtime.run_schedule(DomainSchedule).unwrap();

    assert_eq!(runtime.world().resource::<Runs>().0, ["update", "custom"]);
    assert_eq!(runtime.world().resource::<RealTime>().frame, frame);
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), tick);
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<Runs>().0,
        ["update", "custom", "update"]
    );
    runtime.finish().unwrap();
}
