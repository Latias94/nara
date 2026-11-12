//! Native gestures qualified against original submitted geometry and domain-owned history.

use super::{
    Viewer,
    graph_panel::{GraphHit, GraphSnapshot},
};
use anyhow::{Context as _, Result, ensure};
use nara_core::DocumentHistoryStatus;
use nara_gpui_engine_consumer::graph::{GraphView, Link, NodeId, NodeKind};
use open_gpui::{
    Bounds, DevicePixels, HostedSurfaceGeometry, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, Point, ScrollDelta, ScrollEvent,
    TouchPhase, point, px, size,
};
use winit::{
    event::ElementState,
    keyboard::{Key, NamedKey},
};

type Transform = ([f32; 2], f32);

#[derive(Clone, Copy, Debug)]
enum Phase {
    Initial,
    Scaled,
    Drag(u8),
    Commit,
    Undo,
    Redo,
    CaptureStart,
    Capture(u8),
    CaptureCommit,
    CaptureUndo,
    CancelStart(u8),
    CancelMove(u8),
    CancelFinish(u8),
    CancelCheck(u8),
    RevisionUndo,
    Port(u8),
    Wheel,
    WheelCheck,
    Zoom(u8),
    PanStart,
    Pan(u8),
    RebaseStart,
    Rebase(u8),
    PixelsStart,
    Pixels,
    ClosePreview,
    Close,
    Done,
}

struct Panel {
    canvas: Option<Bounds<Pixels>>,
    header: Option<Point<Pixels>>,
    port: Option<Point<Pixels>>,
    probe: Option<Point<Pixels>>,
    transform: Transform,
    active: bool,
    selected: Option<NodeId>,
    links: usize,
    current: bool,
}

struct Checkpoint {
    snapshot: GraphSnapshot,
    history: DocumentHistoryStatus,
    transforms: [Transform; 2],
}

pub struct GraphGesturesSmoke {
    phase: Phase,
    submitted: u64,
    wait_submission: bool,
    saved: Option<Checkpoint>,
    moved: Option<GraphView>,
    anchor: [f32; 2],
    pointer: [f32; 2],
    before: Transform,
    home: Transform,
    probe: [f32; 2],
    wheel_header: [f32; 2],
}

impl GraphGesturesSmoke {
    pub fn new() -> Self {
        Self {
            phase: Phase::Initial,
            submitted: 0,
            wait_submission: false,
            saved: None,
            moved: None,
            anchor: [0.; 2],
            pointer: [0.; 2],
            before: ([0.; 2], 1.),
            home: ([0.; 2], 1.),
            probe: [0.; 2],
            wheel_header: [0.; 2],
        }
    }

