use std::sync::Arc;

use nara_tooling::{
    EditorAction, EditorImageColorSpace, EditorImageImportAction, EditorImageImportAttemptPhase,
    EditorImageImportAttemptView, EditorImageImportAvailabilityView,
    EditorImageImportDetailRevision, EditorImageImportFreshness, EditorImageImportMetadataIssue,
    EditorImageImportMetadataView, EditorImageImportSettings, EditorImageImportView,
    EditorProjectBrowserBudgetUsage, EditorProjectBrowserEntry, EditorProjectBrowserLocalAction,
    EditorProjectBrowserSnapshot, EditorProjectBrowserState, EditorProjectBrowserStatus,
    EditorProjectSourceInspectReason, EditorProjectSourceKind, EditorProjectSourceOpenCapability,
};

use crate::image_import::{
    EguiImageImportPanel, active_settings_status_label, freshness_status_label,
    test_image_import_control_enabled, test_image_import_control_rect,
    test_image_import_status_lines,
};
use crate::project_browser::EguiProjectBrowserPanel;

#[test]
fn image_import_state_table_uses_closed_status_and_button_rules() {
    let context = egui::Context::default();
    let mut panel = EguiImageImportPanel::new();

    let missing = view(
        "assets/icon.png",
        1,
        EditorImageImportMetadataView::Missing,
        EditorImageImportAttemptView::Idle,
    );
    assert_state(
        &context,
        &mut panel,
        &missing,
        Some(("Import", true)),
        Some(settings(EditorImageColorSpace::Srgb)),
        [
            "Metadata: Missing",
            "Committed settings: None",
            "Last-good image: Unavailable",
            "Attempt: Idle",
        ],
    );

    let ready_stable_id = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f"
        .parse()
        .expect("fixture stable ID should be valid");
    let ready = view(
        "assets/icon.png",
        2,
        EditorImageImportMetadataView::Ready {
            stable_id: ready_stable_id,
            committed_settings: settings(EditorImageColorSpace::Linear),
        },
        EditorImageImportAttemptView::Idle,
    );
    assert_state(
        &context,
        &mut panel,
        &ready,
        Some(("Reimport", true)),
        Some(settings(EditorImageColorSpace::Linear)),
        [
            "Metadata: Ready",
            "Committed color space: Linear",
            "Last-good image: Unavailable",
            "Attempt: Idle",
        ],
    );

    let invalid = view(
        "assets/icon.png",
        3,
        EditorImageImportMetadataView::Invalid {
            issue: EditorImageImportMetadataIssue::InvalidFormat,
        },
        EditorImageImportAttemptView::Idle,
    );
    assert_state(
        &context,
        &mut panel,
        &invalid,
        None,
        None,
        [
            "Metadata: Invalid format",
            "Committed settings: Unavailable",
            "Last-good image: Unavailable",
            "Attempt: Idle",
        ],
    );

    let conflicting_stable_id = "b73f0f16-09e8-4265-b090-b689b41c197e"
        .parse()
        .expect("fixture stable ID should be valid");
    let conflict = view(
        "assets/icon.png",
        4,
        EditorImageImportMetadataView::IdentityConflict {
            stable_id: conflicting_stable_id,
        },
        EditorImageImportAttemptView::Idle,
    );
    assert_state(
        &context,
        &mut panel,
        &conflict,
        None,
        None,
        [
            "Metadata: Identity conflict",
            "Committed settings: Unavailable",
            "Last-good image: Unavailable",
            "Attempt: Idle",
        ],
    );

    let pending = view(
        "assets/icon.png",
        5,
        EditorImageImportMetadataView::Missing,
        EditorImageImportAttemptView::Pending {
            generation: 7,
            phase: EditorImageImportAttemptPhase::DecodeAndValidate,
        },
    );
    assert_state(
        &context,
        &mut panel,
        &pending,
        Some(("Import", false)),
        Some(settings(EditorImageColorSpace::Srgb)),
        [
            "Metadata: Missing",
            "Committed settings: None",
            "Last-good image: Unavailable",
            "Attempt: Decoding and validating",
        ],
    );

    let reconcile = view(
        "assets/icon.png",
        6,
        EditorImageImportMetadataView::Missing,
        EditorImageImportAttemptView::ReconcileRequired { generation: 8 },
    );
    assert_state(
        &context,
        &mut panel,
        &reconcile,
        Some(("Import", false)),
        Some(settings(EditorImageColorSpace::Srgb)),
        [
            "Metadata: Missing",
            "Committed settings: None",
            "Last-good image: Unavailable",
            "Attempt: Refresh required before retry",
        ],
    );
}

