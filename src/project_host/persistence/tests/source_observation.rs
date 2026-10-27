use super::*;

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
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let mut scene = ScenePersistenceHost::open(&directory, "startup.scene.json", &registry)
        .unwrap()
        .host;
    let mut prefab = PrefabPersistenceHost::open(
        &directory,
        EditorDocumentSourceBinding::new("reference-ui.prefab.json"),
        &registry,
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
