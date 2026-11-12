use super::{
    Viewer,
    jellyflow_panel::{AcceptedFrame, Hit, Observation, input, output, sink, source},
};
use anyhow::{Context as _, Result, ensure};
use jellyflow_core::CanvasPoint;
use jellyflow_runtime::{
    graph::{GraphSnapshot, HistoryState},
    viewport::ViewportTransform,
};
use nara_engine::scene::SceneAuthoringRevision;
use open_gpui::{
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, Point,
    ScrollDelta, ScrollEvent, TouchPhase, point, px,
};

#[derive(Clone, Copy, Debug)]
enum Phase {
    Initial,
    FirstFrame,
    OverlayIsolation,
    NativeControl,
    Capture,
    Move(u8),
    Commit,
    Undo,
    Pan,
    PanMove,
    PanRelease,
    Zoom,
    ZoomCheck,
    Done,
}

pub(super) struct JellyflowSmoke {
    phase: Phase,
    submitted: u64,
    original: Option<GraphSnapshot>,
    history: HistoryState,
    transforms: Option<[ViewportTransform; 2]>,
    grips: Option<[Point<Pixels>; 2]>,
    anchor: Point<Pixels>,
    pointer: Point<Pixels>,
    panned: Option<ViewportTransform>,
    hidden_scene: Option<(SceneAuthoringRevision, f64)>,
}

impl JellyflowSmoke {
    pub fn new() -> Self {
        Self {
            phase: Phase::Initial,
            submitted: 0,
            original: None,
            history: HistoryState::default(),
            transforms: None,
            grips: None,
            anchor: point(px(0.), px(0.)),
            pointer: point(px(0.), px(0.)),
            panned: None,
            hidden_scene: None,
        }
    }

