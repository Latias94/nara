use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use image::ImageEncoder;
use nara_app::App;
#[cfg(feature = "serde")]
use nara_app::CoreStage;
use nara_app::tasks::{TaskPoolConfig, TaskPools};
use nara_asset::{
    AssetEvents, AssetId, AssetLoadGenerations, AssetPathScope, AssetRecord,
    AssetReloadDiagnostics, AssetServer, AssetSourceChangeKind, AssetSourceChanges,
    AssetSourceKind, AssetSourcePath, AssetStates, AssetVersion, Assets, Handle, LoadState,
    StableAssetId,
};
#[cfg(feature = "serde")]
use nara_asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetTaskUpdateSet};
use nara_core::vfs::capabilities::{
    CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode,
};
use nara_core::{ByteLimit, ItemLimit};
#[cfg(feature = "serde")]
use nara_ecs::{ResMut, schedule::IntoScheduleConfigs};

use crate::image::*;

enum ImagePipelineEdit {
    SourceChange {
        path: AssetSourcePath,
        kind: AssetSourceChangeKind,
    },
    #[cfg(feature = "serde")]
    SourceChangeBatch(Vec<nara_asset::AssetSourceChange>),
    Invalidate(AssetId),
    ReloadRequests(nara_asset::AssetReloadRequests),
    Insert {
        handle: Handle<ImageAsset>,
        image: ImageAsset,
    },
    ReplaceThenFail {
        handle: Handle<ImageAsset>,
        image: ImageAsset,
    },
    Reimport {
        record: AssetRecord,
        handle: Handle<ImageAsset>,
        settings: ImageImportSettings,
    },
    Remove(Handle<ImageAsset>),
}

impl ImagePipelineEdit {
    fn apply(self, world: &mut nara_ecs::World) -> Option<AssetVersion> {
        match self {
            ImagePipelineEdit::SourceChange { path, kind } => {
                let mut changes = world.resource_mut::<AssetSourceChanges>();
                match kind {
                    AssetSourceChangeKind::Modified => changes.modified(path).unwrap(),
                    AssetSourceChangeKind::MetaModified => changes.meta_modified(path).unwrap(),
                    AssetSourceChangeKind::Removed => changes.removed(path).unwrap(),
                };
            }
            #[cfg(feature = "serde")]
            ImagePipelineEdit::SourceChangeBatch(changes) => {
                world
                    .resource_mut::<AssetSourceChanges>()
                    .try_extend(changes)
                    .unwrap();
            }
            ImagePipelineEdit::Invalidate(id) => {
                world
                    .resource_mut::<AssetStates>()
                    .invalidate_publication(id);
            }
            ImagePipelineEdit::ReloadRequests(requests) => {
                world.insert_resource(requests);
            }
            ImagePipelineEdit::Insert { handle, image } => {
                world
                    .resource_mut::<Assets<ImageAsset>>()
                    .insert(handle, image);
            }
            ImagePipelineEdit::ReplaceThenFail { handle, image } => {
                let version =
                    world.resource_scope(|world, mut images: nara_ecs::Mut<Assets<ImageAsset>>| {
                        let mut states = world.resource_mut::<AssetStates>();
                        let expected = states.version(handle.id()).unwrap();
                        let version = images
                            .commit_reload(
                                handle,
                                expected,
                                image,
                                &mut states,
                                &mut AssetEvents::default(),
                                None,
                                None,
                            )
                            .unwrap();
                        images
                            .record_reload_failure(
                                handle,
                                &mut states,
                                &mut AssetEvents::default(),
                                "decode failed",
                            )
                            .unwrap();
                        version
                    });
                return Some(version);
            }
            ImagePipelineEdit::Reimport {
                record,
                handle,
                settings,
            } => {
                world.resource_scope(|world, mut images: nara_ecs::Mut<Assets<ImageAsset>>| {
                    world.resource_scope(|world, mut states: nara_ecs::Mut<AssetStates>| {
                        let server = world.resource::<AssetServer>();
                        let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
                        let imported = ImageImporter::default()
                            .import_image(
                                bytes_request_with_settings(&record, &bytes, settings),
                                handle,
                                states.version(handle.id()).unwrap(),
                                server,
                                &images,
                                &states,
                            )
                            .unwrap();
                        imported
                            .commit(
                                server,
                                &mut images,
                                &mut states,
                                &mut AssetEvents::default(),
                            )
                            .unwrap();
                    });
                });
            }
            ImagePipelineEdit::Remove(handle) => {
                world.resource_scope(|world, mut images: nara_ecs::Mut<Assets<ImageAsset>>| {
                    images
                        .remove_with_state(
                            handle,
                            &mut world.resource_mut::<AssetStates>(),
                            &mut AssetEvents::default(),
                        )
                        .unwrap();
                });
            }
        }
        None
    }
}

