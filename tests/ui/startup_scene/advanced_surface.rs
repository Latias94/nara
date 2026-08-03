use nara::advanced_prelude::{
    STARTUP_SCENE_ACTIVATION_PLUGIN_ID, StartupSceneActivation, StartupSceneActivationPlugin,
    StartupSceneActivationSet, StartupSceneMaterializeError, StartupSceneSource,
    StartupSceneSourceError, StartupSceneSourceView, materialize_startup_scene,
};
use nara::ecs::World;
use nara::diagnostic::DiagnosticReport;

fn inspect(_activation: StartupSceneActivation<'_>) {}

fn main() {
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