#[test]
fn last_good_status_labels_are_safe_and_specific() {
    assert_eq!(
        freshness_status_label(EditorImageImportFreshness::SourceUnverified),
        "Last-good image: Available; source not verified"
    );
    assert_eq!(
        freshness_status_label(EditorImageImportFreshness::SettingsChanged),
        "Last-good image: Available; settings changed"
    );
    assert_eq!(
        active_settings_status_label(settings(EditorImageColorSpace::Srgb)),
        "Active color space: sRGB"
    );
}

#[test]
fn image_import_draft_is_local_and_resets_on_selection_or_revision_change() {
    let context = egui::Context::default();
    let mut panel = EguiImageImportPanel::new();
    let first = view(
        "assets/icon.png",
        10,
        EditorImageImportMetadataView::Missing,
        EditorImageImportAttemptView::Idle,
    );

    assert!(click_control(&context, &mut panel, &first, "Linear").is_empty());
    assert_eq!(
        panel.draft_settings(),
        Some(settings(EditorImageColorSpace::Linear))
    );

    let actions = click_control(&context, &mut panel, &first, "Import");
    assert_eq!(
        actions,
        [EditorAction::ImageImport(EditorImageImportAction::Submit {
            relative_path: Arc::from("assets/icon.png"),
            expected_detail_revision: EditorImageImportDetailRevision::new(10),
            settings: settings(EditorImageColorSpace::Linear),
        })]
    );

    assert!(click_control(&context, &mut panel, &first, "Revert").is_empty());
    assert_eq!(
        panel.draft_settings(),
        Some(settings(EditorImageColorSpace::Srgb))
    );

    assert!(click_control(&context, &mut panel, &first, "Linear").is_empty());
    let revised = view(
        "assets/icon.png",
        11,
        EditorImageImportMetadataView::Missing,
        EditorImageImportAttemptView::Idle,
    );
    let _ = render(&context, &mut panel, &revised, egui::RawInput::default());
    assert_eq!(
        panel.draft_settings(),
        Some(settings(EditorImageColorSpace::Srgb))
    );

    assert!(click_control(&context, &mut panel, &revised, "Linear").is_empty());
    let changed_selection = view(
        "assets/title.png",
        11,
        EditorImageImportMetadataView::Missing,
        EditorImageImportAttemptView::Idle,
    );
    let _ = render(
        &context,
        &mut panel,
        &changed_selection,
        egui::RawInput::default(),
    );
    assert_eq!(
        panel.draft_settings(),
        Some(settings(EditorImageColorSpace::Srgb))
    );
}

#[test]
fn another_target_pending_disables_an_idle_image_submit() {
    let context = egui::Context::default();
    let mut panel = EguiImageImportPanel::new();
    let idle_selection = view(
        "assets/title.png",
        12,
        EditorImageImportMetadataView::Missing,
        EditorImageImportAttemptView::Idle,
    );

    let actions = render_with_host_actions(
        &context,
        &mut panel,
        &idle_selection,
        false,
        egui::RawInput::default(),
    );
    assert!(actions.is_empty());
    assert_eq!(test_image_import_control_enabled("Import"), Some(false));
}

#[test]
fn project_browser_uses_image_inspector_only_for_the_matching_asset_source() {
    let context = egui::Context::default();
    let mut panel = EguiProjectBrowserPanel::new();
    let mut browser = ready_browser_state();
    let image = view(
        "assets/icon.png",
        20,
        EditorImageImportMetadataView::Missing,
        EditorImageImportAttemptView::Idle,
    );

    assert!(
        browser.apply_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/icon.png")),
        })
    );
    render_project_inspector(&context, &mut panel, &browser.view(), Some(&image));
    assert!(test_image_import_control_rect("Import").is_some());

    assert!(
        browser.apply_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/icon.png.meta")),
        })
    );
    render_project_inspector(&context, &mut panel, &browser.view(), Some(&image));
    assert!(test_image_import_control_rect("Import").is_none());
    assert!(test_image_import_control_rect("Reimport").is_none());

    assert!(
        browser.apply_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/theme.ogg")),
        })
    );
    render_project_inspector(&context, &mut panel, &browser.view(), None);
    assert!(test_image_import_control_rect("Import").is_none());
    assert!(test_image_import_control_rect("Reimport").is_none());
}