#[test]
fn prepare_system_writes_backend_neutral_image_resource() {
    let (app, handle) = app_with_loaded_image(ImageImporter::default());

    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let prepared = runtime.world().resource::<PreparedImages>();
    let resource = prepared.get(image_resource_key(handle)).unwrap().resource();
    assert_eq!(resource.extent(), ImageExtent::new(1, 1));
    assert_eq!(resource.pixel_len(), 4);
    assert_eq!(runtime.world().resource::<ImagePrepareStats>().prepared, 1);
    runtime.finish().unwrap();
}

#[test]
fn retained_publication_prepares_during_loading_and_after_reload_failure() {
    for fail_attempt in [false, true] {
        let (mut app, handle) = app_with_loaded_image(ImageImporter::default());
        let world = app.world_mut().unwrap();
        let mut images = world.remove_resource::<Assets<ImageAsset>>().unwrap();
        let mut states = world.remove_resource::<AssetStates>().unwrap();
        let version = states.set_loading(handle.id());
        if fail_attempt {
            images
                .record_reload_failure(
                    handle,
                    &mut states,
                    &mut AssetEvents::default(),
                    "decode failed",
                )
                .unwrap();
        }
        world.insert_resource(images);
        world.insert_resource(states);

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let prepared = runtime.world().resource::<PreparedImages>();
        let resource = prepared.get(image_resource_key(handle)).unwrap().resource();
        assert_eq!(resource.pixels(), &[0, 0, 255, 255]);
        assert_eq!(
            prepared
                .get(image_resource_key(handle))
                .unwrap()
                .identity()
                .asset_version(),
            version
        );
        assert_eq!(runtime.world().resource::<ImagePrepareStats>().prepared, 1);
        runtime.finish().unwrap();
    }
}

#[test]
fn revoked_publication_removes_prepared_resources_and_cannot_prepare_retained_pixels() {
    for prepare_before_invalidation in [false, true] {
        let (app, handle) = app_with_loaded_image(ImageImporter::default());
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        if prepare_before_invalidation {
            runtime.drive(Duration::ZERO).unwrap();
        }
        edit_image_pipeline(&mut runtime, ImagePipelineEdit::Invalidate(handle.id()));

        runtime.drive(Duration::ZERO).unwrap();

        assert!(
            runtime
                .world()
                .resource::<Assets<ImageAsset>>()
                .get(handle)
                .is_some()
        );
        assert!(
            runtime
                .world()
                .resource::<PreparedImages>()
                .get(image_resource_key(handle))
                .is_none()
        );
        assert_eq!(
            runtime
                .world()
                .resource::<ImagePrepareStats>()
                .skipped_unavailable,
            1
        );
        assert_eq!(
            runtime.world().resource::<ImagePrepareStats>().removed,
            u32::from(prepare_before_invalidation)
        );
        runtime.finish().unwrap();
    }
}

