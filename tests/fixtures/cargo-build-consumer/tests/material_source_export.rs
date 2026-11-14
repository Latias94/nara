#![cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
#![forbid(unsafe_code)]

use std::{env, fs, time::Duration};

use engine::project_tooling::{ProjectToolRequest, ProjectToolSummary, invoke_project_tool};

#[path = "support/material_source_export/library.rs"]
#[allow(dead_code, unused_imports)]
mod library;
#[path = "support/material_source_export/process.rs"]
mod process;
#[path = "support/material_source_export/project.rs"]
mod project;
#[path = "support/workspace.rs"]
#[allow(dead_code)]
mod workspace;

use process::OwnedProcess;
use project::{Project, SOURCE_FILES};
use workspace::{Temporary, Tools};

const TEST_NAME: &str =
    "persistent_palette_source_export_builds_products_and_runs_relocated_headless";
const PROJECT_ENV: &str = "NARA_MATERIAL_SOURCE_EXPORT_PROJECT";
const EXPORT_PARENT_ENV: &str = "NARA_MATERIAL_SOURCE_EXPORT_PARENT";
const EXPORT_MARKER: &str = "palette_source_export: ok products=2";
const DESTINATION: &str = "palette-game";

#[test]
#[ignore = "actual serial locked Cargo builds and headless/Winit/WGPU source Export; requires root target and a native desktop session"]
fn persistent_palette_source_export_builds_products_and_runs_relocated_headless() {
    if let Some(project) = env::var_os(PROJECT_ENV) {
        let summary = invoke_project_tool(ProjectToolRequest::Export {
            project: project.into(),
            parent: env::var_os(EXPORT_PARENT_ENV).unwrap().into(),
            destination: DESTINATION.into(),
        })
        .unwrap_or_else(|error| panic!("public source Export failed: {error}"));
        assert_eq!(
            summary,
            ProjectToolSummary::Exported {
                content_members: SOURCE_FILES.len(),
                product_observations: 2,
            },
        );
        println!("{EXPORT_MARKER}");
        return;
    }

    let tools = Tools::pinned();
    let directory = Temporary::new();
    let project = Project::new(&directory.root, &tools);
    let exports = directory.root.join("exports");
    fs::create_dir(&exports).unwrap();
    let (directory, stdout) = OwnedProcess::export(directory, &tools, &project.root, &exports)
        .run(Duration::from_secs(40 * 60));
    assert!(stdout.contains(EXPORT_MARKER), "{stdout}");
    for input in [
        "Cargo.toml",
        "Cargo.lock",
        "src/lib.rs",
        "src/bin/headless.rs",
        "src/bin/desktop.rs",
    ] {
        assert!(project.root.join(input).is_file(), "{input}");
    }
    let published = exports.join(DESTINATION);
    project.assert_published(&published);
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
    fs::rename(&published, &relocated).unwrap();
    assert!(!published.exists());
    project.assert_published(&relocated);
    directory.remove_owned(&project.root);
    assert!(!project.root.exists());
    let unrelated_cwd = directory.root.join("empty-working-directory");
    fs::create_dir(&unrelated_cwd).unwrap();
    let (directory, stdout) =
        OwnedProcess::headless(directory, &relocated, &unrelated_cwd).run(Duration::from_secs(30));
    assert!(stdout.contains(library::RESIDENCY_MARKER), "{stdout}");
    assert!(
        stdout.contains("starter_headless: ok fixed_ticks=1"),
        "{stdout}",
    );
    project.assert_published(&relocated);
    assert!(!project.root.exists());
    assert!(fs::read_dir(&unrelated_cwd).unwrap().next().is_none());
    drop(directory);
}