    pub fn advance(&mut self, viewer: &mut Viewer) -> Result<bool> {
        if matches!(self.phase, Phase::Done) {
            return Ok(true);
        }
        let renderer = viewer.renderer.as_ref().unwrap().clone();
        let surface = viewer.surface.as_ref().unwrap().clone();
        if viewer.native_geometry_events == 0
            || renderer.pending()
            || surface.needs_frame()
            || renderer.submissions.get() <= self.submitted
        {
            return Ok(false);
        }
        let observed = viewer.application.as_ref().unwrap().update(|cx| {
            viewer.root.unwrap().update(cx, |root, window, cx| {
                root.jellyflow
                    .as_ref()
                    .context("Jellyflow root was not mounted")?
                    .read(cx)
                    .observe(window, cx)
            })
        })??;
        let Some(observed) = observed else {
            return Ok(false);
        };
        let frames = &observed.frames;
        for frame in frames {
            ensure!(
                frame.generation > 0 && frame.transform.is_valid(),
                "Jellyflow has no accepted finite Window publication"
            );
        }
        println!(
            "NARA_GPUI_JELLYFLOW_PHASE phase={:?} submitted={} accepted={:?}",
            self.phase,
            renderer.submissions.get(),
            frames.each_ref().map(|frame| frame.generation)
        );
        match self.phase {
            Phase::Initial => {
                let graph = observed.graph.graph();
                ensure!(
                    graph.nodes().len() == 2
                        && graph.ports().len() == 2
                        && graph.edges().len() == 1,
                    "the finite source/sink graph was not admitted"
                );
                for (node, expected) in [
                    (source(), CanvasPoint { x: 40., y: 64. }),
                    (sink(), CanvasPoint { x: 328., y: 240. }),
                ] {
                    let node = graph.node(&node).context("missing trial node")?;
                    ensure!(
                        node.pos == expected
                            && node.pos.is_finite()
                            && node.size.is_some_and(|size| {
                                size.width.is_finite()
                                    && size.height.is_finite()
                                    && size.width > 0.
                                    && size.height > 0.
                            }),
                        "trial node has invalid source geometry"
                    );
                }
                for frame in frames {
                    for hit in [
                        Hit::Grip(source()),
                        Hit::Grip(sink()),
                        Hit::Port(output()),
                        Hit::Port(input()),
                        Hit::NativeControl(source()),
                        Hit::NativeControl(sink()),
                        Hit::Undo,
                        Hit::Canvas,
                    ] {
                        frame.bounds(hit)?;
                    }
                    ensure!(
                        frame.taps.get(&source()) == Some(&0),
                        "native label did not mount"
                    );
                }
                for geometry in &observed.geometry {
                    ensure!(
                        geometry.nodes().len() == 2 && geometry.ports().len() == 2,
                        "Jellyflow has not accepted both real node and port layouts"
                    );
                    for node in [source(), sink()] {
                        let node = geometry
                            .node(node)
                            .context("node layout was not accepted")?;
                        ensure!(
                            node.is_fresh() && node.bounds().is_positive_finite(),
                            "Jellyflow accepted invalid or stale node geometry"
                        );
                    }
                    for port in [output(), input()] {
                        let port = geometry
                            .port(port)
                            .context("real port layout was not accepted")?;
                        ensure!(
                            port.visual_bounds().is_positive_finite()
                                && port.route_anchor().is_finite(),
                            "Jellyflow accepted non-finite real port geometry"
                        );
                    }
                }
                history_is(observed.history, 0, 0)?;
                self.original = Some(observed.graph.clone());
                self.history = observed.history;
                self.transforms = Some(frames.each_ref().map(|frame| frame.transform));
                self.grips = Some([
                    frames[0].center(Hit::Grip(source()))?,
                    frames[1].center(Hit::Grip(source()))?,
                ]);
                renderer.diagnostic_next.set(true);
                viewer.request()?;
                self.phase = Phase::FirstFrame;
            }
            Phase::FirstFrame => {
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                for frame in frames {
                    let bounds = frame.bounds(Hit::NativeControl(source()))?;
                    let sample = bounds.origin + point(px(5.), px(5.));
                    let pixel = diagnostic.logical_pixel(f32::from(sample.x), f32::from(sample.y));
                    ensure!(
                        pixel[2] > 100 && pixel[1] > 60 && pixel[2] > pixel[0],
                        "accepted native Jellyflow control has no matching GPU pixels: {pixel:?}"
                    );
                }
                ensure!(
                    renderer.diagnostics.get() == 1 && viewer.factory.creates.get() == 0,
                    "Jellyflow bypassed the original Nara renderer"
                );
                println!(
                    "NARA_GPUI_JELLYFLOW_FIRST_FRAME nodes=2 ports=2 edges=1 views=2 accepted_window_publication=true same_host_gpu_pixels=true"
                );
                {
                    let engine = viewer.engine.as_ref().unwrap().borrow();
                    self.hidden_scene = Some((engine.revision()?, engine.sprite_x()?));
                }
                viewer.click(100., 30.)?;
                ensure!(
                    viewer.commands.borrow().is_empty(),
                    "the Jellyflow overlay admitted a hidden Scene command"
                );
                viewer.request()?;
                self.phase = Phase::OverlayIsolation;
            }
            Phase::OverlayIsolation => {
                self.unchanged(&observed)?;
                let engine = viewer.engine.as_ref().unwrap().borrow();
                ensure!(
                    self.hidden_scene == Some((engine.revision()?, engine.sprite_x()?))
                        && viewer.commands.borrow().is_empty(),
                    "clicking the Jellyflow header changed the hidden Nara Scene"
                );
                drop(engine);
                println!(
                    "NARA_GPUI_JELLYFLOW_OVERLAY hidden_scene_unchanged=true hidden_commands=0"
                );
                click(viewer, &frames[0], Hit::NativeControl(source()))?;
                self.phase = Phase::NativeControl;
            }
            Phase::NativeControl => {
                self.unchanged(&observed)?;
                ensure!(
                    frames[0].taps.get(&source()) == Some(&1)
                        && frames[1].taps.get(&source()) == Some(&0),
                    "the ordinary native label control did not preserve per-view state"
                );
                println!(
                    "NARA_GPUI_JELLYFLOW_NATIVE_CONTROL accepted_taps=1 other_view_taps=0 domain_history_unchanged=true"
                );
                self.anchor = frames[0].center(Hit::Grip(source()))?;
                self.pointer = self.anchor;
                mouse(viewer, self.pointer, MouseButton::Left, Pointer::Down)?;
                self.phase = Phase::Capture;
            }
            Phase::Capture => {
                self.unchanged(&observed)?;
                ensure!(
                    observed.captured
                        && frames[0].selection.is_empty()
                        && frames[1].selection.is_empty(),
                    "node grip failed to acquire real Window capture and local selection: active={}, captured={}, selected={:?}, other={:?}",
                    observed.window_active,
                    observed.captured,
                    frames[0].selection,
                    frames[1].selection,
                );
                println!(
                    "NARA_GPUI_JELLYFLOW_CAPTURE window_capture=true selection_unchanged_on_press=true other_view_selected=false"
                );
                self.pointer = self.anchor + point(px(40.), px(20.));
                mouse(viewer, self.pointer, MouseButton::Left, Pointer::Move)?;
                self.phase = Phase::Move(1);
            }
            Phase::Move(sample) => {
                self.unchanged(&observed)?;
                ensure!(observed.captured, "native node drag lost real capture");
                near(frames[0].center(Hit::Grip(source()))?, self.pointer)?;
                ensure!(
                    frames[0].transform == self.transforms.unwrap()[0]
                        && frames[1].transform == self.transforms.unwrap()[1],
                    "node drag changed a view transform"
                );
                if sample < 3 {
                    self.pointer = self.anchor
                        + point(
                            px(40. + f32::from(sample) * 20.),
                            px(20. + f32::from(sample) * 20.),
                        );
                    mouse(viewer, self.pointer, MouseButton::Left, Pointer::Move)?;
                    self.phase = Phase::Move(sample + 1);
                } else {
                    mouse(viewer, self.pointer, MouseButton::Left, Pointer::Up)?;
                    self.phase = Phase::Commit;
                }
            }
            Phase::Commit => {
                ensure!(!observed.captured, "node release retained Window capture");
                if observed
                    .graph
                    .ptr_eq(self.original.as_ref().expect("initial graph"))
                    && observed.history == self.history
                {
                    self.submitted = renderer.submissions.get();
                    viewer.request()?;
                    return Ok(false);
                }
                ensure!(
                    frames[0].selection == [source()] && frames[1].selection.is_empty(),
                    "completed drag did not publish selection only in its originating view"
                );
                history_is(observed.history, self.history.undo_len() + 1, 0)?;
                let original = self.original.as_ref().unwrap();
                let before = original
                    .graph()
                    .node(&source())
                    .context("original source missing")?;
                let moved = observed
                    .graph
                    .graph()
                    .node(&source())
                    .context("moved source missing")?;
                let delta = self.pointer - self.anchor;
                let zoom = self.transforms.unwrap()[0].zoom;
                let expected = CanvasPoint {
                    x: before.pos.x + f32::from(delta.x) / zoom,
                    y: before.pos.y + f32::from(delta.y) / zoom,
                };
                ensure!(
                    moved.pos == expected && observed.graph.provenance() == original.provenance(),
                    "drag did not commit the original source node position"
                );
                let mut restored = moved.clone();
                restored.pos = before.pos;
                ensure!(
                    serde_json::to_value(&restored)? == serde_json::to_value(before)?,
                    "node drag changed non-geometric node content"
                );
                unchanged_ports_and_sink(&observed.graph, original)?;
                near(frames[0].center(Hit::Grip(source()))?, self.pointer)?;
                near(
                    frames[1].center(Hit::Grip(source()))?,
                    self.grips.unwrap()[1] + delta,
                )?;
                println!(
                    "NARA_GPUI_JELLYFLOW_DRAG samples=3 history_delta=1 capture_released=true shared_runtime_views=2"
                );
                click(viewer, &frames[0], Hit::Undo)?;
                self.phase = Phase::Undo;
            }
            Phase::Undo => {
                if observed.history.undo_len() == self.history.undo_len() + 1
                    && observed.history.redo_len() == 0
                {
                    self.submitted = renderer.submissions.get();
                    viewer.request()?;
                    return Ok(false);
                }
                history_is(observed.history, self.history.undo_len(), 1)?;
                same_graph(&observed.graph, self.original.as_ref().unwrap())?;
                for (frame, grip) in frames.iter().zip(self.grips.unwrap()) {
                    near(frame.center(Hit::Grip(source()))?, grip)?;
                }
                println!(
                    "NARA_GPUI_JELLYFLOW_UNDO native_button=true original_graph_restored=true undo=0 redo=1"
                );
                self.history = observed.history;
                let canvas = frames[0].bounds(Hit::Canvas)?;
                self.anchor = canvas.origin
                    + point(canvas.size.width - px(64.), canvas.size.height - px(48.));
                self.pointer = self.anchor;
                mouse(viewer, self.pointer, MouseButton::Middle, Pointer::Down)?;
                self.phase = Phase::Pan;
            }
            Phase::Pan => {
                self.unchanged(&observed)?;
                ensure!(
                    observed.captured,
                    "middle pan did not acquire Window capture"
                );
                self.pointer = self.anchor + point(px(-32.), px(-24.));
                mouse(viewer, self.pointer, MouseButton::Middle, Pointer::Move)?;
                self.phase = Phase::PanMove;
            }
            Phase::PanMove => {
                self.unchanged(&observed)?;
                ensure!(
                    observed.captured
                        && frames[0].transform != self.transforms.unwrap()[0]
                        && frames[1].transform == self.transforms.unwrap()[1],
                    "pan did not remain local to its originating ViewSession"
                );
                mouse(viewer, self.pointer, MouseButton::Middle, Pointer::Up)?;
                self.phase = Phase::PanRelease;
            }
            Phase::PanRelease => {
                self.unchanged(&observed)?;
                ensure!(!observed.captured, "middle pan retained Window capture");
                self.panned = Some(frames[0].transform);
                near(
                    frames[0].center(Hit::Grip(source()))?,
                    self.grips.unwrap()[0] + (self.pointer - self.anchor),
                )?;
                near(
                    frames[1].center(Hit::Grip(source()))?,
                    self.grips.unwrap()[1],
                )?;
                let canvas = frames[1].bounds(Hit::Canvas)?;
                let at = canvas.origin
                    + point(canvas.size.width - px(64.), canvas.size.height - px(48.));
                surface.dispatch_input(PlatformInput::MouseMove(MouseMoveEvent {
                    position: at,
                    ..Default::default()
                }))?;
                surface.dispatch_input(PlatformInput::Scroll(ScrollEvent {
                    position: at,
                    delta: ScrollDelta::Pixels(point(px(0.), px(24.))),
                    touch_phase: TouchPhase::Moved,
                    ..Default::default()
                }))?;
                self.phase = Phase::Zoom;
            }
            Phase::Zoom => {
                self.unchanged(&observed)?;
                ensure!(
                    frames[0].transform == self.panned.unwrap()
                        && frames[1].transform.zoom != self.transforms.unwrap()[1].zoom,
                    "wheel zoom did not remain local to the second ViewSession"
                );
                viewer.request_frame(false)?;
                self.phase = Phase::ZoomCheck;
            }
            Phase::ZoomCheck => {
                self.unchanged(&observed)?;
                ensure!(
                    !observed.captured
                        && frames[0].transform == self.panned.unwrap()
                        && frames[1].transform.zoom != self.transforms.unwrap()[1].zoom,
                    "accepted navigation did not preserve independent local transforms"
                );
                println!(
                    "NARA_GPUI_JELLYFLOW_VIEWS pan_view=0 zoom_view=1 independent_transforms=true domain_history_unchanged=true accepted_window_geometry=true"
                );
                viewer.expected_jellyflow_history = Some(observed.history);
                self.phase = Phase::Done;
                return Ok(true);
            }
            Phase::Done => return Ok(true),
        }
        self.submitted = renderer.submissions.get();
        viewer.request_frame(false)?;
        Ok(false)
    }

