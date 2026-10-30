//! Independent source-location consumption through public asset APIs.
//!
//! This exercises source identity, authored aliases and scoped directory reads. Source Scene
//! dependency loading and Door travel have separate qualification.

use std::{fs, path::Path};

use nara_asset::{
    AssetDatabaseError, AssetMetaIdentity, AssetPath, AssetPathScope, AssetProductRef, AssetRef,
    AssetRefError, AssetRefExportPolicy, AssetServer, AssetSourceKind, AssetSourcePath,
    AssetSourceRoot, ImportedProductId, ProjectAssetDatabase, StableAssetId,
};
use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_image::{
    ImageImportError, ImageImportStage, ImageSourceDirectory, ImageSourceFailureKind,
};

const FIRST: &str = "0d486b57-3040-409f-b4b2-97d85221a6b5";
const SECOND: &str = "bb700acc-4345-472a-8a96-0f965cbf799a";
const THIRD: &str = "8c062e79-00f0-4238-9df3-72715dd6a20c";
const FOURTH: &str = "f07c62e9-11d3-43e6-9b05-809bfe089606";

fn scope(prefix: &str) -> AssetPathScope {
    AssetPathScope::under(AssetSourcePath::new(prefix).unwrap())
}

fn identity(id: &str, path: &str, kind: AssetSourceKind) -> AssetMetaIdentity {
    AssetMetaIdentity::new(
        StableAssetId::parse_str(id).unwrap(),
        AssetPath::new(path).unwrap(),
        kind,
    )
}

#[test]
fn same_local_name_resolves_independently_across_document_and_resource_roots() {
    let roots = [
        (scope("Content/Art"), FIRST, AssetSourceKind::Image),
        (scope("Content/Blueprints"), SECOND, AssetSourceKind::Prefab),
        (scope("Content/Levels"), THIRD, AssetSourceKind::Scene),
        (scope("Packages/Art"), FOURTH, AssetSourceKind::Image),
    ];
    let local = AssetPath::new("shared.json").unwrap();
    let alias = AssetRef::Path(local.clone());
    let mut database = ProjectAssetDatabase::default();
    for (scope, id, kind) in &roots {
        let metadata = identity(id, local.as_str(), kind.clone());
        assert_eq!(metadata.path(), &local);
        let record = metadata.clone().into_record(scope);
        assert_eq!(record.path(), &scope.resolve(&local));
        database
            .insert_identity(scope, local.clone(), metadata)
            .unwrap();
        assert_eq!(database.record_for_path(record.path()), Some(&record));
    }
    assert_eq!(database.len(), 4);
    assert!(
        database
            .resolver(&AssetPathScope::ROOT)
            .resolve_ref(&alias)
            .is_err()
    );

    let mut server = AssetServer::new();
    let mut handles = Vec::new();
    for (scope, id, kind) in &roots {
        let resolver = database.resolver(scope);
        let stable = StableAssetId::parse_str(id).unwrap();
        let primary = AssetProductRef::new(stable, ImportedProductId::PRIMARY).unwrap();
        let record = resolver.resolve_ref(&alias).unwrap();
        assert_eq!(record.stable_id(), stable);
        assert_eq!(record.source_kind(), kind);
        assert!(std::ptr::eq(
            record,
            resolver.resolve_ref(&AssetRef::StableId(stable)).unwrap(),
        ));
        let handle = alias
            .resolve_with_database_as::<String>(&mut server, resolver, kind)
            .unwrap();
        for reference in [AssetRef::StableId(stable), AssetRef::Product(primary)] {
            assert_eq!(
                reference
                    .resolve_with_database::<String>(&mut server, resolver)
                    .unwrap(),
                handle,
            );
        }
        assert_eq!(server.path(handle.id()), Some(record.path()));
        assert_eq!(server.product(handle.id()), Some(primary));
        assert_eq!(
            AssetRef::from_handle(&server, handle, scope).unwrap(),
            alias
        );
        assert_eq!(
            AssetRef::from_handle_with_policy(
                &server,
                handle,
                scope,
                AssetRefExportPolicy::StableIdWhenKnown,
            )
            .unwrap(),
            AssetRef::StableId(stable),
        );

        let exported = AssetRef::from_handle(&server, handle, scope).unwrap();
        let encoded = serde_json::to_string(&exported).unwrap();
        assert!(!encoded.contains(scope.prefix().unwrap().as_str()));
        let reopened: AssetRef = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            reopened
                .resolve_with_database::<String>(&mut server, resolver)
                .unwrap(),
            handle,
        );
        handles.push(handle);
    }
    for (index, handle) in handles.iter().enumerate() {
        assert!(!handles[..index].contains(handle));
    }

    let overlap = scope("Content");
    let overlapping_alias = AssetRef::path("Art/shared.json").unwrap();
    assert!(std::ptr::eq(
        database
            .resolver(&overlap)
            .resolve_ref(&overlapping_alias)
            .unwrap(),
        database.resolver(&roots[0].0).resolve_ref(&alias).unwrap(),
    ));
    assert_eq!(
        AssetRef::from_handle(&server, handles[0], &overlap).unwrap(),
        overlapping_alias,
    );
    let cross_root = AssetRef::from_handle(&server, handles[0], &roots[3].0).unwrap();
    assert_eq!(
        cross_root,
        AssetRef::Product(server.product(handles[0].id()).unwrap()),
    );
    assert_eq!(
        cross_root
            .resolve_with_database::<String>(&mut server, database.resolver(&roots[3].0))
            .unwrap(),
        handles[0],
    );
    let before = server.clone();
    assert!(matches!(
        alias.resolve_with_database_as::<String>(
            &mut server,
            database.resolver(&roots[1].0),
            &AssetSourceKind::Image,
        ),
        Err(AssetRefError::UnexpectedSourceKind { .. }),
    ));
    assert_eq!(server, before);
}

