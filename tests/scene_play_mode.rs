#![cfg(all(feature = "tooling", feature = "runtime-2d", feature = "serde"))]

#[path = "support/editor_action_driver.rs"]
mod editor_action_driver;
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{sync::Arc, time::Duration};

use editor_action_driver as editor_driver;
use nara::{
    project_host::{EditorProjectSession, TrustedCompiledEditorProjectIntent},
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::SceneEntityId,
    tooling::{
        EditorFrameModel, EditorPlayCommand, EditorPlayRequestResult, EditorPlayState,
        EditorRuntimeEditResult,
    },
};
use project_content_fixture::TestProject;

#[test]
fn runtime_only_edit_is_discarded_when_host_owned_play_stops() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let target = project.prefab_projection(
        &SceneEntityId::new("enemy-anchor").unwrap(),
        &SceneEntityId::new("enemy").unwrap(),
    );
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.root_capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let before = editor.current_frame();
    let document = before.workspace().active_document.unwrap();
    let before_scene = before.workspace().scene(document).unwrap().clone();

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running);
    editor_driver::request_runtime_edit(
        &mut editor,
        target,
        ComponentTypeId::new("nara.sprite.Sprite"),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("layer"),
        ComponentValue::I64(7),
    )
    .unwrap();
    let edited = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        edited.runtime_edit_result(),
        Some(EditorRuntimeEditResult::Applied(_))
    ));

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    let stopped = drive_until(&mut editor, EditorPlayState::Empty);
    let stopped_scene = stopped.workspace().scene(document).unwrap();
    assert_eq!(stopped_scene.revision, before_scene.revision);
    assert_eq!(stopped_scene.dirty, before_scene.dirty);
}

fn drive_until(
    editor: &mut EditorProjectSession,
    expected: EditorPlayState,
) -> Arc<EditorFrameModel> {
    for _ in 0..8 {
        let frame = editor.current_frame();
        if frame.play().state() == expected {
            return frame;
        }
        editor.drive_editor_frame(Duration::ZERO);
    }
    let frame = editor.current_frame();
    assert_eq!(frame.play().state(), expected);
    frame
}
