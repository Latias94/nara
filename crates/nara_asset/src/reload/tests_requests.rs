use super::*;

#[test]
fn registered_reload_consumer_must_claim_requests_in_the_same_frame() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    let _consumer = register_image_reload_consumer(&mut app).unwrap();
    let record = image_record("textures/unclaimed.png", stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(record.clone())
        .unwrap();
    let asset_id = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record_id(&record)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone())
        .unwrap();

    app.update().unwrap();

    let requests = app.world().resource::<AssetReloadRequests>();
    assert!(requests.is_empty());
    assert_eq!(requests.retained_payload_bytes(), 0);
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(asset_id)
            .unwrap()
            .load_state(),
        crate::LoadState::Failed { message }
            if message == "asset.reload-consumer-did-not-claim"
    ));
    assert!(
        app.world().resource::<AssetEvents>().iter().any(|event| {
            event.id() == asset_id && event.kind() == AssetEventKind::ReloadRejected
        })
    );
    assert_eq!(
        app.world()
            .resource::<AssetReloadDiagnostics>()
            .iter()
            .map(|diagnostic| diagnostic.code().as_str())
            .collect::<Vec<_>>(),
        vec!["asset.reload-consumer-did-not-claim"]
    );
}

#[test]
fn stale_unclaimed_request_cannot_overwrite_newer_asset_state() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    let _consumer = register_image_reload_consumer(&mut app).unwrap();
    let record = image_record("textures/stale.png", stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(record.clone())
        .unwrap();
    let asset_id = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record_id(&record)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone())
        .unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        (move |mut generations: ResMut<AssetLoadGenerations>, mut states: ResMut<AssetStates>| {
            generations.begin_request(asset_id);
            states.set_loaded_at(asset_id, AssetVersion::from_raw(1), None, None);
        })
        .in_set(AssetTaskUpdateSet::ApplyResults),
    )
    .unwrap();

    app.update().unwrap();

    assert!(app.world().resource::<AssetReloadRequests>().is_empty());
    let state = app
        .world()
        .resource::<AssetStates>()
        .state(asset_id)
        .unwrap();
    assert_eq!(state.version(), AssetVersion::from_raw(1));
    assert_eq!(state.load_state(), &crate::LoadState::Loaded);
    assert!(app.world().resource::<AssetEvents>().is_empty());
    assert!(app.world().resource::<AssetReloadDiagnostics>().is_empty());
}

#[test]
fn unclaimed_request_cannot_overwrite_a_newer_asset_version() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    let _consumer = register_image_reload_consumer(&mut app).unwrap();
    let record = image_record("textures/version-stale.png", stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(record.clone())
        .unwrap();
    let asset_id = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record_id(&record)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone())
        .unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        (move |mut states: ResMut<AssetStates>| {
            states.set_loaded_at(asset_id, AssetVersion::from_raw(1), None, None);
        })
        .in_set(AssetTaskUpdateSet::ApplyResults),
    )
    .unwrap();

    app.update().unwrap();

    let state = app
        .world()
        .resource::<AssetStates>()
        .state(asset_id)
        .unwrap();
    assert_eq!(state.version(), AssetVersion::from_raw(1));
    assert_eq!(state.load_state(), &crate::LoadState::Loaded);
    assert!(app.world().resource::<AssetEvents>().is_empty());
    assert!(app.world().resource::<AssetReloadDiagnostics>().is_empty());
}

#[test]
fn replacing_request_limits_does_not_drop_image_consumer_authority() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    app.insert_resource(AssetReloadRequests::with_limits(
        AssetReloadRequestLimits::new(
            ItemLimit::new(8).unwrap(),
            ByteLimit::new(16 * 1024).unwrap(),
        ),
    ))
    .unwrap();
    let record = image_record("textures/reconfigured.png", stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(record.clone())
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone())
        .unwrap();

    app.update().unwrap();

    assert_eq!(
        app.world()
            .resource::<CapturedReloadRequests>()
            .0
            .iter()
            .map(|request| request.path().as_str())
            .collect::<Vec<_>>(),
        vec!["textures/reconfigured.png"]
    );
}

