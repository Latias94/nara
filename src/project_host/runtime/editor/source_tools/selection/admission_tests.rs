//! Original selection observations survive probe-admission composition.

use std::{thread, time::Instant};

use nara_fs::ContentDigest;

use super::super::tests::fixture::{Project, wait};
use super::*;

#[test]
fn returned_session_only_probe_invalidates_original_durable_enable_before_admission() {
    let project = Project::new();
    project.run(|editor| {
        wait(editor, |editor| {
            editor.source_tool_selection_view().phase == Phase::Idle
        });
        editor.enable_source_tool(project.tool()).unwrap();
        wait(editor, |editor| !editor.source_tool_view().probing);
        let predecessor = editor.source_tool_view().active.unwrap().id().clone();

        let replacement = editor.enable_source_tool(project.tool()).unwrap();
        let original = editor.source_tool_selection_view();
        let returned = editor
            .source_tools
            .probe
            .as_ref()
            .unwrap()
            .task
            .return_observer();
        let deadline = Instant::now() + Duration::from_secs(15);
        while returned.state() == TaskReturnState::Pending {
            assert!(
                Instant::now() < deadline,
                "the real replacement probe did not return"
            );
            thread::sleep(Duration::from_millis(1));
        }
        // No Session drive has consumed this physically completed replacement yet.
        assert_eq!(editor.source_tool_view().active.unwrap().id(), &predecessor);
        assert_eq!(editor.source_tool_selection_view(), original);

        // This private value tests stale admission before any installed approval or IO. It does
        // not exercise successful managed approval; the independent C# consumer covers that path.
        let prepared = PreparedEditorSourceToolSelection {
            root: editor.source_project_root(),
            desired: EditorSourceToolDesiredSelection {
                installed: EditorSourceToolInstalledSelection {
                    package: "test.finite-source-payload".into(),
                    directory: ".nara/source-tools/artifacts/finite-source-payload".into(),
                    installation: [1; 32],
                    receipt: ContentDigest::of_bytes(b"receipt"),
                    manifest: ContentDigest::of_bytes(b"manifest"),
                },
                enabled: true,
            },
            definition: project.tool(),
            graph: Graph::default(),
            basis: None,
        };
        assert_eq!(
            editor.enable_installed_source_tool(&original.basis, prepared),
            Err(Error::StaleBasis),
        );
        let current = editor.source_tool_view();
        assert_eq!(current.attempt, Some(replacement));
        assert_ne!(current.active.unwrap().id(), &predecessor);
        assert!(!current.probing);
        let selection = editor.source_tool_selection_view();
        assert_eq!(selection.operation, original.operation);
        assert_ne!(selection.basis, original.basis);
        assert!(selection.desired.is_none());
        assert!(!project.root.join(SELECTION_PATH).exists());
    });
}
