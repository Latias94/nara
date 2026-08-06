use std::{collections::BTreeMap, sync::Arc};

use nara_ui_kernel::{UiFrame, UiKey};
use nara_ui_widgets::UiButton;

/// Immutable behavior authority compiled alongside one immutable frame candidate.
#[derive(Clone, Debug, Default)]
pub(crate) struct UiBehaviorSnapshot {
    buttons: BTreeMap<UiKey, UiButton>,
}

impl UiBehaviorSnapshot {
    pub(super) fn from_compiled_frame(
        frame: &UiFrame,
        mut buttons: BTreeMap<UiKey, UiButton>,
    ) -> Self {
        buttons.retain(|key, button| {
            frame.semantics().iter().any(|semantic| {
                semantic.key() == key
                    && semantic.actions().len() == 1
                    && &semantic.actions()[0] == button.action()
            })
        });
        Self { buttons }
    }

    pub(crate) fn button(&self, key: &UiKey) -> Option<&UiButton> {
        self.buttons.get(key)
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &UiKey> {
        self.buttons.keys()
    }
}

/// One atomically replaceable frame-and-behavior candidate for a surface.
#[derive(Clone, Debug)]
pub(super) struct UiCompiledCandidate {
    frame: Arc<UiFrame>,
    behavior: Arc<UiBehaviorSnapshot>,
}

impl UiCompiledCandidate {
    pub(super) fn new(frame: Arc<UiFrame>, buttons: BTreeMap<UiKey, UiButton>) -> Self {
        let behavior = Arc::new(UiBehaviorSnapshot::from_compiled_frame(&frame, buttons));
        Self { frame, behavior }
    }

    pub(super) fn frame(&self) -> &Arc<UiFrame> {
        &self.frame
    }

    pub(super) fn behavior(&self) -> &Arc<UiBehaviorSnapshot> {
        &self.behavior
    }
}
