use super::*;

use std::collections::BTreeSet;

use nara_ecs::{ResMut, bevy_ecs::system::SystemState};

fn prepare_materials(
    world: &World,
    content: &ProcessedSceneContent,
    change: StockChange<'_>,
) -> Result<(MaterialPublications, SceneAssetReservations), HostFault> {
    let input = ResidencyInput::Processed(content);
    let metadata = reserve(input, change)?;
    let mut bindings = SceneAssetReservations::from_world(world);
    let prepared = prepare(
        world,
        input,
        change,
        &mut bindings,
        &ImagePublications::empty(),
        metadata,
    )?;
    Ok((prepared, bindings))
}

fn commit_prepared(
    world: &mut World,
    prepared: (MaterialPublications, SceneAssetReservations),
) -> Vec<Material2dContentReceipt> {
    let (materials, bindings) = prepared;
    bindings.validate_baseline(world).unwrap();
    materials.validate(world).unwrap();
    world.insert_resource(bindings.staged_server().clone());
    materials.commit(world)
}

fn fail_reload(world: &mut World, handle: Handle<Material2dAsset>) {
    let mut access = SystemState::<(
        ResMut<Assets<Material2dAsset>>,
        ResMut<AssetStates>,
        ResMut<AssetEvents>,
    )>::new(world);
    let (mut values, mut states, mut events) = access.get_mut(world).unwrap();
    values
        .record_reload_failure(handle, &mut states, &mut events, "invalid replacement")
        .unwrap();
}

#[test]
fn signed_zero_defaults_refuse_sharing_while_identical_bits_keep_the_original_publication() {
    let source = material_source();
    let source_with = |zero| {
        Material2dSource::new(
            source.declaration().clone(),
            source.shader_paths().to_vec(),
            BTreeMap::from([(
                "color.tint".to_owned(),
                MaterialParameterValue::Vec4([0.25, zero, 0.75, 1.0]),
            )]),
            BTreeMap::new(),
        )
        .unwrap()
    };
    let positive = import_material(product(MATERIAL), &source_with(0.0), BODY);
    let negative = import_material(product(MATERIAL), &source_with(-0.0), BODY);
    assert_eq!(positive.default_parameters(), negative.default_parameters());
    let (_fixture, base) = Fixture::with_material(positive);
    let (_changed_fixture, changed_base) = Fixture::with_material(negative);
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    let same = base.prepare_scene_content(product(STARTUP)).unwrap();
    let changed = changed_base
        .prepare_scene_content(product(STARTUP))
        .unwrap();
    let mut world = world();
    publish_processed_startup(&mut world, &content).unwrap();
    let current = world.resource::<ProjectSceneResidency>();
    let handle = current.stock.materials[0].handle();
    let slot = world
        .resource::<Assets<Material2dAsset>>()
        .slot_revision(handle);
    let version = world.resource::<AssetStates>().version(handle.id());
    let events = world.resource::<AssetEvents>().iter().count();
    let identical = prepare_materials(&world, &same, StockChange::Add(current)).unwrap();
    assert_eq!(identical.0.batch.additive_products(), [product(MATERIAL)]);
    assert!(prepare_materials(&world, &changed, StockChange::Add(current)).is_err());
    assert!(commit_prepared(&mut world, identical).is_empty());
    assert_eq!(
        world
            .resource::<Assets<Material2dAsset>>()
            .slot_revision(handle),
        slot
    );
    assert_eq!(
        world.resource::<AssetStates>().version(handle.id()),
        version
    );
    assert_eq!(world.resource::<AssetEvents>().iter().count(), events);
}

#[test]
fn additive_material_shares_primary_handle_slot_version_and_original_custody() {
    let (_fixture, base) = Fixture::open();
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    let independent = base.prepare_scene_content(product(STARTUP)).unwrap();
    let mut world = world();
    publish_processed_startup(&mut world, &content).unwrap();
    let current = world.resource::<ProjectSceneResidency>();
    let handle = current.stock.materials[0].handle();
    let slot = world
        .resource::<Assets<Material2dAsset>>()
        .slot_revision(handle);
    let version = world.resource::<AssetStates>().version(handle.id());
    let events = world.resource::<AssetEvents>().iter().count();
    let baseline = retained_bytes(&base);
    let prepared = prepare_materials(&world, &independent, StockChange::Add(current)).unwrap();
    assert_eq!(prepared.0.batch.additive_products(), [product(MATERIAL)]);
    assert!(prepared.0.batch.retiring_products().next().is_none());
    assert!(commit_prepared(&mut world, prepared).is_empty());
    assert_eq!(retained_bytes(&base), baseline);
    assert_eq!(
        world
            .resource::<Assets<Material2dAsset>>()
            .slot_revision(handle),
        slot
    );
    assert_eq!(
        world.resource::<AssetStates>().version(handle.id()),
        version
    );
    assert_eq!(world.resource::<AssetEvents>().iter().count(), events);
    assert_eq!(
        world
            .resource::<ProjectSceneResidency>()
            .stock
            .materials
            .len(),
        1
    );
}

