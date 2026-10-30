use super::*;
use nara_ecs::{World, bevy_ecs::system::SystemState};
use nara_image::ImageColorSpace;
use nara_material::{AlphaMode2d, SamplerDescriptor};

fn auxiliary() -> ImportedProductId {
    ImportedProductId::parse_subproduct("20000000-0000-4000-8000-000000000003").unwrap()
}

fn product(source: &str, selection: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(source).unwrap(), selection).unwrap()
}

fn processed_image(product: AssetProductRef, extent: ImageExtent, byte: u8) -> ImageAsset {
    let mut image = ImageAsset::from_product(
        product,
        extent,
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        vec![byte; extent.width as usize * extent.height as usize * 4],
    )
    .unwrap();
    assert!(image.try_attach_retention_owner(Arc::new(())));
    image
}

struct ProcessedFixture {
    server: AssetServer,
    images: Assets<ImageAsset>,
    tilesets: Assets<TileSet>,
    states: AssetStates,
    events: AssetEvents,
    owner: TileSetAssets,
    descriptor: TileSetDescriptor,
    handle: Handle<TileSet>,
    image_handle: Handle<ImageAsset>,
}

impl ProcessedFixture {
    fn new(own_selection: ImportedProductId, image_selection: ImportedProductId) -> Self {
        let descriptor = TileSetDescriptor::new(
            product("20000000-0000-4000-8000-000000000001", own_selection),
            product("20000000-0000-4000-8000-000000000002", image_selection),
            TileAtlasLayout::grid(Vec2::ONE, 2, 1),
        )
        .unwrap()
        .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
        .with_alpha_mode(AlphaMode2d::Opaque)
        .with_tint(nara_core::Color::rgb(0.25, 0.5, 0.75))
        .unwrap();
        let mut server = AssetServer::new();
        let handle = server
            .reserve_product::<TileSet>(descriptor.product())
            .unwrap();
        let image_handle = server
            .reserve_product::<ImageAsset>(descriptor.image_product())
            .unwrap();
        Self {
            server,
            images: Assets::default(),
            tilesets: Assets::default(),
            states: AssetStates::default(),
            events: AssetEvents::default(),
            owner: TileSetAssets::default(),
            descriptor,
            handle,
            image_handle,
        }
    }

    fn input(&self, descriptor: TileSetDescriptor, image: &ImageAsset) -> TileSetPublicationInput {
        TileSetPublicationInput::from_product(
            self.handle,
            descriptor,
            self.image_handle,
            image.share_retained().unwrap(),
        )
    }

    fn publish(
        &mut self,
        descriptor: TileSetDescriptor,
        image: ImageAsset,
        receipt: Option<&TileSetContentReceipt>,
    ) -> TileSetContentReceipt {
        let input = self.input(descriptor, &image);
        let input = match receipt {
            Some(receipt) => input.with_receipt(receipt.clone()),
            None => input,
        };
        let prepared = self
            .owner
            .prepare_publications(
                vec![input],
                &[],
                &self.server,
                None,
                &self.tilesets,
                &self.states,
            )
            .unwrap();
        self.images
            .commit_loaded(
                self.image_handle,
                image,
                &mut self.states,
                &mut self.events,
                None,
                None,
            )
            .unwrap();
        let mut receipts = prepared.commit(
            &self.server,
            &self.images,
            &mut self.tilesets,
            &mut self.states,
            &mut self.events,
        );
        assert_eq!(receipts.len(), 1);
        receipts.pop().unwrap()
    }

    fn install(&mut self) -> TileSetContentReceipt {
        self.publish(
            self.descriptor,
            processed_image(self.descriptor.image_product(), ImageExtent::new(2, 1), 10),
            None,
        )
    }

    fn reconcile(&mut self) {
        self.owner.reconcile(
            &mut self.server,
            None,
            &self.images,
            &mut self.tilesets,
            &mut self.states,
            &mut self.events,
        );
    }

