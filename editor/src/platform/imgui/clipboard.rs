use dear_imgui_rs::ClipboardBackend;

pub(super) use crate::native::EditorClipboard;

impl ClipboardBackend for EditorClipboard {
    fn get(&mut self) -> Option<String> {
        self.read().ok()
    }

    fn set(&mut self, text: &str) {
        let _ = self.write(text);
    }
}