#[test]
fn additive_pool_publishes_once_and_replace_retires_primary_and_pooled_union() {
    let (_fixture, base) = Fixture::open();
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    let addition = base.prepare_scene_content(product(ADDITION)).unwrap();
    let independent = base.prepare_scene_content(product(ADDITION)).unwrap();
    let empty = base.prepare_scene_content(product(EMPTY)).unwrap();
    let mut world = world();
    publish_processed_startup(&mut world, &content).unwrap();
    let current = world.resource::<ProjectSceneResidency>();
    let primary = current.stock.materials[0].handle();
    let prepared = prepare_materials(&world, &addition, StockChange::Add(current)).unwrap();
    assert_eq!(
        prepared.0.batch.additive_products(),
        [product(OTHER_MATERIAL)]
    );
    assert!(prepared.0.batch.retiring_products().next().is_none());
    let pool = commit_prepared(&mut world, prepared);
    assert_eq!(pool.len(), 1);
    let handle = pool[0].handle();
    let slot = world
        .resource::<Assets<Material2dAsset>>()
        .slot_revision(handle);
    let version = world.resource::<AssetStates>().version(handle.id());
    let events = world.resource::<AssetEvents>().iter().count();
    world
        .resource_mut::<ProjectSceneResidency>()
        .additions
        .stock
        .materials
        .extend(pool);
    let current = world.resource::<ProjectSceneResidency>();
    let shared = prepare_materials(&world, &independent, StockChange::Add(current)).unwrap();
    assert_eq!(
        shared.0.batch.additive_products(),
        [product(OTHER_MATERIAL)]
    );
    assert!(shared.0.batch.retiring_products().next().is_none());
    assert!(commit_prepared(&mut world, shared).is_empty());
    assert_eq!(
        world
            .resource::<Assets<Material2dAsset>>()
            .slot_revision(handle),
        slot
    );
    assert_eq!(
        world.resource::<AssetStates>().version(handle.id()),
        version
    );
    assert_eq!(world.resource::<AssetEvents>().iter().count(), events);
    assert!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(primary)
            .is_some()
    );
    let current = world.resource::<ProjectSceneResidency>();
    let replacement =
        prepare_materials(&world, &empty, StockChange::Replace(Some(current))).unwrap();
    assert_eq!(
        replacement
            .0
            .batch
            .retiring_products()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([product(MATERIAL), product(OTHER_MATERIAL)]),
    );
    assert!(replacement.0.batch.additive_products().is_empty());
    assert!(commit_prepared(&mut world, replacement).is_empty());
    assert!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(primary)
            .is_none()
    );
    assert!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(handle)
            .is_none()
    );
}

#[test]
fn borrowed_pool_retirement_removes_only_selected_material_and_keeps_escaped_binding() {
    let (_fixture, base) = Fixture::open();
    let baseline = retained_bytes(&base);
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    let addition = base.prepare_scene_content(product(ADDITION)).unwrap();
    let acquisition_bytes = addition.materials[0].retained_bytes();
    let mut world = world();
    publish_processed_startup(&mut world, &content).unwrap();
    let current = world.resource::<ProjectSceneResidency>();
    let primary = current.stock.materials[0].handle();
    let prepared = prepare_materials(&world, &addition, StockChange::Add(current)).unwrap();
    let pool = commit_prepared(&mut world, prepared);
    let handle = pool[0].handle();
    let binding = world
        .resource::<Assets<Material2dAsset>>()
        .get(handle)
        .unwrap()
        .bind_values(BTreeMap::new(), BTreeMap::new(), |_| None)
        .unwrap();
    world
        .resource_mut::<ProjectSceneResidency>()
        .additions
        .stock
        .materials
        .extend(pool);
    let current = world.resource::<ProjectSceneResidency>();
    let retirement = retire(current.additions.stock.materials.iter(), &world).unwrap();
    let retired = retirement
        .batch
        .retiring_products()
        .collect::<BTreeSet<_>>();
    assert_eq!(retired, BTreeSet::from([product(OTHER_MATERIAL)]));
    assert!(retirement.commit(&mut world).is_empty());
    world
        .resource_mut::<ProjectSceneResidency>()
        .additions
        .stock
        .materials
        .retain(|receipt| !retired.contains(&receipt.product()));
    assert!(
        world
            .resource::<ProjectSceneResidency>()
            .additions
            .stock
            .materials
            .is_empty()
    );
    assert!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(primary)
            .is_some()
    );
    assert!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(handle)
            .is_none()
    );
    drop((content, addition, world));
    assert!(retained_bytes(&base) > baseline + acquisition_bytes);
    drop(binding);
    assert_eq!(retained_bytes(&base), baseline);
}

