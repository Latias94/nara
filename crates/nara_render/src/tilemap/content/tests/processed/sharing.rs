use super::*;

#[test]
fn explicit_processed_share_accepts_equal_independent_descriptor_without_republication() {
    for own_selection in [ImportedProductId::PRIMARY, auxiliary()] {
        for image_selection in [ImportedProductId::PRIMARY, auxiliary()] {
            let mut fixture = ProcessedFixture::new(own_selection, image_selection);
            fixture.owner = TileSetAssets::with_limits(TileSetAssetLimits::new(
                ItemLimit::ONE,
                ItemLimit::ONE,
                ByteLimit::new(8).unwrap(),
            ));
            let original_receipt = fixture.install();
            let descriptor = TileSetDescriptor::new(
                fixture.descriptor.product(),
                fixture.descriptor.image_product(),
                TileAtlasLayout::grid(Vec2::ONE, 2, 1),
            )
            .unwrap()
            .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
            .with_alpha_mode(AlphaMode2d::Opaque)
            .with_tint(nara_core::Color::rgb(0.25, 0.5, 0.75))
            .unwrap();
            let input = fixture
                .input(
                    descriptor,
                    fixture.images.get(fixture.image_handle).unwrap(),
                )
                .with_shared_receipt(original_receipt.clone());
            let original = fixture.tilesets.get(fixture.handle).unwrap().clone();
            let slot = fixture.tilesets.slot_revision(fixture.handle);
            let state = fixture.states.state_revision(fixture.handle.id());
            let version = fixture.states.version(fixture.handle.id());
            let image_slot = fixture.images.slot_revision(fixture.image_handle);
            let image_state = fixture.states.state_revision(fixture.image_handle.id());
            let events = fixture.events.len();
            let retention = fixture.owner.retention();
            let claim = Arc::clone(&fixture.owner.entries[&fixture.handle.id()].claim);
            let pixel_owner = fixture.owner.retained[0].clone();
            let receipts = fixture
                .owner
                .prepare_publications(
                    vec![input],
                    &[],
                    &fixture.server,
                    None,
                    &fixture.tilesets,
                    &fixture.states,
                )
                .unwrap()
                .commit(
                    &fixture.server,
                    &fixture.images,
                    &mut fixture.tilesets,
                    &mut fixture.states,
                    &mut fixture.events,
                );
            assert_eq!(receipts.len(), 1);
            assert_eq!(receipts[0].handle(), fixture.handle);
            assert_eq!(fixture.tilesets.get(fixture.handle), Some(&original));
            assert_eq!(fixture.tilesets.slot_revision(fixture.handle), slot);
            assert_eq!(fixture.states.state_revision(fixture.handle.id()), state);
            assert_eq!(fixture.states.version(fixture.handle.id()), version);
            assert_eq!(
                fixture.images.slot_revision(fixture.image_handle),
                image_slot
            );
            assert_eq!(
                fixture.states.state_revision(fixture.image_handle.id()),
                image_state
            );
            assert_eq!(fixture.events.len(), events);
            assert_eq!(fixture.owner.retention(), retention);
            assert!(Arc::ptr_eq(
                &claim,
                &fixture.owner.entries[&fixture.handle.id()].claim
            ));
            assert!(Weak::ptr_eq(&pixel_owner, &fixture.owner.retained[0]));
            fixture.assert_stable();
            fixture.retire(&original_receipt);
            assert!(fixture.owner.is_empty());
            assert!(fixture.tilesets.get(fixture.handle).is_none());
            drop(original);
            let successor = fixture.install();
            for stale in [original_receipt, receipts[0].clone()] {
                assert!(matches!(
                    fixture.owner.prepare_publications(
                        Vec::new(),
                        &[stale],
                        &fixture.server,
                        None,
                        &fixture.tilesets,
                        &fixture.states,
                    ),
                    Err(TileSetAssetError::Superseded)
                ));
            }
            fixture.retire(&successor);
        }
    }
}

