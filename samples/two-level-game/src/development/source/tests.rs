use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use super::*;

const TEMP_PREFIX: &str = "nara-game-source-fence-";
const FIXTURE_FILES: [(&str, &[u8]); 10] = [
    ("Cargo.toml", b"[package]\nname = 'fixture'\n"),
    ("Cargo.lock", b"version = 4\n"),
    ("nara.toml", b"game = 'two-level'\n"),
    ("src/lib.rs", b"pub fn speed() -> u32 { 80 }\n"),
    ("assets/textures/cavern.png", b"cavern pixels"),
    ("assets/textures/cavern.png.meta", b"cavern metadata"),
    ("scenes/meadow.scene.json", b"meadow scene"),
    ("scenes/cavern.scene.json", b"unvisited cavern scene"),
    ("prefabs/marker.prefab.json", b"marker prefab"),
    ("schema/authoring.bundle.json", b"game schema and recipes"),
];

struct NativeTemp {
    parent: PathBuf,
    path: PathBuf,
    parent_directory: DirectoryCapability,
    directory: DirectoryCapability,
}

impl NativeTemp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let parent_directory = host_fs::readable_directory(&parent).unwrap();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = parent.join(format!(
            "{TEMP_PREFIX}{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        let path = path.canonicalize().unwrap();
        assert_eq!(path.parent(), Some(parent.as_path()));
        let directory = host_fs::readable_directory(&path).unwrap();
        Self {
            parent,
            path,
            parent_directory,
            directory,
        }
    }
}

impl Drop for NativeTemp {
    fn drop(&mut self) {
        let verified = (|| {
            let resolved = self.path.canonicalize().ok()?;
            if resolved != self.path
                || resolved.parent() != Some(self.parent.as_path())
                || !resolved.file_name()?.to_str()?.starts_with(TEMP_PREFIX)
                || fs::symlink_metadata(&self.path)
                    .ok()?
                    .file_type()
                    .is_symlink()
            {
                return None;
            }
            let parent = host_fs::readable_directory(&self.parent).ok()?;
            let current = host_fs::readable_directory(&resolved).ok()?;
            if !parent.same_object(&self.parent_directory).ok()?
                || !current.same_object(&self.directory).ok()?
            {
                return None;
            }
            Some(resolved)
        })();
        let Some(resolved) = verified else {
            eprintln!(
                "refusing to remove an unverified source-fence fixture: {}",
                self.path.display()
            );
            return;
        };
        if let Err(error) = fs::remove_dir_all(&resolved) {
            eprintln!(
                "source-fence fixture cleanup failed at {}: {error}",
                resolved.display()
            );
        }
    }
}

struct Fixture {
    owner: NativeTemp,
    project: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let owner = NativeTemp::new();
        let project = owner.path.join("project");
        populate(&project);
        Self { owner, project }
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let target = self.project.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }

    fn digest(&self, path: &str) -> ContentDigest {
        ContentDigest::of_bytes(&fs::read(self.project.join(path)).unwrap())
    }
}