#[test]
fn wrong_scopes_refuse_alias_resolution_path_only_export_and_native_path_derivation() {
    let assets = scope("Content/Art");
    let unrelated = scope("Content/ArtExtra");
    let wrong_case = scope("content/Art");
    let local = AssetPath::new("textures/shared.png").unwrap();
    let alias = AssetRef::Path(local.clone());
    let canonical = assets.resolve(&local);
    let mut database = ProjectAssetDatabase::default();
    database
        .insert_identity(
            &assets,
            local.clone(),
            identity(FIRST, local.as_str(), AssetSourceKind::Image),
        )
        .unwrap();
    let mut server = AssetServer::new();
    let handle = alias.resolve::<String>(&mut server, &assets).unwrap();
    assert_eq!(server.path(handle.id()), Some(&canonical));
    assert_eq!(server.product(handle.id()), None);
    assert_eq!(
        AssetRef::from_handle(&server, handle, &assets).unwrap(),
        alias
    );

    for wrong in [&unrelated, &wrong_case] {
        let before = server.clone();
        assert!(matches!(
            alias.resolve_with_database::<String>(&mut server, database.resolver(wrong)),
            Err(AssetRefError::Database(AssetDatabaseError::UnknownPath(path)))
                if path == wrong.resolve(&local),
        ));
        assert_eq!(server, before);
        assert_eq!(wrong.relativize(&canonical), None);
        assert!(matches!(
            AssetRef::from_handle(&server, handle, wrong),
            Err(AssetRefError::SourceOutsideScope(path)) if path == canonical,
        ));
        let source = AssetSourceRoot::new(
            std::env::temp_dir().join("nara-source-locations"),
            wrong.clone(),
        );
        assert!(matches!(
            source.source_path(&canonical),
            Err(AssetDatabaseError::SourceOutsideRoot),
        ));
    }
    assert_eq!(
        assets.relativize(&AssetSourcePath::new("Content/ArtExtra/shared.png").unwrap()),
        None,
    );
    assert_eq!(
        assets.relativize(&AssetSourcePath::new("Content/Art").unwrap()),
        None,
    );
    let native = std::env::temp_dir().join("nara-source-locations");
    let source = AssetSourceRoot::new(&native, assets);
    assert_eq!(
        source.source_path(&canonical).unwrap(),
        native.join("textures").join("shared.png"),
    );
}

