use nara_scene::{SceneAuthoringSession, SceneDocument};
use nara_tooling::{EditorDocumentId, EditorPersistenceCheckpoint, EditorWorkspace};

fn main() {
    let session = SceneAuthoringSession::new(SceneDocument::default());
    let mut workspace = EditorWorkspace::new();
    let checkpoint = EditorPersistenceCheckpoint {
        document: EditorDocumentId::from_raw(1),
        revision: session.revision(),
    };

    let _ = workspace.__apply_persistence_checkpoint(checkpoint);
}
