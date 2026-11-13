use super::super::engine::{DIALOGUE_RESOURCE_PATH, DIALOGUE_RESOURCE_SCENE};
use super::*;

impl DialogueSmoke {
    pub(super) fn advance_resources(
        &mut self,
        viewer: &mut Viewer,
        snapshot: &DialogueSnapshot,
        frames: &[AcceptedFrame; 2],
    ) -> Result<bool> {
        let panel = viewer.dialogue_panel.as_ref().unwrap().clone();
        let views = viewer.application.as_ref().unwrap().update(|cx| {
            std::array::from_fn::<_, 2, _>(|index| panel.read(cx).resources[index].read(cx).view())
        });
        if views
            .iter()
            .any(|view| view.has_pending_work() || view.snapshot().is_none())
        {
            return Ok(false);
        }
        match self.resource_step {
            0 => {
                ensure!(
                    views[0].entry(DIALOGUE_RESOURCE_PATH).is_some(),
                    "original index omitted the CJK/long-path resource"
                );
                click(viewer, &frames[0], Hit::ResourceToggle)?;
                click(viewer, &frames[1], Hit::ResourceToggle)?;
            }
            1 => click(viewer, &frames[0], Hit::ResourceQuery)?,
            2 => replace(viewer, 0, Hit::ResourceQuery, "旅程说明")?,
            3 => {
                ensure!(
                    views[0].rows().len() == 1
                        && views[0].rows()[0].relative_path() == DIALOGUE_RESOURCE_PATH,
                    "original query lost the full CJK resource path"
                );
                click(viewer, &frames[0], Hit::ResourceRow(0))?;
            }
            4 => {
                ensure!(
                    views[0].selected_path() == Some(DIALOGUE_RESOURCE_PATH)
                        && views[1].selected_path().is_none()
                        && views[1].path_filter().is_empty(),
                    "resource Views share query or selection state"
                );
                ensure!(
                    !views[0]
                        .selected_entry()
                        .unwrap()
                        .open_capability()
                        .is_document(),
                    "an inspect-only resource acquired an Open capability"
                );
                click(viewer, &frames[1], Hit::ResourceQuery)?;
            }
            5 => replace(viewer, 1, Hit::ResourceQuery, "查找文档.scene.json")?,
            6 => {
                ensure!(
                    views[1].rows().len() == 1
                        && views[1].rows()[0].open_capability().is_document(),
                    "original index did not retain the Scene Open capability"
                );
                click(viewer, &frames[1], Hit::ResourceRow(0))?;
            }
            7 => {
                self.before = Some(snapshot.clone());
                click(viewer, &frames[1], Hit::ResourceOpen)?;
            }
            8 => {
                let status = viewer
                    .application
                    .as_ref()
                    .unwrap()
                    .update(|cx| panel.read(cx).resources[1].read(cx).status().to_owned());
                if status == "Opening selected document" {
                    return Ok(false);
                }
                ensure!(
                    status == "Document opened",
                    "the original asynchronous Open did not settle: {status}"
                );
                ensure!(
                    viewer
                        .engine
                        .as_ref()
                        .unwrap()
                        .borrow()
                        .dialogue_browser_result()
                        .is_some_and(|result| matches!(
                            result,
                            nara_engine::tooling::EditorDocumentOpenResult::Opened { .. }
                        )),
                    "resource UI has no original terminal Open result"
                );
                let (status, pending) = viewer
                    .engine
                    .as_ref()
                    .unwrap()
                    .borrow_mut()
                    .dialogue_open_resource(u64::MAX, "scenes/startup.scene.json")?;
                ensure!(
                    pending.is_none() && status.starts_with("Source index changed"),
                    "stale resource Open was not refused"
                );
                unchanged(snapshot, self.before.as_ref().unwrap())?;
                click(viewer, &frames[0], Hit::ResourceToggle)?;
                click(viewer, &frames[1], Hit::ResourceToggle)?;
            }
            9 => {
                ensure!(
                    views[0].selected_path() == Some(DIALOGUE_RESOURCE_PATH)
                        && views[0].path_filter() == "旅程说明"
                        && views[1].selected_path() == Some(DIALOGUE_RESOURCE_SCENE)
                        && views[1].path_filter() == "查找文档.scene.json",
                    "resource query/selection was lost after an original model publication or collapse"
                );
                unchanged(snapshot, self.before.as_ref().unwrap())?;
                println!(
                    "NARA_GPUI_DIALOGUE_RESOURCES original_index=true original_query=true cjk=true long_path=true independent_views=true inspect_only=true original_open=true async_terminal=true stale_index_refused=true authored_edits=0"
                );
            }
            _ => anyhow::bail!("unknown resource smoke phase"),
        }
        Ok(true)
    }
}
