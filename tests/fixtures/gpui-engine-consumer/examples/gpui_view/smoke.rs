use super::{
    Viewer,
    renderer::{Diagnostic, Failure, Snapshot},
};
use anyhow::{Result, ensure};
use open_gpui::{
    DevicePixels, HostedSurfaceGeometry, KeyDownEvent, Keystroke, PlatformInput, px, size,
};
use std::rc::Rc;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Ime, WindowEvent},
    keyboard::{Key, NamedKey},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Initial,
    Inputs,
    LongField,
    RestoredField,
    StageChange,
    PendingInput,
    ResumeChange,
    Moved,
    Undo,
    Refused,
    RecoveredRefusal,
    Panicked,
    RecoveredPanic,
    StageResize,
    ResizeOut,
    ResizeBack,
    Resized,
    Scaled,
    StageClose,
    Close,
    Done,
}

pub(super) struct Smoke {
    phase: Phase,
    initial: Option<Rc<Snapshot>>,
    moved: Option<Rc<Snapshot>>,
    submitted: u64,
    discarded: u64,
    picks: u64,
    logged: Option<Phase>,
}

impl Smoke {
    pub fn new() -> Self {
        Self {
            phase: Phase::Initial,
            initial: None,
            moved: None,
            submitted: 0,
            discarded: 0,
            picks: 0,
            logged: None,
        }
    }