#[test]
fn rejected_removal_revokes_prepared_image_and_retires_obsolete_completion() {
    let temp_root = unique_temp_root();
    let record = image_record("textures/player.png");
    let texture_path = temp_root.join(record.path().as_str());
    fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
    fs::write(&texture_path, rgba_png(1, 1, &[0, 255, 0, 255])).unwrap();
    let mut app = app_with_image_plugin(&temp_root, record.clone());
    let importer = app.world().resource::<ImageImporter>().clone();
    let (server, handle, images, states, events) =
        loaded_image_store(&importer, &record, &[255, 0, 0, 255]);
    let last_good_hash = images.get(handle).unwrap().source().unwrap().source_hash();
    app.insert_resource(server).unwrap();
    app.insert_resource(images).unwrap();
    app.insert_resource(states).unwrap();
    app.insert_resource(events).unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone())
        .unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<PreparedImages>()
            .get(image_resource_key(handle))
            .is_some()
    );
    assert_eq!(runtime.world().resource::<ImageReloadStats>().pending, 1);

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
            kind: AssetSourceChangeKind::Removed,
        },
    );
    runtime.drive(Duration::ZERO).unwrap();

    assert!(matches!(
        runtime.world().resource::<AssetStates>().state(handle.id()).unwrap().load_state(),
        LoadState::Failed { message } if message == "asset.reload-request-byte-limit-exceeded"
    ));
    assert_eq!(
        runtime
            .world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .usable_version(),
        None
    );
    assert!(
        runtime
            .world()
            .resource::<PreparedImages>()
            .get(image_resource_key(handle))
            .is_none()
    );

    assert_eq!(
        runtime
            .world()
            .resource::<TaskPools>()
            .run_pending_for_tests()
            .executed,
        1
    );
    runtime.drive(Duration::ZERO).unwrap();

    let stats = runtime.world().resource::<ImageReloadStats>();
    assert_eq!(stats.applied, 0);
    assert_eq!(stats.stale, 1);
    assert_eq!(stats.pending, 0);
    assert_eq!(
        runtime
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap()
            .source()
            .unwrap()
            .source_hash(),
        last_good_hash
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .usable_version(),
        None
    );
    assert!(
        runtime
            .world()
            .resource::<PreparedImages>()
            .get(image_resource_key(handle))
            .is_none()
    );

    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::Reimport {
            record,
            handle,
            settings: ImageImportSettings::default(),
        },
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .usable_version(),
        Some(AssetVersion::from_raw(2))
    );
    assert_eq!(
        runtime
            .world()
            .resource::<PreparedImages>()
            .get(image_resource_key(handle))
            .unwrap()
            .resource()
            .pixels(),
        &[0, 0, 255, 255]
    );
    runtime.finish().unwrap();
    remove_temp_root(&temp_root);
}

#[test]
fn accepted_replacement_prepares_when_its_next_attempt_already_failed() {
    let (app, handle) = app_with_loaded_image(ImageImporter::default());
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let source = runtime
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap()
        .source()
        .unwrap()
        .clone();
    let replacement = ImageAsset::from_import(
        source,
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        vec![255, 0, 0, 255],
    )
    .unwrap();
    let version = edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::ReplaceThenFail {
            handle,
            image: replacement,
        },
    )
    .unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    let prepared = runtime.world().resource::<PreparedImages>();
    assert_eq!(
        prepared
            .get(image_resource_key(handle))
            .unwrap()
            .resource()
            .pixels(),
        &[255, 0, 0, 255]
    );
    assert_eq!(
        prepared
            .get(image_resource_key(handle))
            .unwrap()
            .identity()
            .asset_version(),
        version
    );
    runtime.finish().unwrap();
}

