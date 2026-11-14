use std::path::Path;

use nara_asset::{AssetRef, AssetSourceChange, AssetSourceChangeKind, AssetSourcePath};
use nara_scene::{PrefabDocument, PrefabInstance, PrefabInstanceBinding};

use super::*;

#[test]
fn source_updates_without_project_owner_do_not_create_authority_or_tasks() {
    let mut world = World::new();
    let change = modified("scenes/startup.scene.json");
    let mut access = SystemState::<ProjectScenes>::new(&mut world);
    {
        let mut scenes = access.get_mut(&mut world).unwrap();
        assert_eq!(scenes.source_update_status(), None);
        assert_eq!(
            scenes.notify_source_change(&change),
            Err(ProjectSceneRequestError::Unavailable)
        );
        assert_eq!(
            scenes.request_source_rescan(),
            Err(ProjectSceneRequestError::Unavailable)
        );
    }
    ProjectScenes::scope(&mut world, |scenes| {
        assert_eq!(scenes.source_update_status(), None);
        assert_eq!(
            scenes.notify_source_change(&change),
            Err(ProjectSceneRequestError::Unavailable)
        );
        assert_eq!(
            scenes.request_source_rescan(),
            Err(ProjectSceneRequestError::Unavailable)
        );
    });
    poll_project_scene_requests(&mut world);
    assert!(!world.contains_resource::<RequestOwner>());
    assert!(!world.contains_resource::<TaskPools>());
    assert!(CurrentScene::get(&world).is_none());
}

#[test]
fn source_updates_require_an_original_published_source_closure() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let Context::Source {
        candidate,
        plan,
        loader,
        ..
    } = context.as_ref()
    else {
        unreachable!()
    };
    let snapshot = loader.load(candidate, plan).unwrap();
    let source = snapshot.scene_source();
    drop(snapshot);
    with_context_world(context, source, |world| {
        world.insert_resource(pools());
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(scenes.source_update_status(), None);
            assert_eq!(
                scenes.notify_source_change(&modified("scenes/startup.scene.json")),
                Err(ProjectSceneRequestError::Unavailable)
            );
            assert_eq!(
                scenes.request_source_rescan(),
                Err(ProjectSceneRequestError::Unavailable)
            );
        });
        poll_project_scene_requests(world);
        assert_eq!(io_admitted(world), 0);
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
    });
}

#[test]
fn optional_scene_sidecar_notices_match_the_principal_before_metadata_exists() {
    let fixture = Fixture::new();
    assert!(!fixture.root.join("scenes/startup.scene.json.meta").exists());
    fixture.with_world(|world| {
        world.insert_resource(pools());
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        ProjectScenes::scope(world, |scenes| {
            let unrelated = AssetSourceChange::new(
                AssetSourcePath::new("assets/startup.scene.json").unwrap(),
                AssetSourceChangeKind::MetaModified,
            );
            assert!(!scenes.notify_source_change(&unrelated).unwrap());
            for change in [
                AssetSourceChange::new(
                    AssetSourcePath::new("scenes/startup.scene.json").unwrap(),
                    AssetSourceChangeKind::MetaModified,
                ),
                AssetSourceChange::metadata_removed(
                    AssetSourcePath::new("scenes/startup.scene.json").unwrap(),
                ),
            ] {
                assert!(scenes.notify_source_change(&change).unwrap());
                assert_eq!(
                    scenes.source_update_status(),
                    Some(ProjectSceneSourceUpdateStatus {
                        request: None,
                        pending: true,
                        error: None,
                    })
                );
            }
        });
        assert_eq!(io_admitted(world), 0);
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
        poll_project_scene_requests(world);
        let request = status(world).request.unwrap();
        wait_for_request(world, request, ProjectSceneRequestState::Ready);
        ProjectScenes::scope(world, |scenes| {
            let prepared = scenes.prepared(request).unwrap();
            assert_eq!(prepared.scene().roots(), [entity("startup")]);
            assert_eq!(prepared.selected_scene_path(), Some(&startup()));
            assert!(scenes.cancel(request));
            assert!(scenes.acknowledge(request));
        });
        poll_project_scene_requests(world);
        assert_eq!(io_admitted(world), 1);
        assert_eq!(
            status(world),
            ProjectSceneSourceUpdateStatus {
                request: None,
                pending: false,
                error: None,
            }
        );
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
    });
}

