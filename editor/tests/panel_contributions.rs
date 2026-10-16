use nara_editor::{
    EditorApplication, EditorLaunchOptions,
    workspace::{EditorPanelMultiplicity, EditorPanelPlacement},
};

#[test]
fn production_builtins_are_registered_contributions() {
    let application = EditorApplication::new(EditorLaunchOptions::project_manager());
    let workspace = application.panel_workspace();
    for kind in [
        "nara.hierarchy",
        "nara.project-browser",
        "nara.authoring-view",
        "nara.inspector",
        "nara.diagnostics",
    ] {
        assert!(
            workspace.registry().descriptor(kind).is_some(),
            "missing {kind}"
        );
    }
    let inspector = workspace.registry().descriptor("nara.inspector").unwrap();
    assert_eq!(inspector.multiplicity(), EditorPanelMultiplicity::Multiple);
    assert_eq!(inspector.default_placement(), EditorPanelPlacement::Right);
    assert!(
        workspace
            .layout()
            .instances()
            .iter()
            .any(|panel| panel.kind() == inspector.kind())
    );
}