#[test]
fn unchanged_image_snapshot_reuses_the_prepared_resource() {
    let (app, handle) = app_with_loaded_image(ImageImporter::default());
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let first = runtime
        .world()
        .resource::<PreparedImages>()
        .get(image_resource_key(handle))
        .unwrap()
        .identity()
        .clone();

    runtime.drive(std::time::Duration::ZERO).unwrap();

    let second = runtime
        .world()
        .resource::<PreparedImages>()
        .get(image_resource_key(handle))
        .unwrap()
        .identity();
    assert_eq!(&first, second);
    assert_eq!(runtime.world().resource::<ImagePrepareStats>().prepared, 0);
    runtime.finish().unwrap();
}

#[test]
fn direct_image_replacement_reprepares_when_asset_version_and_content_are_unchanged() {
    let (app, handle) = app_with_loaded_image(ImageImporter::default());
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let first = runtime
        .world()
        .resource::<PreparedImages>()
        .get(image_resource_key(handle))
        .unwrap()
        .identity()
        .clone();
    let version = first.asset_version();
    let content = first.content_hash();
    let source = runtime
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap()
        .source()
        .unwrap()
        .clone();
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::Insert {
            handle,
            image: ImageAsset::from_import(
                source,
                ImageExtent::new(1, 1),
                ImageFormat::Rgba8,
                vec![0, 0, 255, 255],
            )
            .unwrap(),
        },
    );

    runtime.drive(Duration::ZERO).unwrap();

    let prepared = runtime.world().resource::<PreparedImages>();
    let second = prepared.get(image_resource_key(handle)).unwrap().identity();
    assert_eq!(second.asset_version(), version);
    assert_eq!(second.content_hash(), content);
    assert_ne!(second.slot_revision(), first.slot_revision());
    assert_eq!(runtime.world().resource::<ImagePrepareStats>().prepared, 1);
    assert_eq!(prepared.len(), 1);
    runtime.finish().unwrap();
}

#[test]
fn prepare_system_invalidates_when_image_descriptor_changes() {
    let (app, handle) = app_with_loaded_image(ImageImporter::default());
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let old_snapshot = runtime
        .world()
        .resource::<PreparedImages>()
        .get(image_resource_key(handle))
        .unwrap()
        .identity()
        .clone();

    let expected_version = runtime
        .world()
        .resource::<AssetStates>()
        .version(handle.id())
        .unwrap();
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::Reimport {
            record: image_record("textures/player.png"),
            handle,
            settings: ImageImportSettings::new(ImageColorSpace::Linear),
        },
    );

    runtime.drive(Duration::ZERO).unwrap();

    let prepared = runtime.world().resource::<PreparedImages>();
    let new_snapshot = prepared
        .get(image_resource_key(handle))
        .unwrap()
        .identity()
        .clone();
    assert_ne!(old_snapshot, new_snapshot);
    assert_eq!(
        new_snapshot.asset_version().raw(),
        expected_version.raw() + 1
    );
    assert_eq!(runtime.world().resource::<ImagePrepareStats>().prepared, 1);
    assert_eq!(prepared.len(), 1);
    runtime.finish().unwrap();
}

#[test]
fn prepare_system_removes_prepared_resources_for_removed_images() {
    let (app, handle) = app_with_loaded_image(ImageImporter::default());
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<PreparedImages>()
            .get(image_resource_key(handle))
            .is_some()
    );

    edit_image_pipeline(&mut runtime, ImagePipelineEdit::Remove(handle));

    runtime.drive(Duration::ZERO).unwrap();

    assert!(
        runtime
            .world()
            .resource::<PreparedImages>()
            .get(image_resource_key(handle))
            .is_none()
    );
    assert_eq!(runtime.world().resource::<ImagePrepareStats>().removed, 1);
    assert!(runtime.world().resource::<PreparedImages>().is_empty());
    runtime.finish().unwrap();
}

