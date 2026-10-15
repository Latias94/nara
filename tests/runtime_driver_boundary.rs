use std::{collections::BTreeSet, path::PathBuf, process::Command};

use nara::app::{App, RuntimeInstance};
use serde_json::Value;

#[test]
fn runtime_core_keeps_its_dependency_direction() {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
            "--offline",
        ])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("Cargo metadata must be executable");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let app = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "nara_app")
        .unwrap();
    let actual = app["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|dependency| {
            dependency["kind"].is_null()
                || matches!(dependency["kind"].as_str(), Some("normal" | "build"))
        })
        .map(|dependency| dependency["name"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let allowed = BTreeSet::from(["bevy_ecs", "blake3", "nara_ecs", "thiserror"]);
    assert!(
        actual.is_subset(&allowed),
        "unexpected runtime dependencies: {actual:?}"
    );
}

#[test]
fn managed_world_access_is_read_only_and_scopes_do_not_escape() {
    let mut runtime = RuntimeInstance::start(App::new()).unwrap();
    let marker = runtime.with_driver_scope(|_| 17_u32).unwrap();
    assert_eq!(marker, 17);
    runtime.finish().unwrap();

    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/runtime_driver/raw_app_execution.rs");
    cases.compile_fail("tests/ui/runtime_driver/runtime_world_mut.rs");
    cases.compile_fail("tests/ui/runtime_driver/scope_escape.rs");
}

#[cfg(feature = "desktop-winit")]
#[test]
fn platform_runner_consumes_the_managed_runtime_contract() {
    let _: fn(
        nara::winit::WinitRunner,
        &mut RuntimeInstance,
    ) -> Result<nara::app::AppExit, nara::app::AppRunError> = nara::winit::WinitRunner::run;
}
