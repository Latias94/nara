use nara_ecs::{Mut, World, system::SystemState};

use super::*;

fn fixture_world() -> (World, AssetRecord, AssetRecord) {
    let Fixture {
        server,
        database,
        images,
        tilesets,
        states,
        events,
        owner,
        image_b,
        tileset,
        ..
    } = Fixture::new(TileSetAssetLimits::default());
    let mut world = World::new();
    world.insert_resource(server);
    world.insert_resource(database);
    world.insert_resource(images);
    world.insert_resource(tilesets);
    world.insert_resource(states);
    world.insert_resource(events);
    world.insert_resource(owner);
    (world, tileset, image_b)
}

fn publish_image(world: &mut World, record: &AssetRecord, byte: u8) {
    let handle = world
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(record)
        .unwrap();
    world.resource_scope(|world, mut images: Mut<'_, Assets<ImageAsset>>| {
        world.resource_scope(|world, mut states: Mut<'_, AssetStates>| {
            images
                .commit_loaded(
                    handle,
                    image(record, 2, 1, byte),
                    &mut states,
                    &mut world.resource_mut::<AssetEvents>(),
                    None,
                    None,
                )
                .unwrap();
        });
    });
}

#[test]
fn concrete_access_recovers_a_retarget_and_retains_exact_content_until_its_final_holder() {
    let (mut world, record, image_b) = fixture_world();
    let mut access = SystemState::<TileSetContent>::new(&mut world);
    let (handle, old, initial) = {
        let mut content = access.get_mut(&mut world).unwrap();
        let handle = content
            .request(
                record.clone(),
                source("a.png"),
                SourceHash::from_bytes(b"a"),
            )
            .unwrap();
        content.reconcile();
        assert_eq!(content.error(handle), None);
        let old = content
            .get(handle)
            .unwrap()
            .image_content()
            .unwrap()
            .clone();
        let initial = content.source_revisions().collect::<Vec<_>>();
        assert_eq!(
            content
                .request(
                    record.clone(),
                    source("b.png"),
                    SourceHash::from_bytes(b"b")
                )
                .unwrap(),
            handle
        );
        content.reconcile();
        assert_eq!(
            content.error(handle),
            Some(&TileSetAssetError::DependencyUnavailable)
        );
        assert_eq!(content.get(handle).unwrap().image_content().unwrap(), &old);
        assert_eq!(content.sources().count(), 1);
        (handle, old, initial)
    };
    world
        .resource_mut::<ProjectAssetDatabase>()
        .insert(image_b.clone())
        .unwrap();
    publish_image(&mut world, &image_b, 50);
    {
        let mut content = access.get_mut(&mut world).unwrap();
        content.reconcile();
        assert_eq!(content.error(handle), None);
        assert_eq!(
            content
                .get(handle)
                .unwrap()
                .image_content()
                .unwrap()
                .pixels(),
            &[50; 8]
        );
        assert_ne!(content.source_revisions().collect::<Vec<_>>(), initial);
        assert_eq!(old.pixels(), &[10; 8]);
        assert!(content.revoke_source(&record).unwrap());
        content.reconcile();
        assert!(content.get(handle).is_none());
    }
    assert_eq!(world.resource::<TileSetAssets>().retention().images, 1);
    assert_eq!(old.pixels(), &[10; 8]);
    drop(old);
    assert_eq!(world.resource::<TileSetAssets>().retention().images, 0);
}

#[test]
fn concrete_access_rejection_stops_late_dependency_publication_and_allows_source_correction() {
    let (mut world, record, image_b) = fixture_world();
    let mut access = SystemState::<TileSetContent>::new(&mut world);
    let handle = {
        let mut content = access.get_mut(&mut world).unwrap();
        let handle = content
            .request(
                record.clone(),
                source("a.png"),
                SourceHash::from_bytes(b"a"),
            )
            .unwrap();
        content.reconcile();
        content
            .request(
                record.clone(),
                source("b.png"),
                SourceHash::from_bytes(b"b"),
            )
            .unwrap();
        content.reject_source(&record).unwrap();
        handle
    };
    world
        .resource_mut::<ProjectAssetDatabase>()
        .insert(image_b.clone())
        .unwrap();
    publish_image(&mut world, &image_b, 70);
    let mut content = access.get_mut(&mut world).unwrap();
    content.reconcile();
    assert_eq!(
        content.error(handle),
        Some(&TileSetAssetError::InvalidSource)
    );
    assert_eq!(
        content
            .get(handle)
            .unwrap()
            .image_content()
            .unwrap()
            .pixels(),
        &[10; 8]
    );
    content
        .request(
            record,
            source("b.png"),
            SourceHash::from_bytes(b"corrected"),
        )
        .unwrap();
    content.reconcile();
    assert_eq!(content.error(handle), None);
    assert_eq!(
        content
            .get(handle)
            .unwrap()
            .image_content()
            .unwrap()
            .pixels(),
        &[70; 8]
    );
}

#[test]
fn concrete_access_records_an_initial_rejection_without_claiming_another_source_kind() {
    let (mut world, record, image_b) = fixture_world();
    let baseline = world.resource::<AssetServer>().clone();
    let mut access = SystemState::<TileSetContent>::new(&mut world);
    {
        let mut content = access.get_mut(&mut world).unwrap();
        assert_eq!(
            content.reject_source(&image_b),
            Err(TileSetAssetError::InvalidSourceKind)
        );
        assert_eq!(
            content.revoke_source(&image_b),
            Err(TileSetAssetError::InvalidSourceKind)
        );
    }
    assert_eq!(world.resource::<AssetServer>(), &baseline);
    {
        let mut content = access.get_mut(&mut world).unwrap();
        content.reject_source(&record).unwrap();
        content.reconcile();
    }
    let server = world.resource::<AssetServer>();
    let (id, _) = world
        .resource::<AssetStates>()
        .iter()
        .find(|(id, _)| server.stable_id(*id) == Some(record.stable_id()))
        .unwrap();
    let handle = server.get_handle::<TileSet>(id).unwrap();
    assert!(world.resource::<Assets<TileSet>>().get(handle).is_none());
    assert!(matches!(
        world
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        LoadState::Failed { .. }
    ));
}

#[test]
fn concrete_access_rejects_a_missing_resource_join_before_work() {
    let mut world = World::new();
    let mut access = SystemState::<TileSetContent>::new(&mut world);
    assert!(access.get_mut(&mut world).is_err());
}
