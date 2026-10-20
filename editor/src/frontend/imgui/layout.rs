use std::collections::BTreeMap;

use dear_imgui_rs::{DockLayout, DockSplit, WindowKey};

use crate::workspace::{EditorPanelPlacement, EditorPanelWorkspace};

pub(super) fn toolbar_key() -> WindowKey {
    WindowKey::new("nara-project-actions", "Project actions")
        .expect("the toolbar has a constant valid native identity")
}

pub(super) fn default_layout(workspace: &EditorPanelWorkspace) -> Result<DockLayout, String> {
    let mut groups = BTreeMap::<EditorPanelPlacement, Vec<WindowKey>>::new();
    for instance in workspace
        .layout()
        .instances()
        .iter()
        .filter(|instance| instance.is_visible())
    {
        if instance.placement() == EditorPanelPlacement::Floating {
            continue;
        }
        let Some(descriptor) = workspace.registry().descriptor(instance.kind()) else {
            continue;
        };
        let key = WindowKey::new(
            format!("nara-panel-{}", instance.id().0),
            descriptor.title(),
        )
        .map_err(|error| error.to_string())?;
        groups.entry(instance.placement()).or_default().push(key);
    }

    let center = groups
        .remove(&EditorPanelPlacement::Center)
        .unwrap_or_default();
    let mut body = DockLayout::tabs(Vec::<WindowKey>::new());
    for (index, key) in center.into_iter().rev().enumerate() {
        let leaf = DockLayout::tabs([key]);
        body = if index == 0 {
            leaf
        } else {
            DockLayout::split(DockSplit::Up, 1.0 / (index + 1) as f32, leaf, body)
        };
    }
    for (placement, direction, ratio) in [
        (EditorPanelPlacement::Bottom, DockSplit::Down, 0.20),
        (EditorPanelPlacement::Right, DockSplit::Right, 0.25),
        (EditorPanelPlacement::Left, DockSplit::Left, 0.20),
    ] {
        if let Some(keys) = groups.remove(&placement) {
            body = DockLayout::split(direction, ratio, DockLayout::tabs(keys), body);
        }
    }
    Ok(DockLayout::split(
        DockSplit::Up,
        0.18,
        DockLayout::tabs([toolbar_key()]),
        body,
    ))
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;

    use dear_imgui_rs::{ConfigFlags, Context, DockLayoutApply, Id, sys};

    use crate::workspace::{EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelTarget};

    use super::*;

    fn context() -> Context {
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        let io = context.io_mut();
        io.set_display_size([1280.0, 960.0]);
        io.set_delta_time(1.0 / 60.0);
        io.set_config_flags(ConfigFlags::DOCKING_ENABLE);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context
    }

    fn render_workspace(
        context: &mut Context,
        workspace: &EditorPanelWorkspace,
        layout: &DockLayout,
        apply: DockLayoutApply,
    ) {
        let ui = context.frame();
        ui.dockspace().layout(layout, apply).build().unwrap();
        ui.window(&toolbar_key())
            .build(|| ui.text("Project actions"));
        for instance in workspace
            .layout()
            .instances()
            .iter()
            .filter(|instance| instance.is_visible())
        {
            let descriptor = workspace.registry().descriptor(instance.kind()).unwrap();
            let name =
                super::super::workspace::panel_window_name(descriptor.title(), instance.id());
            ui.window(name).build(|| ui.text(descriptor.title()));
        }
        let _ = context.render_legacy();
    }

    fn panel_name(workspace: &EditorPanelWorkspace, kind: &str) -> String {
        let instance = workspace
            .layout()
            .instances()
            .iter()
            .find(|instance| instance.kind() == kind)
            .unwrap();
        let descriptor = workspace.registry().descriptor(kind).unwrap();
        super::super::workspace::panel_window_name(descriptor.title(), instance.id())
    }

    struct WindowState {
        dock: Id,
        min: [f32; 2],
        max: [f32; 2],
        drawn: bool,
    }

    fn window_state(context: &Context, name: &str) -> WindowState {
        let name = CString::new(name).unwrap();
        context.binding().with_bound_context(|| {
            // SAFETY: the bound Context owns all rendered windows and draw lists. Only copied
            // identity, geometry and draw-presence facts leave this scope.
            unsafe {
                let window = sys::igFindWindowByName(name.as_ptr());
                assert!(!window.is_null(), "the submitted window must exist");
                let window = &*window;
                let draw_data = &*sys::igGetDrawData();
                let drawn = window.Active
                    && !window.Hidden
                    && (0..draw_data.CmdLists.Size).any(|index| {
                        *draw_data.CmdLists.Data.add(index as usize) == window.DrawList
                    });
                WindowState {
                    dock: Id::from(window.DockId),
                    min: [window.Pos.x, window.Pos.y],
                    max: [window.Pos.x + window.Size.x, window.Pos.y + window.Size.y],
                    drawn,
                }
            }
        })
    }

    #[test]
    fn complete_frontend_keeps_both_default_central_windows_visible() {
        use super::super::{ImguiEditorFrontend, interaction_tests::InteractionProject};
        use crate::{EditorApplication, EditorLaunchOptions};

        let project = InteractionProject::new();
        let mut application =
            EditorApplication::new(EditorLaunchOptions::new(&project.root).with_profile("desktop"));
        application.native_shell_ready();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if application.current_frame().is_some() {
                break;
            }
            application.advance_project_open();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(application.current_frame().is_some());
        let mut frontend = ImguiEditorFrontend::new();
        let mut context = context();
        for _ in 0..4 {
            let ui = context.frame();
            frontend.show(ui, &mut application).unwrap();
            let _ = context.render_legacy();
        }
        let workspace = application.panel_workspace();
        let authoring = window_state(&context, &panel_name(workspace, "nara.authoring-view"));
        let game = window_state(&context, &panel_name(workspace, "nara.game-view"));
        assert!(
            authoring.drawn && game.drawn,
            "both default central windows must paint"
        );
        assert_ne!(
            authoring.dock, game.dock,
            "central views must occupy independent leaves"
        );
        assert!(authoring.max[1] <= game.min[1]);
    }

    #[test]
    fn default_docks_show_both_central_views_and_route_registered_panels() {
        let mut workspace = EditorPanelWorkspace::default();
        workspace
            .open("nara.project-browser", EditorPanelTarget::ActiveDocument)
            .unwrap();
        for (kind, placement) in [
            ("external.details", EditorPanelPlacement::Right),
            ("external.floating", EditorPanelPlacement::Floating),
        ] {
            workspace
                .register(EditorPanelDescriptor::new(
                    kind,
                    kind,
                    EditorPanelMultiplicity::Single,
                    placement,
                    true,
                ))
                .unwrap();
        }
        let layout = default_layout(&workspace).unwrap();
        let mut context = context();
        for _ in 0..4 {
            render_workspace(
                &mut context,
                &workspace,
                &layout,
                DockLayoutApply::IfMissing,
            );
        }

        let authoring = window_state(&context, &panel_name(&workspace, "nara.authoring-view"));
        let game = window_state(&context, &panel_name(&workspace, "nara.game-view"));
        assert!(authoring.drawn && game.drawn);
        assert_ne!(authoring.dock, Id::from(0));
        assert_ne!(game.dock, Id::from(0));
        assert_ne!(authoring.dock, game.dock);
        assert!(authoring.max[1] <= game.min[1]);

        let hierarchy = window_state(&context, &panel_name(&workspace, "nara.hierarchy"));
        let project = window_state(&context, &panel_name(&workspace, "nara.project-browser"));
        let inspector = window_state(&context, &panel_name(&workspace, "nara.inspector"));
        let external = window_state(&context, &panel_name(&workspace, "external.details"));
        let bottom = window_state(&context, &panel_name(&workspace, "nara.diagnostics"));
        assert_ne!(hierarchy.dock, Id::from(0));
        assert_eq!(hierarchy.dock, project.dock);
        assert_ne!(inspector.dock, Id::from(0));
        assert_eq!(inspector.dock, external.dock);
        assert!(hierarchy.max[0] <= authoring.min[0]);
        assert!(authoring.max[0] <= inspector.min[0]);
        assert!(game.max[1] <= bottom.min[1]);
        assert_eq!(
            window_state(&context, &panel_name(&workspace, "external.floating")).dock,
            Id::from(0)
        );
        let toolbar = window_state(&context, "Project actions###nara-project-actions");
        assert!(toolbar.drawn);
        assert_ne!(toolbar.dock, Id::from(0));
        assert!(toolbar.max[1] <= hierarchy.min[1]);
    }

    #[test]
    fn restored_user_docks_survive_if_missing_and_reset_rebuilds_default_views() {
        let workspace = EditorPanelWorkspace::default();
        let default = default_layout(&workspace).unwrap();
        let mut user_keys = vec![toolbar_key()];
        for instance in workspace.layout().instances() {
            let descriptor = workspace.registry().descriptor(instance.kind()).unwrap();
            user_keys.push(
                WindowKey::new(
                    format!("nara-panel-{}", instance.id().0),
                    descriptor.title(),
                )
                .unwrap(),
            );
        }
        let user = DockLayout::split(
            DockSplit::Left,
            0.35,
            DockLayout::tabs([user_keys.remove(0)]),
            DockLayout::tabs(user_keys),
        );
        let authoring_name = panel_name(&workspace, "nara.authoring-view");
        let game_name = panel_name(&workspace, "nara.game-view");
        let mut ini = String::new();
        let user_node;
        {
            let mut context = context();
            for _ in 0..4 {
                render_workspace(&mut context, &workspace, &user, DockLayoutApply::IfMissing);
            }
            user_node = window_state(&context, &authoring_name).dock;
            assert_ne!(user_node, Id::from(0));
            assert_eq!(user_node, window_state(&context, &game_name).dock);
            context.save_ini_settings(&mut ini);
        }

        let mut restored = context();
        restored.load_ini_settings(&ini);
        for _ in 0..4 {
            render_workspace(
                &mut restored,
                &workspace,
                &default,
                DockLayoutApply::IfMissing,
            );
        }
        assert_eq!(window_state(&restored, &authoring_name).dock, user_node);
        assert_eq!(window_state(&restored, &game_name).dock, user_node);

        render_workspace(
            &mut restored,
            &workspace,
            &default,
            DockLayoutApply::Replace,
        );
        for _ in 0..3 {
            render_workspace(
                &mut restored,
                &workspace,
                &default,
                DockLayoutApply::IfMissing,
            );
        }
        let authoring = window_state(&restored, &authoring_name);
        let game = window_state(&restored, &game_name);
        assert!(authoring.drawn && game.drawn);
        assert_ne!(authoring.dock, game.dock);
        assert!(authoring.max[1] <= game.min[1]);
    }
}
