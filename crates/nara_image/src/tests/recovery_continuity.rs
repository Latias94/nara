use super::*;

fn removed_fixture() -> (
    PathBuf,
    AssetRecord,
    Handle<ImageAsset>,
    nara_app::RuntimeInstance,
) {
    removed_fixture_with_invalidation(Arc::new(std::sync::atomic::AtomicBool::new(false)))
}

fn removed_fixture_with_invalidation(
    invalidate: Arc<std::sync::atomic::AtomicBool>,
) -> (
    PathBuf,
    AssetRecord,
    Handle<ImageAsset>,
    nara_app::RuntimeInstance,
) {
    let root = unique_temp_root();
    fs::create_dir_all(root.join("textures")).unwrap();
    let record = image_record("textures/player.png");
    let mut app = app_with_image_plugin(&root, record.clone());
    let importer = app.world().resource::<ImageImporter>().clone();
    let (server, handle, images, states, events) =
        loaded_image_store(&importer, &record, &[255, 0, 0, 255]);
    app.add_systems(
        CoreStage::TaskUpdate,
        (move |mut states: ResMut<AssetStates>| {
            if invalidate.swap(false, std::sync::atomic::Ordering::SeqCst) {
                states.invalidate_publication(handle.id());
            }
        })
        .after(AssetTaskUpdateSet::SpawnJobs)
        .before(ImageReloadInternalSet::ApplyResults),
    )
    .unwrap();
    let world = app.world_mut().unwrap();
    world.insert_resource(server);
    world.insert_resource(images);
    world.insert_resource(states);
    world.insert_resource(events);
    world
        .resource_mut::<AssetSourceChanges>()
        .removed(record.path().clone())
        .unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    drive_image_jobs(&mut runtime);
    (root, record, handle, runtime)
}

#[test]
fn post_spawn_invalidation_rejects_old_remove_success_and_failure() {
    for outcome in ["remove", "success", "failure"] {
        let invalidate = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (root, record, handle, mut runtime) =
            removed_fixture_with_invalidation(Arc::clone(&invalidate));
        write_recreated_source(&root, &record);
        if outcome == "failure" {
            fs::write(root.join(record.path().as_str()), b"invalid PNG").unwrap();
        }
        edit_image_pipeline(
            &mut runtime,
            ImagePipelineEdit::SourceChange {
                path: record.path().clone(),
                kind: if outcome == "remove" {
                    AssetSourceChangeKind::Removed
                } else {
                    AssetSourceChangeKind::Modified
                },
            },
        );
        if outcome != "remove" {
            runtime.drive(Duration::ZERO).unwrap();
            let _ = runtime
                .world()
                .resource::<TaskPools>()
                .run_pending_for_tests();
        }
        invalidate.store(true, std::sync::atomic::Ordering::SeqCst);
        runtime.drive(Duration::ZERO).unwrap();
        let eligible = crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
        let absent = runtime
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .is_none();
        let applied = runtime.world().resource::<ImageReloadStats>().applied;
        let stale = runtime.world().resource::<ImageReloadStats>().stale;
        runtime.finish().unwrap();
        remove_temp_root(&root);
        assert!(!eligible, "{outcome}");
        assert!(absent, "{outcome}");
        assert_eq!(applied, 0, "{outcome}");
        assert_eq!(stale, 1, "{outcome}");
    }
}

