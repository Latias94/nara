use super::*;

fn content() -> (CapturedPack, RuntimeContentExpectation) {
    let catalog = b"selected catalog";
    let pack = CapturedPack::new(catalog.to_vec(), vec![b"encoded member".to_vec()]).unwrap();
    let expected = RuntimeContentExpectation::new(
        ContentDigest::of_bytes(catalog),
        RuntimeContentTarget::new("x86_64-pc-windows-msvc", "test-content").unwrap(),
    );
    (pack, expected)
}

#[test]
fn cooked_pack_rejects_an_unrelated_expectation_before_source_check() {
    let (pack, expected) = content();
    let expected = RuntimeContentExpectation::new(
        ContentDigest::of_bytes(b"different catalog"),
        expected.target().clone(),
    );
    let output = tempfile::tempdir().unwrap();
    let parent = crate::host_fs::writable_directory(output.path()).unwrap();
    let destination = RelativeComponent::new("Cooked").unwrap();
    let result = publish_cooked_pack(
        &pack,
        &expected,
        &parent,
        &destination,
        || -> Result<(), io::Error> {
            panic!("catalog rejection must precede source checks");
        },
    );
    assert!(matches!(
        result,
        Err(CookedPackPublicationError::CatalogMismatch)
    ));
    assert_eq!(output.path().read_dir().unwrap().count(), 0);
}

#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
#[test]
fn cooked_pack_publishes_original_bytes_without_replacing_an_existing_destination() {
    let (pack, expected) = content();
    let output = tempfile::tempdir().unwrap();
    let parent = crate::host_fs::writable_directory(output.path()).unwrap();
    let destination = RelativeComponent::new("Cooked").unwrap();
    let mut checks = 0;
    publish_cooked_pack(&pack, &expected, &parent, &destination, || {
        checks += 1;
        Ok::<(), io::Error>(())
    })
    .unwrap();
    assert_eq!(checks, 2);
    let mut encoded = Vec::new();
    pack.write(&mut encoded).unwrap();
    let path = output.path().join("Cooked").join(PLAYER_PACK_FILE);
    assert_eq!(std::fs::read(&path).unwrap(), encoded);
    assert!(
        publish_cooked_pack(&pack, &expected, &parent, &destination, || {
            Ok::<(), io::Error>(())
        })
        .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), encoded);
    assert_eq!(output.path().read_dir().unwrap().count(), 1);
}

#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
#[test]
fn cooked_pack_keeps_staging_evidence_after_the_second_source_check_fails() {
    let (pack, expected) = content();
    let output = tempfile::tempdir().unwrap();
    let parent = crate::host_fs::writable_directory(output.path()).unwrap();
    let destination = RelativeComponent::new("Cooked").unwrap();
    let mut checks = 0;
    let result = publish_cooked_pack(&pack, &expected, &parent, &destination, || {
        checks += 1;
        if checks == 1 {
            Ok(())
        } else {
            Err(io::Error::other("source changed after writing the pack"))
        }
    });
    let Err(CookedPackPublicationError::Orphaned {
        cause: Some(CookedPackStagingError::Selection(_)),
        publication,
    }) = result
    else {
        panic!("expected the original staged pack, got {result:?}");
    };
    assert_eq!(checks, 2);
    assert!(!output.path().join("Cooked").exists());
    let staged_pack = output
        .path()
        .join(publication.staging_name().to_str().unwrap())
        .join(PLAYER_PACK_FILE);
    let mut encoded = Vec::new();
    pack.write(&mut encoded).unwrap();
    assert_eq!(std::fs::read(staged_pack).unwrap(), encoded);
}

#[test]
fn generated_rust_input_preserves_independent_facts_in_the_canonical_golden() {
    let expected = RuntimeContentExpectation::new(
        ContentDigest::from_parts(123, [7; 32]),
        RuntimeContentTarget::new("x86_64-pc-windows-msvc", "desktop-release").unwrap(),
    );
    let mut generated = String::new();
    write_bootstrap(&mut generated, &expected).unwrap();
    assert_eq!(generated, include_str!("bootstrap.golden.rs"));
}

#[test]
fn bootstrap_byte_and_overlapping_content_retention_limits_are_exact() {
    let (pack, expected) = content();
    let limits = PlayerPackageLimits::default();
    let baseline =
        GeneratedPlayerBootstrap::new(&pack, &expected, "src/content_expectation.rs", limits)
            .unwrap();
    let bytes = baseline.bytes().len();
    let retained = pack.retained_bytes() + baseline.retained_bytes();
    let exact = PlayerPackageLimits {
        bootstrap_bytes: ByteLimit::new(bytes).unwrap(),
        retained_bytes: ByteLimit::new(retained).unwrap(),
        work_bytes: ByteLimit::new(retained).unwrap(),
        ..limits
    };
    let admitted = GeneratedPlayerBootstrap::new(&pack, &expected, baseline.name(), exact).unwrap();
    assert_eq!(admitted.digest(), baseline.digest());
    assert_eq!(admitted.bytes(), baseline.bytes());
    for one_short in [
        PlayerPackageLimits {
            bootstrap_bytes: ByteLimit::new(bytes - 1).unwrap(),
            ..exact
        },
        PlayerPackageLimits {
            retained_bytes: ByteLimit::new(retained - 1).unwrap(),
            ..exact
        },
        PlayerPackageLimits {
            work_bytes: ByteLimit::new(retained - 1).unwrap(),
            ..exact
        },
    ] {
        assert!(matches!(
            GeneratedPlayerBootstrap::new(&pack, &expected, baseline.name(), one_short),
            Err(PlayerPackageError::Limit { .. }),
        ));
        assert_eq!(pack.member(0).unwrap(), b"encoded member");
    }
}

