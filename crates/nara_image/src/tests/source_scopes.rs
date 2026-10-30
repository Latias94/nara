use super::*;

use nara_asset::{AssetMetaIdentityCandidate, ProjectAssetDatabase};

const LOCAL_SOURCE: &str = "textures/player.png";

fn scope(prefix: &str) -> AssetPathScope {
    AssetPathScope::under(AssetSourcePath::new(prefix).unwrap())
}

fn source_directory(root: &Path, scope: AssetPathScope) -> ImageSourceDirectory {
    let directory = DirectoryCapability::from_host_handle(
        host_directory(root),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap();
    ImageSourceDirectory::new(directory, scope)
}

fn write_source(
    root: &Path,
    scope: &AssetPathScope,
    stable_id: StableAssetId,
    pixels: &[u8],
) -> AssetRecord {
    let local = AssetPath::new(LOCAL_SOURCE).unwrap();
    let source = root.join(local.as_str());
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, rgba_png(1, 1, pixels)).unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(stable_id, local.clone(), AssetSourceKind::Image),
        ImageImportSettings::default(),
    );
    let sidecar = root.join(local.meta_path());
    fs::write(&sidecar, metadata.to_json_string().unwrap()).unwrap();
    let candidate = AssetMetaIdentityCandidate::decode_json_bytes_owned(
        fs::read(sidecar).unwrap().into_boxed_slice(),
    )
    .unwrap();
    assert_eq!(candidate.identity().path(), &local);
    let (identity, _) = candidate
        .admit::<ImageImportSettings>()
        .unwrap()
        .into_meta()
        .into_parts();
    identity.into_record(scope)
}

#[test]
fn nested_scopes_import_independent_same_name_files_and_retain_exact_pixel_custody() {
    let root = unique_temp_root();
    let sources = [
        (
            root.join("first-native-directory"),
            scope("content/first/images"),
            stable_id(),
            [255, 0, 0, 255],
        ),
        (
            root.join("second-native-directory"),
            scope("content/second/images"),
            StableAssetId::parse_str("30000000-0000-4000-8000-000000000001").unwrap(),
            [0, 255, 0, 255],
        ),
    ];
    let mut database = ProjectAssetDatabase::default();
    let mut server = AssetServer::new();
    let importer = ImageImporter::default();
    let mut handles = Vec::new();
    for (native_root, scope, stable_id, pixels) in sources {
        let record = write_source(&native_root, &scope, stable_id, &pixels);
        database.insert(record.clone()).unwrap();
        assert_eq!(
            database
                .resolver(&scope)
                .record_for_path(&AssetPath::new(LOCAL_SOURCE).unwrap()),
            Some(&record)
        );
        let directory = source_directory(&native_root, scope.clone());
        assert_eq!(directory.scope(), &scope);
        let file = directory.open(&record).unwrap();
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        handles.push(handle);
        assert_eq!(server.path(handle.id()), Some(record.path()));
        let mut images = Assets::default();
        let mut states = AssetStates::default();
        let admitted = importer
            .admit_file(
                ImageFileImportRequest::new(record.clone(), file, ImageImportSettings::default()),
                handle,
                AssetVersion::ZERO,
                &server,
                &images,
                &states,
            )
            .unwrap();
        // The admitted file retains read authority after the directory wrapper is dropped.
        drop(directory);
        let imported = admitted.import(TaskCancellationToken::new()).unwrap();
        assert_eq!(imported.image().pixels(), &pixels);
        assert_eq!(imported.source().path(), record.path());
        imported
            .commit_retained(
                &server,
                &mut images,
                &mut states,
                &mut AssetEvents::default(),
            )
            .unwrap();
        let retained = images.get(handle).unwrap().share_retained().unwrap();
        drop(images);
        assert_eq!(importer.budget_snapshot().active_rgba_bytes(), pixels.len());
        assert_eq!(retained.pixels(), &pixels);
        drop(retained);
        assert_budget_released(importer.budget_snapshot());
    }
    assert_eq!(database.len(), 2);
    assert_ne!(handles[0], handles[1]);
    remove_temp_root(&root);
}

#[test]
fn lookalike_scope_is_rejected_before_source_or_metadata_open() {
    let root = unique_temp_root();
    let admitted_root = root.join("admitted");
    let other_root = root.join("other");
    let admitted_scope = scope("catalog/art");
    let admitted = write_source(
        &admitted_root,
        &admitted_scope,
        stable_id(),
        &[255, 0, 0, 255],
    );
    let outside = write_source(
        &other_root,
        &scope("catalog/artwork"),
        StableAssetId::parse_str("30000000-0000-4000-8000-000000000001").unwrap(),
        &[0, 255, 0, 255],
    );
    // Both incorrect canonical-as-local reads would find real files under this capability.
    let decoy = admitted_root.join(outside.path().as_str());
    fs::create_dir_all(decoy.parent().unwrap()).unwrap();
    fs::write(&decoy, rgba_png(1, 1, &[0, 0, 255, 255])).unwrap();
    fs::write(
        admitted_root.join(outside.path().meta_path()),
        b"invalid metadata",
    )
    .unwrap();
    let directory = source_directory(&admitted_root, admitted_scope);
    assert_eq!(
        directory
            .open(&admitted)
            .unwrap()
            .read_to_end_bounded(4096)
            .unwrap(),
        fs::read(admitted_root.join(LOCAL_SOURCE)).unwrap()
    );
    let expected = ImageImportError::Source {
        stage: ImageImportStage::SourceOpen,
        kind: ImageSourceFailureKind::AuthorityRejected,
    };
    assert_eq!(directory.open(&outside).unwrap_err(), expected);
    assert_eq!(
        directory.read_metadata_bytes(&outside).unwrap_err(),
        expected
    );
    assert_eq!(
        crate::reload::admit_metadata_reload_settings(Some(&directory), &outside),
        Err(ImageReloadError::Import(expected))
    );
    drop(directory);
    remove_temp_root(&root);
}