#[test]
fn shared_processed_refuses_changed_descriptor_or_actual_image_before_effects() {
    enum Mismatch {
        Tint,
        TintBits,
        Sampler,
        Alpha,
        Atlas,
        ImageProduct,
        ImagePixels,
        Transfer,
        Provenance,
        Extent,
        ImageHandle,
    }
    for mismatch in [
        Mismatch::Tint,
        Mismatch::TintBits,
        Mismatch::Sampler,
        Mismatch::Alpha,
        Mismatch::Atlas,
        Mismatch::ImageProduct,
        Mismatch::ImagePixels,
        Mismatch::Transfer,
        Mismatch::Provenance,
        Mismatch::Extent,
        Mismatch::ImageHandle,
    ] {
        let image_selection = if matches!(mismatch, Mismatch::Provenance) {
            ImportedProductId::PRIMARY
        } else {
            auxiliary()
        };
        let mut fixture = ProcessedFixture::new(auxiliary(), image_selection);
        fixture.descriptor = fixture
            .descriptor
            .with_tint(nara_core::Color::rgb(0.0, 0.5, 0.75))
            .unwrap();
        let claim = fixture.install();
        let mut descriptor = fixture.descriptor;
        let mut image = fixture
            .images
            .get(fixture.image_handle)
            .unwrap()
            .share_retained()
            .unwrap();
        let mut image_handle = fixture.image_handle;
        let expected = match mismatch {
            Mismatch::Tint => {
                descriptor = descriptor.with_tint(nara_core::Color::WHITE).unwrap();
                TileSetAssetError::Superseded
            }
            Mismatch::TintBits => {
                descriptor = descriptor
                    .with_tint(nara_core::Color::rgb(-0.0, 0.5, 0.75))
                    .unwrap();
                assert_eq!(descriptor, fixture.descriptor);
                TileSetAssetError::Superseded
            }
            Mismatch::Sampler => {
                descriptor = descriptor.with_sampler(SamplerDescriptor::LINEAR_CLAMP);
                TileSetAssetError::Superseded
            }
            Mismatch::Alpha => {
                descriptor = descriptor.with_alpha_mode(AlphaMode2d::Blend);
                TileSetAssetError::Superseded
            }
            Mismatch::Atlas => {
                descriptor = TileSetDescriptor::new(
                    descriptor.product(),
                    descriptor.image_product(),
                    TileAtlasLayout::grid(Vec2::new(2.0, 1.0), 1, 1),
                )
                .unwrap();
                TileSetAssetError::AtlasLayoutChangeRequiresRebuild
            }
            Mismatch::ImageProduct => {
                image = processed_image(
                    product("20000000-0000-4000-8000-000000000099", auxiliary()),
                    ImageExtent::new(2, 1),
                    10,
                );
                TileSetAssetError::IdentityMismatch
            }
            Mismatch::ImagePixels => {
                image = processed_image(descriptor.image_product(), ImageExtent::new(2, 1), 10);
                TileSetAssetError::Superseded
            }
            Mismatch::Transfer => {
                image = ImageAsset::from_product(
                    descriptor.image_product(),
                    ImageExtent::new(2, 1),
                    ImageFormat::Rgba8,
                    ImageColorSpace::Srgb,
                    vec![10; 8],
                )
                .unwrap();
                TileSetAssetError::Superseded
            }
            Mismatch::Provenance => {
                let record = record(
                    "20000000-0000-4000-8000-000000000002",
                    "image.png",
                    AssetSourceKind::Image,
                );
                image = super::super::image(&record, 2, 1, 10);
                image_handle = fixture
                    .server
                    .reserve_record::<ImageAsset>(&record)
                    .unwrap();
                assert_eq!(image_handle, fixture.image_handle);
                TileSetAssetError::Superseded
            }
            Mismatch::Extent => {
                image = processed_image(descriptor.image_product(), ImageExtent::new(4, 1), 10);
                TileSetAssetError::ImageExtentMismatch
            }
            Mismatch::ImageHandle => {
                let other = product("20000000-0000-4000-8000-000000000099", auxiliary());
                image_handle = fixture.server.reserve_product::<ImageAsset>(other).unwrap();
                TileSetAssetError::IdentityMismatch
            }
        };
        let input =
            TileSetPublicationInput::from_product(fixture.handle, descriptor, image_handle, image)
                .with_shared_receipt(claim.clone());
        let original = fixture.tilesets.get(fixture.handle).unwrap().clone();
        let slot = fixture.tilesets.slot_revision(fixture.handle);
        let state = fixture.states.state_revision(fixture.handle.id());
        let image_slot = fixture.images.slot_revision(fixture.image_handle);
        let image_state = fixture.states.state_revision(fixture.image_handle.id());
        let events = fixture.events.len();
        let retention = fixture.owner.retention();
        assert!(matches!(
            fixture.owner.prepare_publications(
                vec![input],
                &[],
                &fixture.server,
                None,
                &fixture.tilesets,
                &fixture.states,
            ),
            Err(error) if error == expected
        ));
        assert_eq!(fixture.tilesets.get(fixture.handle), Some(&original));
        assert_eq!(fixture.tilesets.slot_revision(fixture.handle), slot);
        assert_eq!(fixture.states.state_revision(fixture.handle.id()), state);
        assert_eq!(
            fixture.images.slot_revision(fixture.image_handle),
            image_slot
        );
        assert_eq!(
            fixture.states.state_revision(fixture.image_handle.id()),
            image_state
        );
        assert_eq!(fixture.events.len(), events);
        assert_eq!(fixture.owner.retention(), retention);
        assert_eq!(fixture.owner.error(fixture.handle), None);

        if matches!(mismatch, Mismatch::Tint | Mismatch::TintBits) {
            let id = fixture.handle.id();
            let version = fixture.states.version(id).unwrap().raw();
            let input = fixture
                .input(
                    descriptor,
                    fixture.images.get(fixture.image_handle).unwrap(),
                )
                .with_receipt(claim.clone());
            let replacements = fixture
                .owner
                .prepare_publications(
                    vec![input],
                    &[],
                    &fixture.server,
                    None,
                    &fixture.tilesets,
                    &fixture.states,
                )
                .unwrap()
                .commit(
                    &fixture.server,
                    &fixture.images,
                    &mut fixture.tilesets,
                    &mut fixture.states,
                    &mut fixture.events,
                );
            assert_eq!(fixture.states.version(id).unwrap().raw(), version + 1);
            assert!(matches!(
                fixture.owner.prepare_publications(
                    Vec::new(),
                    &[claim],
                    &fixture.server,
                    None,
                    &fixture.tilesets,
                    &fixture.states,
                ),
                Err(TileSetAssetError::Superseded)
            ));
            let current = fixture.tilesets.get(fixture.handle).unwrap();
            assert_eq!(
                current.material().tint.r.to_bits(),
                descriptor.tint().r.to_bits()
            );
            fixture.retire(&replacements[0]);
        } else {
            fixture.retire(&claim);
        }
    }
}