#[test]
fn source_update_admission_preserves_pending_and_error_until_tasks_can_start() {
    Fixture::new().with_world(|world| {
        let context = Arc::clone(&world.resource::<RequestOwner>().context);
        let Context::Source { loader, .. } = context.as_ref() else {
            unreachable!()
        };
        let baseline = loader.budget_snapshot();
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(
                scenes.source_update_status(),
                Some(ProjectSceneSourceUpdateStatus {
                    request: None,
                    pending: false,
                    error: None
                })
            );
            assert!(
                !scenes
                    .notify_source_change(&modified("scenes/unrelated.scene.json"))
                    .unwrap()
            );
            assert!(
                scenes
                    .notify_source_change(&modified("scenes/startup.scene.json"))
                    .unwrap()
            );
        });
        assert_eq!(
            status(world),
            ProjectSceneSourceUpdateStatus {
                request: None,
                pending: true,
                error: None
            }
        );
        assert_active_budget(&context, baseline);
        for _ in 0..3 {
            poll_project_scene_requests(world);
            assert_eq!(
                status(world),
                ProjectSceneSourceUpdateStatus {
                    request: None,
                    pending: true,
                    error: Some(ProjectSceneRequestError::TaskUnavailable),
                }
            );
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            assert!(!world.contains_resource::<TaskPools>());
            assert_active_budget(&context, baseline);
        }
        world.insert_resource(pools());
        let (gate, control) = TestGate::new(TestPhase::BeforeRead);
        world.resource_mut::<RequestOwner>().gate = Some(gate);
        poll_project_scene_requests(world);
        let request = status(world).request.unwrap();
        assert!(control.wait_until_started());
        assert_eq!(
            status(world),
            ProjectSceneSourceUpdateStatus {
                request: Some(request),
                pending: false,
                error: None
            }
        );
        assert_eq!(io_admitted(world), 1);
        control.release();
        wait_for_request(world, request, ProjectSceneRequestState::Ready);
        for _ in 0..3 {
            poll_project_scene_requests(world);
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(scenes.state(request), Some(ProjectSceneRequestState::Ready));
                assert_eq!(
                    scenes.prepared(request).unwrap().selected_scene_path(),
                    Some(&startup())
                );
                assert!(!scenes.acknowledge(request));
            });
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            assert_eq!(io_admitted(world), 1);
        }
        ProjectScenes::scope(world, |scenes| {
            assert!(scenes.cancel(request));
            assert!(scenes.acknowledge(request));
        });
        assert_eq!(
            status(world),
            ProjectSceneSourceUpdateStatus {
                request: None,
                pending: false,
                error: None
            }
        );
        assert_active_budget(&context, baseline);
    });
}

#[test]
fn preparing_notices_coalesce_and_cancellation_retains_capture_until_return_and_exact_ack() {
    for phase in [TestPhase::BeforeRead, TestPhase::AfterPreparation] {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        fixture.with_world(move |world| {
            world.insert_resource(pools());
            let context = Arc::clone(&world.resource::<RequestOwner>().context);
            let Context::Source { loader, .. } = context.as_ref() else {
                unreachable!()
            };
            let baseline = loader.budget_snapshot();
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let original_source = CurrentScene::get(world).unwrap().source_view();
            write_scene(&root, "startup.scene.json", "captured-a");
            let (gate, control) = TestGate::new(phase);
            world.resource_mut::<RequestOwner>().gate = Some(gate);
            ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
            assert!(status(world).request.is_none());
            assert_eq!(io_admitted(world), 0);
            poll_project_scene_requests(world);
            let request = status(world).request.unwrap();
            assert!(control.wait_until_started());
            for _ in 0..3 {
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Preparing)
                    );
                    assert!(
                        scenes
                            .notify_source_change(&modified("prefabs/not-yet-known.prefab.json"))
                            .unwrap()
                    );
                    assert!(
                        scenes
                            .notify_source_change(&modified("scenes/startup.scene.json"))
                            .unwrap()
                    );
                    scenes.request_source_rescan().unwrap();
                });
                poll_project_scene_requests(world);
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: Some(request),
                        pending: true,
                        error: None
                    }
                );
                assert_eq!(io_admitted(world), 1);
            }
            ProjectScenes::scope(world, |scenes| assert!(scenes.cancel(request)));
            write_scene(&root, "startup.scene.json", "successor-b");
            for _ in 0..3 {
                poll_project_scene_requests(world);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Cancelling)
                    );
                    assert!(!scenes.cancel(request));
                    assert!(!scenes.acknowledge(request));
                    assert!(scenes.prepared(request).is_none());
                    assert_eq!(
                        scenes.request(selected()),
                        Err(ProjectSceneRequestError::Busy)
                    );
                });
                assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
                assert_eq!(
                    original_source.with_document(|document| document.roots().to_vec()),
                    Some(vec![entity("startup")])
                );
                assert_eq!(io_admitted(world), 1);
                if phase == TestPhase::AfterPreparation {
                    assert_eq!(
                        loader.budget_snapshot().active_generations(),
                        baseline.active_generations() + 1
                    );
                }
            }
            control.release();
            wait_for_request(world, request, ProjectSceneRequestState::Cancelled);
            assert_active_budget(&context, baseline);
            for _ in 0..3 {
                poll_project_scene_requests(world);
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: Some(request),
                        pending: true,
                        error: None
                    }
                );
                assert_eq!(io_admitted(world), 1);
                ProjectScenes::scope(world, |scenes| {
                    assert!(
                        !scenes
                            .notify_source_change(&modified("prefabs/terminal-only.prefab.json"))
                            .unwrap()
                    );
                    assert_eq!(
                        scenes.request(selected()),
                        Err(ProjectSceneRequestError::Busy)
                    );
                });
            }
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
            assert_eq!(
                status(world),
                ProjectSceneSourceUpdateStatus {
                    request: None,
                    pending: true,
                    error: None
                }
            );
            poll_project_scene_requests(world);
            let successor = status(world).request.unwrap();
            assert_ne!(successor, request);
            wait_for_request(world, successor, ProjectSceneRequestState::Ready);
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(scenes.state(request), None);
                assert!(!scenes.cancel(request));
                assert!(!scenes.acknowledge(request));
                assert_eq!(
                    scenes.prepared(successor).unwrap().scene().roots(),
                    [entity("successor-b")]
                );
                assert_eq!(
                    scenes.prepared(successor).unwrap().expected_instance(),
                    &expected
                );
            });
            for _ in 0..3 {
                poll_project_scene_requests(world);
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: Some(successor),
                        pending: false,
                        error: None
                    }
                );
                assert_eq!(io_admitted(world), 2);
                assert_eq!(
                    loader.budget_snapshot().active_generations(),
                    baseline.active_generations() + 1
                );
            }
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.cancel(successor));
                assert!(!scenes.acknowledge(request));
                assert!(scenes.acknowledge(successor));
            });
            for _ in 0..3 {
                poll_project_scene_requests(world);
                assert_eq!(io_admitted(world), 2);
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: None,
                        pending: false,
                        error: None
                    }
                );
            }
            assert_active_budget(&context, baseline);
            assert_eq!(loader.budget_snapshot().generation_high_water(), 2);
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
        });
    }
}