    pub fn advance(&mut self, viewer: &mut Viewer) -> Result<bool> {
        let renderer = viewer.renderer.as_ref().unwrap().clone();
        let surface = viewer.surface.as_ref().unwrap().clone();
        if self.logged != Some(self.phase) {
            println!(
                "NARA_GPUI_VIEW_PHASE {:?} submitted={} pending={} discarded={} demands={}",
                self.phase,
                renderer.submissions.get(),
                renderer.pending(),
                renderer.discarded.get(),
                surface.needs_frame()
            );
            self.logged = Some(self.phase);
            if matches!(self.phase, Phase::RecoveredPanic) {
                let facts = viewer.application.as_ref().unwrap().update(|cx| {
                    viewer
                        .root
                        .unwrap()
                        .update(cx, |_, window, _| window.presentation_facts())
                })?;
                println!("NARA_GPUI_VIEW_RECOVERY_FACTS {facts:?}");
            }
        }
        match self.phase {
            Phase::Initial => {
                let Some(snapshot) = renderer.presented.borrow().clone() else {
                    return Ok(false);
                };
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                initial_pixels(&diagnostic)?;
                ensure!(
                    renderer.submissions.get() > 0,
                    "diagnostic did not accompany a real Nara window submission"
                );
                self.initial = Some(snapshot);
                self.picks = viewer.picks;
                viewer.click(80., 128.)?;
                self.phase = Phase::Inputs;
            }
            Phase::Inputs => {
                ensure!(
                    viewer.clicks.get() == 1 && viewer.picks == self.picks,
                    "overlay input leaked into Scene picking"
                );
                viewer.click(344., 286.)?;
                viewer.click(200., 508.)?;
                viewer.input(WindowEvent::Ime(Ime::Preedit(
                    "组合🦀".into(),
                    Some((0, "组合🦀".len())),
                )))?;
                let composing = viewer
                    .application
                    .as_ref()
                    .unwrap()
                    .update(|cx| viewer.draft.as_ref().unwrap().read(cx).is_composing());
                ensure!(
                    composing,
                    "original GPUI receiver did not retain marked text"
                );
                viewer.input(WindowEvent::Ime(Ime::Commit("Nara 文本🦀".into())))?;
                let (text, composing) = viewer.application.as_ref().unwrap().update(|cx| {
                    let input = viewer.draft.as_ref().unwrap().read(cx);
                    (input.text().to_owned(), input.is_composing())
                });
                ensure!(
                    text == "Nara 文本🦀" && !composing,
                    "IME commit lost exact text or kept preedit: {text:?}"
                );
                viewer.input(WindowEvent::Ime(Ime::Preedit(
                    "cancelled".into(),
                    Some((9, 9)),
                )))?;
                viewer.input(WindowEvent::Ime(Ime::Disabled))?;
                let (text, composing) = viewer.application.as_ref().unwrap().update(|cx| {
                    let input = viewer.draft.as_ref().unwrap().read(cx);
                    (input.text().to_owned(), input.is_composing())
                });
                ensure!(
                    text == "Nara 文本🦀" && !composing,
                    "disabling IME retained cancelled preedit"
                );
                viewer.keyboard_input(
                    Key::Named(NamedKey::Space),
                    Some(" ".into()),
                    ElementState::Pressed,
                    false,
                )?;
                let text = viewer
                    .application
                    .as_ref()
                    .unwrap()
                    .update(|cx| viewer.draft.as_ref().unwrap().read(cx).text().to_owned());
                ensure!(
                    text == "Nara 文本🦀 ",
                    "ordinary space input was lost after IME cancellation"
                );
                let long = format!("{}🦀", "x".repeat(4092));
                ensure!(
                    long.len() == 4096,
                    "capacity probe must exercise the original 4096-byte requirement"
                );
                surface.dispatch_input(PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse("ctrl-a")?,
                    is_held: false,
                    prefer_character_input: false,
                }))?;
                viewer.input(WindowEvent::Ime(Ime::Commit(long)))?;
                self.submitted = renderer.submissions.get();
                renderer.diagnostic_next.set(true);
                viewer.request()?;
                self.phase = Phase::LongField;
            }
            Phase::LongField if renderer.submissions.get() > self.submitted => {
                let Some(_) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                let actual = viewer
                    .application
                    .as_ref()
                    .unwrap()
                    .update(|cx| viewer.draft.as_ref().unwrap().read(cx).text().to_owned());
                ensure!(
                    actual == format!("{}🦀", "x".repeat(4092)),
                    "actual presented field lost its 4096-byte buffer"
                );
                ensure!(
                    surface.text_input_state().caret_bounds.is_some(),
                    "long field did not publish its actual caret geometry"
                );
                surface.dispatch_input(PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse("ctrl-a")?,
                    is_held: false,
                    prefer_character_input: false,
                }))?;
                viewer.input(WindowEvent::Ime(Ime::Commit("Nara 文本🦀".into())))?;
                self.submitted = renderer.submissions.get();
                viewer.request()?;
                self.phase = Phase::RestoredField;
            }
            Phase::RestoredField if renderer.submissions.get() > self.submitted => {
                self.phase = Phase::StageChange;
            }
            Phase::StageChange => {
                ensure!(
                    viewer.picks > self.picks,
                    "presented Scene pixels did not route to the original Editor selection"
                );
                ensure!(
                    viewer.engine.as_ref().unwrap().borrow().selected_sprite(),
                    "actual Editor selection does not match the clicked sprite"
                );
                renderer.paused.set(true);
                viewer.click(80., 38.)?;
                self.phase = Phase::PendingInput;
            }
            Phase::PendingInput if renderer.pending() => {
                ensure!(
                    Rc::ptr_eq(
                        renderer.presented.borrow().as_ref().unwrap(),
                        self.initial.as_ref().unwrap()
                    ) || renderer
                        .presented
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .preview
                        .revision
                        == self.initial.as_ref().unwrap().preview.revision,
                    "pending Scene change replaced submitted interaction before submission"
                );
                self.discarded = renderer.discarded.get();
                viewer.click(80., 128.)?;
                surface
                    .with_input_handler(|handler| handler.replace_text_in_range(None, " pending"))?
                    .ok_or_else(|| {
                        anyhow::anyhow!("pending candidate removed submitted text input")
                    })?;
                viewer.click(50., 508.)?;
                surface
                    .with_input_handler(|handler| handler.replace_text_in_range(None, " keep"))?
                    .ok_or_else(|| {
                        anyhow::anyhow!("unpublished field geometry removed its original handler")
                    })?;
                viewer.request()?;
                self.phase = Phase::ResumeChange;
            }
            Phase::ResumeChange
                if renderer.pending() && renderer.discarded.get() > self.discarded =>
            {
                ensure!(
                    viewer.clicks.get() == 2,
                    "submitted overlay stopped responding behind a pending candidate"
                );
                let text = viewer
                    .application
                    .as_ref()
                    .unwrap()
                    .update(|cx| viewer.draft.as_ref().unwrap().read(cx).text().to_owned());
                ensure!(
                    text == "Nara 文本🦀 pending keep",
                    "submitted field was rebound while pending"
                );
                renderer.diagnostic_next.set(true);
                renderer.paused.set(false);
                self.phase = Phase::Moved;
            }
            Phase::Moved => {
                let Some(snapshot) = renderer.presented.borrow().clone() else {
                    return Ok(false);
                };
                if snapshot.preview.revision == self.initial.as_ref().unwrap().preview.revision {
                    return Ok(false);
                }
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                let old = self.initial.as_ref().unwrap();
                ensure!(
                    viewer
                        .engine
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .pick(&old.preview, old.mapping, [344., 286.])
                        .is_err(),
                    "old content/mapping still admitted a pick"
                );
                ensure!(
                    !is_red(diagnostic.logical_pixel(344., 286.)),
                    "source translation did not change the actual rendered preview"
                );
                self.moved = Some(snapshot);
                renderer.diagnostic_next.set(true);
                viewer.click(260., 38.)?;
                self.phase = Phase::Undo;
            }
            Phase::Undo => {
                let Some(snapshot) = renderer.presented.borrow().clone() else {
                    return Ok(false);
                };
                if snapshot.preview.revision == self.moved.as_ref().unwrap().preview.revision {
                    return Ok(false);
                }
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                ensure!(
                    is_red(diagnostic.logical_pixel(344., 286.)),
                    "original Session Undo did not restore actual sprite pixels"
                );
                self.submitted = renderer.submissions.get();
                renderer.failure.set(Some(Failure::Refuse));
                viewer.request()?;
                self.phase = Phase::Refused;
            }
            Phase::Refused if renderer.refusals.get() == 1 => {
                ensure!(
                    renderer.submissions.get() == self.submitted,
                    "paint refusal counted as successful presentation"
                );
                viewer.request()?;
                self.phase = Phase::RecoveredRefusal;
            }
            Phase::RecoveredRefusal if renderer.submissions.get() > self.submitted => {
                self.submitted = renderer.submissions.get();
                renderer.failure.set(Some(Failure::Panic));
                viewer.request()?;
                self.phase = Phase::Panicked;
            }
            Phase::Panicked if renderer.refusals.get() == 2 => {
                ensure!(
                    renderer.submissions.get() == self.submitted,
                    "paint panic counted as successful presentation"
                );
                viewer.request()?;
                self.phase = Phase::RecoveredPanic;
            }
            Phase::RecoveredPanic if renderer.submissions.get() > self.submitted => {
                renderer.paused.set(true);
                viewer.request()?;
                self.phase = Phase::StageResize;
            }
            Phase::StageResize if renderer.pending() => {
                self.discarded = renderer.discarded.get();
                let _ = viewer
                    .engine
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .window
                    .request_inner_size(LogicalSize::new(843., 620.));
                self.phase = Phase::ResizeOut;
            }
            Phase::ResizeOut => {
                if viewer
                    .engine
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .window
                    .inner_size()
                    .to_logical::<u32>(
                        viewer
                            .engine
                            .as_ref()
                            .unwrap()
                            .borrow()
                            .window
                            .scale_factor(),
                    )
                    .width
                    != 843
                {
                    return Ok(false);
                }
                let _ = viewer
                    .engine
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .window
                    .request_inner_size(LogicalSize::new(900., 640.));
                self.phase = Phase::ResizeBack;
            }
            Phase::ResizeBack => {
                if viewer
                    .engine
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .window
                    .inner_size()
                    .to_logical::<u32>(
                        viewer
                            .engine
                            .as_ref()
                            .unwrap()
                            .borrow()
                            .window
                            .scale_factor(),
                    )
                    .width
                    != 900
                {
                    return Ok(false);
                }
                let geometry = surface.geometry();
                surface.set_geometry(geometry.size(), geometry.scale_factor())?;
                surface.set_geometry(size(px(0.), px(0.)), geometry.scale_factor())?;
                surface.set_geometry(geometry.size(), geometry.scale_factor())?;
                renderer.paused.set(false);
                self.submitted = renderer.submissions.get();
                viewer.request()?;
                self.phase = Phase::Resized;
            }
            Phase::Resized if renderer.submissions.get() > self.submitted => {
                ensure!(
                    renderer.discarded.get() > self.discarded,
                    "resize/ABA retained an old pending candidate"
                );
                let physical = viewer.engine.as_ref().unwrap().borrow().window.inner_size();
                viewer.refresh_preview()?;
                let geometry = HostedSurfaceGeometry::from_physical_size(
                    size(
                        DevicePixels(physical.width.try_into()?),
                        DevicePixels(physical.height.try_into()?),
                    ),
                    1.25,
                )?;
                surface.set_geometry(geometry.size(), geometry.scale_factor())?;
                self.submitted = renderer.submissions.get();
                renderer.diagnostic_next.set(true);
                self.phase = Phase::Scaled;
            }
            Phase::Scaled if renderer.submissions.get() > self.submitted => {
                let Some(diagnostic) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                let extent = viewer.engine.as_ref().unwrap().borrow().window.inner_size();
                ensure!(
                    diagnostic.width == extent.width && diagnostic.height == extent.height,
                    "scaled root changed the actual Nara attachment"
                );
                let snapshot = renderer.presented.borrow().clone().unwrap();
                ensure!(
                    viewer
                        .engine
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .pick(&snapshot.preview, snapshot.mapping, [344., 286.])?
                        .is_some(),
                    "fractional root scale lost actual View picking"
                );
                let content = snapshot.mapping.content_rect();
                println!(
                    "NARA_GPUI_VIEW_SCALED extent={}x{} logical_content={content:?} scale=1.25 physical_dpi=unqualified",
                    diagnostic.width, diagnostic.height
                );
                renderer.paused.set(true);
                viewer.request()?;
                self.phase = Phase::StageClose;
            }
            Phase::StageClose if renderer.pending() => {
                let output = viewer.engine.as_ref().unwrap().borrow_mut().pulse_other()?;
                ensure!(
                    output.is_current(),
                    "independent producer failed before UI close"
                );
                self.phase = Phase::Close;
            }
            Phase::Close => {
                println!(
                    "NARA_GPUI_VIEW_WORKFLOW pixels=true overlay=true selection=true ime=true deferred_input=true source_edit=true undo=true stale_pick_refused=true paint_error_panic_recovered=true resize_aba_zero=true fractional_root_scale=true close_pending=true diagnostic_scope=same_scene_offscreen_in_original_submission"
                );
                self.phase = Phase::Done;
                return Ok(true);
            }
            Phase::Done => return Ok(true),
            _ => {}
        }
        Ok(false)
    }
}

