//! Shared native text clipboard ownership, independent of the Editor frontend toolkit.

use std::{cell::RefCell, rc::Rc};

#[cfg(test)]
use std::cell::Cell;

/// Synchronous text access supplied by the native Host or an injected provider.
///
/// Calls receive the caller's text unchanged, and errors are returned unchanged. Providers are
/// cooperative native code; recursively accessing the same owner returns an error.
pub trait EditorClipboardAccess {
    /// Reads the current text, or reports why it is unavailable.
    fn read(&mut self) -> Result<String, String>;

    /// Writes the supplied text, or reports why the write failed.
    fn write(&mut self, text: &str) -> Result<(), String>;
}

/// A cloneable, single-threaded owner of one text clipboard provider.
///
/// Clones share the provider and its lifetime. Access is serialized for each synchronous call;
/// reentry through any clone returns an error instead of panicking on a conflicting borrow.
#[derive(Clone)]
pub struct EditorClipboard {
    access: Rc<RefCell<Box<dyn EditorClipboardAccess>>>,
    #[cfg(test)]
    reject_write: Option<Rc<Cell<bool>>>,
}

impl EditorClipboard {
    /// Opens the system clipboard, retaining any initialization error for subsequent reads/writes.
    ///
    /// Use [`Self::system`] when initialization failure must be handled immediately. Neither
    /// constructor substitutes an in-memory clipboard when system access is unavailable.
    pub fn new() -> Self {
        Self::system().unwrap_or_else(|error| Self::with_access(UnavailableClipboardAccess(error)))
    }

    /// Opens the desktop text clipboard, reporting unsupported targets or initialization failure.
    ///
    /// Windows, macOS and Linux are supported. Linux uses arboard's default X11 backend unless
    /// its optional Wayland data-control feature is enabled by the Host's dependency configuration.
    pub fn system() -> Result<Self, String> {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            let clipboard = arboard::Clipboard::new()
                .map_err(|error| format!("failed to initialize the system clipboard: {error}"))?;
            Ok(Self::with_access(SystemClipboardAccess(clipboard)))
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        Err("system clipboard integration is unavailable on this target".into())
    }

    /// Owns an injected provider without opening a system clipboard.
    pub fn with_access(access: impl EditorClipboardAccess + 'static) -> Self {
        Self {
            access: Rc::new(RefCell::new(Box::new(access))),
            #[cfg(test)]
            reject_write: None,
        }
    }

    /// Reads text, returning provider errors or an error if another call is in progress.
    pub fn read(&self) -> Result<String, String> {
        self.access
            .try_borrow_mut()
            .map_err(|_| "clipboard access is already in progress".to_owned())?
            .read()
    }

    /// Writes the caller's exact text, returning provider errors or an access-in-progress error.
    pub fn write(&self, text: &str) -> Result<(), String> {
        self.access
            .try_borrow_mut()
            .map_err(|_| "clipboard access is already in progress".to_owned())?
            .write(text)
    }

    #[cfg(test)]
    pub(crate) fn memory(text: &str) -> Self {
        let reject_write = Rc::new(Cell::new(false));
        let mut clipboard = Self::with_access(MemoryClipboardAccess {
            text: text.to_owned(),
            reject_write: reject_write.clone(),
        });
        clipboard.reject_write = Some(reject_write);
        clipboard
    }

    #[cfg(test)]
    pub(crate) fn reject_next_write(&self) {
        if let Some(reject_write) = &self.reject_write {
            reject_write.set(true);
        }
    }
}

impl Default for EditorClipboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
struct SystemClipboardAccess(arboard::Clipboard);

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl EditorClipboardAccess for SystemClipboardAccess {
    fn read(&mut self) -> Result<String, String> {
        self.0
            .get_text()
            .map_err(|error| format!("failed to read the system clipboard: {error}"))
    }

    fn write(&mut self, text: &str) -> Result<(), String> {
        self.0
            .set_text(text)
            .map_err(|error| format!("failed to write the system clipboard: {error}"))
    }
}

struct UnavailableClipboardAccess(String);

impl EditorClipboardAccess for UnavailableClipboardAccess {
    fn read(&mut self) -> Result<String, String> {
        Err(self.0.clone())
    }

    fn write(&mut self, _text: &str) -> Result<(), String> {
        Err(self.0.clone())
    }
}