    pub fn advance(&mut self, viewer: &mut Viewer) -> Result<bool> {
        if viewer.native_geometry_events == 0 {
            return Ok(false);
        }
        if matches!(self.phase, Phase::Done) {
            return Ok(true);
        }
        let renderer = viewer.renderer.as_ref().unwrap().clone();
        let surface = viewer.surface.as_ref().unwrap().clone();
        if renderer.pending()
            || surface.needs_frame()
            || self.wait_submission && renderer.submissions.get() <= self.submitted
        {
            return Ok(false);
        }
        let (snapshot, history) = {
            let engine = viewer.engine.as_ref().unwrap().borrow();
            (engine.graph_snapshot()?, engine.graph_history()?)
        };
        let panels = observe(viewer, &snapshot)?;
        if !panels.iter().all(|panel| panel.current) {
            return Ok(false);
        }
        for panel in &panels {
            near(panel.transform.0, panel.transform.0)?;
            ensure!(
                (0.5..=2.).contains(&panel.transform.1),
                "non-finite or unbounded graph zoom"
            );
        }
        if let Some(saved) = &self.saved {
            ensure!(
                panels[1].transform == saved.transforms[1],
                "panel 0 navigation changed panel 1"
            );
        }
        println!(
            "NARA_GPUI_GRAPH_GESTURES_PHASE phase={:?} submitted={}",
            self.phase,
            renderer.submissions.get()
        );
        self.submitted = renderer.submissions.get();
        self.wait_submission = true;
        let panel = &panels[0];
        self.phase = match self.phase {
            Phase::Initial => {
                let seeds = [
                    (NodeKind::Scalar(2.), [24., 24.]),
                    (NodeKind::Scalar(3.), [24., 144.]),
                    (NodeKind::Add, [208., 24.]),
                    (NodeKind::Output, [392., 24.]),
                ];
                ensure!(
                    snapshot.view.nodes.len() == 4
                        && seeds
                            .iter()
                            .enumerate()
                            .all(|(i, (kind, position))| snapshot
                                .view
                                .node(NodeId(i as u32 + 1))
                                .is_some_and(|n| &n.kind == kind && &n.position == position)),
                    "gesture mode did not receive the original four seeded nodes"
                );
                ensure!(
                    snapshot.view.links
                        == [
                            Link {
                                from: NodeId(1),
                                to: NodeId(3),
                                input: 0
                            },
                            Link {
                                from: NodeId(2),
                                to: NodeId(3),
                                input: 1
                            },
                            Link {
                                from: NodeId(3),
                                to: NodeId(4),
                                input: 0
                            }
                        ],
                    "gesture mode did not receive the original three links"
                );
                history_is(history, 7, 0)?;
                self.saved = Some(checkpoint(&snapshot, history, &panels));
                let physical = viewer.engine.as_ref().unwrap().borrow().window.inner_size();
                let geometry = HostedSurfaceGeometry::from_physical_size(
                    size(
                        DevicePixels(physical.width.try_into()?),
                        DevicePixels(physical.height.try_into()?),
                    ),
                    1.25,
                )?;
                surface.set_geometry(geometry.size(), geometry.scale_factor())?;
                Phase::Scaled
            }
            Phase::Scaled => {
                self.unchanged(&snapshot, history)?;
                ensure!(
                    surface.geometry().scale_factor() == 1.25,
                    "fractional logical scale was not applied"
                );
                self.begin(
                    viewer,
                    checkpoint(&snapshot, history, &panels),
                    xy(panel.header.context("node 1 header is not submitted")?),
                    MouseButton::Left,
                )?;
                Phase::Drag(0)
            }
            Phase::Drag(sample) => {
                self.preview(&snapshot, history, panel)?;
                if sample > 0 {
                    near_painted(
                        xy(panel.header.context("node preview header disappeared")?),
                        self.pointer,
                        surface.geometry().scale_factor(),
                    )?;
                }
                if sample < 3 {
                    self.pointer = add(
                        self.anchor,
                        [10. * f32::from(sample + 1), 6. * f32::from(sample + 1)],
                    );
                    mouse(viewer, self.pointer, MouseButton::Left, Pointer::Move)?;
                    Phase::Drag(sample + 1)
                } else {
                    mouse(viewer, self.pointer, MouseButton::Left, Pointer::Up)?;
                    Phase::Commit
                }
            }
            Phase::Commit => {
                self.one_move(&snapshot, history, panel)?;
                self.moved = Some(snapshot.view.clone());
                click(viewer, GraphHit::Undo)?;
                Phase::Undo
            }
            Phase::Undo => {
                self.restored(&snapshot, history)?;
                click(viewer, GraphHit::Redo)?;
                Phase::Redo
            }
            Phase::Redo => {
                ensure!(
                    Some(&snapshot.view) == self.moved.as_ref(),
                    "native Redo did not restore exact moved content"
                );
                history_is(history, self.saved().history.undo_depth + 1, 0)?;
                ensure!(
                    snapshot.revision == self.saved().snapshot.revision.next().next().next(),
                    "Undo/Redo bypassed original revisions"
                );
                println!(
                    "NARA_GPUI_GRAPH_GESTURES_MOVE one_history=true previews=3 undo_redo=true scale=1.25"
                );
                click(viewer, GraphHit::ZoomIn)?;
                Phase::CaptureStart
            }
            Phase::CaptureStart => {
                ensure!(
                    Some(&snapshot.view) == self.moved.as_ref()
                        && snapshot.revision == self.saved().snapshot.revision.next().next().next(),
                    "zoom mutated graph source"
                );
                history_is(history, self.saved().history.undo_depth + 1, 0)?;
                zoom_is(panel.transform.1, 1.25)?;
                self.begin(
                    viewer,
                    checkpoint(&snapshot, history, &panels),
                    xy(panel
                        .header
                        .context("zoomed node header is not submitted")?),
                    MouseButton::Left,
                )?;
                Phase::Capture(0)
            }
            Phase::Capture(sample) => {
                self.preview(&snapshot, history, panel)?;
                match sample {
                    0 | 2 => {
                        let bounds = panel.canvas.context("graph canvas is not submitted")?;
                        self.pointer = [
                            f32::from(bounds.origin.x + bounds.size.width)
                                + 80.
                                + if sample == 2 { 12. } else { 0. },
                            self.anchor[1] + 12.,
                        ];
                        mouse(viewer, self.pointer, MouseButton::Left, Pointer::Move)?;
                    }
                    1 => {
                        mouse(viewer, self.pointer, MouseButton::Right, Pointer::Up)?;
                        self.wait_submission = false;
                    }
                    3 => {
                        mouse(viewer, self.pointer, MouseButton::Left, Pointer::Up)?;
                        return self.next(Phase::CaptureCommit);
                    }
                    _ => unreachable!(),
                }
                Phase::Capture(sample + 1)
            }
            Phase::CaptureCommit => {
                self.one_move(&snapshot, history, panel)?;
                println!(
                    "NARA_GPUI_GRAPH_GESTURES_MOVE one_history=true outside_capture=true wrong_button_preserved=true zoom=1.25"
                );
                click(viewer, GraphHit::Undo)?;
                Phase::CaptureUndo
            }
            Phase::CaptureUndo => {
                self.restored(&snapshot, history)?;
                self.wait_submission = false;
                Phase::CancelStart(0)
            }
            Phase::CancelStart(kind) => {
                self.begin(
                    viewer,
                    checkpoint(&snapshot, history, &panels),
                    xy(panel
                        .header
                        .context("cancel node header is not submitted")?),
                    MouseButton::Left,
                )?;
                Phase::CancelMove(kind)
            }
            Phase::CancelMove(kind) => {
                self.preview(&snapshot, history, panel)?;
                self.pointer = add(self.anchor, [20., 12.]);
                mouse(viewer, self.pointer, MouseButton::Left, Pointer::Move)?;
                Phase::CancelFinish(kind)
            }
            Phase::CancelFinish(kind) => {
                self.preview(&snapshot, history, panel)?;
                near_painted(
                    xy(panel.header.context("cancel preview header disappeared")?),
                    self.pointer,
                    surface.geometry().scale_factor(),
                )?;
                match kind {
                    0 => escape(viewer)?,
                    1 => surface.set_active(false)?,
                    2 => viewer
                        .engine
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .increment_graph()?,
                    _ => unreachable!(),
                }
                Phase::CancelCheck(kind)
            }
            Phase::CancelCheck(kind) => {
                ensure!(
                    !panel.active,
                    "Escape, focus loss, or revision change retained node capture"
                );
                near(
                    xy(panel.header.context("cancelled header disappeared")?),
                    self.anchor,
                )?;
                if kind == 2 {
                    let mut expected = self.saved().snapshot.view.clone();
                    let node = expected
                        .nodes
                        .iter_mut()
                        .find(|node| node.id == NodeId(1))
                        .context("seeded scalar disappeared")?;
                    let NodeKind::Scalar(value) = &mut node.kind else {
                        anyhow::bail!("seeded scalar changed kind")
                    };
                    *value += 1.;
                    ensure!(
                        snapshot.view == expected
                            && snapshot.revision == self.saved().snapshot.revision.next(),
                        "revision cancellation committed or lost a node preview"
                    );
                    history_is(history, self.saved().history.undo_depth + 1, 0)?;
                    println!(
                        "NARA_GPUI_GRAPH_GESTURES_CANCEL revision=true focus=true escape=true external_domain_control=true"
                    );
                    click(viewer, GraphHit::Undo)?;
                    Phase::RevisionUndo
                } else {
                    self.unchanged(&snapshot, history)?;
                    if kind == 1 {
                        surface.set_active(true)?;
                    }
                    self.wait_submission = false;
                    Phase::CancelStart(kind + 1)
                }
            }
            Phase::RevisionUndo => {
                self.restored(&snapshot, history)?;
                self.wait_submission = false;
                Phase::Port(0)
            }
            Phase::Port(sample) => {
                if sample == 0 {
                    self.begin(
                        viewer,
                        checkpoint(&snapshot, history, &panels),
                        xy(panel.port.context("output port is not submitted")?),
                        MouseButton::Left,
                    )?;
                } else {
                    self.unchanged(&snapshot, history)?;
                    ensure!(!panel.active, "a port began a node gesture");
                    if sample == 1 {
                        self.pointer = add(self.anchor, [7., 4.]);
                        mouse(viewer, self.pointer, MouseButton::Left, Pointer::Move)?;
                    } else if sample == 2 {
                        mouse(viewer, self.pointer, MouseButton::Left, Pointer::Up)?;
                    } else {
                        escape(viewer)?;
                        return self.next(Phase::Wheel);
                    }
                }
                self.wait_submission = false;
                Phase::Port(sample + 1)
            }
            Phase::Wheel => {
                self.saved = Some(checkpoint(&snapshot, history, &panels));
                self.wheel_header = xy(panel
                    .header
                    .context("the pre-wheel header is not submitted")?);
                self.pointer = canvas_point(panel)?;
                scroll(
                    viewer,
                    self.pointer,
                    ScrollDelta::Lines(point(1., -1.)),
                    false,
                )?;
                Phase::WheelCheck
            }
            Phase::WheelCheck => {
                self.unchanged(&snapshot, history)?;
                near_painted(
                    xy(panel
                        .header
                        .context("the post-wheel header is not submitted")?),
                    add(self.wheel_header, [32., -32.]),
                    surface.geometry().scale_factor(),
                )?;
                near(
                    panel.transform.0,
                    add(self.saved().transforms[0].0, [32., -32.]),
                )?;
                ensure!(
                    panel.transform.1 == self.saved().transforms[0].1,
                    "normal wheel changed zoom"
                );
                self.home = panel.transform;
                self.before = panel.transform;
                scroll(
                    viewer,
                    self.pointer,
                    ScrollDelta::Pixels(point(px(0.), px(120.))),
                    true,
                )?;
                Phase::Zoom(0)
            }
            Phase::Zoom(sample) => {
                self.unchanged(&snapshot, history)?;
                let bounds = panel.canvas.context("zoom canvas is not submitted")?;
                let local = sub(self.pointer, xy(bounds.origin));
                let world = sub(local, self.before.0).map(|v| v / self.before.1);
                near(
                    add(panel.transform.0, world.map(|v| v * panel.transform.1)),
                    local,
                )?;
                let expected = match sample {
                    0 => self.home.1 * 0.18_f32.exp(),
                    1 => 2.,
                    2 => 0.5,
                    _ => self.home.1,
                };
                zoom_is(panel.transform.1, expected)?;
                self.before = panel.transform;
                if sample < 3 {
                    let delta = match sample {
                        0 => 10_000.,
                        1 => -10_000.,
                        _ => (self.home.1 / 0.5).ln() / 0.0015,
                    };
                    scroll(
                        viewer,
                        self.pointer,
                        ScrollDelta::Pixels(point(px(0.), px(delta))),
                        true,
                    )?;
                    Phase::Zoom(sample + 1)
                } else {
                    near(panel.transform.0, self.home.0)?;
                    self.wait_submission = false;
                    Phase::PanStart
                }
            }
            Phase::PanStart | Phase::RebaseStart => {
                self.begin(
                    viewer,
                    checkpoint(&snapshot, history, &panels),
                    canvas_point(panel)?,
                    MouseButton::Middle,
                )?;
                if matches!(self.phase, Phase::PanStart) {
                    Phase::Pan(0)
                } else {
                    Phase::Rebase(0)
                }
            }
            Phase::Pan(sample) => {
                self.unchanged(&snapshot, history)?;
                ensure!(
                    panel.active == (sample < 4),
                    "middle capture did not survive outside motion/right release or cancel on Escape"
                );
                if sample == 4 {
                    near(panel.transform.0, self.saved().transforms[0].0)?;
                    self.wait_submission = false;
                    Phase::RebaseStart
                } else {
                    if sample > 0 {
                        near(
                            panel.transform.0,
                            add(self.saved().transforms[0].0, sub(self.pointer, self.anchor)),
                        )?;
                    }
                    match sample {
                        0 | 2 => {
                            let bounds = panel.canvas.context("pan canvas is not submitted")?;
                            self.pointer = [
                                f32::from(bounds.origin.x + bounds.size.width)
                                    + 80.
                                    + if sample == 2 { 8. } else { 0. },
                                self.anchor[1] + 20.,
                            ];
                            mouse(viewer, self.pointer, MouseButton::Middle, Pointer::Move)?;
                        }
                        1 => {
                            mouse(viewer, self.pointer, MouseButton::Right, Pointer::Up)?;
                            self.wait_submission = false;
                        }
                        3 => escape(viewer)?,
                        _ => unreachable!(),
                    }
                    Phase::Pan(sample + 1)
                }
            }
            Phase::Rebase(sample) => {
                self.unchanged(&snapshot, history)?;
                ensure!(
                    panel.active == (sample < 4),
                    "middle pan finished with the wrong lifecycle"
                );
                match sample {
                    0 => {
                        self.pointer = add(self.anchor, [16., 9.]);
                        mouse(viewer, self.pointer, MouseButton::Middle, Pointer::Move)?;
                    }
                    1 => {
                        near(
                            panel.transform.0,
                            add(self.saved().transforms[0].0, [16., 9.]),
                        )?;
                        self.before = panel.transform;
                        scroll(
                            viewer,
                            self.pointer,
                            ScrollDelta::Pixels(point(px(12.), px(-7.))),
                            false,
                        )?;
                    }
                    2 => {
                        near(panel.transform.0, add(self.before.0, [12., -7.]))?;
                        self.before = panel.transform;
                        mouse(viewer, self.pointer, MouseButton::Middle, Pointer::Move)?;
                        self.wait_submission = false;
                    }
                    3 => {
                        ensure!(
                            panel.transform == self.before,
                            "stationary motion jumped after wheel rebase"
                        );
                        mouse(viewer, self.pointer, MouseButton::Middle, Pointer::Up)?;
                    }
                    4 => {
                        ensure!(
                            panel.transform == self.before,
                            "stationary release jumped after wheel rebase"
                        );
                        println!(
                            "NARA_GPUI_GRAPH_GESTURES_VIEW anchor=true independent=true wheel=true zoom_limits=true middle_capture=true escape=true wrong_button_preserved=true stationary_release=true ports_no_move=true"
                        );
                        let delta = sub(self.home.0, panel.transform.0);
                        scroll(
                            viewer,
                            canvas_point(panel)?,
                            ScrollDelta::Pixels(point(px(delta[0]), px(delta[1]))),
                            false,
                        )?;
                        return self.next(Phase::PixelsStart);
                    }
                    _ => unreachable!(),
                }
                Phase::Rebase(sample + 1)
            }
            Phase::PixelsStart => {
                self.unchanged(&snapshot, history)?;
                near(panel.transform.0, self.home.0)?;
                zoom_is(panel.transform.1, self.home.1)?;
                ensure!(
                    panels.iter().all(|p| !p.active && p.links >= 3),
                    "final graph paths are not submitted"
                );
                self.probe = xy(panel.probe.context("submitted curve has no pixel probe")?);
                renderer.diagnostic_next.set(true);
                viewer.request()?;
                Phase::Pixels
            }
            Phase::Pixels => {
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    self.wait_submission = false;
                    return Ok(false);
                };
                self.unchanged(&snapshot, history)?;
                near(
                    xy(panel
                        .probe
                        .context("diagnostic curve lost its submitted probe")?),
                    self.probe,
                )?;
                ensure!(
                    diagnostic.scale == 1.25 && renderer.diagnostics.get() == 1,
                    "graph diagnostic used invalid work or scale"
                );
                let mut colored = 0;
                for y in -3..=3 {
                    for x in -3..=3 {
                        let sample = add(self.probe, [x as f32, y as f32]);
                        if sample[0] < 0.
                            || sample[1] < 0.
                            || sample[0] * diagnostic.scale >= diagnostic.width as f32
                            || sample[1] * diagnostic.scale >= diagnostic.height as f32
                        {
                            continue;
                        }
                        let pixel = diagnostic.logical_pixel(sample[0], sample[1]);
                        if pixel[2] > 140 && pixel[1] > 100 && pixel[2] > pixel[0] {
                            colored += 1;
                        }
                    }
                }
                ensure!(
                    colored > 0,
                    "original submitted curve has no matching blue GPU pixels"
                );
                println!(
                    "NARA_GPUI_GRAPH_GESTURES_PIXELS colored={colored} scale=1.25 same_host_submission=true qualification_replay=true"
                );
                self.begin(
                    viewer,
                    checkpoint(&snapshot, history, &panels),
                    xy(panel
                        .header
                        .context("the final node header is not submitted")?),
                    MouseButton::Left,
                )?;
                Phase::ClosePreview
            }
            Phase::ClosePreview => {
                self.preview(&snapshot, history, panel)?;
                self.pointer = add(self.anchor, [16., 10.]);
                mouse(viewer, self.pointer, MouseButton::Left, Pointer::Move)?;
                Phase::Close
            }
            Phase::Close => {
                self.preview(&snapshot, history, panel)?;
                near_painted(
                    xy(panel
                        .header
                        .context("the final preview header is not submitted")?),
                    self.pointer,
                    surface.geometry().scale_factor(),
                )?;
                viewer.expected_gesture_close_history = Some(history);
                println!(
                    "NARA_GPUI_GRAPH_GESTURES_CLOSE_CAPTURE admitted=true preview_submitted=true"
                );
                self.phase = Phase::Done;
                return Ok(true);
            }
            Phase::Done => return Ok(true),
        };
        Ok(false)
    }

    fn next(&mut self, phase: Phase) -> Result<bool> {
        self.phase = phase;
        Ok(false)
    }
    fn saved(&self) -> &Checkpoint {
        self.saved.as_ref().expect("a graph sequence has started")
    }
    fn begin(
        &mut self,
        viewer: &Viewer,
        saved: Checkpoint,
        anchor: [f32; 2],
        button: MouseButton,
    ) -> Result<()> {
        self.saved = Some(saved);
        self.anchor = anchor;
        self.pointer = anchor;
        mouse(viewer, anchor, button, Pointer::Down)
    }
    fn unchanged(&self, snapshot: &GraphSnapshot, history: DocumentHistoryStatus) -> Result<()> {
        ensure!(
            snapshot == &self.saved().snapshot && history == self.saved().history,
            "preview/cancellation/navigation changed source or history"
        );
        Ok(())
    }
    fn preview(
        &self,
        snapshot: &GraphSnapshot,
        history: DocumentHistoryStatus,
        panel: &Panel,
    ) -> Result<()> {
        self.unchanged(snapshot, history)?;
        ensure!(
            panel.active
                && panel.selected == Some(NodeId(1))
                && panel.transform == self.saved().transforms[0],
            "node capture lost its original selection or transform"
        );
        Ok(())
    }
    fn one_move(
        &self,
        snapshot: &GraphSnapshot,
        history: DocumentHistoryStatus,
        panel: &Panel,
    ) -> Result<()> {
        let saved = self.saved();
        let original = saved
            .snapshot
            .view
            .node(NodeId(1))
            .context("original node 1 is missing")?
            .position;
        let position = snapshot
            .view
            .node(NodeId(1))
            .context("moved node 1 is missing")?
            .position;
        let zoom = saved.transforms[0].1;
        let expected = add(original, sub(self.pointer, self.anchor).map(|v| v / zoom));
        near(position.map(|v| v * zoom), expected.map(|v| v * zoom))?;
        let mut actual = snapshot.view.clone();
        actual
            .nodes
            .iter_mut()
            .find(|node| node.id == NodeId(1))
            .unwrap()
            .position = original;
        ensure!(
            !panel.active
                && actual == saved.snapshot.view
                && snapshot.document == saved.snapshot.document
                && snapshot.revision == saved.snapshot.revision.next(),
            "release did not commit exactly one original-basis node move"
        );
        history_is(history, saved.history.undo_depth + 1, 0)
    }
    fn restored(&self, snapshot: &GraphSnapshot, history: DocumentHistoryStatus) -> Result<()> {
        ensure!(
            snapshot.view == self.saved().snapshot.view
                && snapshot.document == self.saved().snapshot.document
                && snapshot.revision == self.saved().snapshot.revision.next().next(),
            "native Undo did not restore exact original content"
        );
        history_is(history, self.saved().history.undo_depth, 1)
    }
}

