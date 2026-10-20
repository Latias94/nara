use super::*;
use crate::__private::{TileSetOwnedSourceReceipt, TileSetPublicationInput};

fn destination_image(record: &AssetRecord, byte: u8) -> ImageAsset {
    let mut image = image(record, 2, 1, byte);
    assert!(image.try_attach_retention_owner(Arc::new(())));
    image
}

fn input(
    fixture: &mut Fixture,
    record: AssetRecord,
    source: TileSetSource,
    image: &ImageAsset,
) -> (TileSetPublicationInput, Handle<TileSet>, Handle<ImageAsset>) {
    let image_record = fixture.database.resolve_ref(&source.image).unwrap();
    let image_handle = fixture
        .server
        .reserve_record::<ImageAsset>(image_record)
        .unwrap();
    let handle = fixture.server.reserve_record::<TileSet>(&record).unwrap();
    (
        TileSetPublicationInput::new(
            handle,
            record,
            source,
            SourceHash::from_bytes(b"prepared"),
            image_handle,
            image.share_retained().unwrap(),
        ),
        handle,
        image_handle,
    )
}

fn terrain_input(
    fixture: &mut Fixture,
    source: TileSetSource,
    image: &ImageAsset,
) -> (TileSetPublicationInput, Handle<TileSet>, Handle<ImageAsset>) {
    let record = fixture.tileset.clone();
    input(fixture, record, source, image)
}

fn receipt(fixture: &Fixture, handle: Handle<TileSet>) -> TileSetOwnedSourceReceipt {
    fixture
        .owner
        .owned_source_receipt(handle, &fixture.tilesets, &fixture.states)
        .unwrap()
}