#[test]
fn receipt_mode_explicitly_selects_replacement_or_sharing_for_identical_content() {
    let mut fixture = ProcessedFixture::new(auxiliary(), auxiliary());
    let original_receipt = fixture.install();
    let image = fixture
        .images
        .get(fixture.image_handle)
        .unwrap()
        .share_retained()
        .unwrap();
    let slot = fixture.tilesets.slot_revision(fixture.handle);
    let version = fixture.states.version(fixture.handle.id());
    let events = fixture.events.len();
    let input = fixture
        .input(fixture.descriptor, &image)
        .with_shared_receipt(original_receipt.clone())
        .with_receipt(original_receipt.clone());
    let receipts = fixture
        .owner
        .prepare_publications(
            vec![input],
            &[],
            &fixture.server,
            None,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap()
        .commit(
            &fixture.server,
            &fixture.images,
            &mut fixture.tilesets,
            &mut fixture.states,
            &mut fixture.events,
        );
    assert_ne!(fixture.tilesets.slot_revision(fixture.handle), slot);
    assert_ne!(fixture.states.version(fixture.handle.id()), version);
    assert_eq!(fixture.events.len(), events + 1);
    let slot = fixture.tilesets.slot_revision(fixture.handle);
    let version = fixture.states.version(fixture.handle.id());
    let events = fixture.events.len();
    let input = fixture
        .input(fixture.descriptor, &image)
        .with_receipt(receipts[0].clone())
        .with_shared_receipt(original_receipt.clone());
    fixture
        .owner
        .prepare_publications(
            vec![input],
            &[],
            &fixture.server,
            None,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap()
        .commit(
            &fixture.server,
            &fixture.images,
            &mut fixture.tilesets,
            &mut fixture.states,
            &mut fixture.events,
        );
    assert_eq!(fixture.tilesets.slot_revision(fixture.handle), slot);
    assert_eq!(fixture.states.version(fixture.handle.id()), version);
    assert_eq!(fixture.events.len(), events);
    fixture.retire(&original_receipt);
}
