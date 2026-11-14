use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use engine::{
    fs::RelativePath,
    project_tooling::cargo_build::{CargoBuildLimits, CargoBuildRequest, build_executable},
};

use super::workspace::{PROFILE, Tools, install_native, selection};

pub const PACKAGE: &str = "palette-cooked-game";
pub const COOKER: &str = "palette-cooked-cooker";
pub const PLAYER: &str = "palette-cooked-player";
pub const GENERATED: &str = "src/player/generated.rs";
const INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/config.toml",
    "src/lib.rs",
    "src/probe.rs",
    "src/palette.wgsl",
    "src/cooker.rs",
    "src/player/main.rs",
    GENERATED,
];

pub struct MaterialWorkspace {
    pub root: PathBuf,
}

impl MaterialWorkspace {
    pub fn new(parent: &Path, tools: &Tools) -> Self {
        let result = Self {
            root: parent.join("ordinary-palette-cooked-workspace"),
        };
        fs::create_dir(&result.root).unwrap();
        let engine = serde_json::to_string(tools.repository.to_str().unwrap()).unwrap();
        result.write(
            "Cargo.toml",
            &format!(
                r#"[package]
name = "{PACKAGE}"
version = "0.0.0"
edition = "2024"
rust-version = "1.95"
publish = false

[workspace]
resolver = "3"

[features]
default = []
cook = ["engine/project-tooling"]
player = []

[dependencies]
engine = {{ package = "nara", path = {engine}, default-features = false, features = ["runtime-2d", "serde"] }}

[[bin]]
name = "{COOKER}"
path = "src/cooker.rs"
required-features = ["cook"]

[[bin]]
name = "{PLAYER}"
path = "src/player/main.rs"
required-features = ["player"]

[profile.{PROFILE}]
inherits = "dev"
debug = 0
debug-assertions = false
overflow-checks = true
"#,
            ),
        );
        result.write(
            ".cargo/config.toml",
            &format!(
                "[build]\njobs = 1\ntarget-dir = {}\n\n[net]\noffline = true\n",
                serde_json::to_string(tools.target.to_str().unwrap()).unwrap(),
            ),
        );
        for (path, contents) in [
            ("src/lib.rs", include_str!("game.rs")),
            ("src/probe.rs", include_str!("probe.rs")),
            ("src/palette.wgsl", include_str!("palette.wgsl")),
            ("src/cooker.rs", include_str!("cooker.rs")),
            ("src/player/main.rs", include_str!("player.rs")),
            (
                GENERATED,
                "compile_error!(\"the cooker must generate the Player expectation\");\n",
            ),
        ] {
            result.write(path, contents);
        }
        fs::create_dir(result.root.join("source")).unwrap();
        let png = include_bytes!("../../../../images/valid-rgba-1x1.png");
        for path in ["source/indices.png", "source/palette.png"] {
            fs::write(result.root.join(path), png).unwrap();
        }
        result
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

pub fn cooker_directory(parent: &Path) -> PathBuf {
    parent.join("captured-cooker")
}

pub fn build_cooker(root: &Path, tools: &Tools) {
    let status = Command::new(&tools.cargo)
        .current_dir(root)
        .args(["generate-lockfile", "--offline", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .env("RUSTC", &tools.rustc)
        .env("CARGO_TARGET_DIR", &tools.target)
        .env("CARGO_BUILD_JOBS", "1")
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "generated workspace lockfile: {status}");
    let selected = selection(PACKAGE, COOKER, &tools.host, &["cook"]);
    let request = CargoBuildRequest::new(
        root,
        RelativePath::new("Cargo.toml").unwrap(),
        selected.clone(),
        tools.build_tools(),
        INPUTS.iter().map(|path| RelativePath::new(path).unwrap()),
        CargoBuildLimits::default(),
    )
    .unwrap()
    .with_target_directory(&tools.target)
    .unwrap();
    let captured = build_executable(&request, |_| Ok(())).unwrap();
    assert_eq!(captured.provenance().selection(), &selected);
    assert_eq!(captured.provenance().resolved_features(), ["cook"]);
    assert_eq!(captured.provenance().declared_inputs().len(), INPUTS.len());
    let directory = cooker_directory(root.parent().unwrap());
    fs::create_dir(&directory).unwrap();
    install_native(
        captured.bytes(),
        &directory.join(format!("{COOKER}{}", std::env::consts::EXE_SUFFIX)),
    );
}

pub fn assert_payload(directory: &Path, executable: &str) {
    let mut files = fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            entry.file_name().into_string().unwrap()
        })
        .collect::<Vec<_>>();
    let mut expected = vec![
        executable.to_owned(),
        "content.pack".to_owned(),
        "package.json".to_owned(),
    ];
    files.sort();
    expected.sort();
    assert_eq!(files, expected);
}