#[test]
fn dropped_plan_changes_no_desired_source_diagnostic_publication_or_retention() {
    let mut fixture = Fixture::new(TileSetAssetLimits::default());
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let claim = receipt(&fixture, handle);
    let old = fixture.tilesets.get(handle).unwrap().clone();
    let slot = fixture.tilesets.slot_revision(handle);
    let state = fixture.states.state(handle.id()).unwrap().clone();
    let state_revision = fixture.states.state_revision(handle.id());
    let retention = fixture.owner.retention();
    let event_count = fixture.events.len();
    let image = destination_image(&fixture.image_a, 30);
    let allocation = image.pixels().as_ptr();
    let desired = source("a.png").with_tint(nara_core::Color::rgb(1.0, 0.0, 0.0));
    let (input, _, _) = terrain_input(&mut fixture, desired, &image);
    let input = input.with_owned_source(claim.clone());
    let prepared = fixture
        .owner
        .prepare_publications(
            vec![input],
            &[],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap();
    drop(prepared);
    assert_eq!(image.pixels().as_ptr(), allocation);
    assert_eq!(fixture.owner.desired(handle), Some(&source("a.png")));
    assert_eq!(fixture.owner.error(handle), None);
    assert_eq!(fixture.tilesets.get(handle), Some(&old));
    assert_eq!(fixture.tilesets.slot_revision(handle), slot);
    assert_eq!(fixture.states.state(handle.id()), Some(&state));
    assert_eq!(fixture.states.state_revision(handle.id()), state_revision);
    assert_eq!(fixture.events.len(), event_count);
    assert_eq!(fixture.owner.retention(), retention);
    // A dropped plan did not consume or rotate the original source claim either.
    drop(
        fixture
            .owner
            .prepare_publications(
                Vec::new(),
                &[claim],
                &fixture.server,
                &fixture.database,
                &fixture.tilesets,
                &fixture.states,
            )
            .unwrap(),
    );
}

#[test]
fn prepare_rejects_extent_and_aggregate_retention_before_any_image_publication() {
    for extent_mismatch in [false, true] {
        let limits = TileSetAssetLimits::new(
            ItemLimit::new(2).unwrap(),
            ItemLimit::ONE,
            ByteLimit::new(8).unwrap(),
        );
        let mut fixture = Fixture::new(limits);
        let handle = fixture.request("a.png");
        fixture.reconcile();
        let claim = receipt(&fixture, handle);
        let old = fixture.tilesets.get(handle).unwrap().clone();
        let old_state = fixture.states.state_revision(handle.id());
        let old_image_slot = fixture
            .images
            .slot_revision(old.image_content().unwrap().handle());
        let old_events = fixture.events.len();
        let image = destination_image(&fixture.image_a, 30);
        let desired = if extent_mismatch {
            TileSetSource::new(
                AssetRef::path("a.png").unwrap(),
                TileAtlasLayout::grid(Vec2::ONE, 1, 1),
            )
        } else {
            source("a.png")
        };
        let (input, _, _) = terrain_input(&mut fixture, desired, &image);
        let input = input.with_owned_source(claim);
        let result = fixture.owner.prepare_publications(
            vec![input],
            &[],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states,
        );
        assert!(matches!(result, Err(error) if error == if extent_mismatch {
            TileSetAssetError::ImageExtentMismatch
        } else { TileSetAssetError::RetentionCapacity }));
        assert_eq!(fixture.tilesets.get(handle), Some(&old));
        assert_eq!(fixture.states.state_revision(handle.id()), old_state);
        assert_eq!(
            fixture
                .images
                .slot_revision(old.image_content().unwrap().handle()),
            old_image_slot
        );
        assert_eq!(fixture.owner.error(handle), None);
        assert_eq!(
            fixture.owner.retention(),
            TileSetRetentionSnapshot {
                images: 1,
                bytes: 8
            }
        );
        assert_eq!(fixture.events.len(), old_events);
    }
}

#[test]
fn aggregate_plan_deduplicates_one_image_and_installs_its_exact_postpublication_snapshot() {
    let limits = TileSetAssetLimits::new(
        ItemLimit::new(2).unwrap(),
        ItemLimit::ONE,
        ByteLimit::new(8).unwrap(),
    );
    let mut fixture = Fixture::new(limits);
    let other = record(
        "10000000-0000-4000-8000-000000000004",
        "other.tileset.ron",
        crate::tileset_asset_source_kind(),
    );
    fixture.database.insert(other.clone()).unwrap();
    let image = destination_image(&fixture.image_a, 40);
    let (first, first_handle, image_handle) = terrain_input(&mut fixture, source("a.png"), &image);
    let (second, second_handle, _) = input(&mut fixture, other, source("a.png"), &image);
    let prepared = fixture
        .owner
        .prepare_publications(
            vec![first, second],
            &[],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap();
    fixture
        .images
        .commit_loaded(
            image_handle,
            image,
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
    let expected = ImageContentSnapshot::capture(
        image_handle,
        &fixture.server,
        &fixture.states,
        &fixture.images,
    )
    .unwrap();
    let claims = prepared.commit(
        &fixture.server,
        &fixture.images,
        &mut fixture.tilesets,
        &mut fixture.states,
        &mut fixture.events,
    );
    assert_eq!(
        claims
            .iter()
            .map(TileSetOwnedSourceReceipt::handle)
            .collect::<Vec<_>>(),
        [first_handle, second_handle]
    );
    for handle in [first_handle, second_handle] {
        let actual = fixture
            .tilesets
            .get(handle)
            .unwrap()
            .image_content()
            .unwrap();
        assert_eq!(actual, &expected);
        assert!(actual.is_current(&fixture.server, &fixture.states, &fixture.images));
    }
    assert_eq!(
        fixture.owner.retention(),
        TileSetRetentionSnapshot {
            images: 1,
            bytes: 8
        }
    );
    let revisions = [
        fixture.tilesets.slot_revision(first_handle),
        fixture.tilesets.slot_revision(second_handle),
    ];
    fixture.reconcile();
    assert_eq!(fixture.tilesets.slot_revision(first_handle), revisions[0]);
    assert_eq!(fixture.tilesets.slot_revision(second_handle), revisions[1]);
}

#[test]
fn aggregate_plan_checks_entry_image_and_byte_ceilings_before_claiming_any_source() {
    for (entries, images, bytes, expected) in [
        (1, 2, 16, TileSetAssetError::Capacity),
        (2, 1, 16, TileSetAssetError::RetentionCapacity),
        (2, 2, 8, TileSetAssetError::RetentionCapacity),
    ] {
        let limits = TileSetAssetLimits::new(
            ItemLimit::new(entries).unwrap(),
            ItemLimit::new(images).unwrap(),
            ByteLimit::new(bytes).unwrap(),
        );
        let mut fixture = Fixture::new(limits);
        let other = record(
            "10000000-0000-4000-8000-000000000004",
            "other.tileset.ron",
            crate::tileset_asset_source_kind(),
        );
        fixture.database.insert(other.clone()).unwrap();
        fixture.database.insert(fixture.image_b.clone()).unwrap();
        let image_a = destination_image(&fixture.image_a, 41);
        let image_b = destination_image(&fixture.image_b, 42);
        let (first, first_handle, _) = terrain_input(&mut fixture, source("a.png"), &image_a);
        let (second, second_handle, _) = input(&mut fixture, other, source("b.png"), &image_b);
        let event_count = fixture.events.len();
        assert!(
            matches!(fixture.owner.prepare_publications(vec![first, second], &[], &fixture.server,
            &fixture.database, &fixture.tilesets, &fixture.states), Err(error) if error == expected)
        );
        assert!(fixture.owner.is_empty());
        assert_eq!(
            fixture.owner.retention(),
            TileSetRetentionSnapshot {
                images: 0,
                bytes: 0
            }
        );
        for handle in [first_handle, second_handle] {
            assert!(fixture.tilesets.get(handle).is_none());
            assert!(fixture.states.state(handle.id()).is_none());
        }
        assert_eq!(fixture.events.len(), event_count);
    }
}

#[test]
fn owned_explicit_unload_frees_entries_prevents_resurrection_and_permits_fresh_layout() {
    let limits = TileSetAssetLimits::new(
        ItemLimit::ONE,
        ItemLimit::new(3).unwrap(),
        ByteLimit::new(24).unwrap(),
    );
    let mut fixture = Fixture::new(limits);
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let claim = receipt(&fixture, handle);
    let old = fixture
        .tilesets
        .get(handle)
        .unwrap()
        .image_content()
        .unwrap()
        .clone();
    let prepared = fixture
        .owner
        .prepare_publications(
            Vec::new(),
            &[claim],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap();
    assert!(
        prepared
            .commit(
                &fixture.server,
                &fixture.images,
                &mut fixture.tilesets,
                &mut fixture.states,
                &mut fixture.events
            )
            .is_empty()
    );
    assert!(fixture.owner.is_empty());
    assert_eq!(fixture.owner.sources().count(), 0);
    fixture.reconcile();
    assert!(fixture.tilesets.get(handle).is_none());
    assert_eq!(
        fixture.owner.retention(),
        TileSetRetentionSnapshot {
            images: 1,
            bytes: 8
        }
    );

    let image = destination_image(&fixture.image_a, 50);
    let layout = TileSetSource::new(
        AssetRef::path("a.png").unwrap(),
        TileAtlasLayout::grid(Vec2::new(2.0, 1.0), 1, 1),
    );
    let (input, new_handle, image_handle) = terrain_input(&mut fixture, layout, &image);
    assert_eq!(new_handle, handle);
    let prepared = fixture
        .owner
        .prepare_publications(
            vec![input],
            &[],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap();
    fixture
        .images
        .commit_loaded(
            image_handle,
            image,
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
    let claims = prepared.commit(
        &fixture.server,
        &fixture.images,
        &mut fixture.tilesets,
        &mut fixture.states,
        &mut fixture.events,
    );
    assert_eq!(fixture.owner.len(), 1);
    assert_eq!(
        fixture
            .tilesets
            .get(handle)
            .unwrap()
            .atlas()
            .unwrap()
            .columns,
        1
    );
    assert_eq!(old.pixels(), &[10; 8]);
    assert_eq!(fixture.owner.retention().images, 2);
    drop(old);
    assert_eq!(fixture.owner.retention().images, 1);
    let prepared = fixture
        .owner
        .prepare_publications(
            Vec::new(),
            &claims,
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap();
    prepared.commit(
        &fixture.server,
        &fixture.images,
        &mut fixture.tilesets,
        &mut fixture.states,
        &mut fixture.events,
    );
    assert_eq!(
        fixture.owner.retention(),
        TileSetRetentionSnapshot {
            images: 0,
            bytes: 0
        }
    );
    assert!(fixture.owner.is_empty());
}

#[test]
fn shared_and_automatically_revoked_sources_keep_the_atlas_barrier() {
    for revoke in [false, true] {
        let mut fixture = Fixture::new(TileSetAssetLimits::default());
        let handle = fixture.request("a.png");
        fixture.reconcile();
        let claim = receipt(&fixture, handle);
        if revoke {
            fixture
                .owner
                .revoke_source(
                    handle,
                    &mut fixture.tilesets,
                    &mut fixture.states,
                    &mut fixture.events,
                )
                .unwrap();
        }
        let image = destination_image(&fixture.image_a, 60);
        let changed = TileSetSource::new(
            AssetRef::path("a.png").unwrap(),
            TileAtlasLayout::grid(Vec2::new(2.0, 1.0), 1, 1),
        );
        let (input, _, _) = terrain_input(&mut fixture, changed, &image);
        let input = input.with_owned_source(claim);
        assert!(matches!(
            fixture.owner.prepare_publications(
                vec![input],
                &[],
                &fixture.server,
                &fixture.database,
                &fixture.tilesets,
                &fixture.states
            ),
            Err(TileSetAssetError::AtlasLayoutChangeRequiresRebuild)
        ));
        assert_eq!(fixture.owner.len(), 1);
        assert_eq!(
            fixture.owner.entries[&handle.id()]
                .accepted_atlas
                .unwrap()
                .columns,
            2
        );
    }
}

#[test]
fn source_claim_survives_reconciliation_but_rejects_foreign_desired_or_raw_storage() {
    let mut fixture = Fixture::new(TileSetAssetLimits::default());
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let claim = receipt(&fixture, handle);
    fixture.publish_image(fixture.image_a.clone(), 2, 1, 70);
    fixture.reconcile();
    fixture.request("a.png");
    drop(
        fixture
            .owner
            .prepare_publications(
                Vec::new(),
                &[claim.clone()],
                &fixture.server,
                &fixture.database,
                &fixture.tilesets,
                &fixture.states,
            )
            .unwrap(),
    );
    fixture.request("b.png");
    assert!(matches!(
        fixture.owner.prepare_publications(
            Vec::new(),
            &[claim],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states
        ),
        Err(TileSetAssetError::Superseded)
    ));

    let claim = receipt(&fixture, handle);
    let foreign = fixture.tilesets.remove(handle).unwrap();
    fixture.tilesets.insert(handle, foreign);
    let slot = fixture.tilesets.slot_revision(handle);
    assert!(matches!(
        fixture.owner.prepare_publications(
            Vec::new(),
            &[claim],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states
        ),
        Err(TileSetAssetError::Superseded)
    ));
    assert_eq!(fixture.tilesets.slot_revision(handle), slot);
    assert!(fixture.tilesets.get(handle).is_some());
}

#[test]
fn duplicate_or_overlapping_batch_inputs_reject_without_claiming_an_entry() {
    let mut fixture = Fixture::new(TileSetAssetLimits::default());
    let image = destination_image(&fixture.image_a, 80);
    let (first, _, _) = terrain_input(&mut fixture, source("a.png"), &image);
    let (second, _, _) = terrain_input(&mut fixture, source("a.png"), &image);
    assert!(matches!(
        fixture.owner.prepare_publications(
            vec![first, second],
            &[],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states
        ),
        Err(TileSetAssetError::InvalidPublicationBatch)
    ));
    assert!(fixture.owner.is_empty());
    assert_eq!(fixture.owner.retention().images, 0);
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let claim = receipt(&fixture, handle);
    let (input, _, _) = terrain_input(&mut fixture, source("a.png"), &image);
    let input = input.with_owned_source(claim.clone());
    assert!(matches!(
        fixture.owner.prepare_publications(
            vec![input],
            &[claim],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states
        ),
        Err(TileSetAssetError::InvalidPublicationBatch)
    ));
    assert_eq!(fixture.owner.len(), 1);
}

#[test]
fn commit_rejects_another_image_allocation_before_changing_any_tileset() {
    let mut fixture = Fixture::new(TileSetAssetLimits::default());
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let claim = receipt(&fixture, handle);
    let old = fixture.tilesets.get(handle).unwrap().clone();
    let slot = fixture.tilesets.slot_revision(handle);
    let image = destination_image(&fixture.image_a, 90);
    let (input, _, image_handle) = terrain_input(&mut fixture, source("a.png"), &image);
    let input = input.with_owned_source(claim);
    let prepared = fixture
        .owner
        .prepare_publications(
            vec![input],
            &[],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap();
    fixture
        .images
        .commit_loaded(
            image_handle,
            destination_image(&fixture.image_a, 90),
            &mut fixture.states,
            &mut fixture.events,
            None,
            None,
        )
        .unwrap();
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
    assert_eq!(fixture.tilesets.slot_revision(handle), slot);
    assert_eq!(fixture.tilesets.get(handle), Some(&old));
    assert_eq!(fixture.owner.desired(handle), Some(&source("a.png")));
    assert_eq!(fixture.owner.error(handle), None);
}

#[test]
fn commit_rejects_state_drift_before_removing_an_owned_source() {
    let mut fixture = Fixture::new(TileSetAssetLimits::default());
    let handle = fixture.request("a.png");
    fixture.reconcile();
    let claim = receipt(&fixture, handle);
    let old = fixture.tilesets.get(handle).unwrap().clone();
    let slot = fixture.tilesets.slot_revision(handle);
    let event_count = fixture.events.len();
    let prepared = fixture
        .owner
        .prepare_publications(
            Vec::new(),
            &[claim],
            &fixture.server,
            &fixture.database,
            &fixture.tilesets,
            &fixture.states,
        )
        .unwrap();
    fixture.states.set_loading(handle.id());
    let changed_state = fixture.states.state_revision(handle.id());
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
    assert_eq!(fixture.tilesets.get(handle), Some(&old));
    assert_eq!(fixture.tilesets.slot_revision(handle), slot);
    assert_eq!(fixture.states.state_revision(handle.id()), changed_state);
    assert_eq!(fixture.owner.desired(handle), Some(&source("a.png")));
    assert_eq!(fixture.owner.len(), 1);
    assert_eq!(fixture.events.len(), event_count);
}
