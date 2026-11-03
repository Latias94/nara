use std::{cell::RefCell, rc::Rc};

use dear_imgui_rs::{ClipboardBackend, SystemClipboard};

#[derive(Clone)]
pub(super) struct EditorClipboard(Rc<RefCell<ClipboardAccess>>);

enum ClipboardAccess {
    System(Result<SystemClipboard, String>),
    #[cfg(test)]
    Memory {
        text: String,
        reject_write: bool,
    },
}

impl EditorClipboard {
    pub(super) fn new() -> Self {
        Self(Rc::new(RefCell::new(ClipboardAccess::System(
            SystemClipboard::new().map_err(|error| error.to_string()),
        ))))
    }

    pub(super) fn read(&self) -> Result<String, String> {
        match &mut *self.0.borrow_mut() {
            ClipboardAccess::System(Ok(clipboard)) => {
                clipboard.get_text().map_err(|error| error.to_string())
            }
            ClipboardAccess::System(Err(error)) => Err(error.clone()),
            #[cfg(test)]
            ClipboardAccess::Memory { text, .. } => Ok(text.clone()),
        }
    }

    pub(super) fn write(&self, text: &str) -> Result<(), String> {
        match &mut *self.0.borrow_mut() {
            ClipboardAccess::System(Ok(clipboard)) => {
                clipboard.set_text(text).map_err(|error| error.to_string())
            }
            ClipboardAccess::System(Err(error)) => Err(error.clone()),
            #[cfg(test)]
            ClipboardAccess::Memory {
                text: stored,
                reject_write,
            } => {
                if std::mem::take(reject_write) {
                    return Err("clipboard write refused".into());
                }
                *stored = text.to_owned();
                Ok(())
            }
        }
    }

    #[cfg(test)]
    pub(super) fn memory(text: &str) -> Self {
        Self(Rc::new(RefCell::new(ClipboardAccess::Memory {
            text: text.into(),
            reject_write: false,
        })))
    }

    #[cfg(test)]
    pub(super) fn reject_next_write(&self) {
        if let ClipboardAccess::Memory { reject_write, .. } = &mut *self.0.borrow_mut() {
            *reject_write = true;
        }
    }
}

impl ClipboardBackend for EditorClipboard {
    fn get(&mut self) -> Option<String> {
        self.read().ok()
    }

    fn set(&mut self, text: &str) {
        let _ = self.write(text);
    }
}
