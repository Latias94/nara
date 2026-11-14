use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_core::{ByteLimit, ItemLimit};

use super::*;

#[cfg(any(windows, target_os = "linux"))]
mod native;

#[test]
fn configuration_requires_an_explicit_absolute_root_without_opening_it() {
    for root in ["", ".", "project", "project/assets"] {
        assert_eq!(
            ProjectSceneWatchPlugin::new(root),
            Err(ProjectSceneWatchConfigurationError::AbsoluteRootRequired),
            "{root:?}"
        );
    }
    #[cfg(windows)]
    for root in [r"\project", r"C:project"] {
        assert_eq!(
            ProjectSceneWatchPlugin::new(root),
            Err(ProjectSceneWatchConfigurationError::AbsoluteRootRequired)
        );
    }
    let directory = TestDirectory::new();
    let root = directory.path.join("not-created");
    assert!(!root.exists());
    let configuration = ProjectSceneWatchPlugin::new(root.clone()).unwrap();
    assert_eq!(configuration.root, root);
    assert!(!root.exists());
}

#[test]
fn canonical_configuration_distinguishes_root_and_each_queue_limit() {
    let directory = TestDirectory::new();
    let configuration = ProjectSceneWatchPlugin::new(directory.path.join("project")).unwrap();
    let limits =
        AssetWatchQueueLimits::new(ItemLimit::new(2).unwrap(), ByteLimit::new(4096).unwrap());
    let configuration = configuration.with_queue_limits(limits);
    let bytes = canonical(&configuration);
    assert_eq!(bytes, canonical(&configuration.clone()));
    assert_ne!(
        bytes,
        canonical(
            &ProjectSceneWatchPlugin::new(directory.path.join("other"))
                .unwrap()
                .with_queue_limits(limits)
        )
    );
    assert_ne!(
        bytes,
        canonical(
            &configuration
                .clone()
                .with_queue_limits(AssetWatchQueueLimits::new(
                    ItemLimit::new(3).unwrap(),
                    limits.retained_bytes(),
                ))
        )
    );
    assert_ne!(
        bytes,
        canonical(&configuration.with_queue_limits(AssetWatchQueueLimits::new(
            limits.events(),
            ByteLimit::new(4097).unwrap(),
        )))
    );
}

#[cfg(any(windows, unix))]
#[test]
fn canonical_configuration_preserves_distinct_native_non_unicode_paths() {
    #[cfg(windows)]
    let names = {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        [
            OsString::from_wide(&[0xd800]),
            OsString::from_wide(&[0xd801]),
        ]
    };
    #[cfg(unix)]
    let names = {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        [
            OsString::from_vec(vec![0x80]),
            OsString::from_vec(vec![0x81]),
        ]
    };
    let directory = TestDirectory::new();
    let [first, second] = names.map(|name| directory.path.join(name));
    assert_ne!(first, second);
    assert_eq!(first.to_string_lossy(), second.to_string_lossy());
    assert_ne!(
        canonical(&ProjectSceneWatchPlugin::new(first).unwrap()),
        canonical(&ProjectSceneWatchPlugin::new(second).unwrap())
    );
}