#[cfg(test)]
struct MemoryClipboardAccess {
    text: String,
    reject_write: Rc<Cell<bool>>,
}

#[cfg(test)]
impl EditorClipboardAccess for MemoryClipboardAccess {
    fn read(&mut self) -> Result<String, String> {
        Ok(self.text.clone())
    }

    fn write(&mut self, text: &str) -> Result<(), String> {
        if self.reject_write.replace(false) {
            return Err("clipboard write refused".into());
        }
        self.text = text.to_owned();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Weak;

    use super::*;

    #[test]
    fn clones_share_one_provider_and_preserve_exact_text() {
        let clipboard = EditorClipboard::memory("original text");
        let other = clipboard.clone();
        assert_eq!(other.read().unwrap(), "original text");

        for text in ["", " \tNara\r\n剪贴板 e\u{301} 👩‍💻\0\n "] {
            other.write(text).unwrap();
            assert_eq!(clipboard.read().unwrap(), text);
        }

        drop(clipboard);
        other.write("remaining owner").unwrap();
        assert_eq!(other.read().unwrap(), "remaining owner");
    }

    struct RefusingAccess;

    impl EditorClipboardAccess for RefusingAccess {
        fn read(&mut self) -> Result<String, String> {
            Err("provider read refused".into())
        }

        fn write(&mut self, _text: &str) -> Result<(), String> {
            Err("provider write refused".into())
        }
    }

    #[test]
    fn injected_provider_failures_reach_every_caller_unchanged() {
        let clipboard = EditorClipboard::with_access(RefusingAccess);
        let other = clipboard.clone();

        for owner in [&clipboard, &other] {
            assert_eq!(owner.read().unwrap_err(), "provider read refused");
            assert_eq!(owner.write("text").unwrap_err(), "provider write refused");
        }
    }

    #[test]
    fn retained_initialization_error_is_reported_on_every_operation() {
        let error = "failed to initialize the system clipboard: desktop unavailable";
        let clipboard = EditorClipboard::with_access(UnavailableClipboardAccess(error.into()));
        let other = clipboard.clone();

        assert_eq!(clipboard.read().unwrap_err(), error);
        assert_eq!(other.write("text").unwrap_err(), error);
        assert_eq!(other.read().unwrap_err(), error);
        assert_eq!(clipboard.write("").unwrap_err(), error);
    }

    #[test]
    fn refused_memory_write_preserves_contents_and_only_rejects_once() {
        let clipboard = EditorClipboard::memory("retained text");
        let other = clipboard.clone();
        clipboard.reject_next_write();

        assert_eq!(other.read().unwrap(), "retained text");
        assert_eq!(
            other.write("refused").unwrap_err(),
            "clipboard write refused"
        );
        assert_eq!(clipboard.read().unwrap(), "retained text");
        clipboard.write("accepted").unwrap();
        assert_eq!(other.read().unwrap(), "accepted");
    }

    struct ReentrantAccess(Rc<RefCell<Weak<EditorClipboard>>>);

    impl EditorClipboardAccess for ReentrantAccess {
        fn read(&mut self) -> Result<String, String> {
            self.0.borrow().upgrade().expect("owner is alive").read()
        }

        fn write(&mut self, text: &str) -> Result<(), String> {
            self.0
                .borrow()
                .upgrade()
                .expect("owner is alive")
                .write(text)
        }
    }

    #[test]
    fn reentry_returns_an_error_instead_of_panicking() {
        let callback = Rc::new(RefCell::new(Weak::new()));
        let clipboard = Rc::new(EditorClipboard::with_access(ReentrantAccess(
            callback.clone(),
        )));
        *callback.borrow_mut() = Rc::downgrade(&clipboard);
        let other = clipboard.as_ref().clone();

        assert_eq!(
            clipboard.read().unwrap_err(),
            "clipboard access is already in progress"
        );
        assert_eq!(
            other.write("reentrant text").unwrap_err(),
            "clipboard access is already in progress"
        );
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    #[test]
    fn unsupported_system_access_never_substitutes_a_memory_clipboard() {
        let error = "system clipboard integration is unavailable on this target";
        assert_eq!(EditorClipboard::system().err().as_deref(), Some(error));

        let clipboard = EditorClipboard::new();
        assert_eq!(clipboard.read().unwrap_err(), error);
        assert_eq!(clipboard.write("text").unwrap_err(), error);
    }
}