    fn unchanged(&self, observed: &Observation) -> Result<()> {
        same_graph(&observed.graph, self.original.as_ref().unwrap())?;
        ensure!(
            observed.history == self.history,
            "Jellyflow preview/navigation changed history"
        );
        Ok(())
    }
}

fn graph_content(snapshot: &GraphSnapshot) -> Result<serde_json::Value> {
    let graph = snapshot.graph();
    Ok(serde_json::to_value((
        graph.nodes().into_iter().collect::<Vec<_>>(),
        graph.ports().into_iter().collect::<Vec<_>>(),
        graph.edges().into_iter().collect::<Vec<_>>(),
    ))?)
}

fn same_graph(actual: &GraphSnapshot, expected: &GraphSnapshot) -> Result<()> {
    ensure!(
        actual.provenance() == expected.provenance()
            && graph_content(actual)? == graph_content(expected)?,
        "Jellyflow graph content differs from the original finite source"
    );
    Ok(())
}

fn unchanged_ports_and_sink(actual: &GraphSnapshot, expected: &GraphSnapshot) -> Result<()> {
    ensure!(
        serde_json::to_value(actual.graph().node(&sink()))?
            == serde_json::to_value(expected.graph().node(&sink()))?
            && serde_json::to_value(actual.graph().ports().into_iter().collect::<Vec<_>>())?
                == serde_json::to_value(expected.graph().ports().into_iter().collect::<Vec<_>>())?
            && serde_json::to_value(actual.graph().edges().into_iter().collect::<Vec<_>>())?
                == serde_json::to_value(expected.graph().edges().into_iter().collect::<Vec<_>>())?,
        "drag changed the sink, real ports, or graph edges"
    );
    Ok(())
}