#[test]
fn coverage_deduplicates_and_tracks_missing_directories_without_recursive_project_root() {
    let directory = TestDirectory::new();
    let root = &directory.path;
    fs::create_dir_all(root.join("assets/textures")).unwrap();
    fs::create_dir(root.join("future")).unwrap();
    fs::create_dir(root.join("schema")).unwrap();
    fs::create_dir_all(root.join("target/debug/incremental")).unwrap();
    let desired = [
        AssetWatchPath::new(root.clone(), false),
        AssetWatchPath::new(root.join("assets/textures"), false),
        AssetWatchPath::new(root.join("assets"), true),
        AssetWatchPath::new(root.join("assets/textures"), true),
        AssetWatchPath::new(root.join("assets"), false),
        AssetWatchPath::new(root.join("future/scenes/nested"), true),
        AssetWatchPath::new(root.join("future/prefabs"), true),
        AssetWatchPath::new(root.join("missing/assets"), true),
        AssetWatchPath::new(root.join("schema"), false),
        AssetWatchPath::new(root.join("schema"), false),
    ];
    assert_eq!(
        coverage_paths(root, &desired).unwrap(),
        vec![
            AssetWatchPath::new(root.clone(), false),
            AssetWatchPath::new(root.join("assets"), true),
            AssetWatchPath::new(root.join("future"), false),
            AssetWatchPath::new(root.join("schema"), false),
        ]
    );
    fs::create_dir_all(root.join("future/scenes")).unwrap();
    let partial = coverage_paths(root, &desired).unwrap();
    assert!(partial.contains(&AssetWatchPath::new(root.join("future/scenes"), false)));
    fs::create_dir(root.join("future/scenes/nested")).unwrap();
    let complete = coverage_paths(root, &desired).unwrap();
    assert!(complete.contains(&AssetWatchPath::new(root.join("future/scenes"), false)));
    assert!(complete.contains(&AssetWatchPath::new(
        root.join("future/scenes/nested"),
        true
    )));
    for paths in [&partial, &complete] {
        assert!(paths.contains(&AssetWatchPath::new(root.clone(), false)));
        assert!(
            !paths
                .iter()
                .any(|path| path.path() == root && path.recursive())
        );
        assert!(
            !paths
                .iter()
                .any(|path| path.path().starts_with(root.join("target")))
        );
    }
    assert!(
        coverage_paths(
            root,
            &[AssetWatchPath::new(root.parent().unwrap().to_owned(), true)]
        )
        .is_err()
    );
}

#[test]
fn existing_nested_scopes_cover_each_uncovered_ancestor_nonrecursively() {
    let directory = TestDirectory::new();
    let root = &directory.path;
    fs::create_dir_all(root.join("content/source/scenes")).unwrap();
    fs::create_dir_all(root.join("content/source/prefabs")).unwrap();
    fs::create_dir_all(root.join("config/controls/player")).unwrap();
    let desired = [
        AssetWatchPath::new(root.join("content/source/scenes"), true),
        AssetWatchPath::new(root.join("content/source/scenes/levels"), true),
        AssetWatchPath::new(root.join("content/source/prefabs"), true),
        AssetWatchPath::new(root.join("config/controls/player"), false),
    ];
    assert_eq!(
        coverage_paths(root, &desired).unwrap(),
        vec![
            AssetWatchPath::new(root.clone(), false),
            AssetWatchPath::new(root.join("content"), false),
            AssetWatchPath::new(root.join("content/source"), false),
            AssetWatchPath::new(root.join("content/source/scenes"), true),
            AssetWatchPath::new(root.join("content/source/prefabs"), true),
            AssetWatchPath::new(root.join("config"), false),
            AssetWatchPath::new(root.join("config/controls"), false),
            AssetWatchPath::new(root.join("config/controls/player"), false),
        ]
    );
}

#[test]
fn coverage_accepts_128_native_paths_and_refuses_the_next_uncovered_path() {
    let directory = TestDirectory::new();
    let root = &directory.path;
    let mut desired = Vec::new();
    for index in 0..127 {
        let path = root.join(format!("source-{index}"));
        fs::create_dir(&path).unwrap();
        desired.push(AssetWatchPath::new(path, true));
    }
    assert_eq!(coverage_paths(root, &desired).unwrap().len(), 128);
    let additional = root.join("one-too-many");
    fs::create_dir(&additional).unwrap();
    desired.push(AssetWatchPath::new(additional, true));
    assert!(
        coverage_paths(root, &desired)
            .unwrap_err()
            .contains("128 native paths")
    );
}

fn canonical(configuration: &ProjectSceneWatchPlugin) -> Vec<u8> {
    let mut bytes = Vec::new();
    configuration.write_canonical(&mut bytes);
    bytes
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nara_scene_watch_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
