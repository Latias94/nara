//! Project integrations for primary Scene activation and retained content.

mod replacement;

use nara_app::{App, Plugin, PluginDeclaration, PluginError};
#[cfg(test)]
use nara_scene::{CompiledSceneCandidate, SceneComponentBindings};

#[cfg(feature = "project-source")]
pub(crate) use nara_scene::current_scene::materialized_startup_scene_receipt;
pub use nara_scene::current_scene::{
    CurrentScene, STARTUP_SCENE_ACTIVATION_PLUGIN_ID, SceneSource, SceneSourceView,
    StartupSceneActivationSet, StartupSceneMaterializeError, add_startup_scene_activation_systems,
    materialize_startup_scene,
};
#[cfg(feature = "serde")]
pub(crate) use nara_scene::current_scene::{
    current_scene_bindings, current_scene_inspection_bindings,
};

pub use replacement::{replace_current_scene_with_product, restart_current_scene_with_product};

const STARTUP_SCENE_ACTIVATION_PLUGIN_DECLARATION: PluginDeclaration =
    nara_scene::current_scene::STARTUP_SCENE_ACTIVATION_PLUGIN_DECLARATION.shutdown_obligations(&[
        #[cfg(feature = "serde")]
        crate::project_host::PROJECT_SCENE_REQUEST_CLOSE_OBLIGATION,
    ]);

/// Installs Scene activation and the project host's request polling and shutdown.
#[derive(Debug, Default, Clone, Copy)]
pub struct StartupSceneActivationPlugin;

impl Plugin for StartupSceneActivationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &STARTUP_SCENE_ACTIVATION_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        #[cfg(feature = "serde")]
        {
            crate::project_host::register_project_scene_close_participant(app)?;
            app.add_systems(
                nara_app::CoreStage::TaskUpdate,
                crate::project_host::poll_project_scene_requests,
            )?;
        }
        nara_scene::current_scene::install_startup_scene_activation(app)
    }
}

#[cfg(test)]
mod replacement_tests;

#[cfg(test)]
pub(crate) fn compile_test_scene(
    document: &nara_scene::SceneDocument,
    bindings: &SceneComponentBindings,
    source: nara_scene::SourceDocumentKey,
) -> CompiledSceneCandidate {
    let report = bindings.compile_scene(document, &nara_scene::InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report
        .candidate
        .expect("the fixture compiles")
        .bind_source(source)
}
