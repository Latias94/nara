//! egui Adapters for Nara's UI-neutral Editor workspace and Project Shell seams.
//!
//! The workspace Adapter renders one immutable [`nara_tooling::EditorFrameModel`] and returns an
//! [`EguiEditorOutput`] containing one [`nara_tooling::EditorActionBatch`] plus an optional typed
//! product action. The Project Shell Adapter renders borrowed product state and returns an updated
//! draft plus one typed action. Both own only transient toolkit state; document, project lifecycle,
//! filesystem capability, window, and GPU authority remain elsewhere.

mod editor;
mod fields;
mod game_view;
mod image_import;
mod inspector;
mod project_browser;
mod project_shell;
mod scene_view;
mod ui_authoring;

#[cfg(test)]
mod image_import_tests;
#[cfg(test)]
mod project_browser_tests;
#[cfg(test)]
mod project_shell_tests;
#[cfg(test)]
mod tests;

pub use editor::{
    EguiAuthoringViewportOutput, EguiCentralViewportInput, EguiCentralViewportOutput,
    EguiEditorLayout, EguiEditorLayoutInput, EguiEditorOutput, EguiEditorPanel,
    EguiEditorProjectAction,
};
pub use game_view::{EguiGameViewInput, EguiGameViewOutput};
pub use project_shell::{
    EguiProjectManagerInput, EguiProjectOpenDraft, EguiProjectOpeningInput,
    EguiProjectRecoveryDiagnostic, EguiProjectRecoveryInput, EguiProjectShellAction,
    EguiProjectShellInput, EguiProjectShellOutput, EguiProjectShellPanel,
};
pub use scene_view::{EguiSceneViewInput, EguiSceneViewOutput};
pub use ui_authoring::{
    EguiUiAuthoringImages, EguiUiAuthoringInput, EguiUiAuthoringMutations, EguiUiCanvasInput,
    EguiUiCanvasOutput,
};
