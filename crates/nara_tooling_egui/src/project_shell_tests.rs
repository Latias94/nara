use crate::{
    EguiProjectManagerInput, EguiProjectOpenDraft, EguiProjectRecoveryDiagnostic,
    EguiProjectRecoveryInput, EguiProjectShellAction, EguiProjectShellInput,
    EguiProjectShellOutput, EguiProjectShellPanel,
    project_shell::{TestRecoveryFact, test_control_rect, test_recovery_facts},
};

#[test]
fn project_manager_and_recovery_focus_follow_entry_generation() {
    let context = egui::Context::default();
    let mut panel = EguiProjectShellPanel::new();
    let draft = EguiProjectOpenDraft::new("C:\\projects\\demo", "desktop");

    let manager = EguiProjectShellInput::ProjectManager(EguiProjectManagerInput::new(1, &draft));
    let _ = render_shell(&context, &mut panel, manager, egui::RawInput::default());
    let path_focus = focused(&context);
    let profile_focus = render_tab_and_focus(&context, &mut panel, manager);
    let open_focus = render_tab_and_focus(&context, &mut panel, manager);
    let exit_focus = render_tab_and_focus(&context, &mut panel, manager);

    assert_ne!(path_focus, profile_focus);
    assert_ne!(profile_focus, open_focus);
    assert_ne!(open_focus, exit_focus);

    let _ = render_shell(&context, &mut panel, manager, egui::RawInput::default());
    assert_eq!(focused(&context), exit_focus);

    let next_manager =
        EguiProjectShellInput::ProjectManager(EguiProjectManagerInput::new(2, &draft));
    let _ = render_shell(
        &context,
        &mut panel,
        next_manager,
        egui::RawInput::default(),
    );
    assert_eq!(focused(&context), path_focus);
    assert_eq!(
        click_shell_control(manager, "Open Project").action(),
        Some(EguiProjectShellAction::OpenProject)
    );
    assert_eq!(
        click_shell_control(manager, "Exit").action(),
        Some(EguiProjectShellAction::Exit)
    );

    let diagnostics = [recovery_diagnostic()];
    let recovery =
        EguiProjectShellInput::Recovery(EguiProjectRecoveryInput::new(3, &draft, 7, &diagnostics));
    let _ = render_shell(&context, &mut panel, recovery, egui::RawInput::default());
    let retry_focus = focused(&context);
    let close_focus = render_tab_and_focus(&context, &mut panel, recovery);
    let open_project_focus = render_tab_and_focus(&context, &mut panel, recovery);

    assert_ne!(retry_focus, close_focus);
    assert_ne!(close_focus, open_project_focus);

    let _ = render_shell(&context, &mut panel, recovery, egui::RawInput::default());
    assert_eq!(focused(&context), open_project_focus);

    let next_recovery =
        EguiProjectShellInput::Recovery(EguiProjectRecoveryInput::new(4, &draft, 8, &diagnostics));
    let _ = render_shell(
        &context,
        &mut panel,
        next_recovery,
        egui::RawInput::default(),
    );
    assert_eq!(focused(&context), retry_focus);
}

#[test]
fn recovery_renders_structured_facts_and_emits_exact_actions() {
    let draft = EguiProjectOpenDraft::new("C:\\projects\\broken", "desktop");
    let diagnostics = [recovery_diagnostic()];
    let input = EguiProjectShellInput::Recovery(EguiProjectRecoveryInput::new(
        11,
        &draft,
        41,
        &diagnostics,
    ));

    let mut panel = EguiProjectShellPanel::new();
    let context = egui::Context::default();
    let _ = render_shell(&context, &mut panel, input, egui::RawInput::default());

    assert_eq!(
        test_recovery_facts(),
        vec![TestRecoveryFact {
            code: "scene.file-unknown-component".to_owned(),
            phase: "Startup document admission".to_owned(),
            relative_source: Some("scenes/main.scene.json".to_owned()),
            summary: "The startup scene contains an unavailable component.".to_owned(),
            impact: "No authoring session was published.".to_owned(),
            repair_hint: "Install or publish the missing authoring schema, then retry.".to_owned(),
        }]
    );

    let retry = click_shell_control(input, "Retry");
    assert_eq!(retry.action(), Some(EguiProjectShellAction::Retry));
    assert_eq!(retry.draft(), &draft);

    let close = click_shell_control(input, "Close Project");
    assert_eq!(close.action(), Some(EguiProjectShellAction::CloseProject));

    let open = click_shell_control(input, "Open Project");
    assert_eq!(
        open.action(),
        Some(EguiProjectShellAction::ReturnToProjectManager)
    );

    let retry_rect = test_control_rect("Retry").expect("Retry should be rendered");
    let close_rect = test_control_rect("Close Project").expect("Close Project should be rendered");
    let open_rect = test_control_rect("Open Project").expect("Open Project should be rendered");
    assert!(retry_rect.center().x < close_rect.center().x);
    assert!(close_rect.center().x < open_rect.center().x);
}

fn recovery_diagnostic() -> EguiProjectRecoveryDiagnostic<'static> {
    EguiProjectRecoveryDiagnostic::new(
        "scene.file-unknown-component",
        "Startup document admission",
        Some("scenes/main.scene.json"),
        "The startup scene contains an unavailable component.",
        "No authoring session was published.",
        "Install or publish the missing authoring schema, then retry.",
    )
}

fn render_tab_and_focus(
    context: &egui::Context,
    panel: &mut EguiProjectShellPanel,
    input: EguiProjectShellInput<'_>,
) -> egui::Id {
    let mut raw_input = egui::RawInput::default();
    raw_input.events.push(egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    });
    let _ = render_shell(context, panel, input, raw_input);
    focused(context)
}

fn focused(context: &egui::Context) -> egui::Id {
    context
        .memory(|memory| memory.focused())
        .expect("the shell should keep one deterministic keyboard focus")
}

fn click_shell_control(input: EguiProjectShellInput<'_>, label: &str) -> EguiProjectShellOutput {
    let context = egui::Context::default();
    let mut panel = EguiProjectShellPanel::new();
    let _ = render_shell(&context, &mut panel, input, egui::RawInput::default());
    let _ = render_shell(&context, &mut panel, input, egui::RawInput::default());
    let position = test_control_rect(label)
        .unwrap_or_else(|| panic!("control {label} should be rendered"))
        .center();

    let mut pressed = egui::RawInput::default();
    pressed.events = vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let _ = render_shell(&context, &mut panel, input, pressed);

    let mut released = egui::RawInput::default();
    released.events = vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    render_shell(&context, &mut panel, input, released)
}

fn render_shell(
    context: &egui::Context,
    panel: &mut EguiProjectShellPanel,
    input: EguiProjectShellInput<'_>,
    mut raw_input: egui::RawInput,
) -> EguiProjectShellOutput {
    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(900.0, 640.0),
    ));
    let mut response = None;
    let output = context.run_ui(raw_input, |ui| {
        response = Some(panel.show(ui, input));
    });
    output.drop_without_applying_deltas();
    response.expect("the Project Shell should render exactly once")
}
