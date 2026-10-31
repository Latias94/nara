//! Ordinary generated Cargo workspaces; all nested Cargo work is explicitly qualified and serial.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use engine::{
    fs::RelativePath,
    project_tooling::cargo_build::{CargoBuildLimits, CargoBuildRequest, CargoBuildTools},
    project_workflow::cargo::{CargoBuildSelection, CargoMetadataFacts},
};

pub const PROFILE: &str = "consumer-checked";
pub const HOST_PACKAGE: &str = "orbit-authoring-kit";
pub const HOST_BIN: &str = "orbit-cook-helper";
pub const PLAYER_PACKAGE: &str = "orbit-native-game";
pub const PLAYER_BIN: &str = "orbit-native-player";
pub const FOREIGN_PACKAGE: &str = "orbit-wasi-game";
pub const FOREIGN_BIN: &str = "orbit-wasi-player";
pub const FOREIGN_TARGET: &str = "wasm32-wasip1";
pub const HOST_MARKER: &str = "host-was-executed";
pub const FOREIGN_MARKER: &str = "foreign-was-executed";
pub const GENERATION: &str = "host-kit/src/generation.rs";
pub const GOOD_GENERATION: &str = "pub const GENERATION: &str = \"A\";\n";

pub struct Tools {
    pub cargo: PathBuf,
    pub rustc: PathBuf,
    pub host: String,
    pub repository: PathBuf,
    pub target: PathBuf,
}

impl Tools {
    pub fn pinned() -> Self {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let target = repository.join("target").canonicalize().unwrap();
        let configured = PathBuf::from(
            std::env::var_os("CARGO_TARGET_DIR")
                .expect("qualification must set CARGO_TARGET_DIR to the repository target"),
        );
        assert!(configured.is_absolute());
        assert_eq!(configured.canonicalize().unwrap(), target);

        let cargo = rustup_which("cargo");
        let rustc = rustup_which("rustc");
        let version = successful(Command::new(&rustc).arg("-vV"));
        let version = String::from_utf8(version.stdout).unwrap();
        assert!(version.lines().any(|line| line == "release: 1.95.0"));
        let host = version
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .expect("the selected actual compiler reports its Host target")
            .to_owned();
        Self {
            cargo,
            rustc,
            host,
            repository,
            target,
        }
    }

    pub fn build_tools(&self) -> CargoBuildTools {
        CargoBuildTools::new(&self.cargo, &self.rustc).unwrap()
    }

    pub fn require_foreign_std(&self) {
        let sysroot = self.rustc.parent().unwrap().parent().unwrap();
        assert!(
            sysroot
                .join("lib/rustlib")
                .join(FOREIGN_TARGET)
                .join("lib")
                .is_dir(),
            "qualification requires the already installed 1.95.0 {FOREIGN_TARGET} standard library",
        );
        assert_ne!(self.host, FOREIGN_TARGET);
    }
}

fn rustup_which(tool: &str) -> PathBuf {
    let output = successful(Command::new("rustup").args(["which", "--toolchain", "1.95.0", tool]));
    let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(path.is_absolute());
    assert!(path.is_file());
    path.canonicalize().unwrap()
}

fn successful(command: &mut Command) -> Output {
    let output = command.stdin(Stdio::null()).output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

const PREFIX: &str = "nara-cargo-build-consumer-";

pub struct Temporary {
    parent: PathBuf,
    pub root: PathBuf,
}

impl Temporary {
    pub fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = parent.join(format!("{PREFIX}{}-{unique}", std::process::id()));
        fs::create_dir(&root).unwrap();
        Self {
            parent,
            root: root.canonicalize().unwrap(),
        }
    }

    pub fn remove_owned(&self, path: &Path) {
        let resolved = path.canonicalize().unwrap();
        assert_eq!(self.root.parent(), Some(self.parent.as_path()));
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(PREFIX),
        );
        assert!(resolved == self.root || resolved.starts_with(&self.root));
        fs::remove_dir_all(&resolved).unwrap();
        assert!(!path.exists());
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        if self.root.exists() {
            self.remove_owned(&self.root);
        }
    }
}

pub struct Workspace {
    owner: Temporary,
    pub root: PathBuf,
    target: PathBuf,
}