fn checkpoint(
    snapshot: &GraphSnapshot,
    history: DocumentHistoryStatus,
    panels: &[Panel; 2],
) -> Checkpoint {
    Checkpoint {
        snapshot: snapshot.clone(),
        history,
        transforms: [panels[0].transform, panels[1].transform],
    }
}

fn observe(viewer: &Viewer, snapshot: &GraphSnapshot) -> Result<[Panel; 2]> {
    viewer.application.as_ref().unwrap().update(|cx| {
        viewer.root.unwrap().read_with(cx, |root, cx| {
            root.graph_panels
                .as_ref()
                .unwrap()
                .each_ref()
                .map(|entity| {
                    let panel = entity.read(cx);
                    Panel {
                        canvas: panel.canvas_bounds(),
                        header: panel.hit_center(GraphHit::Node(NodeId(1))),
                        port: panel.hit_center(GraphHit::OutputPort(NodeId(1))),
                        probe: panel.link_probe(),
                        transform: panel.transform(),
                        active: panel.gesture_active(),
                        selected: panel.selected(),
                        links: panel.rendered_links(),
                        current: panel.presents(snapshot),
                    }
                })
        })
    })
}

fn click(viewer: &Viewer, hit: GraphHit) -> Result<()> {
    let center = viewer
        .application
        .as_ref()
        .unwrap()
        .update(|cx| {
            viewer.root.unwrap().read_with(cx, |root, cx| {
                root.graph_panels.as_ref().unwrap()[0]
                    .read(cx)
                    .hit_center(hit)
            })
        })?
        .context("graph control has no submitted bounds")?;
    viewer.click(f32::from(center.x), f32::from(center.y))
}

