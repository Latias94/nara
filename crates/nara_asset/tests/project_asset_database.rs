#![cfg(all(feature = "authoring", any(unix, windows)))]

use std::{fs, path::Path};

use nara_asset::{
    AssetDatabaseError, AssetMeta, AssetMetaError, AssetPath, AssetPathScope, AssetRef,
    AssetScanError, AssetScanMode, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase,
    StableAssetId,
    source::{ProjectAssetSource, asset_id_path, read_asset},
};
use nara_core::vfs::{AssetPath as VfsPath, AssetSource, DirAssetSource};
use serde::Deserialize;

const FIXTURE_ID: &str = "0193a100-1111-7000-8000-000000000001";

fn path(value: &str) -> AssetSourcePath {
    AssetSourcePath::new(value).unwrap()
}

fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, bytes).unwrap();
}

fn scan(root: &Path, mode: AssetScanMode) -> Result<ProjectAssetDatabase, AssetScanError> {
    ProjectAssetDatabase::scan(root, &[path("assets")], mode)
}

fn fixture_meta(importer: &str) -> String {
    format!("AssetMeta(id: \"{FIXTURE_ID}\", importer: \"{importer}\", settings: ())\n")
}

#[test]
fn discovery_does_not_create_sidecars_in_a_hidden_mount() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), ".git/HEAD", b"ref: refs/heads/main\n");
    assert!(matches!(
        ProjectAssetDatabase::scan(root.path(), &[path(".git")], AssetScanMode::Discover),
        Err(AssetScanError::InvalidEntry { .. }),
    ));
    assert!(!root.path().join(".git/HEAD.meta").exists());
}

