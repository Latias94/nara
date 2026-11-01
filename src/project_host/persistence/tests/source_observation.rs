use super::*;

#[test]
fn authoring_hosts_transfer_their_current_observation_and_keep_old_captures_independent() {
    let root = TestRoot::new();
    let directory = root.capability();
    let scene_path = "startup.scene.json";
    let prefab_path = "reference-ui.prefab.json";
    let scene_file = directory
        .open_file(&RelativePath::new(scene_path).unwrap())
        .unwrap();
    let prefab_file = directory
        .open_file(&RelativePath::new(prefab_path).unwrap())
        .unwrap();
    let scene_digest = scene_file.digest(65_536).unwrap();
    let prefab_digest = prefab_file.digest(65_536).unwrap();
    let (mut scene, _) = ScenePersistenceHost::bind_authoring_source(
        &directory,
        scene_path,
        scene_file,
        scene_digest,
    )
    .unwrap();
    let (mut prefab, _) = PrefabPersistenceHost::bind_authoring_source(
        &directory,
        EditorDocumentSourceBinding::new(prefab_path),
        prefab_file,
        prefab_digest,
    )
    .unwrap();
    let old_scene = scene.accepted_source_observation().unwrap();
    let old_prefab = prefab.accepted_source_observation().unwrap();
    assert!(scene.source_observation_cost().is_some());
    assert!(prefab.source_observation_cost().is_some());
    let scene_input = scene.capture_source_observation(old_scene).unwrap();
    let prefab_input = prefab.capture_source_observation(old_prefab).unwrap();
    // Reopen supplies actual newly read leaves to the updated owner; new captures must use
    // that owner, while escaped old captures continue to own the original observations.
    for (path, moved) in [
        (scene_path, "old-scene.json"),
        (prefab_path, "old-prefab.json"),
    ] {
        fs::rename(root.path.join(path), root.path.join(moved)).unwrap();
        fs::write(root.path.join(path), b"new read observation").unwrap();
    }
    let new_scene = directory
        .open_file(&RelativePath::new(scene_path).unwrap())
        .unwrap();
    let new_prefab = directory
        .open_file(&RelativePath::new(prefab_path).unwrap())
        .unwrap();
    let digest = ContentDigest::of_bytes(b"new read observation");
    scene.commit_reopen(new_scene, digest);
    prefab.commit_reopen(new_prefab, digest);
    assert!(matches!(
        scene.capture_source_observation(old_scene),
        Err(EditorPersistenceRejection::TargetChanged)
    ));
    assert!(matches!(
        prefab.capture_source_observation(old_prefab),
        Err(EditorPersistenceRejection::TargetChanged)
    ));
    let next_scene = scene
        .capture_source_observation(scene.accepted_source_observation().unwrap())
        .unwrap();
    let next_prefab = prefab
        .capture_source_observation(prefab.accepted_source_observation().unwrap())
        .unwrap();
    drop((scene, prefab));
    let old_scene_leaf = directory
        .open_file(&RelativePath::new("old-scene.json").unwrap())
        .unwrap();
    let old_prefab_leaf = directory
        .open_file(&RelativePath::new("old-prefab.json").unwrap())
        .unwrap();
    assert!(scene_input.matches(&old_scene_leaf).unwrap());
    assert!(prefab_input.matches(&old_prefab_leaf).unwrap());
    assert!(!next_scene.matches(&old_scene_leaf).unwrap());
    assert!(!next_prefab.matches(&old_prefab_leaf).unwrap());
    assert_eq!(
        next_scene.file().read_to_end_bounded(65_536).unwrap(),
        b"new read observation"
    );
    assert_eq!(
        next_prefab.file().read_to_end_bounded(65_536).unwrap(),
        b"new read observation"
    );
}

