use super::*;

const R: &str = "mobile/scenes/r.scene.json";
const CONTROL: &str = "scenes/control.scene.json";

fn fixture() -> Fixture {
    let project = Fixture::new(
        "stable_metadata",
        r#"
[profiles.desktop.paths]
assets = "desktop/assets"
scenes = "desktop/scenes"
prefabs = "desktop/prefabs"
scripts = "mobile/scenes"
[profiles.mobile.paths]
assets = "mobile/assets"
scenes = "mobile/scenes"
prefabs = "mobile/prefabs"
scripts = "mobile/scripts"
"#,
    );
    for (path, alias, id) in [
        (R, "r.scene.json", FIRST_ID),
        (CONTROL, "control.scene.json", SECOND_ID),
    ] {
        project.scene(path);
        project.metadata(path, alias, id, AssetSourceKind::Scene, 0);
    }
    project
}

fn capture_claims(
    schema: &AuthoringSchemaReceipt,
    open: &[OpenSource<'_>],
    claims: &[SourceClaim<'_>],
) -> Result<SourceInventory, SourceInventoryError> {
    let mut budget = schema.budget;
    with_candidate_storage(&mut budget, |budget| {
        let manifest = SourceInventoryManifest::parse(schema.manifest_bytes(), budget)?;
        let root = schema.project_root();
        SourceInventory::capture_admitted(
            SourceInventoryInput {
                root: &root,
                manifest_bytes: schema.manifest_bytes(),
                manifest_identity: schema.manifest_identity(),
                manifest_digest: schema.manifest_digest(),
                bundle_path: schema.bundle_path().as_str(),
                bundle_identity: schema.bundle_identity(),
                bundle_digest: schema.bundle_digest(),
                selected_settings: Some(schema.settings()),
            },
            manifest,
            open,
            claims,
            &[],
            SceneFileLimits::default(),
            budget,
        )
    })
}

fn assert_contexts(inventory: &SourceInventory, explicit_native: bool) {
    assert_eq!(paths(inventory), [R, CONTROL]);
    assert_eq!(inventory.metadata().len(), 2);
    let configurations = inventory.configurations();
    assert_eq!(
        configurations
            .iter()
            .map(ConfigurationPolicy::profile_name)
            .collect::<Vec<_>>(),
        [None, Some("desktop"), Some("mobile")],
    );
    assert_eq!(
        configurations[0].participant_paths().collect::<Vec<_>>(),
        [CONTROL],
    );
    let desktop: &[&str] = if explicit_native { &[R] } else { &[] };
    assert_eq!(
        configurations[1].participant_paths().collect::<Vec<_>>(),
        desktop,
    );
    assert_eq!(
        configurations[2].participant_paths().collect::<Vec<_>>(),
        [R],
    );
    let metadata = inventory.metadata_for_source(R).unwrap();
    assert_eq!(metadata.policy, 2);
    let visibility: &[(usize, Option<usize>)] = if explicit_native {
        &[(1, None), (2, Some(2))]
    } else {
        &[(2, Some(2))]
    };
    assert_eq!(
        metadata
            .visibility()
            .iter()
            .map(|policy| (
                policy.configuration_index(),
                policy.catalogue_binding_index()
            ))
            .collect::<Vec<_>>(),
        visibility,
    );
    for policy in metadata.visibility() {
        assert_eq!(policy.scope().prefix().unwrap().as_str(), "mobile/scenes");
        assert_eq!(
            policy.scope().relative_path(metadata.record().path()),
            Some("r.scene.json"),
        );
    }
    assert_eq!(
        inventory.participant(R).unwrap().explicit_native(),
        explicit_native,
    );
}

#[test]
fn complete_physical_claims_keep_closed_profile_contexts_and_exact_metadata() {
    let project = fixture();
    let schema = project.receipt(Some("mobile"));
    let original = capture(&schema);
    assert_contexts(&original, false);
    assert!(
        original
            .participants()
            .all(|source| !source.explicit_native())
    );
    let mut claims = original
        .participants()
        .map(|source| SourceClaim {
            path: source.file().path(),
            kind: source.kind(),
            explicit_native: source.explicit_native(),
        })
        .collect::<Vec<_>>();
    let physical = capture_claims(&schema, &[], &claims).unwrap();
    assert_eq!(physical, original);
    assert_contexts(&physical, false);

    // An unrelated explicit native claim must not authorize R's Scripts context.
    claims
        .iter_mut()
        .find(|claim| claim.path == CONTROL)
        .unwrap()
        .explicit_native = true;
    let mixed = capture_claims(&schema, &[], &claims).unwrap();
    assert_contexts(&mixed, false);
    assert_eq!(mixed.participant(R), original.participant(R));
    assert_eq!(mixed.metadata, original.metadata);
    assert_eq!(mixed.configurations(), original.configurations());
    let closed = original.participant(CONTROL).unwrap();
    let explicit = mixed.participant(CONTROL).unwrap();
    assert!(!closed.explicit_native());
    assert!(explicit.explicit_native());
    assert_eq!(explicit.file(), closed.file());
    assert_eq!(explicit.native_source(), closed.native_source());
    assert_eq!(explicit.encoding(), closed.encoding());
    assert_eq!(explicit.bytes(), closed.bytes());
    assert_ne!(explicit, closed);
}

#[test]
fn actual_open_guard_and_explicit_claim_retain_only_their_admitted_scripts_context() {
    let project = fixture();
    let schema = project.receipt(Some("mobile"));
    let original = capture(&schema);
    let source = project.open_source(&schema, R);
    let open = [OpenSource::Bound(&source)];
    let mut budget = schema.budget;
    let ordinary_open =
        SourceInventory::capture(&schema, &open, SceneFileLimits::default(), &mut budget).unwrap();
    // Preserve the actual captured flags when reopening the physical inventory without a
    // Workspace open fact, as a persistence package must do for its original participants.
    let mut claims = ordinary_open
        .participants()
        .map(|source| SourceClaim {
            path: source.file().path(),
            kind: source.kind(),
            explicit_native: source.explicit_native(),
        })
        .collect::<Vec<_>>();
    let explicit = capture_claims(&schema, &[], &claims).unwrap();
    assert_eq!(explicit.captured_open_sources().len(), 0);
    claims
        .iter_mut()
        .find(|claim| claim.path == R)
        .unwrap()
        .explicit_native = false;
    let claimed_open = capture_claims(&schema, &open, &claims).unwrap();
    assert_eq!(claimed_open, ordinary_open);
    assert_eq!(ordinary_open.captured_open_sources().len(), 1);

    let closed = original.participant(R).unwrap();
    let closed_metadata = original.metadata_for_source(R).unwrap();
    let disk = fs::read(project.root.join(R)).unwrap();
    let sidecar = fs::read(project.root.join(format!("{R}.meta"))).unwrap();
    for inventory in [&explicit, &ordinary_open, &claimed_open] {
        assert_contexts(inventory, true);
        assert_eq!(inventory.metadata, explicit.metadata);
        let participant = inventory.participant(R).unwrap();
        assert!(participant.file().matches_open(&source));
        assert_eq!(participant.native_source(), source.native_source());
        assert_eq!(participant.file(), closed.file());
        assert_eq!(participant.native_source(), closed.native_source());
        assert_eq!(participant.encoding(), closed.encoding());
        assert_eq!(participant.bytes(), disk);
        assert_ne!(participant, closed);
        let metadata = inventory.metadata_for_source(R).unwrap();
        assert_eq!(metadata.file(), closed_metadata.file());
        assert_eq!(metadata.record(), closed_metadata.record());
        assert_eq!(metadata.authored_path(), closed_metadata.authored_path());
        assert_eq!(metadata.bytes(), sidecar);
        assert_eq!(
            inventory.participant(CONTROL),
            original.participant(CONTROL)
        );
        assert_eq!(inventory.metadata[CONTROL], original.metadata[CONTROL]);
    }
}

#[test]
fn physical_claim_outside_catalogue_roots_cannot_use_native_fallback() {
    let project = Fixture::new("project_location", "");
    let path = "unscoped/r.scene.json";
    project.scene(path);
    let schema = project.receipt(None);
    let mut claims = [SourceClaim {
        path,
        kind: SourceDocumentKind::Scene,
        explicit_native: false,
    }];
    assert!(matches!(
        capture_claims(&schema, &[], &claims),
        Err(SourceInventoryError::MembershipChanged),
    ));
    claims[0].explicit_native = true;
    let inventory = capture_claims(&schema, &[], &claims).unwrap();
    assert_eq!(paths(&inventory), [path]);
    assert!(inventory.participant(path).unwrap().explicit_native());
    assert_eq!(inventory.captured_open_sources().len(), 0);
    assert_eq!(
        inventory.configurations()[0]
            .participant_paths()
            .collect::<Vec<_>>(),
        [path],
    );
}

#[test]
fn explicit_location_claim_restores_the_selected_context_outside_its_catalogue_roots() {
    let project = Fixture::new(
        "project_location",
        "[profiles.mobile.paths]\nscenes = 'mobile/scenes'\n",
    );
    project.scene(R);
    let schema = project.receipt(None);
    // This actual ProjectLocation source is intentionally outside the selected Browser roots.
    // Bind the Host's real opened file rather than inventing a source-index row for it.
    let root = schema.project_root();
    let file = root
        .open_file_exact(&RelativePath::new(R).unwrap())
        .unwrap();
    let digest = file.digest(65_536).unwrap();
    let native = NativeDocumentSourcePolicy::new(schema.settings())
        .admit(&root, R, SourceDocumentKind::Scene, file.identity())
        .unwrap();
    let source = AuthoringDocumentSourceFact::from_native_observation(
        R,
        file.identity(),
        file.parent_identity(),
        digest,
        &native,
    )
    .unwrap();
    let mut budget = schema.budget;
    let original = SourceInventory::capture(
        &schema,
        &[OpenSource::Bound(&source)],
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    let claims = [SourceClaim {
        path: R,
        kind: SourceDocumentKind::Scene,
        explicit_native: true,
    }];
    let restored = capture_claims(&schema, &[], &claims).unwrap();
    assert_eq!(restored.selected_profile(), original.selected_profile());
    assert_eq!(restored.configurations(), original.configurations());
    assert_eq!(restored.participant(R), original.participant(R));
    assert_eq!(restored.captured_open_sources().len(), 0);
    assert_eq!(original.captured_open_sources().len(), 1);
    assert_eq!(
        restored.configurations()[0]
            .participant_paths()
            .collect::<Vec<_>>(),
        [R]
    );
    assert_eq!(
        restored.configurations()[1]
            .participant_paths()
            .collect::<Vec<_>>(),
        [R]
    );

    let discovered = capture(&schema);
    assert_eq!(discovered.configurations()[0].participant_paths().len(), 0);
    assert_eq!(
        discovered.configurations()[1]
            .participant_paths()
            .collect::<Vec<_>>(),
        [R]
    );
    assert!(!discovered.participant(R).unwrap().explicit_native());
}
