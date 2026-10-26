#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{fs, path::Path, time::Duration};

use nara::tooling::{EditorAction, EditorActionBatch, EditorCloseDecision};
use nara_editor::{
    EditorApplication, EditorLaunchOptions, ImguiEditorFrontend,
    workspace::{EditorPanelPlacement, EditorPanelTarget},
};

#[path = "support/imgui.rs"]
mod imgui;

#[path = "support/stateful_panel_interactions.rs"]
mod stateful_panel_interactions;

#[test]
fn registered_panels_scroll_and_keep_independent_inspector_instances() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/generic-editor/known-unbound");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "nara-registered-panel-scroll-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&root).expect("this test must exclusively own its new temporary project");
    copy_project(&fixture, &root);
    let path = root.join("worlds/entry.scene.json");
    let mut scene: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let original = scene["payload"]["entities"][0].clone();
    let mut roots = Vec::new();
    let mut entities = Vec::new();
    for index in 0..100 {
        let id = format!("scroll-probe-{index:03}");
        let mut entity = original.clone();
        entity["id"] = id.clone().into();
        roots.push(serde_json::Value::String(id));
        entities.push(entity);
    }
    scene["payload"]["roots"] = roots.into();
    scene["payload"]["entities"] = entities.into();
    fs::write(&path, serde_json::to_vec(&scene).unwrap()).unwrap();
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(&root).with_profile("desktop"));
    application.native_shell_ready();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if application.current_frame().is_some() {
            break;
        }
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    let original = application.current_frame().unwrap();
    let original_scene = original.workspace().active_scene.as_ref().unwrap();
    let mut frontend = ImguiEditorFrontend::new();
    let mut context = imgui::context();
    for _ in 0..4 {
        let _ = imgui::draw(&mut context, &mut frontend, &mut application);
    }
    let before = panel_windows(&context, "Hierarchy###nara-panel-").remove(0);
    assert!(
        before.drawn,
        "the registered hierarchy must contribute real draw commands"
    );
    assert_eq!(
        before.scroll_y, 0.0,
        "the first hierarchy row must initially be visible"
    );
    for _ in 0..10 {
        context.io_mut().add_mouse_pos_event(before.center);
        context.io_mut().add_mouse_wheel_event([0.0, -6.0]);
        let _ = imgui::draw(&mut context, &mut frontend, &mut application);
    }
    let after = panel_windows(&context, "Hierarchy###nara-panel-").remove(0);
    assert!(after.drawn);
    assert!(
        after.scroll_y > before.scroll_y + 50.0,
        "wheel input must move the registered hierarchy contents"
    );

    let first = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.inspector")
        .unwrap()
        .id();
    let second = application
        .panel_workspace_mut()
        .open("nara.inspector", EditorPanelTarget::Workspace)
        .unwrap();
    for instance in [first, second] {
        application
            .panel_workspace_mut()
            .layout_mut()
            .place(instance, EditorPanelPlacement::Floating);
    }
    for _ in 0..4 {
        let _ = imgui::draw(&mut context, &mut frontend, &mut application);
    }
    let inspectors = panel_windows(&context, "Inspector###nara-panel-");
    assert_eq!(inspectors.iter().filter(|panel| panel.drawn).count(), 2);
    assert_ne!(inspectors[0].id, inspectors[1].id);
    application.panel_workspace_mut().layout_mut().close(first);
    let _ = imgui::draw(&mut context, &mut frontend, &mut application);
    let retained = panel_windows(&context, "Inspector###nara-panel-");
    assert_eq!(retained.iter().filter(|panel| panel.drawn).count(), 1);
    assert!(
        application
            .panel_workspace()
            .layout()
            .instance(second)
            .unwrap()
            .is_visible()
    );
    assert_eq!(
        application
            .panel_workspace()
            .layout()
            .instance(second)
            .unwrap()
            .target(),
        &EditorPanelTarget::Workspace
    );
    let current = application.current_frame().unwrap();
    let current_scene = current.workspace().active_scene.as_ref().unwrap();
    assert_eq!(current_scene.document, original_scene.document);
    assert_eq!(current_scene.revision, original_scene.revision);
    assert_eq!(current_scene.dirty, original_scene.dirty);
    let mut closed = application.request_exit().exit_ready();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !closed {
        assert!(
            std::time::Instant::now() < deadline,
            "the Application did not physically retire"
        );
        let update = if let Some(frame) = application.current_frame() {
            let actions = frame.workspace_intent().decision_prompt().map(|prompt| {
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
            });
            application
                .submit_and_drive(
                    EditorActionBatch::new(frame.revision(), actions),
                    Duration::ZERO,
                )
                .update()
        } else {
            application.advance_project_open()
        };
        closed = update.exit_ready();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        application.current_frame().is_none(),
        "the owned session must retire before deleting its fixture"
    );
    let owned_root = root.canonicalize().unwrap();
    assert!(owned_root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    assert!(
        owned_root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("nara-registered-panel-scroll-")
    );
    fs::remove_dir_all(&owned_root).unwrap();
}

struct PanelWindow {
    id: u32,
    center: [f32; 2],
    scroll_y: f32,
    drawn: bool,
}

fn panel_windows(context: &dear_imgui_rs::Context, prefix: &str) -> Vec<PanelWindow> {
    context.binding().with_bound_context(|| {
        // SAFETY: the bound Context owns all windows and the completed draw data. Only copied
        // identities, scroll offsets, geometry and draw-presence facts leave this scope.
        unsafe {
            let current = &*dear_imgui_rs::sys::igGetCurrentContext();
            let draw_data = &*dear_imgui_rs::sys::igGetDrawData();
            (0..current.Windows.Size)
                .filter_map(|index| {
                    let window = &**current.Windows.Data.add(index as usize);
                    if !std::ffi::CStr::from_ptr(window.Name)
                        .to_string_lossy()
                        .starts_with(prefix)
                    {
                        return None;
                    }
                    Some(PanelWindow {
                        id: window.ID,
                        center: [
                            (window.InnerRect.Min.x + window.InnerRect.Max.x) * 0.5,
                            (window.InnerRect.Min.y + window.InnerRect.Max.y) * 0.5,
                        ],
                        scroll_y: window.Scroll.y,
                        drawn: window.Active
                            && !window.Hidden
                            && (0..draw_data.CmdLists.Size).any(|index| {
                                *draw_data.CmdLists.Data.add(index as usize) == window.DrawList
                            }),
                    })
                })
                .collect()
        }
    })
}

fn copy_project(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_project(&entry.path(), &destination.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}