    fn assert_stable(&mut self) {
        let server = self.server.clone();
        let slot = self.tilesets.slot_revision(self.handle);
        let state = self.states.state_revision(self.handle.id());
        let version = self.states.version(self.handle.id());
        let events = self.events.len();
        let retention = self.owner.retention();
        for _ in 0..3 {
            self.reconcile();
            assert_eq!(self.server, server);
            assert_eq!(self.tilesets.slot_revision(self.handle), slot);
            assert_eq!(self.states.state_revision(self.handle.id()), state);
            assert_eq!(self.states.version(self.handle.id()), version);
            assert_eq!(self.events.len(), events);
            assert_eq!(self.owner.retention(), retention);
        }
    }

    fn retire(&mut self, receipt: &TileSetContentReceipt) {
        self.owner
            .prepare_publications(
                Vec::new(),
                std::slice::from_ref(receipt),
                &self.server,
                None,
                &self.tilesets,
                &self.states,
            )
            .unwrap()
            .commit(
                &self.server,
                &self.images,
                &mut self.tilesets,
                &mut self.states,
                &mut self.events,
            );
    }
}

#[test]
fn processed_primary_and_auxiliary_content_needs_no_database_or_source_provenance() {
    for own_selection in [ImportedProductId::PRIMARY, auxiliary()] {
        for image_selection in [ImportedProductId::PRIMARY, auxiliary()] {
            let mut fixture = ProcessedFixture::new(own_selection, image_selection);
            let receipt = fixture.install();
            let tileset = fixture.tilesets.get(fixture.handle).unwrap();
            let snapshot = tileset.image_content().unwrap();
            assert_eq!(receipt.handle(), fixture.handle);
            assert_eq!(tileset.atlas(), Some(fixture.descriptor.atlas()));
            assert_eq!(tileset.material().image, Some(fixture.image_handle));
            assert_eq!(tileset.material().sampler, fixture.descriptor.sampler());
            assert_eq!(
                tileset.material().alpha_mode,
                fixture.descriptor.alpha_mode()
            );
            assert_eq!(tileset.material().tint, fixture.descriptor.tint());
            assert_eq!(snapshot.product(), fixture.descriptor.image_product());
            assert_eq!(snapshot.color_space(), ImageColorSpace::Linear);
            assert!(snapshot.source().is_none());
            assert!(std::ptr::eq(
                snapshot.pixels(),
                fixture.images.get(fixture.image_handle).unwrap().pixels()
            ));
            assert!(
                fixture
                    .states
                    .state(fixture.handle.id())
                    .unwrap()
                    .source_hash()
                    .is_none()
            );
            assert!(fixture.owner.desired(fixture.handle).is_none());
            assert_eq!(fixture.owner.sources().count(), 0);
            assert_eq!(fixture.server.path(fixture.handle.id()), None);
            assert_eq!(fixture.server.path(fixture.image_handle.id()), None);
            assert!(
                !fixture
                    .owner
                    .reject_source(
                        fixture.handle,
                        &mut fixture.tilesets,
                        &mut fixture.states,
                        &mut fixture.events,
                    )
                    .unwrap()
            );
            assert!(
                !fixture
                    .owner
                    .revoke_source(
                        fixture.handle,
                        &mut fixture.tilesets,
                        &mut fixture.states,
                        &mut fixture.events,
                    )
                    .unwrap()
            );
            fixture.assert_stable();
            fixture.retire(&receipt);
            assert!(fixture.owner.is_empty());
            assert!(fixture.tilesets.get(fixture.handle).is_none());
        }
    }
}

