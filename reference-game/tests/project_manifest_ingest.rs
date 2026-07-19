use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    project::{ProductCapability, RuntimePreset},
    project_host::ingest_project_manifest,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new(label: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nara_reference_manifest_{label}_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}

#[test]
fn committed_manifest_is_opened_by_capability_and_lowers_product_settings() {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = DirectoryCapability::from_host_handle(
        host_directory(project_root),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
    .unwrap();
    let manifest = root
        .open_file(&RelativePath::new(Path::new("nara.toml")).unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();

    assert_eq!(
        candidate.settings().runtime_preset,
        RuntimePreset::LocalHeadless
    );
    assert!(
        candidate
            .normalized_capabilities()
            .contains(ProductCapability::RuntimeCore)
    );
    assert!(
        candidate
            .normalized_capabilities()
            .contains(ProductCapability::Runtime2d)
    );
    assert_eq!(candidate.normalized_capabilities().len(), 2);
    assert_eq!(
        candidate.settings().runtime.fixed_time().timestep(),
        std::time::Duration::from_millis(20)
    );
    assert_eq!(
        candidate
            .settings()
            .startup
            .default_scene
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("startup.scene.json")
    );
}

#[test]
fn headless_cli_opens_the_committed_manifest_from_a_random_working_directory() {
    let cwd = TemporaryDirectory::new("cli_success");
    let home = TemporaryDirectory::new("cli_home");
    let output = Command::new(env!("CARGO_BIN_EXE_headless"))
        .current_dir(cwd.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env_remove("NARA_REFERENCE_GAME_MANIFEST")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "{\"schema\":\"nara-reference-game.wave-summary-v1\",",
            "\"outcome\":\"completed\",\"tick\":49,\"score\":300,",
            "\"player_hit_points\":20,\"enemies_remaining\":0,",
            "\"projectiles_remaining\":4}\n"
        )
    );
    assert!(output.stderr.is_empty());
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}
