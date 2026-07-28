use nara_scene::{SceneAuthoringSession, SceneDocument};
use nara_tooling::EditorWorkspace;

fn main() {
    let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
    let document = workspace
        .open_scene_session("main", SceneAuthoringSession::new(SceneDocument::default()))
        .unwrap()
        .opened_document
        .unwrap();
    let checkpoint = authority.capture(&workspace, document).unwrap();
    let _duplicate = checkpoint.clone();
}