#[test]
fn processed_preflight_rejects_actual_product_binding_and_extent_mismatches_without_mutation() {
    #[derive(Clone, Copy)]
    enum Mismatch {
        OwnProduct,
        ImageProduct,
        ActualImageProduct,
        AnonymousImage,
        Extent,
    }
    for mismatch in [
        Mismatch::OwnProduct,
        Mismatch::ImageProduct,
        Mismatch::ActualImageProduct,
        Mismatch::AnonymousImage,
        Mismatch::Extent,
    ] {
        let mut fixture =
            ProcessedFixture::new(ImportedProductId::PRIMARY, ImportedProductId::PRIMARY);
        let mut descriptor = fixture.descriptor;
        let mut image_product = descriptor.image_product();
        let mut extent = ImageExtent::new(2, 1);
        let expected = match mismatch {
            Mismatch::OwnProduct => {
                descriptor = TileSetDescriptor::new(
                    AssetProductRef::new(descriptor.product().source(), auxiliary()).unwrap(),
                    descriptor.image_product(),
                    descriptor.atlas(),
                )
                .unwrap();
                TileSetAssetError::IdentityMismatch
            }
            Mismatch::ImageProduct => {
                descriptor = TileSetDescriptor::new(
                    descriptor.product(),
                    AssetProductRef::new(image_product.source(), auxiliary()).unwrap(),
                    descriptor.atlas(),
                )
                .unwrap();
                TileSetAssetError::IdentityMismatch
            }
            Mismatch::ActualImageProduct => {
                image_product = AssetProductRef::new(image_product.source(), auxiliary()).unwrap();
                TileSetAssetError::IdentityMismatch
            }
            Mismatch::AnonymousImage => TileSetAssetError::IdentityMismatch,
            Mismatch::Extent => {
                extent = ImageExtent::new(1, 1);
                TileSetAssetError::ImageExtentMismatch
            }
        };
        let image = if matches!(mismatch, Mismatch::AnonymousImage) {
            let mut image = ImageAsset::new(
                extent,
                ImageFormat::Rgba8,
                ImageColorSpace::Linear,
                vec![10; 8],
            )
            .unwrap();
            assert!(image.try_attach_retention_owner(Arc::new(())));
            image
        } else {
            processed_image(image_product, extent, 10)
        };
        let input = fixture.input(descriptor, &image);
        let server = fixture.server.clone();
        assert!(matches!(fixture.owner.prepare_publications(
            vec![input], &[], &fixture.server, None, &fixture.tilesets, &fixture.states,
        ), Err(error) if error == expected));
        assert_eq!(fixture.server, server);
        assert!(fixture.owner.is_empty());
        assert!(fixture.images.is_empty());
        assert!(fixture.tilesets.is_empty());
        assert!(fixture.states.is_empty());
        assert!(fixture.events.is_empty());
        assert_eq!(fixture.owner.retention().images, 0);
    }
}

#[test]
fn processed_preflight_requires_both_rust_type_claims() {
    for wrong_own_type in [false, true] {
        let mut fixture =
            ProcessedFixture::new(ImportedProductId::PRIMARY, ImportedProductId::PRIMARY);
        // Take handles from a correctly typed staged catalogue, then present a conflicting sibling.
        let mut server = AssetServer::new();
        let own_id = server
            .reserve_record_id(&record(
                "20000000-0000-4000-8000-000000000001",
                "terrain.tileset.ron",
                crate::tileset_asset_source_kind(),
            ))
            .unwrap();
        let image_id = server
            .reserve_record_id(&record(
                "20000000-0000-4000-8000-000000000002",
                "terrain.png",
                AssetSourceKind::Image,
            ))
            .unwrap();
        let mut typed = server.clone();
        fixture.handle = typed.claim::<TileSet>(own_id).unwrap();
        fixture.image_handle = typed.claim::<ImageAsset>(image_id).unwrap();
        if wrong_own_type {
            server.claim::<String>(own_id).unwrap();
            server.claim::<ImageAsset>(image_id).unwrap();
        } else {
            server.claim::<TileSet>(own_id).unwrap();
            server.claim::<String>(image_id).unwrap();
        }
        fixture.server = server;
        let image = processed_image(
            fixture.descriptor.image_product(),
            ImageExtent::new(2, 1),
            10,
        );
        let input = fixture.input(fixture.descriptor, &image);
        let server = fixture.server.clone();
        assert!(matches!(
            fixture.owner.prepare_publications(
                vec![input],
                &[],
                &fixture.server,
                None,
                &fixture.tilesets,
                &fixture.states,
            ),
            Err(TileSetAssetError::IdentityMismatch)
        ));
        assert_eq!(fixture.server, server);
        assert!(fixture.owner.is_empty());
        assert!(fixture.states.is_empty());
    }
}

