//! Real presented controls and original native delivery for the finite extension graph.

use super::{Viewer, graph_panel::GraphHit};
use anyhow::{Result, ensure};
use nara_gpui_engine_consumer::graph::{GraphView, NodeId, NodeKind, evaluate};
use open_gpui::{
    DevicePixels, HostedSurfaceGeometry, KeyDownEvent, Keystroke, PlatformInput, size,
};
use winit::{
    event::{ElementState, Ime, WindowEvent},
    keyboard::{Key, NamedKey},
};

pub(super) struct GraphSmoke {
    step: usize,
    submitted: u64,
    saved: Option<GraphView>,
    refused: u64,
    before_refusal: Option<nara_core::DocumentRevision>,
    before_reopen: Option<nara_core::DocumentRevision>,
}

impl GraphSmoke {
    pub fn new() -> Self {
        Self {
            step: 0,
            submitted: 0,
            saved: None,
            refused: 0,
            before_refusal: None,
            before_reopen: None,
        }
    }

    pub fn advance(&mut self, viewer: &mut Viewer) -> Result<bool> {
        let renderer = viewer.renderer.as_ref().unwrap().clone();
        let surface = viewer.surface.as_ref().unwrap().clone();
        if renderer.pending()
            || surface.needs_frame()
            || self.step != 44 && renderer.submissions.get() <= self.submitted
            || viewer
                .engine
                .as_ref()
                .unwrap()
                .borrow()
                .graph_persistence_pending()
        {
            return Ok(false);
        }
        let snapshot = viewer.engine.as_ref().unwrap().borrow().graph_snapshot()?;
        let presented = viewer.application.as_ref().unwrap().update(|cx| {
            viewer.root.unwrap().read_with(cx, |root, cx| {
                root.graph_panels
                    .as_ref()
                    .unwrap()
                    .iter()
                    .all(|panel| panel.read(cx).presents(&snapshot))
            })
        })?;
        if !presented {
            return Ok(false);
        }
        println!(
            "NARA_GPUI_GRAPH_PHASE step={} nodes={} links={} submitted={}",
            self.step,
            snapshot.view.nodes.len(),
            snapshot.view.links.len(),
            renderer.submissions.get()
        );
        self.submitted = renderer.submissions.get();
        match self.step {
            0 => click(viewer, 0, GraphHit::AddScalar)?,
            1 => {
                ensure!(
                    snapshot.view.nodes.len() == 1,
                    "the first graph command did not publish"
                );
                click(viewer, 0, GraphHit::AddScalar)?;
            }
            2 => click(viewer, 0, GraphHit::Add)?,
            3 => click(viewer, 0, GraphHit::Output)?,
            4 => {
                ensure!(
                    snapshot.view.nodes.len() == 4,
                    "graph creation did not retain four nodes"
                );
                click(viewer, 0, GraphHit::OutputPort(NodeId(1)))?;
            }
            5 => click(viewer, 0, GraphHit::InputPort(NodeId(3), 0))?,
            6 => {
                ensure!(
                    snapshot.view.links.len() == 1,
                    "the first graph connection did not publish"
                );
                click(viewer, 0, GraphHit::OutputPort(NodeId(2)))?;
            }
            7 => click(viewer, 0, GraphHit::InputPort(NodeId(3), 1))?,
            8 => click(viewer, 0, GraphHit::OutputPort(NodeId(3)))?,
            9 => click(viewer, 0, GraphHit::InputPort(NodeId(4), 0))?,
            10 => {
                ensure!(
                    snapshot.view.links.len() == 3 && evaluate(&snapshot.view)?.is_some(),
                    "typed graph ports did not form a complete value path"
                );
                click(viewer, 0, GraphHit::Node(NodeId(1)))?;
            }
            11 => click(viewer, 1, GraphHit::Node(NodeId(1)))?,
            12 => click(viewer, 1, GraphHit::Draft)?,
            13 => replace_text(viewer, "stale graph draft")?,
            14 => click(viewer, 0, GraphHit::Draft)?,
            15 => {
                replace_text(viewer, "7")?;
                click(viewer, 0, GraphHit::Apply)?;
            }
            16 => {
                ensure!(
                    snapshot
                        .view
                        .node(NodeId(1))
                        .is_some_and(|node| node.kind == NodeKind::Scalar(7.)),
                    "the original graph scalar command did not publish"
                );
                ensure!(
                    draft(viewer, 1)? == "stale graph draft" && conflict(viewer, 1)?,
                    "the other graph panel lost its independent dirty draft"
                );
                self.before_refusal = Some(snapshot.revision);
                click(viewer, 1, GraphHit::Apply)?;
            }
            17 => {
                ensure!(
                    Some(snapshot.revision) == self.before_refusal,
                    "a conflicted graph panel mutated the source"
                );
                click(viewer, 1, GraphHit::Reload)?;
            }
            18 => {
                ensure!(
                    draft(viewer, 1)? == "7" && !conflict(viewer, 1)?,
                    "explicit graph reload did not adopt the current scalar"
                );
                click(viewer, 0, GraphHit::AddVector)?;
            }
            19 => {
                self.refused = viewer.engine.as_ref().unwrap().borrow().graph_refusals;
                self.before_refusal = Some(snapshot.revision);
                click(viewer, 0, GraphHit::OutputPort(NodeId(5)))?;
            }
            20 => click(viewer, 0, GraphHit::InputPort(NodeId(3), 0))?,
            21 => {
                ensure!(
                    snapshot.view.nodes.len() == 5
                        && snapshot.view.links.len() == 3
                        && Some(snapshot.revision) == self.before_refusal
                        && viewer.engine.as_ref().unwrap().borrow().graph_refusals
                            == self.refused + 1,
                    "invalid typed graph link changed the document or bypassed its owner"
                );
                self.saved = Some(snapshot.view.clone());
                click(viewer, 0, GraphHit::Undo)?;
            }
            22 => {
                ensure!(
                    snapshot.view.nodes.len() == 4,
                    "graph Undo lost original history"
                );
                click(viewer, 0, GraphHit::Redo)?;
            }
            23 => {
                ensure!(
                    Some(&snapshot.view) == self.saved.as_ref(),
                    "graph Redo did not restore exact content"
                );
                click(viewer, 0, GraphHit::Save)?;
            }
            24 => {
                ensure!(
                    !snapshot.dirty
                        && Some(
                            &viewer
                                .engine
                                .as_ref()
                                .unwrap()
                                .borrow()
                                .graph_saved_view()?
                        ) == self.saved.as_ref(),
                    "the graph source did not save through the original file Host"
                );
                click(viewer, 0, GraphHit::Node(NodeId(1)))?;
            }
            25 => click(viewer, 0, GraphHit::Draft)?,
            26 => {
                replace_text(viewer, "9")?;
                click(viewer, 0, GraphHit::Apply)?;
            }
            27 => {
                ensure!(
                    snapshot.dirty
                        && snapshot
                            .view
                            .node(NodeId(1))
                            .is_some_and(|node| node.kind == NodeKind::Scalar(9.)),
                    "the unsaved graph edit did not reach its original owner"
                );
                self.before_reopen = Some(snapshot.revision);
                click(viewer, 0, GraphHit::Reopen)?;
            }
            28 => {
                ensure!(
                    !snapshot.dirty
                        && Some(&snapshot.view) == self.saved.as_ref()
                        && snapshot.revision.source_id() != self.before_reopen.unwrap().source_id(),
                    "graph Reopen did not restore the captured source with a new incarnation"
                );
                click(viewer, 0, GraphHit::ZoomIn)?;
            }
            29 => click(viewer, 0, GraphHit::PanRight)?,
            30 => {
                ensure!(
                    transform(viewer, 0)? != transform(viewer, 1)?,
                    "graph view transforms were shared between panels"
                );
                ensure!(
                    Some(&snapshot.view) == self.saved.as_ref(),
                    "view navigation mutated the graph source"
                );
                let physical = viewer.engine.as_ref().unwrap().borrow().window.inner_size();
                let geometry = HostedSurfaceGeometry::from_physical_size(
                    size(
                        DevicePixels(physical.width.try_into()?),
                        DevicePixels(physical.height.try_into()?),
                    ),
                    1.25,
                )?;
                surface.set_geometry(geometry.size(), geometry.scale_factor())?;
            }
            31 => {
                ensure!(
                    surface.geometry().scale_factor() == 1.25,
                    "graph fractional scale was not applied"
                );
                click(viewer, 0, GraphHit::Node(NodeId(3)))?;
            }
            32 => {
                let state = viewer.application.as_ref().unwrap().update(|cx| {
                    viewer.root.unwrap().read_with(cx, |root, cx| {
                        let panels = root.graph_panels.as_ref().unwrap();
                        (
                            panels[0].read(cx).selected(),
                            panels[0].read(cx).rendered_links(),
                            panels[1].read(cx).rendered_links(),
                        )
                    })
                })?;
                ensure!(
                    state.0 == Some(NodeId(3)) && state.1 >= 3 && state.2 >= 3,
                    "fractional graph paint and input did not agree"
                );
                ensure!(
                    renderer.diagnostics.get() == 0,
                    "the graph renderer reported invalid work"
                );
                println!(
                    "NARA_GPUI_GRAPH_WORKFLOW nodes={} links={} typed_refusal=true independent_drafts=true undo_redo=true save_reopen=true independent_pan_zoom=true scale=1.25 original_host=true",
                    snapshot.view.nodes.len(),
                    snapshot.view.links.len()
                );
                click(viewer, 0, GraphHit::Node(NodeId(1)))?;
            }
            33 => click(viewer, 0, GraphHit::Draft)?,
            34 => {
                viewer
                    .surface
                    .as_ref()
                    .unwrap()
                    .dispatch_input(PlatformInput::KeyDown(KeyDownEvent {
                        keystroke: Keystroke::parse("ctrl-a")?,
                        is_held: false,
                        prefer_character_input: false,
                    }))?;
                viewer.input(WindowEvent::Ime(Ime::Preedit("11".into(), Some((0, 2)))))?;
                self.before_refusal = Some(snapshot.revision);
                click(viewer, 0, GraphHit::Apply)?;
            }
            35 => {
                ensure!(
                    Some(snapshot.revision) == self.before_refusal && composing(viewer, 0)?,
                    "Apply promoted uncommitted graph composition"
                );
                viewer.input(WindowEvent::Ime(Ime::Disabled))?;
                ensure!(
                    draft(viewer, 0)? == "7",
                    "composition cancellation lost committed graph draft"
                );
                click(viewer, 0, GraphHit::Node(NodeId(3)))?;
            }
            36 => key(viewer, NamedKey::ArrowUp)?,
            37 => {
                ensure!(
                    snapshot.view.node(NodeId(3)).unwrap().position[1] == 8.,
                    "native up key did not move graph node"
                );
                key(viewer, NamedKey::ArrowDown)?;
            }
            38 => {
                ensure!(
                    snapshot.view.node(NodeId(3)).unwrap().position[1] == 24.,
                    "native down key did not move graph node: {:?}; status={:?}",
                    snapshot.view,
                    snapshot.status
                );
                click(viewer, 0, GraphHit::Node(NodeId(1)))?;
            }
            39 => key(viewer, NamedKey::Delete)?,
            40 => {
                ensure!(
                    snapshot.view.node(NodeId(1)).is_none(),
                    "native delete key did not remove graph node"
                );
                click(viewer, 0, GraphHit::AddScalar)?;
            }
            41 => {
                let new = snapshot
                    .view
                    .node(NodeId(6))
                    .ok_or_else(|| anyhow::anyhow!("the successor graph node is missing"))?;
                ensure!(
                    snapshot
                        .view
                        .nodes
                        .iter()
                        .filter(|node| node.id != new.id)
                        .all(|node| {
                            (node.position[0] - new.position[0]).abs() >= 156.
                                || (node.position[1] - new.position[1]).abs() >= 104.
                        }),
                    "the added graph node covers an existing node"
                );
                click(viewer, 0, GraphHit::Undo)?;
            }
            42 => {
                ensure!(
                    snapshot.view.node(NodeId(6)).is_none()
                        && snapshot.view.node(NodeId(1)).is_none(),
                    "graph Undo did not remove the newly added node"
                );
                key(viewer, NamedKey::Enter)?;
            }
            43 => {
                ensure!(
                    Some(&snapshot.view) == self.saved.as_ref(),
                    "native Enter did not activate focused graph Undo"
                );
                println!(
                    "NARA_GPUI_GRAPH_INPUT preedit_apply_refused=true composition_restored=true node_arrows=true delete=true nonoverlap=true enter_routed=true"
                );
                renderer.diagnostic_next.set(true);
                viewer.request()?;
            }
            44 => {
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                let probe = viewer
                    .application
                    .as_ref()
                    .unwrap()
                    .update(|cx| {
                        viewer.root.unwrap().read_with(cx, |root, cx| {
                            root.graph_panels.as_ref().unwrap()[0].read(cx).link_probe()
                        })
                    })?
                    .ok_or_else(|| {
                        anyhow::anyhow!("the graph path probe has no current endpoints")
                    })?;
                let center = [f32::from(probe.x), f32::from(probe.y)];
                let mut colored = 0;
                for y in -3..=3 {
                    for x in -3..=3 {
                        let pixel =
                            diagnostic.logical_pixel(center[0] + x as f32, center[1] + y as f32);
                        if pixel[2] > 140 && pixel[1] > 100 && pixel[2] > pixel[0] {
                            colored += 1;
                        }
                    }
                }
                ensure!(
                    colored > 0,
                    "the submitted graph connection has no matching GPU pixels at its actual endpoints"
                );
                println!(
                    "NARA_GPUI_GRAPH_PIXELS colored={} scale={} same_host_submission=true qualification_replay=true",
                    colored, diagnostic.scale
                );
                return Ok(true);
            }
            _ => unreachable!("finite graph qualification has a terminal step"),
        }
        self.step += 1;
        Ok(false)
    }
}

