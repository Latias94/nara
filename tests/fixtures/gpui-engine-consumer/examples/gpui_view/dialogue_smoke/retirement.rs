use super::*;

impl DialogueSmoke {
    pub(super) fn advance_closed(
        &self,
        viewer: &mut Viewer,
        snapshot: &DialogueSnapshot,
        frame: &AcceptedFrame,
    ) -> Result<bool> {
        let panel = viewer.dialogue_panel.as_ref().unwrap().clone();
        viewer.application.as_ref().unwrap().update(|cx| {
            let panel = panel.read(cx);
            ensure!(
                panel.view_closed(0)
                    && !panel.view_closed(1)
                    && panel.accepted_view(0, cx).is_none(),
                "closing one View retired its peer or retained old input"
            );
            Ok::<_, anyhow::Error>(())
        })?;
        unchanged(snapshot, self.before.as_ref().unwrap())?;
        ensure!(
            viewer.dialogue_queue.borrow().is_empty(),
            "pre-close work was not drained"
        );
        match self.step {
            95 => {
                self.replay_callbacks(viewer)?;
                click(viewer, frame, Hit::Text)?;
            }
            96 => replace(viewer, 1, Hit::Text, PEER_TEXT)?,
            97 => {
                viewer.application.as_ref().unwrap().update(|cx| {
                    let draft = &panel.read(cx).editors[1].read(cx).draft;
                    ensure!(
                        draft.state() == DraftState::Editing && draft.value().text == PEER_TEXT,
                        "surviving View did not accept text into an Editing draft"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                click(viewer, frame, Hit::Cancel)?;
            }
            98 => {
                viewer.application.as_ref().unwrap().update(|cx| {
                    let draft = &panel.read(cx).editors[1].read(cx).draft;
                    let Some(NodeBody::Line { text, .. }) = snapshot
                        .view
                        .node(draft.node())
                        .map(|node| node.body.as_ref())
                    else {
                        anyhow::bail!("the surviving property target is not the original line");
                    };
                    ensure!(
                        draft.state() == DraftState::Clean && &draft.value().text == text,
                        "surviving View did not Cancel to the original text and Clean state"
                    );
                    Ok::<_, anyhow::Error>(())
                })?;
                self.replay_callbacks(viewer)?;
                self.queue_before_close(viewer, frame)?;
                click(viewer, frame, Hit::CloseView)?;
            }
            _ => anyhow::bail!("unknown Dialogue retirement phase"),
        }
        Ok(false)
    }

    pub(super) fn queue_before_close(
        &self,
        viewer: &mut Viewer,
        frame: &AcceptedFrame,
    ) -> Result<()> {
        ensure!(
            viewer.dialogue_queue.borrow().is_empty(),
            "retirement setup has unrelated queued work"
        );
        let text = frame.text_callback(Hit::Text)?;
        let apply = frame.activate_callback(Hit::Apply)?;
        viewer.application.as_ref().unwrap().update(|cx| {
            viewer.root.unwrap().update(cx, |_, window, cx| {
                text("Queued before close\n必须退休".into(), window, cx);
                apply(window, cx);
            })
        })?;
        ensure!(
            viewer.dialogue_queue.borrow().len() == 1,
            "accepted Apply did not queue original work"
        );
        click(viewer, frame, Hit::Undo)?;
        ensure!(
            viewer.dialogue_queue.borrow().len() == 2,
            "ordinary Undo did not queue before close"
        );
        Ok(())
    }

    pub(super) fn finish_retirement(
        &self,
        viewer: &Viewer,
        snapshot: &DialogueSnapshot,
    ) -> Result<bool> {
        ensure!(
            viewer.dialogue_queue.borrow().is_empty(),
            "last View's pre-close work survived draining"
        );
        unchanged(snapshot, self.before.as_ref().unwrap())?;
        let panel = viewer.dialogue_panel.as_ref().unwrap();
        viewer
            .application
            .as_ref()
            .unwrap()
            .update(|cx| panel.read(cx).verify_retired(cx))?;
        println!(
            "NARA_GPUI_DIALOGUE_OLD_CALLBACKS cancel=true reopen=true close=true queued_edits=0 original_accepted_callbacks=true queued_before_close_edit=true queued_before_close_action=true"
        );
        println!(
            "NARA_GPUI_DIALOGUE_RETIRE views_closed=2 document_open=true peer_remained_editable=true cancel_restored_text=true"
        );
        Ok(true)
    }

    pub(super) fn assert_retired_node_actions(
        &self,
        viewer: &Viewer,
        frame: &AcceptedFrame,
    ) -> Result<()> {
        for hit in [Hit::SetEntry, Hit::RemoveNode] {
            let action = frame.activate_callback(hit)?;
            viewer.application.as_ref().unwrap().update(|cx| {
                viewer
                    .root
                    .unwrap()
                    .update(cx, |_, window, cx| action(window, cx))
            })?;
        }
        ensure!(
            viewer.dialogue_queue.borrow().is_empty(),
            "a retired node selection enqueued a node action"
        );
        Ok(())
    }
}
