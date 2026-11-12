//! Complete native two-View and late-evaluation workflow through original owners.

use super::{
    Viewer,
    engine::views::ViewKey,
    views_panel::{ViewControl, ViewSnapshot},
};
use anyhow::{Result, ensure};
use nara_gpui_engine_consumer::graph::{
    evaluate,
    preview::{GraphPreviewBasis, GraphPreviewValue},
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
enum Phase {
    Initial,
    InitialPixels,
    ClickedA,
    Relocated,
    OldPoint,
    ClickedB,
    ArmedEdit,
    HeldEdit,
    PendingPixels,
    NewerEdit,
    ReleasedEdit,
    Saved,
    ArmedReopen,
    HeldReopen,
    Reopened,
    ReleasedReopen,
    CapacityArm,
    CapacityCompile,
    CapacityFilled,
    CapacityWaiting,
    CapacityCompleted,
    ShutdownArm,
    ShutdownHeld,
    ClosedA,
    PulsedB,
    ClickedRemaining,
    ArmedClose,
    HeldClose,
    ClosedGraph,
    ReleasedClose,
    ClosedPixels,
}

pub(super) struct ViewsSmoke {
    phase: Phase,
    old_a: Option<Rc<ViewSnapshot>>,
    old_b: Option<Rc<ViewSnapshot>>,
    basis: Option<GraphPreviewBasis>,
    good: Option<GraphPreviewValue>,
    stale: u64,
    b_frame: u64,
    close_pending: bool,
}

impl ViewsSmoke {
    pub fn new(close_pending: bool) -> Self {
        Self {
            phase: Phase::Initial,
            old_a: None,
            old_b: None,
            basis: None,
            good: None,
            stale: 0,
            b_frame: 0,
            close_pending,
        }
    }

    pub fn advance(&mut self, viewer: &mut Viewer) -> Result<bool> {
        let renderer = viewer.renderer.as_ref().unwrap().clone();
        if renderer.pending() || viewer.surface.as_ref().unwrap().needs_frame() {
            return Ok(false);
        }
        let state = viewer.preview_jobs.as_ref().unwrap().state();
        let controls_ready = viewer.application.as_ref().unwrap().update(|cx| {
            viewer
                .views_panel
                .as_ref()
                .unwrap()
                .read(cx)
                .presents_basis(state.current)
        });
        if !controls_ready {
            return Ok(false);
        }
        let [a, b] = observations(viewer);
        match self.phase {
            Phase::Initial => {
                if !ready(viewer, 5., true) {
                    return Ok(false);
                }
                let (a, b) = (a.unwrap(), b.unwrap());
                ensure!(
                    a.image.display.receipt().producer_id()
                        != b.image.display.receipt().producer_id()
                        && a.image.display.receipt().source_generation()
                            != b.image.display.receipt().source_generation(),
                    "the graph Views alias a render producer or source"
                );
                self.old_a = Some(a);
                self.old_b = Some(b);
                renderer.diagnostic_next.set(true);
                viewer.request()?;
                self.phase = Phase::InitialPixels;
            }
            Phase::InitialPixels => {
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                check_pixels(&diagnostic, self.old_a.as_ref().unwrap())?;
                check_pixels(&diagnostic, self.old_b.as_ref().unwrap())?;
                println!(
                    "NARA_GPUI_VIEWS_PIXELS distinct_producers=true outputs=2 original_submission=true"
                );
                click_view(viewer, &a.unwrap())?;
                self.phase = Phase::ClickedA;
            }
            Phase::ClickedA => {
                if !ready(viewer, 6., true) {
                    return Ok(false);
                }
                refuses(viewer, self.old_a.as_ref().unwrap())?;
                self.old_a = a;
                self.old_b = b;
                click(viewer, ViewControl::RelocateA)?;
                self.phase = Phase::Relocated;
            }
            Phase::Relocated => {
                if !ready(viewer, 6., true) {
                    return Ok(false);
                }
                ensure!(
                    viewer
                        .application
                        .as_ref()
                        .unwrap()
                        .update(|cx| { viewer.views_panel.as_ref().unwrap().read(cx).relocated() }),
                    "the relocated View did not retain local placement"
                );
                let a = a.unwrap();
                let old = self.old_a.as_ref().unwrap();
                if a.mapping == old.mapping {
                    return Ok(false);
                }
                ensure!(
                    a.image.display.receipt() == old.image.display.receipt()
                        && a.image.display.occurrence() != old.image.display.occurrence(),
                    "relocation changed pixels or reused the old display occurrence"
                );
                ensure!(
                    b.as_ref().unwrap().image.display.receipt()
                        == self.old_b.as_ref().unwrap().image.display.receipt(),
                    "relocating A republished B"
                );
                refuses(viewer, old)?;
                self.basis = state.current;
                click_view(viewer, old)?;
                self.phase = Phase::OldPoint;
            }
            Phase::OldPoint => {
                ensure!(
                    state.current == self.basis,
                    "the withdrawn A position still edited the graph"
                );
                click_view(viewer, &b.unwrap())?;
                self.phase = Phase::ClickedB;
            }
            Phase::ClickedB => {
                if !ready(viewer, 7., true) {
                    return Ok(false);
                }
                println!(
                    "NARA_GPUI_VIEWS_ROUTES relocated_occurrence=true old_route_refused=true other_view_interacts=true"
                );
                click(viewer, ViewControl::HoldNext)?;
                self.phase = Phase::ArmedEdit;
            }
            Phase::ArmedEdit => {
                click(viewer, ViewControl::Edit)?;
                self.phase = Phase::HeldEdit;
            }
            Phase::HeldEdit => {
                if viewer.preview_jobs.as_ref().unwrap().held_jobs() != 1 || state.pending.is_none()
                {
                    return Ok(false);
                }
                ensure!(
                    state.last_good.as_ref().unwrap().value == Some(7.)
                        && a.as_ref().unwrap().image.value == Some(7.)
                        && b.as_ref().unwrap().image.value == Some(7.),
                    "pending evaluation discarded or relabeled last-good output"
                );
                renderer.diagnostic_next.set(true);
                viewer.request()?;
                self.phase = Phase::PendingPixels;
            }
            Phase::PendingPixels => {
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                check_pixels(&diagnostic, &a.unwrap())?;
                check_pixels(&diagnostic, &b.unwrap())?;
                click(viewer, ViewControl::Edit)?;
                self.phase = Phase::NewerEdit;
            }
            Phase::NewerEdit => {
                if !ready(viewer, 9., true) {
                    return Ok(false);
                }
                ensure!(
                    viewer.preview_jobs.as_ref().unwrap().held_jobs() == 1,
                    "the old job was not retained"
                );
                self.good = state.last_good;
                self.stale = viewer.preview_jobs.as_ref().unwrap().stale;
                click(viewer, ViewControl::ReleaseHeld)?;
                self.phase = Phase::ReleasedEdit;
            }
            Phase::ReleasedEdit => {
                if viewer.preview_jobs.as_ref().unwrap().pending_jobs() != 0 {
                    return Ok(false);
                }
                ensure!(
                    state.last_good == self.good
                        && viewer.preview_jobs.as_ref().unwrap().stale == self.stale + 1,
                    "the older edited-source evaluation replaced the current result"
                );
                println!(
                    "NARA_GPUI_VIEWS_LATE_EDIT rejected=true last_good_pixels_while_pending=true"
                );
                click(viewer, ViewControl::Save)?;
                self.phase = Phase::Saved;
            }
            Phase::Saved => {
                let engine = viewer.engine.as_ref().unwrap().borrow();
                if engine.graph_persistence_pending() || engine.graph_snapshot()?.dirty {
                    return Ok(false);
                }
                ensure!(
                    engine.graph_saved_view()? == engine.graph_snapshot()?.view,
                    "the original Host saved another graph"
                );
                drop(engine);
                click(viewer, ViewControl::HoldNext)?;
                self.phase = Phase::ArmedReopen;
            }
            Phase::ArmedReopen => {
                click(viewer, ViewControl::Compile)?;
                self.phase = Phase::HeldReopen;
            }
            Phase::HeldReopen => {
                if viewer.preview_jobs.as_ref().unwrap().held_jobs() != 1 || state.pending.is_none()
                {
                    return Ok(false);
                }
                self.basis = state.current;
                click(viewer, ViewControl::Reopen)?;
                self.phase = Phase::Reopened;
            }
            Phase::Reopened => {
                if !ready(viewer, 9., true) || state.current == self.basis {
                    return Ok(false);
                }
                ensure!(
                    state.current.unwrap().revision.source_id()
                        != self.basis.unwrap().revision.source_id(),
                    "Reopen did not replace the graph source incarnation"
                );
                self.good = state.last_good;
                self.stale = viewer.preview_jobs.as_ref().unwrap().stale;
                click(viewer, ViewControl::ReleaseHeld)?;
                self.phase = Phase::ReleasedReopen;
            }
            Phase::ReleasedReopen => {
                if viewer.preview_jobs.as_ref().unwrap().pending_jobs() != 0 {
                    return Ok(false);
                }
                ensure!(
                    state.last_good == self.good
                        && viewer.preview_jobs.as_ref().unwrap().stale == self.stale + 1,
                    "a pre-Reopen job acquired the new source's result authority"
                );
                println!(
                    "NARA_GPUI_VIEWS_LATE_REOPEN rejected=true original_source_incarnation=true"
                );
                click(viewer, ViewControl::HoldNext)?;
                self.phase = if self.close_pending {
                    Phase::ShutdownArm
                } else {
                    Phase::CapacityCompile
                };
            }
            Phase::CapacityArm => {
                click(viewer, ViewControl::HoldNext)?;
                self.phase = Phase::CapacityCompile;
            }
            Phase::CapacityCompile => {
                click(viewer, ViewControl::Compile)?;
                self.phase = Phase::CapacityFilled;
            }
            Phase::CapacityFilled => {
                if viewer.preview_jobs.as_ref().unwrap().held_jobs() < 4 {
                    self.phase = Phase::CapacityArm;
                    return Ok(false);
                }
                ensure!(
                    viewer.preview_jobs.as_ref().unwrap().pending_jobs() == 4,
                    "the evaluation capacity control did not retain four original Tasks"
                );
                click(viewer, ViewControl::Edit)?;
                self.phase = Phase::CapacityWaiting;
            }
            Phase::CapacityWaiting => {
                if !viewer.compile_requested {
                    return Ok(false);
                }
                ensure!(
                    viewer.preview_jobs.as_ref().unwrap().held_jobs() == 4,
                    "waiting for capacity dropped a captured Task"
                );
                ensure!(
                    state.last_good.as_ref().unwrap().value == Some(9.),
                    "capacity waiting replaced last-good output"
                );
                self.stale = viewer.preview_jobs.as_ref().unwrap().stale;
                click(viewer, ViewControl::ReleaseHeld)?;
                self.phase = Phase::CapacityCompleted;
            }
            Phase::CapacityCompleted => {
                if !ready(viewer, 10., true)
                    || viewer.preview_jobs.as_ref().unwrap().pending_jobs() != 0
                {
                    return Ok(false);
                }
                ensure!(
                    !viewer.compile_requested
                        && viewer.preview_jobs.as_ref().unwrap().stale == self.stale + 4,
                    "the queued newest preview did not recover after original Tasks returned"
                );
                println!(
                    "NARA_GPUI_VIEWS_CAPACITY retained_jobs=4 automatic_retry=true stale_results=4"
                );
                self.old_a = a;
                self.old_b = b;
                click(viewer, ViewControl::CloseA)?;
                self.phase = Phase::ClosedA;
            }
            Phase::ShutdownArm => {
                click(viewer, ViewControl::Compile)?;
                self.phase = Phase::ShutdownHeld;
            }
            Phase::ShutdownHeld => {
                let jobs = viewer.preview_jobs.as_ref().unwrap();
                if jobs.held_jobs() != 1 || state.pending.is_none() {
                    return Ok(false);
                }
                viewer.expected_shutdown_stale = Some(jobs.stale + 1);
                println!("NARA_GPUI_VIEWS_CLOSE_PENDING pending_jobs=1 original_executor=true");
                return Ok(true);
            }
            Phase::ClosedA => {
                if viewer.engine.as_ref().unwrap().borrow().view_images()[0].is_some() {
                    return Ok(false);
                }
                let old = self.old_a.as_ref().unwrap();
                refuses(viewer, old)?;
                ensure!(
                    old.image.display.output().is_current(),
                    "closing one producer revoked a retained image in the live device epoch"
                );
                viewer
                    .engine
                    .as_ref()
                    .unwrap()
                    .borrow_mut()
                    .host
                    .with_image(old.image.display.output(), |_, _, _| ())?;
                self.b_frame = self
                    .old_b
                    .as_ref()
                    .unwrap()
                    .image
                    .display
                    .receipt()
                    .render_transaction_frame_index();
                click(viewer, ViewControl::PulseB)?;
                self.phase = Phase::PulsedB;
            }
            Phase::PulsedB => {
                if !ready(viewer, 10., false) {
                    return Ok(false);
                }
                let (retired, complete) = viewer
                    .engine
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .view_retirement_progress()?;
                ensure!(
                    retired == 1,
                    "closing A did not retain its original submitted prefix"
                );
                if !complete {
                    return Ok(false);
                }
                let b = b.unwrap();
                if b.image.display.receipt().render_transaction_frame_index() <= self.b_frame {
                    return Ok(false);
                }
                ensure!(a.is_none(), "closing A recreated its route");
                click_view(viewer, &b)?;
                self.phase = Phase::ClickedRemaining;
            }
            Phase::ClickedRemaining => {
                if !ready(viewer, 11., false) {
                    return Ok(false);
                }
                println!(
                    "NARA_GPUI_VIEWS_RETIRE one_producer_closed=true submitted_prefix_complete=true retained_output_passive=true other_view_continues=true"
                );
                click(viewer, ViewControl::HoldNext)?;
                self.phase = Phase::ArmedClose;
            }
            Phase::ArmedClose => {
                click(viewer, ViewControl::Compile)?;
                self.phase = Phase::HeldClose;
            }
            Phase::HeldClose => {
                if viewer.preview_jobs.as_ref().unwrap().held_jobs() != 1 || state.pending.is_none()
                {
                    return Ok(false);
                }
                self.good = state.last_good;
                self.old_b = b;
                self.stale = viewer.preview_jobs.as_ref().unwrap().stale;
                click(viewer, ViewControl::CloseGraph)?;
                self.phase = Phase::ClosedGraph;
            }
            Phase::ClosedGraph => {
                if state.current.is_some() {
                    return Ok(false);
                }
                ensure!(
                    viewer
                        .engine
                        .as_ref()
                        .unwrap()
                        .borrow()
                        .graph_document
                        .is_none(),
                    "the source did not close through its Host"
                );
                refuses(viewer, self.old_b.as_ref().unwrap())?;
                click(viewer, ViewControl::ReleaseHeld)?;
                self.phase = Phase::ReleasedClose;
            }
            Phase::ReleasedClose => {
                if viewer.preview_jobs.as_ref().unwrap().pending_jobs() != 0 {
                    return Ok(false);
                }
                ensure!(
                    state.current.is_none()
                        && state.last_good == self.good
                        && viewer.preview_jobs.as_ref().unwrap().stale == self.stale + 1,
                    "the closed document's task published a current result"
                );
                renderer.diagnostic_next.set(true);
                viewer.request()?;
                self.phase = Phase::ClosedPixels;
            }
            Phase::ClosedPixels => {
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                check_pixels(&diagnostic, &b.unwrap())?;
                println!(
                    "NARA_GPUI_VIEWS_LATE_CLOSE rejected=true last_good_passive_pixels=true returned_tasks=0"
                );
                return Ok(true);
            }
        }
        println!(
            "NARA_GPUI_VIEWS_PHASE next={:?} submitted={}",
            self.phase,
            renderer.submissions.get()
        );
        Ok(false)
    }
}

fn observations(viewer: &Viewer) -> [Option<Rc<ViewSnapshot>>; 2] {
    viewer.application.as_ref().unwrap().update(|cx| {
        let panel = viewer.views_panel.as_ref().unwrap().read(cx);
        [
            panel.presented_view(ViewKey::A),
            panel.presented_view(ViewKey::B),
        ]
    })
}

fn ready(viewer: &Viewer, value: f64, require_a: bool) -> bool {
    let graph = viewer.engine.as_ref().unwrap().borrow().graph_snapshot();
    if graph
        .ok()
        .and_then(|snapshot| evaluate(&snapshot.view).ok())
        != Some(Some(value))
    {
        return false;
    }
    let state = viewer.preview_jobs.as_ref().unwrap().state();
    if state.pending.is_some()
        || state
            .last_good
            .as_ref()
            .is_none_or(|good| Some(good.basis) != state.current || good.value != Some(value))
    {
        return false;
    }
    let current = state.last_good.unwrap();
    let views = observations(viewer);
    views.into_iter().enumerate().all(|(index, view)| {
        if index == 0 && !require_a {
            return view.is_none();
        }
        view.is_some_and(|view| {
            view.image.request == current.request
                && view.image.basis == current.basis
                && view.presented.borrow().as_ref().is_some_and(|presented| {
                    Rc::ptr_eq(presented.image(), &view.image)
                        && presented.mapping() == view.mapping
                        && presented.composite().image() == view.image.display.receipt()
                        && presented.composite().occurrence() == view.image.display.occurrence()
                })
        })
    })
}

fn click(viewer: &Viewer, control: ViewControl) -> Result<()> {
    let point = viewer
        .application
        .as_ref()
        .unwrap()
        .update(|cx| {
            viewer
                .views_panel
                .as_ref()
                .unwrap()
                .read(cx)
                .hit_center(control)
        })
        .ok_or_else(|| anyhow::anyhow!("View control {control:?} has no submitted bounds"))?;
    viewer.click(f32::from(point.x), f32::from(point.y))
}

fn center(snapshot: &ViewSnapshot) -> [f32; 2] {
    let bounds = snapshot.mapping.content_rect();
    [
        (bounds.min[0] + bounds.max[0]) / 2.,
        (bounds.min[1] + bounds.max[1]) / 2.,
    ]
}

fn click_view(viewer: &Viewer, snapshot: &ViewSnapshot) -> Result<()> {
    let point = center(snapshot);
    viewer.click(point[0], point[1])
}

fn refuses(viewer: &Viewer, snapshot: &ViewSnapshot) -> Result<()> {
    let presentation = snapshot
        .presented
        .borrow()
        .clone()
        .ok_or_else(|| anyhow::anyhow!("the prior View never obtained input authority"))?;
    ensure!(
        viewer
            .engine
            .as_ref()
            .unwrap()
            .borrow_mut()
            .activate_view(&presentation, center(snapshot))
            .is_err(),
        "a stale or closed View route edited the source"
    );
    Ok(())
}

fn check_pixels(diagnostic: &super::renderer::Diagnostic, snapshot: &ViewSnapshot) -> Result<()> {
    let point = center(snapshot);
    let pixel = diagnostic.logical_pixel(point[0], point[1]);
    for channel in 0..3 {
        let linear = snapshot.image.clear[channel];
        let encoded = if linear <= 0.0031308 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1. / 2.4) - 0.055
        };
        let expected = (encoded * 255.).round() as u8;
        ensure!(
            pixel[channel].abs_diff(expected) <= 3,
            "graph View {:?} pixels differ: {pixel:?}, expected channel {channel}={expected}",
            snapshot.image.key
        );
    }
    ensure!(pixel[3] == 255, "the opaque preview lost alpha");
    Ok(())
}
