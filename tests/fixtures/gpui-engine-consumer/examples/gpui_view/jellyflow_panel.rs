use anyhow::{Context as _, Result, ensure};
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, NodeId,
    NodeKindKey, PortCapacity, PortDirection, PortId, PortKind,
};
use jellyflow_open_gpui::{
    authoring::OpenGpuiNodeBinding,
    host::OpenGpuiSceneHost,
    registry::{
        OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
    },
};
use jellyflow_runtime::{
    commands::{CommandExecutionKind, GraphCommand},
    graph::{GraphRuntime, GraphSnapshot, HistoryState},
    schema::{NodeCatalog, NodeKindSpec, NodePortSpec},
    view::{GeometrySnapshot, RendererStamp, ViewSessionOptions, WeakViewSession},
    viewport::ViewportTransform,
};
use open_gpui::{
    AnyElement, App, Bounds, Context, CursorStyle, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, LayoutId, MouseButton, Pixels, Point, PrepaintPublicationId, Render,
    Subscription, Window, div, prelude::*, px, rgb,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

const SOURCE_KIND: &str = "nara.trial.source";
const SINK_KIND: &str = "nara.trial.sink";

pub(super) fn source() -> NodeId {
    NodeId::from_u128(1)
}

pub(super) fn sink() -> NodeId {
    NodeId::from_u128(2)
}

pub(super) fn output() -> PortId {
    PortId::from_u128(11)
}

pub(super) fn input() -> PortId {
    PortId::from_u128(12)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Hit {
    Canvas,
    Undo,
    Grip(NodeId),
    Port(PortId),
    NativeControl(NodeId),
}

#[derive(Clone)]
pub(super) struct AcceptedFrame {
    pub generation: u64,
    pub graph: GraphSnapshot,
    pub transform: ViewportTransform,
    pub selection: Vec<NodeId>,
    pub controls: BTreeMap<Hit, Bounds<Pixels>>,
    pub taps: BTreeMap<NodeId, u32>,
}

impl AcceptedFrame {
    pub fn bounds(&self, hit: Hit) -> Result<Bounds<Pixels>> {
        self.controls
            .get(&hit)
            .copied()
            .filter(|bounds| !bounds.is_empty())
            .with_context(|| format!("Jellyflow control has no accepted Window geometry: {hit:?}"))
    }

    pub fn center(&self, hit: Hit) -> Result<Point<Pixels>> {
        Ok(self.bounds(hit)?.center())
    }
}

#[derive(Default)]
struct PendingGeometry {
    controls: BTreeMap<Hit, Bounds<Pixels>>,
    taps: BTreeMap<NodeId, u32>,
}

#[derive(Default)]
struct Probe {
    pending: RefCell<PendingGeometry>,
    accepted: RefCell<Option<AcceptedFrame>>,
}

struct ProbeElement {
    child: AnyElement,
    probe: Rc<Probe>,
    hit: Option<Hit>,
    taps: Option<(NodeId, u32)>,
    publication: Option<(PrepaintPublicationId, WeakViewSession)>,
}

impl ProbeElement {
    fn control(probe: &Rc<Probe>, hit: Hit, child: impl IntoElement) -> Self {
        Self {
            child: child.into_any_element(),
            probe: probe.clone(),
            hit: Some(hit),
            taps: None,
            publication: None,
        }
    }
}

impl IntoElement for ProbeElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for ProbeElement {
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
        if self.publication.is_some() {
            *self.probe.pending.borrow_mut() = PendingGeometry::default();
        }
        self.child.prepaint(window, cx);
        if let Some(hit) = self.hit
            && let Ok(geometry) = window.try_element_geometry(bounds)
        {
            let mut pending = self.probe.pending.borrow_mut();
            pending.controls.insert(
                hit,
                geometry.displayed_bounds().intersect(&window.clip_bounds()),
            );
            if let Some((node, taps)) = self.taps {
                pending.taps.insert(node, taps);
            }
        }
        if let Some((publication, view)) = &self.publication {
            let pending = self.probe.pending.borrow();
            let frame = view.graph_snapshot().and_then(|graph| {
                Ok(AcceptedFrame {
                    generation: 0,
                    graph,
                    transform: view.viewport_transform()?,
                    selection: view.selection()?.nodes().to_vec(),
                    controls: pending.controls.clone(),
                    taps: pending.taps.clone(),
                })
            });
            let accepted = self.probe.clone();
            let discarded = self.probe.clone();
            window.record_prepaint_window_transaction(
                *publication,
                move |fence, _, _| {
                    *accepted.accepted.borrow_mut() =
                        frame.as_ref().ok().cloned().map(|mut frame| {
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

struct TrialNode {
    binding: Entity<OpenGpuiNodeBinding>,
    probe: Rc<Probe>,
    taps: u32,
    _binding_subscription: Subscription,
}

impl TrialNode {
    fn new(binding: Entity<OpenGpuiNodeBinding>, probe: Rc<Probe>, cx: &mut Context<Self>) -> Self {
        let binding_subscription = cx.observe(&binding, |_, _, cx| cx.notify());
        Self {
            binding,
            probe,
            taps: 0,
            _binding_subscription: binding_subscription,
        }
    }
}

impl Render for TrialNode {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let binding = self.binding.read(cx);
        let Ok(authoring) = binding.authoring() else {
            return div().child("Retired Jellyflow node").into_any_element();
        };
        let node = authoring.node_id();
        let is_source = node == source();
        let label = if is_source {
            "Finite source"
        } else {
            "Finite sink"
        };
        let header = binding.drag_handle().wrap(
            div()
                .h(px(32.))
                .w_full()
                .px(px(10.))
                .flex()
                .items_center()
                .cursor(CursorStyle::OpenHand)
                .bg(rgb(0x2d4258))
                .child(format!("Drag title · {label}")),
        );
        let ports = authoring.ports().map(|port| {
            let marker = div().w(px(22.)).h(px(22.)).rounded_full().bg(rgb(0x80d4ee));
            div()
                .absolute()
                .left(px(if is_source { 170. } else { 8. }))
                .top(px(50.))
                .w(px(22.))
                .h(px(22.))
                .child(ProbeElement::control(
                    &self.probe,
                    Hit::Port(port.id()),
                    port.handle().interactive(marker),
                ))
        });
        let button = div()
            .id("trial-native-label")
            .absolute()
            .left(px(28.))
            .top(px(88.))
            .w(px(144.))
            .h(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x347b9f))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                this.taps += 1;
                cx.notify();
            }))
            .child(format!("Local label: {}", self.taps));
        let mut button = ProbeElement::control(&self.probe, Hit::NativeControl(node), button);
        button.taps = Some((node, self.taps));
        binding.measure_content(
            div()
                .relative()
                .w(px(200.))
                .h(px(128.))
                .bg(rgb(0x253347))
                .text_size(px(13.))
                .child(ProbeElement::control(&self.probe, Hit::Grip(node), header))
                .child(
                    div()
                        .absolute()
                        .left(px(36.))
                        .top(px(52.))
                        .child(if is_source {
                            "Output: finite value"
                        } else {
                            "Input: finite value"
                        }),
                )
                .children(ports)
                .child(button),
        )
    }
}

pub(super) struct JellyflowTrial {
    runtime: Rc<GraphRuntime>,
    hosts: [OpenGpuiSceneHost; 2],
    views: [WeakViewSession; 2],
    probes: [Rc<Probe>; 2],
}

impl JellyflowTrial {
    pub fn new() -> Result<Self> {
        let mut catalog = NodeCatalog::new();
        for (kind, direction, name) in [
            (SOURCE_KIND, PortDirection::Out, "output"),
            (SINK_KIND, PortDirection::In, "input"),
        ] {
            catalog.register(
                NodeKindSpec::builder(kind)
                    .port(NodePortSpec::new(
                        name,
                        direction,
                        PortKind::Data,
                        PortCapacity::Multi,
                    ))
                    .build(),
            )?;
        }
        let mut graph = GraphBuilder::new(GraphId::from_u128(100));
        for (kind, node, port, origin) in [
            (
                SOURCE_KIND,
                source(),
                output(),
                CanvasPoint { x: 40., y: 64. },
            ),
            (SINK_KIND, sink(), input(), CanvasPoint { x: 328., y: 240. }),
        ] {
            let creation = catalog.instantiate_node_with_ids(
                &NodeKindKey::new(kind),
                node,
                CanvasRect {
                    origin,
                    size: CanvasSize {
                        width: 200.,
                        height: 128.,
                    },
                },
                [port],
            )?;
            let (node, value, ports) = creation.into_parts();
            graph.insert_node(node, value);
            for (port, value) in ports {
                graph.insert_port(port, value);
            }
        }
        graph.insert_edge(
            EdgeId::from_u128(21),
            Edge::new(EdgeKind::Data, output(), input()),
        );
        let graph = graph
            .build()
            .map_err(|errors| anyhow::anyhow!("invalid Jellyflow trial graph: {errors:?}"))?;
        let runtime = Rc::new(GraphRuntime::builder(catalog.clone()).build(graph)?);
        let probes = std::array::from_fn(|_| Rc::new(Probe::default()));
        let mut mounted = Vec::with_capacity(2);
        for probe in &probes {
            let mut registry = OpenGpuiNodeRegistry::builder();
            for (kind, label) in [(SOURCE_KIND, "Finite source"), (SINK_KIND, "Finite sink")] {
                let probe = probe.clone();
                registry.register(OpenGpuiNodeRendererDescriptor::new(
                    kind,
                    1,
                    RendererStamp::new(format!("{kind}.native"), 1),
                    OpenGpuiNodeAdapterPresentation::new(label),
                    move |binding, _, cx| {
                        Ok(cx.new(|cx| TrialNode::new(binding, probe.clone(), cx)))
                    },
                ))?;
            }
            let view = runtime.attach_view(ViewSessionOptions::default())?;
            let weak = view.downgrade();
            let host = OpenGpuiSceneHost::new_measured_surface(view, registry.seal(&catalog)?)?;
            mounted.push((host, weak));
        }
        let [(host_a, view_a), (host_b, view_b)] = mounted
            .try_into()
            .map_err(|_| anyhow::anyhow!("Jellyflow trial requires exactly two views"))?;
        Ok(Self {
            runtime,
            hosts: [host_a, host_b],
            views: [view_a, view_b],
            probes,
        })
    }
}

pub(super) struct Observation {
    pub frames: [AcceptedFrame; 2],
    pub graph: GraphSnapshot,
    pub history: HistoryState,
    pub geometry: [GeometrySnapshot; 2],
    pub captured: bool,
    pub window_active: bool,
}

pub(super) struct JellyflowPanel {
    runtime: Rc<GraphRuntime>,
    hosts: [Entity<OpenGpuiSceneHost>; 2],
    views: [WeakViewSession; 2],
    probes: [Rc<Probe>; 2],
    publications: [PrepaintPublicationId; 2],
    command_error: Option<String>,
}

impl JellyflowPanel {
    pub fn new(trial: JellyflowTrial, cx: &mut App) -> Self {
        let JellyflowTrial {
            runtime,
            hosts,
            views,
            probes,
        } = trial;
        Self {
            runtime,
            hosts: hosts.map(|host| cx.new(|_| host)),
            views,
            probes,
            publications: std::array::from_fn(|_| PrepaintPublicationId::new()),
            command_error: None,
        }
    }

    pub fn observe(&self, window: &Window, cx: &App) -> Result<Option<Observation>> {
        ensure!(
            self.command_error.is_none(),
            "Jellyflow command failed: {:?}",
            self.command_error
        );
        for host in &self.hosts {
            let host = host.read(cx);
            ensure!(
                host.last_diagnostic().is_none(),
                "Jellyflow host diagnostic: {:?}",
                host.last_diagnostic()
            );
        }
        let frames = self
            .probes
            .each_ref()
            .map(|probe| probe.accepted.borrow().clone());
        let [Some(frame_a), Some(frame_b)] = frames else {
            return Ok(None);
        };
        let graph = self.runtime.snapshot()?;
        if !frame_a.graph.ptr_eq(&graph) || !frame_b.graph.ptr_eq(&graph) {
            return Ok(None);
        }
        for (frame, view) in [&frame_a, &frame_b].into_iter().zip(&self.views) {
            if frame.transform != view.viewport_transform()?
                || frame.selection != view.selection()?.nodes()
            {
                return Ok(None);
            }
        }
        Ok(Some(Observation {
            frames: [frame_a, frame_b],
            graph,
            history: self.runtime.history_state()?,
            geometry: [
                self.views[0].geometry_snapshot()?,
                self.views[1].geometry_snapshot()?,
            ],
            captured: window.captured_pointer().is_some(),
            window_active: window.is_window_active(),
        }))
    }

    pub fn views_closed(&self) -> bool {
        self.views.iter().all(|view| !view.is_open())
    }

    pub fn close(&self, window: &mut Window, cx: &mut App) -> Result<()> {
        for host in &self.hosts {
            if host.read(cx).close_reason().is_none() {
                let _ = host.update(cx, |host, cx| host.close_in_window(window, cx))?;
            }
        }
        Ok(())
    }

    pub fn verify_retired(&self, cx: &App) -> Result<()> {
        ensure!(
            self.views_closed(),
            "a Jellyflow ViewSession survived normal Window close"
        );
        for host in &self.hosts {
            let host = host.read(cx);
            let resources = host.resource_snapshot();
            ensure!(
                host.close_reason().is_some()
                    && resources.active_nodes() == 0
                    && resources.parked_nodes() == 0
                    && resources.strong_instances() == 0
                    && resources.protected_leases() == 0
                    && !resources.interaction_submission_pending(),
                "Jellyflow retained native resources after close: {resources:?}"
            );
        }
        Ok(())
    }

    pub fn history(&self) -> Result<HistoryState> {
        Ok(self.runtime.history_state()?)
    }
}

impl Render for JellyflowPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panels = (0..2).map(|index| {
            let undo = div()
                .id(("jellyflow-undo", index))
                .w(px(80.))
                .h(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(0x315878))
                .cursor_pointer()
                .child("Undo")
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.runtime.snapshot().map(|snapshot| {
                        this.runtime.execute(GraphCommand::undo(snapshot.version()))
                    });
                    this.command_error = match result {
                        Ok(completion)
                            if matches!(
                                completion.kind(),
                                CommandExecutionKind::Applied | CommandExecutionKind::Noop
                            ) =>
                        {
                            None
                        }
                        Ok(completion) => completion.diagnostic_message(),
                        Err(error) => Some(error.to_string()),
                    };
                    cx.notify();
                }));
            let contents = div()
                .size_full()
                .flex()
                .flex_col()
                .bg(rgb(0x17212e))
                .child(
                    div()
                        .h(px(36.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .child(format!("Jellyflow View {}", index + 1))
                        .child(ProbeElement::control(&self.probes[index], Hit::Undo, undo)),
                )
                .child(ProbeElement::control(
                    &self.probes[index],
                    Hit::Canvas,
                    div()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .child(self.hosts[index].clone()),
                ));
            div()
                .absolute()
                .left(px(16. + index as f32 * 632.))
                .top(px(76.))
                .w(px(600.))
                .h(px(548.))
                .child(ProbeElement {
                    child: contents.into_any_element(),
                    probe: self.probes[index].clone(),
                    hit: None,
                    taps: None,
                    publication: Some((self.publications[index], self.views[index].clone())),
                })
        });
        div()
            .size_full()
            .relative()
            .bg(rgb(0x17212e))
            .font_family("Segoe UI")
            .text_size(px(14.))
            .text_color(rgb(0xf0f3f8))
            .child(
                div().absolute().left(px(16.)).top(px(12.)).child(
                    "Nara hosted renderer · standalone Jellyflow graph · two independent views",
                ),
            )
            .child(div().absolute().left(px(16.)).top(px(40.)).child(
                "Drag a node title to move it. Local label is a button. Middle-drag pans; wheel zooms.",
            ))
            .children(panels)
    }
}
