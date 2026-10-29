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
