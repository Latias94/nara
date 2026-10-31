//! Three conventional Cargo packages; the generated expectation is a normal declared Rust input.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use engine::{
    fs::RelativePath,
    project_tooling::cargo_build::{CargoBuildLimits, CargoBuildRequest},
    project_workflow::cargo::CargoBuildSelection,
};

use super::workspace::{PROFILE, Tools};

pub const HOST_PACKAGE: &str = "aurora-author-tools";
pub const HOST_BIN: &str = "aurora-cook-station";
pub const PLAYER_BIN: &str = "aurora-portable-player";

const HOST_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/config.toml",
    "host/Cargo.toml",
    "host/src/main.rs",
    "game/Cargo.toml",
    "game/src/lib.rs",
    "game/src/probe.rs",
    "player/Cargo.toml",
    "player/src/main.rs",
];

pub struct CookedWorkspace {
    pub root: PathBuf,
}

impl CookedWorkspace {
    pub fn new(parent: &Path, tools: &Tools) -> Self {
        let root = parent.join("ordinary-cooked-workspace");
        fs::create_dir(&root).unwrap();
        let result = Self { root };
        result.write("Cargo.toml", &format!(
            "[workspace]\nresolver = '3'\nmembers = ['host', 'player', 'game']\n\n\
             [profile.{PROFILE}]\ninherits = 'dev'\ndebug = 0\ndebug-assertions = false\noverflow-checks = true\n",
        ));
        result.write(
            ".cargo/config.toml",
            &format!(
                "[build]\njobs = 1\ntarget-dir = {}\n",
                quoted(&tools.target)
            ),
        );
        let engine = quoted(&tools.repository);
        let domain = quoted(
            &tools
                .repository
                .join("tests/fixtures/module-consumer/asset-domain-consumer"),
        );
        result.write("game/Cargo.toml", &format!(
            "[package]\nname = 'aurora-game'\nversion = '0.0.0'\nedition = '2024'\n\n\
             [dependencies]\nengine = {{ package = 'nara', path = {engine}, default-features = false, features = ['runtime-2d', 'serde'] }}\n\
             nara-asset-domain-consumer = {{ path = {domain} }}\n",
        ));
        result.write("game/src/lib.rs", include_str!("cooked_game.rs"));
        result.write("game/src/probe.rs", include_str!("cooked_probe.rs"));
        result.write("host/Cargo.toml", &format!(
            "[package]\nname = '{HOST_PACKAGE}'\nversion = '0.0.0'\nedition = '2024'\n\n\
             [[bin]]\nname = '{HOST_BIN}'\npath = 'src/main.rs'\nrequired-features = ['cook-entry']\n\n\
             [features]\ndefault = ['unwanted-default']\nunwanted-default = []\ncook-entry = []\ncook = ['cook-entry']\n\n\
             [dependencies]\nengine = {{ package = 'nara', path = {engine}, default-features = false, features = ['project-tooling'] }}\n\
             aurora-game = {{ path = '../game' }}\nnara-asset-domain-consumer = {{ path = {domain} }}\nserde_json = '1'\n",
        ));
        result.write("host/src/main.rs", include_str!("cooked_host.rs"));
        result.write("player/Cargo.toml", &format!(
            "[package]\nname = 'aurora-native-game'\nversion = '0.0.0'\nedition = '2024'\n\n\
             [[bin]]\nname = '{PLAYER_BIN}'\npath = 'src/main.rs'\nrequired-features = ['player-entry']\n\n\
             [features]\ndefault = ['unwanted-default']\nunwanted-default = []\nplayer-entry = []\nquality = []\nship = ['player-entry', 'quality']\n\n\
             [dependencies]\nengine = {{ package = 'nara', path = {engine}, default-features = false, features = ['runtime-2d', 'serde'] }}\n\
             aurora-game = {{ path = '../game' }}\n",
        ));
        result.write("player/src/main.rs", include_str!("cooked_player.rs"));
        result.write("player/src/generated.rs", "compile_error!(\"the cooker must generate the Player expectation before building it\");\n");
        fs::create_dir(result.root.join("source")).unwrap();
        fs::write(result.root.join("source/stripe.png"), rgba_png()).unwrap();
        // This is preparation, before either production owner starts a locked build. No Cargo
        // command from the generated Host executes concurrently with this lockfile generation.
        let output = Command::new(&tools.cargo)
            .current_dir(&result.root)
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(result.root.join("Cargo.toml"))
            .env("RUSTC", &tools.rustc)
            .env("CARGO_TARGET_DIR", &tools.target)
            .env("CARGO_BUILD_JOBS", "1")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        result
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    pub fn host_request(
        &self,
        tools: &Tools,
        selection: CargoBuildSelection,
        limits: CargoBuildLimits,
    ) -> CargoBuildRequest {
        CargoBuildRequest::new(
            &self.root,
            RelativePath::new("host/Cargo.toml").unwrap(),
            selection,
            tools.build_tools(),
            HOST_INPUTS
                .iter()
                .map(|path| RelativePath::new(path).unwrap()),
            limits,
        )
        .unwrap()
        .with_target_directory(&tools.target)
        .unwrap()
    }
}

fn quoted(path: &Path) -> String {
    serde_json::to_string(path.to_str().unwrap()).unwrap()
}

/// Eight genuine RGBA texels in a one-row PNG; zlib, Adler32 and PNG CRCs are complete. The
/// production PNG importer and target cook must read this file, rather than a fake Runtime Image.
fn rgba_png() -> Vec<u8> {
    let mut scanline = vec![0];
    for x in 0..8u8 {
        scanline.extend_from_slice(&[x, x * 17, 255 - x, 255]);
    }
    let length = u16::try_from(scanline.len()).unwrap();
    let mut zlib = vec![0x78, 0x01, 0x01];
    zlib.extend_from_slice(&length.to_le_bytes());
    zlib.extend_from_slice(&(!length).to_le_bytes());
    zlib.extend_from_slice(&scanline);
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &scanline {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = 8u32.to_be_bytes().to_vec();
    header.extend_from_slice(&1u32.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    png.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
    let start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(payload);
    let mut crc = !0u32;
    for byte in &png[start..] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    png.extend_from_slice(&(!crc).to_be_bytes());
}