#[test]
fn missing_and_uncommitted_publications_have_no_accepted_observation() {
    let root = TestRoot::new();
    let directory = root.capability();
    let mut publication =
        FilePublicationHost::bind_missing(&directory, "new.scene.json", 65_536).unwrap();
    assert_eq!(publication.accepted_source_observation(), None);
    let bytes = b"source";
    let FilePublicationOutcome::Published(accepted) = publication.publish(bytes) else {
        panic!("a missing target must accept its first publication");
    };
    assert_eq!(publication.accepted_source_observation(), None);
    let (_, source) = accepted.into_parts();
    let identity = source.identity();
    publication.commit_published(source);
    let observation = Some((
        identity,
        Some(directory.identity()),
        ContentDigest::of_bytes(bytes),
    ));
    assert_eq!(publication.accepted_source_observation(), observation);
    publication.mark_uncertain();
    assert_eq!(publication.accepted_source_observation(), None);
    let file = publication.read_source().open().unwrap();
    publication.commit_reconciled(file, ContentDigest::of_bytes(bytes));
    assert_eq!(publication.accepted_source_observation(), observation);
}

#[test]
fn accepted_observation_uses_the_retained_parent_and_does_not_reopen_the_target() {
    let root = TestRoot::new();
    let directory = root.capability();
    let path = "nested/source.scene.json";
    fs::create_dir(root.path.join("nested")).unwrap();
    fs::write(root.path.join(path), b"accepted source").unwrap();
    let file = directory
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    let identity = file.identity();
    let parent = file.parent_identity();
    let digest = file.digest(65_536).unwrap();
    let publication =
        FilePublicationHost::bind_existing(&directory, path, &file, digest, 65_536).unwrap();
    assert_ne!(parent, Some(directory.identity()));
    drop(file);
    fs::remove_file(root.path.join(path)).unwrap();
    assert_eq!(
        publication.accepted_source_observation(),
        Some((identity, parent, digest)),
    );
}

#[test]
fn scene_and_prefab_hosts_expose_only_their_accepted_persistence_observation() {
    let root = TestRoot::new();
    let directory = root.capability();
    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let mut scene = ScenePersistenceHost::open_authoring(
        &directory,
        "startup.scene.json",
        AuthoringDocumentSource::new(
            nara_asset::AssetRef::path("scenes/startup.scene.json").unwrap(),
        ),
        &basis,
    )
    .unwrap()
    .host;
    let mut prefab = PrefabPersistenceHost::open_authoring(
        &directory,
        EditorDocumentSourceBinding::new("reference-ui.prefab.json"),
        AuthoringDocumentSource::new(
            nara_asset::AssetRef::path("prefabs/reference-ui.prefab.json").unwrap(),
        ),
        &basis,
    )
    .unwrap()
    .host;
    let scene_file = directory
        .open_file_exact(&RelativePath::new("startup.scene.json").unwrap())
        .unwrap();
    let prefab_file = directory
        .open_file_exact(&RelativePath::new("reference-ui.prefab.json").unwrap())
        .unwrap();
    assert_eq!(
        scene.accepted_source_observation(),
        Some((
            scene_file.identity(),
            scene_file.parent_identity(),
            scene_file.digest(65_536).unwrap(),
        )),
    );
    assert_eq!(
        prefab.accepted_source_observation(),
        Some((
            prefab_file.identity(),
            prefab_file.parent_identity(),
            prefab_file.digest(65_536).unwrap(),
        )),
    );
    scene.mark_uncertain();
    prefab.mark_uncertain();
    assert_eq!(scene.accepted_source_observation(), None);
    assert_eq!(prefab.accepted_source_observation(), None);
    let scene_identity = scene_file.identity();
    let scene_digest = scene_file.digest(65_536).unwrap();
    let prefab_identity = prefab_file.identity();
    let prefab_digest = prefab_file.digest(65_536).unwrap();
    scene.commit_reopen(scene_file, scene_digest);
    prefab.commit_reopen(prefab_file, prefab_digest);
    assert_eq!(
        scene.accepted_source_observation(),
        Some((scene_identity, Some(directory.identity()), scene_digest)),
    );
    assert_eq!(
        prefab.accepted_source_observation(),
        Some((prefab_identity, Some(directory.identity()), prefab_digest)),
    );
}
