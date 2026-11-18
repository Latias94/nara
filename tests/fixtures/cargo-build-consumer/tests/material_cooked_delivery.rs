#![cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
#![forbid(unsafe_code)]

use std::{env, fs, path::PathBuf, time::Duration};

#[path = "support/material_cooked_delivery/workspace.rs"]
mod material_workspace;
#[path = "support/material_cooked_delivery/process.rs"]
mod process;
#[path = "support/workspace.rs"]
#[allow(dead_code)]
mod workspace;

use material_workspace::{GENERATED, MaterialWorkspace, PACKAGE, PLAYER};
use process::OwnedProcess;
use workspace::{Temporary, Tools};

const TEST_NAME: &str = "compiled_palette_player_runs_relocated_with_only_cooked_content";
const BUILD_ROOT_ENV: &str = "NARA_MATERIAL_COOKED_BUILD_ROOT";
const BUILD_MARKER: &str = "palette-cooker-build ok captured";
const COOK_MARKER: &str =
    "palette-cooker ok png-import material-import canonical-products published";
const PLAYER_MARKER: &str =
    "palette-player ok frames=2 products=4 default-palette-pixels=24,120,220,255 source-free";

#[test]
#[ignore = "actual serial Rust 1.95 offline cooker/Player builds and source-free execution; requires shared root target"]
fn compiled_palette_player_runs_relocated_with_only_cooked_content() {
    if let Some(root) = env::var_os(BUILD_ROOT_ENV) {
        let tools = Tools::pinned();
        material_workspace::build_cooker(&PathBuf::from(root), &tools);
        println!("{BUILD_MARKER}");
        return;
    }

    let tools = Tools::pinned();
    let directory = Temporary::new();
    let project = MaterialWorkspace::new(&directory.root, &tools);
    let (directory, stdout) =
        OwnedProcess::build(directory, &tools, &project.root).run(Duration::from_secs(30 * 60));
    assert!(stdout.contains(BUILD_MARKER), "{stdout}");
    let output = directory.root.join("delivery-output");
    fs::create_dir(&output).unwrap();
    let (directory, stdout) = OwnedProcess::cook(directory, &tools, &project.root, &output)
        .run(Duration::from_secs(30 * 60));
    assert_eq!(stdout, format!("{COOK_MARKER}\n"));

    let published = output.join("published");
    let executable = format!("{PLAYER}{}", env::consts::EXE_SUFFIX);
    material_workspace::assert_payload(&published, &executable);
    let manifest_bytes = fs::read(published.join("package.json")).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(envelope["kind"], "player_package");
    let manifest = &envelope["payload"];
    assert_eq!(manifest["build"]["selection"]["package"], PACKAGE);
    assert_eq!(manifest["build"]["selection"]["bin"], PLAYER);
    assert_eq!(manifest["build"]["selection"]["target_triple"], tools.host);
    assert_eq!(manifest["build"]["selection"]["default_features"], false);
    assert_eq!(
        manifest["build"]["selection"]["requested_features"],
        serde_json::json!(["player"])
    );
    assert_eq!(
        manifest["build"]["resolved_features"],
        serde_json::json!(["player"])
    );
    assert_eq!(manifest["generated_input_name"], GENERATED);
    assert_eq!(manifest["content_target"]["profile"], "palette-shipping");
    assert!(
        manifest["build"]["declared_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|input| input["path"] != GENERATED)
    );
    let generated = &manifest["build"]["generated_rust_input"];
    assert_eq!(generated["path"], GENERATED);
    assert_eq!(
        generated["digest"][0],
        manifest["generated_input_digest"]["length"]
    );
    assert_eq!(
        generated["digest"][1],
        manifest["generated_input_digest"]["hash"]
    );
    assert!(!project.root.join(GENERATED).exists());
    assert!(!project.root.join("src/player/generated.rs").exists());

    let installation = directory.root.join("unrelated-installation");
    fs::create_dir(&installation).unwrap();
    let relocated = installation.join("Palette");
    assert!(
        published
            .canonicalize()
            .unwrap()
            .starts_with(&directory.root)
    );
    assert!(
        installation
            .canonicalize()
            .unwrap()
            .starts_with(&directory.root)
    );
    assert!(!relocated.exists());
    fs::rename(&published, &relocated).unwrap();
    assert!(!published.exists());
    assert_eq!(
        fs::read(relocated.join("package.json")).unwrap(),
        manifest_bytes
    );
    assert!(
        project
            .root
            .join("source/palette.material2d.json")
            .is_file()
    );
    assert!(project.root.join("source/palette.wgsl").is_file());
    assert!(project.root.join("source/palette.png.meta").is_file());
    assert!(project.root.join("source-cache").is_dir());
    directory.remove_owned(&project.root);
    directory.remove_owned(&material_workspace::cooker_directory(&directory.root));
    assert!(!project.root.exists());

    let cwd = directory.root.join("empty-working-directory");
    fs::create_dir(&cwd).unwrap();
    let (directory, stdout) = OwnedProcess::player(directory, &relocated.join(&executable), &cwd)
        .run(Duration::from_secs(30));
    assert_eq!(stdout, format!("{PLAYER_MARKER}\n"));
    assert!(!project.root.exists());
    assert!(fs::read_dir(&cwd).unwrap().next().is_none());
    assert_eq!(
        fs::read(relocated.join("package.json")).unwrap(),
        manifest_bytes
    );
    material_workspace::assert_payload(&relocated, &executable);
    drop(directory);
}
