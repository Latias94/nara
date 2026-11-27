use super::*;

fn project() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("assets")).unwrap();
    std::fs::write(
        project.path().join("nara.toml"),
        "[project]\nname = 'Test game'\nversion = '0.1.0'\n",
    )
    .unwrap();
    project
}

fn job_origin(workspace: &Workspace, kind: JobKind) -> Rc<JobOrigin> {
    Rc::new(JobOrigin {
        directory: workspace.project.directory().to_path_buf(),
        kind,
    })
}

#[test]
fn play_controls_wait_for_matching_delivery_and_acknowledgment() {
    let project = project();
    let mut workspace = Workspace::open(project.path(), ProjectTrust::Untrusted).unwrap();
    let origin = job_origin(&workspace, JobKind::Play);
    let state = |state, pending, steps| Message::Play {
        state,
        pending,
        steps,
        mode: EditorPlayMode::Snapshot,
    };
    let delivery = |control, result| Message::PlayControlResult { control, result };

    workspace.pending = Some(PendingPlayControl::Queued(PlayControl::Pause));
    workspace.receive_job_message(state(PlayState::Running, None, 0), &origin);
    workspace.receive_job_message(delivery(PlayControl::Resume, Ok(())), &origin);
    assert!(workspace.control_pending());
    workspace.receive_job_message(delivery(PlayControl::Pause, Ok(())), &origin);
    workspace.receive_job_message(
        state(PlayState::Running, Some(PlayControl::Pause), 0),
        &origin,
    );
    assert!(workspace.control_pending());
    workspace.receive_job_message(state(PlayState::Paused, None, 0), &origin);
    assert!(!workspace.control_pending());
    assert!(workspace.status == JobStatus::Paused);

    workspace.pending = Some(PendingPlayControl::Queued(PlayControl::Step));
    workspace.receive_job_message(
        delivery(PlayControl::Step, Err("Control delivery failed".into())),
        &origin,
    );
    assert!(!workspace.control_pending());
    assert!(workspace.console.entries().iter().any(|entry| {
        entry.severity == DiagnosticSeverity::Warning && entry.message == "Control delivery failed"
    }));

    workspace.pending = Some(PendingPlayControl::Queued(PlayControl::Step));
    workspace.receive_job_message(delivery(PlayControl::Step, Ok(())), &origin);
    workspace.receive_job_message(state(PlayState::Paused, None, 1), &origin);
    assert!(!workspace.control_pending());
    assert_eq!(workspace.steps, 1);
    workspace.pending = Some(PendingPlayControl::Queued(PlayControl::Resume));
    workspace.receive_job_message(Message::Finished(Ok(())), &origin);
    assert!(!workspace.control_pending());
    assert!(workspace.status == JobStatus::Idle);
}

#[cfg(windows)]
mod shell {
    use std::{cell::Cell, fs};

    use nara_reflect::{ComponentSchema, ComponentSchemaVersion};
    use open_gpui::{AppContext as _, TestAppContext};
    use open_gpui_base::TextInputEvent;

    use super::*;

    fn write_scene(project: &Path, name: &str) -> (AssetPath, DocumentId, EntityUuid) {
        let document = DocumentId::try_from_uuid(uuid::Uuid::new_v4()).unwrap();
        let entity = EntityUuid::try_from_uuid(uuid::Uuid::new_v4()).unwrap();
        let mut scene = SceneDocument::new(document);
        scene.entities.push(SceneEntity::new(entity, "Original"));
        scene.write(project.join("assets").join(name)).unwrap();
        (AssetPath::new(name).unwrap(), document, entity)
    }

