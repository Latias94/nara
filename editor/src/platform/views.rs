use crate::workspace::{EditorPanelInstanceId, EditorPanelWorkspace, EditorRenderViewId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PanelRenderView {
    panel: EditorPanelInstanceId,
    render: EditorRenderViewId,
}

/// Host-owned view lifetimes. Hiding or replacing a panel retires its render identity.
#[derive(Default)]
pub(super) struct EditorViewInstances {
    next: u64,
    workspace_generation: Option<u64>,
    authoring: Option<PanelRenderView>,
    game: Option<PanelRenderView>,
}

impl EditorViewInstances {
    pub(super) fn synchronize(
        &mut self,
        workspace: &mut EditorPanelWorkspace,
        generation: Option<u64>,
    ) -> Result<(), &'static str> {
        if self.workspace_generation != generation {
            self.authoring = None;
            self.game = None;
            self.workspace_generation = generation;
        }
        if generation.is_none() {
            self.authoring = None;
            self.game = None;
            let panels = workspace
                .layout()
                .instances()
                .iter()
                .filter(|panel| matches!(panel.kind(), "nara.authoring-view" | "nara.game-view"))
                .map(|panel| panel.id())
                .collect::<Vec<_>>();
            for panel in panels {
                workspace.layout_mut().set_render_view(panel, None);
            }
            return Ok(());
        }
        self.authoring = reconcile_panel(
            workspace,
            "nara.authoring-view",
            self.authoring,
            &mut self.next,
        )?;
        self.game = reconcile_panel(workspace, "nara.game-view", self.game, &mut self.next)?;
        Ok(())
    }

    pub(super) fn authoring(&self) -> Option<EditorRenderViewId> {
        self.authoring.map(|view| view.render)
    }

    pub(super) fn game(&self) -> Option<EditorRenderViewId> {
        self.game.map(|view| view.render)
    }
}

fn reconcile_panel(
    workspace: &mut EditorPanelWorkspace,
    kind: &str,
    current: Option<PanelRenderView>,
    next: &mut u64,
) -> Result<Option<PanelRenderView>, &'static str> {
    let panel = workspace
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == kind);
    let Some(panel) = panel else {
        return Ok(None);
    };
    let id = panel.id();
    if !panel.is_visible() {
        workspace.layout_mut().set_render_view(id, None);
        return Ok(None);
    }
    if let Some(current) = current
        && current.panel == id
        && panel.render_view() == Some(current.render)
    {
        return Ok(Some(current));
    }
    *next = next
        .checked_add(1)
        .ok_or("Editor render view identity exhausted")?;
    let view = PanelRenderView {
        panel: id,
        render: EditorRenderViewId(*next),
    };
    workspace
        .layout_mut()
        .set_render_view(id, Some(view.render));
    Ok(Some(view))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_and_reopening_one_view_preserves_the_other_and_never_reuses_identity() {
        let mut workspace = EditorPanelWorkspace::default();
        let mut views = EditorViewInstances::default();
        views.synchronize(&mut workspace, Some(1)).unwrap();
        let authoring = views.authoring().unwrap();
        let game = views.game().unwrap();
        assert_ne!(authoring, game);
        let panel = workspace
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "nara.game-view")
            .unwrap()
            .id();
        workspace.layout_mut().close(panel);
        views.synchronize(&mut workspace, Some(1)).unwrap();
        assert_eq!(views.authoring(), Some(authoring));
        assert_eq!(views.game(), None);
        workspace.layout_mut().reopen(panel);
        views.synchronize(&mut workspace, Some(1)).unwrap();
        assert_eq!(views.authoring(), Some(authoring));
        assert_ne!(views.game(), Some(game));
        let reopened = views.game();
        workspace.reset();
        views.synchronize(&mut workspace, Some(1)).unwrap();
        assert_ne!(views.authoring(), Some(authoring));
        assert_ne!(views.game(), reopened);
        let previous = views.authoring();
        views.synchronize(&mut workspace, Some(2)).unwrap();
        assert_ne!(views.authoring(), previous);
        let previous = (views.authoring(), views.game());
        views.synchronize(&mut workspace, None).unwrap();
        assert_eq!(views.authoring(), None);
        assert_eq!(views.game(), None);
        assert!(
            workspace
                .layout()
                .instances()
                .iter()
                .all(|panel| panel.render_view().is_none())
        );
        views.synchronize(&mut workspace, Some(3)).unwrap();
        assert_ne!(views.authoring(), previous.0);
        assert_ne!(views.game(), previous.1);
    }
}