impl Workspace {
    pub fn new(tools: &Tools, native_player: bool) -> Self {
        let owner = Temporary::new();
        let root = owner.root.join("ordinary-workspace");
        fs::create_dir(&root).unwrap();
        let workspace = Self {
            owner,
            root,
            target: tools.target.clone(),
        };
        let members = if native_player {
            "['host-kit', 'selection-support', 'decoy', 'foreign-kit', 'player-kit']"
        } else {
            "['host-kit', 'selection-support', 'decoy', 'foreign-kit']"
        };
        workspace.write(
            "Cargo.toml",
            &format!(
                "[workspace]\nresolver = '3'\nmembers = {members}\n\n\
                 [profile.{PROFILE}]\ninherits = 'dev'\ndebug = 0\n\
                 debug-assertions = false\noverflow-checks = true\n",
            ),
        );
        workspace.write(
            ".cargo/config.toml",
            &format!(
                "[build]\njobs = 1\ntarget-dir = {}\n",
                serde_json::to_string(tools.target.to_str().unwrap()).unwrap(),
            ),
        );
        workspace.write(
            "host-kit/Cargo.toml",
            &format!(
                "[package]\nname = '{HOST_PACKAGE}'\nversion = '0.0.0'\nedition = '2024'\n\n\
                 [[bin]]\nname = '{HOST_BIN}'\npath = 'src/main.rs'\n\
                 required-features = ['host-entry', 'support/eligible']\n\n\
                 [features]\ndefault = ['unwanted-default']\nunwanted-default = []\n\
                 host-entry = []\nexpanded = []\nmetadata-only = []\n\
                 trial-host = ['host-entry', 'expanded', 'support/eligible']\n\n\
                 [dependencies]\nsupport = {{ package = 'orbit-selection-support', \
                 path = '../selection-support', default-features = false }}\n",
            ),
        );
        workspace.write("host-kit/src/main.rs", include_str!("host_main.rs"));
        workspace.write("host-kit/src/lib.rs", "pub const LIBRARY_MARKER: u8 = 1;\n");
        workspace.write(GENERATION, GOOD_GENERATION);
        workspace.write(
            "selection-support/Cargo.toml",
            "[package]\nname = 'orbit-selection-support'\nversion = '0.0.0'\nedition = '2024'\n\n\
             [features]\neligible = []\n",
        );
        workspace.write(
            "selection-support/src/lib.rs",
            "pub const SUPPORT_MARKER: u8 = 1;\n",
        );
        // A workspace member owns exactly the same bin name. Selecting by bin alone is ambiguous.
        // Its library dependency also activates a Host feature in workspace metadata only.
        workspace.write(
            "decoy/Cargo.toml",
            &manifest(
                "orbit-decoy",
                HOST_BIN,
                "host-entry",
                "trial-host",
                "[dependencies]\norbit-authoring-kit = { path = '../host-kit', \
                 default-features = false, features = ['metadata-only'] }\n",
            ),
        );
        workspace.write(
            "decoy/src/main.rs",
            "compile_error!(\"the decoy package must never be built\");\nfn main() {}\n",
        );
        workspace.write(
            "foreign-kit/Cargo.toml",
            &manifest(FOREIGN_PACKAGE, FOREIGN_BIN, "wasi-entry", "trial-wasi", ""),
        );
        workspace.write("foreign-kit/src/main.rs", include_str!("foreign_main.rs"));
        if native_player {
            let dependency = format!(
                "[dependencies]\nengine = {{ package = 'nara', path = {}, \
                 default-features = false, features = ['runtime-2d', 'serde'] }}\n",
                serde_json::to_string(tools.repository.to_str().unwrap()).unwrap(),
            );
            workspace.write(
                "player-kit/Cargo.toml",
                &manifest(
                    PLAYER_PACKAGE,
                    PLAYER_BIN,
                    "player-entry",
                    "trial-player",
                    &dependency,
                ),
            );
            workspace.write(
                "player-kit/src/main.rs",
                include_str!(
                    "../../../runtime-runner/renamed-root/tests/support/processed_input_main.rs"
                ),
            );
            workspace.write(
                "player-kit/src/processed_input_probe.rs",
                include_str!(
                    "../../../runtime-runner/renamed-root/tests/support/processed_input_probe.rs"
                ),
            );
        }
        // Only fixture preparation resolves the ordinary workspace. The API builds with --locked.
        successful(
            Command::new(&tools.cargo)
                .current_dir(&workspace.root)
                .args(["generate-lockfile", "--offline", "--manifest-path"])
                .arg(workspace.root.join("Cargo.toml"))
                .env("RUSTC", &tools.rustc)
                .env("CARGO_BUILD_JOBS", "1")
                .env("CARGO_TARGET_DIR", &tools.target),
        );
        workspace
    }

    pub fn write(&self, relative: &str, source: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }

    pub fn request(
        &self,
        tools: &Tools,
        package_manifest: &str,
        selection: CargoBuildSelection,
        inputs: &[&str],
    ) -> CargoBuildRequest {
        self.request_with_limits(
            tools,
            package_manifest,
            selection,
            inputs,
            CargoBuildLimits::default(),
        )
    }

