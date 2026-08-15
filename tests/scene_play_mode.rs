#![cfg(all(feature = "tooling", feature = "runtime-2d", feature = "serde"))]

#[path = "support/editor_action_driver.rs"]
mod editor_action_driver;
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{sync::Arc, time::Duration};

use editor_action_driver as editor_driver;
use nara::{
    project_host::{EditorProjectIntent, EditorProjectSession},
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
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
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
        SceneEntityId::new("enemy-anchor/enemy").unwrap(),
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

#[test]
fn tooling_has_no_owned_play_world_escape_hatch() {
    const SOURCES: &[(&str, &str)] = &[
        (
            "nara_tooling::lib",
            include_str!("../crates/nara_tooling/src/lib.rs"),
        ),
        (
            "nara_tooling::persistence",
            include_str!("../crates/nara_tooling/src/persistence.rs"),
        ),
        (
            "nara_tooling::play",
            include_str!("../crates/nara_tooling/src/play.rs"),
        ),
        (
            "nara_tooling::workspace",
            include_str!("../crates/nara_tooling/src/workspace.rs"),
        ),
        (
            "nara_tooling_egui",
            include_str!("../crates/nara_tooling_egui/src/lib.rs"),
        ),
    ];
    const FORBIDDEN_OWNER_TYPES: &[&str] = &[
        "World",
        "RuntimeStartAttempt",
        "RuntimeInstance",
        "DirectoryCapability",
        "FileCapability",
        "WindowSurfaceLease",
        "Surface",
        "Device",
        "Queue",
    ];

    for (label, source) in SOURCES {
        assert!(!source.contains("struct ScenePlaySession"), "{label}");
        assert!(!source.contains("play_world_mut"), "{label}");
        assert!(!source.contains("MarkSaved"), "{label}");
        let syntax = syn::parse_file(source).unwrap_or_else(|error| {
            panic!("{label} must remain valid Rust for the ownership audit: {error}")
        });
        let mut audit = ToolingOwnerAudit {
            source: label,
            forbidden: FORBIDDEN_OWNER_TYPES,
        };
        syn::visit::Visit::visit_file(&mut audit, &syntax);
    }
}

struct ToolingOwnerAudit<'a> {
    source: &'a str,
    forbidden: &'a [&'a str],
}

impl<'ast> syn::visit::Visit<'ast> for ToolingOwnerAudit<'_> {
    fn visit_field(&mut self, field: &'ast syn::Field) {
        let mut field_audit = ToolingFieldTypeAudit {
            source: self.source,
            forbidden: self.forbidden,
        };
        syn::visit::Visit::visit_type(&mut field_audit, &field.ty);
    }
}

struct ToolingFieldTypeAudit<'a> {
    source: &'a str,
    forbidden: &'a [&'a str],
}

impl<'ast> syn::visit::Visit<'ast> for ToolingFieldTypeAudit<'_> {
    fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
        for segment in &path.path.segments {
            let identifier = segment.ident.to_string();
            assert!(
                !self.forbidden.contains(&identifier.as_str()),
                "{} owns forbidden authority type {identifier}",
                self.source
            );
        }
        syn::visit::visit_type_path(self, path);
    }
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