#[test]
fn generated_input_rejects_alternate_or_nonportable_path_spellings() {
    for path in [
        "",
        "../expected.rs",
        "src//expected.rs",
        "src/./expected.rs",
        "src\\expected.rs",
        "/expected.rs",
    ] {
        assert!(validate_input_name(path).is_err(), "accepted {path:?}");
    }
    validate_input_name("src/expected.rs").unwrap();
}

#[test]
fn changed_content_expectation_cannot_reuse_the_original_generated_input() {
    let (pack, expected) = content();
    let limits = PlayerPackageLimits::default();
    let bootstrap =
        GeneratedPlayerBootstrap::new(&pack, &expected, "src/expected.rs", limits).unwrap();
    check_bootstrap(&pack, &expected, &bootstrap).unwrap();

    let foreign_target = RuntimeContentExpectation::new(
        expected.catalog_digest(),
        RuntimeContentTarget::new(expected.target().triple(), "other-profile").unwrap(),
    );
    assert!(matches!(
        check_bootstrap(&pack, &foreign_target, &bootstrap),
        Err(PlayerPackageError::BootstrapMismatch)
    ));

    let digest = expected.catalog_digest();
    for wrong in [
        ContentDigest::from_parts(digest.length(), [0; 32]),
        ContentDigest::from_parts(digest.length() + 1, *digest.as_bytes()),
    ] {
        let foreign = RuntimeContentExpectation::new(wrong, expected.target().clone());
        assert!(matches!(
            GeneratedPlayerBootstrap::new(&pack, &foreign, bootstrap.name(), limits),
            Err(PlayerPackageError::CatalogMismatch)
        ));
        assert!(matches!(
            check_bootstrap(&pack, &foreign, &bootstrap),
            Err(PlayerPackageError::CatalogMismatch)
        ));
    }
}

#[test]
fn binding_requires_the_request_owned_generated_input() {
    let (pack, expected) = content();
    let limits = PlayerPackageLimits::default();
    let bootstrap =
        GeneratedPlayerBootstrap::new(&pack, &expected, "player-bootstrap.rs", limits).unwrap();
    assert!(matches!(
        check_generated_build_input(&bootstrap, None),
        Err(PlayerPackageError::BuildInputMismatch)
    ));
}

#[test]
fn streaming_pack_extent_matches_actual_bytes_and_refuses_short_or_excess_writes() {
    let (pack, _) = content();
    let mut bytes = Vec::new();
    pack.write(&mut bytes).unwrap();
    assert_eq!(pack.encoded_bytes(), bytes.len() as u64);
    let mut stream = DigestWriter::new(io::sink(), pack.encoded_bytes());
    pack.write(&mut stream).unwrap();
    assert_eq!(stream.finish().unwrap(), ContentDigest::of_bytes(&bytes));
    let mut excess = DigestWriter::new(io::sink(), bytes.len() as u64 - 1);
    assert!(pack.write(&mut excess).is_err());
    let mut short = DigestWriter::new(io::sink(), bytes.len() as u64 + 1);
    pack.write(&mut short).unwrap();
    assert!(matches!(
        short.finish(),
        Err(PlayerPackageError::StagedBytes)
    ));
}

#[test]
fn callback_panic_and_cancellation_keep_the_exact_control_point() {
    let point = PlayerPackagePoint::AfterPackWrite;
    let panic = checkpoint(&mut |_| panic!("injected Host fence panic"), point);
    assert!(matches!(
        panic,
        Err(PlayerPackageError::Control {
            point: PlayerPackagePoint::AfterPackWrite,
            failure: PlayerPackageControlFailure::Panicked,
        })
    ));
    let cancelled = checkpoint(&mut |_| Err(PlayerPackageControlFailure::Cancelled), point);
    assert!(matches!(
        cancelled,
        Err(PlayerPackageError::Control {
            point: PlayerPackagePoint::AfterPackWrite,
            failure: PlayerPackageControlFailure::Cancelled,
        })
    ));
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn staged_inventory_rejects_extra_missing_and_non_file_members() {
    use std::fs;

    let root = tempfile::tempdir().unwrap();
    let staging = crate::host_fs::writable_directory(root.path()).unwrap();
    for name in [PLAYER_PACK_FILE, PLAYER_PACKAGE_MANIFEST_FILE, "game.exe"] {
        write_member(&staging, name, b"inventory owner test", false).unwrap();
    }
    validate_staged_members(&staging, "game.exe").unwrap();
    write_member(&staging, "undeclared.bin", b"extra disk payload", false).unwrap();
    assert!(matches!(
        validate_staged_members(&staging, "game.exe"),
        Err(PlayerPackageError::StagedMembers)
    ));
    fs::remove_file(root.path().join("undeclared.bin")).unwrap();
    fs::remove_file(root.path().join(PLAYER_PACK_FILE)).unwrap();
    assert!(matches!(
        validate_staged_members(&staging, "game.exe"),
        Err(PlayerPackageError::StagedMembers)
    ));
    fs::create_dir(root.path().join(PLAYER_PACK_FILE)).unwrap();
    assert!(matches!(
        validate_staged_members(&staging, "game.exe"),
        Err(PlayerPackageError::StagedMembers)
    ));
}
