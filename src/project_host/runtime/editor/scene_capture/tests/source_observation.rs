//! Persistence custody is acquired at admission without losing unavailable working copies.

use super::*;
use crate::project_host::authoring::AuthoringPrefabSessionSnapshot;

const STARTUP_SCENE: &str = "scenes/startup.scene.json";

fn recheck_startup_preview(editor: &mut EditorProjectSession) {
    // Advance the existing background check without waiting for its periodic deadline.
    editor.scene_capture.validate_at = Some(Instant::now());
    editor.drive_editor_frame(Duration::ZERO);
    let deadline = Instant::now() + WAIT;
    while editor.scene_capture.has_pending_work() {
        editor.drive_editor_frame(Duration::ZERO);
        assert!(
            Instant::now() < deadline,
            "startup source validation did not physically return"
        );
        thread::yield_now();
    }
}

#[test]
fn startup_preview_refuses_equal_byte_leaf_replacement_before_and_after_first_validation() {
    for (replace_leaf, verified_before_replacement) in [(false, false), (true, false), (true, true)]
    {
        let fixture = Fixture::new();
        let editor = fixture.open_editor_with_limits(AuthoringProjectAdmissionLimits::default());
        fixture.run_editor(editor, |editor| {
            let startup = editor.scene_preview_document.unwrap();
            let revision = editor.workspace.document_revision(startup).unwrap();
            let reused = editor.scene_capture.captured.as_ref().unwrap();
            assert!(reused.source_capture.is_none());
            assert!(reused.sources.is_empty());
            assert!(!reused.verified);
            editor.set_editor_preview_extent(Extent2d::new(320, 180));
            let previous = verified_before_replacement
                .then(|| wait_preview(editor, startup, None).content_stamp());
            assert!(
                editor
                    .scene_capture
                    .captured
                    .as_ref()
                    .unwrap()
                    .source_capture
                    .is_none(),
                "the regression must exercise the reused startup capture"
            );
            assert!(!editor.scene_capture.has_pending_work());

            let path = fixture.root.join(STARTUP_SCENE);
            let retired = fixture.root.join("retired-startup.json");
            let bytes = fs::read(&path).unwrap();
            let relative = RelativePath::new(STARTUP_SCENE).unwrap();
            let original = fixture.capability.open_file_exact(&relative).unwrap();
            let replaced = if replace_leaf {
                match fs::rename(&path, &retired) {
                    Ok(()) => {
                        fs::write(&path, &bytes).unwrap();
                        true
                    }
                    Err(error) => {
                        assert!(
                            cfg!(windows)
                                && error.kind() == std::io::ErrorKind::PermissionDenied
                                && matches!(error.raw_os_error(), Some(5) | Some(32)),
                            "unexpected startup leaf rename failure: {error}"
                        );
                        eprintln!(
                            "native source custody prevented startup leaf replacement: {error}"
                        );
                        assert!(!retired.exists());
                        false
                    }
                }
            } else {
                false
            };
            let current = fixture.capability.open_file_exact(&relative).unwrap();
            assert_eq!(original.same_object(&current).unwrap(), !replaced);
            assert_eq!(fs::read(&path).unwrap(), bytes);

            // External changes have no synchronous watcher guarantee. Drive the real check
            // before requiring revocation of the startup candidate and its input stamp.
            recheck_startup_preview(editor);
            if replaced {
                assert!(
                    editor
                        .scene_capture
                        .captured
                        .as_ref()
                        .is_none_or(|capture| !capture.verified),
                    "an equal-byte replacement must not become or remain a verified startup root"
                );
                assert!(editor.current_editor_preview_content_stamp().is_none());
                assert!(editor.take_editor_preview_candidate().is_none());
            } else {
                // Both the unchanged control and a native refusal preserve the original binding.
                if let Some(previous) = &previous {
                    assert_eq!(
                        editor.current_editor_preview_content_stamp().as_ref(),
                        Some(previous)
                    );
                } else {
                    wait_preview(editor, startup, None);
                }
                assert!(editor.scene_capture.captured.as_ref().unwrap().verified);
            }
            assert_eq!(editor.workspace.document_revision(startup), Some(revision));
            assert_eq!(editor.workspace.document_is_dirty(startup), Some(false));
        });
    }
}