fn is_red(pixel: [u8; 4]) -> bool {
    pixel[0] > 180 && pixel[1] < 80 && pixel[2] < 80 && pixel[3] > 240
}

fn initial_pixels(image: &Diagnostic) -> Result<()> {
    ensure!(
        is_red(image.logical_pixel(344., 286.)),
        "actual Scene sprite is absent from diagnostic: {:?}",
        image.logical_pixel(344., 286.)
    );
    let background = image.logical_pixel(850., 600.);
    let glyph_pixels = (72..94)
        .flat_map(|y| (24..550).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            image.logical_pixel(x as f32, y as f32) != background
                && image.logical_pixel(x as f32, y as f32)[0] > 100
        })
        .count();
    ensure!(glyph_pixels > 40, "actual GPUI glyph output is absent");
    ensure!(
        image.logical_pixel(50., 32.) != background,
        "actual GPUI button is absent"
    );
    ensure!(
        image.logical_pixel(35., 115.) != image.logical_pixel(35., 155.),
        "overlay fill is absent above the actual engine output"
    );
    ensure!(
        image.logical_pixel(24., 106.) == background,
        "rounded Scene View clip did not preserve the original UI background"
    );
    ensure!(
        !is_red(image.logical_pixel(80., 128.)),
        "overlay did not cover the engine image"
    );
    println!(
        "NARA_GPUI_VIEW_PIXELS center={:?} glyph_pixels={glyph_pixels}",
        image.logical_pixel(344., 286.)
    );
    Ok(())
}
