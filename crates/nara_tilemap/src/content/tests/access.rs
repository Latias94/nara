use nara_ecs::{Mut, World, bevy_ecs::system::SystemState};

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
fn native_plugin_refreshes_requested_source_when_only_image_content_changes() {
    #[derive(Resource)]
    struct ImageMutation {
        frame: usize,
        record: AssetRecord,
    }

    fn mutate_image_after_first_frame(world: &mut World) {
        let record = {
            let mut mutation = world.resource_mut::<ImageMutation>();
            mutation.frame += 1;
            (mutation.frame == 2).then(|| mutation.record.clone())
        };
        if let Some(record) = record {
            publish_image(world, &record, 70);
        }
    }

    let scope = AssetPathScope::under(AssetSourcePath::new("project/art").unwrap());
    let image_record = record(
        "11111111-1111-4111-8111-111111111111",
        "project/art/a.png",
        AssetSourceKind::Image,
    );
    let source_record = record(
        "22222222-2222-4222-8222-222222222222",
        "project/art/terrain.ron",
        crate::tileset_asset_source_kind(),
    );
    let original_source = source("a.png");
    // This is the only Source request. No Host reconcile system or source execution plugin
    // is installed; the ordinary native plugin must observe the later Image publication.
    let mut app = App::new();
    app.add_plugins((nara_asset::AssetPlugin, TileSetAssetPlugin))
        .unwrap();
    app.init_resource::<ProjectAssetDatabase>().unwrap();
    app.insert_resource(ImageMutation {
        frame: 0,
        record: image_record.clone(),
    })
    .unwrap();
    let world = app.world_mut().unwrap();
    for record in [&image_record, &source_record] {
        world
            .resource_mut::<ProjectAssetDatabase>()
            .insert(record.clone())
            .unwrap();
    }
    publish_image(world, &image_record, 10);
    let mut access = SystemState::<TileSetContent>::new(world);
    let handle = access
        .get_mut(world)
        .unwrap()
        .request(
            source_record.clone(),
            original_source.clone(),
            SourceHash::from_bytes(b"scoped"),
            &scope,
        )
        .unwrap();
    drop(scope);
    app.add_systems(CoreStage::First, mutate_image_after_first_frame)
        .unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let first = runtime
        .world()
        .resource::<Assets<TileSet>>()
        .get(handle)
        .unwrap();
    let image_handle = first.material().image;
    let retained = first.image_content().unwrap().clone();
    let slot = runtime
        .world()
        .resource::<Assets<TileSet>>()
        .slot_revision(handle);
    assert_eq!(retained.pixels(), &[10; 8]);

    runtime.drive(std::time::Duration::ZERO).unwrap();

    let world = runtime.world();
    let current = world.resource::<Assets<TileSet>>().get(handle).unwrap();
    assert_eq!(current.material().image, image_handle);
    assert_eq!(current.image_content().unwrap().pixels(), &[70; 8]);
    assert_eq!(retained.pixels(), &[10; 8]);
    assert_ne!(
        world.resource::<Assets<TileSet>>().slot_revision(handle),
        slot
    );
    let owner = world.resource::<TileSetAssets>();
    assert_eq!(owner.error(handle), None);
    assert_eq!(owner.desired(handle), Some(&original_source));
    assert_eq!(owner.sources().next().unwrap().1, &source_record);
    assert!(!world.contains_resource::<nara_asset::AssetSourceChanges>());
    drop(retained);
    runtime.finish().unwrap();
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
                &nara_asset::AssetPathScope::ROOT,
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
                    SourceHash::from_bytes(b"b"),
                    &nara_asset::AssetPathScope::ROOT,
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
                &nara_asset::AssetPathScope::ROOT,
            )
            .unwrap();
        content.reconcile();
        content
            .request(
                record.clone(),
                source("b.png"),
                SourceHash::from_bytes(b"b"),
                &nara_asset::AssetPathScope::ROOT,
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
            &nara_asset::AssetPathScope::ROOT,
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
