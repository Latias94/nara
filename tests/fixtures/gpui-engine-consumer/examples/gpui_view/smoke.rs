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
    PanelsStable,
    PanelsFocusFirst,
    PanelsInvalid,
    PanelsFocusSecond,
    PanelsSecondText,
    PanelsFocusApply,
    PanelsApplied,
    PanelsUndo,
    PanelsLocality,
    PanelsLocalityUndo,
    PanelsRemoved,
    PanelsRedo,
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
    panel_hierarchy_renders: u64,
    panel_inspector_renders: [u64; 2],
    panel_revision: Option<nara_engine::scene::SceneAuthoringRevision>,
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
            panel_hierarchy_renders: 0,
            panel_inspector_renders: [0; 2],
            panel_revision: None,
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
                content_pixels(&diagnostic)?;
                ensure!(
                    renderer.submissions.get() > 0,
                    "diagnostic did not accompany a real Nara window submission"
                );
                self.initial = Some(snapshot);
                self.picks = viewer.picks;
                viewer.click(980., 60.)?;
                viewer.click(80., 128.)?;
                self.phase = Phase::Inputs;
            }
            Phase::Inputs => {
                ensure!(
                    viewer.engine.as_ref().unwrap().borrow().selected_sprite(),
                    "Hierarchy source row did not reach original selection"
                );
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
                self.panel_hierarchy_renders = viewer.panel_work.hierarchy_renders.get();
                self.panel_inspector_renders =
                    std::array::from_fn(|index| viewer.panel_work.inspector_renders[index].get());
                self.panel_revision = Some(viewer.engine.as_ref().unwrap().borrow().revision()?);
                self.phase = Phase::PanelsStable;
            }
            Phase::PanelsStable => {
                ensure!(
                    viewer.panel_work.hierarchy_renders.get() == self.panel_hierarchy_renders,
                    "unchanged Hierarchy rebuilt during a submitted redraw"
                );
                ensure!(
                    std::array::from_fn::<_, 2, _>(|index| viewer.panel_work.inspector_renders
                        [index]
                        .get())
                        == self.panel_inspector_renders,
                    "unchanged Inspector rebuilt during a submitted redraw"
                );
                viewer.focus_inspector(0)?;
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsFocusFirst;
            }
            Phase::PanelsFocusFirst if renderer.submissions.get() > self.submitted => {
                replace_inspector_text(viewer, 0, "not-a-number")?;
                viewer.click(970., 292.)?;
                ensure!(
                    viewer.commands.borrow().is_empty(),
                    "invalid numeric draft queued a source mutation"
                );
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsInvalid;
            }
            Phase::PanelsInvalid if renderer.submissions.get() > self.submitted => {
                ensure!(
                    viewer.engine.as_ref().unwrap().borrow().revision()?
                        == self.panel_revision.unwrap(),
                    "invalid draft mutated source history"
                );
                ensure!(
                    inspector_text(viewer, 0)? == "not-a-number"
                        && inspector_text(viewer, 1)? == "0",
                    "panel drafts are not independently owned"
                );
                viewer.focus_inspector(1)?;
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsFocusSecond;
            }
            Phase::PanelsFocusSecond if renderer.submissions.get() > self.submitted => {
                replace_inspector_text(viewer, 1, "stale draft")?;
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsSecondText;
            }
            Phase::PanelsSecondText if renderer.submissions.get() > self.submitted => {
                viewer.focus_inspector(0)?;
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsFocusApply;
            }
            Phase::PanelsFocusApply if renderer.submissions.get() > self.submitted => {
                replace_inspector_text(viewer, 0, "40")?;
                viewer.click(970., 292.)?;
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsApplied;
            }
            Phase::PanelsApplied => {
                let Some(snapshot) = renderer.presented.borrow().clone() else {
                    return Ok(false);
                };
                if snapshot.preview.revision == self.panel_revision.unwrap()
                    || renderer.submissions.get() <= self.submitted
                {
                    return Ok(false);
                }
                ensure!(
                    viewer.engine.as_ref().unwrap().borrow().sprite_x()? == 40.,
                    "Inspector did not commit through the source owner"
                );
                ensure!(
                    inspector_text(viewer, 0)? == "40"
                        && inspector_text(viewer, 1)? == "stale draft",
                    "source publication overwrote another panel's dirty draft"
                );
                let revision = viewer.engine.as_ref().unwrap().borrow().revision()?;
                viewer.click(970., 482.)?;
                ensure!(
                    viewer.commands.borrow().is_empty(),
                    "conflicted Inspector queued an unreviewed write"
                );
                ensure!(
                    viewer.engine.as_ref().unwrap().borrow().revision()? == revision,
                    "conflicted draft mutated the source"
                );
                viewer.click(970., 512.)?;
                ensure!(
                    inspector_text(viewer, 1)? == "40",
                    "Reload did not explicitly reset the stale draft"
                );
                viewer.click(260., 38.)?;
                self.panel_revision = Some(revision);
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsUndo;
            }
            Phase::PanelsUndo => {
                let Some(snapshot) = renderer.presented.borrow().clone() else {
                    return Ok(false);
                };
                if snapshot.preview.revision == self.panel_revision.unwrap()
                    || renderer.submissions.get() <= self.submitted
                {
                    return Ok(false);
                }
                ensure!(
                    viewer.engine.as_ref().unwrap().borrow().sprite_x()? == 0.,
                    "one original Undo did not restore the Inspector edit"
                );
                ensure!(
                    inspector_text(viewer, 0)? == "0" && inspector_text(viewer, 1)? == "0",
                    "Undo did not update both clean Inspector projections"
                );
                self.panel_hierarchy_renders = viewer.panel_work.hierarchy_renders.get();
                let snapshot = viewer.engine.as_ref().unwrap().borrow().panels_snapshot()?;
                self.panel_revision = Some(snapshot.basis.revision);
                viewer
                    .commands
                    .borrow_mut()
                    .push_back(super::Command::SetX {
                        basis: snapshot.basis,
                        entity: snapshot
                            .sprite
                            .expect("locality source still contains the Sprite"),
                        value: 16.,
                    });
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsLocality;
            }
            Phase::PanelsLocality => {
                let Some(snapshot) = renderer.presented.borrow().clone() else {
                    return Ok(false);
                };
                if snapshot.preview.revision == self.panel_revision.unwrap()
                    || renderer.submissions.get() <= self.submitted
                {
                    return Ok(false);
                }
                ensure!(
                    viewer.engine.as_ref().unwrap().borrow().sprite_x()? == 16.,
                    "source-locality control did not commit"
                );
                ensure!(
                    viewer.panel_work.hierarchy_renders.get() == self.panel_hierarchy_renders,
                    "isolated source property publication rebuilt unchanged Hierarchy rows"
                );
                ensure!(
                    inspector_text(viewer, 0)? == "16" && inspector_text(viewer, 1)? == "16",
                    "locality publication left stale Inspector values"
                );
                self.panel_revision = Some(snapshot.preview.revision);
                viewer.commands.borrow_mut().push_back(super::Command::Undo);
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsLocalityUndo;
            }
            Phase::PanelsLocalityUndo => {
                let Some(snapshot) = renderer.presented.borrow().clone() else {
                    return Ok(false);
                };
                if snapshot.preview.revision == self.panel_revision.unwrap()
                    || renderer.submissions.get() <= self.submitted
                {
                    return Ok(false);
                }
                ensure!(
                    viewer.engine.as_ref().unwrap().borrow().sprite_x()? == 0.,
                    "source-locality Undo did not restore original Scene"
                );
                println!(
                    "NARA_GPUI_STATE_LOCALITY hierarchy_renders={} inspector_renders={:?} projected_notifications={} shared_model=true independent_drafts=true conflict_refused=true undo=true isolated_source_publication_reused_hierarchy=true scope=caller_render_counts",
                    viewer.panel_work.hierarchy_renders.get(),
                    std::array::from_fn::<_, 2, _>(|index| viewer.panel_work.inspector_renders
                        [index]
                        .get()),
                    viewer.panel_work.projected_notifications.get()
                );
                viewer.commands.borrow_mut().push_back(super::Command::Undo);
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsRemoved;
            }
            Phase::PanelsRemoved => {
                let Some(snapshot) = renderer.presented.borrow().clone() else {
                    return Ok(false);
                };
                if snapshot.preview.revision == self.panel_revision.unwrap()
                    || renderer.submissions.get() <= self.submitted
                {
                    return Ok(false);
                }
                let panels = viewer.engine.as_ref().unwrap().borrow().panels_snapshot()?;
                ensure!(
                    panels.sprite.is_none() && panels.x.is_none(),
                    "removing the source Sprite did not produce an explicit absent Inspector state"
                );
                ensure!(
                    !viewer.finished,
                    "an absent source property closed the GPUI viewer"
                );
                viewer.commands.borrow_mut().push_back(super::Command::Redo);
                self.submitted = renderer.submissions.get();
                self.phase = Phase::PanelsRedo;
            }
            Phase::PanelsRedo => {
                let Some(snapshot) = renderer.presented.borrow().clone() else {
                    return Ok(false);
                };
                if snapshot.preview.revision == self.panel_revision.unwrap()
                    || renderer.submissions.get() <= self.submitted
                {
                    return Ok(false);
                }
                let panels = viewer.engine.as_ref().unwrap().borrow().panels_snapshot()?;
                ensure!(
                    panels.sprite.is_some()
                        && viewer.engine.as_ref().unwrap().borrow().sprite_x()? == 0.,
                    "Redo did not restore the source Sprite and Inspector property"
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
                    .request_inner_size(LogicalSize::new(1280., 640.));
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
                    != 1280
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
                content_pixels(&diagnostic)?;
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
                    "NARA_GPUI_VIEW_WORKFLOW pixels=true overlay=true selection=true ime=true deferred_input=true source_edit=true undo=true stale_pick_refused=true paint_error_panic_recovered=true resize_aba_zero=true fractional_root_scale=true close_pending=true source_cooked_content=true linear_srgb_composition=true diagnostic_scope=same_scene_offscreen_in_original_submission"
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

fn inspector_text(viewer: &Viewer, index: usize) -> Result<String> {
    viewer.application.as_ref().unwrap().update(|cx| {
        viewer.root.unwrap().update(cx, |root, _, cx| {
            let draft = root.inspectors[index].read(cx).draft.clone();
            draft.read(cx).text().to_owned()
        })
    })
}

fn replace_inspector_text(viewer: &mut Viewer, index: usize, value: &str) -> Result<()> {
    viewer
        .surface
        .as_ref()
        .unwrap()
        .dispatch_input(PlatformInput::KeyDown(KeyDownEvent {
            keystroke: Keystroke::parse("ctrl-a")?,
            is_held: false,
            prefer_character_input: false,
        }))?;
    viewer.input(WindowEvent::Ime(Ime::Commit(value.into())))?;
    ensure!(
        inspector_text(viewer, index)? == value,
        "Inspector native text route lost its draft"
    );
    Ok(())
}

fn content_pixels(image: &Diagnostic) -> Result<()> {
    let background = [64_u8; 3];
    for index in 0..2 {
        let y = 182. + index as f32 * 140.;
        let mut observed = Vec::new();
        for (band, alpha) in super::content::ALPHAS.into_iter().enumerate() {
            let x = 722. + band as f32 * 44.;
            let expected = content_composite(alpha, background);
            let actual = image.logical_pixel(x, y);
            near_pixel(
                actual,
                expected,
                "source/cooked content over linear background",
            )?;
            observed.push(actual);
        }
        let base = content_composite(super::content::ALPHAS[2], background);
        let alpha = 128. / 255.;
        let overlay = [32_u8, 128, 192];
        let mut expected = [255; 4];
        for channel in 0..3 {
            expected[channel] = encode_srgb(
                decode_srgb(f64::from(overlay[channel]) / 255.) * alpha
                    + decode_srgb(f64::from(base[channel]) / 255.) * (1. - alpha),
            );
        }
        near_pixel(
            image.logical_pixel(810., y - 16.),
            expected,
            "translucent UI overlay",
        )?;
        ensure!(
            image.logical_pixel(698., y) == image.logical_pixel(880., y),
            "content escaped its original rounded clip"
        );
        println!(
            "NARA_GPUI_CONTENT_PIXELS mode={} bands={observed:?} composition=linear_srgb",
            if index == 0 { "source" } else { "processed" }
        );
    }
    Ok(())
}

fn content_composite(alpha: u8, background: [u8; 3]) -> [u8; 4] {
    let coverage = f64::from(alpha) / 255.;
    let mut output = [255; 4];
    for channel in 0..3 {
        let color = decode_srgb(f64::from(super::content::RGB[channel]) / 255.);
        let accumulated = (color * coverage * 255.).round() / 255.;
        output[channel] = encode_srgb(
            accumulated + decode_srgb(f64::from(background[channel]) / 255.) * (1. - coverage),
        );
    }
    output
}

fn decode_srgb(encoded: f64) -> f64 {
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

fn encode_srgb(linear: f64) -> u8 {
    let encoded = if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1. / 2.4) - 0.055
    };
    (encoded.clamp(0., 1.) * 255.).round() as u8
}

fn near_pixel(actual: [u8; 4], expected: [u8; 4], context: &str) -> Result<()> {
    ensure!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= 2),
        "{context}: actual={actual:?}, expected={expected:?}"
    );
    Ok(())
}
