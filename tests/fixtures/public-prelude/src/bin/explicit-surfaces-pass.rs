use nara::{
    advanced_prelude::{GameplayCommandQueue, RuntimeDiagnostics},
    backend_prelude::WindowEvents,
    hierarchy,
    project_host::ProjectSettingsCandidate,
    scene,
    tooling_prelude::SceneInspectorState,
};

fn main() {
    let _: Option<WindowEvents> = None;
    let _: Option<SceneInspectorState> = None;
    let _: Option<RuntimeDiagnostics> = None;
    let _: Option<GameplayCommandQueue> = None;
    let _: Option<ProjectSettingsCandidate> = None;
    let _: Option<hierarchy::Parent> = None;
    let _: Option<hierarchy::HierarchyPlugin> = None;
    let _: Option<scene::SceneComponentsPlugin> = None;
    let _ = hierarchy::HIERARCHY_PLUGIN_ID;
    let _ = scene::SCENE_COMPONENTS_PLUGIN_ID;
}
