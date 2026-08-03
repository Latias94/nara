use nara::advanced_prelude::{
    StartupSceneActivation, StartupSceneActivationSet, StartupSceneMaterializeError,
    StartupSceneSource, StartupSceneSourceView, materialize_startup_scene,
};
use nara::ecs::World;
use nara::diagnostic::DiagnosticReport;

fn inspect(_activation: StartupSceneActivation<'_>) {}

fn main() {
    let _ = inspect;
    let _: Option<StartupSceneActivationSet> = None;
    let _: Option<StartupSceneSource> = None;
    let _: Option<StartupSceneSourceView> = None;
    let _: fn(
        &mut World,
        StartupSceneSource,
    ) -> Result<DiagnosticReport, StartupSceneMaterializeError> = materialize_startup_scene;
}