#[test]
fn reload_request_item_limit_rejects_excess_work_without_retention() {
    let mut app = App::new();
    app.insert_resource(AssetReloadRequests::with_limits(
        AssetReloadRequestLimits::new(ItemLimit::ONE, ByteLimit::new(4 * 1024).unwrap()),
    ))
    .unwrap();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    let accepted = image_record("textures/a.png", stable_id());
    let first_rejected = image_record("textures/b.png", dependent_stable_id());
    let second_rejected = image_record("textures/c.png", transitive_dependent_stable_id());
    {
        let mut database = app
            .world_mut()
            .unwrap()
            .resource_mut::<ProjectAssetDatabase>();
        database.insert(accepted.clone()).unwrap();
        database.insert(first_rejected.clone()).unwrap();
        database.insert(second_rejected.clone()).unwrap();
    }
    let (first_rejected_id, second_rejected_id) = {
        let mut server = app.world_mut().unwrap().resource_mut::<AssetServer>();
        server.reserve_record_id(&accepted).unwrap();
        (
            server.reserve_record_id(&first_rejected).unwrap(),
            server.reserve_record_id(&second_rejected).unwrap(),
        )
    };
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetDependencyGraph>()
        .add_source_dependency(accepted.stable_id(), first_rejected.stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetDependencyGraph>()
        .add_source_dependency(accepted.stable_id(), second_rejected.stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(accepted.path().clone())
        .unwrap();

    app.update().unwrap();

    let captured = app.world().resource::<CapturedReloadRequests>();
    assert_eq!(captured.0.len(), 1);
    assert_eq!(captured.0[0].path(), accepted.path());
    let requests = app.world().resource::<AssetReloadRequests>();
    assert!(requests.is_empty());
    assert_eq!(requests.retained_payload_bytes(), 0);
    for rejected_id in [first_rejected_id, second_rejected_id] {
        assert!(matches!(
            app.world()
                .resource::<AssetStates>()
                .state(rejected_id)
                .unwrap()
                .load_state(),
            crate::LoadState::Failed { message }
                if message == "asset.reload-request-item-limit-exceeded"
        ));
        assert!(app.world().resource::<AssetEvents>().iter().any(|event| {
            event.id() == rejected_id && event.kind() == AssetEventKind::ReloadRejected
        }));
    }
    let diagnostics = app.world().resource::<AssetReloadDiagnostics>();
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code().as_str())
            .collect::<Vec<_>>(),
        vec![
            "asset.reload-request-item-limit-exceeded",
            "asset.reload-request-item-limit-exceeded"
        ]
    );
    for diagnostic in diagnostics.iter() {
        let field = |key| {
            diagnostic
                .fields()
                .iter()
                .find(|field| field.key().as_str() == key)
                .unwrap()
        };
        assert_eq!(
            field("source-kind").value(),
            nara_diagnostic::DiagnosticValueRef::Identifier("image")
        );
        assert_eq!(
            field("limit").value(),
            nara_diagnostic::DiagnosticValueRef::Unsigned(1)
        );
        assert_eq!(
            field("attempted").value(),
            nara_diagnostic::DiagnosticValueRef::Unsigned(2)
        );
        assert_eq!(
            field("error-detail").class(),
            nara_diagnostic::DiagnosticFieldClass::Sensitive
        );
        assert_eq!(
            field("error-detail").value(),
            nara_diagnostic::DiagnosticValueRef::Redacted
        );
    }
}

#[test]
fn reload_request_byte_limit_rejects_work_without_retention() {
    let mut app = App::new();
    app.insert_resource(AssetReloadRequests::with_limits(
        AssetReloadRequestLimits::new(ItemLimit::new(8).unwrap(), ByteLimit::ONE),
    ))
    .unwrap();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    let record = image_record("textures/over-budget.png", stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(record.clone())
        .unwrap();
    let asset_id = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record_id(&record)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone())
        .unwrap();

    app.update().unwrap();

    assert!(
        app.world()
            .resource::<CapturedReloadRequests>()
            .0
            .is_empty()
    );
    let requests = app.world().resource::<AssetReloadRequests>();
    assert!(requests.is_empty());
    assert_eq!(requests.retained_payload_bytes(), 0);
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(asset_id)
            .unwrap()
            .load_state(),
        crate::LoadState::Failed { message }
            if message == "asset.reload-request-byte-limit-exceeded"
    ));
    assert_eq!(
        app.world()
            .resource::<AssetReloadDiagnostics>()
            .iter()
            .map(|diagnostic| diagnostic.code().as_str())
            .collect::<Vec<_>>(),
        vec!["asset.reload-request-byte-limit-exceeded"]
    );
}