#[test]
fn duplicate_uuids_case_aliases_and_sidecar_mismatches_preserve_the_catalogue() {
    let assets = scope("Content/Art");
    let prefabs = scope("Content/Blueprints");
    let local = AssetPath::new("shared.json").unwrap();
    let mut database = ProjectAssetDatabase::default();
    database
        .insert_identity(
            &assets,
            local.clone(),
            identity(FIRST, local.as_str(), AssetSourceKind::Image),
        )
        .unwrap();
    let before = database.clone();
    assert!(matches!(
        database.insert_identity(
            &prefabs,
            local.clone(),
            identity(FIRST, local.as_str(), AssetSourceKind::Prefab),
        ),
        Err(AssetDatabaseError::DuplicateStableId { stable_id, existing_path, new_path })
            if stable_id == StableAssetId::parse_str(FIRST).unwrap()
                && existing_path == assets.resolve(&local)
                && new_path == prefabs.resolve(&local),
    ));
    assert_eq!(database, before);

    let wrong_case = scope("content/Art");
    for (scope, spelling) in [(&assets, "Shared.json"), (&wrong_case, "shared.json")] {
        let path = AssetPath::new(spelling).unwrap();
        assert!(matches!(
            database.insert_identity(
                scope,
                path,
                identity(SECOND, spelling, AssetSourceKind::Image),
            ),
            Err(AssetDatabaseError::CaseInsensitivePathCollision { .. }),
        ));
        assert_eq!(database, before);
    }
    assert!(matches!(
        database.insert_identity(
            &assets,
            AssetPath::new("renamed.json").unwrap(),
            identity(SECOND, local.as_str(), AssetSourceKind::Image),
        ),
        Err(AssetDatabaseError::MetaPathMismatch { .. }),
    ));
    assert_eq!(database, before);
    assert_eq!(
        database
            .record_for_stable_id(StableAssetId::parse_str(FIRST).unwrap())
            .unwrap()
            .path(),
        &assets.resolve(&local),
    );
}

#[test]
fn narrow_image_directories_read_their_local_files_and_refuse_other_source_roots() {
    let temporary = tempfile::tempdir().unwrap();
    let first_root = temporary.path().join("first-native-root");
    let second_root = temporary.path().join("second-native-root");
    let first_scope = scope("Content/Art");
    let second_scope = scope("Packages/Art");
    let local = "textures/player.png";
    let first_bytes = include_bytes!("../../../../reference-game/assets/textures/player.png");
    let second_bytes =
        include_bytes!("../../../../reference-game/assets/textures/tiny-dungeon.png");
    for (root, bytes) in [
        (&first_root, first_bytes.as_slice()),
        (&second_root, second_bytes.as_slice()),
    ] {
        fs::create_dir_all(root.join("textures")).unwrap();
        fs::write(root.join(local), bytes).unwrap();
    }
    let first = identity(FIRST, local, AssetSourceKind::Image).into_record(&first_scope);
    let second = identity(SECOND, local, AssetSourceKind::Image).into_record(&second_scope);
    let first_directory = image_directory(&first_root, first_scope);
    let second_directory = image_directory(&second_root, second_scope);
    assert_eq!(
        first_directory
            .open(&first)
            .unwrap()
            .read_to_end_bounded(first_bytes.len() as u64)
            .unwrap(),
        first_bytes.as_slice(),
    );
    assert_eq!(
        second_directory
            .open(&second)
            .unwrap()
            .read_to_end_bounded(second_bytes.len() as u64)
            .unwrap(),
        second_bytes.as_slice(),
    );
    for (directory, outside) in [(&first_directory, &second), (&second_directory, &first)] {
        assert!(matches!(
            directory.open(outside),
            Err(ImageImportError::Source {
                stage: ImageImportStage::SourceOpen,
                kind: ImageSourceFailureKind::AuthorityRejected,
            }),
        ));
    }
}

fn image_directory(path: &Path, scope: AssetPathScope) -> ImageSourceDirectory {
    #[cfg(windows)]
    let handle = {
        use std::os::windows::fs::OpenOptionsExt;

        fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    };
    #[cfg(unix)]
    let handle = fs::File::open(path).unwrap();
    let directory = DirectoryCapability::from_host_handle(
        handle,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap();
    ImageSourceDirectory::new(directory, scope)
}
