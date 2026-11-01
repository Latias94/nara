use super::*;

enum Host {
    Scene(ScenePersistenceHost),
    Prefab(PrefabPersistenceHost),
}

impl Host {
    fn new(root: &TestRoot, prefab: bool) -> (Self, &'static str, DirectoryCapability) {
        let path = if prefab {
            "reference-ui.prefab.json"
        } else {
            "startup.scene.json"
        };
        let directory = root.capability();
        let file = directory
            .open_file(&RelativePath::new(path).unwrap())
            .unwrap();
        let digest = file.digest(65_536).unwrap();
        let host = if prefab {
            Self::Prefab(
                PrefabPersistenceHost::bind_authoring_source(
                    &directory,
                    EditorDocumentSourceBinding::new(path),
                    file,
                    digest,
                )
                .unwrap()
                .0,
            )
        } else {
            Self::Scene(
                ScenePersistenceHost::bind_authoring_source(&directory, path, file, digest)
                    .unwrap()
                    .0,
            )
        };
        (host, path, directory)
    }

    fn file(&mut self) -> &mut PersistenceFileState {
        match self {
            Self::Scene(host) => &mut host.file,
            Self::Prefab(host) => &mut host.file,
        }
    }

    fn bind(
        &mut self,
        path: &str,
        expected: (FileIdentity, Option<FileIdentity>, ContentDigest),
        file: FileCapability,
        digest: ContentDigest,
    ) -> Result<PersistenceAuthoredUpgradeCommit<'_>, EditorPersistenceRejection> {
        let result = match self {
            Self::Scene(host) => host.bind_authored_upgrade(expected, file, digest),
            Self::Prefab(host) => host.bind_authored_upgrade(
                &EditorDocumentSourceBinding::new(path),
                expected,
                file,
                digest,
            ),
        };
        result.map_err(|failure| failure.reason)
    }
}

#[test]
fn authored_upgrade_binds_the_new_leaf_without_adoption_until_commit() {
    for prefab in [false, true] {
        let root = TestRoot::new();
        let (mut host, path, directory) = Host::new(&root, prefab);
        let before = host
            .file()
            .publication
            .accepted_source_observation()
            .unwrap();
        let encoding = host.file().encoding;
        let limits = host.file().limits;
        let original = fs::read(root.path.join(path)).unwrap();
        let mut next = original.clone();
        next.extend_from_slice(b"\n");
        fs::rename(
            root.path.join(path),
            root.path.join("retained-original.json"),
        )
        .unwrap();
        fs::write(root.path.join(path), &next).unwrap();
        let digest = ContentDigest::of_bytes(&next);
        let current = || {
            directory
                .open_file(&RelativePath::new(path).unwrap())
                .unwrap()
        };
        let after_id = current().identity();
        assert_ne!(before.0, after_id);
        let prepared_leaf = current();
        let prepared_id = prepared_leaf.identity();
        let returned = host
            .bind(path, before, prepared_leaf, digest)
            .unwrap()
            .into_replacement();
        assert_eq!(returned.identity(), prepared_id);
        drop(returned);
        assert_eq!(
            host.file().publication.accepted_source_observation(),
            Some(before)
        );

        host.file().validated_authoring_digest = Some(before.2);
        host.bind(path, before, current(), digest).unwrap().commit();
        assert_eq!(
            host.file().publication.accepted_source_observation(),
            Some((after_id, before.1, digest))
        );
        assert_eq!(host.file().validated_authoring_digest, None);
        assert_eq!(host.file().encoding, encoding);
        assert_eq!(host.file().limits, limits);
    }
}

#[test]
fn authored_upgrade_refuses_wrong_old_observation_name_digest_and_original_file_bounds() {
    for prefab in [false, true] {
        let root = TestRoot::new();
        let (mut host, path, directory) = Host::new(&root, prefab);
        let before = host
            .file()
            .publication
            .accepted_source_observation()
            .unwrap();
        let current = || {
            directory
                .open_file(&RelativePath::new(path).unwrap())
                .unwrap()
        };
        let mut wrong = before;
        wrong.2 = ContentDigest::of_bytes(b"not the accepted source");
        assert!(matches!(
            host.bind(path, wrong, current(), before.2),
            Err(EditorPersistenceRejection::TargetChanged)
        ));
        let other = directory
            .open_file(&RelativePath::new("other.prefab.json").unwrap())
            .unwrap();
        let other_digest = other.digest(65_536).unwrap();
        assert!(matches!(
            host.bind(path, before, other, other_digest),
            Err(EditorPersistenceRejection::TargetChanged)
        ));
        assert!(matches!(
            host.bind(path, before, current(), wrong.2),
            Err(EditorPersistenceRejection::TargetChanged)
        ));
        if prefab {
            assert!(matches!(
                host.bind("foreign.prefab.json", before, current(), before.2),
                Err(EditorPersistenceRejection::SourceBindingMismatch)
            ));
        }
        host.file().limits = host
            .file()
            .limits
            .with_encoded_bytes(nara_core::ByteLimit::new(before.2.length() as usize - 1).unwrap());
        assert!(matches!(
            host.bind(path, before, current(), before.2),
            Err(EditorPersistenceRejection::TargetChanged)
        ));
        assert_eq!(
            host.file().publication.accepted_source_observation(),
            Some(before)
        );
        host.file().publication.mark_uncertain();
        assert!(matches!(
            host.bind(path, before, current(), before.2),
            Err(EditorPersistenceRejection::RequiresReconcile)
        ));
    }
}