#[test]
fn prepared_image_tracks_source_reloads_with_accepted_settings() {
    let (root, record, app, handle) = source_image_app(ImageColorSpace::Linear);
    let texture = root.join(record.path().as_str());
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();

    for pixels in [[255, 0, 0, 255], [0, 255, 0, 255]] {
        fs::write(&texture, rgba_png(1, 1, &pixels)).unwrap();
        change_source(&mut runtime, &record, AssetSourceChangeKind::Modified);
        drive_image_jobs(&mut runtime);
        let prepared = prepared_image(&runtime, handle).unwrap();
        assert_eq!(prepared.pixels(), pixels);
        assert_eq!(prepared.color_space(), ImageColorSpace::Linear);
    }
    assert_eq!(runtime.world().resource::<ImagePrepareStats>().prepared, 1);
    assert_eq!(runtime.world().resource::<PreparedImages>().len(), 1);
    runtime.finish().unwrap();
    remove_temp_root(&root);
}

#[cfg(feature = "serde")]
#[test]
fn coalesced_source_and_metadata_removal_clears_the_prepared_image() {
    let (root, record, app, handle) = source_image_app(ImageColorSpace::Srgb);
    let texture = root.join(record.path().as_str());
    let metadata = image_metadata(&record, ImageColorSpace::Linear);
    let sidecar = texture.with_extension("png.meta");
    fs::write(&sidecar, metadata.to_json_string().unwrap()).unwrap();
    fs::write(&texture, rgba_png(1, 1, &[0, 255, 0, 255])).unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    for kind in [
        AssetSourceChangeKind::Removed,
        AssetSourceChangeKind::MetaModified,
        AssetSourceChangeKind::Modified,
    ] {
        change_source(&mut runtime, &record, kind);
    }
    drive_image_jobs(&mut runtime);
    assert_eq!(
        prepared_image(&runtime, handle).unwrap().pixels(),
        &[0, 255, 0, 255]
    );

    fs::remove_file(&texture).unwrap();
    fs::write(&sidecar, metadata.to_json_string().unwrap()).unwrap();
    change_source(&mut runtime, &record, AssetSourceChangeKind::Removed);
    change_source(&mut runtime, &record, AssetSourceChangeKind::MetaModified);
    drive_image_jobs(&mut runtime);
    assert!(prepared_image(&runtime, handle).is_none());
    runtime.finish().unwrap();
    remove_temp_root(&root);
}