#[test]
fn ready_source_updates_publish_frozen_a_then_prepare_b_only_after_exact_ack() {
    for managed in [false, true] {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        fixture.with_world(move |world| {
            world.insert_resource(pools());
            let context = Arc::clone(&world.resource::<RequestOwner>().context);
            let Context::Source { loader, .. } = context.as_ref() else {
                unreachable!()
            };
            let baseline = loader.budget_snapshot();
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let original_stamp = CurrentScene::get(world)
                .unwrap()
                .source_content_stamp()
                .unwrap();
            let original_source = CurrentScene::get(world).unwrap().source_view();
            write_scene(&root, "startup.scene.json", "frozen-a");
            ProjectScenes::scope(world, |scenes| {
                assert!(
                    scenes
                        .notify_source_change(&modified("scenes/startup.scene.json"))
                        .unwrap()
                );
            });
            assert_eq!(io_admitted(world), 0);
            poll_project_scene_requests(world);
            let request = status(world).request.unwrap();
            wait_for_request(world, request, ProjectSceneRequestState::Ready);
            let frozen_stamp = ProjectScenes::scope(world, |scenes| {
                scenes
                    .prepared(request)
                    .unwrap()
                    .source_content_stamp()
                    .unwrap()
            });
            assert_ne!(frozen_stamp, original_stamp);
            write_scene(&root, "startup.scene.json", "successor-b");
            if !managed {
                ProjectScenes::scope(world, |scenes| {
                    assert!(
                        scenes
                            .notify_source_change(&modified("scenes/startup.scene.json"))
                            .unwrap()
                    );
                    assert!(
                        scenes
                            .notify_source_change(&modified(
                                "prefabs/unknown-while-ready.prefab.json"
                            ))
                            .unwrap()
                    );
                });
            }
            for _ in 0..3 {
                poll_project_scene_requests(world);
                ProjectScenes::scope(world, |scenes| {
                    let prepared = scenes.prepared(request).unwrap();
                    assert_eq!(prepared.scene().roots(), [entity("frozen-a")]);
                    assert_eq!(prepared.source_content_stamp(), Some(frozen_stamp));
                    assert_eq!(
                        prepared.selected_scene(),
                        &ProjectSceneSelection::Source(startup())
                    );
                    assert_eq!(prepared.selected_scene_path(), Some(&startup()));
                    assert_eq!(prepared.selected_scene_product(), None);
                    assert_eq!(prepared.expected_instance(), &expected);
                    assert!(!scenes.acknowledge(request));
                });
                assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
                assert_eq!(
                    CurrentScene::get(world).unwrap().source_content_stamp(),
                    Some(original_stamp)
                );
                assert_eq!(
                    loader.budget_snapshot().active_generations(),
                    baseline.active_generations() + 1
                );
                assert_eq!(io_admitted(world), 1);
                assert_eq!(status(world).pending, !managed);
            }
            if managed {
                begin_prepared_project_scene_travel(world, request, limits(), &[], |_| {}).unwrap();
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Retiring)
                    );
                    assert!(!scenes.source_update_status().unwrap().pending);
                    assert!(
                        scenes
                            .notify_source_change(&modified(
                                "prefabs/unknown-while-managed.prefab.json"
                            ))
                            .unwrap()
                    );
                    assert!(!scenes.acknowledge(request));
                });
                poll_project_scene_requests(world);
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: Some(request),
                        pending: true,
                        error: None
                    }
                );
                assert_eq!(io_admitted(world), 1);
                assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
                assert_eq!(
                    CurrentScene::get(world).unwrap().source_content_stamp(),
                    Some(original_stamp)
                );
                world.resource_mut::<nara_app::RealTime>().frame += 1;
                poll_project_scene_requests(world);
            } else {
                commit(world, request);
            }
            let committed = CurrentScene::get(world).unwrap().receipt().clone();
            assert_ne!(committed, expected);
            assert_eq!(
                CurrentScene::get(world).unwrap().source().roots(),
                [entity("frozen-a")]
            );
            assert_eq!(
                CurrentScene::get(world).unwrap().source_content_stamp(),
                Some(frozen_stamp)
            );
            assert_eq!(original_source.with_document(|_| ()), None);
            assert_eq!(
                loader.budget_snapshot().active_generations(),
                baseline.active_generations()
            );
            let frozen_source = CurrentScene::get(world).unwrap().source_view();
            for _ in 0..3 {
                poll_project_scene_requests(world);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Committed)
                    );
                    assert!(
                        !scenes
                            .notify_source_change(&modified(
                                "prefabs/unknown-after-commit.prefab.json"
                            ))
                            .unwrap()
                    );
                    assert_eq!(
                        scenes.request(selected()),
                        Err(ProjectSceneRequestError::Busy)
                    );
                });
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: Some(request),
                        pending: true,
                        error: None
                    }
                );
                assert_eq!(io_admitted(world), 1);
            }
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
            assert!(status(world).request.is_none());
            assert!(status(world).pending);
            poll_project_scene_requests(world);
            let successor = status(world).request.unwrap();
            assert_ne!(successor, request);
            wait_for_request(world, successor, ProjectSceneRequestState::Ready);
            let next_stamp = ProjectScenes::scope(world, |scenes| {
                assert!(!scenes.acknowledge(request));
                let prepared = scenes.prepared(successor).unwrap();
                assert_eq!(prepared.expected_instance(), &committed);
                assert_eq!(prepared.selected_scene_path(), Some(&startup()));
                assert_eq!(prepared.scene().roots(), [entity("successor-b")]);
                prepared.source_content_stamp().unwrap()
            });
            assert_ne!(next_stamp, frozen_stamp);
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &committed);
            assert_eq!(
                loader.budget_snapshot().active_generations(),
                baseline.active_generations() + 1
            );
            commit(world, successor);
            assert_eq!(
                CurrentScene::get(world).unwrap().source().roots(),
                [entity("successor-b")]
            );
            assert_eq!(
                CurrentScene::get(world).unwrap().source_content_stamp(),
                Some(next_stamp)
            );
            assert_eq!(frozen_source.with_document(|_| ()), None);
            ProjectScenes::scope(world, |scenes| {
                assert!(!scenes.acknowledge(request));
                assert!(scenes.acknowledge(successor));
            });
            for _ in 0..3 {
                poll_project_scene_requests(world);
                assert_eq!(io_admitted(world), 2);
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: None,
                        pending: false,
                        error: None
                    }
                );
            }
            assert_eq!(
                loader.budget_snapshot().active_generations(),
                baseline.active_generations()
            );
            assert_eq!(loader.budget_snapshot().generation_high_water(), 2);
        });
    }
}