#[test]
fn conflicting_live_material_refuses_before_any_publication_or_scene_mutation() {
    let (_fixture, base) = Fixture::open();
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    let source = material_source();
    let changed = Material2dSource::new(
        source.declaration().clone(),
        source.shader_paths().to_vec(),
        BTreeMap::from([(
            "color.tint".to_owned(),
            MaterialParameterValue::Vec4([1.0, 0.0, 0.0, 1.0]),
        )]),
        BTreeMap::new(),
    )
    .unwrap();
    let (_other_fixture, other_base) =
        Fixture::with_material(import_material(product(MATERIAL), &changed, BODY));
    let conflicting = other_base.prepare_scene_content(product(STARTUP)).unwrap();
    let mut world = world();
    publish_processed_startup(&mut world, &content).unwrap();
    let current = world.resource::<ProjectSceneResidency>();
    let handle = current.stock.materials[0].handle();
    let slot = world
        .resource::<Assets<Material2dAsset>>()
        .slot_revision(handle);
    let version = world.resource::<AssetStates>().version(handle.id());
    let events = world.resource::<AssetEvents>().iter().count();
    let entities = world.entities().len();
    let server = world.resource::<AssetServer>().clone();
    assert!(prepare_materials(&world, &conflicting, StockChange::Add(current)).is_err());
    assert_eq!(
        world
            .resource::<Assets<Material2dAsset>>()
            .slot_revision(handle),
        slot
    );
    assert_eq!(
        world.resource::<AssetStates>().version(handle.id()),
        version
    );
    assert_eq!(world.resource::<AssetEvents>().iter().count(), events);
    assert_eq!(world.entities().len(), entities);
    assert_eq!(world.resource::<AssetServer>(), &server);
}

#[test]
fn failed_reload_keeps_exact_last_good_retirement_but_revokes_ordinary_sharing() {
    for failed_before_preparation in [true, false] {
        let (_fixture, base) = Fixture::open();
        let content = base.prepare_scene_content(product(STARTUP)).unwrap();
        let mut world = world();
        publish_processed_startup(&mut world, &content).unwrap();
        let current = world.resource::<ProjectSceneResidency>();
        let handle = current.stock.materials[0].handle();
        let shared = prepare_materials(&world, &content, StockChange::Add(current)).unwrap();
        let slot = world
            .resource::<Assets<Material2dAsset>>()
            .slot_revision(handle);
        let publication = world
            .resource::<AssetStates>()
            .publication_revision(handle.id());
        if failed_before_preparation {
            fail_reload(&mut world, handle);
        }
        let retirement = retire(
            world.resource::<ProjectSceneResidency>().owned_materials(),
            &world,
        )
        .unwrap();
        if !failed_before_preparation {
            fail_reload(&mut world, handle);
        }
        assert_eq!(
            world
                .resource::<Assets<Material2dAsset>>()
                .slot_revision(handle),
            slot
        );
        assert_eq!(
            world
                .resource::<AssetStates>()
                .publication_revision(handle.id()),
            publication
        );
        assert!(shared.0.validate(&world).is_err());
        let current = world.resource::<ProjectSceneResidency>();
        assert!(prepare_materials(&world, &content, StockChange::Add(current)).is_err());
        retirement.validate(&world).unwrap();
        assert!(retirement.commit(&mut world).is_empty());
        assert!(
            world
                .resource::<Assets<Material2dAsset>>()
                .get(handle)
                .is_none()
        );
        assert!(matches!(
            world
                .resource::<AssetStates>()
                .state(handle.id())
                .unwrap()
                .load_state(),
            LoadState::Removed,
        ));
    }
}

#[test]
fn failed_last_good_material_cannot_retire_after_publication_revocation() {
    let (_fixture, base) = Fixture::open();
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    let mut world = world();
    publish_processed_startup(&mut world, &content).unwrap();
    let handle = world.resource::<ProjectSceneResidency>().stock.materials[0].handle();
    fail_reload(&mut world, handle);
    let retirement = retire(
        world.resource::<ProjectSceneResidency>().owned_materials(),
        &world,
    )
    .unwrap();
    world
        .resource_mut::<AssetStates>()
        .invalidate_publication(handle.id());
    assert!(retirement.validate(&world).is_err());
    assert!(
        retire(
            world.resource::<ProjectSceneResidency>().owned_materials(),
            &world,
        )
        .is_err(),
    );
    assert!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(handle)
            .is_some()
    );
}