fn populate(project: &Path) {
    fs::create_dir(project).unwrap();
    for (path, bytes) in FIXTURE_FILES {
        let target = project.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
}

#[test]
fn captures_current_body_and_structure_edits_without_incumbent_source_authority() {
    let fixture = Fixture::new();
    let incumbent = fixture.digest("src/lib.rs");
    fixture.write(
        "src/lib.rs",
        b"pub mod movement;\npub fn speed() -> u32 { 40 }\n",
    );
    fixture.write("src/movement.rs", b"pub struct Velocity(pub f32);\n");
    let mut fence = SourceFence::capture(&fixture.project).unwrap();

    assert!(fence.bind_inputs([("src/lib.rs", incumbent)]).is_err());
    fence
        .bind_inputs([
            ("Cargo.toml", fixture.digest("Cargo.toml")),
            ("src/lib.rs", fixture.digest("src/lib.rs")),
            ("src/movement.rs", fixture.digest("src/movement.rs")),
        ])
        .unwrap();
    fence.verify().unwrap();

    fixture.write("src/movement.rs", b"pub struct Velocity(pub f64);\n");
    assert!(fence.verify().is_err());
}

#[test]
fn detects_changes_in_manifests_unvisited_levels_rust_and_all_content_trees() {
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "nara.toml",
        "src/lib.rs",
        "assets/textures/cavern.png",
        "assets/textures/cavern.png.meta",
        "scenes/cavern.scene.json",
        "prefabs/marker.prefab.json",
        "schema/authoring.bundle.json",
    ] {
        let fixture = Fixture::new();
        let fence = SourceFence::capture(&fixture.project).unwrap();
        let mut changed = fs::read(fixture.project.join(path)).unwrap();
        changed[0] ^= 1;
        fixture.write(path, &changed);
        assert!(
            fence.verify().is_err(),
            "same-length change was missed at {path}"
        );
    }
}

#[test]
fn rejects_new_or_missing_files_and_new_empty_directories() {
    let fixture = Fixture::new();
    let fence = SourceFence::capture(&fixture.project).unwrap();
    fixture.write("src/not_declared_as_a_module.rs", b"new Rust input");
    assert!(fence.verify().is_err());

    let fixture = Fixture::new();
    let fence = SourceFence::capture(&fixture.project).unwrap();
    fs::remove_file(fixture.project.join("scenes/cavern.scene.json")).unwrap();
    assert!(fence.verify().is_err());

    let fixture = Fixture::new();
    let fence = SourceFence::capture(&fixture.project).unwrap();
    fs::create_dir(fixture.project.join("prefabs/empty")).unwrap();
    assert!(fence.verify().is_err());
}

#[test]
fn capture_requires_every_root_file_and_known_directory() {
    for path in ROOT_FILES {
        let fixture = Fixture::new();
        fs::remove_file(fixture.project.join(path)).unwrap();
        assert!(
            SourceFence::capture(&fixture.project).is_err(),
            "missing {path}"
        );
    }
    for path in SOURCE_DIRECTORIES {
        let fixture = Fixture::new();
        let source = fixture.project.join(path);
        let moved = fixture.owner.path.join("missing-directory");
        assert_eq!(source.parent(), Some(fixture.project.as_path()));
        assert_eq!(moved.parent(), Some(fixture.owner.path.as_path()));
        fs::rename(source, moved).unwrap();
        assert!(
            SourceFence::capture(&fixture.project).is_err(),
            "missing {path}"
        );
    }
}

#[test]
fn root_name_replacement_with_identical_bytes_rejects_the_original_fence() {
    let fixture = Fixture::new();
    let fence = SourceFence::capture(&fixture.project).unwrap();
    let moved = fixture.owner.path.join("original-project");
    assert_eq!(fixture.project.parent(), Some(fixture.owner.path.as_path()));
    assert_eq!(moved.parent(), Some(fixture.owner.path.as_path()));
    fs::rename(&fixture.project, &moved).unwrap();
    populate(&fixture.project);

    assert!(fence.verify().unwrap_err().contains("root object changed"));
    SourceFence::capture(&fixture.project)
        .unwrap()
        .verify()
        .unwrap();
}

#[test]
fn bound_extra_inputs_remain_admission_observations_without_read_leases() {
    let fixture = Fixture::new();
    let mut fence = SourceFence::capture(&fixture.project).unwrap();
    fixture.write("build-inputs/recipe.rs", b"pub const RECIPE: u32 = 1;\n");
    fence.verify().unwrap();
    fence
        .bind_inputs([(
            "build-inputs/recipe.rs",
            fixture.digest("build-inputs/recipe.rs"),
        )])
        .unwrap();
    fence.verify().unwrap();

    fixture.write("build-inputs/recipe.rs", b"pub const RECIPE: u32 = 2;\n");
    assert!(fence.verify().is_err());

    fixture.write("build-inputs/recipe.rs", b"pub const RECIPE: u32 = 1;\n");
    fence.verify().unwrap();
    fs::remove_file(fixture.project.join("build-inputs/recipe.rs")).unwrap();
    assert!(fence.verify().is_err());
}

