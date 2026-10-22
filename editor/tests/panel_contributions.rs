use nara_editor::{
    EditorApplication, EditorCompositionError, EditorLaunchOptions, EditorNativeComposition,
    ImguiEditorPanel, ImguiEditorPanelContext,
    workspace::{
        EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement,
        EditorPanelRegistrationError,
    },
};

#[test]
fn production_builtins_are_registered_contributions() {
    let application = EditorApplication::new(EditorLaunchOptions::project_manager());
    let workspace = application.panel_workspace();
    for kind in [
        "nara.hierarchy",
        "nara.project-browser",
        "nara.authoring-view",
        "nara.game-view",
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

struct EmptyPanel;

impl ImguiEditorPanel for EmptyPanel {
    fn show(&mut self, _: &dear_imgui_rs::Ui, _: &mut ImguiEditorPanelContext<'_, '_>) {}
}

fn descriptor(kind: &str) -> EditorPanelDescriptor {
    EditorPanelDescriptor::new(
        kind,
        "External",
        EditorPanelMultiplicity::Multiple,
        EditorPanelPlacement::Bottom,
        true,
    )
}

#[test]
fn native_composition_admits_descriptor_and_factory_together() {
    let mut application = EditorApplication::new(EditorLaunchOptions::project_manager());
    let mut composition = EditorNativeComposition::new();
    assert_eq!(
        composition.register_panel(descriptor("nara.inspector"), || EmptyPanel),
        Err(EditorPanelRegistrationError::Duplicate(
            "nara.inspector".to_owned()
        ))
    );
    assert_eq!(
        composition.register_panel(descriptor(""), || EmptyPanel),
        Err(EditorPanelRegistrationError::EmptyIdentity)
    );
    composition
        .register_panel(descriptor("sample.draft"), || EmptyPanel)
        .unwrap();
    assert_eq!(
        composition.register_panel(descriptor("sample.draft"), || EmptyPanel),
        Err(EditorPanelRegistrationError::Duplicate(
            "sample.draft".to_owned()
        ))
    );
    let _frontend = composition.install_panels(&mut application).unwrap();
    let workspace = application.panel_workspace();
    assert!(workspace.registry().descriptor("sample.draft").is_some());
    assert_eq!(
        workspace
            .layout()
            .instances()
            .iter()
            .filter(|panel| panel.kind() == "sample.draft")
            .count(),
        1
    );
}

#[test]
fn conflicting_embedded_composition_preserves_the_complete_workspace() {
    let mut application = EditorApplication::new(EditorLaunchOptions::project_manager());
    application
        .panel_workspace_mut()
        .register(descriptor("sample.z-conflict"))
        .unwrap();
    let before = application.panel_workspace().layout().to_json().unwrap();
    let mut composition = EditorNativeComposition::new();
    composition
        .register_panel(descriptor("sample.a-first"), || EmptyPanel)
        .unwrap();
    composition
        .register_panel(descriptor("sample.z-conflict"), || EmptyPanel)
        .unwrap();
    assert!(
        matches!(composition.install_panels(&mut application), Err(EditorCompositionError::Panel(EditorPanelRegistrationError::Duplicate(kind))) if kind == "sample.z-conflict")
    );
    assert!(
        application
            .panel_workspace()
            .registry()
            .descriptor("sample.a-first")
            .is_none()
    );
    assert_eq!(
        application.panel_workspace().layout().to_json().unwrap(),
        before
    );
}