#[test]
fn source_handoff_preserves_the_accepted_digest_and_worker_preparation_rejects_changed_bytes() {
    for (path, kind) in [
        (SECOND, AuthoringDocumentKind::Scene),
        (LEAF, AuthoringDocumentKind::Prefab),
    ] {
        let fixture = Fixture::new();
        fixture.run(|editor| {
            let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
            let document = open_document(editor, path, kind);
            let selected = editor.authoring_source_selection(document).unwrap();
            let source = selected.receipt.as_ref().unwrap();
            let (selection, mut inputs) = editor.capture_selection().unwrap().unwrap();
            assert_eq!(selection.root.document, root);
            let mut changed = fs::read(fixture.root.join(path)).unwrap();
            changed.push(b'\n');
            fs::write(fixture.root.join(path), changed).unwrap();

            assert!(editor.authoring_source_selection(document).as_ref() == Some(&selected));
            assert!(editor.authoring_source_selection_is_current(&selected));
            let observation = editor
                .capture_authoring_source_observation(&selected, &mut inputs)
                .expect("handoff compares live custody without reading the body");
            assert_eq!(
                observation.observation(),
                (source.identity(), source.parent_identity(), source.digest())
            );
            let (root_observation, prefabs) = match kind {
                AuthoringDocumentKind::Scene => (observation, Vec::new().into_boxed_slice()),
                AuthoringDocumentKind::Prefab => {
                    let prefab = AuthoringPrefabSessionSnapshot::capture(
                        source,
                        observation,
                        editor.workspace.prefab(document).unwrap().session(),
                    );
                    let root_observation = editor
                        .capture_authoring_source_observation(&selection.root, &mut inputs)
                        .unwrap();
                    (root_observation, vec![prefab].into_boxed_slice())
                }
            };
            let error = editor
                .authoring_project()
                .unwrap()
                .capture_scene_request(
                    selection.root.receipt.as_ref().unwrap(),
                    root_observation,
                    editor.workspace.scene(root).unwrap().session(),
                    inputs,
                    prefabs,
                )
                .unwrap()
                .prepare()
                .err()
                .expect("the worker must compare current content with the accepted digest");
            assert!(matches!(
                error,
                AuthoringProjectSourceAdmissionError::SourceChanged {
                    path: changed_path,
                    ..
                } if changed_path.as_str() == path
            ));
        });
    }
}

#[test]
fn another_source_cannot_reuse_a_documents_accepted_tuple_for_observation_handoff() {
    for (path, other_path, kind) in [
        (
            SECOND,
            "scenes/startup.scene.json",
            AuthoringDocumentKind::Scene,
        ),
        (
            LEAF,
            "prefabs/unrelated.prefab.json",
            AuthoringDocumentKind::Prefab,
        ),
    ] {
        let fixture = Fixture::new();
        fixture.run(|editor| {
            let document = open_document(editor, path, kind);
            let other = open_document(editor, other_path, kind);
            let selected = editor.authoring_source_selection(document).unwrap();
            let source = selected.receipt.as_ref().unwrap();
            let mut foreign = editor.authoring_source_selection(other).unwrap();
            foreign.document = document;
            foreign.receipt = Some(
                foreign
                    .receipt
                    .as_ref()
                    .unwrap()
                    .with_persistence_observation(
                        source.identity(),
                        source.parent_identity(),
                        source.digest(),
                    )
                    .unwrap(),
            );
            let mut inputs = editor
                .authoring_project()
                .unwrap()
                .admit_source_capture_inputs(0, 0)
                .unwrap();
            assert!(matches!(
                editor.capture_authoring_source_observation(&foreign, &mut inputs),
                Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
            ));
        });
    }
}

