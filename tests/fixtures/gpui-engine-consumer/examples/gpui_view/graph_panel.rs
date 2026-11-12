//! Independent graph presentation and drafts over a shared, domain-owned document.

use super::text_input::DraftInput;
use nara_core::DocumentRevision;
use nara_engine::tooling::EditorDocumentId;
use nara_gpui_engine_consumer::graph::{
    GraphCommand, GraphView, Link, Node, NodeId, NodeKind,
    canvas::{CanvasButton, CanvasTransform, GraphCanvas, ScrollUnit},
    evaluate,
    preview::GraphPreviewBasis,
};
use open_gpui::{
    AnyElement, App, Bounds, Context, Div, Element, ElementId, Entity, FocusHandle,
    GlobalElementId, InspectorElementId, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent,
    PathBuilder, Pixels, Point, PointerCaptureHandle, PrepaintPublicationId, Render, Role,
    ScrollDelta, ScrollEvent, ScrollWheelIntent, SharedString, Stateful, Subscription,
    TargetedEvent, Window, WindowMouseEvent, canvas, div, point, prelude::*, px, rgb,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct GraphSnapshot {
    pub document: EditorDocumentId,
    pub revision: DocumentRevision,
    pub view: GraphView,
    pub dirty: bool,
    pub status: Option<String>,
}

pub(super) struct GraphModel {
    pub snapshot: GraphSnapshot,
}

impl GraphModel {
    pub fn publish(&mut self, snapshot: GraphSnapshot, cx: &mut Context<Self>) {
        if self.snapshot != snapshot {
            self.snapshot = snapshot;
            cx.notify();
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum GraphIntent {
    Edit {
        document: EditorDocumentId,
        revision: DocumentRevision,
        command: GraphCommand,
    },
    Undo {
        document: EditorDocumentId,
        revision: DocumentRevision,
    },
    Redo {
        document: EditorDocumentId,
        revision: DocumentRevision,
    },
    Save {
        document: EditorDocumentId,
        revision: DocumentRevision,
    },
    Reopen {
        document: EditorDocumentId,
        revision: DocumentRevision,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphHit {
    Canvas,
    AddScalar,
    AddVector,
    Add,
    Output,
    Undo,
    Redo,
    Save,
    Reopen,
    Draft,
    Apply,
    Reload,
    ZoomIn,
    ZoomOut,
    PanRight,
    Node(NodeId),
    OutputPort(NodeId),
    InputPort(NodeId, u8),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Basis {
    document: EditorDocumentId,
    revision: DocumentRevision,
}

impl From<&GraphSnapshot> for Basis {
    fn from(snapshot: &GraphSnapshot) -> Self {
        Self {
            document: snapshot.document,
            revision: snapshot.revision,
        }
    }
}

struct ControlGeometry {
    hit: GraphHit,
    layout: Bounds<Pixels>,
    visible: Bounds<Pixels>,
}

#[derive(Default)]
struct FrameGeometry {
    controls: Vec<ControlGeometry>,
    links: usize,
    transform: CanvasTransform,
    scale: f32,
    node_preview: Option<(NodeId, [f32; 2])>,
}

#[derive(Clone)]
struct GraphPresentation {
    basis: Basis,
    frame: Rc<RefCell<FrameGeometry>>,
}

struct CanvasCapture {
    basis: Basis,
    viewport: Bounds<Pixels>,
    scale: f32,
}

impl FrameGeometry {
    fn port(&self, hit: GraphHit) -> Option<Point<Pixels>> {
        self.controls
            .iter()
            .find(|control| control.hit == hit)
            .map(|control| control.layout.center())
    }
}

struct RecordedControl {
    hit: GraphHit,
    child: AnyElement,
    frame: Rc<RefCell<FrameGeometry>>,
}

impl IntoElement for RecordedControl {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for RecordedControl {
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
        self.child.prepaint(window, cx);
        if let Ok(geometry) = window.try_element_geometry(bounds) {
            let control = ControlGeometry {
                hit: self.hit,
                layout: geometry.layout_bounds(),
                visible: geometry.displayed_bounds().intersect(&window.clip_bounds()),
            };
            let mut frame = self.frame.borrow_mut();
            if let Some(old) = frame.controls.iter_mut().find(|old| old.hit == self.hit) {
                *old = control;
            } else {
                frame.controls.push(control);
            }
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

pub(super) struct GraphPanel {
    index: usize,
    model: Entity<GraphModel>,
    snapshot: GraphSnapshot,
    pub draft: Entity<DraftInput>,
    intents: Rc<RefCell<VecDeque<GraphIntent>>>,
    selected: Option<NodeId>,
    outgoing: Option<(Basis, NodeId)>,
    canvas: GraphCanvas,
    capture: Option<CanvasCapture>,
    pointer: PointerCaptureHandle,
    focus: FocusHandle,
    draft_basis: Basis,
    original: String,
    submitted: Option<(Basis, NodeId, f64, String)>,
    conflict: bool,
    error: Option<String>,
    frame: Rc<RefCell<FrameGeometry>>,
    presented: Rc<RefCell<Option<GraphPresentation>>>,
    publication: PrepaintPublicationId,
    _subscription: Subscription,
    _pointer_events: Subscription,
}

impl GraphPanel {
    pub fn new(
        index: usize,
        model: Entity<GraphModel>,
        intents: Rc<RefCell<VecDeque<GraphIntent>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = model.read(cx).snapshot.clone();
        let draft = cx.new(|cx| DraftInput::new(window, cx));
        let weak = cx.weak_entity();
        let subscription = window.observe(&model, cx, move |model, window, cx| {
            let snapshot = model.read(cx).snapshot.clone();
            let _ = weak.update(cx, |this, cx| this.synchronize(snapshot, window, cx));
        });
        let weak = cx.weak_entity();
        let pointer_events = window.intercept_window_mouse_events(move |event, window, cx| {
            let _ = weak.update(cx, |this, cx| this.pointer_event(event, window, cx));
        });
        Self {
            index,
            model,
            draft,
            intents,
            draft_basis: Basis::from(&snapshot),
            snapshot,
            selected: None,
            outgoing: None,
            canvas: GraphCanvas::default(),
            capture: None,
            pointer: window.new_pointer_capture_handle(),
            focus: cx.focus_handle(),
            original: String::new(),
            submitted: None,
            conflict: false,
            error: None,
            frame: Rc::default(),
            presented: Rc::default(),
            publication: PrepaintPublicationId::new(),
            _subscription: subscription,
            _pointer_events: pointer_events,
        }
    }

    /// Returns geometry published by the original Window after exact frame submission.
    pub fn hit_center(&self, hit: GraphHit) -> Option<Point<Pixels>> {
        let presented = self.presented.borrow();
        let frame = presented.as_ref()?.frame.borrow();
        frame
            .controls
            .iter()
            .rev()
            .find(|control| control.hit == hit && !control.visible.is_empty())
            .map(|control| control.visible.center())
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.selected
    }
    pub fn transform(&self) -> ([f32; 2], f32) {
        let transform = self.canvas.transform();
        (transform.pan, transform.zoom)
    }

    pub fn canvas_bounds(&self) -> Option<Bounds<Pixels>> {
        self.presented
            .borrow()
            .as_ref()?
            .frame
            .borrow()
            .controls
            .iter()
            .find(|control| control.hit == GraphHit::Canvas)
            .map(|control| control.visible)
    }

    pub fn gesture_active(&self) -> bool {
        self.canvas.is_active()
    }
    pub fn conflict(&self) -> bool {
        self.conflict
    }
    /// Counts paths emitted by the canvas for the currently submitted frame.
    pub fn rendered_links(&self) -> usize {
        self.presented
            .borrow()
            .as_ref()
            .map_or(0, |presented| presented.frame.borrow().links)
    }

    pub fn presents(&self, snapshot: &GraphSnapshot) -> bool {
        self.presented.borrow().as_ref().is_some_and(|presented| {
            let frame = presented.frame.borrow();
            presented.basis == Basis::from(snapshot)
                && frame.transform == self.canvas.transform()
                && frame.node_preview == self.node_preview()
        })
    }

    fn node_preview(&self) -> Option<(NodeId, [f32; 2])> {
        let node = self.canvas.active_node()?;
        let source = self.snapshot.view.node(node)?;
        Some((node, self.canvas.node_position(node, source.position)))
    }

    pub fn link_probe(&self) -> Option<Point<Pixels>> {
        let link = self.snapshot.view.links.last()?;
        let presented = self.presented.borrow();
        let presented = presented.as_ref()?;
        if presented.basis != Basis::from(&self.snapshot) {
            return None;
        }
        let frame = presented.frame.borrow();
        let from = frame.port(GraphHit::OutputPort(link.from))?;
        let to = frame.port(GraphHit::InputPort(link.to, link.input))?;
        Some(point((from.x + to.x) / 2., (from.y + to.y) / 2.))
    }

    fn displayed_canvas(&self) -> Option<(Basis, Bounds<Pixels>, CanvasTransform, f32)> {
        let presented = self.presented.borrow();
        let presented = presented.as_ref()?;
        let frame = presented.frame.borrow();
        let viewport = frame
            .controls
            .iter()
            .find(|control| control.hit == GraphHit::Canvas)?;
        Some((
            presented.basis,
            viewport.layout,
            frame.transform,
            frame.scale,
        ))
    }

    fn cancel_gesture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let changed = self.canvas.cancel();
        self.capture = None;
        if self.pointer.is_captured(window)
            && let Err(error) = window.release_pointer(&self.pointer)
        {
            self.error = Some(error.to_string());
        }
        if changed {
            cx.notify();
        }
    }

    fn begin_gesture(
        &mut self,
        node: Option<NodeId>,
        basis: Basis,
        event: &TargetedEvent<MouseDownEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((displayed, viewport, transform, scale)) = self.displayed_canvas() else {
            return;
        };
        if self.canvas.is_active()
            || basis != displayed
            || basis != Basis::from(&self.model.read(cx).snapshot)
            || self.canvas.transform() != transform
            || window.scale_factor() != scale
        {
            return;
        }
        let position = event.window_event().position;
        let pointer = [
            f32::from(position.x - viewport.origin.x),
            f32::from(position.y - viewport.origin.y),
        ];
        if window
            .capture_pointer(&self.pointer, event.window_event().button)
            .is_err()
        {
            return;
        }
        let began = if let Some(node) = node {
            let Some(position) = self.snapshot.view.node(node).map(|node| node.position) else {
                let _ = window.release_pointer(&self.pointer);
                return;
            };
            self.canvas.begin_node(
                GraphPreviewBasis {
                    document: basis.document,
                    revision: basis.revision,
                },
                node,
                position,
                pointer,
            )
        } else {
            self.canvas.begin_pan(pointer)
        };
        if !began {
            let _ = window.release_pointer(&self.pointer);
            return;
        }
        self.capture = Some(CanvasCapture {
            basis,
            viewport,
            scale,
        });
        if let Some(node) = node
            && self.selected != Some(node)
        {
            self.selected = Some(node);
            self.reload(window, cx);
        }
        if node.is_none() {
            self.focus.focus(window, cx);
            cx.stop_propagation();
        }
        cx.notify();
    }

    fn pointer_event(
        &mut self,
        event: WindowMouseEvent<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, WindowMouseEvent::Cancel(_)) {
            self.cancel_gesture(window, cx);
            return;
        }
        if !self.canvas.is_active() {
            return;
        }
        let current = self.displayed_canvas();
        let valid = self.capture.as_ref().is_some_and(|capture| {
            capture.basis == Basis::from(&self.model.read(cx).snapshot)
                && current.is_some_and(|(_, viewport, _, scale)| {
                    viewport == capture.viewport && scale == capture.scale
                })
                && window.scale_factor() == capture.scale
                && self.pointer.is_captured(window)
        });
        if !valid {
            self.cancel_gesture(window, cx);
            return;
        }
        let capture = self.capture.as_ref().unwrap();
        let local = |position: Point<Pixels>| {
            [
                f32::from(position.x - capture.viewport.origin.x),
                f32::from(position.y - capture.viewport.origin.y),
            ]
        };
        match event {
            WindowMouseEvent::Move(event) => {
                if self.canvas.motion(local(event.position)) {
                    cx.notify();
                }
                cx.stop_propagation();
            }
            WindowMouseEvent::Up(event) => {
                let button = match event.button {
                    MouseButton::Left => Some(CanvasButton::Primary),
                    MouseButton::Middle => Some(CanvasButton::Middle),
                    _ => None,
                };
                let movement =
                    button.and_then(|button| self.canvas.finish(button, local(event.position)));
                if !self.canvas.is_active() {
                    self.capture = None;
                    let _ = window.release_pointer(&self.pointer);
                }
                if let Some(movement) = movement {
                    self.edit(
                        Basis {
                            document: movement.basis.document,
                            revision: movement.basis.revision,
                        },
                        GraphCommand::Move {
                            node: movement.node,
                            position: movement.position,
                        },
                    );
                }
                cx.stop_propagation();
                cx.notify();
            }
            _ => {}
        }
    }

    fn scroll_canvas(
        &mut self,
        event: &TargetedEvent<ScrollEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ScrollWheelIntent {
        let Some((basis, viewport, _, scale)) = self.displayed_canvas() else {
            return ScrollWheelIntent::allow_default();
        };
        if basis != Basis::from(&self.model.read(cx).snapshot) || scale != window.scale_factor() {
            return ScrollWheelIntent::allow_default();
        }
        let event = event.window_event();
        let pointer = [
            f32::from(event.position.x - viewport.origin.x),
            f32::from(event.position.y - viewport.origin.y),
        ];
        let (delta, unit) = match event.delta {
            ScrollDelta::Lines(delta) => ([delta.x, delta.y], ScrollUnit::Lines),
            ScrollDelta::Pixels(delta) => {
                ([f32::from(delta.x), f32::from(delta.y)], ScrollUnit::Pixels)
            }
        };
        if self.canvas.scroll(
            pointer,
            delta,
            unit,
            event.modifiers.control || event.modifiers.platform,
        ) {
            if !self.canvas.is_active() {
                self.capture = None;
                let _ = window.release_pointer(&self.pointer);
            }
            cx.notify();
        }
        ScrollWheelIntent::handled().stop_propagation()
    }

    fn scalar(&self) -> Option<f64> {
        self.snapshot.view.nodes.iter().find_map(|node| {
            if Some(node.id) == self.selected {
                if let NodeKind::Scalar(value) = &node.kind {
                    return Some(*value);
                }
            }
            None
        })
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.original = self
            .scalar()
            .map(|value| value.to_string())
            .unwrap_or_default();
        self.draft.update(cx, |draft, cx| {
            draft.rebind(self.original.clone(), window, cx)
        });
        self.draft_basis = Basis::from(&self.snapshot);
        self.submitted = None;
        self.conflict = false;
        self.error = None;
        cx.notify();
    }

    fn synchronize(
        &mut self,
        snapshot: GraphSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        let previous = Basis::from(&self.snapshot);
        let next = Basis::from(&snapshot);
        if previous != next {
            self.cancel_gesture(window, cx);
        }
        let rebound = previous.document != next.document
            || previous.revision.source_id() != next.revision.source_id();
        let dirty = {
            let draft = self.draft.read(cx);
            draft.is_composing() || draft.text() != self.original
        };
        self.snapshot = snapshot;
        let missing = self.selected.is_some_and(|selected| {
            !self
                .snapshot
                .view
                .nodes
                .iter()
                .any(|node| node.id == selected)
        });
        if rebound || missing {
            self.selected = None;
            self.outgoing = None;
            self.reload(window, cx);
        } else if previous != next {
            self.outgoing = None;
            let accepted = self
                .submitted
                .as_ref()
                .is_some_and(|(basis, node, value, text)| {
                    *basis == previous
                        && Some(*node) == self.selected
                        && previous.revision.generation().checked_add(1)
                            == Some(next.revision.generation())
                        && self.scalar() == Some(*value)
                        && !self.draft.read(cx).is_composing()
                        && self.draft.read(cx).text() == text.as_str()
                });
            if !self.conflict && (!dirty || accepted) {
                self.reload(window, cx);
            } else {
                self.submitted = None;
                self.conflict = true;
                self.error = Some("Source changed; Reload the scalar draft".into());
            }
        }
        cx.notify();
    }

    fn edit(&self, basis: Basis, command: GraphCommand) {
        self.intents.borrow_mut().push_back(GraphIntent::Edit {
            document: basis.document,
            revision: basis.revision,
            command,
        });
    }

    fn apply(&mut self, basis: Basis, cx: &mut Context<Self>) {
        if self.conflict || self.draft_basis != basis {
            self.error = Some("Source changed; Reload the scalar draft".into());
            return;
        }
        let Some(node) = self.selected.filter(|_| self.scalar().is_some()) else {
            self.error = Some("Select a scalar node".into());
            return;
        };
        let draft = self.draft.read(cx);
        if draft.is_composing() {
            self.error = Some("Finish composition before Apply".into());
            return;
        }
        let Some(value) = draft
            .text()
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
        else {
            self.error = Some("Enter a finite scalar".into());
            return;
        };
        self.submitted = Some((basis, node, value, draft.text().to_owned()));
        self.edit(basis, GraphCommand::SetScalar { node, value });
    }

    fn activate(
        &mut self,
        hit: GraphHit,
        basis: Basis,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if hit == GraphHit::Reload {
            self.reload(window, cx);
            return;
        }
        if basis != Basis::from(&self.model.read(cx).snapshot) {
            self.error = Some("Displayed revision changed; retry after refresh".into());
            cx.notify();
            return;
        }
        self.error = None;
        let Basis { document, revision } = basis;
        match hit {
            GraphHit::AddScalar | GraphHit::AddVector | GraphHit::Add | GraphHit::Output => {
                let kind = match hit {
                    GraphHit::AddScalar => NodeKind::Scalar(1.),
                    GraphHit::AddVector => NodeKind::Vector([1., 2.]),
                    GraphHit::Add => NodeKind::Add,
                    _ => NodeKind::Output,
                };
                let position = (0..240)
                    .map(|slot| {
                        [
                            24. + (slot % 3) as f32 * 184.,
                            24. + (slot / 3) as f32 * 120.,
                        ]
                    })
                    .find(|position| {
                        self.snapshot.view.nodes.iter().all(|node| {
                            (node.position[0] - position[0]).abs() >= 156.
                                || (node.position[1] - position[1]).abs() >= 104.
                        })
                    });
                let Some(position) = position else {
                    self.error =
                        Some("Move existing nodes to free a position before adding".into());
                    cx.notify();
                    return;
                };
                self.edit(basis, GraphCommand::Add { kind, position });
            }
            GraphHit::Undo => self
                .intents
                .borrow_mut()
                .push_back(GraphIntent::Undo { document, revision }),
            GraphHit::Redo => self
                .intents
                .borrow_mut()
                .push_back(GraphIntent::Redo { document, revision }),
            GraphHit::Save => self
                .intents
                .borrow_mut()
                .push_back(GraphIntent::Save { document, revision }),
            GraphHit::Reopen => self
                .intents
                .borrow_mut()
                .push_back(GraphIntent::Reopen { document, revision }),
            GraphHit::Apply => self.apply(basis, cx),
            GraphHit::ZoomIn | GraphHit::ZoomOut => {
                self.cancel_gesture(window, cx);
                self.canvas
                    .zoom_by(if hit == GraphHit::ZoomIn { 1.25 } else { 0.8 }, [0., 0.]);
            }
            GraphHit::PanRight => {
                self.cancel_gesture(window, cx);
                self.canvas.pan_by([32., 0.]);
            }
            GraphHit::Node(node) => {
                if self.selected != Some(node) {
                    self.selected = Some(node);
                    self.reload(window, cx);
                }
            }
            GraphHit::OutputPort(node) => self.outgoing = Some((basis, node)),
            GraphHit::InputPort(to, input) => {
                if let Some((source, from)) = self.outgoing.take() {
                    if source == basis {
                        self.edit(basis, GraphCommand::Connect(Link { from, to, input }));
                    } else {
                        self.error = Some("Source changed; choose the output port again".into());
                    }
                } else if self
                    .snapshot
                    .view
                    .links
                    .iter()
                    .any(|link| link.to == to && link.input == input)
                {
                    self.edit(basis, GraphCommand::Disconnect { to, input });
                } else {
                    self.error = Some("Choose an output port before an input port".into());
                }
            }
            GraphHit::Canvas | GraphHit::Draft | GraphHit::Reload => {}
        }
        cx.notify();
    }

    fn node_key(
        &mut self,
        node: NodeId,
        basis: Basis,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.modified()
            || basis != Basis::from(&self.model.read(cx).snapshot)
        {
            return;
        }
        let command = match event.keystroke.key.as_str() {
            "delete" | "backspace" => GraphCommand::Remove(node),
            key => {
                let delta = match key {
                    "left" => [-16., 0.],
                    "right" => [16., 0.],
                    "up" => [0., -16.],
                    "down" => [0., 16.],
                    _ => return,
                };
                let Some(current) = self
                    .snapshot
                    .view
                    .nodes
                    .iter()
                    .find(|current| current.id == node)
                else {
                    return;
                };
                GraphCommand::Move {
                    node,
                    position: [
                        current.position[0] + delta[0],
                        current.position[1] + delta[1],
                    ],
                }
            }
        };
        self.edit(basis, command);
        cx.stop_propagation();
    }

    fn button(
        &self,
        hit: GraphHit,
        label: impl Into<SharedString>,
        basis: Basis,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let label = label.into();
        let selected = self.selected;
        div()
            .id(format!(
                "graph:{}:{}:{:?}:{hit:?}",
                self.index,
                basis.document.raw(),
                basis.revision.source_id()
            ))
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label(label.clone())
            .h(px(28.))
            .px(px(7.))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(rgb(0x456078))
            .rounded_sm()
            .bg(rgb(0x2d4258))
            .focus_visible(|style| style.border_color(rgb(0x9bd7ff)))
            .cursor_pointer()
            .overflow_hidden()
            .child(label)
            .capture_any_mouse_down(cx.listener(move |this, _, window, cx| {
                if hit == GraphHit::Apply && this.draft.read(cx).is_composing() {
                    window.prevent_default();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                if hit == GraphHit::Apply && this.selected != selected {
                    this.error = Some("Selection changed; retry after refresh".into());
                    cx.notify();
                    return;
                }
                this.activate(hit, basis, window, cx);
            }))
    }

    fn record(&self, hit: GraphHit, child: impl IntoElement) -> RecordedControl {
        RecordedControl {
            hit,
            child: child.into_any_element(),
            frame: self.frame.clone(),
        }
    }

    fn node_card(&self, node: &Node, basis: Basis, cx: &Context<Self>) -> Div {
        let id = node.id;
        let CanvasTransform { pan, zoom } = self.canvas.transform();
        let position = self.canvas.node_position(id, node.position);
        let output_label = if matches!(&node.kind, NodeKind::Vector(_)) {
            "vec2"
        } else {
            "f64"
        };
        let (label, inputs, output) = match &node.kind {
            NodeKind::Scalar(value) => (format!("Scalar {} = {value}", id.0), 0, true),
            NodeKind::Vector(value) => (
                format!("Vector {} = [{}, {}]", id.0, value[0], value[1]),
                0,
                true,
            ),
            NodeKind::Add => (format!("Add {}", id.0), 2, true),
            NodeKind::Output => (format!("Output {}", id.0), 1, false),
        };
        let header = self
            .button(GraphHit::Node(id), label, basis, cx)
            .w(px(144. * zoom))
            .h(px(28. * zoom))
            .text_size(px(12. * zoom))
            .aria_selected(self.selected == Some(id))
            .bg(rgb(if self.selected == Some(id) {
                0x35678a
            } else {
                0x2d4258
            }))
            .on_key_down(cx.listener(move |this, event, _, cx| this.node_key(id, basis, event, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| {
                    this.begin_gesture(Some(id), basis, event, window, cx);
                }),
            );
        div()
            .absolute()
            .left(px(pan[0] + position[0] * zoom))
            .top(px(pan[1] + position[1] * zoom))
            .w(px(144. * zoom))
            .h(px(96. * zoom))
            .bg(rgb(0x253347))
            .rounded_sm()
            .child(self.record(GraphHit::Node(id), header))
            .children((0..inputs).map(|input| {
                let hit = GraphHit::InputPort(id, input);
                self.record(
                    hit,
                    self.button(hit, if input == 0 { "A" } else { "B" }, basis, cx)
                        .aria_label(format!(
                            "Node {} scalar input {}; click a connected input to disconnect",
                            id.0, input
                        ))
                        .absolute()
                        .left(px(0.))
                        .top(px((36. + f32::from(input) * 28.) * zoom))
                        .w(px(30. * zoom))
                        .h(px(24. * zoom))
                        .px(px(0.))
                        .text_size(px(12. * zoom)),
                )
            }))
            .when(output, |card| {
                let hit = GraphHit::OutputPort(id);
                card.child(
                    self.record(
                        hit,
                        self.button(hit, output_label, basis, cx)
                            .aria_label(format!(
                                "Node {} {output_label} output; choose an input next",
                                id.0
                            ))
                            .absolute()
                            .left(px(108. * zoom))
                            .top(px(36. * zoom))
                            .w(px(36. * zoom))
                            .h(px(24. * zoom))
                            .px(px(0.))
                            .text_size(px(12. * zoom))
                            .when(
                                self.outgoing.is_some_and(|(_, from)| from == id),
                                |button| button.bg(rgb(0x567d42)),
                            ),
                    ),
                )
            })
    }

    fn graph_canvas(&self) -> impl IntoElement + use<> {
        let links = self.snapshot.view.links.clone();
        let frame = self.frame.clone();
        let zoom = self.canvas.transform().zoom;
        let transform = self.canvas.transform();
        let publication = self.publication;
        let presented = self.presented.clone();
        let presentation = GraphPresentation {
            basis: Basis::from(&self.snapshot),
            frame: frame.clone(),
        };
        canvas(
            move |_, window, _| {
                let mut geometry = presentation.frame.borrow_mut();
                geometry.transform = transform;
                geometry.scale = window.scale_factor();
                drop(geometry);
                let committed = presented.clone();
                let discarded = presented.clone();
                let presentation = presentation.clone();
                window.record_prepaint_window_transaction(
                    publication,
                    move |_, _, _| {
                        *committed.borrow_mut() = Some(presentation.clone());
                    },
                    move |_, _, _| {
                        discarded.borrow_mut().take();
                    },
                );
            },
            move |bounds, _, window, _| {
                let mut frame = frame.borrow_mut();
                frame.links = 0;
                for link in links {
                    let Some(from) = frame.port(GraphHit::OutputPort(link.from)) else {
                        continue;
                    };
                    let Some(to) = frame.port(GraphHit::InputPort(link.to, link.input)) else {
                        continue;
                    };
                    let bend = px((f32::from(to.x - from.x).abs() * 0.5).max(40. * zoom));
                    let mut path = PathBuilder::stroke(px(2. * zoom));
                    path.move_to(from);
                    path.cubic_bezier_to(
                        to,
                        point(from.x + bend, from.y),
                        point(to.x - bend, to.y),
                    );
                    match path.build() {
                        Ok(path) if path.bounds.intersects(&bounds) => {
                            window.paint_path(path, rgb(0x89c4e8));
                            frame.links += 1;
                        }
                        Err(error) => log::error!("graph link path failed: {error}"),
                        _ => {}
                    }
                }
            },
        )
        .absolute()
        .size_full()
    }
}

impl Render for GraphPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frame = Rc::new(RefCell::new(FrameGeometry {
            node_preview: self.node_preview(),
            ..Default::default()
        }));
        let basis = Basis::from(&self.snapshot);
        let output = match evaluate(&self.snapshot.view) {
            Ok(Some(value)) => format!("Output: {value}"),
            Ok(None) => "Output: not connected".into(),
            Err(error) => format!("Output: {error}"),
        };
        let status = if self.conflict {
            "Source changed; Reload the scalar draft".into()
        } else {
            self.error
                .clone()
                .or_else(|| self.snapshot.status.clone())
                .unwrap_or_else(|| {
                    self.outgoing.map_or_else(
                        || "Choose a node or connect ports".into(),
                        |(_, node)| format!("Connect node {} output to an input", node.0),
                    )
                })
        };
        let rows = [
            vec![
                (GraphHit::AddScalar, "Add scalar"),
                (GraphHit::AddVector, "Add vector"),
                (GraphHit::Add, "Add"),
                (GraphHit::Output, "Output"),
                (GraphHit::Undo, "Undo"),
                (GraphHit::Redo, "Redo"),
            ],
            vec![
                (GraphHit::Save, "Save"),
                (GraphHit::Reopen, "Reopen"),
                (GraphHit::ZoomIn, "Zoom +"),
                (GraphHit::ZoomOut, "Zoom -"),
                (GraphHit::PanRight, "Pan right"),
            ],
        ];
        div().id(format!("graph-panel:{}", self.index)).size_full().p_2()
            .flex().flex_col().gap_1().min_h(px(0.)).overflow_hidden()
            .bg(rgb(0x172434)).text_color(rgb(0xe6eef8)).text_size(px(13.)).line_height(px(20.))
            .font_family("Segoe UI").track_focus(&self.focus).tab_group().tab_stop(false)
            .role(Role::Group).aria_label(format!("Graph panel {}", self.index + 1))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" {
                    if event.keystroke.modifiers.shift { window.focus_prev(cx); } else { window.focus_next(cx); }
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" {
                    this.outgoing = None;
                    this.cancel_gesture(window, cx);
                    cx.notify();
                }
            }))
            .child(div().h(px(22.)).flex_shrink_0().child(format!(
                "Graph {} · {} · {}% · {output}", self.index + 1,
                if self.snapshot.dirty { "Modified" } else { "Saved" }, (self.canvas.transform().zoom * 100.).round(),
            )))
            .children(rows.into_iter().map(|buttons| {
                div().h(px(28.)).flex_shrink_0().flex().gap_1()
                    .children(buttons.into_iter().map(|(hit, label)| self.record(hit, self.button(hit, label, basis, cx))))
            }))
            .child(div().h(px(46.)).flex_shrink_0().flex().items_center().gap_1()
                .child(div().flex_1().min_w(px(0.))
                    .when(self.scalar().is_some(), |field| field.child(self.record(GraphHit::Draft, self.draft.clone())))
                    .when(self.scalar().is_none(), |field| field.child("Select a scalar node to edit")))
                .child(self.record(GraphHit::Apply, self.button(GraphHit::Apply, "Apply", basis, cx)))
                .child(self.record(GraphHit::Reload, self.button(GraphHit::Reload, "Reload", basis, cx))))
            .child(div().h(px(36.)).flex_shrink_0().overflow_hidden().text_size(px(12.)).child(status))
            .child(self.record(GraphHit::Canvas, div().id("graph-viewport").relative().flex_1().min_h(px(0.)).w_full().overflow_hidden().bg(rgb(0x101a26))
                .track_pointer_capture(&self.pointer)
                .on_mouse_down(MouseButton::Middle, cx.listener(move |this, event, window, cx| {
                    this.begin_gesture(None, basis, event, window, cx);
                }))
                .on_scroll_wheel({
                    let weak = cx.weak_entity();
                    move |event, window, cx| weak.update(cx, |this, cx| this.scroll_canvas(event, window, cx))
                        .unwrap_or_else(|_| ScrollWheelIntent::allow_default())
                })
                .child(self.graph_canvas())
                .children(self.snapshot.view.nodes.iter().map(|node| self.node_card(node, basis, cx)))))
            .child(div().h(px(18.)).flex_shrink_0().text_size(px(11.)).child(
                "Node drag: move · Middle drag: pan · Ctrl wheel: zoom · Esc: cancel · Arrows / Delete: node",
            ))
    }
}