#[test]
fn discovery_indexes_project_paths_and_moving_the_sidecar_preserves_identity() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    for source in [
        "assets/art/ship.PNG",
        "assets/levels/main.nara.scene.ron",
        "assets/levels/props.prefab.ron",
        "assets/paint.material.ron",
        "assets/program.WGSL",
        "shared/beep.ogg",
    ] {
        write(root, source, []);
    }
    write(root, "assets/.cache/ignored.png", []);
    write(root, "outside.png", []);

    let original = path("assets/art/ship.PNG");
    assert!(matches!(
        scan(root, AssetScanMode::ReadOnly),
        Err(AssetScanError::MissingSidecar { path }) if path == original
    ));
    assert!(!root.join(original.meta_path()).exists());

    let roots = [
        path("shared"),
        path("assets/art"),
        path("assets"),
        path("missing"),
    ];
    let discovered = ProjectAssetDatabase::scan(root, &roots, AssetScanMode::Discover).unwrap();
    let records = discovered
        .records()
        .map(|record| {
            let meta = record.meta().unwrap();
            let _: () = meta.settings().unwrap();
            assert_eq!(meta.id().as_uuid().get_version_num(), 7);
            assert_eq!(
                fs::read_to_string(root.join(record.path().meta_path())).unwrap(),
                meta.as_ron()
            );
            (
                record.path().as_str(),
                meta.importer(),
                record.source_kind().clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        records,
        [
            ("assets/art/ship.PNG", "nara.image", AssetSourceKind::Image),
            (
                "assets/levels/main.nara.scene.ron",
                "nara.scene",
                AssetSourceKind::Scene
            ),
            (
                "assets/levels/props.prefab.ron",
                "nara.prefab",
                AssetSourceKind::Prefab
            ),
            (
                "assets/paint.material.ron",
                "nara.material",
                AssetSourceKind::Other("nara.material".into())
            ),
            (
                "assets/program.WGSL",
                "nara.shader",
                AssetSourceKind::Other("nara.shader".into())
            ),
            (
                "shared/beep.ogg",
                "nara.raw",
                AssetSourceKind::Other("nara.raw".into())
            ),
        ]
    );
    assert!(!root.join("assets/.cache/ignored.png.meta").exists());
    assert!(!root.join("outside.png.meta").exists());
    assert_eq!(
        ProjectAssetDatabase::scan(root, &roots, AssetScanMode::ReadOnly).unwrap(),
        discovered
    );

    let previous = discovered.record_for_path(&original).unwrap();
    let id = previous.stable_id();
    let sidecar = previous.meta().unwrap().as_ron();
    let renamed = path("assets/levels/hero.png");
    fs::rename(root.join(original.as_str()), root.join(renamed.as_str())).unwrap();
    fs::rename(
        root.join(original.meta_path()),
        root.join(renamed.meta_path()),
    )
    .unwrap();
    let reopened = scan(root, AssetScanMode::ReadOnly).unwrap();
    assert!(reopened.record_for_path(&original).is_none());
    let record = reopened.record_for_stable_id(id).unwrap();
    assert_eq!(record.path(), &renamed);
    assert_eq!(record.meta().unwrap().as_ron(), sidecar);
    let scope = AssetPathScope::under(path("assets"));
    let resolver = reopened.resolver(&scope);
    assert_eq!(
        resolver.record_for_path(&AssetPath::new("levels/hero.png").unwrap()),
        Some(record)
    );
    assert!(matches!(
        resolver.resolve_ref(&AssetRef::path("art/ship.PNG").unwrap()),
        Err(AssetDatabaseError::UnknownPath(_))
    ));
    assert_eq!(
        resolver.resolve_ref(&AssetRef::StableId(id)).unwrap(),
        record
    );
}

#[test]
fn directory_id_source_resolves_the_registered_file_in_its_virtual_namespace() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    write(root, "assets/ship.png", b"image-source");
    write(root, "assets/ship.png.meta", fixture_meta("nara.image"));
    let id = FIXTURE_ID.parse().unwrap();
    let directory = DirAssetSource::new(root).unwrap();
    let database = scan(root, AssetScanMode::ReadOnly).unwrap();
    let source = ProjectAssetSource::new(&directory, &database);
    assert_eq!(read_asset(&source, id).unwrap(), b"image-source");
    assert_eq!(
        source.list_dir(&VfsPath::ROOT).unwrap(),
        [asset_id_path(id)]
    );
    assert!(!source.exists(&VfsPath::new("assets/ship.png").unwrap()));
    assert!(!source.exists(&VfsPath::new(FIXTURE_ID.replace('-', "")).unwrap()));

    for suffix in ["", ".meta"] {
        fs::rename(
            root.join(format!("assets/ship.png{suffix}")),
            root.join(format!("assets/moved.png{suffix}")),
        )
        .unwrap();
    }
    assert!(!source.exists(&asset_id_path(id)));
    let database = scan(root, AssetScanMode::ReadOnly).unwrap();
    let source = ProjectAssetSource::new(&directory, &database);
    assert_eq!(read_asset(&source, id).unwrap(), b"image-source");
}

#[test]
fn a_custom_importer_keeps_exact_ron_and_its_enum_settings() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Settings {
        srgb: bool,
        filter: Filter,
        mipmaps: bool,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    enum Filter {
        Nearest,
    }

    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    write(root, "assets/sprite.png", []);
    let sidecar = format!(
        r#"// Import defaults for the sprite atlas.
AssetMeta(
    id: "{FIXTURE_ID}",
    importer: "studio.sprite-atlas",
    settings: (
        srgb: true,
        filter: Nearest,
        mipmaps: false,
    ),
)
"#
    );
    write(root, "assets/sprite.png.meta", &sidecar);

    let database = scan(root, AssetScanMode::Discover).unwrap();
    let record = database
        .record_for_path(&path("assets/sprite.png"))
        .unwrap();
    assert_eq!(
        record.source_kind(),
        &AssetSourceKind::Other("studio.sprite-atlas".into())
    );
    let meta = record.meta().unwrap();
    assert_eq!(meta.id(), FIXTURE_ID.parse::<StableAssetId>().unwrap());
    assert_eq!(meta.as_ron(), sidecar);
    assert_eq!(
        meta.settings::<Settings>().unwrap(),
        Settings {
            srgb: true,
            filter: Filter::Nearest,
            mipmaps: false,
        }
    );
    assert_eq!(
        fs::read_to_string(root.join("assets/sprite.png.meta")).unwrap(),
        sidecar
    );
}

