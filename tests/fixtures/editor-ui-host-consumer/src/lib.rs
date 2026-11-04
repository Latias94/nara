//! Independently authored controlled UI and semantic Editor panel state.
#![forbid(unsafe_code)]

pub mod project;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    project_host::EditorEditPreparationError,
    reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue,
    },
    scene::{AuthoringPropertyEdit, SceneEntityId},
    tooling::{EditorDocumentQueryBasis, PreparedEditorDocumentEdit},
};
use nara_editor::{
    PanelActionDispatch, PanelSubmissionId,
    panels::{EditorPanel, EditorPanelContext},
};
use nara_text::{TextEdit, TextEngine, bundled_font_face};
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiBrush, UiColorRole,
    UiDispatchBatch, UiKey, UiLayer, UiLocalKey, UiNode, UiRoot, UiRootKey, UiStyle, UiVisual,
};
use nara_ui_widgets::{
    UiButton, UiButtonState, UiIntent, UiIntentBatch, UiTextField, UiTextInputToken,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub const COMPONENT: &str = "consumer.Caption";
pub const FIELD: &str = "text";
pub const INITIAL: &str = "original";
static NEXT_PANEL: AtomicU64 = AtomicU64::new(1);

/// Observations belong to the consumer, including results from retired panel generations.
#[derive(Default)]
pub struct Evidence {
    pub created: Cell<usize>,
    pub dropped: RefCell<Vec<usize>>,
    pub finished: RefCell<Vec<(usize, PanelSubmissionId, PanelActionDispatch)>>,
}

/// Controlled widgets retain editing and focus lifetimes independently of the document owner.
pub struct DraftPanel {
    serial: usize,
    incarnation: u64,
    evidence: Rc<Evidence>,
    text: TextEngine,
    field: UiTextField,
    root: UiRootKey,
    popup_root: UiRootKey,
    buttons: [(UiButton, UiButtonState); 2],
    popup: bool,
    reviews: usize,
}

impl DraftPanel {
    pub fn new(evidence: Rc<Evidence>) -> Result<Self> {
        let serial = evidence.created.get() + 1;
        let incarnation = NEXT_PANEL
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| std::io::Error::other("panel incarnation capacity exhausted"))?;
        let root = UiRootKey::runtime("external-editor-panel", incarnation.to_string())?;
        let popup_root =
            UiRootKey::runtime("external-editor-panel-popup", incarnation.to_string())?;
        let mut text = TextEngine::new(bundled_font_face()?, Default::default())?;
        let button = |root: &UiRootKey, action, label| -> Result<_> {
            Ok((
                UiButton::new(
                    UiKey::scoped(root.clone(), UiLocalKey::runtime("command", action)?)?,
                    label,
                    UiActionId::new(action)?,
                )?,
                UiButtonState::default(),
            ))
        };
        let panel = Self {
            serial,
            incarnation,
            field: UiTextField::new(root.clone(), "Caption", INITIAL, 18.0, &mut text)?,
            text,
            buttons: [
                button(&root, "review", "Review change")?,
                button(&popup_root, "apply", "Apply caption")?,
            ],
            root,
            popup_root,
            popup: false,
            reviews: 0,
            evidence: Rc::clone(&evidence),
        };
        evidence.created.set(serial);
        Ok(panel)
    }

    pub fn serial(&self) -> usize {
        self.serial
    }
    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }
    pub fn draft(&self) -> &str {
        self.field.editor().text()
    }
    pub fn display_text(&self) -> &str {
        self.field.editor().display_text()
    }
    pub fn field_key(&self) -> &UiKey {
        self.field.key()
    }
    pub fn input_token(&self) -> Option<UiTextInputToken> {
        self.field.input_token()
    }
    pub fn popup_open(&self) -> bool {
        self.popup
    }
    pub fn reviews(&self) -> usize {
        self.reviews
    }

    /// Semantic CPU callers explicitly establish local focus; this is not native input admission.
    pub fn focus_for_cpu(&mut self) -> Result<UiTextInputToken> {
        Ok(self.field.focus()?)
    }

    pub fn edit(&mut self, token: UiTextInputToken, edit: &TextEdit) -> Result<()> {
        self.field.edit(token, edit, &mut self.text)?;
        Ok(())
    }

    pub fn replace_for_cpu(&mut self, value: &str) -> Result<UiTextInputToken> {
        let token = self.focus_for_cpu()?;
        self.edit(token, &TextEdit::SelectAll)?;
        self.edit(token, &TextEdit::Commit(value.into()))?;
        Ok(token)
    }

    /// Repeated composition creates descriptions without submitting Editor work.
    pub fn compose(&self, revision: u64) -> Result<UiAuthoringSnapshot> {
        let button = |index: usize, root: &UiRootKey, style| {
            self.buttons[index].0.authored_node(
                root,
                style,
                UiVisual::role(UiColorRole::Accent),
                UiBrush::Role(UiColorRole::PrimaryText),
                16.0,
            )
        };
        let base = UiRoot::new(self.root.clone(), UiLayer::Base, UiStyle::fill())
            .with_visual(UiVisual::role(UiColorRole::Surface))
            .with_child(self.field.authored_node(
                &self.root,
                UiStyle::absolute(12.0, 12.0, 160.0, 40.0),
                160.0,
            )?)
            .with_child(button(
                0,
                &self.root,
                UiStyle::absolute(12.0, 70.0, 160.0, 40.0),
            )?);
        let mut builder = UiAuthoringSnapshotBuilder::new(revision).with_root(base);
        if self.popup {
            builder = builder.with_root(
                UiRoot::new(self.popup_root.clone(), UiLayer::Modal, UiStyle::fill()).with_child(
                    UiNode::new(UiStyle::absolute(190.0, 12.0, 180.0, 140.0))
                        .with_visual(UiVisual::role(UiColorRole::Panel))
                        .with_child(button(
                            1,
                            &self.popup_root,
                            UiStyle::absolute(8.0, 24.0, 160.0, 40.0),
                        )?),
                ),
            );
        }
        Ok(builder.build()?)
    }

    /// Reduces admitted input to local state and an explicit request to apply the current draft.
    pub fn reduce(&mut self, dispatch: &UiDispatchBatch) -> Result<bool> {
        self.field.reduce_focus(dispatch, &mut self.text)?;
        let mut intents = UiIntentBatch::default();
        for (button, state) in &mut self.buttons {
            state.reduce_batch(button, dispatch, &mut intents)?;
        }
        let mut apply = false;
        for UiIntent::Activate { action, .. } in intents.intents() {
            match action.as_str() {
                "review" => {
                    self.reviews += 1;
                    self.popup = true;
                }
                "apply" => {
                    self.popup = false;
                    apply = true;
                }
                _ => unreachable!("only this panel's buttons emit its intents"),
            }
        }
        Ok(apply)
    }

    /// Prepares one owned candidate; callers retain it if Context refuses queue capacity.
    pub fn prepare_edit(
        &self,
        context: &mut EditorPanelContext<'_, '_>,
        basis: &EditorDocumentQueryBasis,
        entity: SceneEntityId,
    ) -> std::result::Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        context.prepare_authoring_property_edit(
            basis,
            &[AuthoringPropertyEdit {
                entity,
                component: ComponentTypeId::new(COMPONENT),
                component_version: ComponentSchemaVersion::ONE,
                property: ComponentPropertyPath::new(ComponentFieldId::new(FIELD)),
                value: ComponentValue::String(self.draft().into()),
            }],
        )
    }
}

impl EditorPanel for DraftPanel {
    fn action_finished(&mut self, id: PanelSubmissionId, dispatch: PanelActionDispatch) {
        self.evidence
            .finished
            .borrow_mut()
            .push((self.serial, id, dispatch));
    }
}

impl Drop for DraftPanel {
    fn drop(&mut self) {
        self.evidence.dropped.borrow_mut().push(self.serial);
    }
}
