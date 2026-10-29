use super::*;
use nara_asset::{AssetProductRef, ImportedProductId};
use nara_image::ImageColorSpace;

fn source_less_image(record: &AssetRecord) -> ImageAsset {
    ImageAsset::from_product(
        AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY).unwrap(),
        ImageExtent::new(2, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![30; 8],
    )
    .unwrap()
}

fn publish_unmatched_image(fixture: &mut Fixture, wrong_path: bool) -> Handle<ImageAsset> {
    let handle = fixture
        .server
        .reserve_record::<ImageAsset>(&fixture.image_a)
        .unwrap();
    let candidate = if wrong_path {
        image(
            &AssetRecord::new(
                fixture.image_a.stable_id(),
                AssetPath::new("other.png").unwrap(),
                AssetSourceKind::Image,
            ),
            2,
            1,
            30,
        )
    } else {
        source_less_image(&fixture.image_a)
    };
    fixture
        .images
        .commit_loaded(
            handle,
            candidate,
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
    handle
}

fn assert_reconcile_stable(fixture: &mut Fixture, handle: Handle<TileSet>) {
    let version = fixture.states.version(handle.id());
    let state = fixture.states.state_revision(handle.id());
    let slot = fixture.tilesets.slot_revision(handle);
    let events = fixture.events.len();
    for _ in 0..3 {
        fixture.reconcile();
        assert_eq!(fixture.states.version(handle.id()), version);
        assert_eq!(fixture.states.state_revision(handle.id()), state);
        assert_eq!(fixture.tilesets.slot_revision(handle), slot);
        assert_eq!(fixture.events.len(), events);
    }
}

#[test]
fn new_source_rejects_processed_or_wrong_path_images_without_publication_churn() {
    for wrong_path in [false, true] {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        publish_unmatched_image(&mut fixture, wrong_path);
        let handle = fixture.request("a.png");
        fixture.reconcile();
        assert_eq!(
            fixture.owner.error(handle),
            Some(&TileSetAssetError::IdentityMismatch)
        );
        assert!(fixture.tilesets.get(handle).is_none());
        assert_eq!(fixture.owner.retention().images, 0);
        assert_reconcile_stable(&mut fixture, handle);

        fixture.publish_image(fixture.image_a.clone(), 2, 1, 40);
        fixture.reconcile();
        assert_eq!(fixture.owner.error(handle), None);
        assert_eq!(
            fixture
                .tilesets
                .get(handle)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[40; 8]
        );
        assert_reconcile_stable(&mut fixture, handle);
    }
}

#[test]
fn rejected_successor_keeps_last_good_until_its_publication_is_revoked() {
    for wrong_path in [false, true] {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let handle = fixture.request("a.png");
        fixture.reconcile();
        let old = fixture
            .tilesets
            .get(handle)
            .unwrap()
            .image_content()
            .unwrap()
            .clone();
        let slot = fixture.tilesets.slot_revision(handle);
        let version = fixture.states.version(handle.id());
        let dependency = publish_unmatched_image(&mut fixture, wrong_path);
        fixture.reconcile();
        assert_eq!(
            fixture.owner.error(handle),
            Some(&TileSetAssetError::IdentityMismatch)
        );
        assert_eq!(fixture.tilesets.slot_revision(handle), slot);
        assert_eq!(fixture.states.version(handle.id()), version);
        assert_eq!(
            fixture.tilesets.get(handle).unwrap().image_content(),
            Some(&old)
        );
        assert!(old.is_authorized(&fixture.server, &fixture.states, &fixture.images));
        assert_reconcile_stable(&mut fixture, handle);

        fixture
            .images
            .remove_with_state(dependency, &mut fixture.states, &mut fixture.events)
            .unwrap();
        fixture.reconcile();
        assert!(fixture.tilesets.get(handle).is_none());
        assert!(!old.is_authorized(&fixture.server, &fixture.states, &fixture.images));
        assert_eq!(old.pixels(), &[10; 8]);
        assert_reconcile_stable(&mut fixture, handle);
    }
}

#[test]
fn new_source_rejects_foreign_source_identity_and_non_image_records() {
    for wrong_kind in [false, true] {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        if wrong_kind {
            assert!(fixture.database.remove_exact(&fixture.image_a));
            fixture
                .database
                .insert(AssetRecord::new(
                    fixture.image_a.stable_id(),
                    fixture.image_a.path().clone(),
                    AssetSourceKind::Other(String::from("not-image")),
                ))
                .unwrap();
        } else {
            let handle = fixture
                .server
                .reserve_record::<ImageAsset>(&fixture.image_a)
                .unwrap();
            fixture
                .images
                .commit_loaded(
                    handle,
                    image(&fixture.image_b, 2, 1, 30),
                    &mut fixture.states,
                    &mut fixture.events,
                    None,
                    None,
                )
                .unwrap();
        }
        let handle = fixture.request("a.png");
        fixture.reconcile();
        assert!(fixture.tilesets.get(handle).is_none());
        assert_eq!(
            fixture.owner.error(handle),
            Some(&TileSetAssetError::DependencyUnavailable)
        );
        assert_reconcile_stable(&mut fixture, handle);
    }
}

#[test]
fn source_dependency_reload_status_preserves_tileset_until_a_new_image_publication() {
    let mut fixture = Fixture::new(TileSetAssetLimits::default());
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let original = fixture.tilesets.get(handle).unwrap().clone();
    let image = original.image_content().unwrap();
    let image_handle = image.handle();
    let slot = fixture.tilesets.slot_revision(handle);
    let state = fixture.states.state_revision(handle.id());
    let version = fixture.states.version(handle.id());
    let retention = fixture.owner.retention();

    for failed in [false, true] {
        if failed {
            fixture
                .images
                .record_reload_failure(
                    image_handle,
                    &mut fixture.states,
                    &mut fixture.events,
                    "successor decode failed",
                )
                .unwrap();
        } else {
            fixture.states.set_loading(image_handle.id());
        }
        let events = fixture.events.len();
        fixture.reconcile();
        assert_eq!(fixture.tilesets.get(handle), Some(&original));
        assert_eq!(fixture.tilesets.slot_revision(handle), slot);
        assert_eq!(fixture.states.state_revision(handle.id()), state);
        assert_eq!(fixture.states.version(handle.id()), version);
        assert_eq!(fixture.events.len(), events);
        assert_eq!(fixture.owner.retention(), retention);
        assert_eq!(fixture.owner.error(handle), None);
        assert_reconcile_stable(&mut fixture, handle);
    }

    let shared = fixture
        .images
        .get(image_handle)
        .unwrap()
        .share_with_retention_owner(Arc::new(()));
    fixture
        .images
        .commit_loaded(
            image_handle,
            shared,
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
    let published = ImageContentSnapshot::capture(
        image_handle,
        &fixture.server,
        &fixture.states,
        &fixture.images,
    )
    .unwrap();
    assert!(published.same_allocation(image));
    assert_ne!(&published, image);
    let events = fixture.events.len();
    fixture.reconcile();
    assert_ne!(fixture.tilesets.slot_revision(handle), slot);
    assert_ne!(fixture.states.version(handle.id()), version);
    assert_eq!(fixture.events.len(), events + 1);
    assert_eq!(fixture.owner.retention(), retention);
    assert_eq!(
        fixture.tilesets.get(handle).unwrap().image_content(),
        Some(&published)
    );
    assert_reconcile_stable(&mut fixture, handle);
}

#[test]
fn source_appearance_hash_edits_and_rejection_recovery_publish_with_unchanged_image() {
    let mut fixture = Fixture::new(TileSetAssetLimits::default());
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let image = fixture
        .tilesets
        .get(handle)
        .unwrap()
        .image_content()
        .unwrap()
        .clone();
    let changed = source("a.png")
        .with_sampler(nara_material::SamplerDescriptor::NEAREST_CLAMP)
        .with_alpha_mode(nara_material::AlphaMode2d::Opaque)
        .with_tint(nara_core::Color::rgb(0.25, 0.5, 0.75));
    let changed_hash = SourceHash::from_bytes(b"appearance source edit");
    for (desired, source_hash) in [
        (changed.clone(), SourceHash::from_bytes(b"a.png")),
        (changed.clone(), changed_hash),
    ] {
        let slot = fixture.tilesets.slot_revision(handle);
        let version = fixture.states.version(handle.id());
        fixture
            .owner
            .request(
                fixture.tileset.clone(),
                desired.clone(),
                source_hash,
                &mut fixture.server,
                &fixture.tilesets,
                &mut fixture.states,
            )
            .unwrap();
        let events = fixture.events.len();
        fixture.reconcile();
        let current = fixture.tilesets.get(handle).unwrap();
        assert_eq!(current.material().sampler, desired.sampler);
        assert_eq!(current.material().alpha_mode, desired.alpha_mode);
        assert_eq!(current.material().tint, desired.tint);
        assert_eq!(current.image_content(), Some(&image));
        assert_ne!(fixture.tilesets.slot_revision(handle), slot);
        assert_ne!(fixture.states.version(handle.id()), version);
        assert_eq!(fixture.events.len(), events + 1);
        let state = fixture.states.state(handle.id()).unwrap();
        assert_eq!(state.load_state(), &LoadState::Loaded);
        assert_eq!(state.source_hash(), Some(source_hash));
        assert_eq!(fixture.owner.error(handle), None);
        assert_reconcile_stable(&mut fixture, handle);
    }

    fixture
        .owner
        .reject_source(
            handle,
            &mut fixture.tilesets,
            &mut fixture.states,
            &mut fixture.events,
        )
        .unwrap();
    assert!(matches!(
        fixture.states.state(handle.id()).unwrap().load_state(),
        LoadState::Failed { .. }
    ));
    let slot = fixture.tilesets.slot_revision(handle);
    let version = fixture.states.version(handle.id());
    fixture
        .owner
        .request(
            fixture.tileset.clone(),
            changed,
            changed_hash,
            &mut fixture.server,
            &fixture.tilesets,
            &mut fixture.states,
        )
        .unwrap();
    assert_eq!(
        fixture.states.state(handle.id()).unwrap().load_state(),
        &LoadState::Loading
    );
    let events = fixture.events.len();
    fixture.reconcile();
    assert_eq!(
        fixture.tilesets.get(handle).unwrap().image_content(),
        Some(&image)
    );
    assert_ne!(fixture.tilesets.slot_revision(handle), slot);
    assert_ne!(fixture.states.version(handle.id()), version);
    assert_eq!(fixture.events.len(), events + 1);
    assert_eq!(
        fixture.states.state(handle.id()).unwrap().load_state(),
        &LoadState::Loaded
    );
    assert_eq!(fixture.owner.error(handle), None);
    assert_reconcile_stable(&mut fixture, handle);
}

#[test]
fn source_publication_requires_a_database_and_recovery_rechecks_unchanged_images() {
    let mut fixture = Fixture::new(TileSetAssetLimits::default());
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let old = fixture
        .tilesets
        .get(handle)
        .unwrap()
        .image_content()
        .unwrap()
        .clone();
    fixture.owner.reconcile(
        &mut fixture.server,
        None,
        &fixture.images,
        &mut fixture.tilesets,
        &mut fixture.states,
        &mut fixture.events,
    );
    assert!(fixture.tilesets.get(handle).is_none());
    assert_eq!(
        fixture.owner.error(handle),
        Some(&TileSetAssetError::IdentityMismatch)
    );
    let state = fixture.states.state_revision(handle.id());
    let slot = fixture.tilesets.slot_revision(handle);
    let events = fixture.events.len();
    for _ in 0..3 {
        fixture.owner.reconcile(
            &mut fixture.server,
            None,
            &fixture.images,
            &mut fixture.tilesets,
            &mut fixture.states,
            &mut fixture.events,
        );
        assert_eq!(fixture.states.state_revision(handle.id()), state);
        assert_eq!(fixture.tilesets.slot_revision(handle), slot);
        assert_eq!(fixture.events.len(), events);
    }
    fixture.reconcile();
    assert_eq!(fixture.owner.error(handle), None);
    assert!(
        fixture
            .tilesets
            .get(handle)
            .unwrap()
            .image_content()
            .unwrap()
            .same_allocation(&old)
    );
    assert_reconcile_stable(&mut fixture, handle);
}

#[test]
fn source_preflight_rejects_missing_database_and_missing_or_wrong_provenance() {
    for (database_present, wrong_path) in [(false, false), (true, false), (true, true)] {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let handle = fixture
            .server
            .reserve_record::<TileSet>(&fixture.tileset)
            .unwrap();
        let image_handle = fixture
            .server
            .reserve_record::<ImageAsset>(&fixture.image_a)
            .unwrap();
        let image = if wrong_path {
            image(
                &AssetRecord::new(
                    fixture.image_a.stable_id(),
                    AssetPath::new("other.png").unwrap(),
                    AssetSourceKind::Image,
                ),
                2,
                1,
                30,
            )
        } else {
            source_less_image(&fixture.image_a)
        };
        let input = TileSetPublicationInput::from_source(
            handle,
            fixture.tileset.clone(),
            source("a.png"),
            SourceHash::from_bytes(b"prepared"),
            image_handle,
            image,
        );
        let state = fixture.states.state_revision(handle.id());
        let slot = fixture.tilesets.slot_revision(handle);
        let events = fixture.events.len();
        assert!(matches!(
            fixture.owner.prepare_publications(
                vec![input],
                &[],
                &fixture.server,
                database_present.then_some(&fixture.database),
                &fixture.tilesets,
                &fixture.states,
            ),
            Err(TileSetAssetError::IdentityMismatch)
        ));
        assert!(fixture.owner.is_empty());
        assert_eq!(fixture.states.state_revision(handle.id()), state);
        assert_eq!(fixture.tilesets.slot_revision(handle), slot);
        assert_eq!(fixture.events.len(), events);
    }
}
