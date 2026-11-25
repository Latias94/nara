use super::*;

fn shared_input(
    fixture: &Fixture,
    handle: Handle<TileSet>,
    source: TileSetSource,
    claim: TileSetContentReceipt,
) -> TileSetPublicationInput {
    let image_handle = fixture
        .tilesets
        .get(handle)
        .unwrap()
        .image_content()
        .unwrap()
        .handle();
    TileSetPublicationInput::from_source(
        handle,
        fixture.tileset.clone(),
        source,
        SourceHash::from_bytes(b"a.png"),
        image_handle,
        fixture
            .images
            .get(image_handle)
            .unwrap()
            .share_with_retention_owner(Arc::new(())),
        &AssetPathScope::ROOT,
        fixture.owner.limits().source_reference_bytes(),
    )
    .unwrap()
    .with_shared_receipt(claim)
}

#[test]
fn explicit_source_share_preserves_publication_claim_and_pixel_owner_at_full_capacity() {
    let limits =
        TileSetAssetLimits::new(ItemLimit::ONE, ItemLimit::ONE, ByteLimit::new(8).unwrap());
    let mut fixture = Fixture::new(limits);
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let original_receipt = receipt(&fixture, handle);
    let original = fixture.tilesets.get(handle).unwrap().clone();
    let slot = fixture.tilesets.slot_revision(handle);
    let state = fixture.states.state_revision(handle.id());
    let version = fixture.states.version(handle.id());
    let image_handle = original.image_content().unwrap().handle();
    let image_slot = fixture.images.slot_revision(image_handle);
    let image_state = fixture.states.state_revision(image_handle.id());
    let events = fixture.events.len();
    let retention = fixture.owner.retention();
    let pixel_owner = fixture.owner.retained[0].clone();
    let claim = Arc::clone(&fixture.owner.entries[&handle.id()].claim);
    let claim_holders = Arc::strong_count(&claim);
    let observation = fixture.owner.entries[&handle.id()].observation.clone();
    let input = shared_input(&fixture, handle, source("a.png"), original_receipt.clone());
    let retained_input = input.share_retained().unwrap();
    drop(
        fixture
            .owner
            .prepare_publications(
                vec![input],
                &[],
                &fixture.server,
                Some(&fixture.database),
                &fixture.tilesets,
                &fixture.states,
            )
            .unwrap(),
    );
    assert_eq!(fixture.states.state_revision(handle.id()), state);
    assert_eq!(fixture.events.len(), events);
    assert_eq!(fixture.owner.retention(), retention);
    let prepared = fixture
        .owner
        .prepare_publications(
            vec![retained_input],
            &[],
            &fixture.server,
            Some(&fixture.database),
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap();
    let reused = prepared.commit(
        &fixture.server,
        &fixture.images,
        &mut fixture.tilesets,
        &mut fixture.states,
        &mut fixture.events,
    );
    assert_eq!(reused.len(), 1);
    assert_eq!(reused[0].handle(), handle);
    fixture.reconcile();
    assert_eq!(fixture.tilesets.get(handle), Some(&original));
    assert_eq!(fixture.tilesets.slot_revision(handle), slot);
    assert_eq!(fixture.states.state_revision(handle.id()), state);
    assert_eq!(fixture.states.version(handle.id()), version);
    assert_eq!(fixture.images.slot_revision(image_handle), image_slot);
    assert_eq!(
        fixture.states.state_revision(image_handle.id()),
        image_state
    );
    assert_eq!(fixture.events.len(), events);
    assert_eq!(fixture.owner.retention(), retention);
    assert_eq!(fixture.owner.entries[&handle.id()].observation, observation);
    assert!(Arc::ptr_eq(
        &fixture.owner.entries[&handle.id()].claim,
        &claim
    ));
    assert_eq!(Arc::strong_count(&claim), claim_holders);
    assert!(Weak::ptr_eq(&fixture.owner.retained[0], &pixel_owner));
    assert_eq!(
        fixture.owner.entries[&handle.id()].accepted_atlas,
        original.atlas()
    );
    drop(
        fixture
            .owner
            .prepare_publications(
                Vec::new(),
                &[original_receipt.clone()],
                &fixture.server,
                Some(&fixture.database),
                &fixture.tilesets,
                &fixture.states,
            )
            .unwrap(),
    );
    fixture
        .owner
        .prepare_publications(
            Vec::new(),
            &reused,
            &fixture.server,
            Some(&fixture.database),
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
    assert!(fixture.tilesets.get(handle).is_none());
    fixture.reconcile();
    assert!(fixture.tilesets.get(handle).is_none());
    assert_eq!(fixture.owner.retention(), retention);
    drop(original);
    assert_eq!(fixture.owner.retention().images, 0);
    let successor = fixture.request("a.png");
    fixture.reconcile();
    assert_eq!(successor, handle);
    for stale in [original_receipt, reused[0].clone()] {
        assert!(matches!(
            fixture.owner.prepare_publications(
                Vec::new(),
                &[stale],
                &fixture.server,
                Some(&fixture.database),
                &fixture.tilesets,
                &fixture.states,
            ),
            Err(TileSetAssetError::Superseded)
        ));
    }
}

#[test]
fn one_original_transaction_publishes_replaces_shares_and_retires_in_input_order() {
    let limits = TileSetAssetLimits::new(
        ItemLimit::new(3).unwrap(),
        ItemLimit::ONE,
        ByteLimit::new(8).unwrap(),
    );
    let mut fixture = Fixture::new(limits);
    let shared_handle = fixture.request("a.png");
    fixture.reconcile();
    let replace_record = record(
        "10000000-0000-4000-8000-000000000004",
        "replace.tileset.ron",
        crate::tilemap::tileset_asset_source_kind(),
    );
    let retire_record = record(
        "10000000-0000-4000-8000-000000000005",
        "retire.tileset.ron",
        crate::tilemap::tileset_asset_source_kind(),
    );
    let new_record = record(
        "10000000-0000-4000-8000-000000000006",
        "new.tileset.ron",
        crate::tilemap::tileset_asset_source_kind(),
    );
    let mut handles = Vec::new();
    for record in [&replace_record, &retire_record] {
        fixture.database.insert(record.clone()).unwrap();
        handles.push(
            fixture
                .owner
                .request(
                    record.clone(),
                    source("a.png"),
                    SourceHash::from_bytes(b"a.png"),
                    &AssetPathScope::ROOT,
                    &mut fixture.server,
                    &fixture.tilesets,
                    &mut fixture.states,
                )
                .unwrap(),
        );
    }
    fixture.reconcile();
    let replace_handle = handles[0];
    let retire_handle = handles[1];
    let shared_claim = receipt(&fixture, shared_handle);
    let replace_claim = receipt(&fixture, replace_handle);
    let retire_claim = receipt(&fixture, retire_handle);
    let shared = shared_input(
        &fixture,
        shared_handle,
        source("a.png"),
        shared_claim.clone(),
    );
    let image_handle = fixture
        .tilesets
        .get(shared_handle)
        .unwrap()
        .image_content()
        .unwrap()
        .handle();
    let resident = fixture
        .images
        .get(image_handle)
        .unwrap()
        .share_with_retention_owner(Arc::new(()));
    fixture.database.insert(new_record.clone()).unwrap();
    let (new_input, new_handle, _) = input(&mut fixture, new_record, source("a.png"), &resident);
    let (replace, _, _) = input(
        &mut fixture,
        replace_record,
        source("a.png").with_tint(nara_core::Color::rgb(0.2, 0.4, 0.6)),
        &resident,
    );
    let shared_slot = fixture.tilesets.slot_revision(shared_handle);
    let shared_state = fixture.states.state_revision(shared_handle.id());
    let shared_version = fixture.states.version(shared_handle.id());
    let replace_slot = fixture.tilesets.slot_revision(replace_handle);
    let replace_version = fixture.states.version(replace_handle.id());
    let image_slot = fixture.images.slot_revision(image_handle);
    let image_state = fixture.states.state_revision(image_handle.id());
    let events = fixture.events.len();
    let retention = fixture.owner.retention();
    let receipts = fixture
        .owner
        .prepare_publications(
            vec![new_input, shared, replace.with_receipt(replace_claim)],
            &[retire_claim],
            &fixture.server,
            Some(&fixture.database),
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
    assert_eq!(
        receipts
            .iter()
            .map(TileSetContentReceipt::handle)
            .collect::<Vec<_>>(),
        [new_handle, shared_handle, replace_handle]
    );
    assert_eq!(fixture.owner.len(), 3);
    assert!(fixture.tilesets.get(retire_handle).is_none());
    assert!(fixture.tilesets.get(new_handle).is_some());
    assert_ne!(fixture.tilesets.slot_revision(replace_handle), replace_slot);
    assert_ne!(fixture.states.version(replace_handle.id()), replace_version);
    assert_eq!(fixture.tilesets.slot_revision(shared_handle), shared_slot);
    assert_eq!(
        fixture.states.state_revision(shared_handle.id()),
        shared_state
    );
    assert_eq!(fixture.states.version(shared_handle.id()), shared_version);
    assert_eq!(fixture.images.slot_revision(image_handle), image_slot);
    assert_eq!(
        fixture.states.state_revision(image_handle.id()),
        image_state
    );
    assert_eq!(fixture.events.len(), events + 3);
    assert_eq!(fixture.owner.retention(), retention);
    let events = fixture.events.len();
    fixture.reconcile();
    assert_eq!(fixture.events.len(), events);
    drop(
        fixture
            .owner
            .prepare_publications(
                Vec::new(),
                &[shared_claim],
                &fixture.server,
                Some(&fixture.database),
                &fixture.tilesets,
                &fixture.states,
            )
            .unwrap(),
    );
}

#[test]
fn shared_source_refuses_changed_properties_hash_scope_and_new_image_allocation_before_effects() {
    enum Mismatch {
        Tint,
        TintBits,
        Sampler,
        Alpha,
        Atlas,
        AtlasBits,
        Hash,
        Scope,
        Image,
    }
    for mismatch in [
        Mismatch::Tint,
        Mismatch::TintBits,
        Mismatch::Sampler,
        Mismatch::Alpha,
        Mismatch::Atlas,
        Mismatch::AtlasBits,
        Mismatch::Hash,
        Mismatch::Scope,
        Mismatch::Image,
    ] {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let authored = source("a.png").with_tint(nara_core::Color::rgb(0.0, 1.0, 1.0));
        let handle = fixture
            .owner
            .request(
                fixture.tileset.clone(),
                authored.clone(),
                SourceHash::from_bytes(b"a.png"),
                &AssetPathScope::ROOT,
                &mut fixture.server,
                &fixture.tilesets,
                &mut fixture.states,
            )
            .unwrap();
        fixture.reconcile();
        let claim = receipt(&fixture, handle);
        let original = fixture.tilesets.get(handle).unwrap().clone();
        let image_handle = original.image_content().unwrap().handle();
        let mut image = fixture
            .images
            .get(image_handle)
            .unwrap()
            .share_with_retention_owner(Arc::new(()));
        let mut desired = authored.clone();
        let mut hash = SourceHash::from_bytes(b"a.png");
        let mut scope = AssetPathScope::ROOT;
        let expected = match mismatch {
            Mismatch::Tint => {
                desired.tint.g = 0.5;
                TileSetAssetError::Superseded
            }
            Mismatch::TintBits => {
                desired.tint.r = -0.0;
                assert_eq!(authored, desired);
                assert_ne!(
                    authored.encode_ron_bytes().unwrap(),
                    desired.encode_ron_bytes().unwrap()
                );
                TileSetAssetError::Superseded
            }
            Mismatch::Sampler => {
                desired.sampler = crate::material::SamplerDescriptor::NEAREST_CLAMP;
                TileSetAssetError::Superseded
            }
            Mismatch::Alpha => {
                desired.alpha_mode = crate::material::AlphaMode2d::Opaque;
                TileSetAssetError::Superseded
            }
            Mismatch::Atlas => {
                desired.atlas = TileAtlasLayout::grid(Vec2::new(2.0, 1.0), 1, 1);
                TileSetAssetError::AtlasLayoutChangeRequiresRebuild
            }
            Mismatch::AtlasBits => {
                desired.atlas.margin.x = -0.0;
                assert_eq!(authored, desired);
                TileSetAssetError::Superseded
            }
            Mismatch::Hash => {
                hash = SourceHash::from_bytes(b"changed source");
                TileSetAssetError::Superseded
            }
            Mismatch::Scope => {
                scope = AssetPathScope::under(AssetSourcePath::new("another").unwrap());
                let other = record(
                    "10000000-0000-4000-8000-000000000004",
                    "another/a.png",
                    AssetSourceKind::Image,
                );
                fixture.database.insert(other).unwrap();
                TileSetAssetError::IdentityMismatch
            }
            Mismatch::Image => {
                image = destination_image(&fixture.image_a, 10);
                TileSetAssetError::Superseded
            }
        };
        let input = TileSetPublicationInput::from_source(
            handle,
            fixture.tileset.clone(),
            desired,
            hash,
            image_handle,
            image,
            &scope,
            fixture.owner.limits().source_reference_bytes(),
        )
        .unwrap()
        .with_shared_receipt(claim.clone());
        let server = fixture.server.clone();
        let slot = fixture.tilesets.slot_revision(handle);
        let state = fixture.states.state_revision(handle.id());
        let image_slot = fixture.images.slot_revision(image_handle);
        let image_state = fixture.states.state_revision(image_handle.id());
        let events = fixture.events.len();
        let retention = fixture.owner.retention();
        assert!(matches!(
            fixture.owner.prepare_publications(
                vec![input],
                &[],
                &fixture.server,
                Some(&fixture.database),
                &fixture.tilesets,
                &fixture.states,
            ),
            Err(error) if error == expected
        ));
        assert_eq!(fixture.server, server);
        assert_eq!(fixture.tilesets.get(handle), Some(&original));
        assert_eq!(fixture.tilesets.slot_revision(handle), slot);
        assert_eq!(fixture.states.state_revision(handle.id()), state);
        assert_eq!(fixture.images.slot_revision(image_handle), image_slot);
        assert_eq!(
            fixture.states.state_revision(image_handle.id()),
            image_state
        );
        assert_eq!(fixture.events.len(), events);
        assert_eq!(fixture.owner.retention(), retention);
        assert_eq!(fixture.owner.desired(handle), Some(&authored));
        assert_eq!(fixture.owner.error(handle), None);
        drop(
            fixture
                .owner
                .prepare_publications(
                    Vec::new(),
                    &[claim],
                    &fixture.server,
                    Some(&fixture.database),
                    &fixture.tilesets,
                    &fixture.states,
                )
                .unwrap(),
        );
    }
}

#[test]
fn stale_prepared_shared_baselines_refuse_before_any_publication_or_retirement() {
    enum Drift {
        TileSetSlot,
        TileSetState,
        ImageSlot,
        ImageState,
        ImagePublication,
        ImageRevocation,
    }
    for drift in [
        Drift::TileSetSlot,
        Drift::TileSetState,
        Drift::ImageSlot,
        Drift::ImageState,
        Drift::ImagePublication,
        Drift::ImageRevocation,
    ] {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let handle = fixture.request("a.png");
        fixture.reconcile();
        let retire_record = record(
            "10000000-0000-4000-8000-000000000004",
            "retire.tileset.ron",
            crate::tilemap::tileset_asset_source_kind(),
        );
        fixture.database.insert(retire_record.clone()).unwrap();
        let retire_handle = fixture
            .owner
            .request(
                retire_record,
                source("a.png"),
                SourceHash::from_bytes(b"a.png"),
                &AssetPathScope::ROOT,
                &mut fixture.server,
                &fixture.tilesets,
                &mut fixture.states,
            )
            .unwrap();
        fixture.reconcile();
        let original = fixture.tilesets.get(handle).unwrap().clone();
        let retired = fixture.tilesets.get(retire_handle).unwrap().clone();
        let original_claim = Arc::clone(&fixture.owner.entries[&handle.id()].claim);
        let image_handle = original.image_content().unwrap().handle();
        let resident = fixture
            .images
            .get(image_handle)
            .unwrap()
            .share_with_retention_owner(Arc::new(()));
        let shared = shared_input(&fixture, handle, source("a.png"), receipt(&fixture, handle));
        let new_record = record(
            "10000000-0000-4000-8000-000000000005",
            "new.tileset.ron",
            crate::tilemap::tileset_asset_source_kind(),
        );
        fixture.database.insert(new_record.clone()).unwrap();
        fixture.database.insert(fixture.image_b.clone()).unwrap();
        let other_image_handle = fixture.publish_image(fixture.image_b.clone(), 2, 1, 20);
        let other_image = fixture
            .images
            .get(other_image_handle)
            .unwrap()
            .share_with_retention_owner(Arc::new(()));
        let (new_input, new_handle, _) =
            input(&mut fixture, new_record, source("b.png"), &other_image);
        let retire_claim = receipt(&fixture, retire_handle);
        let retirement_slot = fixture.tilesets.slot_revision(retire_handle);
        let retirement_state = fixture.states.state_revision(retire_handle.id());
        let new_slot = fixture.tilesets.slot_revision(new_handle);
        let new_state = fixture.states.state_revision(new_handle.id());
        let retention = fixture.owner.retention();
        let prepared = fixture
            .owner
            .prepare_publications(
                vec![new_input, shared],
                &[retire_claim.clone()],
                &fixture.server,
                Some(&fixture.database),
                &fixture.tilesets,
                &fixture.states,
            )
            .unwrap();
        match drift {
            Drift::TileSetSlot => {
                let removed = fixture.tilesets.remove(handle).unwrap();
                fixture.tilesets.insert(handle, removed);
            }
            Drift::TileSetState => {
                fixture.states.set_loading(handle.id());
            }
            Drift::ImageSlot => {
                let removed = fixture.images.remove(image_handle).unwrap();
                fixture.images.insert(image_handle, removed);
            }
            Drift::ImageState => {
                fixture.states.set_loading(image_handle.id());
            }
            Drift::ImagePublication => {
                fixture
                    .images
                    .commit_loaded(
                        image_handle,
                        resident,
                        &mut fixture.states,
                        &mut fixture.events,
                        None,
                        None,
                    )
                    .unwrap();
            }
            Drift::ImageRevocation => fixture.states.invalidate_publication(image_handle.id()),
        }
        let slot = fixture.tilesets.slot_revision(handle);
        let state = fixture.states.state_revision(handle.id());
        let events = fixture.events.len();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            prepared.commit(
                &fixture.server,
                &fixture.images,
                &mut fixture.tilesets,
                &mut fixture.states,
                &mut fixture.events,
            );
        }));
        assert!(result.is_err());
        assert_eq!(fixture.tilesets.get(handle), Some(&original));
        assert_eq!(fixture.tilesets.slot_revision(handle), slot);
        assert_eq!(fixture.states.state_revision(handle.id()), state);
        assert_eq!(fixture.tilesets.get(retire_handle), Some(&retired));
        assert_eq!(
            fixture.tilesets.slot_revision(retire_handle),
            retirement_slot
        );
        assert_eq!(
            fixture.states.state_revision(retire_handle.id()),
            retirement_state
        );
        assert!(fixture.tilesets.get(new_handle).is_none());
        assert_eq!(fixture.tilesets.slot_revision(new_handle), new_slot);
        assert_eq!(fixture.states.state_revision(new_handle.id()), new_state);
        assert_eq!(fixture.events.len(), events);
        assert_eq!(fixture.owner.len(), 2);
        assert_eq!(fixture.owner.retention(), retention);
        assert_eq!(fixture.owner.error(handle), None);
        assert!(Arc::ptr_eq(
            &original_claim,
            &fixture.owner.entries[&handle.id()].claim
        ));
        drop(
            fixture
                .owner
                .prepare_publications(
                    Vec::new(),
                    &[retire_claim],
                    &fixture.server,
                    Some(&fixture.database),
                    &fixture.tilesets,
                    &fixture.states,
                )
                .unwrap(),
        );
    }
}