fn write_recreated_source(root: &Path, record: &AssetRecord) {
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            record.stable_id(),
            record.path().clone(),
            record.source_kind().clone(),
        ),
        ImageImportSettings::default(),
    );
    let path = root.join(record.path().as_str());
    fs::write(
        path.with_extension("png.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(path, rgba_png(1, 1, &[0, 255, 0, 255])).unwrap();
}

#[test]
fn external_writes_cannot_be_blessed_by_a_later_capacity_rejection() {
    for retain_recipe in [false, true] {
        for kind in [
            RecoveryInterference::Loading,
            RecoveryInterference::Failure,
            RecoveryInterference::Invalidate,
            RecoveryInterference::Metadata,
            RecoveryInterference::SlotAba,
            RecoveryInterference::ReplaceValues,
            RecoveryInterference::ReplaceStates,
        ] {
            let (root, record, handle, mut runtime) = removed_fixture();
            write_recreated_source(&root, &record);
            if retain_recipe {
                fs::write(root.join(record.path().as_str()), b"invalid PNG").unwrap();
                edit_image_pipeline(
                    &mut runtime,
                    ImagePipelineEdit::SourceChange {
                        path: record.path().clone(),
                        kind: AssetSourceChangeKind::Modified,
                    },
                );
                drive_image_jobs(&mut runtime);
            }
            let before = crate::reload::recovery_facts_for_tests(runtime.world(), handle);
            edit_image_pipeline(
                &mut runtime,
                ImagePipelineEdit::RecoveryInterference { handle, kind },
            );
            let after_write = crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
            edit_image_pipeline(
                &mut runtime,
                ImagePipelineEdit::ReloadRequests(nara_asset::AssetReloadRequests::with_limits(
                    nara_asset::AssetReloadRequestLimits::new(ItemLimit::ONE, ByteLimit::ONE),
                )),
            );
            edit_image_pipeline(
                &mut runtime,
                ImagePipelineEdit::SourceChange {
                    path: record.path().clone(),
                    kind: AssetSourceChangeKind::Modified,
                },
            );
            runtime.drive(Duration::ZERO).unwrap();
            let after_rejection =
                crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
            let absent = runtime
                .world()
                .resource::<Assets<ImageAsset>>()
                .get(handle)
                .is_none();
            runtime.finish().unwrap();
            remove_temp_root(&root);
            assert_eq!(before, (true, retain_recipe), "{kind:?}");
            assert!(!after_write, "{kind:?}");
            assert!(!after_rejection, "{kind:?}");
            assert!(absent, "{kind:?}");
        }
    }
}

#[test]
fn loading_recovery_superseded_by_rejected_successor_remains_retryable() {
    let (root, record, handle, mut runtime) = removed_fixture();
    write_recreated_source(&root, &record);
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::SourceChange {
            path: record.path().clone(),
            kind: AssetSourceChangeKind::Modified,
        },
    );
    runtime.drive(Duration::ZERO).unwrap();
    let loading = runtime
        .world()
        .resource::<AssetStates>()
        .state(handle.id())
        .unwrap()
        .load_state()
        .clone();
    let first_generation = runtime
        .world()
        .resource::<AssetLoadGenerations>()
        .current(handle.id());
    let loading_facts = crate::reload::recovery_facts_for_tests(runtime.world(), handle);
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::ReloadRequests(nara_asset::AssetReloadRequests::with_limits(
            nara_asset::AssetReloadRequestLimits::new(ItemLimit::ONE, ByteLimit::ONE),
        )),
    );
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::SourceChange {
            path: record.path().clone(),
            kind: AssetSourceChangeKind::Modified,
        },
    );
    runtime.drive(Duration::ZERO).unwrap();
    let failed = runtime
        .world()
        .resource::<AssetStates>()
        .state(handle.id())
        .unwrap()
        .load_state()
        .clone();
    let successor = runtime
        .world()
        .resource::<AssetLoadGenerations>()
        .current(handle.id());
    let rejected_facts = crate::reload::recovery_facts_for_tests(runtime.world(), handle);
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::ReloadRequests(nara_asset::AssetReloadRequests::default()),
    );
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::SourceChange {
            path: record.path().clone(),
            kind: AssetSourceChangeKind::Modified,
        },
    );
    drive_image_jobs(&mut runtime);
    let pixels = runtime
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .map(|image| image.pixels().to_vec());
    let recovered = crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
    runtime.finish().unwrap();
    remove_temp_root(&root);
    assert_eq!(loading, LoadState::Loading);
    assert_eq!(loading_facts, (true, true));
    assert_ne!(first_generation, successor);
    assert!(
        matches!(failed, LoadState::Failed { message } if message == "asset.reload-request-byte-limit-exceeded")
    );
    assert_eq!(rejected_facts, (true, true));
    assert_eq!(pixels, Some(vec![0, 255, 0, 255]));
    assert!(!recovered);
}