fn assert_state<const STATUS_COUNT: usize>(
    context: &egui::Context,
    panel: &mut EguiImageImportPanel,
    view: &EditorImageImportView,
    command: Option<(&str, bool)>,
    expected_draft: Option<EditorImageImportSettings>,
    expected_status: [&str; STATUS_COUNT],
) {
    let actions = render(context, panel, view, egui::RawInput::default());
    assert!(actions.is_empty());
    assert_eq!(panel.draft_settings(), expected_draft);
    assert_eq!(
        test_image_import_status_lines(),
        expected_status
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );

    match command {
        Some((label, enabled)) => {
            assert!(test_image_import_control_rect(label).is_some());
            assert_eq!(test_image_import_control_enabled(label), Some(enabled));
        }
        None => {
            assert!(test_image_import_control_rect("Import").is_none());
            assert!(test_image_import_control_rect("Reimport").is_none());
        }
    }
}

fn view(
    relative_path: &str,
    detail_revision: u64,
    metadata: EditorImageImportMetadataView,
    attempt: EditorImageImportAttemptView,
) -> EditorImageImportView {
    EditorImageImportView::new(
        Arc::from(relative_path),
        EditorImageImportDetailRevision::new(detail_revision),
        metadata,
        EditorImageImportAvailabilityView::Unavailable,
        attempt,
        true,
    )
}

fn ready_browser_state() -> EditorProjectBrowserState {
    let snapshot = Arc::new(EditorProjectBrowserSnapshot::new(
        1,
        [
            EditorProjectBrowserEntry::new(
                Arc::from("assets/icon.png"),
                EditorProjectSourceKind::AssetSource,
                EditorProjectSourceOpenCapability::InspectOnly(
                    EditorProjectSourceInspectReason::ReadOnlySource,
                ),
            ),
            EditorProjectBrowserEntry::new(
                Arc::from("assets/icon.png.meta"),
                EditorProjectSourceKind::AssetMeta,
                EditorProjectSourceOpenCapability::InspectOnly(
                    EditorProjectSourceInspectReason::ReadOnlySource,
                ),
            ),
            EditorProjectBrowserEntry::new(
                Arc::from("assets/theme.ogg"),
                EditorProjectSourceKind::AssetSource,
                EditorProjectSourceOpenCapability::InspectOnly(
                    EditorProjectSourceInspectReason::ReadOnlySource,
                ),
            ),
        ],
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
    while state.advance_query(16).pending() {}
    state
}

const fn settings(color_space: EditorImageColorSpace) -> EditorImageImportSettings {
    EditorImageImportSettings::new(color_space)
}

fn click_control(
    context: &egui::Context,
    panel: &mut EguiImageImportPanel,
    view: &EditorImageImportView,
    label: &str,
) -> Vec<EditorAction> {
    let _ = render(context, panel, view, egui::RawInput::default());
    let _ = render(context, panel, view, egui::RawInput::default());
    let position = test_image_import_control_rect(label)
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
    let _ = render(context, panel, view, pressed);

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
    render(context, panel, view, released)
}

fn render(
    context: &egui::Context,
    panel: &mut EguiImageImportPanel,
    view: &EditorImageImportView,
    input: egui::RawInput,
) -> Vec<EditorAction> {
    render_with_host_actions(context, panel, view, true, input)
}

fn render_with_host_actions(
    context: &egui::Context,
    panel: &mut EguiImageImportPanel,
    view: &EditorImageImportView,
    host_actions_enabled: bool,
    mut input: egui::RawInput,
) -> Vec<EditorAction> {
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(320.0, 600.0),
    ));
    let mut actions = Vec::new();
    let output = context.run_ui(input, |ui| {
        panel.show(ui, view, host_actions_enabled, &mut actions);
    });
    output.drop_without_applying_deltas();
    actions
}

fn render_project_inspector(
    context: &egui::Context,
    panel: &mut EguiProjectBrowserPanel,
    browser: &nara_tooling::EditorProjectBrowserView,
    image_import: Option<&EditorImageImportView>,
) {
    let mut input = egui::RawInput::default();
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(320.0, 600.0),
    ));
    let mut actions = Vec::new();
    let output = context.run_ui(input, |ui| {
        panel.show_inspector(ui, browser, image_import, true, false, &mut actions);
    });
    output.drop_without_applying_deltas();
    assert!(actions.is_empty());
}