fn history_is(history: HistoryState, undo: usize, redo: usize) -> Result<()> {
    ensure!(
        history.undo_len() == undo && history.redo_len() == redo,
        "unexpected Jellyflow history: {history:?}; expected undo={undo} redo={redo}"
    );
    Ok(())
}

fn near(actual: Point<Pixels>, expected: Point<Pixels>) -> Result<()> {
    ensure!(
        f32::from(actual.x - expected.x).abs() <= 1.
            && f32::from(actual.y - expected.y).abs() <= 1.,
        "accepted Jellyflow geometry differs: actual={actual:?} expected={expected:?}"
    );
    Ok(())
}

fn click(viewer: &Viewer, frame: &AcceptedFrame, hit: Hit) -> Result<()> {
    let at = frame.center(hit)?;
    viewer.click(f32::from(at.x), f32::from(at.y))
}

enum Pointer {
    Down,
    Move,
    Up,
}

fn mouse(
    viewer: &Viewer,
    position: Point<Pixels>,
    button: MouseButton,
    phase: Pointer,
) -> Result<()> {
    if matches!(phase, Pointer::Down) {
        viewer
            .surface
            .as_ref()
            .unwrap()
            .dispatch_input(PlatformInput::MouseMove(MouseMoveEvent {
                position,
                ..Default::default()
            }))?;
    }
    let input = match phase {
        Pointer::Down => PlatformInput::MouseDown(MouseDownEvent {
            position,
            button,
            click_count: 1,
            ..Default::default()
        }),
        Pointer::Move => PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: Some(button),
            ..Default::default()
        }),
        Pointer::Up => PlatformInput::MouseUp(MouseUpEvent {
            position,
            button,
            click_count: 1,
            ..Default::default()
        }),
    };
    viewer.surface.as_ref().unwrap().dispatch_input(input)?;
    Ok(())
}