#[test]
fn unavailable_open_prefab_stays_in_the_snapshot_set_and_refuses_only_when_demanded() {
    for demanded in [false, true] {
        let fixture = Fixture::new();
        fixture.run(|editor| {
            let leaf = open_document(editor, LEAF, AuthoringDocumentKind::Prefab);
            let unrelated = open_document(
                editor,
                "prefabs/unrelated.prefab.json",
                AuthoringDocumentKind::Prefab,
            );
            let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
            let (selection, mut inputs) = editor.capture_selection().unwrap().unwrap();
            assert_eq!(selection.root.document, root);
            let observation = editor
                .capture_authoring_source_observation(&selection.root, &mut inputs)
                .unwrap();
            let blocked = if demanded { leaf } else { unrelated };
            let host = editor.prefab_lanes.get(&blocked).unwrap().host.clone();
            let guard = host.try_bind().unwrap();
            let prefabs = editor
                .capture_open_prefabs(&selection.prefabs, &mut inputs, None)
                .unwrap();
            assert_eq!(prefabs.len(), selection.prefabs.len());
            drop(guard);
            let scenes = editor
                .capture_open_scenes(&selection.scenes, &mut inputs)
                .unwrap();
            let result = editor
                .authoring_project()
                .unwrap()
                .capture_scene_request(
                    selection.root.receipt.as_ref().unwrap(),
                    observation,
                    editor.workspace.scene(root).unwrap().session(),
                    inputs,
                    prefabs,
                )
                .unwrap()
                .with_open_scenes(scenes)
                .prepare();
            if demanded {
                let error = result
                    .err()
                    .expect("an unavailable open dependency must refuse");
                assert!(matches!(
                    error,
                    AuthoringProjectSourceAdmissionError::SourceChanged {
                        kind: crate::project_host::authoring::AuthoringSourceKind::Prefab,
                        path,
                    } if path.as_str() == LEAF
                ));
            } else {
                let capture = result.expect("an unrelated unavailable Prefab is not demanded");
                assert!(!capture.requires_recapture());
            }
        });
    }
}

#[test]
fn unavailable_open_scene_stays_in_the_snapshot_set_and_refuses_only_when_demanded() {
    for demanded in [false, true] {
        let fixture = scene_dependencies::scene_fixture();
        fixture.run(|editor| {
            let dependent = open_document(
                editor,
                "scenes/dependent.scene.json",
                AuthoringDocumentKind::Scene,
            );
            let unrelated = open_document(
                editor,
                "scenes/unrelated.scene.json",
                AuthoringDocumentKind::Scene,
            );
            let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
            let (selection, mut inputs) = editor.capture_selection().unwrap().unwrap();
            assert_eq!(selection.root.document, root);
            let observation = editor
                .capture_authoring_source_observation(&selection.root, &mut inputs)
                .unwrap();
            let blocked = if demanded { dependent } else { unrelated };
            let host = editor.scene_lanes.get(&blocked).unwrap().host.clone();
            let guard = host.try_bind().unwrap();
            let scenes = editor
                .capture_open_scenes(&selection.scenes, &mut inputs)
                .unwrap();
            assert_eq!(scenes.len(), selection.scenes.len());
            drop(guard);
            let prefabs = editor
                .capture_open_prefabs(&selection.prefabs, &mut inputs, None)
                .unwrap();
            let result = editor
                .authoring_project()
                .unwrap()
                .capture_scene_request(
                    selection.root.receipt.as_ref().unwrap(),
                    observation,
                    editor.workspace.scene(root).unwrap().session(),
                    inputs,
                    prefabs,
                )
                .unwrap()
                .with_open_scenes(scenes)
                .prepare();
            if demanded {
                let error = result
                    .err()
                    .expect("an unavailable open dependency must refuse");
                assert!(matches!(
                    error,
                    AuthoringProjectSourceAdmissionError::SourceChanged {
                        kind: crate::project_host::authoring::AuthoringSourceKind::Scene,
                        path,
                    } if path.as_str() == "scenes/dependent.scene.json"
                ));
            } else {
                let capture = result.expect("an unrelated unavailable Scene is not demanded");
                assert!(!capture.requires_recapture());
            }
        });
    }
}