#[test]
fn duplicate_ids_fail_before_creating_any_missing_sidecars() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    for name in [
        "assets/first.png",
        "assets/second.png",
        "assets/000-new.png",
    ] {
        write(root, name, []);
    }
    for name in ["assets/first.png.meta", "assets/second.png.meta"] {
        write(root, name, fixture_meta("nara.image"));
    }
    for mode in [AssetScanMode::ReadOnly, AssetScanMode::Discover] {
        assert!(matches!(
            scan(root, mode),
            Err(AssetScanError::Database(AssetDatabaseError::DuplicateStableId {
                stable_id,
                existing_path,
                new_path,
            })) if stable_id.to_string() == FIXTURE_ID
                && existing_path.as_str() == "assets/first.png"
                && new_path.as_str() == "assets/second.png"
        ));
    }
    assert!(!root.join("assets/000-new.png.meta").exists());
}

#[test]
fn invalid_sidecars_are_never_replaced_or_skipped() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    write(root, "assets/first.png", []);
    write(root, "assets/second.png", []);
    for (bytes, expected_error) in [
        (b"\xff".to_vec(), "not UTF-8"),
        (vec![b' '; 65_537], "exceeds 64 KiB"),
        (b"AssetMeta(settings: ())".to_vec(), "invalid RON"),
        (
            fixture_meta("nara.image")
                .replace(FIXTURE_ID, "00000000-0000-0000-0000-000000000000")
                .into_bytes(),
            "must not be nil",
        ),
    ] {
        write(root, "assets/second.png.meta", &bytes);
        let error = scan(root, AssetScanMode::Discover).unwrap_err();
        assert!(
            matches!(error, AssetScanError::InvalidSidecar { ref path, .. }
                if path.as_str() == "assets/second.png.meta")
        );
        assert!(error.to_string().contains(expected_error), "{error}");
        assert_eq!(
            fs::read(root.join("assets/second.png.meta")).unwrap(),
            bytes
        );
        assert!(!root.join("assets/first.png.meta").exists());
    }
    assert!(matches!(
        AssetMeta::from_ron(fixture_meta("not an importer")),
        Err(AssetMetaError::InvalidImporter)
    ));
}

#[test]
fn a_sidecar_without_an_exact_source_is_orphaned() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    write(root, "assets/ship.png", []);
    write(root, "assets/Ship.png.meta", fixture_meta("nara.image"));
    assert!(matches!(
        scan(root, AssetScanMode::Discover),
        Err(AssetScanError::OrphanSidecar { path }) if path.as_str() == "assets/Ship.png.meta"
    ));
    assert_eq!(fs::read_dir(root.join("assets")).unwrap().count(), 2);
}

#[cfg(target_os = "linux")]
#[test]
fn case_collisions_are_rejected_even_without_sidecars() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    write(root, "assets/Ship/art.png", []);
    write(root, "assets/ship/tiles.png", []);
    assert!(matches!(
        scan(root, AssetScanMode::Discover),
        Err(AssetScanError::Database(
            AssetDatabaseError::CaseInsensitivePathCollision { existing_path, new_path }
        )) if existing_path.as_str() == "assets/Ship" && new_path.as_str() == "assets/ship"
    ));
    assert!(!root.join("assets/Ship/art.png.meta").exists());
    assert!(!root.join("assets/ship/tiles.png.meta").exists());
}

#[cfg(unix)]
#[test]
fn directory_links_are_not_scanned_or_used_for_writes() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "ship.png", []);
    let assets = root.path().join("assets");
    std::os::unix::fs::symlink(outside.path(), &assets).unwrap();
    for mode in [AssetScanMode::Discover, AssetScanMode::ReadOnly] {
        assert!(matches!(
            scan(root.path(), mode),
            Err(AssetScanError::InvalidEntry { path, .. }) if path.ends_with("assets")
        ));
    }
    assert!(!outside.path().join("ship.png.meta").exists());
}
