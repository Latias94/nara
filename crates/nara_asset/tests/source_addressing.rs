use nara_asset::{
    AssetDatabaseError, AssetMetaIdentity, AssetPath, AssetPathError, AssetPathScope,
    AssetProductRef, AssetRecord, AssetRef, AssetRefError, AssetRefExportPolicy,
    AssetReferenceResolver, AssetServer, AssetSourceChangeKind, AssetSourceChanges,
    AssetSourceKind, AssetSourcePath, AssetSourceRoot, ImportedProductId, ProjectAssetDatabase,
    StableAssetId,
};

fn source(path: &str) -> AssetSourcePath {
    AssetSourcePath::new(path).unwrap()
}

fn alias(path: &str) -> AssetPath {
    AssetPath::new(path).unwrap()
}

fn scope(prefix: &str) -> AssetPathScope {
    AssetPathScope::under(source(prefix))
}

fn stable_id(value: u128) -> StableAssetId {
    StableAssetId::from_uuid(uuid::Uuid::from_u128(value))
}

fn record(id: u128, path: &str, kind: AssetSourceKind) -> AssetRecord {
    AssetRecord::new(stable_id(id), source(path), kind)
}

fn primary(record: &AssetRecord) -> AssetProductRef {
    AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY).unwrap()
}

#[test]
fn source_locations_and_authored_aliases_share_the_validated_relative_grammar() {
    for (path, error) in [
        ("", AssetPathError::Empty),
        ("/absolute.png", AssetPathError::Absolute),
        ("textures\\player.png", AssetPathError::ContainsBackslash),
        (
            "C:/textures/player.png",
            AssetPathError::ContainsDrivePrefix,
        ),
        (
            "textures/./player.png",
            AssetPathError::ContainsCurrentDirectory,
        ),
        (
            "textures/../player.png",
            AssetPathError::ContainsParentDirectory,
        ),
        ("textures//player.png", AssetPathError::ContainsEmptySegment),
        ("textures/player.png/", AssetPathError::ContainsEmptySegment),
        ("textures/player\0.png", AssetPathError::ContainsNull),
    ] {
        assert_eq!(AssetPath::new(path), Err(error.clone()));
        assert_eq!(AssetSourcePath::new(path), Err(error));
    }
    let source = source("Content/Images/player.png");
    assert_eq!(source.to_string(), "Content/Images/player.png");
    assert_eq!(source.meta_path(), "Content/Images/player.png.meta");
}

