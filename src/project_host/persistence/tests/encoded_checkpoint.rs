use super::*;

fn capture(
    prefab: bool,
) -> (
    EditorWorkspace,
    nara_tooling::EditorPersistenceAuthority,
    EditorPersistenceCheckpoint,
) {
    let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
    let document = if prefab {
        workspace
            .open_prefab_session(
                "Prefab",
                EditorDocumentSourceBinding::new("reference-ui.prefab.json"),
                PrefabAuthoringSession::new(PrefabDocument::default()),
            )
            .unwrap()
            .opened_document
            .unwrap()
    } else {
        workspace
            .open_scene_session(
                "Scene",
                SceneAuthoringSession::new(SceneDocument::default()),
            )
            .unwrap()
            .opened_document
            .unwrap()
    };
    let checkpoint = authority.capture(&workspace, document).unwrap();
    (workspace, authority, checkpoint)
}

#[test]
fn valid_publication_of_other_bytes_cannot_acknowledge_a_captured_source() {
    for prefab in [false, true] {
        let root = TestRoot::new();
        let directory = root.capability();
        let path = if prefab {
            "reference-ui.prefab.json"
        } else {
            "startup.scene.json"
        };
        let original = directory
            .open_file(&RelativePath::new(path).unwrap())
            .unwrap();
        let original_digest = original.digest(65_536).unwrap();
        let mut publication = FilePublicationHost::bind_existing(
            &directory,
            path,
            &original,
            original_digest,
            65_536,
        )
        .unwrap();
        let (mut workspace, mut authority, checkpoint) = capture(prefab);
        let document = checkpoint.document();
        let revision = checkpoint.revision();
        let encoded = checkpoint
            .encode(SceneFileEncoding::Ron, SceneFileLimits::default())
            .unwrap();
        let allocation = encoded.bytes().as_ptr();
        let expected = encoded.bytes().to_vec();

        let FilePublicationOutcome::Published(unrelated) =
            publication.publish(b"unrelated actual output")
        else {
            panic!("the unrelated filesystem control must publish successfully")
        };
        let (evidence, source) = unrelated.into_parts();
        let (receipt, _) = evidence.into_parts();
        assert_eq!(
            receipt.verified_content_digest(),
            Some(ContentDigest::of_bytes(b"unrelated actual output"))
        );
        let failure =
            EditorPersistenceCommit::from_publication(encoded, original.identity(), receipt)
                .unwrap_err();
        let (encoded, receipt) = failure.into_parts();
        assert_eq!(encoded.bytes().as_ptr(), allocation);
        assert_eq!(encoded.checkpoint().revision(), revision);
        assert_eq!(encoded.bytes(), expected);
        assert_ne!(receipt.verified_content_digest(), Some(encoded.digest()));
        let unrelated_identity = source.identity();
        publication.commit_published(source);

        let observed = directory
            .open_file(&RelativePath::new(path).unwrap())
            .unwrap();
        let failure = EditorPersistenceCommit::from_observed_file(encoded, &observed).unwrap_err();
        let (encoded, _) = failure.into_parts();
        assert_eq!(encoded.bytes().as_ptr(), allocation);

        let FilePublicationOutcome::Published(accepted) = publication.publish(encoded.bytes())
        else {
            panic!("the exact captured output must publish successfully")
        };
        let (evidence, source) = accepted.into_parts();
        let (receipt, _) = evidence.into_parts();
        let commit =
            EditorPersistenceCommit::from_publication(encoded, unrelated_identity, receipt)
                .unwrap();
        assert_eq!(commit.document(), document);
        assert_eq!(commit.revision(), revision);
        assert_eq!(fs::read(root.path.join(path)).unwrap(), expected);
        publication.commit_published(source);
        let report = authority.commit(&mut workspace, commit);
        assert!(report.applied, "{report:?}");
        let saved = workspace.authoring_model();
        assert_eq!(saved.documents[0].saved_revision, revision);
    }
}

