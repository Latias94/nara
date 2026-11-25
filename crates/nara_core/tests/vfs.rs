use std::io::ErrorKind;

use nara_core::vfs::{AssetPath, AssetSource, MemoryAssetSource};

const FILES: &[(&str, &[u8])] = &[
    ("assets/sub/file.txt", b"nested"),
    ("assets/sub/second.txt", b"second"),
    ("assets/sub-item.txt", b"sibling"),
    ("assets/美術.txt", b"utf8"),
    ("plain.txt", b"root"),
];

fn path(value: &str) -> AssetPath {
    AssetPath::new(value).unwrap()
}

fn assert_source_contract(source: &dyn AssetSource) {
    for (name, bytes) in FILES {
        let name = path(name);
        assert!(source.exists(&name));
        assert_eq!(source.read_bytes(&name).unwrap(), *bytes);
    }
    assert!(source.exists(&AssetPath::ROOT));
    assert_eq!(
        source.list_dir(&AssetPath::ROOT).unwrap(),
        [path("assets"), path("plain.txt")]
    );
    assert!(source.exists(&path("assets")));
    assert_eq!(
        source.list_dir(&path("assets")).unwrap(),
        [
            path("assets/sub"),
            path("assets/sub-item.txt"),
            path("assets/美術.txt")
        ],
    );
    assert_eq!(
        source.read_bytes(&AssetPath::ROOT).unwrap_err().kind(),
        ErrorKind::IsADirectory
    );
    assert_eq!(
        source.list_dir(&path("plain.txt")).unwrap_err().kind(),
        ErrorKind::NotADirectory
    );
    let missing = path("asset");
    assert!(!source.exists(&missing));
    assert_eq!(
        source.read_bytes(&missing).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        source.list_dir(&missing).unwrap_err().kind(),
        ErrorKind::NotFound
    );
}

#[test]
fn portable_paths_reject_escape_and_windows_alias_spelling() {
    for invalid in [
        "",
        "/absolute",
        "drive:C",
        "nested\\entry",
        "nested//entry",
        "nested/",
        "./entry",
        "nested/../entry",
        "nested/.. /entry",
        "nested/trailing.",
        "nested/trailing ",
        "nested/a\0b",
        "nested/NUL.txt",
        "nested/COM¹.png",
        "nested/pipe|name",
    ] {
        assert_eq!(
            AssetPath::new(invalid).unwrap_err().kind(),
            ErrorKind::InvalidInput,
            "{invalid}"
        );
    }
    assert_eq!(
        path("Assets/.hidden/美術.txt").as_str(),
        "Assets/.hidden/美術.txt"
    );
}

#[test]
fn memory_files_replace_without_colliding_with_directories() {
    let mut source = MemoryAssetSource::new();
    assert!(source.list_dir(&AssetPath::ROOT).unwrap().is_empty());
    for (name, bytes) in FILES {
        source.insert(&path(name), *bytes).unwrap();
    }
    assert_eq!(
        source.insert(&path("assets/sub"), []).unwrap_err().kind(),
        ErrorKind::IsADirectory
    );
    assert_eq!(
        source
            .insert(&path("plain.txt/child"), [])
            .unwrap_err()
            .kind(),
        ErrorKind::NotADirectory
    );
    assert_source_contract(&source);
    source
        .insert(&path("plain.txt"), b"updated".to_vec())
        .unwrap();
    assert_eq!(source.read_bytes(&path("plain.txt")).unwrap(), b"updated");
}

#[cfg(any(unix, windows))]
mod native {
    use std::{fs, path::Path};

    use nara_core::vfs::{DirAssetSource, write_atomic};

    use super::*;

    #[test]
    fn directory_source_has_the_same_read_and_listing_contract() {
        let root = tempfile::tempdir().unwrap();
        for (name, bytes) in FILES {
            let file = root.path().join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        let source = DirAssetSource::new(root.path()).unwrap();
        assert_source_contract(&source);
    }

    #[cfg(unix)]
    #[test]
    fn development_symlinks_must_resolve_inside_the_source_root() {
        use std::os::unix::fs::symlink;

        let sandbox = tempfile::tempdir().unwrap();
        let root = sandbox.path().join("assets");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("inside.txt"), b"inside").unwrap();
        fs::write(sandbox.path().join("outside.txt"), b"outside").unwrap();
        symlink("inside.txt", root.join("alias.txt")).unwrap();
        symlink("../outside.txt", root.join("escape.txt")).unwrap();

        let source = DirAssetSource::new(&root).unwrap();
        assert_eq!(source.read_bytes(&path("alias.txt")).unwrap(), b"inside");
        assert_eq!(
            source.read_bytes(&path("escape.txt")).unwrap_err().kind(),
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            source.list_dir(&AssetPath::ROOT).unwrap(),
            [path("alias.txt"), path("inside.txt")]
        );
    }

    #[test]
    fn atomic_write_creates_and_replaces_and_cleans_up_after_failure() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("content.tmp");
        write_atomic(&target, b"initial contents").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"initial contents");
        write_atomic(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");

        let directory = root.path().join("directory");
        fs::create_dir(&directory).unwrap();
        assert!(write_atomic(&directory, b"invalid replacement").is_err());
        assert!(directory.is_dir());
        let mut invalid = target.as_os_str().to_owned();
        invalid.push("\0suffix");
        assert_eq!(
            write_atomic(Path::new(&invalid), b"invalid filename")
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidInput
        );
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn an_open_windows_target_can_reject_replacement_without_losing_bytes() {
        use std::os::windows::fs::OpenOptionsExt;

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("content.txt");
        fs::write(&target, b"previous").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        assert!(write_atomic(&target, b"replacement").is_err());
        drop(held);
        assert_eq!(fs::read(&target).unwrap(), b"previous");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
