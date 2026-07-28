use nara_scene::{SceneAuthoringSession, SceneDocument};
use nara_tooling::{EditorDocumentDigest, EditorWorkspace};

fn main() {
    let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
    let document = workspace
        .open_scene_session("main", SceneAuthoringSession::new(SceneDocument::default()))
        .unwrap()
        .opened_document
        .unwrap();
    let checkpoint = authority.capture(&workspace, document).unwrap();
    let digest = EditorDocumentDigest::new(4, [0x44; 32]);
    let _ = authority.commit(&mut workspace, checkpoint, digest);
}