#[cfg(feature = "serde")]
#[test]
fn canonical_source_path_wire_is_a_validated_string_in_json_and_ron() {
    let path = source("Content/Images/player.png");
    for wire in [
        serde_json::to_string(&path).unwrap(),
        ron::to_string(&path).unwrap(),
    ] {
        assert_eq!(wire, r#""Content/Images/player.png""#);
        assert_eq!(
            serde_json::from_str::<AssetSourcePath>(&wire).unwrap(),
            path
        );
        assert_eq!(ron::from_str::<AssetSourcePath>(&wire).unwrap(), path);
    }
    for invalid in [
        "",
        "/absolute.png",
        "textures\\player.png",
        "C:/player.png",
        "textures/./player.png",
        "textures/../player.png",
        "textures//player.png",
        "textures/player.png/",
        "textures/player\0.png",
    ] {
        let json = serde_json::to_string(invalid).unwrap();
        let ron = ron::to_string(invalid).unwrap();
        assert!(serde_json::from_str::<AssetSourcePath>(&json).is_err());
        assert!(ron::from_str::<AssetSourcePath>(&ron).is_err());
    }
    for invalid_shape in ["null", "7", "[]", r#"{"path":"Content/Images/player.png"}"#] {
        assert!(serde_json::from_str::<AssetSourcePath>(invalid_shape).is_err());
    }
}

#[test]
fn scopes_round_trip_aliases_with_exact_case_and_segment_boundaries() {
    let scope = scope("Content/Images");
    let local = alias("textures/player.png");
    let canonical = scope.resolve(&local);
    assert_eq!(scope.prefix(), Some(&source("Content/Images")));
    assert_eq!(canonical, source("Content/Images/textures/player.png"));
    assert_eq!(scope.relativize(&canonical), Some(local));
    let relative = scope.relative_path(&canonical).unwrap();
    assert_eq!(relative, "textures/player.png");
    let offset = scope.prefix().unwrap().as_str().len() + 1;
    assert_eq!(relative.as_ptr(), canonical.as_str()[offset..].as_ptr());

    for outside in [
        "Content/Images",
        "Content/Images-extra/player.png",
        "Content/Images2/player.png",
        "Content/Image/player.png",
        "content/Images/player.png",
        "Content/images/player.png",
        "Other/Content/Images/player.png",
    ] {
        let outside = source(outside);
        assert_eq!(scope.relative_path(&outside), None);
        assert_eq!(scope.relativize(&outside), None);
    }

    assert_eq!(AssetPathScope::ROOT.prefix(), None);
    assert_eq!(
        AssetPathScope::ROOT.resolve(&alias(canonical.as_str())),
        canonical
    );
    assert_eq!(
        AssetPathScope::ROOT.relative_path(&canonical),
        Some(canonical.as_str())
    );
    assert_eq!(
        AssetPathScope::ROOT.relativize(&canonical),
        Some(alias(canonical.as_str()))
    );
}

#[test]
fn canonical_index_distinguishes_same_names_in_multiple_roots_and_same_kind_directories() {
    let mut database = ProjectAssetDatabase::default();
    let records = [
        record(1, "Content/Images/shared.json", AssetSourceKind::Image),
        record(2, "Packages/Images/shared.json", AssetSourceKind::Image),
        record(3, "Templates/shared.json", AssetSourceKind::Prefab),
        record(4, "Worlds/shared.json", AssetSourceKind::Scene),
    ];
    for record in &records {
        database.insert(record.clone()).unwrap();
    }
    let baseline = database.clone();
    let local = alias("shared.json");
    let mut server = AssetServer::new();
    let mut runtime_ids = std::collections::BTreeSet::new();
    for (prefix, record) in ["Content/Images", "Packages/Images", "Templates", "Worlds"]
        .into_iter()
        .zip(&records)
    {
        let scope = scope(prefix);
        let resolver = database.resolver(&scope);
        assert_eq!(resolver.record_for_path(&local), Some(record));
        let reference = AssetRef::Path(local.clone());
        assert_eq!(resolver.resolve_ref(&reference), Ok(record));
        assert_eq!(database.record_for_path(record.path()), Some(record));
        let handle = reference
            .resolve_with_database::<String>(&mut server, resolver)
            .unwrap();
        assert!(runtime_ids.insert(handle.id()));
        assert_eq!(server.path(handle.id()), Some(record.path()));
    }
    assert_eq!(database.record_for_path(&source("shared.json")), None);
    assert_eq!(database, baseline);
    assert_eq!(database.len(), 4);
}

#[test]
fn metadata_binding_checks_actual_local_spelling_before_global_insertion() {
    let scope = scope("Content/Images");
    let identity = AssetMetaIdentity::new(stable_id(1), alias("Icon.png"), AssetSourceKind::Image);
    let expected = record(1, "Content/Images/Icon.png", AssetSourceKind::Image);
    assert_eq!(identity.clone().into_record(&scope), expected);
    let mut database = ProjectAssetDatabase::default();
    let baseline = database.clone();
    for wrong in ["icon.png", "other.png", "Content/Images/Icon.png"] {
        let wrong = alias(wrong);
        assert_eq!(
            database.insert_identity(&scope, wrong.clone(), identity.clone()),
            Err(AssetDatabaseError::MetaPathMismatch {
                source_path: wrong,
                meta_path: identity.path().clone(),
            })
        );
        assert_eq!(database, baseline);
    }
    database
        .insert_identity(&scope, alias("Icon.png"), identity)
        .unwrap();
    assert_eq!(database.record_for_path(expected.path()), Some(&expected));
}

#[test]
fn borrowed_resolvers_keep_explicit_scope_and_snapshot_identity() {
    fn copied<T: Copy>(value: T) -> T {
        value
    }

    let mut database = ProjectAssetDatabase::default();
    let first = record(1, "assets/shared.json", AssetSourceKind::Image);
    let second = record(2, "other-assets/shared.json", AssetSourceKind::Image);
    database.insert(first.clone()).unwrap();
    database.insert(second.clone()).unwrap();
    let snapshot = database.clone();
    assert!(database.remove_exact(&first));
    let asset_scope = scope("assets");
    let other_scope = scope("other-assets");
    let resolver: AssetReferenceResolver<'_> = copied(snapshot.resolver(&asset_scope));
    assert!(std::ptr::eq(resolver.database(), &snapshot));
    assert!(std::ptr::eq(resolver.scope(), &asset_scope));
    assert_eq!(
        resolver.record_for_path(&alias("shared.json")),
        Some(&first)
    );
    assert_eq!(
        database
            .resolver(&asset_scope)
            .record_for_path(&alias("shared.json")),
        None
    );
    assert_eq!(
        snapshot
            .resolver(&other_scope)
            .record_for_path(&alias("shared.json")),
        Some(&second)
    );
    assert_eq!(
        resolver.resolve_ref(&AssetRef::StableId(second.stable_id())),
        Ok(&second)
    );
    let auxiliary = AssetProductRef::new(
        second.stable_id(),
        ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap(),
    )
    .unwrap();
    assert_eq!(
        resolver.resolve_ref(&AssetRef::Product(auxiliary)),
        Ok(&second)
    );
}

#[test]
fn resolver_work_queries_match_scoped_joins_and_borrowed_root_lookups() {
    let image = record(1, "内容/Images/textures/玩家.png", AssetSourceKind::Image);
    let mut database = ProjectAssetDatabase::default();
    database.insert(image.clone()).unwrap();
    let baseline = database.clone();
    let asset_scope = scope("内容/Images");
    let resolver = database.resolver(&asset_scope);
    let path = alias("textures/玩家.png");
    let reference = AssetRef::Path(path.clone());
    let expected = Some(image.path().as_str().len());
    assert_eq!(resolver.path_resolution_work_bytes(&path), expected);
    assert_eq!(resolver.resolution_work_bytes(&reference), expected);
    assert_eq!(resolver.resolve_ref(&reference), Ok(&image));
    assert_eq!(resolver.record_for_path(&path), Some(&image));
    let root_scope = AssetPathScope::ROOT;
    let root = database.resolver(&root_scope);
    let root_path = alias(image.path().as_str());
    assert_eq!(root.path_resolution_work_bytes(&root_path), Some(0));
    assert_eq!(
        root.resolution_work_bytes(&AssetRef::Path(root_path.clone())),
        Some(0)
    );
    assert_eq!(root.record_for_path(&root_path), Some(&image));
    assert_eq!(root.resolve_ref(&AssetRef::Path(root_path)), Ok(&image));
    for reference in [
        AssetRef::StableId(image.stable_id()),
        AssetRef::Product(primary(&image)),
    ] {
        assert_eq!(resolver.resolution_work_bytes(&reference), Some(0));
        assert_eq!(root.resolution_work_bytes(&reference), Some(0));
        assert_eq!(resolver.resolve_ref(&reference), Ok(&image));
    }
    assert_eq!(database, baseline);
}

#[test]
fn scoped_alias_stable_id_and_primary_product_share_one_runtime_identity_in_both_orders() {
    let image = record(
        1,
        "Content/Images/textures/player.png",
        AssetSourceKind::Image,
    );
    let mut database = ProjectAssetDatabase::default();
    database.insert(image.clone()).unwrap();
    let asset_scope = scope("Content/Images");
    let references = [
        AssetRef::path("textures/player.png").unwrap(),
        AssetRef::StableId(image.stable_id()),
        AssetRef::Product(primary(&image)),
    ];
    for path_first in [false, true] {
        let mut server = AssetServer::new();
        let first = if path_first {
            references[0]
                .resolve::<String>(&mut server, &asset_scope)
                .unwrap()
        } else {
            server.reserve_product::<String>(primary(&image)).unwrap()
        };
        for reference in &references {
            assert_eq!(
                reference.resolve_with_database_as::<String>(
                    &mut server,
                    database.resolver(&asset_scope),
                    &AssetSourceKind::Image,
                ),
                Ok(first)
            );
        }
        assert_eq!(server.path(first.id()), Some(image.path()));
        assert_eq!(server.product(first.id()), Some(primary(&image)));
        assert_eq!(server.reserve::<String>(image.path().as_str()), Ok(first));
        assert_eq!(
            AssetRef::from_handle(&server, first, &asset_scope),
            Ok(references[0].clone())
        );
        let overlapping_scope = scope("Content");
        let overlapping = AssetRef::path("Images/textures/player.png").unwrap();
        assert_eq!(
            overlapping.resolve_with_database::<String>(
                &mut server,
                database.resolver(&overlapping_scope),
            ),
            Ok(first)
        );
        assert_eq!(
            AssetRef::from_handle(&server, first, &overlapping_scope),
            Ok(overlapping)
        );
    }
}

#[test]
fn scoped_resolution_refuses_canonical_spellings_as_unrelated_aliases_without_mutation() {
    let image = record(1, "Content/Images/player.png", AssetSourceKind::Image);
    let mut database = ProjectAssetDatabase::default();
    database.insert(image.clone()).unwrap();
    let asset_scope = scope("Content/Images");
    let mut server = AssetServer::new();
    let baseline = server.clone();
    let database_baseline = database.clone();
    let reference = AssetRef::path(image.path().as_str()).unwrap();
    assert_eq!(
        reference.resolve_with_database::<String>(&mut server, database.resolver(&asset_scope)),
        Err(AssetRefError::Database(AssetDatabaseError::UnknownPath(
            source("Content/Images/Content/Images/player.png")
        )))
    );
    for prefix in ["Other/Images", "content/Images", "Content"] {
        let wrong_scope = scope(prefix);
        let local = alias("player.png");
        assert_eq!(
            AssetRef::Path(local.clone())
                .resolve_with_database::<String>(&mut server, database.resolver(&wrong_scope),),
            Err(AssetRefError::Database(AssetDatabaseError::UnknownPath(
                wrong_scope.resolve(&local)
            )))
        );
        assert_eq!(server, baseline);
    }
    assert_eq!(database, database_baseline);
    assert_eq!(server, baseline);
}

#[test]
fn reverse_export_uses_local_aliases_or_complete_products_for_cross_scope_sources() {
    let image = record(1, "Content/Images/player.png", AssetSourceKind::Image);
    let asset_scope = scope("Content/Images");
    let outside_scope = scope("Other/Images");
    let exact_scope = scope("Content/Images/player.png");
    let wrong_case_scope = scope("content/Images");
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<String>(&image).unwrap();
    let baseline = server.clone();
    assert_eq!(
        AssetRef::from_handle(&server, handle, &asset_scope),
        Ok(AssetRef::path("player.png").unwrap())
    );
    for scope in [&outside_scope, &exact_scope, &wrong_case_scope] {
        let exported = AssetRef::from_handle(&server, handle, scope).unwrap();
        assert_eq!(exported, AssetRef::Product(primary(&image)));
        assert_eq!(exported.resolve::<String>(&mut server, scope), Ok(handle));
        assert_eq!(
            AssetRef::from_handle_with_policy(
                &server,
                handle,
                scope,
                AssetRefExportPolicy::StableIdWhenKnown,
            ),
            Ok(AssetRef::StableId(image.stable_id()))
        );
    }
    assert_eq!(server, baseline);
}

#[test]
fn path_only_handles_refuse_unrepresentable_aliases_without_catalogue_mutation() {
    let asset_scope = scope("Content/Images");
    let outside_scope = scope("Other/Images");
    let exact_scope = scope("Content/Images/player.png");
    let mut server = AssetServer::new();
    let handle = server
        .reserve::<String>("Content/Images/player.png")
        .unwrap();
    let baseline = server.clone();
    assert_eq!(
        AssetRef::from_handle(&server, handle, &asset_scope),
        Ok(AssetRef::path("player.png").unwrap())
    );
    for scope in [&outside_scope, &exact_scope] {
        for policy in [
            AssetRefExportPolicy::Path,
            AssetRefExportPolicy::StableIdWhenKnown,
        ] {
            assert_eq!(
                AssetRef::from_handle_with_policy(&server, handle, scope, policy),
                Err(AssetRefError::SourceOutsideScope(source(
                    "Content/Images/player.png"
                )))
            );
        }
    }
    assert_eq!(server, baseline);
    assert_eq!(server.product(handle.id()), None);
}

#[test]
fn duplicate_uuid_and_case_aliases_reject_across_roots_without_mutation() {
    let first = record(1, "Content/Images/Icon.png", AssetSourceKind::Image);
    let mut database = ProjectAssetDatabase::default();
    database.insert(first.clone()).unwrap();
    let baseline = database.clone();
    for rejected in [
        record(1, "Packages/Images/Icon.png", AssetSourceKind::Image),
        record(1, "Worlds/another.json", AssetSourceKind::Scene),
    ] {
        assert_eq!(
            database.insert(rejected.clone()),
            Err(AssetDatabaseError::DuplicateStableId {
                stable_id: first.stable_id(),
                existing_path: first.path().clone(),
                new_path: rejected.path().clone(),
            })
        );
        assert_eq!(database, baseline);
    }
    let wrong_case = record(2, "content/images/icon.png", AssetSourceKind::Image);
    assert_eq!(
        database.insert(wrong_case.clone()),
        Err(AssetDatabaseError::CaseInsensitivePathCollision {
            existing_path: first.path().clone(),
            new_path: wrong_case.path().clone(),
        })
    );
    assert_eq!(database, baseline);
    assert_eq!(
        database.insert(first.clone()),
        Err(AssetDatabaseError::DuplicatePath {
            path: first.path().clone(),
        })
    );
    assert_eq!(database, baseline);
}

#[test]
fn exact_removal_requires_canonical_location_and_complete_record_identity() {
    let own = record(1, "Content/Images/Icon.png", AssetSourceKind::Image);
    let foreign = record(2, "Packages/Images/Icon.png", AssetSourceKind::Image);
    let mut database = ProjectAssetDatabase::default();
    database.insert(own.clone()).unwrap();
    database.insert(foreign.clone()).unwrap();
    let baseline = database.clone();
    for unexpected in [
        record(1, "Packages/Images/Icon.png", AssetSourceKind::Image),
        record(1, "content/images/icon.png", AssetSourceKind::Image),
        record(3, "Content/Images/Icon.png", AssetSourceKind::Image),
        record(1, "Content/Images/Icon.png", AssetSourceKind::Scene),
    ] {
        assert!(!database.remove_exact(&unexpected));
        assert_eq!(database, baseline);
    }
    assert!(database.remove_exact(&own));
    assert!(!database.remove_exact(&own));
    assert_eq!(database.records().collect::<Vec<_>>(), vec![&foreign]);
    assert_eq!(database.record_for_stable_id(own.stable_id()), None);
    let replacement = record(1, "content/images/icon.png", AssetSourceKind::Image);
    database.insert(replacement.clone()).unwrap();
    assert_eq!(
        database.record_for_path(replacement.path()),
        Some(&replacement)
    );
    assert_eq!(baseline.record_for_path(own.path()), Some(&own));
}

#[test]
fn source_roots_refuse_unrelated_prefixes_before_deriving_native_read_paths() {
    let native = std::env::temp_dir().join("nara_source_addressing_no_io");
    let root = AssetSourceRoot::new(&native, scope("Content/Images"));
    assert_eq!(root.scope(), &scope("Content/Images"));
    assert_eq!(
        root.source_path(&source("Content/Images/textures/player.png")),
        Ok(native.join("textures").join("player.png"))
    );
    for outside in [
        "Content/Images",
        "Content/Images-extra/player.png",
        "content/Images/player.png",
        "Other/Images/player.png",
    ] {
        assert_eq!(
            root.source_path(&source(outside)),
            Err(AssetDatabaseError::SourceOutsideRoot)
        );
    }
    let root_scope = AssetSourceRoot::new(&native, AssetPathScope::ROOT);
    assert_eq!(
        root_scope.source_path(&source("Content/Images/player.png")),
        Ok(native.join("Content").join("Images").join("player.png"))
    );
}

#[test]
fn canonical_source_changes_keep_same_named_roots_separate_and_metadata_order_intact() {
    let first = source("Content/Images/shared.png");
    let second = source("Packages/Images/shared.png");
    let mut changes = AssetSourceChanges::new();
    changes.removed(first.clone()).unwrap();
    changes.meta_modified(first.clone()).unwrap();
    changes.modified(second.clone()).unwrap();
    let coalesced = changes.drain_coalesced();
    assert_eq!(coalesced.len(), 2);
    assert_eq!(coalesced[0].path(), &first);
    assert_eq!(coalesced[0].kind(), AssetSourceChangeKind::Removed);
    assert!(coalesced[0].metadata_dirty());
    assert_eq!(coalesced[1].path(), &second);
    assert_eq!(coalesced[1].kind(), AssetSourceChangeKind::Modified);
    assert!(!coalesced[1].metadata_dirty());
    assert!(changes.is_empty());
    assert_eq!(changes.retained_payload_bytes(), 0);
}
