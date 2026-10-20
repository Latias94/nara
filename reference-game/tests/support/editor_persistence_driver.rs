use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{project_host::EditorProjectSession, tooling::EditorFrameModel};

pub fn drive_until_result(editor: &mut EditorProjectSession) -> Arc<EditorFrameModel> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        if frame.persistence().result().is_some() {
            return frame;
        }
        assert!(Instant::now() < deadline, "Editor persistence did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}
