use super::{
    Viewer,
    dialogue_panel::{AcceptedFrame, ActivateCallback, Hit, PreviewPresentation, TextCallback},
};
use anyhow::{Context as _, Result, ensure};
use nara_gpui_engine_consumer::dialogue::{
    ChoiceId, NodeBody, NodeId, OutputPort,
    workflow::{DialogueSnapshot, DraftState},
};
use winit::{
    event::{ElementState, Ime, WindowEvent},
    keyboard::NamedKey,
};

#[path = "dialogue_smoke/input.rs"]
mod input;
use input::*;
#[path = "dialogue_smoke/persistence.rs"]
mod persistence;
#[path = "dialogue_smoke/resources.rs"]
mod resources;
#[path = "dialogue_smoke/retirement.rs"]
mod retirement;

const TEXT: &str = "Accepted multiline text\n第二行：森林 🦀\n\nTrailing newline\n";
const NODE_TEXT: &str = "Node entry\n节点编辑";
const PROMPT: &str = "Edited prompt\n修改后的问题";
const LABEL: &str = "Edited branch\n新分支";
const KEYBOARD_TEXT: &str = "aa\naa";
const PEER_TEXT: &str = "Peer remains editable\n另一个视图";

pub(super) struct DialogueSmoke {
    step: u8,
    resource_step: u8,
    submitted: u64,
    initial: Option<DialogueSnapshot>,
    edited: Option<DialogueSnapshot>,
    before: Option<DialogueSnapshot>,
    saved: Option<DialogueSnapshot>,
    choice_ids: Vec<ChoiceId>,
    created: Option<(NodeId, u64)>,
    old_text: Option<TextCallback>,
    old_apply: Option<ActivateCallback>,
}

impl DialogueSmoke {
    pub fn new() -> Self {
        Self {
            step: 0,
            resource_step: 0,
            submitted: 0,
            initial: None,
            edited: None,
            before: None,
            saved: None,
            choice_ids: Vec::new(),
            created: None,
            old_text: None,
            old_apply: None,
        }
    }

