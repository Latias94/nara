use super::*;

impl DialogueSmoke {
    pub(super) fn persistence_preview(
        &mut self,
        viewer: &mut Viewer,
        snapshot: &DialogueSnapshot,
        frames: &[AcceptedFrame; 2],
        line: NodeId,
    ) -> Result<bool> {
        let panel = viewer.dialogue_panel.as_ref().unwrap().clone();
        match self.step {
            31 => {
                if !popup_settled(viewer, 1, 0)? {
                    return Ok(false);
                }
                viewer.application.as_ref().unwrap().update(|cx| {
                    ensure!(
                        panel.read(cx).editors[1].read(cx).draft.node() == line,
                        "popup did not restore the line editor"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                println!(
                    "NARA_GPUI_DIALOGUE_STATES readonly=true invalid=true ime_preedit_no_edit=true"
                );
                println!(
                    "NARA_GPUI_DIALOGUE_POPUP escape=true outside_dismiss=true keyboard_select=true"
                );
                click(viewer, &frames[0], Hit::Save)?;
            }
            32 => {
                if snapshot.dirty {
                    return Ok(false);
                }
                let saved = viewer
                    .engine
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .dialogue_saved_view()?;
                ensure!(
                    authored(&saved)? == authored(&snapshot.view)?,
                    "Save did not persist exact Dialogue bytes"
                );
                self.saved = Some(snapshot.clone());
                self.retain_callbacks(&frames[0])?;
                click(viewer, &frames[0], Hit::Reopen)?;
            }
            33 => {
                let saved = self.saved.as_ref().unwrap();
                if saved.same_source(snapshot) {
                    return Ok(false);
                }
                ensure!(
                    authored(&saved.view)? == authored(&snapshot.view)? && !snapshot.dirty,
                    "Reopen changed authored data"
                );
                viewer.application.as_ref().unwrap().update(|cx| {
                    for editor in &panel.read(cx).editors {
                        ensure!(
                            editor.read(cx).draft.state() == DraftState::Retired,
                            "Reopen revived an old draft"
                        );
                    }
                    Ok::<_, anyhow::Error>(())
                })?;
                self.replay_callbacks(viewer)?;
                self.assert_retired_node_actions(viewer, &frames[0])?;
                click(viewer, &frames[0], Hit::NextTarget)?;
                ensure!(
                    popup_settled(viewer, 0, 0)?,
                    "retired Next target opened a popup"
                );
                unchanged(
                    &viewer
                        .engine
                        .as_ref()
                        .unwrap()
                        .borrow()
                        .dialogue_snapshot()?,
                    snapshot,
                )?;
                println!(
                    "NARA_GPUI_DIALOGUE_PERSISTENCE save=true reopen=true old_drafts_retired=true"
                );
                click(viewer, &frames[0], Hit::PreviewRestart)?;
            }
            34 => {
                ensure!(
                    matches!(frames[0].preview.as_ref(), Some(PreviewPresentation::Line { node, text, .. }) if *node == line && text == NODE_TEXT),
                    "preview did not present the reopened line"
                );
                ensure!(
                    matches!(frames[1].preview, Some(PreviewPresentation::Unavailable(_))),
                    "the other View's preview was restarted implicitly"
                );
                click(viewer, &frames[0], Hit::PreviewNext)?;
            }
            35 => {
                ensure!(
                    matches!(frames[0].preview.as_ref(), Some(PreviewPresentation::Choice { choices, .. }) if choices.len() == 2),
                    "Next did not present the real choices"
                );
                click(
                    viewer,
                    &frames[0],
                    Hit::PreviewChoice(choice_ids(snapshot)?[1]),
                )?;
            }
            36 => {
                ensure!(
                    matches!(frames[0].preview.as_ref(), Some(PreviewPresentation::Line { node, .. }) if *node == line),
                    "loop choice did not return to the entry"
                );
                click(viewer, &frames[0], Hit::PreviewNext)?;
            }
            37 => {
                ensure!(
                    matches!(frames[0].preview, Some(PreviewPresentation::Choice { .. })),
                    "loop advance did not return to choices"
                );
                click(
                    viewer,
                    &frames[0],
                    Hit::PreviewChoice(choice_ids(snapshot)?[0]),
                )?;
            }
            38 => {
                ensure!(
                    matches!(frames[0].preview.as_ref(), Some(PreviewPresentation::End { text, .. }) if text == "You reached the forest.\n旅程结束。")
                        && frames[0].preview_error.is_none(),
                    "ending choice did not present the real ending"
                );
                ensure!(
                    snapshot.history.undo_depth == 0 && !snapshot.dirty,
                    "preview mutated history"
                );
                println!(
                    "NARA_GPUI_DIALOGUE_PREVIEW line=true choice=true loop=true ending=true authored_edits=0"
                );
                click(viewer, &frames[1], Hit::PreviewRestart)?;
            }
            39 => {
                ensure!(
                    matches!(frames[0].preview, Some(PreviewPresentation::End { .. }))
                        && matches!(frames[1].preview, Some(PreviewPresentation::Line { .. })),
                    "Views shared a preview cursor"
                );
                println!(
                    "NARA_GPUI_DIALOGUE_VIEWS drafts_independent=true preview_independent=true selection_local=true"
                );
                click(viewer, &frames[0], Hit::Rebind)?;
                click(viewer, &frames[1], Hit::Rebind)?;
            }
            _ => anyhow::bail!("unknown Dialogue persistence/preview phase"),
        }
        Ok(true)
    }
}
