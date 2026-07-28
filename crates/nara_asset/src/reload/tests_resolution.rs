use super::*;

#[test]
fn asset_plugin_resolves_manual_changes_into_generation_requests() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(image_record("textures/player.png", stable_id()))
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(AssetPath::new("textures/player.png").unwrap())
        .unwrap();

    app.update().unwrap();

    let requests = app.world().resource::<CapturedReloadRequests>();
    let request = requests.0.first().unwrap();
    assert_eq!(request.path().as_str(), "textures/player.png");
    assert_eq!(request.generation().raw(), 1);
    assert_eq!(request.expected_version(), AssetVersion::ZERO);
    assert_eq!(request.request_kind(), AssetReloadRequestKind::LoadOrReload);
    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .state(request.asset_id())
            .unwrap()
            .load_state(),
        &crate::LoadState::Loading
    );
}

#[test]
fn resolver_terminates_unknown_source_changes_instead_of_retaining_them() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(AssetPath::new("textures/missing.png").unwrap())
        .unwrap();

    app.update().unwrap();

    let requests = app.world().resource::<AssetReloadRequests>();
    assert!(requests.is_empty());
    assert_eq!(
        app.world()
            .resource::<AssetReloadDiagnostics>()
            .iter()
            .map(|diagnostic| diagnostic.code().as_str())
            .collect::<Vec<_>>(),
        vec!["asset.reload-record-missing"]
    );
}

#[test]
fn missing_record_invalidates_an_existing_runtime_identity() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    let path = AssetPath::new("textures/missing.png").unwrap();
    let asset_id = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_path_id(path.clone())
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetStates>()
        .set_loading(asset_id);
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetLoadGenerations>()
        .begin_request(asset_id);
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(path)
        .unwrap();

    app.update().unwrap();

    assert_eq!(
        app.world()
            .resource::<AssetLoadGenerations>()
            .current(asset_id)
            .raw(),
        2
    );
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(asset_id)
            .unwrap()
            .load_state(),
        crate::LoadState::Failed { message } if message == "asset.reload-record-missing"
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
        vec!["asset.reload-record-missing"]
    );
}

#[test]
fn missing_dependent_record_invalidates_its_existing_runtime_identity() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    let source = image_record("textures/source.png", stable_id());
    let missing_dependent = image_record("textures/missing-dependent.png", dependent_stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(source.clone())
        .unwrap();
    let dependent_asset_id = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record_id(&missing_dependent)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetStates>()
        .set_loading(dependent_asset_id);
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetLoadGenerations>()
        .begin_request(dependent_asset_id);
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetDependencyGraph>()
        .add_source_dependency(source.stable_id(), missing_dependent.stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(source.path().clone())
        .unwrap();

    app.update().unwrap();

    assert_eq!(
        app.world()
            .resource::<AssetLoadGenerations>()
            .current(dependent_asset_id)
            .raw(),
        2
    );
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(dependent_asset_id)
            .unwrap()
            .load_state(),
        crate::LoadState::Failed { message }
            if message == "asset.reload-dependent-record-missing"
    ));
    assert!(app.world().resource::<AssetEvents>().iter().any(|event| {
        event.id() == dependent_asset_id && event.kind() == AssetEventKind::ReloadRejected
    }));
    assert_eq!(
        app.world()
            .resource::<AssetReloadDiagnostics>()
            .iter()
            .map(|diagnostic| diagnostic.code().as_str())
            .collect::<Vec<_>>(),
        vec!["asset.reload-dependent-record-missing"]
    );
    assert_eq!(
        app.world()
            .resource::<CapturedReloadRequests>()
            .0
            .iter()
            .map(|request| request.path().as_str())
            .collect::<Vec<_>>(),
        vec!["textures/source.png"]
    );
}