#[test]
fn observed_output_still_requires_the_hosts_actual_current_named_leaf() {
    let root = TestRoot::new();
    let directory = root.capability();
    let (_, _, checkpoint) = capture(false);
    let encoded = checkpoint
        .encode(SceneFileEncoding::Json, SceneFileLimits::default())
        .unwrap();
    let bytes = encoded.bytes().to_vec();
    let original = directory
        .open_file(&RelativePath::new("startup.scene.json").unwrap())
        .unwrap();
    let digest = original.digest(65_536).unwrap();
    let (mut host, _) = ScenePersistenceHost::bind_authoring_source(
        &directory,
        "startup.scene.json",
        original,
        digest,
    )
    .unwrap();
    let expected = host.accepted_source_observation().unwrap();
    fs::write(root.path.join("same-bytes.scene.json"), &bytes).unwrap();
    let other = directory
        .open_file(&RelativePath::new("same-bytes.scene.json").unwrap())
        .unwrap();
    let commit = EditorPersistenceCommit::from_observed_file(encoded, &other).unwrap();
    assert_eq!(
        commit.digest(),
        editor_digest(ContentDigest::of_bytes(&bytes))
    );
    let failure = match host.bind_published_leaf(expected, other, ContentDigest::of_bytes(&bytes)) {
        Err(failure) => failure,
        Ok(_) => panic!("identical contents at another leaf do not bind the Host"),
    };
    assert_eq!(failure.reason, EditorPersistenceRejection::TargetChanged);
    drop(failure);
    assert_eq!(host.accepted_source_observation(), Some(expected));
}

#[test]
fn encoded_output_failure_preserves_the_captured_source_allocation_for_retry() {
    let (_, _, checkpoint) = capture(false);
    let identity = std::ptr::from_ref(checkpoint.scene().unwrap());
    let revision = checkpoint.revision();
    let failure = checkpoint
        .encode_with_admission(SceneFileEncoding::Json, SceneFileLimits::default(), |_| {
            Err(73_usize)
        })
        .unwrap_err();
    let (checkpoint, error) = failure.into_parts();
    assert!(matches!(
        error,
        nara_tooling::EditorPersistenceEncodingError::Admission(73)
    ));
    assert_eq!(checkpoint.revision(), revision);
    assert_eq!(std::ptr::from_ref(checkpoint.scene().unwrap()), identity);
    let encoded = checkpoint
        .encode(SceneFileEncoding::Json, SceneFileLimits::default())
        .unwrap();
    assert_eq!(encoded.digest(), ContentDigest::of_bytes(encoded.bytes()));
}

#[test]
fn exact_digest_scan_is_admitted_after_encoding_and_refusal_returns_the_original_capture() {
    for prefab in [false, true] {
        for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
            let (_, _, checkpoint) = capture(prefab);
            let mut requests = Vec::new();
            let output = checkpoint
                .encode_with_admission(encoding, SceneFileLimits::default(), |bytes| {
                    requests.push(bytes);
                    Ok::<_, std::convert::Infallible>(())
                })
                .unwrap();
            assert!(requests.len() > 1);
            assert_eq!(requests.last(), Some(&output.bytes().len()));
            let digest = output.digest();
            let checkpoint = output.into_checkpoint();
            let revision = checkpoint.revision();
            let allocation = if prefab {
                std::ptr::from_ref(checkpoint.prefab().unwrap()) as usize
            } else {
                std::ptr::from_ref(checkpoint.scene().unwrap()) as usize
            };
            let mut called = 0;
            let failure = checkpoint
                .encode_with_admission(encoding, SceneFileLimits::default(), |bytes| {
                    assert_eq!(bytes, requests[called]);
                    called += 1;
                    if called == requests.len() {
                        Err("digest scan refused")
                    } else {
                        Ok(())
                    }
                })
                .unwrap_err();
            assert_eq!(called, requests.len());
            let (checkpoint, error) = failure.into_parts();
            assert!(matches!(
                error,
                nara_tooling::EditorPersistenceEncodingError::Admission("digest scan refused")
            ));
            assert_eq!(checkpoint.revision(), revision);
            let returned = if prefab {
                std::ptr::from_ref(checkpoint.prefab().unwrap()) as usize
            } else {
                std::ptr::from_ref(checkpoint.scene().unwrap()) as usize
            };
            assert_eq!(returned, allocation);
            let retried = checkpoint
                .encode(encoding, SceneFileLimits::default())
                .unwrap();
            assert_eq!(retried.digest(), digest);
        }
    }
}
