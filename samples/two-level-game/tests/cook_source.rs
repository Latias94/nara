#[path = "../src/bin/cook/source.rs"]
mod source;

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nara-cook-fence-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        for (name, bytes) in [
            ("nara.toml", "manifest"),
            ("schema/authoring.bundle.json", "schema"),
            ("scenes/meadow.scene.json", "meadow"),
            ("scenes/cavern.scene.json", "cavern"),
            ("assets/textures/meadow.png", "image A"),
            ("assets/textures/meadow.png.meta", "meta A"),
            ("assets/textures/cavern.png", "image B"),
            ("assets/textures/cavern.png.meta", "meta B"),
            ("Cargo.toml", "package"),
            ("Cargo.lock", "lock"),
            ("build.rs", "fn main() {}"),
            ("src/lib.rs", "pub fn game() {}"),
            ("src/bin/player/generated.rs", "expected catalog"),
        ] {
            let target = path.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn content_fence_remains_original_when_code_is_bound_after_generation() {
    let directory = Directory::new();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let inputs = source::code_inventory(&directory.0).unwrap();
    fs::write(directory.0.join("scenes/cavern.scene.json"), "changed").unwrap();
    assert!(fence.bind_code(&inputs).is_err());
}

#[test]
fn code_and_generated_bootstrap_stay_fenced_through_later_publication_checks() {
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/lib.rs",
        "src/bin/player/generated.rs",
    ] {
        let directory = Directory::new();
        let mut fence = source::ContentFence::capture(&directory.0).unwrap();
        let inputs = source::code_inventory(&directory.0).unwrap();
        fence.bind_code(&inputs).unwrap();
        assert_eq!(
            fence
                .root()
                .open_file(&nara::fs::RelativePath::new("nara.toml").unwrap())
                .unwrap()
                .read_to_end_bounded(64)
                .unwrap(),
            b"manifest"
        );
        fence.verify().unwrap();
        fs::write(directory.0.join(name), "changed code selection").unwrap();
        assert!(fence.verify().is_err(), "{name}");
    }
}

#[test]
fn code_set_changes_and_rebinding_are_refused() {
    let directory = Directory::new();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let inputs = source::code_inventory(&directory.0).unwrap();
    fence.bind_code(&inputs).unwrap();
    assert!(fence.bind_code(&inputs).is_err());
    fs::write(directory.0.join("src/later.rs"), "new code").unwrap();
    assert!(fence.verify().is_err());
}