fn click(viewer: &Viewer, index: usize, hit: GraphHit) -> Result<()> {
    let center = viewer
        .application
        .as_ref()
        .unwrap()
        .update(|cx| {
            viewer.root.unwrap().read_with(cx, |root, cx| {
                root.graph_panels.as_ref().unwrap()[index]
                    .read(cx)
                    .hit_center(hit)
            })
        })?
        .ok_or_else(|| anyhow::anyhow!("graph control {hit:?} has no painted bounds"))?;
    viewer.click(f32::from(center.x), f32::from(center.y))
}

fn replace_text(viewer: &mut Viewer, text: &str) -> Result<()> {
    viewer
        .surface
        .as_ref()
        .unwrap()
        .dispatch_input(PlatformInput::KeyDown(KeyDownEvent {
            keystroke: Keystroke::parse("ctrl-a")?,
            is_held: false,
            prefer_character_input: false,
        }))?;
    viewer.input(WindowEvent::Ime(Ime::Commit(text.into())))
}

fn draft(viewer: &Viewer, index: usize) -> Result<String> {
    viewer.application.as_ref().unwrap().update(|cx| {
        viewer.root.unwrap().read_with(cx, |root, cx| {
            let draft = root.graph_panels.as_ref().unwrap()[index]
                .read(cx)
                .draft
                .clone();
            draft.read(cx).text().to_owned()
        })
    })
}

fn conflict(viewer: &Viewer, index: usize) -> Result<bool> {
    viewer.application.as_ref().unwrap().update(|cx| {
        viewer.root.unwrap().read_with(cx, |root, cx| {
            root.graph_panels.as_ref().unwrap()[index]
                .read(cx)
                .conflict()
        })
    })
}

fn transform(viewer: &Viewer, index: usize) -> Result<([f32; 2], f32)> {
    viewer.application.as_ref().unwrap().update(|cx| {
        viewer.root.unwrap().read_with(cx, |root, cx| {
            root.graph_panels.as_ref().unwrap()[index]
                .read(cx)
                .transform()
        })
    })
}

fn composing(viewer: &Viewer, index: usize) -> Result<bool> {
    viewer.application.as_ref().unwrap().update(|cx| {
        viewer.root.unwrap().read_with(cx, |root, cx| {
            root.graph_panels.as_ref().unwrap()[index]
                .read(cx)
                .draft
                .read(cx)
                .is_composing()
        })
    })
}

fn key(viewer: &Viewer, key: NamedKey) -> Result<()> {
    viewer.keyboard_input(Key::Named(key), None, ElementState::Pressed, false)?;
    viewer.keyboard_input(Key::Named(key), None, ElementState::Released, false)
}