#[test]
fn invalid_source_update_retains_incumbent_and_error_until_repair_starts() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture.with_world(move |world| {
        world.insert_resource(pools());
        let context = Arc::clone(&world.resource::<RequestOwner>().context);
        let Context::Source { loader, .. } = context.as_ref() else {
            unreachable!()
        };
        let baseline = loader.budget_snapshot();
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        let stamp = CurrentScene::get(world).unwrap().source_content_stamp();
        let source = CurrentScene::get(world).unwrap().source_view();
        fs::write(root.join("scenes/startup.scene.json"), b"{ malformed").unwrap();
        ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
        poll_project_scene_requests(world);
        let request = status(world).request.unwrap();
        let error = ProjectSceneRequestError::Content(ProjectContentErrorKind::SceneFormat);
        wait_for_request(world, request, ProjectSceneRequestState::Rejected(error));
        ProjectScenes::scope(world, |scenes| {
            assert!(
                scenes
                    .diagnostics(request)
                    .unwrap()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str()
                        == "project.content.scene-format-invalid")
            );
            assert!(scenes.prepared(request).is_none());
            assert!(
                !scenes
                    .notify_source_change(&modified("prefabs/not-read.prefab.json"))
                    .unwrap()
            );
        });
        assert_eq!(
            status(world),
            ProjectSceneSourceUpdateStatus {
                request: Some(request),
                pending: false,
                error: Some(error)
            }
        );
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
        assert_eq!(
            CurrentScene::get(world).unwrap().source_content_stamp(),
            stamp
        );
        assert_eq!(
            source.with_document(|document| document.roots().to_vec()),
            Some(vec![entity("startup")])
        );
        assert_eq!(
            loader.budget_snapshot().active_generations(),
            baseline.active_generations()
        );
        for _ in 0..3 {
            poll_project_scene_requests(world);
            assert_eq!(io_admitted(world), 1);
        }
        ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
        assert_eq!(
            status(world),
            ProjectSceneSourceUpdateStatus {
                request: None,
                pending: false,
                error: Some(error)
            }
        );
        write_scene(&root, "startup.scene.json", "repaired");
        ProjectScenes::scope(world, |scenes| {
            assert!(
                scenes
                    .notify_source_change(&modified("scenes/startup.scene.json"))
                    .unwrap()
            );
        });
        assert_eq!(status(world).error, Some(error));
        poll_project_scene_requests(world);
        let repair = status(world).request.unwrap();
        assert_ne!(repair, request);
        assert_eq!(status(world).error, None);
        wait_for_request(world, repair, ProjectSceneRequestState::Ready);
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
        commit(world, repair);
        assert_eq!(
            CurrentScene::get(world).unwrap().source().roots(),
            [entity("repaired")]
        );
        assert_eq!(source.with_document(|_| ()), None);
        assert_eq!(status(world).error, None);
        ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(repair)));
        assert_eq!(io_admitted(world), 2);
        assert_eq!(
            loader.budget_snapshot().active_generations(),
            baseline.active_generations()
        );
    });
}