#[test]
fn shared_input_requires_an_original_live_publication_and_disjoint_claim() {
    enum Invalid {
        Unpublished,
        Revoked,
        Rejected,
        WrongClaim,
        MissingOwner,
        Overlap,
    }
    for invalid in [
        Invalid::Unpublished,
        Invalid::Revoked,
        Invalid::Rejected,
        Invalid::WrongClaim,
        Invalid::MissingOwner,
        Invalid::Overlap,
    ] {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let handle = fixture.request("a.png");
        fixture.reconcile();
        let claim = receipt(&fixture, handle);
        let mut shared = shared_input(&fixture, handle, source("a.png"), claim.clone());
        let mut retirements = Vec::new();
        let expected = match invalid {
            Invalid::Unpublished => {
                fixture
                    .owner
                    .remove(
                        handle,
                        &mut fixture.tilesets,
                        &mut fixture.states,
                        &mut fixture.events,
                    )
                    .unwrap();
                let successor = fixture.request("a.png");
                shared = shared.with_shared_receipt(receipt(&fixture, successor));
                TileSetAssetError::DependencyUnavailable
            }
            Invalid::Revoked => {
                fixture
                    .owner
                    .revoke_source(
                        handle,
                        &mut fixture.tilesets,
                        &mut fixture.states,
                        &mut fixture.events,
                    )
                    .unwrap();
                TileSetAssetError::DependencyUnavailable
            }
            Invalid::Rejected => {
                fixture
                    .owner
                    .reject_source(
                        handle,
                        &mut fixture.tilesets,
                        &mut fixture.states,
                        &mut fixture.events,
                    )
                    .unwrap();
                TileSetAssetError::Superseded
            }
            Invalid::WrongClaim => {
                fixture.request("b.png");
                shared = shared.with_shared_receipt(receipt(&fixture, handle));
                TileSetAssetError::Superseded
            }
            Invalid::MissingOwner => {
                fixture.owner = TileSetAssets::default();
                TileSetAssetError::Superseded
            }
            Invalid::Overlap => {
                retirements.push(claim);
                TileSetAssetError::InvalidPublicationBatch
            }
        };
        let slot = fixture.tilesets.slot_revision(handle);
        let state = fixture.states.state_revision(handle.id());
        let events = fixture.events.len();
        let retention = fixture.owner.retention();
        assert!(matches!(
            fixture.owner.prepare_publications(
                vec![shared],
                &retirements,
                &fixture.server,
                Some(&fixture.database),
                &fixture.tilesets,
                &fixture.states,
            ),
            Err(error) if error == expected
        ));
        assert_eq!(fixture.tilesets.slot_revision(handle), slot);
        assert_eq!(fixture.states.state_revision(handle.id()), state);
        assert_eq!(fixture.events.len(), events);
        assert_eq!(fixture.owner.retention(), retention);
    }
}
