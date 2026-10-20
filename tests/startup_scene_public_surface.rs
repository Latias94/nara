#![cfg(feature = "runtime-core")]

use nara::advanced_prelude::{
    STARTUP_SCENE_ACTIVATION_PLUGIN_ID, StartupSceneActivation, StartupSceneActivationPlugin,
    StartupSceneActivationSet, StartupSceneMaterializeError, StartupSceneSource,
    StartupSceneSourceError, StartupSceneSourceView, materialize_startup_scene,
};
use nara::diagnostic::DiagnosticReport;
use nara::ecs::World;

fn inspect(_activation: StartupSceneActivation<'_>) {}

#[test]
fn advanced_startup_scene_surface_is_available() {
    let _ = inspect;
    let _ = STARTUP_SCENE_ACTIVATION_PLUGIN_ID;
    let _ = StartupSceneActivationPlugin;
    let _ = StartupSceneActivationSet;
    let _: Option<StartupSceneSource> = None;
    let _: Option<StartupSceneSourceError> = None;
    let _: Option<StartupSceneSourceView> = None;
    let _: fn(
        &mut World,
        StartupSceneSource,
    ) -> Result<DiagnosticReport, StartupSceneMaterializeError> = materialize_startup_scene;
}

#[test]
fn startup_scene_activation_is_advanced_only() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/startup_scene/activation_pair_not_constructible.rs");
    tests.compile_fail("tests/ui/startup_scene/activation_view_not_resource.rs");
    tests.compile_fail("tests/ui/startup_scene/direct_module.rs");
    tests.compile_fail("tests/ui/startup_scene/ordinary_materialize.rs");
    tests.compile_fail("tests/ui/startup_scene/ordinary_prelude.rs");
    tests.compile_fail("tests/ui/startup_scene/ordinary_plugin.rs");
    tests.compile_fail("tests/ui/startup_scene/source_not_clone.rs");
}