    pub fn advance(&mut self, viewer: &mut Viewer) -> Result<bool> {
        let renderer = viewer.renderer.as_ref().unwrap().clone();
        let surface = viewer.surface.as_ref().unwrap().clone();
        if viewer.native_geometry_events == 0
            || renderer.pending()
            || surface.needs_frame()
            || renderer.submissions.get() <= self.submitted
        {
            return Ok(false);
        }
        let panel = viewer
            .dialogue_panel
            .as_ref()
            .context("Dialogue was not mounted")?
            .clone();
        let snapshot = viewer
            .engine
            .as_ref()
            .unwrap()
            .borrow()
            .dialogue_snapshot()?;
        if snapshot.persistence_pending {
            return Ok(false);
        }
        if self.step == 99 {
            return self.finish_retirement(viewer, &snapshot);
        }
        if self.step >= 95 {
            let frame = viewer
                .application
                .as_ref()
                .unwrap()
                .update(|cx| panel.read(cx).accepted_view(1, cx));
            let Some(frame) = frame else {
                return Ok(false);
            };
            let done = self.advance_closed(viewer, &snapshot, &frame)?;
            self.step += 1;
            self.submitted = renderer.submissions.get();
            viewer.request()?;
            return Ok(done);
        }
        let frames = viewer
            .application
            .as_ref()
            .unwrap()
            .update(|cx| panel.read(cx).accepted(cx));
        let Some(frames) = frames else {
            return Ok(false);
        };
        ensure!(
            frames.iter().all(|frame| frame.generation > 0),
            "Dialogue lacks accepted Window geometry"
        );
        if self.step == 0 && self.resource_step < 10 {
            if self.advance_resources(viewer, &snapshot, &frames)? {
                self.resource_step += 1;
            }
            self.submitted = renderer.submissions.get();
            viewer.request()?;
            return Ok(false);
        }
        println!(
            "NARA_GPUI_DIALOGUE_PHASE step={} generation={:?}",
            self.step,
            frames.each_ref().map(|frame| frame.generation)
        );
        let line = self
            .initial
            .as_ref()
            .unwrap_or(&snapshot)
            .view
            .entry()
            .context("Dialogue entry disappeared")?;
        if (78..=92).contains(&self.step) {
            unchanged(&snapshot, self.before.as_ref().unwrap())?;
        }
        match self.step {
            0 => {
                viewer.input(WindowEvent::Focused(true))?;
                ensure!(
                    snapshot.view.nodes().len() == 3 && snapshot.view.links().len() == 3,
                    "Dialogue seed has no real conversation"
                );
                self.initial = Some(snapshot.clone());
                renderer.diagnostic_next.set(true);
                println!(
                    "NARA_GPUI_DIALOGUE_FIRST_FRAME views=2 original_session=true graph_canvas=unsupported"
                );
            }
            1 => {
                let Some(pixels) = renderer.take_diagnostic()? else {
                    return Ok(false);
                };
                let colors = pixels
                    .rgba
                    .chunks_exact(4)
                    .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                    .collect::<std::collections::BTreeSet<_>>();
                ensure!(
                    colors.len() > 16,
                    "Dialogue readback contains no actual text/controls"
                );
                if let Some(directory) = std::env::var_os("NARA_GPUI_DIALOGUE_CAPTURE_DIR") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory)?;
                    let mut ppm =
                        format!("P6\n{} {}\n255\n", pixels.width, pixels.height).into_bytes();
                    for pixel in pixels.rgba.chunks_exact(4) {
                        ppm.extend_from_slice(&pixel[..3]);
                    }
                    std::fs::write(directory.join("dialogue-accepted.ppm"), ppm)?;
                }
                println!(
                    "NARA_GPUI_DIALOGUE_PIXELS width={} height={} colors={} accepted_frame=true",
                    pixels.width,
                    pixels.height,
                    colors.len()
                );
                click(viewer, &frames[0], Hit::Text)?;
            }
            2 => replace(viewer, 0, Hit::Text, "Cancel this\n未提交")?,
            3 => {
                self.retain_callbacks(&frames[0])?;
                click(viewer, &frames[0], Hit::Cancel)?;
            }
            4 => {
                unchanged(&snapshot, self.initial.as_ref().unwrap())?;
                self.replay_callbacks(viewer)?;
                viewer.application.as_ref().unwrap().update(|cx| {
                    ensure!(
                        panel.read(cx).editors[0].read(cx).draft.state() == DraftState::Clean,
                        "old accepted text callback changed the cancelled draft"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                println!("NARA_GPUI_DIALOGUE_CANCEL authored_edits=0");
                click(viewer, &frames[0], Hit::Text)?;
            }
            5 => {
                viewer.input(WindowEvent::Ime(Ime::Preedit(
                    "候选".into(),
                    Some((0, "候选".len())),
                )))?;
                click(viewer, &frames[0], Hit::Apply)?;
            }
            6 => {
                unchanged(&snapshot, self.initial.as_ref().unwrap())?;
                viewer.input(WindowEvent::Ime(Ime::Disabled))?;
                click(viewer, &frames[0], Hit::Text)?;
            }
            7 => replace(viewer, 0, Hit::Text, TEXT)?,
            8 => click(viewer, &frames[0], Hit::Apply)?,
            9 => {
                ensure!(
                    matches!(snapshot.view.node(line).unwrap().body.as_ref(), NodeBody::Line { text, .. } if text == TEXT),
                    "Apply lost multiline/CJK bytes"
                );
                ensure!(
                    snapshot.history.undo_depth
                        == self.initial.as_ref().unwrap().history.undo_depth + 1,
                    "Apply did not create exactly one original history entry"
                );
                self.edited = Some(snapshot.clone());
                println!("NARA_GPUI_DIALOGUE_APPLY multiline=true cjk=true history_entries=1");
                click(viewer, &frames[1], Hit::Text)?;
            }
            10 => replace(viewer, 1, Hit::Text, "View B local proposal\n保留草稿")?,
            11 => {
                unchanged(&snapshot, self.edited.as_ref().unwrap())?;
                click(viewer, &frames[0], Hit::NativeText(line))?;
            }
            12 => replace(viewer, 0, Hit::NativeText(line), NODE_TEXT)?,
            13 => click(viewer, &frames[0], Hit::NativeApply(line))?,
            14 => {
                ensure!(
                    snapshot.history.undo_depth
                        == self.edited.as_ref().unwrap().history.undo_depth + 1,
                    "node edit bypassed original history"
                );
                viewer.application.as_ref().unwrap().update(|cx| {
                    let editor = panel.read(cx).editors[1].read(cx);
                    ensure!(
                        editor.draft.state() == DraftState::Conflict
                            && editor.draft.value().text == "View B local proposal\n保留草稿",
                        "another View erased the independent draft"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                self.before = Some(snapshot.clone());
                click(viewer, &frames[1], Hit::Undo)?;
            }
            15 => {
                ensure!(
                    snapshot.view == self.edited.as_ref().unwrap().view,
                    "cross-entry Undo did not restore source"
                );
                click(viewer, &frames[0], Hit::Redo)?;
            }
            16 => {
                ensure!(
                    snapshot.view == self.before.as_ref().unwrap().view,
                    "cross-entry Redo did not restore node edit"
                );
                self.before = Some(snapshot.clone());
                click(viewer, &frames[1], Hit::Cancel)?;
                println!(
                    "NARA_GPUI_DIALOGUE_HISTORY shared=true cross_entry_undo_redo=true drafts_independent=true"
                );
            }
            17 => click(viewer, &frames[1], Hit::ReadOnly)?,
            18 => {
                viewer.application.as_ref().unwrap().update(|cx| {
                    ensure!(
                        !panel.read(cx).editors[1].read(cx).draft.editable(),
                        "read-only draft remained editable"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                click(viewer, &frames[1], Hit::ReadOnly)?;
                click(viewer, &frames[0], Hit::X)?;
            }
            19 => replace(viewer, 0, Hit::X, "NaN")?,
            20 => click(viewer, &frames[0], Hit::ApplyPosition)?,
            21 => {
                unchanged(&snapshot, self.before.as_ref().unwrap())?;
                viewer.application.as_ref().unwrap().update(|cx| {
                    ensure!(
                        panel.read(cx).editors[0].read(cx).draft.error().is_some(),
                        "invalid number has no field feedback"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                click(viewer, &frames[0], Hit::Cancel)?;
                click(viewer, &frames[1], Hit::NodeSelector)?;
            }
            22 => {
                if !popup_settled(viewer, 1, 1)? {
                    return Ok(false);
                }
                key(viewer, NamedKey::Escape)?;
            }
            23 => {
                if !popup_settled(viewer, 1, 0)? {
                    return Ok(false);
                }
                click(viewer, &frames[1], Hit::NodeSelector)?;
            }
            24 => {
                if !popup_settled(viewer, 1, 1)? {
                    return Ok(false);
                }
                let at = frames[0].center(Hit::GraphUnavailable)?;
                mouse(viewer, at, ElementState::Pressed)?;
                mouse(viewer, at, ElementState::Released)?;
            }
            25 => {
                if !popup_settled(viewer, 1, 0)? {
                    return Ok(false);
                }
                unchanged(&snapshot, self.before.as_ref().unwrap())?;
                click(viewer, &frames[1], Hit::NodeSelector)?;
            }
            26 => {
                if !popup_settled(viewer, 1, 1)? {
                    return Ok(false);
                }
                key(viewer, NamedKey::End)?;
            }
            27 => key(viewer, NamedKey::Enter)?,
            28 => {
                if !popup_settled(viewer, 1, 0)? {
                    return Ok(false);
                }
                viewer.application.as_ref().unwrap().update(|cx| {
                    let node = panel.read(cx).editors[1].read(cx).draft.node();
                    ensure!(
                        matches!(
                            snapshot.view.node(node).map(|node| node.body.as_ref()),
                            Some(NodeBody::End { .. })
                        ),
                        "popup keyboard selection did not bind the ending"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                click(viewer, &frames[1], Hit::NodeSelector)?;
            }
            29 => key(viewer, NamedKey::Home)?,
            30 => key(viewer, NamedKey::Enter)?,
            31..=39 => {
                if !self.persistence_preview(viewer, &snapshot, &frames, line)? {
                    return Ok(false);
                }
            }
            40 => click(viewer, &frames[0], Hit::X)?,
            41 => replace(viewer, 0, Hit::X, "120.5")?,
            42 => click(viewer, &frames[0], Hit::ApplyPosition)?,
            43 => {
                ensure!(
                    snapshot.view.node(line).unwrap().position[0] == 120.5
                        && snapshot.history.undo_depth == 1,
                    "position did not use one original history entry"
                );
                click(viewer, &frames[1], Hit::Undo)?;
            }
            44 => {
                self.assert_saved(&snapshot)?;
                println!(
                    "NARA_GPUI_DIALOGUE_POSITION apply=true cross_view_undo=true graph_drag=unsupported"
                );
                click(viewer, &frames[0], Hit::NodeSelector)?;
            }
            45 => key(viewer, NamedKey::Home)?,
            46 => key(viewer, NamedKey::ArrowDown)?,
            47 => key(viewer, NamedKey::Enter)?,
            48 => {
                self.choice_ids = choice_ids(&snapshot)?;
                viewer.application.as_ref().unwrap().update(|cx| {
                    let node = panel.read(cx).editors[0].read(cx).draft.node();
                    ensure!(
                        matches!(
                            snapshot.view.node(node).unwrap().body.as_ref(),
                            NodeBody::Choice { .. }
                        ),
                        "properties did not select Choice"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                click(viewer, &frames[0], Hit::Text)?;
            }
            49 => replace(viewer, 0, Hit::Text, PROMPT)?,
            50 => click(viewer, &frames[0], Hit::ChoiceLabel(0))?,
            51 => replace(viewer, 0, Hit::ChoiceLabel(0), LABEL)?,
            52 => click(viewer, &frames[0], Hit::AddChoice)?,
            53 => {
                self.assert_saved(&snapshot)?;
                viewer.application.as_ref().unwrap().update(|cx| {
                    let editor = panel.read(cx).editors[0].read(cx);
                    ensure!(
                        editor.draft.value().choices.len() == 3
                            && editor.draft.value().choices[2].id.is_none(),
                        "Add choice did not retain an unallocated local draft"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                click(viewer, &frames[0], Hit::ChoiceUp(2))?;
            }
            54 => {
                viewer.application.as_ref().unwrap().update(|cx| {
                    ensure!(
                        panel.read(cx).editors[0].read(cx).draft.value().choices[1]
                            .id
                            .is_none(),
                        "choice reorder lost the local choice"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                click(viewer, &frames[0], Hit::ChoiceRemove(1))?;
            }
            55 => click(viewer, &frames[0], Hit::ChoiceTarget(0))?,
            56 => key(viewer, NamedKey::Home)?,
            57 => key(viewer, NamedKey::ArrowDown)?,
            58 => key(viewer, NamedKey::Enter)?,
            59 => click(viewer, &frames[0], Hit::Apply)?,
            60 => {
                let node = choice_node(&snapshot)?;
                ensure!(
                    matches!(snapshot.view.node(node).unwrap().body.as_ref(), NodeBody::Choice { prompt, choices } if prompt == PROMPT && choices[0].label == LABEL && choices.iter().map(|choice| choice.id).collect::<Vec<_>>() == self.choice_ids),
                    "choice Apply lost prompt, label or stable IDs"
                );
                ensure!(
                    snapshot.view.target(OutputPort::Choice {
                        node,
                        choice: self.choice_ids[0]
                    }) == Some(line)
                        && snapshot.history.undo_depth == 1,
                    "choice target bypassed atomic original history"
                );
                self.before = Some(snapshot.clone());
                click(viewer, &frames[1], Hit::Undo)?;
            }
            61 => {
                self.assert_saved(&snapshot)?;
                click(viewer, &frames[1], Hit::Redo)?;
            }
            62 => {
                ensure!(
                    snapshot.view == self.before.as_ref().unwrap().view,
                    "choice Redo changed the accepted result"
                );
                click(viewer, &frames[0], Hit::Undo)?;
            }
            63 => {
                self.assert_saved(&snapshot)?;
                println!(
                    "NARA_GPUI_DIALOGUE_CHOICES prompt=true label=true target=true reorder=true add_remove=true stable_ids=true shared_history=true"
                );
                click(viewer, &frames[0], Hit::CreateEnd)?;
            }
            64 => {
                ensure!(
                    snapshot.view.nodes().len() == 4 && snapshot.history.undo_depth == 1,
                    "Create did not reach the original owner"
                );
                let node = snapshot.view.nodes().map(|node| node.id).max().unwrap();
                self.created = Some((node, snapshot.view.node_incarnation(node).unwrap()));
                click(viewer, &frames[0], Hit::NodeSelect(node))?;
            }
            65 => {
                ensure!(
                    frames[0].node == self.created.unwrap().0 && frames[1].node == line,
                    "Views shared node selection"
                );
                click(viewer, &frames[0], Hit::SetEntry)?;
            }
            66 => {
                ensure!(
                    snapshot.view.entry() == Some(self.created.unwrap().0)
                        && snapshot.history.undo_depth == 2,
                    "Set entry missed the original owner"
                );
                click(viewer, &frames[0], Hit::RemoveNode)?;
            }
            67 => {
                ensure!(
                    snapshot.view.node(self.created.unwrap().0).is_none()
                        && snapshot.history.undo_depth == 3,
                    "Delete missed the original owner"
                );
                click(viewer, &frames[1], Hit::Undo)?;
            }
            68 => {
                let (node, incarnation) = self.created.unwrap();
                ensure!(
                    snapshot.view.entry() == Some(node)
                        && snapshot
                            .view
                            .node_incarnation(node)
                            .is_some_and(|current| current != incarnation),
                    "Undo did not restore the node with a fresh incarnation"
                );
                self.assert_retired_node_actions(viewer, &frames[0])?;
                click(viewer, &frames[0], Hit::Undo)?;
            }
            69 => {
                ensure!(
                    snapshot.view.entry() == Some(line),
                    "Undo did not restore the original entry"
                );
                click(viewer, &frames[0], Hit::Undo)?;
            }
            70 => {
                self.assert_saved(&snapshot)?;
                println!(
                    "NARA_GPUI_DIALOGUE_STRUCTURE create=true entry=true delete=true undo=true selection_independent=true"
                );
                click(viewer, &frames[0], Hit::NodeSelect(line))?;
            }
            71 => click(viewer, &frames[0], Hit::CreateEnd)?,
            72 => {
                ensure!(
                    snapshot.view.nodes().len() == 4 && snapshot.history.undo_depth == 1,
                    "recovery setup did not create a real node"
                );
                let node = snapshot.view.nodes().map(|node| node.id).max().unwrap();
                self.created = Some((node, snapshot.view.node_incarnation(node).unwrap()));
                click(viewer, &frames[0], Hit::NodeSelect(node))?;
            }
            73 => click(viewer, &frames[0], Hit::NodeSelector)?,
            74 => {
                if !popup_settled(viewer, 0, 1)? {
                    return Ok(false);
                }
                key(viewer, NamedKey::End)?;
            }
            75 => key(viewer, NamedKey::Enter)?,
            76 => {
                if !popup_settled(viewer, 0, 0)? {
                    return Ok(false);
                }
                viewer.application.as_ref().unwrap().update(|cx| {
                    ensure!(
                        panel.read(cx).editors[0].read(cx).draft.node() == self.created.unwrap().0,
                        "the property selector did not bind the node to be deleted"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                self.before = Some(snapshot.clone());
                self.retain_callbacks(&frames[0])?;
                click(viewer, &frames[0], Hit::RemoveNode)?;
            }
            77 => {
                ensure!(
                    snapshot.view.node(self.created.unwrap().0).is_none()
                        && snapshot.history.undo_depth
                            == self.before.as_ref().unwrap().history.undo_depth + 1,
                    "the property target was not deleted through the original owner"
                );
                self.replay_callbacks(viewer)?;
                viewer.application.as_ref().unwrap().update(|cx| {
                    ensure!(
                        panel.read(cx).editors[0].read(cx).draft.state() == DraftState::Retired,
                        "deleting the property target did not retire its draft"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                self.before = Some(snapshot.clone());
                click(viewer, &frames[0], Hit::NodeSelector)?;
            }
            78 => {
                if !popup_settled(viewer, 0, 1)? {
                    return Ok(false);
                }
                key(viewer, NamedKey::Home)?;
            }
            79 => key(viewer, NamedKey::Enter)?,
            80 => {
                if !popup_settled(viewer, 0, 0)? {
                    return Ok(false);
                }
                self.replay_callbacks(viewer)?;
                viewer.application.as_ref().unwrap().update(|cx| {
                    let editor = panel.read(cx).editors[0].read(cx);
                    ensure!(
                        editor.draft.node() == line
                            && editor.draft.state() == DraftState::Clean
                            && editor.draft.value().text == NODE_TEXT,
                        "retired node callbacks changed the recovered property draft"
                    );
                    ensure!(
                        panel.read(cx).node_editors[0].read(cx).draft.state()
                            == DraftState::Retired,
                        "property recovery silently rebound the independent node editor"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                click(viewer, &frames[0], Hit::Text)?;
            }
            81 => replace(viewer, 0, Hit::Text, "")?,
            82 => assert_textarea(viewer, 0, "", 0)?,
            83 => type_text(viewer, "aa")?,
            84 => {
                assert_textarea(viewer, 0, "aa", 2)?;
                key(viewer, NamedKey::Enter)?;
            }
            85 => {
                assert_textarea(viewer, 0, "aa\n", 3)?;
                type_text(viewer, "aa")?;
            }
            86 => {
                assert_textarea(viewer, 0, KEYBOARD_TEXT, 5)?;
                key(viewer, NamedKey::ArrowLeft)?;
            }
            87 => {
                assert_textarea(viewer, 0, KEYBOARD_TEXT, 4)?;
                key(viewer, NamedKey::Backspace)?;
            }
            88 => {
                assert_textarea(viewer, 0, "aa\na", 3)?;
                key(viewer, NamedKey::ArrowRight)?;
            }
            89 => {
                assert_textarea(viewer, 0, "aa\na", 4)?;
                type_text(viewer, "a")?;
            }
            90 => {
                assert_textarea(viewer, 0, KEYBOARD_TEXT, 5)?;
                key(viewer, NamedKey::ArrowUp)?;
            }
            91 => {
                assert_textarea(viewer, 0, KEYBOARD_TEXT, 2)?;
                key(viewer, NamedKey::ArrowDown)?;
            }
            92 => {
                assert_textarea(viewer, 0, KEYBOARD_TEXT, 5)?;
                click(viewer, &frames[0], Hit::Apply)?;
            }
            93 => {
                ensure!(
                    snapshot.view.node(self.created.unwrap().0).is_none()
                        && matches!(snapshot.view.node(line).unwrap().body.as_ref(), NodeBody::Line { text, .. } if text == KEYBOARD_TEXT)
                        && snapshot.history.undo_depth
                            == self.before.as_ref().unwrap().history.undo_depth + 1,
                    "property recovery did not Apply to the live node without undoing deletion"
                );
                println!(
                    "NARA_GPUI_DIALOGUE_RECOVER_TARGET deleted=true undo=false selector=true apply_other=true retired_callbacks_rejected=true"
                );
                println!(
                    "NARA_GPUI_DIALOGUE_KEYBOARD route=winit_converter enter=true backspace=true arrows=4 ordinary_text=true history_entries=1 clipboard=unqualified"
                );
            }
            94 => {
                self.retain_callbacks(&frames[0])?;
                self.before = Some(snapshot.clone());
                self.queue_before_close(viewer, &frames[0])?;
                click(viewer, &frames[0], Hit::CloseView)?;
            }
            _ => anyhow::bail!("unknown Dialogue smoke phase"),
        }
        self.step += 1;
        self.submitted = renderer.submissions.get();
        viewer.request()?;
        Ok(false)
    }

    fn retain_callbacks(&mut self, frame: &AcceptedFrame) -> Result<()> {
        self.old_text = Some(frame.text_callback(Hit::Text)?);
        self.old_apply = Some(frame.activate_callback(Hit::Apply)?);
        Ok(())
    }

    fn replay_callbacks(&self, viewer: &Viewer) -> Result<()> {
        viewer.application.as_ref().unwrap().update(|cx| {
            viewer.root.unwrap().update(cx, |_, window, cx| {
                self.old_text.as_ref().unwrap()(
                    "Retired accepted callback must not change input".into(),
                    window,
                    cx,
                );
                self.old_apply.as_ref().unwrap()(window, cx);
            })
        })?;
        ensure!(
            viewer.dialogue_queue.borrow().is_empty(),
            "retired accepted callbacks enqueued an edit"
        );
        Ok(())
    }

    fn assert_saved(&self, snapshot: &DialogueSnapshot) -> Result<()> {
        ensure!(
            authored(&snapshot.view)? == authored(&self.saved.as_ref().unwrap().view)?
                && !snapshot.dirty,
            "Undo did not restore the saved authored source"
        );
        Ok(())
    }
}

pub(super) fn choice_node(snapshot: &DialogueSnapshot) -> Result<NodeId> {
    snapshot
        .view
        .nodes()
        .find_map(|node| matches!(node.body.as_ref(), NodeBody::Choice { .. }).then_some(node.id))
        .context("Choice node missing")
}

fn choice_ids(snapshot: &DialogueSnapshot) -> Result<Vec<ChoiceId>> {
    let node = choice_node(snapshot)?;
    let NodeBody::Choice { choices, .. } = snapshot.view.node(node).unwrap().body.as_ref() else {
        unreachable!()
    };
    Ok(choices.iter().map(|choice| choice.id).collect())
}

pub(super) fn authored(
    view: &nara_gpui_engine_consumer::dialogue::DialogueView,
) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(
        view.entry(),
        view.nodes().collect::<Vec<_>>(),
        view.links().collect::<Vec<_>>(),
    ))?)
}

fn unchanged(actual: &DialogueSnapshot, expected: &DialogueSnapshot) -> Result<()> {
    ensure!(
        actual.document == expected.document
            && actual.revision == expected.revision
            && actual.view == expected.view
            && actual.history == expected.history
            && actual.dirty == expected.dirty,
        "a local Dialogue operation changed original source/history"
    );
    Ok(())
}