#[test]
fn latest_automatic_missing_path_cut_survives_unrelated_failure_and_repairs_by_notice() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture.with_world(move |world| {
        world.insert_resource(pools());
        let context = Arc::clone(&world.resource::<RequestOwner>().context);
        let Context::Source { loader, .. } = context.as_ref() else {
            unreachable!()
        };
        let baseline = loader.budget_snapshot();
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        let stamp = CurrentScene::get(world).unwrap().source_content_stamp();
        let mut last_error = None;
        let mut failed_reservations = None;
        for missing in ["first.prefab.json", "latest.prefab.json"] {
            write_scene_with_prefab(&root, "startup.scene.json", "needs-prefab", missing);
            ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
            poll_project_scene_requests(world);
            let request = status(world).request.unwrap();
            let error = wait_rejected(world, request);
            assert!(matches!(error, ProjectSceneRequestError::Content(_)));
            assert_eq!(
                status(world),
                ProjectSceneSourceUpdateStatus {
                    request: Some(request),
                    pending: false,
                    error: Some(error)
                }
            );
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.diagnostics(request).unwrap().has_errors());
                assert!(scenes.acknowledge(request));
            });
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            assert_eq!(
                CurrentScene::get(world).unwrap().source_content_stamp(),
                stamp
            );
            assert_eq!(
                loader.budget_snapshot().active_generations(),
                baseline.active_generations()
            );
            let reservations = loader.budget_snapshot().active_reservations();
            assert!(reservations > baseline.active_reservations());
            if let Some(previous) = failed_reservations {
                assert_eq!(reservations, previous);
                ProjectScenes::scope(world, |scenes| {
                    assert!(
                        !scenes
                            .notify_source_change(&modified("prefabs/first.prefab.json"))
                            .unwrap()
                    );
                });
            }
            failed_reservations = Some(reservations);
            last_error = Some(error);
        }
        let failed_budget = loader.budget_snapshot();
        write_scene_with_prefab(
            &root,
            "second.scene.json",
            "unrelated",
            "explicit.prefab.json",
        );
        let explicit = ProjectScenes::scope(world, |scenes| scenes.request(selected())).unwrap();
        wait_rejected(world, explicit);
        assert_eq!(
            status(world),
            ProjectSceneSourceUpdateStatus {
                request: None,
                pending: false,
                error: last_error
            }
        );
        assert_active_budget(&context, failed_budget);
        ProjectScenes::scope(world, |scenes| {
            assert!(
                !scenes
                    .notify_source_change(&modified("prefabs/explicit.prefab.json"))
                    .unwrap()
            );
            assert!(
                !scenes
                    .notify_source_change(&modified("prefabs/first.prefab.json"))
                    .unwrap()
            );
            assert!(scenes.acknowledge(explicit));
        });
        write_prefab(&root, "latest.prefab.json", "repaired-child");
        ProjectScenes::scope(world, |scenes| {
            assert!(
                scenes
                    .notify_source_change(&modified("prefabs/latest.prefab.json"))
                    .unwrap()
            );
        });
        poll_project_scene_requests(world);
        let repair = status(world).request.unwrap();
        assert_eq!(status(world).error, None);
        wait_for_request(world, repair, ProjectSceneRequestState::Ready);
        ProjectScenes::scope(world, |scenes| {
            let prepared = scenes.prepared(repair).unwrap();
            assert_eq!(prepared.selected_scene_path(), Some(&startup()));
            assert_eq!(prepared.scene().roots(), [entity("needs-prefab")]);
            assert_eq!(prepared.scene().entities().len(), 2);
            assert_eq!(prepared.expected_instance(), &expected);
        });
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
        commit(world, repair);
        ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(repair)));
        assert_ne!(CurrentScene::get(world).unwrap().receipt(), &expected);
        assert_eq!(
            CurrentScene::get(world).unwrap().source().entities().len(),
            2
        );
        assert_eq!(
            status(world),
            ProjectSceneSourceUpdateStatus {
                request: None,
                pending: false,
                error: None
            }
        );
        ProjectScenes::scope(world, |scenes| {
            assert!(
                !scenes
                    .notify_source_change(&modified("prefabs/first.prefab.json"))
                    .unwrap()
            );
            assert!(
                !scenes
                    .notify_source_change(&modified("prefabs/explicit.prefab.json"))
                    .unwrap()
            );
        });
        assert_eq!(io_admitted(world), 4);
        assert_eq!(
            loader.budget_snapshot().active_generations(),
            baseline.active_generations()
        );
    });
}

