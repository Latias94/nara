use nara_scene::{SceneAuthoringSession, SceneDocument};
use nara_tooling::{EditorDocumentDigest, EditorWorkspace};

fn main() {
    let mut workspace = EditorWorkspace::new();
    let document = workspace
        .open_scene_session("main", SceneAuthoringSession::new(SceneDocument::default()))
        .unwrap()
        .opened_document
        .unwrap();
    let digest = EditorDocumentDigest::new(4, [0x44; 32]);
    let _ = workspace.bind_opened_source_digest(document, digest);
    let reopened = SceneAuthoringSession::new(SceneDocument::default());
    let _ = workspace.publish_reopened_session(Some(document), reopened, digest);
}