#[cfg(feature = "serde")]
#[test]
fn prepared_image_returns_after_recovery_from_same_identity_metadata() {
    let (root, record, app, handle) = source_image_app(ImageColorSpace::Srgb);
    let texture = root.join(record.path().as_str());
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(prepared_image(&runtime, handle).is_some());
    change_source(&mut runtime, &record, AssetSourceChangeKind::Removed);
    drive_image_jobs(&mut runtime);
    assert!(prepared_image(&runtime, handle).is_none());

    let metadata = image_metadata(&record, ImageColorSpace::Linear);
    fs::write(
        texture.with_extension("png.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(&texture, b"not a PNG").unwrap();
    change_source(&mut runtime, &record, AssetSourceChangeKind::Modified);
    drive_image_jobs(&mut runtime);
    assert!(prepared_image(&runtime, handle).is_none());

    fs::write(&texture, rgba_png(1, 1, &[0, 255, 0, 255])).unwrap();
    change_source(&mut runtime, &record, AssetSourceChangeKind::Modified);
    drive_image_jobs(&mut runtime);
    assert_eq!(
        prepared_image(&runtime, handle).unwrap().pixels(),
        &[0, 255, 0, 255]
    );
    runtime.finish().unwrap();
    remove_temp_root(&root);
}

#[cfg(feature = "serde")]
#[test]
fn prepared_image_stays_absent_when_state_interference_blocks_recovery() {
    let (root, record, mut app, handle) = source_image_app(ImageColorSpace::Srgb);
    let texture = root.join(record.path().as_str());
    let mut fail_removal = true;
    app.add_systems(
        CoreStage::TaskUpdate,
        (move |mut images: ResMut<Assets<ImageAsset>>,
               mut states: ResMut<AssetStates>,
               mut events: ResMut<AssetEvents>| {
            if std::mem::take(&mut fail_removal) {
                images
                    .record_reload_failure(
                        handle,
                        &mut states,
                        &mut events,
                        "image.intervening-removal-failure",
                    )
                    .unwrap();
            }
        })
        .after(AssetTaskUpdateSet::ResolveSourceChanges)
        .before(AssetTaskUpdateSet::SpawnJobs),
    )
    .unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    change_source(&mut runtime, &record, AssetSourceChangeKind::Removed);
    drive_image_jobs(&mut runtime);

    let metadata = image_metadata(&record, ImageColorSpace::Linear);
    fs::write(
        texture.with_extension("png.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(&texture, rgba_png(1, 1, &[0, 255, 0, 255])).unwrap();
    change_source(&mut runtime, &record, AssetSourceChangeKind::Modified);
    drive_image_jobs(&mut runtime);
    assert!(prepared_image(&runtime, handle).is_none());
    runtime.finish().unwrap();
    remove_temp_root(&root);
}

#[cfg(feature = "serde")]
#[test]
fn prepared_image_keeps_the_last_good_value_until_metadata_revokes_identity() {
    let (root, record, app, handle) = source_image_app(ImageColorSpace::Srgb);
    let texture = root.join(record.path().as_str());
    let sidecar = texture.with_extension("png.meta");
    fs::write(&texture, rgba_png(1, 1, &[255, 0, 0, 255])).unwrap();
    fs::write(&sidecar, b"{ invalid metadata").unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    change_source(&mut runtime, &record, AssetSourceChangeKind::MetaModified);
    drive_image_jobs(&mut runtime);
    assert_eq!(
        prepared_image(&runtime, handle).unwrap().pixels(),
        &[255, 0, 0, 255]
    );

    let metadata = image_metadata(&record, ImageColorSpace::Linear);
    fs::write(&sidecar, metadata.to_json_string().unwrap()).unwrap();
    change_source(&mut runtime, &record, AssetSourceChangeKind::MetaModified);
    drive_image_jobs(&mut runtime);
    assert_eq!(
        prepared_image(&runtime, handle).unwrap().color_space(),
        ImageColorSpace::Linear
    );

    let changed = AssetMeta::new(
        AssetMetaIdentity::new(
            record.stable_id(),
            AssetPath::new("textures/other.png").unwrap(),
            record.source_kind().clone(),
        ),
        ImageImportSettings::new(ImageColorSpace::Linear),
    );
    fs::remove_file(&sidecar).unwrap();
    fs::write(&sidecar, changed.to_json_string().unwrap()).unwrap();
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::SourceChangeBatch(vec![
            nara_asset::AssetSourceChange::metadata_removed(record.path().clone()),
            nara_asset::AssetSourceChange::new(
                record.path().clone(),
                AssetSourceChangeKind::MetaModified,
            ),
        ]),
    );
    drive_image_jobs(&mut runtime);
    assert!(prepared_image(&runtime, handle).is_none());

    fs::write(&texture, rgba_png(1, 1, &[0, 255, 0, 255])).unwrap();
    change_source(&mut runtime, &record, AssetSourceChangeKind::Modified);
    drive_image_jobs(&mut runtime);
    assert!(prepared_image(&runtime, handle).is_none());

    fs::write(&sidecar, metadata.to_json_string().unwrap()).unwrap();
    change_source(&mut runtime, &record, AssetSourceChangeKind::MetaModified);
    drive_image_jobs(&mut runtime);
    assert_eq!(
        prepared_image(&runtime, handle).unwrap().pixels(),
        &[0, 255, 0, 255]
    );
    runtime.finish().unwrap();
    remove_temp_root(&root);
}

#[test]
fn accepted_source_removal_clears_the_prepared_image() {
    let (root, record, app, handle) = source_image_app(ImageColorSpace::Srgb);
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(prepared_image(&runtime, handle).is_some());

    change_source(&mut runtime, &record, AssetSourceChangeKind::Removed);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(prepared_image(&runtime, handle).is_none());
    assert!(runtime.world().resource::<PreparedImages>().is_empty());
    runtime.finish().unwrap();
    remove_temp_root(&root);
}

#[test]
fn native_image_storage_prepares_procedural_pixels_without_source_resources_or_tasks() {
    let mut app = App::new();
    app.add_plugins((nara_asset::AssetPlugin, ImagePlugin, ImagePreparePlugin))
        .unwrap();
    let handle = nara_asset::allocate_anonymous_handle::<ImageAsset>().unwrap();
    let image = ImageAsset::new(
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![24, 120, 220, 255],
    )
    .unwrap();
    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    images
        .commit_loaded(handle, image, &mut states, &mut events, None, None)
        .unwrap();
    app.insert_resource(images)
        .unwrap()
        .insert_resource(states)
        .unwrap()
        .insert_resource(events)
        .unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let world = runtime.world();
    let prepared = world
        .resource::<PreparedImages>()
        .get(image_resource_key(handle))
        .unwrap();
    assert_eq!(prepared.resource().pixels(), &[24, 120, 220, 255]);
    assert!(!world.contains_resource::<nara_asset::ProjectAssetDatabase>());
    assert!(!world.contains_resource::<AssetSourceChanges>());
    assert!(!world.contains_resource::<nara_asset::AssetReloadRequests>());
    assert!(!world.contains_resource::<AssetReloadDiagnostics>());
    assert!(!world.contains_resource::<AssetLoadGenerations>());
    assert!(!world.contains_resource::<nara_asset::AssetDependencyGraph>());
    assert!(!world.contains_resource::<ImageImporter>());
    assert!(!world.contains_resource::<ImageReloadStats>());
    assert!(!world.contains_resource::<TaskPools>());
    runtime.finish().unwrap();
}

fn app_with_loaded_image(importer: ImageImporter) -> (App, Handle<ImageAsset>) {
    let record = image_record("textures/player.png");
    let (server, handle, images, states, _) =
        loaded_image_store(&importer, &record, &[0, 0, 255, 255]);
    let mut app = App::new();
    app.add_plugin(ImagePreparePlugin).unwrap();
    app.world_mut().unwrap().insert_resource(server);
    app.world_mut().unwrap().insert_resource(images);
    app.world_mut().unwrap().insert_resource(states);
    (app, handle)
}

fn edit_image_pipeline(
    runtime: &mut nara_app::RuntimeInstance,
    edit: ImagePipelineEdit,
) -> Option<AssetVersion> {
    let outcome = runtime
        .with_driver_scope(|scope| {
            assert!(matches!(
                scope.state(),
                nara_app::RuntimeState::Running | nara_app::RuntimeState::Paused
            ));
            scope.with_world_mut(|world| edit.apply(world))
        })
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output
}

fn image_record(path: &str) -> AssetRecord {
    AssetRecord::new(
        StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap(),
        AssetSourcePath::new(path).unwrap(),
        AssetSourceKind::Image,
    )
}

fn bytes_request_with_settings(
    record: &AssetRecord,
    source_bytes: &[u8],
    settings: ImageImportSettings,
) -> ImageBytesImportRequest {
    ImageBytesImportRequest::new(record.clone(), source_bytes.to_vec(), settings)
}

fn loaded_image_store(
    importer: &ImageImporter,
    record: &AssetRecord,
    pixels: &[u8],
) -> (
    AssetServer,
    Handle<ImageAsset>,
    Assets<ImageAsset>,
    AssetStates,
    AssetEvents,
) {
    loaded_image_store_with_settings(importer, record, pixels, ImageImportSettings::default())
}

fn loaded_image_store_with_settings(
    importer: &ImageImporter,
    record: &AssetRecord,
    pixels: &[u8],
    settings: ImageImportSettings,
) -> (
    AssetServer,
    Handle<ImageAsset>,
    Assets<ImageAsset>,
    AssetStates,
    AssetEvents,
) {
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(record).unwrap();
    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let bytes = rgba_png(1, 1, pixels);
    importer
        .import_image(
            bytes_request_with_settings(record, &bytes, settings),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap()
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap();
    events.drain();
    (server, handle, images, states, events)
}

fn rgba_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder
        .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
        .unwrap();
    bytes
}

fn source_image_app(
    color_space: ImageColorSpace,
) -> (PathBuf, AssetRecord, App, Handle<ImageAsset>) {
    let root = unique_temp_root();
    let record = image_record("textures/player.png");
    let texture = root.join(record.path().as_str());
    fs::create_dir_all(texture.parent().unwrap()).unwrap();
    let mut app = app_with_image_plugin(&root, record.clone());
    let importer = app.world().resource::<ImageImporter>().clone();
    let (server, handle, images, states, events) = loaded_image_store_with_settings(
        &importer,
        &record,
        &[255, 0, 0, 255],
        ImageImportSettings::new(color_space),
    );
    app.insert_resource(server).unwrap();
    app.insert_resource(images).unwrap();
    app.insert_resource(states).unwrap();
    app.insert_resource(events).unwrap();
    (root, record, app, handle)
}

fn change_source(
    runtime: &mut nara_app::RuntimeInstance,
    record: &AssetRecord,
    kind: AssetSourceChangeKind,
) {
    edit_image_pipeline(
        runtime,
        ImagePipelineEdit::SourceChange {
            path: record.path().clone(),
            kind,
        },
    );
}

fn prepared_image(
    runtime: &nara_app::RuntimeInstance,
    handle: Handle<ImageAsset>,
) -> Option<&PreparedImageResource> {
    runtime
        .world()
        .resource::<PreparedImages>()
        .get(image_resource_key(handle))
        .map(PreparedImageSnapshot::resource)
}

#[cfg(feature = "serde")]
fn image_metadata(
    record: &AssetRecord,
    color_space: ImageColorSpace,
) -> AssetMeta<ImageImportSettings> {
    AssetMeta::new(
        AssetMetaIdentity::new(
            record.stable_id(),
            AssetPathScope::ROOT.relativize(record.path()).unwrap(),
            record.source_kind().clone(),
        ),
        ImageImportSettings::new(color_space),
    )
}

fn app_with_image_plugin(asset_root: &Path, record: AssetRecord) -> App {
    let mut app = App::new();
    app.insert_resource(TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap())
        .unwrap();
    app.add_plugins((
        nara_app::tasks::TaskPlugin::default(),
        (nara_asset::AssetPlugin, nara_asset::AssetSourcePlugin),
        ImagePlugin,
        ImagePreparePlugin,
        ImageSourcePlugin::default().with_source_directory(image_source_directory(asset_root)),
    ))
    .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<nara_asset::ProjectAssetDatabase>()
        .insert(record)
        .unwrap();
    app
}

fn image_source_directory(path: &Path) -> ImageSourceDirectory {
    let directory = DirectoryCapability::from_host_handle(
        host_directory(path),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap();
    ImageSourceDirectory::new(directory, AssetPathScope::ROOT)
}

fn host_directory(path: &Path) -> fs::File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    {
        fs::File::open(path).unwrap()
    }
}

fn drive_image_jobs(runtime: &mut nara_app::RuntimeInstance) {
    runtime.drive(Duration::ZERO).unwrap();
    let _ = runtime
        .world()
        .resource::<TaskPools>()
        .run_pending_for_tests();
    runtime.drive(Duration::ZERO).unwrap();
}

fn unique_temp_root() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nara_image_test_{}_{}", std::process::id(), stamp))
}

fn remove_temp_root(path: &Path) {
    if let Err(error) = fs::remove_dir_all(path) {
        panic!(
            "failed to remove temp test directory {}: {error}",
            path.display()
        );
    }
}