#[test]
fn missing_source_record_still_resolves_known_dependents() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    let missing_source = image_record("textures/missing-source.png", stable_id());
    let dependent = image_record("textures/dependent.png", dependent_stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(dependent.clone())
        .unwrap();
    let missing_source_id = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record_id(&missing_source)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetStates>()
        .set_loading(missing_source_id);
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetLoadGenerations>()
        .begin_request(missing_source_id);
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetDependencyGraph>()
        .add_source_dependency(missing_source.stable_id(), dependent.stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(missing_source.path().clone())
        .unwrap();

    app.update().unwrap();

    assert_eq!(
        app.world()
            .resource::<CapturedReloadRequests>()
            .0
            .iter()
            .map(|request| request.path().as_str())
            .collect::<Vec<_>>(),
        vec!["textures/dependent.png"]
    );
    assert_eq!(
        app.world()
            .resource::<AssetLoadGenerations>()
            .current(missing_source_id)
            .raw(),
        2
    );
}

#[test]
fn resolver_rejects_a_known_source_without_a_reload_consumer() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    let record = image_record("textures/player.png", stable_id());
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
        .modified(AssetPath::new("textures/player.png").unwrap())
        .unwrap();

    app.update().unwrap();

    assert!(app.world().resource::<AssetReloadRequests>().is_empty());
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(asset_id)
            .unwrap()
            .load_state(),
        crate::LoadState::Failed { message }
            if message == "asset.reload-source-consumer-missing"
    ));
    assert_eq!(
        app.world()
            .resource::<AssetReloadDiagnostics>()
            .iter()
            .map(|diagnostic| diagnostic.code().as_str())
            .collect::<Vec<_>>(),
        vec!["asset.reload-source-consumer-missing"]
    );
}

#[test]
fn resolver_rejects_unsupported_source_kinds_with_typed_diagnostics() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    let record = AssetRecord::new(
        stable_id(),
        AssetPath::new("scenes/level.scene").unwrap(),
        AssetSourceKind::Scene,
    );
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
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(asset_id)
            .unwrap()
            .load_state(),
        crate::LoadState::Failed { message }
            if message == "asset.reload-source-consumer-missing"
    ));
    let diagnostic = app
        .world()
        .resource::<AssetReloadDiagnostics>()
        .iter()
        .next()
        .unwrap();
    assert_eq!(
        diagnostic.code().as_str(),
        "asset.reload-source-consumer-missing"
    );
    assert_eq!(
        diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == "source-kind")
            .unwrap()
            .value(),
        nara_diagnostic::DiagnosticValueRef::Identifier("scene")
    );
    assert!(
        app.world().resource::<AssetEvents>().iter().any(|event| {
            event.id() == asset_id && event.kind() == AssetEventKind::ReloadRejected
        })
    );
}

#[test]
fn resolver_errors_are_recorded_as_reload_diagnostics() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(image_record("textures/player.png", stable_id()))
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record_id(&image_record("textures/player.png", dependent_stable_id()))
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(AssetPath::new("textures/player.png").unwrap())
        .unwrap();

    app.update().unwrap();

    let diagnostics = app.world().resource::<AssetReloadDiagnostics>();
    assert!(diagnostics.has_errors());
    assert_eq!(diagnostics.iter().len(), 1);
    let diagnostic = diagnostics.iter().next().unwrap();
    assert_eq!(
        diagnostic.code().as_str(),
        "asset.reload-source-change-resolve-failed"
    );
    assert_eq!(
        diagnostic.summary().as_str(),
        "Asset source change resolution failed"
    );
    let field = |key: &str| {
        diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == key)
            .unwrap()
    };
    assert_eq!(
        field("asset-path").value(),
        nara_diagnostic::DiagnosticValueRef::ProjectRelative("textures/player.png")
    );
    assert_eq!(
        field("change-kind").value(),
        nara_diagnostic::DiagnosticValueRef::Identifier("modified")
    );
    assert_eq!(
        field("reason").value(),
        nara_diagnostic::DiagnosticValueRef::Identifier("asset-id-already-bound-to-stable-id")
    );
    assert_eq!(
        field("error-detail").class(),
        nara_diagnostic::DiagnosticFieldClass::Sensitive
    );
    assert_eq!(
        field("error-detail").value(),
        nara_diagnostic::DiagnosticValueRef::Redacted
    );
    let diagnostic_debug = format!("{diagnostic:?}");
    assert!(!diagnostic_debug.contains(&stable_id().to_string()));
    assert!(!diagnostic_debug.contains(&dependent_stable_id().to_string()));
    assert!(app.world().resource::<AssetReloadRequests>().is_empty());
}

#[test]
fn identity_conflict_invalidates_every_known_runtime_identity() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    let target = image_record("textures/conflict.png", stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(target.clone())
        .unwrap();
    let (path_asset_id, stable_asset_id) = {
        let mut server = app.world_mut().unwrap().resource_mut::<AssetServer>();
        let path_asset_id = server
            .reserve_record_id(&image_record(
                "textures/conflict.png",
                dependent_stable_id(),
            ))
            .unwrap();
        let stable_asset_id = server
            .reserve_record_id(&image_record("textures/other.png", stable_id()))
            .unwrap();
        (path_asset_id, stable_asset_id)
    };
    assert_ne!(path_asset_id, stable_asset_id);
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(target.path().clone())
        .unwrap();

    app.update().unwrap();

    let states = app.world().resource::<AssetStates>();
    for asset_id in [path_asset_id, stable_asset_id] {
        assert!(matches!(
            states.state(asset_id).unwrap().load_state(),
            LoadState::Failed { message }
                if message == "asset.reload-source-change-resolve-failed"
        ));
    }
    let rejected = app
        .world()
        .resource::<AssetEvents>()
        .iter()
        .filter(|event| event.kind() == AssetEventKind::ReloadRejected)
        .map(|event| event.id())
        .collect::<BTreeSet<_>>();
    assert_eq!(rejected, BTreeSet::from([path_asset_id, stable_asset_id]));
    assert!(app.world().resource::<AssetReloadRequests>().is_empty());
}