#[test]
fn cancelled_and_stale_late_failures_cannot_replace_previous_retry_interest() {
    for stale in [false, true] {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        fixture.with_world(move |world| {
            world.insert_resource(pools());
            let context = Arc::clone(&world.resource::<RequestOwner>().context);
            let Context::Source { loader, .. } = context.as_ref() else {
                unreachable!()
            };
            let mut expected = CurrentScene::get(world).unwrap().receipt().clone();
            let stamp = CurrentScene::get(world).unwrap().source_content_stamp();
            write_scene_with_prefab(&root, "startup.scene.json", "first", "kept.prefab.json");
            ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
            poll_project_scene_requests(world);
            let first = status(world).request.unwrap();
            wait_rejected(world, first);
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(first)));
            let baseline = loader.budget_snapshot();
            write_scene_with_prefab(&root, "startup.scene.json", "late", "late.prefab.json");
            let (gate, control) = TestGate::new(TestPhase::AfterPreparation);
            world.resource_mut::<RequestOwner>().gate = Some(gate);
            ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
            poll_project_scene_requests(world);
            assert!(control.wait_until_started());
            let (late, terminal) = if stale {
                ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
                let report = crate::startup_scene::restart_current_scene_with_product(
                    world,
                    &expected,
                    limits(),
                    &[],
                    |_| {},
                );
                assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
                let restarted = report.instance.unwrap();
                assert_ne!(restarted, expected);
                expected = restarted;
                let observed = status(world);
                let late = observed.request.unwrap();
                assert_ne!(late, first);
                assert!(observed.pending);
                assert_eq!(observed.error, None);
                (
                    late,
                    ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Stale),
                )
            } else {
                let late = status(world).request.unwrap();
                assert_eq!(status(world).error, None);
                ProjectScenes::scope(world, |scenes| assert!(scenes.cancel(late)));
                (late, ProjectSceneRequestState::Cancelled)
            };
            for _ in 0..3 {
                poll_project_scene_requests(world);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(late),
                        Some(ProjectSceneRequestState::Cancelling)
                    );
                    assert!(scenes.prepared(late).is_none());
                    assert!(!scenes.acknowledge(late));
                    assert_eq!(
                        scenes.request(selected()),
                        Err(ProjectSceneRequestError::Busy)
                    );
                });
                assert_eq!(io_admitted(world), 2);
                assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
                assert_eq!(
                    CurrentScene::get(world).unwrap().source_content_stamp(),
                    stamp
                );
            }
            control.release();
            wait_for_request(world, late, terminal);
            assert_active_budget(&context, baseline);
            if stale {
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.source_update_status(),
                        Some(ProjectSceneSourceUpdateStatus {
                            request: Some(late),
                            pending: true,
                            error: None,
                        })
                    );
                    assert_eq!(
                        scenes.notify_source_change(&modified("prefabs/late.prefab.json")),
                        Err(ProjectSceneRequestError::Stale)
                    );
                    assert_eq!(
                        scenes.notify_source_change(&modified("prefabs/kept.prefab.json")),
                        Err(ProjectSceneRequestError::Stale)
                    );
                    assert_eq!(
                        scenes.request_source_rescan(),
                        Err(ProjectSceneRequestError::Stale)
                    );
                    assert!(!scenes.acknowledge(first));
                    assert!(scenes.acknowledge(late));
                });
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: None,
                        pending: true,
                        error: None
                    }
                );
                for _ in 0..3 {
                    poll_project_scene_requests(world);
                    assert_eq!(io_admitted(world), 2);
                    assert_active_budget(&context, baseline);
                    ProjectScenes::scope(world, |scenes| {
                        assert_eq!(
                            scenes.source_update_status(),
                            Some(ProjectSceneSourceUpdateStatus {
                                request: None,
                                pending: true,
                                error: Some(ProjectSceneRequestError::Stale),
                            })
                        );
                        assert_eq!(scenes.state(late), None);
                    });
                }
                let explicit =
                    ProjectScenes::scope(world, |scenes| scenes.request(selected())).unwrap();
                assert_ne!(explicit, late);
                wait_for_request(world, explicit, ProjectSceneRequestState::Ready);
                ProjectScenes::scope(world, |scenes| {
                    assert!(!scenes.acknowledge(late));
                    assert!(scenes.cancel(explicit));
                    assert!(scenes.acknowledge(explicit));
                });
                assert_eq!(io_admitted(world), 3);
                assert_active_budget(&context, baseline);
            } else {
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: Some(late),
                        pending: false,
                        error: None
                    }
                );
                ProjectScenes::scope(world, |scenes| {
                    assert!(
                        !scenes
                            .notify_source_change(&modified("prefabs/late.prefab.json"))
                            .unwrap()
                    );
                    assert!(
                        scenes
                            .notify_source_change(&modified("prefabs/kept.prefab.json"))
                            .unwrap()
                    );
                    assert!(!scenes.acknowledge(first));
                    assert!(scenes.acknowledge(late));
                });
                assert_eq!(
                    status(world),
                    ProjectSceneSourceUpdateStatus {
                        request: None,
                        pending: true,
                        error: None
                    }
                );
            }
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            assert_eq!(
                CurrentScene::get(world).unwrap().source().roots(),
                [entity("startup")]
            );
        });
    }
}