    pub fn request_with_limits(
        &self,
        tools: &Tools,
        package_manifest: &str,
        selection: CargoBuildSelection,
        inputs: &[&str],
        limits: CargoBuildLimits,
    ) -> CargoBuildRequest {
        CargoBuildRequest::new(
            &self.root,
            RelativePath::new(package_manifest).unwrap(),
            selection,
            tools.build_tools(),
            inputs.iter().map(|path| RelativePath::new(path).unwrap()),
            limits,
        )
        .unwrap()
        .with_target_directory(&self.target)
        .unwrap()
    }

    pub fn host_metadata_features(
        &self,
        tools: &Tools,
        selection: &CargoBuildSelection,
    ) -> Vec<String> {
        let manifest = self.root.join("host-kit/Cargo.toml");
        let output = successful(
            Command::new(&tools.cargo)
                .current_dir(&self.root)
                .args([
                    "metadata",
                    "--locked",
                    "--format-version",
                    "1",
                    "--no-default-features",
                    "--features",
                ])
                .arg(selection.features().join(","))
                .arg("--filter-platform")
                .arg(selection.target_triple())
                .arg("--manifest-path")
                .arg(&manifest)
                .env("RUSTC", &tools.rustc)
                .env("CARGO_TARGET_DIR", &self.target),
        );
        CargoMetadataFacts::parse(&output.stdout, &manifest, selection)
            .unwrap()
            .metadata_features()
            .to_vec()
    }

    pub fn output(&self, selection: &CargoBuildSelection) -> PathBuf {
        let suffix = if selection.target_triple() == FOREIGN_TARGET {
            ".wasm"
        } else {
            std::env::consts::EXE_SUFFIX
        };
        self.target
            .join(selection.target_triple())
            .join(selection.profile())
            .join(format!("{}{suffix}", selection.target()))
    }

    pub fn replace_output(&self, selection: &CargoBuildSelection) {
        let path = self.output(selection);
        assert!(path.starts_with(&self.target));
        assert!(path.is_file());
        // Vacate the name before replacement: Windows may retain a deleted open file's name until
        // its final handle closes. The captured original permits sharing deletion and renaming.
        let retired = path.with_file_name(format!(
            "{}.retired-{}",
            path.file_name().unwrap().to_str().unwrap(),
            self.owner.root.file_name().unwrap().to_str().unwrap(),
        ));
        assert_eq!(retired.parent(), path.parent());
        assert!(!retired.exists());
        fs::rename(&path, &retired).unwrap();
        fs::write(&path, b"mutable Cargo output replaced after capture\n").unwrap();
        fs::remove_file(retired).unwrap();
    }

    pub fn remove_source(&self) {
        self.owner.remove_owned(&self.root);
    }
}

fn manifest(package: &str, bin: &str, entry: &str, alias: &str, dependencies: &str) -> String {
    format!(
        "[package]\nname = '{package}'\nversion = '0.0.0'\nedition = '2024'\n\n\
         [[bin]]\nname = '{bin}'\npath = 'src/main.rs'\nrequired-features = ['{entry}']\n\n\
         [features]\ndefault = ['unwanted-default']\nunwanted-default = []\n\
         {entry} = []\nexpanded = []\n{alias} = ['{entry}', 'expanded']\n\n{dependencies}",
    )
}

pub fn selection(package: &str, bin: &str, triple: &str, features: &[&str]) -> CargoBuildSelection {
    CargoBuildSelection::new(
        package,
        bin,
        PROFILE,
        triple,
        features.iter().copied(),
        false,
    )
    .unwrap()
}

pub const HOST_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/config.toml",
    "host-kit/Cargo.toml",
    "host-kit/src/lib.rs",
    "host-kit/src/main.rs",
    "selection-support/Cargo.toml",
    "selection-support/src/lib.rs",
    "decoy/Cargo.toml",
    GENERATION,
];

pub const PLAYER_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/config.toml",
    "player-kit/Cargo.toml",
    "player-kit/src/main.rs",
    "player-kit/src/processed_input_probe.rs",
];

pub const FOREIGN_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/config.toml",
    "foreign-kit/Cargo.toml",
    "foreign-kit/src/main.rs",
];

pub fn install_native(bytes: &[u8], path: &Path) {
    fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

pub fn run_host(bytes: &[u8], parent: &Path, name: &str) {
    let root = parent.join(name);
    fs::create_dir(&root).unwrap();
    let executable = root.join(format!("{HOST_BIN}{}", std::env::consts::EXE_SUFFIX));
    install_native(bytes, &executable);
    let missing_path = root.join("no-build-tools-on-PATH");
    assert!(!missing_path.exists());
    let mut command = Command::new(&executable);
    command
        .current_dir(&root)
        .env_clear()
        .env("PATH", missing_path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
        for name in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
    }
    let output = successful(&mut command);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "host-generation=A;debug_assertions=false;expanded=true\n",
    );
    assert_eq!(
        fs::read(root.join(HOST_MARKER)).unwrap(),
        b"explicit Host launch\n",
    );
}
