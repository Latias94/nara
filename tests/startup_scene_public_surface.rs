#![cfg(feature = "runtime-core")]

use nara::advanced_prelude::{
    CurrentScene, STARTUP_SCENE_ACTIVATION_PLUGIN_ID, SceneSource, SceneSourceError,
    SceneSourceView, StartupSceneActivationPlugin, StartupSceneActivationSet,
    StartupSceneMaterializeError, materialize_startup_scene,
};
use nara::diagnostic::DiagnosticReport;
use nara::ecs::World;

fn inspect(_activation: CurrentScene<'_>) {}

#[test]
fn advanced_startup_scene_surface_is_available() {
    let _ = inspect;
    let _ = STARTUP_SCENE_ACTIVATION_PLUGIN_ID;
    let _ = StartupSceneActivationPlugin;
    let _ = StartupSceneActivationSet;
    let _: Option<SceneSource> = None;
    let _: Option<SceneSourceError> = None;
    let _: Option<SceneSourceView> = None;
    let _: fn(&mut World, SceneSource) -> Result<DiagnosticReport, StartupSceneMaterializeError> =
        materialize_startup_scene;
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
