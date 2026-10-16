use nara_editor::workspace::{
    EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement, EditorPanelRegistry,
    EditorPanelTarget, EditorToolkitLayout, EditorWindowBounds, EditorWorkspaceLayout,
    EditorWorkspaceRestoreReason,
};

fn registry() -> EditorPanelRegistry {
    let mut registry = EditorPanelRegistry::new();
    registry
        .register(EditorPanelDescriptor::new(
            "test.inspector",
            "Inspector",
            EditorPanelMultiplicity::Multiple,
            EditorPanelPlacement::Right,
            true,
        ))
        .unwrap();
    registry
}

#[test]
fn independent_instances_keep_targets_and_closed_instances_reopen() {
    let registry = registry();
    let mut layout = EditorWorkspaceLayout::default_for(&registry);
    let first = layout.instances()[0].id();
    let second = layout
        .open(
            &registry,
            "test.inspector",
            EditorPanelTarget::ActiveDocument,
        )
        .unwrap();
    assert_ne!(first, second);
    assert!(layout.set_visible(first, false));
    assert!(layout.instance(first).unwrap().is_open());
    assert!(!layout.instance(first).unwrap().is_visible());
    assert!(layout.close(first));
    assert!(!layout.instance(first).unwrap().is_open());
    assert!(layout.reopen(first));
    assert!(layout.instance(first).unwrap().is_visible());
    assert_eq!(layout.instances().len(), 2);
    assert_eq!(
        layout.instance(second).unwrap().target(),
        &EditorPanelTarget::ActiveDocument
    );
}

#[test]
fn missing_contribution_toolkit_change_and_offscreen_window_recover_with_reasons() {
    let registry = registry();
    let mut saved = EditorWorkspaceLayout::default_for(&registry);
    let instance = saved.instances()[0].id();
    saved.close(instance);
    saved.set_toolkit_layout(Some(EditorToolkitLayout::new(
        "old-toolkit",
        1,
        "dock-tree",
    )));
    saved.set_window_bounds(EditorWindowBounds::new(90_000.0, 50_000.0, 800.0, 600.0));
    let visible = [EditorWindowBounds::new(0.0, 0.0, 1920.0, 1080.0)];
    let restored = EditorWorkspaceLayout::restore(saved, &registry, "new-toolkit", 2, &visible);
    assert!(
        restored
            .reasons()
            .contains(&EditorWorkspaceRestoreReason::IncompatibleToolkitLayout)
    );
    assert!(
        restored
            .reasons()
            .contains(&EditorWorkspaceRestoreReason::OffscreenWindow)
    );
    assert!(restored.layout().toolkit_layout().is_none());
    assert!(!restored.layout().instances().is_empty());
    assert!(restored.layout().instances()[0].is_visible());
    assert_eq!(restored.layout().instances()[0].id(), instance);

    let mut without_contribution = EditorPanelRegistry::new();
    without_contribution
        .register(EditorPanelDescriptor::new(
            "replacement.hierarchy",
            "Hierarchy",
            EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Left,
            true,
        ))
        .unwrap();
    let restored = EditorWorkspaceLayout::restore(
        restored.into_layout(),
        &without_contribution,
        "new-toolkit",
        2,
        &visible,
    );
    assert!(restored.reasons().iter().any(|reason| matches!(
        reason, EditorWorkspaceRestoreReason::MissingContribution(kind) if kind == "test.inspector"
    )));
    assert_eq!(
        restored.layout().instances()[0].kind(),
        "replacement.hierarchy"
    );
}

#[test]
fn explicit_user_preferences_persist_layout_without_project_access() {
    use nara_editor::workspace::EditorWorkspacePreferences;
    let registry = registry();
    let mut layout = EditorWorkspaceLayout::default_for(&registry);
    layout.close(layout.instances()[0].id());
    let root = std::env::temp_dir().join(format!(
        "nara-editor-workspace-preferences-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("workspace.json");
    let preferences = EditorWorkspacePreferences::new(&path);
    preferences.save(&layout).unwrap();
    let loaded = preferences.load().unwrap().unwrap();
    let matches = loaded == layout;
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&root).unwrap();
    assert!(matches);
}

#[test]
fn layout_round_trips_outside_source_documents_and_reset_is_explicit() {
    let registry = registry();
    let mut layout = EditorWorkspaceLayout::default_for(&registry);
    layout.close(layout.instances()[0].id());
    let bytes = layout.to_json().unwrap();
    let loaded = EditorWorkspaceLayout::from_json(&bytes).unwrap();
    assert!(!loaded.instances()[0].is_open());
    layout.reset(&registry);
    assert!(layout.instances()[0].is_visible());
    assert!(layout.toolkit_layout().is_none());
}