    #[test]
    fn opening_a_document_preserves_drafts_and_unsaved_edits() {
        let project = project();
        let (initial, initial_id, entity) = write_scene(project.path(), "initial.scene.ron");
        let (target, target_id, _) = write_scene(project.path(), "next.prefab.ron");
        let cx = TestAppContext::single();
        let (workspace, inspector) = cx.update(|cx| {
            let workspace =
                cx.new(|_| Workspace::open(project.path(), ProjectTrust::Untrusted).unwrap());
            workspace.update(cx, |workspace, cx| workspace.select([entity], cx));
            let inspector = cx.new(|cx| InspectorPanel::new(workspace.clone(), cx));
            let input = inspector.read(cx).name_input().unwrap().clone();
            input.update(cx, |input, cx| {
                input.set_value("Draft", cx);
                cx.emit(TextInputEvent::Change("Draft".into()));
            });
            (workspace, inspector)
        });
        cx.update(|cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.open_asset(&target, cx);
                assert_eq!(workspace.session.document().scene().id, initial_id);
                assert_eq!(workspace.selected_asset.as_ref(), Some(&initial));
                workspace.select([], cx);
            });
        });
        cx.update(|cx| {
            assert!(!inspector.read(cx).drafts(cx).is_empty());
            workspace.update(cx, |workspace, cx| {
                workspace.open_asset(&target, cx);
                assert_eq!(workspace.session.document().scene().id, initial_id);
                assert_eq!(workspace.selected_asset.as_ref(), Some(&initial));
                workspace.select([entity], cx);
            });
        });
        cx.update(|cx| {
            let input = inspector.read(cx).name_input().unwrap().clone();
            input.update(cx, |_, cx| cx.emit(TextInputEvent::Submit("Draft".into())));
        });
        cx.update(|cx| {
            assert!(inspector.read(cx).drafts(cx).is_empty());
            workspace.update(cx, |workspace, cx| {
                assert!(workspace.session.is_dirty());
                workspace.open_asset(&target, cx);
                assert_eq!(workspace.session.document().scene().id, initial_id);
                assert_eq!(workspace.selected_asset.as_ref(), Some(&initial));
                assert_eq!(
                    workspace.session.document().scene().entities[0].name,
                    "Draft"
                );
                workspace.undo(cx);
                workspace.open_asset(&target, cx);
                assert_eq!(workspace.session.document().scene().id, target_id);
                assert_eq!(workspace.selected_asset.as_ref(), Some(&target));
            });
        });
    }

    fn schema(component: &ComponentTypeId) -> ComponentSchemaCatalog {
        let schema = ComponentSchema::new(
            component.clone(),
            "Fixture",
            ComponentSchemaVersion::new(1).unwrap(),
        );
        ComponentSchemaCatalog::from_components([schema]).unwrap()
    }

    fn finish_schema_job(workspace: &mut Workspace, component: &ComponentTypeId) {
        let origin = job_origin(workspace, JobKind::Schema);
        workspace.status = JobStatus::Schema;
        workspace.receive_job_message(Message::Schema(schema(component)), &origin);
        workspace.receive_job_message(Message::Finished(Ok(())), &origin);
        assert!(workspace.status == JobStatus::Schema);
        assert!(workspace.session.registry().schema(component).is_none());
    }

    #[test]
    fn schema_refresh_publishes_after_gesture_commit_or_cancel() {
        let project = project();
        let component = ComponentTypeId::new("test.Fixture");
        let cx = TestAppContext::single();
        cx.update(|cx| {
            for commit in [true, false] {
                let workspace =
                    cx.new(|_| Workspace::open(project.path(), ProjectTrust::Untrusted).unwrap());
                workspace.update(cx, |workspace, cx| {
                    let gesture = workspace.begin_gesture(cx).unwrap();
                    let entity = EntityUuid::try_from_uuid(uuid::Uuid::new_v4()).unwrap();
                    assert!(workspace.apply_gesture(
                        &gesture,
                        EditorCommand::Create(SceneEntity::new(entity, "Added")),
                        cx
                    ));
                    finish_schema_job(workspace, &component);
                    if commit {
                        workspace.end_gesture(&gesture, cx);
                    } else {
                        workspace.cancel_gesture(&gesture, cx);
                    }
                    assert!(workspace.status == JobStatus::Idle);
                    assert!(workspace.session.registry().schema(&component).is_some());
                    assert_eq!(
                        workspace.session.document().scene().entities.len(),
                        usize::from(commit)
                    );
                    assert_eq!(workspace.session.can_undo(), commit);
                    assert!(workspace.deferred_schema.is_none());
                });
            }
        });
    }

    #[test]
    fn stopping_or_switching_project_rejects_a_deferred_catalog() {
        let original = project();
        let replacement = project();
        let component = ComponentTypeId::new("test.Fixture");
        let cx = TestAppContext::single();
        cx.update(|cx| {
            let workspace =
                cx.new(|_| Workspace::open(original.path(), ProjectTrust::Untrusted).unwrap());
            workspace.update(cx, |workspace, cx| {
                let gesture = workspace.begin_gesture(cx).unwrap();
                finish_schema_job(workspace, &component);
                workspace.stop(cx);
                workspace.end_gesture(&gesture, cx);
                assert!(workspace.status == JobStatus::Idle);
                assert!(workspace.session.registry().schema(&component).is_none());

                let gesture = workspace.begin_gesture(cx).unwrap();
                finish_schema_job(workspace, &component);
                workspace.project = EditorProject::open(replacement.path()).unwrap();
                workspace.end_gesture(&gesture, cx);
                assert!(workspace.status == JobStatus::Idle);
                assert!(workspace.session.registry().schema(&component).is_none());
                assert!(workspace.deferred_schema.is_none());
            });
        });
    }

    #[test]
    fn snapshot_play_keeps_its_discovered_schema_across_finish_and_gesture_exit() {
        let project = project();
        let component = ComponentTypeId::new("test.Snapshot");
        let cx = TestAppContext::single();
        cx.update(|cx| {
            for outcome in [Ok(()), Err("The game failed after discovery".to_owned())] {
                for finish_first in [true, false] {
                    let workspace = cx
                        .new(|_| Workspace::open(project.path(), ProjectTrust::Untrusted).unwrap());
                    workspace.update(cx, |workspace, cx| {
                        let origin = job_origin(workspace, JobKind::Play);
                        workspace.status = JobStatus::Building;
                        let gesture = workspace.begin_gesture(cx).unwrap();
                        workspace.receive_job_message(Message::Schema(schema(&component)), &origin);
                        workspace.receive_job_message(
                            Message::Play {
                                mode: EditorPlayMode::Snapshot,
                                state: PlayState::Running,
                                pending: None,
                                steps: 0,
                            },
                            &origin,
                        );
                        assert!(workspace.session.registry().schema(&component).is_none());
                        if finish_first {
                            workspace
                                .receive_job_message(Message::Finished(outcome.clone()), &origin);
                            assert!(workspace.status == JobStatus::Schema);
                            assert!(workspace.session.registry().schema(&component).is_none());
                            workspace.cancel_gesture(&gesture, cx);
                        } else {
                            workspace.end_gesture(&gesture, cx);
                            assert!(workspace.status == JobStatus::Running);
                            workspace
                                .receive_job_message(Message::Finished(outcome.clone()), &origin);
                        }
                        assert!(workspace.status == JobStatus::Idle);
                        assert!(workspace.deferred_schema.is_none());
                        assert!(workspace.session.registry().schema(&component).is_some());
                    });
                }
            }
        });
    }

    #[test]
    fn stale_save_as_checks_the_live_gesture_and_revision_before_the_write_callback() {
        let project = project();
        let cx = TestAppContext::single();
        for start_gesture in [false, true] {
            let before_write = Rc::new(Cell::new(false));
            let saved = Rc::new(Cell::new(None));
            let workspace = cx.update(|cx| {
                let workspace =
                    cx.new(|_| Workspace::open(project.path(), ProjectTrust::Untrusted).unwrap());
                workspace.update(cx, |workspace, cx| {
                    let checked = before_write.clone();
                    let completion = saved.clone();
                    workspace.save_with_completion(
                        cx,
                        move |_, _| {
                            checked.set(true);
                            Ok(())
                        },
                        move |result, _| completion.set(Some(result)),
                    );
                    if start_gesture {
                        assert!(workspace.begin_gesture(cx).is_some());
                    } else {
                        workspace.create(Vec2::ZERO, None, None, cx);
                    }
                });
                workspace
            });
            let path = project.path().join("assets/untitled.scene.ron");
            assert!(cx.did_prompt_for_new_path());
            cx.simulate_new_path_selection(|_| Some(path.clone()));
            cx.run_until_parked();
            assert!(!before_write.get());
            assert_eq!(saved.get(), Some(false));
            assert!(!path.exists());
            assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 0);
            cx.update(|cx| {
                let session = &workspace.read(cx).session;
                assert!(session.document().path().is_none() && session.is_dirty());
                assert_eq!(session.gesture_active(), start_gesture);
            });
        }
    }

    #[test]
    fn save_as_and_the_following_save_touch_only_the_active_scene() {
        let project = project();
        let assets = project.path().join("assets");
        let destination = assets.join("chosen.SCENE.RON");
        let entity = EntityUuid::try_from_uuid(uuid::Uuid::new_v4()).unwrap();
        let cx = TestAppContext::single();
        let workspace = cx.update(|cx| {
            let workspace =
                cx.new(|_| Workspace::open(project.path(), ProjectTrust::Untrusted).unwrap());
            workspace.update(cx, |workspace, cx| {
                workspace.apply(
                    EditorCommand::Create(SceneEntity::new(entity, "Initial")),
                    cx,
                );
                workspace.save(cx);
            });
            workspace
        });
        fs::write(assets.join("unrelated.png"), []).unwrap();
        fs::write(assets.join("other.wgsl"), []).unwrap();
        let malformed = assets.join("other.wgsl.meta");
        fs::write(&malformed, "malformed meta").unwrap();
        assert!(cx.did_prompt_for_new_path());
        cx.simulate_new_path_selection(|_| Some(destination.clone()));
        cx.run_until_parked();

        let metadata = assets.join("chosen.SCENE.RON.meta");
        let meta = fs::read_to_string(&metadata).unwrap();
        cx.update(|cx| {
            workspace.update(cx, |workspace, cx| {
                let id = workspace.opened_asset.unwrap();
                assert_eq!(id.as_uuid().get_version_num(), 7);
                assert!(meta.contains(&id.to_string()));
                assert!(!workspace.session.is_dirty());
                assert!(workspace.apply(
                    EditorCommand::Rename {
                        entity,
                        name: "Edited".into()
                    },
                    cx
                ));
                workspace.save(cx);
                assert!(!workspace.session.is_dirty());
            });
        });
        assert!(!cx.did_prompt_for_new_path());
        assert_eq!(
            SceneDocument::read(destination).unwrap().entities[0].name,
            "Edited"
        );
        assert_eq!(fs::read_to_string(metadata).unwrap(), meta);
        assert_eq!(fs::read_to_string(malformed).unwrap(), "malformed meta");
        assert!(!assets.join("unrelated.png.meta").exists());
    }
}