#[test]
fn repeated_remove_requires_continuity_from_the_still_eligible_start() {
    for interfere in [false, true] {
        let (root, record, handle, mut runtime) = removed_fixture();
        let initial = crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
        if interfere {
            edit_image_pipeline(
                &mut runtime,
                ImagePipelineEdit::RecoveryInterference {
                    handle,
                    kind: RecoveryInterference::Loading,
                },
            );
        }
        edit_image_pipeline(
            &mut runtime,
            ImagePipelineEdit::SourceChange {
                path: record.path().clone(),
                kind: AssetSourceChangeKind::Removed,
            },
        );
        drive_image_jobs(&mut runtime);
        let eligible = crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
        let removed = runtime
            .world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state()
            .clone();
        runtime.finish().unwrap();
        remove_temp_root(&root);
        assert!(initial);
        assert_eq!(eligible, !interfere);
        assert_eq!(removed, LoadState::Removed);
    }
}

#[test]
fn capacity_continuation_cannot_admit_wrong_stable_path_or_kind_metadata() {
    for mismatch in ["stable", "path", "kind"] {
        let (root, record, handle, mut runtime) = removed_fixture();
        write_recreated_source(&root, &record);
        edit_image_pipeline(
            &mut runtime,
            ImagePipelineEdit::ReloadRequests(nara_asset::AssetReloadRequests::with_limits(
                nara_asset::AssetReloadRequestLimits::new(ItemLimit::ONE, ByteLimit::ONE),
            )),
        );
        edit_image_pipeline(
            &mut runtime,
            ImagePipelineEdit::SourceChange {
                path: record.path().clone(),
                kind: AssetSourceChangeKind::Modified,
            },
        );
        runtime.drive(Duration::ZERO).unwrap();
        let after_rejection = crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
        let identity = AssetMetaIdentity::new(
            if mismatch == "stable" {
                StableAssetId::parse_str("30000000-0000-4000-8000-000000000001").unwrap()
            } else {
                record.stable_id()
            },
            if mismatch == "path" {
                AssetPath::new("textures/other.png").unwrap()
            } else {
                record.path().clone()
            },
            if mismatch == "kind" {
                AssetSourceKind::Scene
            } else {
                AssetSourceKind::Image
            },
        );
        fs::write(
            root.join(record.path().as_str()).with_extension("png.meta"),
            AssetMeta::new(identity, ImageImportSettings::default())
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        edit_image_pipeline(
            &mut runtime,
            ImagePipelineEdit::ReloadRequests(nara_asset::AssetReloadRequests::default()),
        );
        edit_image_pipeline(
            &mut runtime,
            ImagePipelineEdit::SourceChange {
                path: record.path().clone(),
                kind: AssetSourceChangeKind::Modified,
            },
        );
        drive_image_jobs(&mut runtime);
        let eligible = crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
        let absent = runtime
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .is_none();
        let state = runtime
            .world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state()
            .clone();
        runtime.finish().unwrap();
        remove_temp_root(&root);
        assert!(after_rejection, "{mismatch}");
        assert!(!eligible, "{mismatch}");
        assert!(absent, "{mismatch}");
        assert!(
            matches!(state, LoadState::Failed { message } if message == "image.reload-metadata-identity-mismatch"),
            "{mismatch}"
        );
    }
}

#[test]
fn public_success_supersedes_removal_recovery_even_without_another_source_event() {
    let (root, record, handle, mut runtime) = removed_fixture();
    let before = crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::Reimport {
            record,
            handle,
            settings: ImageImportSettings::default(),
        },
    );
    let after = crate::reload::recovery_facts_for_tests(runtime.world(), handle).0;
    let pixels = runtime
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap()
        .pixels()
        .to_vec();
    runtime.finish().unwrap();
    remove_temp_root(&root);
    assert!(before);
    assert!(!after);
    assert_eq!(pixels, vec![0, 0, 255, 255]);
}