enum Pointer {
    Down,
    Move,
    Up,
}
fn mouse(viewer: &Viewer, at: [f32; 2], button: MouseButton, event: Pointer) -> Result<()> {
    let position = point(px(at[0]), px(at[1]));
    let input = match event {
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
fn scroll(viewer: &Viewer, at: [f32; 2], delta: ScrollDelta, control: bool) -> Result<()> {
    viewer
        .surface
        .as_ref()
        .unwrap()
        .dispatch_input(PlatformInput::Scroll(ScrollEvent {
            position: point(px(at[0]), px(at[1])),
            delta,
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        }))?;
    Ok(())
}
fn escape(viewer: &Viewer) -> Result<()> {
    viewer.keyboard_input(
        Key::Named(NamedKey::Escape),
        None,
        ElementState::Pressed,
        false,
    )?;
    viewer.keyboard_input(
        Key::Named(NamedKey::Escape),
        None,
        ElementState::Released,
        false,
    )
}
fn canvas_point(panel: &Panel) -> Result<[f32; 2]> {
    let bounds = panel
        .canvas
        .context("graph canvas has no submitted bounds")?;
    ensure!(!bounds.is_empty(), "submitted graph canvas is empty");
    Ok(xy(bounds.center()))
}
fn history_is(actual: DocumentHistoryStatus, undo_depth: usize, redo_depth: usize) -> Result<()> {
    ensure!(
        actual
            == DocumentHistoryStatus {
                undo_depth,
                redo_depth
            },
        "unexpected graph history: {actual:?}, expected undo={undo_depth} redo={redo_depth}"
    );
    Ok(())
}
fn xy(point: Point<Pixels>) -> [f32; 2] {
    [f32::from(point.x), f32::from(point.y)]
}
fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn near(actual: [f32; 2], expected: [f32; 2]) -> Result<()> {
    ensure!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(a, b)| a.is_finite() && b.is_finite() && (a - b).abs() <= 0.02),
        "logical pixel mismatch: actual={actual:?}, expected={expected:?}, tolerance=0.02"
    );
    Ok(())
}

/// Bounds the difference between independently snapped layout positions in physical pixels.
fn near_painted(actual: [f32; 2], expected: [f32; 2], scale: f32) -> Result<()> {
    ensure!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(a, b)| { a.is_finite() && b.is_finite() && (a - b).abs() * scale <= 1.0001 }),
        "painted graph geometry differs by more than one physical pixel: actual={actual:?}, expected={expected:?}, scale={scale}"
    );
    Ok(())
}
fn zoom_is(actual: f32, expected: f32) -> Result<()> {
    ensure!(
        actual.is_finite() && expected.is_finite() && (actual - expected).abs() <= 0.000001,
        "graph zoom mismatch: actual={actual}, expected={expected}"
    );
    Ok(())
}
