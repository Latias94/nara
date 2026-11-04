use std::sync::Arc;

use nara::tooling::{EditorDocumentId, EditorFrameModel};

use crate::workspace::{EditorPanelInstance, EditorPanelInstanceId, EditorPanelTarget};

use super::submission::{PanelIncarnation, PanelOrigin};

#[derive(Default)]
pub(crate) struct PanelBinding {
    pub(crate) incarnation: Arc<PanelIncarnation>,
    kind: String,
    document: Option<EditorDocumentId>,
    target: EditorPanelTarget,
    instance_lifecycle: Arc<()>,
}

impl PanelBinding {
    pub(crate) fn for_instance(frame: &EditorFrameModel, instance: &EditorPanelInstance) -> Self {
        Self {
            incarnation: Arc::default(),
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