#[test]
fn nested_scope_reloads_actual_local_metadata_and_recovers_after_source_removal() {
    let root = unique_temp_root();
    let bound_scope = scope("packages/selected/image-sources");
    let record = write_source(&root, &bound_scope, stable_id(), &[255, 0, 0, 255]);
    let local = bound_scope.relativize(record.path()).unwrap();
    let source = root.join(local.as_str());
    let sidecar = root.join(local.meta_path());
    let mut app = app_with_image_source_directory(
        source_directory(&root, bound_scope),
        [record.clone()],
        TaskPools::inline_for_tests(bounded_task_config(1, 4)).unwrap(),
        ImageSourcePlugin::default(),
    );
    let importer = app.world().resource::<ImageImporter>().clone();
    let handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&record)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .meta_modified(record.path().clone())
        .unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    drive_image_jobs(&mut runtime);
    assert_eq!(
        runtime
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap()
            .pixels(),
        &[255, 0, 0, 255]
    );
    let initial_version = runtime
        .world()
        .resource::<AssetStates>()
        .version(handle.id())
        .unwrap();
    let settings = ImageImportSettings::new(ImageColorSpace::Linear);
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(record.stable_id(), local, AssetSourceKind::Image),
        settings,
    );
    fs::write(&sidecar, metadata.to_json_string().unwrap()).unwrap();
    fs::write(&source, rgba_png(1, 1, &[0, 255, 0, 255])).unwrap();
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::SourceChangeBatch(vec![
            nara_asset::AssetSourceChange::new(
                record.path().clone(),
                AssetSourceChangeKind::MetaModified,
            ),
            nara_asset::AssetSourceChange::new(
                record.path().clone(),
                AssetSourceChangeKind::Modified,
            ),
        ]),
    );
    drive_image_jobs(&mut runtime);
    let image = runtime
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap();
    assert_eq!(image.pixels(), &[0, 255, 0, 255]);
    assert_eq!(image.source().unwrap().settings(), settings);
    assert_eq!(image.source().unwrap().path(), record.path());
    assert_eq!(
        runtime
            .world()
            .resource::<AssetStates>()
            .version(handle.id())
            .unwrap()
            .raw(),
        initial_version.raw() + 1
    );

    fs::remove_file(&source).unwrap();
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::SourceChange {
            path: record.path().clone(),
            kind: AssetSourceChangeKind::Removed,
        },
    );
    drive_image_jobs(&mut runtime);
    assert!(
        runtime
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .is_none()
    );
    fs::write(&source, rgba_png(1, 1, &[0, 0, 255, 255])).unwrap();
    fs::write(&sidecar, metadata.to_json_string().unwrap()).unwrap();
    edit_image_pipeline(
        &mut runtime,
        ImagePipelineEdit::SourceChange {
            path: record.path().clone(),
            kind: AssetSourceChangeKind::MetaModified,
        },
    );
    drive_image_jobs(&mut runtime);
    let image = runtime
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap();
    assert_eq!(image.pixels(), &[0, 0, 255, 255]);
    assert_eq!(image.source().unwrap().settings(), settings);
    assert_eq!(
        runtime.world().resource::<AssetServer>().path(handle.id()),
        Some(record.path())
    );
    assert_eq!(runtime.world().resource::<ImageReloadStats>().applied, 3);
    runtime.finish().unwrap();
    assert_budget_released(importer.budget_snapshot());
    remove_temp_root(&root);
}

#[test]
fn nested_scope_metadata_refuses_a_canonical_locator_used_as_a_local_alias() {
    let root = unique_temp_root();
    let bound_scope = scope("project/custom/image-sources");
    let record = write_source(&root, &bound_scope, stable_id(), &[255, 0, 0, 255]);
    let wrong_identity = AssetMetaIdentity::new(
        record.stable_id(),
        AssetPath::new(record.path().as_str()).unwrap(),
        AssetSourceKind::Image,
    );
    fs::write(
        root.join(format!("{LOCAL_SOURCE}.meta")),
        AssetMeta::new(wrong_identity, ImageImportSettings::default())
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let directory = source_directory(&root, bound_scope);
    assert_eq!(
        crate::reload::admit_metadata_reload_settings(Some(&directory), &record),
        Err(ImageReloadError::MetadataIdentityMismatch)
    );
    drop(directory);
    remove_temp_root(&root);
}
