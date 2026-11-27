use std::{
    fs::{self, File},
    io::{self, Write},
    path::{self, Path, PathBuf},
};

use nara_core::{identity::try_uuid_v7, vfs::AssetPath};
use thiserror::Error;

use super::SCHEMA_BINARY;

pub const GAME_BINARY: &str = "game";
pub const DESKTOP_FEATURE: &str = "desktop";

const STARTER_IMAGE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0x0f, 0x04, 0x00,
    0x09, 0xfb, 0x03, 0xfd, 0xfb, 0x5e, 0x6b, 0x2b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

#[derive(Debug, Error)]
pub enum StarterError {
    #[error("invalid starter package name {0:?}")]
    InvalidPackage(String),
    #[error("could not read the local Nara checkout: {0}")]
    CheckoutIo(#[source] io::Error),
    #[error("could not parse the local Nara checkout manifest: {0}")]
    CheckoutManifest(#[from] toml::de::Error),
    #[error("the local checkout manifest must declare the nara package")]
    WrongCheckout,
    #[error("missing string {0} in the Nara checkout manifest")]
    MissingCheckoutField(&'static str),
    #[error("the local Nara dependency path is not valid UTF-8: {}", .0.display())]
    NonUtf8Checkout(PathBuf),
    #[error("could not generate a starter identity: {0}")]
    Identity(#[source] io::Error),
    #[error("could not create the project at {}: {source}", .destination.display())]
    Create {
        destination: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Creates a code-first 2D project in a new directory using a local Nara checkout.
///
/// `package` starts with an ASCII letter and uses ASCII letters, digits, `-`, or `_`.
/// `nara` and the `nara_` prefix are reserved for the engine. `destination` must have an
/// existing parent and must not exist. The generated workspace uses absolute local dependency
/// paths and the checkout's Cargo patches. This operation writes source files without running Cargo.
pub fn create_project(
    destination: &Path,
    package: &str,
    nara_checkout: &Path,
) -> Result<(), StarterError> {
    validate_package(package)?;
    let cargo_manifest = cargo_manifest(package, nara_checkout)?;
    let new_id = || try_uuid_v7().map_err(StarterError::Identity);
    let scene_asset = new_id()?;
    let prefab_asset = new_id()?;
    let image_asset = new_id()?;
    let project_manifest = format!(
        include_str!("template/nara.toml.in"),
        name = toml_string(package),
        scene_asset = scene_asset,
    );
    let scene = format!(
        include_str!("template/main.scene.ron"),
        document = new_id()?,
        entity = new_id()?,
        prefab_asset = prefab_asset,
    );
    let prefab = format!(
        include_str!("template/sprite.prefab.ron"),
        document = new_id()?,
        entity = new_id()?,
        image_asset = image_asset,
    );
    let scene_meta = format!(
        include_str!("template/main.scene.ron.meta"),
        id = scene_asset
    );
    let prefab_meta = format!(
        include_str!("template/sprite.prefab.ron.meta"),
        id = prefab_asset
    );
    let image_meta = format!(include_str!("template/white.png.meta"), id = image_asset);
    let main = include_str!("template/main.rs").replace("{entry_scene}", &scene_asset.to_string());
    let files = [
        (".gitignore", "/target/\n/.nara/\n".as_bytes()),
        ("README.md", include_bytes!("template/README.md").as_slice()),
        ("nara.toml", project_manifest.as_bytes()),
        ("assets/main.scene.ron", scene.as_bytes()),
        ("assets/main.scene.ron.meta", scene_meta.as_bytes()),
        ("assets/sprite.prefab.ron", prefab.as_bytes()),
        ("assets/sprite.prefab.ron.meta", prefab_meta.as_bytes()),
        ("assets/white.png", STARTER_IMAGE),
        ("assets/white.png.meta", image_meta.as_bytes()),
        ("src/lib.rs", include_bytes!("template/lib.rs")),
        ("src/main.rs", main.as_bytes()),
        ("src/source.rs", include_bytes!("template/source.rs")),
        (
            "src/bin/nara_schema.rs",
            include_bytes!("template/nara_schema.rs"),
        ),
        ("Cargo.toml", cargo_manifest.as_bytes()),
    ];

    write_project(destination, &files).map_err(|source| StarterError::Create {
        destination: destination.to_path_buf(),
        source,
    })
}

fn validate_package(name: &str) -> Result<(), StarterError> {
    if name.len() > 64
        || !name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || name == "nara"
        || name.starts_with("nara_")
        || AssetPath::new(name).is_err()
    {
        return Err(StarterError::InvalidPackage(name.to_owned()));
    }
    Ok(())
}

fn cargo_manifest(package: &str, checkout: &Path) -> Result<String, StarterError> {
    let checkout = path::absolute(checkout).map_err(StarterError::CheckoutIo)?;
    let manifest =
        fs::read_to_string(checkout.join("Cargo.toml")).map_err(StarterError::CheckoutIo)?;
    let manifest: toml::Value = toml::from_str(&manifest)?;
    let field = |name: &'static str| {
        name.split('.')
            .try_fold(&manifest, |value, segment| value.get(segment))
            .and_then(toml::Value::as_str)
            .ok_or(StarterError::MissingCheckoutField(name))
    };
    if field("package.name")? != "nara" {
        return Err(StarterError::WrongCheckout);
    }

    let winit_path = checkout.join(field("patch.crates-io.winit.path")?);
    fs::metadata(winit_path.join("Cargo.toml")).map_err(StarterError::CheckoutIo)?;
    Ok(format!(
        include_str!("template/Cargo.toml.in"),
        package = toml_string(package),
        edition = toml_string(field("package.edition")?),
        rust_version = toml_string(field("package.rust-version")?),
        game_binary = toml_string(GAME_BINARY),
        schema_binary = toml_string(SCHEMA_BINARY),
        desktop_feature = toml_string(DESKTOP_FEATURE),
        checkout = toml_path(&checkout)?,
        wgpu_git = toml_string(field("patch.crates-io.wgpu.git")?),
        wgpu_rev = toml_string(field("patch.crates-io.wgpu.rev")?),
        winit_path = toml_path(&winit_path)?,
    ))
}

fn toml_path(path: &Path) -> Result<String, StarterError> {
    path.to_str()
        .map(toml_string)
        .ok_or_else(|| StarterError::NonUtf8Checkout(path.to_path_buf()))
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

fn write_project(destination: &Path, files: &[(&str, &[u8])]) -> io::Result<()> {
    fs::create_dir(destination)?;
    let mut directories = Vec::new();
    let mut written = Vec::new();
    let result = (|| {
        for directory in ["assets", "src", "src/bin"] {
            let path = destination.join(directory);
            fs::create_dir(&path)?;
            directories.push(path);
        }
        for (name, contents) in files {
            let path = destination.join(name);
            let mut file = File::create_new(&path)?;
            written.push(path);
            file.write_all(contents)?;
        }
        Ok(())
    })();
    if result.is_err() {
        for file in written {
            let _ = fs::remove_file(file);
        }
        for directory in directories.into_iter().rev() {
            let _ = fs::remove_dir(directory);
        }
        let _ = fs::remove_dir(destination);
    }
    result
}
