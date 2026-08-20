use std::sync::Arc;

use nara_tooling::{
    EditorAction, EditorProjectBrowserAction, EditorProjectBrowserBudgetUsage,
    EditorProjectBrowserDiagnosticPhase, EditorProjectBrowserDiagnosticView,
    EditorProjectBrowserEntry, EditorProjectBrowserLocalAction, EditorProjectBrowserSnapshot,
    EditorProjectBrowserState, EditorProjectBrowserStatus, EditorProjectSourceInspectReason,
    EditorProjectSourceKind, EditorProjectSourceOpenCapability,
};

use crate::project_browser::{
    EguiProjectBrowserPanel, test_control_rect, test_rendered_rows, test_status_label,
};

#[test]
fn project_browser_keeps_local_navigation_outside_the_host_batch() {
    let mut state = ready_state([
        entry(
            "assets/icon.png",
            EditorProjectSourceKind::AssetSource,
            EditorProjectSourceOpenCapability::InspectOnly(
                EditorProjectSourceInspectReason::ReadOnlySource,
            ),
        ),
        entry(
            "scenes",
            EditorProjectSourceKind::Directory,
            EditorProjectSourceOpenCapability::InspectOnly(
                EditorProjectSourceInspectReason::Directory,
            ),
        ),
        entry(
            "scenes/main.scene.json",
            EditorProjectSourceKind::Scene,
            EditorProjectSourceOpenCapability::Scene,
        ),
    ]);
    assert!(
        state.apply_local_action(EditorProjectBrowserLocalAction::ToggleExpanded {
            relative_path: Arc::from("scenes"),
        })
    );
    while state.advance_query(8).pending() {}

    let context = egui::Context::default();
    let mut panel = EguiProjectBrowserPanel::new();
    let (_, selected) = click_control(
        &context,
        &mut panel,
        &state.view(),
        "scenes/main.scene.json",
    );
    assert_eq!(
        selected,
        [EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("scenes/main.scene.json")),
        }]
    );
    assert!(state.apply_local_action(selected.into_iter().next().unwrap()));

    let (opened, local) = click_control(&context, &mut panel, &state.view(), "Project Open");
    assert!(local.is_empty());
    assert_eq!(
        opened,
        [EditorAction::ProjectBrowser(
            EditorProjectBrowserAction::Open {
                relative_path: Arc::from("scenes/main.scene.json"),
            }
        )]
    );

    let (refresh, local) = click_control(&context, &mut panel, &state.view(), "Project Refresh");
    assert!(local.is_empty());
    assert_eq!(
        refresh,
        [EditorAction::ProjectBrowser(
            EditorProjectBrowserAction::Refresh,
        )]
    );

    assert!(
        state.apply_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/icon.png")),
        })
    );
    let _ = render(
        &context,
        &mut panel,
        &state.view(),
        egui::RawInput::default(),
        900.0,
    );
    assert!(test_control_rect("Project Open").is_none());
}

#[test]
fn project_browser_renders_all_states_and_caps_one_frame_of_rows() {
    let mut panel = EguiProjectBrowserPanel::new();
    let context = egui::Context::default();
    for (status, expected, pending) in [
        (
            EditorProjectBrowserStatus::NotRequested,
            "Not indexed",
            false,
        ),
        (EditorProjectBrowserStatus::Scanning, "Scanning", true),
        (EditorProjectBrowserStatus::Ready, "Ready", false),
        (
            EditorProjectBrowserStatus::ReadyWithDiagnostics,
            "Ready with diagnostics",
            false,
        ),
        (
            EditorProjectBrowserStatus::Unavailable,
            "Unavailable",
            false,
        ),
    ] {
        let mut state = EditorProjectBrowserState::new();
        state.synchronize(
            status,
            None,
            (status == EditorProjectBrowserStatus::Unavailable).then(|| {
                EditorProjectBrowserDiagnosticView::new(
                    "project.source-index.unavailable",
                    EditorProjectBrowserDiagnosticPhase::Publication,
                    Some(Arc::from("scenes")),
                    "Project sources are unavailable",
                    "No candidate index was published.",
                    "Repair the declared source root and refresh.",
                )
            }),
            None,
            pending,
            false,
        );
        let _ = render(
            &context,
            &mut panel,
            &state.view(),
            egui::RawInput::default(),
            900.0,
        );
        assert_eq!(test_status_label().as_deref(), Some(expected));
    }

    let entries = (0..1_000).map(|index| {
        entry(
            &format!("scene-{index:04}.scene.json"),
            EditorProjectSourceKind::Scene,
            EditorProjectSourceOpenCapability::Scene,
        )
    });
    let state = ready_state(entries);
    let _ = render(
        &context,
        &mut panel,
        &state.view(),
        egui::RawInput::default(),
        20_000.0,
    );
    assert_eq!(state.view().rows().len(), 1_000);
    assert_eq!(test_rendered_rows().len(), 256);
}

fn ready_state(
    entries: impl IntoIterator<Item = EditorProjectBrowserEntry>,
) -> EditorProjectBrowserState {
    let snapshot = Arc::new(EditorProjectBrowserSnapshot::new(
        1,
        entries,
        [],
        EditorProjectBrowserBudgetUsage::default(),
    ));
    let mut state = EditorProjectBrowserState::new();
    state.synchronize(
        EditorProjectBrowserStatus::Ready,
        Some(snapshot),
        None,
        None,
        false,
        false,
    );
    while state.advance_query(1_024).pending() {}
    state
}

fn entry(
    relative_path: &str,
    kind: EditorProjectSourceKind,
    open_capability: EditorProjectSourceOpenCapability,
) -> EditorProjectBrowserEntry {
    EditorProjectBrowserEntry::new(Arc::from(relative_path), kind, open_capability)
}

fn click_control(
    context: &egui::Context,
    panel: &mut EguiProjectBrowserPanel,
    view: &nara_tooling::EditorProjectBrowserView,
    label: &str,
) -> (Vec<EditorAction>, Vec<EditorProjectBrowserLocalAction>) {
    let _ = render(context, panel, view, egui::RawInput::default(), 900.0);
    let _ = render(context, panel, view, egui::RawInput::default(), 900.0);
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
    let _ = render(context, panel, view, pressed, 900.0);
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
    render(context, panel, view, released, 900.0)
}

fn render(
    context: &egui::Context,
    panel: &mut EguiProjectBrowserPanel,
    view: &nara_tooling::EditorProjectBrowserView,
    mut input: egui::RawInput,
    height: f32,
) -> (Vec<EditorAction>, Vec<EditorProjectBrowserLocalAction>) {
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(900.0, height),
    ));
    let mut host_actions = Vec::new();
    let mut local_actions = Vec::new();
    let output = context.run_ui(input, |ui| {
        ui.set_max_height(height);
        ui.columns(2, |columns| {
            panel.show_tree(
                &mut columns[0],
                view,
                true,
                &mut host_actions,
                &mut local_actions,
            );
            panel.show_inspector(&mut columns[1], view, None, true, false, &mut host_actions);
        });
    });
    output.drop_without_applying_deltas();
    (host_actions, local_actions)
}