#[test]
fn dependency_records_enqueue_dependent_assets() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    {
        let mut database = app
            .world_mut()
            .unwrap()
            .resource_mut::<ProjectAssetDatabase>();
        database
            .insert(image_record("textures/source.png", stable_id()))
            .unwrap();
        database
            .insert(image_record(
                "textures/dependent.png",
                dependent_stable_id(),
            ))
            .unwrap();
    }
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetDependencyGraph>()
        .add_source_dependency(stable_id(), dependent_stable_id());
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(AssetPath::new("textures/source.png").unwrap())
        .unwrap();

    app.update().unwrap();

    let paths = app
        .world()
        .resource::<CapturedReloadRequests>()
        .0
        .iter()
        .map(|request| request.path().as_str().to_string())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec!["textures/source.png", "textures/dependent.png"]);
}

#[test]
fn dependency_records_enqueue_transitive_dependents_once() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    {
        let mut database = app
            .world_mut()
            .unwrap()
            .resource_mut::<ProjectAssetDatabase>();
        database
            .insert(image_record("textures/source.png", stable_id()))
            .unwrap();
        database
            .insert(image_record(
                "textures/dependent.png",
                dependent_stable_id(),
            ))
            .unwrap();
        database
            .insert(image_record(
                "textures/transitive.png",
                transitive_dependent_stable_id(),
            ))
            .unwrap();
    }
    {
        let mut graph = app
            .world_mut()
            .unwrap()
            .resource_mut::<AssetDependencyGraph>();
        graph.add_source_dependency(stable_id(), dependent_stable_id());
        graph.add_source_dependency(dependent_stable_id(), transitive_dependent_stable_id());
        graph.add_source_dependency(stable_id(), transitive_dependent_stable_id());
    }
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(AssetPath::new("textures/source.png").unwrap())
        .unwrap();

    app.update().unwrap();

    let paths = app
        .world()
        .resource::<CapturedReloadRequests>()
        .0
        .iter()
        .map(|request| request.path().as_str().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            "textures/source.png",
            "textures/dependent.png",
            "textures/transitive.png"
        ]
    );
}

#[test]
fn coalesced_sources_enqueue_a_shared_dependent_once() {
    let mut app = App::new();
    install_asset_plugin(&mut app);
    install_test_reload_consumer(&mut app);
    let first = image_record("textures/a.png", stable_id());
    let second = image_record("textures/b.png", alternate_source_stable_id());
    let dependent = image_record("textures/shared.png", dependent_stable_id());
    {
        let mut database = app
            .world_mut()
            .unwrap()
            .resource_mut::<ProjectAssetDatabase>();
        database.insert(first.clone()).unwrap();
        database.insert(second.clone()).unwrap();
        database.insert(dependent.clone()).unwrap();
    }
    {
        let mut graph = app
            .world_mut()
            .unwrap()
            .resource_mut::<AssetDependencyGraph>();
        graph.add_source_dependency(first.stable_id(), dependent.stable_id());
        graph.add_source_dependency(second.stable_id(), dependent.stable_id());
    }
    {
        let mut changes = app
            .world_mut()
            .unwrap()
            .resource_mut::<AssetSourceChanges>();
        changes.modified(first.path().clone()).unwrap();
        changes.modified(second.path().clone()).unwrap();
    }

    app.update().unwrap();

    let captured = app.world().resource::<CapturedReloadRequests>();
    assert_eq!(
        captured
            .0
            .iter()
            .filter(|request| request.stable_id() == dependent.stable_id())
            .count(),
        1
    );
    let dependent_id = captured
        .0
        .iter()
        .find(|request| request.stable_id() == dependent.stable_id())
        .unwrap()
        .asset_id();
    assert_eq!(
        app.world()
            .resource::<AssetLoadGenerations>()
            .current(dependent_id)
            .raw(),
        1
    );
}