#[test]
fn processed_dependency_status_is_quiet_but_image_and_appearance_publications_advance() {
    let mut fixture = ProcessedFixture::new(auxiliary(), auxiliary());
    let receipt = fixture.install();
    let original = fixture.tilesets.get(fixture.handle).unwrap().clone();
    let image = original.image_content().unwrap();
    let slot = fixture.tilesets.slot_revision(fixture.handle);
    let state = fixture.states.state_revision(fixture.handle.id());
    let version = fixture.states.version(fixture.handle.id());
    let retention = fixture.owner.retention();

    for failed in [false, true] {
        if failed {
            fixture
                .images
                .record_reload_failure(
                    fixture.image_handle,
                    &mut fixture.states,
                    &mut fixture.events,
                    "successor decode failed",
                )
                .unwrap();
        } else {
            fixture.states.set_loading(fixture.image_handle.id());
        }
        let events = fixture.events.len();
        fixture.reconcile();
        assert_eq!(fixture.tilesets.get(fixture.handle), Some(&original));
        assert_eq!(fixture.tilesets.slot_revision(fixture.handle), slot);
        assert_eq!(fixture.states.state_revision(fixture.handle.id()), state);
        assert_eq!(fixture.states.version(fixture.handle.id()), version);
        assert_eq!(fixture.events.len(), events);
        assert_eq!(fixture.owner.retention(), retention);
        assert_eq!(fixture.owner.error(fixture.handle), None);
        fixture.assert_stable();
    }

    // A new publication of the exact same pixel allocation still changes its snapshot identity.
    let shared = fixture
        .images
        .get(fixture.image_handle)
        .unwrap()
        .share_retained()
        .unwrap();
    fixture
        .images
        .commit_loaded(
            fixture.image_handle,
            shared,
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
    let published = ImageContentSnapshot::capture(
        fixture.image_handle,
        &fixture.server,
        &fixture.states,
        &fixture.images,
    )
    .unwrap();
    assert!(published.same_allocation(image));
    assert_ne!(&published, image);
    let events = fixture.events.len();
    fixture.reconcile();
    assert_ne!(fixture.tilesets.slot_revision(fixture.handle), slot);
    assert_ne!(fixture.states.version(fixture.handle.id()), version);
    assert_eq!(fixture.events.len(), events + 1);
    assert_eq!(fixture.owner.retention(), retention);
    assert_eq!(
        fixture
            .tilesets
            .get(fixture.handle)
            .unwrap()
            .image_content(),
        Some(&published)
    );
    fixture.assert_stable();

    let changed = fixture
        .descriptor
        .with_sampler(SamplerDescriptor::LINEAR_CLAMP)
        .with_alpha_mode(AlphaMode2d::Blend)
        .with_tint(nara_core::Color::WHITE)
        .unwrap();
    let image = fixture.images.get(fixture.image_handle).unwrap();
    let input = fixture.input(changed, image).with_receipt(receipt);
    let image_slot = fixture.images.slot_revision(fixture.image_handle);
    let image_state = fixture.states.state_revision(fixture.image_handle.id());
    let slot = fixture.tilesets.slot_revision(fixture.handle);
    let version = fixture.states.version(fixture.handle.id());
    let events = fixture.events.len();
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
    let current = fixture.tilesets.get(fixture.handle).unwrap();
    assert_eq!(current.material().sampler, changed.sampler());
    assert_eq!(current.material().alpha_mode, changed.alpha_mode());
    assert_eq!(current.material().tint, changed.tint());
    assert_eq!(current.image_content(), Some(&published));
    assert_eq!(
        fixture.images.slot_revision(fixture.image_handle),
        image_slot
    );
    assert_eq!(
        fixture.states.state_revision(fixture.image_handle.id()),
        image_state
    );
    assert_ne!(fixture.tilesets.slot_revision(fixture.handle), slot);
    assert_ne!(fixture.states.version(fixture.handle.id()), version);
    assert_eq!(fixture.events.len(), events + 1);
    fixture.assert_stable();
}

#[test]
fn processed_last_good_survives_rejected_extent_until_revocation_then_recovers_without_churn() {
    let mut fixture = ProcessedFixture::new(auxiliary(), auxiliary());
    let receipt = fixture.install();
    let old = fixture
        .tilesets
        .get(fixture.handle)
        .unwrap()
        .image_content()
        .unwrap()
        .clone();
    let slot = fixture.tilesets.slot_revision(fixture.handle);
    let version = fixture.states.version(fixture.handle.id());
    fixture
        .images
        .commit_loaded(
            fixture.image_handle,
            processed_image(
                fixture.descriptor.image_product(),
                ImageExtent::new(4, 1),
                20,
            ),
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
    fixture.reconcile();
    assert_eq!(
        fixture.owner.error(fixture.handle),
        Some(&TileSetAssetError::ImageExtentMismatch)
    );
    assert_eq!(
        fixture
            .tilesets
            .get(fixture.handle)
            .unwrap()
            .image_content(),
        Some(&old)
    );
    assert_eq!(fixture.tilesets.slot_revision(fixture.handle), slot);
    assert_eq!(fixture.states.version(fixture.handle.id()), version);
    assert!(old.is_authorized(&fixture.server, &fixture.states, &fixture.images));
    fixture.assert_stable();

    fixture
        .images
        .remove_with_state(
            fixture.image_handle,
            &mut fixture.states,
            &mut fixture.events,
        )
        .unwrap();
    fixture.reconcile();
    assert!(fixture.tilesets.get(fixture.handle).is_none());
    assert!(!old.is_authorized(&fixture.server, &fixture.states, &fixture.images));
    fixture.assert_stable();
    fixture
        .images
        .commit_loaded(
            fixture.image_handle,
            processed_image(
                fixture.descriptor.image_product(),
                ImageExtent::new(2, 1),
                30,
            ),
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
    fixture.reconcile();
    assert_eq!(fixture.owner.error(fixture.handle), None);
    assert_eq!(
        fixture
            .tilesets
            .get(fixture.handle)
            .unwrap()
            .image_content()
            .unwrap()
            .pixels(),
        &[30; 8]
    );
    assert_eq!(old.pixels(), &[10; 8]);
    fixture.assert_stable();
    fixture.retire(&receipt);
    assert_eq!(fixture.owner.retention().images, 1);
    drop(old);
    assert_eq!(fixture.owner.retention().images, 0);
}

#[test]
fn processed_image_product_mismatch_or_unpublished_slot_revokes_last_good() {
    for raw_slot in [false, true] {
        let mut fixture = ProcessedFixture::new(auxiliary(), ImportedProductId::PRIMARY);
        fixture.install();
        let old = fixture
            .tilesets
            .get(fixture.handle)
            .unwrap()
            .image_content()
            .unwrap()
            .clone();
        let image = processed_image(
            if raw_slot {
                fixture.descriptor.image_product()
            } else {
                AssetProductRef::new(fixture.descriptor.image_product().source(), auxiliary())
                    .unwrap()
            },
            ImageExtent::new(2, 1),
            40,
        );
        if raw_slot {
            fixture.images.insert(fixture.image_handle, image);
        } else {
            fixture
                .images
                .commit_loaded(
                    fixture.image_handle,
                    image,
                    &mut fixture.states,
                    &mut fixture.events,
                    None,
                    None,
                )
                .unwrap();
        }
        fixture.reconcile();
        assert!(fixture.tilesets.get(fixture.handle).is_none());
        assert_eq!(
            fixture.owner.error(fixture.handle),
            Some(&TileSetAssetError::DependencyUnavailable)
        );
        assert!(!old.is_authorized(&fixture.server, &fixture.states, &fixture.images));
        assert_eq!(old.pixels(), &[10; 8]);
        fixture.assert_stable();
    }
}

#[test]
fn processed_layout_barrier_survives_revocation_and_needs_disjoint_receipt_retirement() {
    let mut fixture = ProcessedFixture::new(auxiliary(), auxiliary());
    let receipt = fixture.install();
    let changed = TileSetDescriptor::new(
        fixture.descriptor.product(),
        fixture.descriptor.image_product(),
        TileAtlasLayout::grid(Vec2::new(2.0, 1.0), 1, 1),
    )
    .unwrap();
    let image = processed_image(changed.image_product(), ImageExtent::new(2, 1), 50);
    for revoked in [false, true] {
        if revoked {
            fixture
                .images
                .remove_with_state(
                    fixture.image_handle,
                    &mut fixture.states,
                    &mut fixture.events,
                )
                .unwrap();
            fixture.reconcile();
            assert!(fixture.tilesets.get(fixture.handle).is_none());
        }
        let input = fixture.input(changed, &image).with_receipt(receipt.clone());
        assert!(matches!(
            fixture.owner.prepare_publications(
                vec![input],
                &[],
                &fixture.server,
                None,
                &fixture.tilesets,
                &fixture.states,
            ),
            Err(TileSetAssetError::AtlasLayoutChangeRequiresRebuild)
        ));
        assert_eq!(
            fixture.owner.entries[&fixture.handle.id()].accepted_atlas,
            Some(fixture.descriptor.atlas())
        );
    }
    let input = fixture.input(changed, &image).with_receipt(receipt.clone());
    assert!(matches!(
        fixture.owner.prepare_publications(
            vec![input],
            std::slice::from_ref(&receipt),
            &fixture.server,
            None,
            &fixture.tilesets,
            &fixture.states,
        ),
        Err(TileSetAssetError::InvalidPublicationBatch)
    ));
    fixture.retire(&receipt);
    fixture.assert_stable();
    let current = fixture.publish(changed, image, None);
    assert_eq!(
        fixture.tilesets.get(fixture.handle).unwrap().atlas(),
        Some(changed.atlas())
    );
    assert!(matches!(
        fixture.owner.prepare_publications(
            Vec::new(),
            &[receipt],
            &fixture.server,
            None,
            &fixture.tilesets,
            &fixture.states,
        ),
        Err(TileSetAssetError::Superseded)
    ));
    fixture.retire(&current);
    assert!(fixture.owner.is_empty());
}

#[test]
fn processed_receipt_tracks_declarations_and_rejects_foreign_storage_writers() {
    let mut fixture = ProcessedFixture::new(auxiliary(), auxiliary());
    let original = fixture.install();
    fixture
        .images
        .commit_loaded(
            fixture.image_handle,
            processed_image(
                fixture.descriptor.image_product(),
                ImageExtent::new(2, 1),
                60,
            ),
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
    fixture.reconcile();
    drop(
        fixture
            .owner
            .prepare_publications(
                Vec::new(),
                std::slice::from_ref(&original),
                &fixture.server,
                None,
                &fixture.tilesets,
                &fixture.states,
            )
            .unwrap(),
    );
    let current = fixture.publish(
        fixture
            .descriptor
            .with_tint(nara_core::Color::WHITE)
            .unwrap(),
        processed_image(
            fixture.descriptor.image_product(),
            ImageExtent::new(2, 1),
            70,
        ),
        Some(&original),
    );
    assert!(matches!(
        fixture.owner.prepare_publications(
            Vec::new(),
            &[original],
            &fixture.server,
            None,
            &fixture.tilesets,
            &fixture.states,
        ),
        Err(TileSetAssetError::Superseded)
    ));
    let foreign = fixture.tilesets.remove(fixture.handle).unwrap();
    fixture.tilesets.insert(fixture.handle, foreign);
    let slot = fixture.tilesets.slot_revision(fixture.handle);
    assert!(matches!(
        fixture.owner.prepare_publications(
            Vec::new(),
            &[current],
            &fixture.server,
            None,
            &fixture.tilesets,
            &fixture.states,
        ),
        Err(TileSetAssetError::Superseded)
    ));
    fixture.reconcile();
    assert_eq!(
        fixture.owner.error(fixture.handle),
        Some(&TileSetAssetError::Superseded)
    );
    assert_eq!(fixture.tilesets.slot_revision(fixture.handle), slot);
    assert!(fixture.tilesets.get(fixture.handle).is_some());
    fixture.assert_stable();
}

#[test]
fn processed_batch_shares_retention_and_preserves_original_pixel_custody_through_retirement() {
    let mut fixture = ProcessedFixture::new(ImportedProductId::PRIMARY, auxiliary());
    fixture.owner = TileSetAssets::with_limits(TileSetAssetLimits::new(
        ItemLimit::new(2).unwrap(),
        ItemLimit::ONE,
        ByteLimit::new(8).unwrap(),
    ));
    let other = TileSetDescriptor::new(
        AssetProductRef::new(fixture.descriptor.product().source(), auxiliary()).unwrap(),
        fixture.descriptor.image_product(),
        fixture.descriptor.atlas(),
    )
    .unwrap();
    let other_handle = fixture
        .server
        .reserve_product::<TileSet>(other.product())
        .unwrap();
    let custody = Arc::new(());
    let held = Arc::downgrade(&custody);
    let image = ImageAsset::from_product(
        fixture.descriptor.image_product(),
        ImageExtent::new(2, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        vec![80; 8],
    )
    .unwrap()
    .share_with_retention_owner(custody);
    let first = fixture.input(fixture.descriptor, &image);
    let second = TileSetPublicationInput::from_product(
        other_handle,
        other,
        fixture.image_handle,
        image.share_retained().unwrap(),
    );
    let prepared = fixture
        .owner
        .prepare_publications(
            vec![first, second],
            &[],
            &fixture.server,
            None,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap();
    fixture
        .images
        .commit_loaded(
            fixture.image_handle,
            image,
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
    let receipts = prepared.commit(
        &fixture.server,
        &fixture.images,
        &mut fixture.tilesets,
        &mut fixture.states,
        &mut fixture.events,
    );
    assert_eq!(
        fixture.owner.retention(),
        TileSetRetentionSnapshot {
            images: 1,
            bytes: 8
        }
    );
    let escaped = fixture
        .tilesets
        .get(other_handle)
        .unwrap()
        .image_content()
        .unwrap()
        .clone();
    fixture
        .images
        .remove_with_state(
            fixture.image_handle,
            &mut fixture.states,
            &mut fixture.events,
        )
        .unwrap();
    fixture
        .owner
        .prepare_publications(
            Vec::new(),
            &receipts,
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
    assert!(fixture.owner.is_empty());
    assert!(held.upgrade().is_some());
    assert_eq!(fixture.owner.retention().images, 1);
    drop(escaped);
    assert!(held.upgrade().is_none());
    assert_eq!(fixture.owner.retention().images, 0);
    fixture.assert_stable();
}

#[test]
fn concrete_access_reconciles_processed_content_without_a_database_and_filters_source_apis() {
    let mut fixture = ProcessedFixture::new(ImportedProductId::PRIMARY, auxiliary());
    fixture.install();
    let handle = fixture.handle;
    let source_record = AssetRecord::new(
        fixture.descriptor.product().source(),
        AssetSourcePath::new("terrain.tileset.ron").unwrap(),
        crate::tileset_asset_source_kind(),
    );
    let mut world = World::new();
    world.insert_resource(fixture.server);
    world.insert_resource(fixture.images);
    world.insert_resource(fixture.tilesets);
    world.insert_resource(fixture.states);
    world.insert_resource(fixture.events);
    world.insert_resource(fixture.owner);
    let server = world.resource::<AssetServer>().clone();
    let slot = world.resource::<Assets<TileSet>>().slot_revision(handle);
    let state = world.resource::<AssetStates>().state_revision(handle.id());
    let events = world.resource::<AssetEvents>().len();
    let mut access = SystemState::<TileSetContent>::new(&mut world);
    {
        let mut content = access.get_mut(&mut world).unwrap();
        content.reconcile();
        assert_eq!(content.error(handle), None);
        assert!(content.get(handle).is_some());
        assert_eq!(content.sources().count(), 0);
        assert_eq!(content.source_revisions().count(), 0);
        assert_eq!(
            content.reject_source(&source_record),
            Err(TileSetAssetError::Superseded)
        );
        assert!(!content.revoke_source(&source_record).unwrap());
        assert_eq!(
            content.request(
                source_record,
                source("a.png"),
                SourceHash::from_bytes(b"foreign source"),
                &nara_asset::AssetPathScope::ROOT,
            ),
            Err(TileSetAssetError::Superseded)
        );
    }
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert_eq!(world.resource::<AssetServer>(), &server);
    assert_eq!(
        world.resource::<Assets<TileSet>>().slot_revision(handle),
        slot
    );
    assert_eq!(
        world.resource::<AssetStates>().state_revision(handle.id()),
        state
    );
    assert_eq!(world.resource::<AssetEvents>().len(), events);
}