#[test]
fn validates_every_declared_path_and_both_digest_length_and_hash() {
    let fixture = Fixture::new();
    let mut fence = SourceFence::capture(&fixture.project).unwrap();
    let manifest = fixture.digest("Cargo.toml");
    let absolute = fixture.project.join("Cargo.toml");
    for path in [absolute.to_str().unwrap(), "../Cargo.toml", "missing.rs"] {
        assert!(
            fence
                .bind_inputs([("Cargo.toml", manifest), (path, manifest)])
                .is_err()
        );
        fence.verify().unwrap();
    }
    assert!(
        fence
            .bind_inputs([("Cargo.toml", manifest), ("Cargo.toml", manifest)])
            .is_err()
    );
    let wrong_length = ContentDigest::from_parts(manifest.length() + 1, *manifest.as_bytes());
    let wrong_hash = ContentDigest::from_parts(manifest.length(), [0; 32]);
    assert!(fence.bind_inputs([("Cargo.toml", wrong_length)]).is_err());
    assert!(fence.bind_inputs([("Cargo.toml", wrong_hash)]).is_err());
    let oversized = ContentDigest::from_parts(MAX_FILE_BYTES + 1, [0; 32]);
    assert!(
        fence
            .bind_inputs([("oversized.rs", oversized)])
            .unwrap_err()
            .contains("exceeds 64 MiB")
    );
    let combined = ContentDigest::from_parts(MAX_TOTAL_BYTES, [0; 32]);
    assert!(
        fence
            .bind_inputs([("additional.rs", combined)])
            .unwrap_err()
            .contains("combined source envelope")
    );
    fence.verify().unwrap();
}

#[test]
fn ignores_undeclared_files_outside_the_game_envelope() {
    let fixture = Fixture::new();
    let fence = SourceFence::capture(&fixture.project).unwrap();
    for path in ["target/generated.rs", ".git/HEAD", "repo-ref/reference.rs"] {
        fixture.write(path, b"outside the finite game closure");
    }
    fence.verify().unwrap();
}

#[test]
fn enforces_the_file_and_depth_allowances() {
    let fixture = Fixture::new();
    for index in FIXTURE_FILES.len()..MAX_FILES {
        fixture.write(&format!("src/file-{index}.rs"), b"");
    }
    let fence = SourceFence::capture(&fixture.project).unwrap();
    fence.verify().unwrap();
    fixture.write("src/excess.rs", b"");
    assert!(fence.verify().is_err());

    let fixture = Fixture::new();
    let at_limit = format!("src/{}leaf.rs", "nested/".repeat(MAX_DEPTH - 2));
    fixture.write(&at_limit, b"");
    let fence = SourceFence::capture(&fixture.project).unwrap();
    let too_deep = format!("src/{}leaf.rs", "nested/".repeat(MAX_DEPTH - 1));
    fixture.write(&too_deep, b"");
    assert!(fence.verify().is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symlinks_in_the_known_trees_and_in_declared_inputs() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let mut fence = SourceFence::capture(&fixture.project).unwrap();
    let target = fixture.project.join("src/lib.rs");
    symlink(&target, fixture.project.join("linked.rs")).unwrap();
    assert!(
        fence
            .bind_inputs([("linked.rs", fixture.digest("src/lib.rs"))])
            .is_err()
    );
    symlink(&target, fixture.project.join("src/linked.rs")).unwrap();
    assert!(fence.verify().is_err());
    assert!(SourceFence::capture(&fixture.project).is_err());
}
