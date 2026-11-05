use std::sync::Arc;

use nara::tooling::{EditorDocumentId, EditorFrameModel};

use crate::workspace::{EditorPanelInstance, EditorPanelInstanceId, EditorPanelTarget};

use super::submission::{PanelIncarnation, PanelOrigin};

#[derive(Clone, Default)]
pub(crate) struct PanelBinding {
    pub(crate) incarnation: Arc<PanelIncarnation>,
    kind: String,
    document: Option<EditorDocumentId>,
    target: EditorPanelTarget,
    instance_lifecycle: Arc<()>,
}

impl PanelBinding {
    pub(crate) fn for_instance(frame: &EditorFrameModel, instance: &EditorPanelInstance) -> Self {
        Self::with_incarnation(frame, instance, Arc::default())
    }

    pub(crate) fn with_incarnation(
        frame: &EditorFrameModel,
        instance: &EditorPanelInstance,
        incarnation: Arc<PanelIncarnation>,
    ) -> Self {
        Self {
            incarnation,
            kind: instance.kind().to_owned(),
            document: instance.target().document(frame),
            target: instance.target().clone(),
            instance_lifecycle: Arc::clone(instance.lifecycle()),
        }
    }

    pub(crate) fn matches(&self, frame: &EditorFrameModel, instance: &EditorPanelInstance) -> bool {
        Arc::ptr_eq(&self.instance_lifecycle, instance.lifecycle())
            && matches_instance(frame, instance, &self.kind, &self.target, self.document)
    }

    pub(crate) fn origin(&self, instance: EditorPanelInstanceId) -> PanelOrigin {
        PanelOrigin {
            instance,
            incarnation: Arc::clone(&self.incarnation),
        }
    }
}

/// An observation of one exact live semantic panel instance.
///
/// Native attachments retain this token rather than reinterpret a saved instance ID. It grants
/// no source mutation, input, rendering, or native-window authority by itself.
#[derive(Clone)]
pub struct EditorPanelInstanceToken {
    pub(crate) instance: EditorPanelInstanceId,
    pub(crate) binding: PanelBinding,
    pub(crate) project: Option<u64>,
    pub(crate) layout: Arc<()>,
    pub(crate) state_lifetime: std::sync::Weak<()>,
    pub(crate) host: Arc<()>,
}

impl EditorPanelInstanceToken {
    #[must_use]
    pub const fn instance(&self) -> EditorPanelInstanceId {
        self.instance
    }

    /// Returns the exact document resolved when this token was captured.
    ///
    /// This does not follow later active-document changes. Use [`Self::is_current`] to check
    /// whether the captured binding is still live.
    #[must_use]
    pub const fn document(&self) -> Option<EditorDocumentId> {
        self.binding.document
    }

    /// Returns the registered panel kind captured by this token.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.binding.kind
    }

    /// Checks the original project, layout and complete instance binding without changing it.
    #[must_use]
    pub fn is_current(&self, application: &crate::EditorApplication) -> bool {
        self.state_lifetime.upgrade().is_some()
            && self.project == application.workspace_generation()
            && Arc::ptr_eq(
                &self.layout,
                application.panel_workspace().layout().lifecycle(),
            )
            && application.current_frame().is_some_and(|frame| {
                application
                    .panel_workspace()
                    .layout()
                    .instance(self.instance)
                    .is_some_and(|instance| self.binding.matches(&frame, instance))
            })
    }

    pub(crate) fn matches_origin(&self, origin: &PanelOrigin) -> bool {
        self.instance == origin.instance
            && Arc::ptr_eq(&self.binding.incarnation, &origin.incarnation)
    }
}

impl PartialEq for EditorPanelInstanceToken {
    fn eq(&self, other: &Self) -> bool {
        self.instance == other.instance
            && Arc::ptr_eq(&self.binding.incarnation, &other.binding.incarnation)
            && self.project == other.project
            && Arc::ptr_eq(&self.layout, &other.layout)
            && std::sync::Weak::ptr_eq(&self.state_lifetime, &other.state_lifetime)
            && Arc::ptr_eq(&self.host, &other.host)
    }
}

impl Eq for EditorPanelInstanceToken {}

impl std::fmt::Debug for EditorPanelInstanceToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorPanelInstanceToken")
            .field("instance", &self.instance)
            .finish_non_exhaustive()
    }
}

pub(crate) fn matches_instance(
    frame: &EditorFrameModel,
    instance: &EditorPanelInstance,
    kind: &str,
    target: &EditorPanelTarget,
    document: Option<EditorDocumentId>,
) -> bool {
    instance.is_visible()
        && instance.kind() == kind
        && instance.target() == target
        && instance.target().document(frame) == document
}