#[test]
fn explicit_travel_replaces_selected_source_interest_only_at_actual_publication() {
    for managed in [false, true] {
        let fixture = Fixture::new();
        write_prefab(&fixture.root, "old.prefab.json", "old-child");
        write_prefab(&fixture.root, "new.prefab.json", "new-child");
        write_scene_with_prefab(
            &fixture.root,
            "startup.scene.json",
            "old-scene",
            "old.prefab.json",
        );
        write_scene_with_prefab(
            &fixture.root,
            "second.scene.json",
            "new-scene",
            "new.prefab.json",
        );
        let root = fixture.root.clone();
        fixture.with_world(move |world| {
            world.insert_resource(pools());
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let original_source = CurrentScene::get(world).unwrap().source_view();
            write_scene_with_prefab(
                &root,
                "startup.scene.json",
                "failed-update",
                "retry-only.prefab.json",
            );
            ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
            poll_project_scene_requests(world);
            let failed = status(world).request.unwrap();
            let error = wait_rejected(world, failed);
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(failed)));
            let request = ProjectScenes::scope(world, |scenes| scenes.request(selected())).unwrap();
            wait_for_request(world, request, ProjectSceneRequestState::Ready);
            let captured = ProjectScenes::scope(world, |scenes| {
                assert!(
                    !scenes
                        .notify_source_change(&modified("prefabs/new.prefab.json"))
                        .unwrap()
                );
                assert!(
                    scenes
                        .notify_source_change(&modified("prefabs/old.prefab.json"))
                        .unwrap()
                );
                scenes.request_source_rescan().unwrap();
                let prepared = scenes.prepared(request).unwrap();
                assert_eq!(prepared.selected_scene_path(), Some(&selected()));
                prepared.source_content_stamp().unwrap()
            });
            assert_eq!(
                status(world),
                ProjectSceneSourceUpdateStatus {
                    request: None,
                    pending: true,
                    error: Some(error)
                }
            );
            if managed {
                begin_prepared_project_scene_travel(world, request, limits(), &[], |_| {}).unwrap();
                poll_project_scene_requests(world);
                assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
                assert!(status(world).pending);
                ProjectScenes::scope(world, |scenes| {
                    assert!(
                        !scenes
                            .notify_source_change(&modified("prefabs/new.prefab.json"))
                            .unwrap()
                    );
                });
                world.resource_mut::<nara_app::RealTime>().frame += 1;
                poll_project_scene_requests(world);
            } else {
                commit(world, request);
            }
            assert_ne!(CurrentScene::get(world).unwrap().receipt(), &expected);
            assert_eq!(
                CurrentScene::get(world).unwrap().source_content_stamp(),
                Some(captured)
            );
            assert_eq!(
                CurrentScene::get(world).unwrap().source().roots(),
                [entity("new-scene")]
            );
            assert_eq!(original_source.with_document(|_| ()), None);
            assert_eq!(
                status(world),
                ProjectSceneSourceUpdateStatus {
                    request: None,
                    pending: true,
                    error: None
                }
            );
            poll_project_scene_requests(world);
            assert_eq!(io_admitted(world), 2);
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Committed)
                );
                assert!(
                    !scenes
                        .notify_source_change(&modified("prefabs/old.prefab.json"))
                        .unwrap()
                );
                assert!(
                    !scenes
                        .notify_source_change(&modified("prefabs/retry-only.prefab.json"))
                        .unwrap()
                );
                assert!(
                    !scenes
                        .notify_source_change(&modified("scenes/startup.scene.json"))
                        .unwrap()
                );
                assert!(scenes.acknowledge(request));
            });
            assert_eq!(
                status(world),
                ProjectSceneSourceUpdateStatus {
                    request: None,
                    pending: true,
                    error: None
                }
            );
            poll_project_scene_requests(world);
            let automatic = status(world).request.unwrap();
            assert_ne!(automatic, request);
            wait_for_request(world, automatic, ProjectSceneRequestState::Ready);
            ProjectScenes::scope(world, |scenes| {
                let prepared = scenes.prepared(automatic).unwrap();
                assert_eq!(
                    prepared.selected_scene(),
                    &ProjectSceneSelection::Source(selected())
                );
                assert_eq!(prepared.selected_scene_path(), Some(&selected()));
                assert_eq!(prepared.scene().roots(), [entity("new-scene")]);
                assert!(!scenes.acknowledge(request));
                assert!(scenes.cancel(automatic));
                assert!(scenes.acknowledge(automatic));
            });
            assert_eq!(io_admitted(world), 3);
            assert!(!status(world).pending);
            ProjectScenes::scope(world, |scenes| {
                assert!(
                    scenes
                        .notify_source_change(&modified("prefabs/new.prefab.json"))
                        .unwrap()
                );
            });
            assert!(status(world).pending);
        });
    }
}

