//! Trusted source-built document entrypoints keep filesystem authority in the session Host.

use nara::project_host::EditorExtensionDocumentHostError;
use nara::tooling::{EditorDocumentId, EditorExtensionDocument};

use super::EditorApplication;

impl EditorApplication {
    /// Registers one document owner for the currently published project session.
    pub fn register_extension_document<D: EditorExtensionDocument>(
        &mut self,
    ) -> Result<(), EditorExtensionDocumentHostError> {
        if self.session_retirement.is_some() {
            return Err(EditorExtensionDocumentHostError::Busy);
        }
        self.state
            .workspace_mut()
            .ok_or(EditorExtensionDocumentHostError::Busy)?
            .session
            .register_extension_document::<D>()
    }

    /// Opens a registered document using the current project's capability-bound Host.
    pub fn open_extension_document<D: EditorExtensionDocument>(
        &mut self,
        source: &str,
    ) -> Result<EditorDocumentId, EditorExtensionDocumentHostError> {
        if self.session_retirement.is_some() {
            return Err(EditorExtensionDocumentHostError::Busy);
        }
        self.state
            .workspace_mut()
            .ok_or(EditorExtensionDocumentHostError::Busy)?
            .session
            .open_extension_document::<D>(source)
    }
}