#[test]
fn reload_request_charges_the_actual_owned_spare_capacity() {
    let mut app = App::new();
    app.insert_resource(AssetReloadRequests::new()).unwrap();
    let consumer = register_image_reload_consumer(&mut app).unwrap();
    let authority = Arc::clone(&app.world().resource::<ImageReloadRegistration>().authority);
    let mut path = String::with_capacity(4_096);
    path.push_str("textures/capacity.png");
    let record = AssetRecord::new(
        stable_id(),
        AssetPath::new(path).unwrap(),
        AssetSourceKind::Image,
    );
    let mut artifacts = Vec::with_capacity(8);
    artifacts.push(ImportArtifactDigest::from_digest([7; 32]));
    let expected_bytes = record
        .retained_bytes()
        .unwrap()
        .checked_add(
            artifacts
                .capacity()
                .checked_mul(std::mem::size_of::<ImportArtifactDigest>())
                .unwrap(),
        )
        .unwrap();
    let mut requests = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetReloadRequests>();

    requests
        .try_push_resolved(
            AssetId::from_raw(1),
            record,
            AssetReloadRequestKind::LoadOrReload,
            AssetSourceChangeKind::Modified,
            AssetVersion::ZERO,
            AssetLoadGeneration::ZERO,
            artifacts,
            authority,
        )
        .unwrap();

    assert_eq!(requests.retained_payload_bytes(), expected_bytes);
    let drained = requests.drain_images(&consumer).unwrap();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].retained_payload_bytes().unwrap(), expected_bytes);
    assert_eq!(requests.retained_payload_bytes(), 0);
}

#[test]
fn image_reload_consumer_authority_is_scoped_to_its_app() {
    let mut owner_app = App::new();
    owner_app
        .insert_resource(AssetReloadRequests::new())
        .unwrap();
    let owner_consumer = register_image_reload_consumer(&mut owner_app).unwrap();
    let owner_authority = Arc::clone(
        &owner_app
            .world()
            .resource::<ImageReloadRegistration>()
            .authority,
    );

    let mut foreign_app = App::new();
    foreign_app
        .insert_resource(AssetReloadRequests::new())
        .unwrap();
    let foreign_consumer = register_image_reload_consumer(&mut foreign_app).unwrap();

    let mut requests = owner_app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetReloadRequests>();
    requests
        .try_push_resolved(
            AssetId::from_raw(1),
            image_record("textures/authority.png", stable_id()),
            AssetReloadRequestKind::LoadOrReload,
            AssetSourceChangeKind::Modified,
            AssetVersion::ZERO,
            AssetLoadGeneration::ZERO,
            Vec::new(),
            owner_authority,
        )
        .unwrap();
    let retained_bytes = requests.retained_payload_bytes();

    assert_eq!(
        requests.drain_images(&foreign_consumer),
        Err(ImageReloadDrainError::AuthorityMismatch)
    );
    assert_eq!(requests.len(), 1);
    assert_eq!(requests.retained_payload_bytes(), retained_bytes);
    assert_eq!(requests.drain_images(&owner_consumer).unwrap().len(), 1);
    assert_eq!(requests.retained_payload_bytes(), 0);
}

#[test]
fn image_reload_consumer_rejects_duplicate_owners() {
    let mut app = App::new();
    app.insert_resource(AssetReloadRequests::new()).unwrap();
    let _consumer = register_image_reload_consumer(&mut app).unwrap();

    assert!(matches!(
        register_image_reload_consumer(&mut app),
        Err(ImageReloadRegistrationError::AlreadyRegistered)
    ));
}

#[test]
fn image_reload_consumer_requires_asset_plugin_resources() {
    let mut app = App::new();

    assert!(matches!(
        register_image_reload_consumer(&mut app),
        Err(ImageReloadRegistrationError::AssetPluginMissing)
    ));
}

#[test]
fn first_load_failure_uses_distinct_event_without_value() {
    let mut assets = Assets::<String>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let mut server = AssetServer::new();
    let handle = server
        .reserve_record::<String>(&image_record("textures/player.png", stable_id()))
        .unwrap();
    states.set_loading(handle.id());

    assets
        .record_load_failure(handle, &mut states, &mut events, "decode failed")
        .unwrap();

    assert!(assets.get(handle).is_none());
    assert_eq!(
        states.state(handle.id()).unwrap().load_state(),
        &crate::LoadState::Failed {
            message: "decode failed".to_string()
        }
    );
    assert_eq!(
        events.drain(),
        vec![crate::AssetEvent::new(
            handle.id(),
            AssetVersion::ZERO,
            AssetEventKind::LoadFailed
        )]
    );
}
