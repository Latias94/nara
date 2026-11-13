use super::*;
use nara_gpui_engine_consumer::dialogue::ChoiceId;
use open_gpui::{
    Bounds, Element, ElementId, GlobalElementId, InspectorElementId, LayoutId, Pixels, Point,
    PrepaintPublicationId,
};
use std::{cell::Cell, collections::BTreeMap};

pub(crate) type TextCallback = Rc<dyn Fn(String, &mut Window, &mut App)>;
pub(crate) type ActivateCallback = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PreviewPresentation {
    Line {
        node: NodeId,
        speaker: String,
        text: String,
    },
    Choice {
        node: NodeId,
        prompt: String,
        choices: Vec<PreviewChoice>,
    },
    End {
        node: NodeId,
        text: String,
    },
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreviewChoice {
    pub id: ChoiceId,
    pub label: String,
}

impl PreviewPresentation {
    pub fn capture(result: std::result::Result<ConversationStep<'_>, String>) -> Self {
        match result {
            Ok(ConversationStep::Line {
                node,
                speaker,
                text,
                ..
            }) => Self::Line {
                node,
                speaker: speaker.into(),
                text: text.into(),
            },
            Ok(ConversationStep::Choice {
                node,
                prompt,
                choices,
            }) => Self::Choice {
                node,
                prompt: prompt.into(),
                choices: choices
                    .iter()
                    .map(|choice| PreviewChoice {
                        id: choice.id,
                        label: choice.label.clone(),
                    })
                    .collect(),
            },
            Ok(ConversationStep::End { node, text }) => Self::End {
                node,
                text: text.into(),
            },
            Err(error) => Self::Unavailable(error),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Hit {
    GraphUnavailable,
    ResourceQuery,
    ResourceToggle,
    ResourceRow(usize),
    ResourceOpen,
    ResourceRefresh,
    CloseView,
    CreateLine,
    CreateChoice,
    CreateEnd,
    NodeSelect(NodeId),
    SetEntry,
    RemoveNode,
    NextTarget,
    Undo,
    Redo,
    Save,
    Reopen,
    Text,
    Speaker,
    X,
    Y,
    Apply,
    ApplyPosition,
    Cancel,
    Review,
    ResolveCurrent(bool, usize),
    ResolveDraft(bool, usize),
    Rebind,
    ReadOnly,
    NodeSelector,
    AddChoice,
    ChoiceLabel(usize),
    ChoiceTarget(usize),
    ChoiceUp(usize),
    ChoiceRemove(usize),
    PreviewRestart,
    PreviewNext,
    PreviewChoice(ChoiceId),
    NativeSpeaker(NodeId),
    NativeText(NodeId),
    NativeApply(NodeId),
    NativeCancel(NodeId),
    NativeReview(NodeId),
    NativeRebind(NodeId),
    NativeChoiceLabel(NodeId, usize),
    NativeChoiceTarget(NodeId, usize),
    NativeChoiceUp(NodeId, usize),
    NativeChoiceRemove(NodeId, usize),
    NativeAddChoice(NodeId),
}

#[derive(Clone)]
pub(crate) struct AcceptedFrame {
    pub generation: u64,
    pub snapshot: DialogueSnapshot,
    pub node: NodeId,
    controls: BTreeMap<Hit, Bounds<Pixels>>,
    text_callbacks: BTreeMap<Hit, TextCallback>,
    activate_callbacks: BTreeMap<Hit, ActivateCallback>,
    pub preview: Option<PreviewPresentation>,
    pub preview_error: Option<String>,
}

impl AcceptedFrame {
    pub fn bounds(&self, hit: Hit) -> Result<Bounds<Pixels>> {
        self.controls
            .get(&hit)
            .copied()
            .filter(|bounds| !bounds.is_empty())
            .with_context(|| format!("Dialogue control has no accepted visible bounds: {hit:?}"))
    }

    pub fn center(&self, hit: Hit) -> Result<Point<Pixels>> {
        Ok(self.bounds(hit)?.center())
    }

    pub fn text_callback(&self, hit: Hit) -> Result<TextCallback> {
        self.text_callbacks
            .get(&hit)
            .cloned()
            .with_context(|| format!("no accepted text callback for {hit:?}"))
    }

    pub fn activate_callback(&self, hit: Hit) -> Result<ActivateCallback> {
        self.activate_callbacks
            .get(&hit)
            .cloned()
            .with_context(|| format!("no accepted activation callback for {hit:?}"))
    }
}

pub(crate) struct Probe {
    pub view_index: Cell<usize>,
    pub retired: Cell<bool>,
    pending: RefCell<BTreeMap<Hit, Bounds<Pixels>>>,
    pending_text: RefCell<BTreeMap<Hit, TextCallback>>,
    pending_activate: RefCell<BTreeMap<Hit, ActivateCallback>>,
    pub accepted: RefCell<Option<AcceptedFrame>>,
    publication: PrepaintPublicationId,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            view_index: Cell::new(0),
            retired: Cell::new(false),
            pending: RefCell::default(),
            pending_text: RefCell::default(),
            pending_activate: RefCell::default(),
            accepted: RefCell::default(),
            publication: PrepaintPublicationId::new(),
        }
    }
}

pub(super) struct ObservedElement {
    child: AnyElement,
    probe: Rc<Probe>,
    hit: Option<Hit>,
    frame: Option<(DialogueSnapshot, NodeId)>,
    text_callback: Option<TextCallback>,
    activate_callback: Option<ActivateCallback>,
    preview: Option<(PreviewPresentation, Option<String>)>,
}

impl ObservedElement {
    pub fn control(probe: &Rc<Probe>, hit: Hit, child: impl IntoElement) -> Self {
        Self {
            child: child.into_any_element(),
            probe: probe.clone(),
            hit: Some(hit),
            frame: None,
            text_callback: None,
            activate_callback: None,
            preview: None,
        }
    }

    pub fn frame(
        probe: &Rc<Probe>,
        snapshot: DialogueSnapshot,
        node: NodeId,
        child: impl IntoElement,
    ) -> Self {
        Self {
            child: child.into_any_element(),
            probe: probe.clone(),
            hit: None,
            frame: Some((snapshot, node)),
            text_callback: None,
            activate_callback: None,
            preview: None,
        }
    }

    pub fn with_text(mut self, callback: TextCallback) -> Self {
        self.text_callback = Some(callback);
        self
    }

    pub fn with_activation(mut self, callback: ActivateCallback) -> Self {
        self.activate_callback = Some(callback);
        self
    }

    pub fn with_preview(mut self, preview: PreviewPresentation, error: Option<String>) -> Self {
        self.preview = Some((preview, error));
        self
    }
}

impl Element for ObservedElement {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.frame.is_some() {
            self.probe.pending.borrow_mut().clear();
            self.probe.pending_text.borrow_mut().clear();
            self.probe.pending_activate.borrow_mut().clear();
        }
        self.child.prepaint(window, cx);
        if let Some(hit) = self.hit
            && let Ok(geometry) = window.try_element_geometry(bounds)
        {
            self.probe.pending.borrow_mut().insert(
                hit,
                geometry.displayed_bounds().intersect(&window.clip_bounds()),
            );
            if let Some(callback) = &self.text_callback {
                self.probe
                    .pending_text
                    .borrow_mut()
                    .insert(hit, callback.clone());
            }
            if let Some(callback) = &self.activate_callback {
                self.probe
                    .pending_activate
                    .borrow_mut()
                    .insert(hit, callback.clone());
            }
        }
        if let Some((snapshot, node)) = &self.frame {
            let captured = AcceptedFrame {
                generation: 0,
                snapshot: snapshot.clone(),
                node: *node,
                controls: self.probe.pending.borrow().clone(),
                text_callbacks: self.probe.pending_text.borrow().clone(),
                activate_callbacks: self.probe.pending_activate.borrow().clone(),
                preview: self.preview.as_ref().map(|(preview, _)| preview.clone()),
                preview_error: self.preview.as_ref().and_then(|(_, error)| error.clone()),
            };
            let accepted = self.probe.clone();
            let discarded = self.probe.clone();
            window.record_prepaint_window_transaction(
                self.probe.publication,
                move |fence, _, _| {
                    *accepted.accepted.borrow_mut() = (!accepted.retired.get()).then(|| {
                        let mut frame = captured.clone();
                        frame.generation = fence.generation();
                        frame
                    });
                },
                move |_, _, _| {
                    discarded.accepted.borrow_mut().take();
                },
            );
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}
