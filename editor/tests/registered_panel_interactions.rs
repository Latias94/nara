#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{fs, path::Path, time::Duration};

use nara::tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorWorkspaceIntentAction,
};
use nara_editor::{EditorApplication, EditorLaunchOptions, EguiEditorFrontend};

#[test]
fn registered_hierarchy_scrolls_to_entities_below_the_visible_column() {
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
    for _ in 0..8 {
        if application.current_frame().is_some() {
            break;
        }
        application.advance_project_open();
    }
    let mut frontend = EguiEditorFrontend::new();
    let context = egui::Context::default();
    let before = draw(&context, &mut frontend, &mut application, Vec::new());
    let mut after = before;
    for _ in 0..10 {
        after = draw(
            &context,
            &mut frontend,
            &mut application,
            vec![
                egui::Event::PointerMoved(egui::pos2(100.0, 430.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -180.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    application.request_exit();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let Some(frame) = application.current_frame() else {
            break;
        };
        let actions = if frame.workspace_intent().phase()
            == Some(nara::tooling::EditorWorkspaceIntentPhase::AwaitingDecision)
        {
            vec![EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Resolve(EditorCloseDecision::Discard),
            )]
        } else {
            vec![]
        };
        if application
            .submit_and_drive(
                EditorActionBatch::new(frame.revision(), actions),
                Duration::ZERO,
            )
            .exit_ready()
        {
            break;
        }
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
    assert!(before.is_some(), "the first hierarchy row must be painted");
    assert!(
        after.is_none_or(|position| position < before.unwrap() - 50.0),
        "scroll input must move the registered hierarchy contents"
    );
}

fn draw(
    context: &egui::Context,
    frontend: &mut EguiEditorFrontend,
    application: &mut EditorApplication,
    events: Vec<egui::Event>,
) -> Option<f32> {
    let mut input = egui::RawInput::default();
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(1200.0, 800.0),
    ));
    input.events = events;
    let output = context.run_ui(input, |ui| {
        let _ = frontend.show(ui, application);
    });
    let position = output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::epaint::Shape::Text(text)
            if text.galley.job.text.starts_with("scroll-probe-000 (") =>
        {
            Some(text.pos.y)
        }
        _ => None,
    });
    output.drop_without_applying_deltas();
    position
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
