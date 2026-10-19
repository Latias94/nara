use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::{
    profile_font_product::{ProfileFontOutcome, profile_font_recipe},
    project_content_fixture::TestProject,
};
use nara::{
    project_host::{
        EditorProjectCloseProgress, EditorProjectSession, TrustedCompiledEditorProjectIntent,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorPlayAction,
        EditorPlayRequestResult, EditorPlayState,
    },
};

#[test]
fn ordinary_font_recipe_is_reconstructed_by_managed_editor_play() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let manifest_path = project.path().join("nara.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap().replace(
        "requested = [\"runtime-2d\"]",
        "requested = [\"runtime-2d\", \"runtime-ui\", \"presentation-ui\"]",
    );
    std::fs::write(
        manifest_path,
        format!("{manifest}\n[ui.primary_surface]\nlogical_width = 320\nlogical_height = 180\n"),
    )
    .unwrap();
    let (_, plan, _) = project.candidate_plan_and_root();
    std::fs::write(
        project.path().join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let face = nara::ui::FontFace::from_bytes(
        include_bytes!("../../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let observations = Arc::new(Mutex::new(Vec::new()));
    let recipe = profile_font_recipe(face, Some(Arc::clone(&observations))).unwrap();
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.root_capability(),
        TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
    )
    .unwrap();
    for expected_instances in 1..=2 {
        let frame = editor.current_frame();
        let active = frame.workspace().active_scene.as_ref().unwrap();
        request(
            &mut editor,
            EditorPlayAction::Play {
                document: active.document,
                base_revision: active.revision,
            },
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while observations.lock().unwrap().len() < expected_instances {
            editor.drive_editor_frame(Duration::from_millis(20));
            assert!(
                Instant::now() < deadline,
                "the managed Play runtime did not publish its configured text: {:?}",
                editor.current_frame().diagnostics()
            );
            std::thread::yield_now();
        }
        assert_eq!(
            editor.current_frame().play().state(),
            EditorPlayState::Running
        );
        assert_eq!(
            observations.lock().unwrap()[expected_instances - 1],
            ProfileFontOutcome {
                compiled_selected_face: true,
                rendered_selected_face: true,
                presented_selected_face: false,
            }
        );
        request(&mut editor, EditorPlayAction::Stop);
        let deadline = Instant::now() + Duration::from_secs(5);
        while editor.current_frame().play().state() != EditorPlayState::Empty {
            editor.drive_editor_frame(Duration::ZERO);
            assert!(
                Instant::now() < deadline,
                "the Play runtime did not close: {:?}",
                editor.current_frame().play()
            );
            std::thread::yield_now();
        }
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(
            Instant::now() < deadline,
            "the Editor did not retire its owned work"
        );
        std::thread::yield_now();
    }
}

fn request(editor: &mut EditorProjectSession, action: EditorPlayAction) {
    let frame = editor.current_frame();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Play(action)],
        ))
        .unwrap();
    assert!(matches!(
        report.into_outcomes().as_slice(),
        [EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)]
    ));
}