#[test]
fn closing_fences_source_interest_and_late_preparation_without_releasing_capture_early() {
    for phase in [TestPhase::BeforeRead, TestPhase::AfterPreparation] {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        fixture.with_world(move |world| {
            world.insert_resource(pools());
            let context = Arc::clone(&world.resource::<RequestOwner>().context);
            let Context::Source { loader, .. } = context.as_ref() else {
                unreachable!()
            };
            let baseline = loader.budget_snapshot();
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let stamp = CurrentScene::get(world).unwrap().source_content_stamp();
            let source = CurrentScene::get(world).unwrap().source_view();
            write_scene(&root, "startup.scene.json", "must-not-publish");
            let (gate, control) = TestGate::new(phase);
            world.resource_mut::<RequestOwner>().gate = Some(gate);
            ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
            poll_project_scene_requests(world);
            let request = status(world).request.unwrap();
            assert!(control.wait_until_started());
            ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
            world.resource::<Closing>().0.store(true, Ordering::Release);
            for _ in 0..3 {
                poll_project_scene_requests(world);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.source_update_status(),
                        Some(ProjectSceneSourceUpdateStatus {
                            request: Some(request),
                            pending: true,
                            error: None,
                        })
                    );
                    assert_eq!(
                        scenes.notify_source_change(&modified("scenes/startup.scene.json")),
                        Err(ProjectSceneRequestError::Unavailable)
                    );
                    assert_eq!(
                        scenes.request_source_rescan(),
                        Err(ProjectSceneRequestError::Unavailable)
                    );
                    assert_eq!(
                        scenes.request(selected()),
                        Err(ProjectSceneRequestError::Unavailable)
                    );
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Cancelling)
                    );
                    assert!(scenes.prepared(request).is_none());
                    assert!(!scenes.acknowledge(request));
                });
                assert_eq!(io_admitted(world), 1);
                assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
                assert_eq!(
                    CurrentScene::get(world).unwrap().source_content_stamp(),
                    stamp
                );
                if phase == TestPhase::AfterPreparation {
                    assert_eq!(
                        loader.budget_snapshot().active_generations(),
                        baseline.active_generations() + 1
                    );
                }
            }
            let report = commit_prepared_project_scene(world, request, limits(), &[], |_| {
                panic!("closing forbids publication")
            });
            assert!(report.diagnostics.has_errors());
            assert!(report.instance.is_none());
            control.release();
            wait_for_request(
                world,
                request,
                ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Stale),
            );
            assert_active_budget(&context, baseline);
            assert_eq!(
                source.with_document(|document| document.roots().to_vec()),
                Some(vec![entity("startup")])
            );
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
            for _ in 0..3 {
                poll_project_scene_requests(world);
                assert_eq!(io_admitted(world), 1);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(scenes.state(request), None);
                    assert_eq!(
                        scenes.source_update_status(),
                        Some(ProjectSceneSourceUpdateStatus {
                            request: None,
                            pending: true,
                            error: None,
                        })
                    );
                });
            }
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            assert_active_budget(&context, baseline);
        });
    }
}

fn modified(path: &str) -> AssetSourceChange {
    AssetSourceChange::new(
        AssetSourcePath::new(path).unwrap(),
        AssetSourceChangeKind::Modified,
    )
}

fn status(world: &mut World) -> ProjectSceneSourceUpdateStatus {
    ProjectScenes::scope(world, |scenes| scenes.source_update_status().unwrap())
}

fn io_admitted(world: &World) -> u64 {
    world
        .resource::<TaskPools>()
        .stats()
        .for_kind(nara_tasks::TaskPoolKind::Io)
        .admitted
}

fn startup() -> ProjectPath {
    ProjectPath::new("startup.scene.json").unwrap()
}

fn entity(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn write_scene(root: &Path, path: &str, name: &str) {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity(name))]);
    fs::write(
        root.join("scenes").join(path),
        document.to_json_string().unwrap(),
    )
    .unwrap();
}

fn write_scene_with_prefab(root: &Path, path: &str, name: &str, prefab: &str) {
    let mut anchor = SceneEntityRecord::new(entity(name));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path(prefab).unwrap(),
        binding: PrefabInstanceBinding::Forest,
        arguments: None,
        overrides: None,
    });
    let document = SceneDocument::from_ordered_roots([anchor]);
    fs::write(
        root.join("scenes").join(path),
        document.to_json_string().unwrap(),
    )
    .unwrap();
}

fn write_prefab(root: &Path, path: &str, name: &str) {
    let document = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(entity(name))]);
    fs::write(
        root.join("prefabs").join(path),
        document.to_json_string().unwrap(),
    )
    .unwrap();
}

fn limits() -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(ItemLimit::new(4).unwrap(), ItemLimit::new(4).unwrap())
}

fn commit(world: &mut World, request: ProjectSceneRequestId) {
    let report = commit_prepared_project_scene(world, request, limits(), &[], |_| {});
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let committed = report.instance.unwrap();
    assert_eq!(CurrentScene::get(world).unwrap().receipt(), &committed);
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Committed)
        );
    });
}

fn wait_rejected(world: &mut World, request: ProjectSceneRequestId) -> ProjectSceneRequestError {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        poll_project_scene_requests(world);
        match ProjectScenes::scope(world, |scenes| scenes.state(request)) {
            Some(ProjectSceneRequestState::Rejected(error)) => return error,
            Some(ProjectSceneRequestState::Preparing) => {}
            state => panic!("source preparation must reject instead of publishing {state:?}"),
        }
        assert!(
            Instant::now() < deadline,
            "source update preparation did not reject"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
