//! Real compiled recipe failures through the ordinary external tool caller.

#![cfg(any(windows, target_os = "linux"))]
#![forbid(unsafe_code)]

use std::{
    fs,
    process::{Command, Stdio},
};

use engine::project_tooling::{
    ProjectToolFailure, ProjectToolFailureStage, ProjectToolInvocationError, ProjectToolRequest,
    invoke_project_tool,
};

#[path = "support/workspace.rs"]
#[allow(dead_code)]
mod workspace;

#[test]
#[ignore = "actual serial Cargo build and recipe failure through an independent project"]
fn actual_recipe_errors_keep_their_classification_without_running_gameplay() {
    let tools = workspace::Tools::pinned();
    let owner = workspace::Temporary::new();
    let project = &owner.root;
    fs::create_dir_all(project.join("src/bin")).unwrap();
    fs::create_dir(project.join(".cargo")).unwrap();
    fs::write(
        project.join(".cargo/config.toml"),
        format!(
            "[build]\njobs = 1\ntarget-dir = {}\n[net]\noffline = true\n",
            serde_json::to_string(tools.target.to_str().unwrap()).unwrap(),
        ),
    )
    .unwrap();
    let manifest = format!(
        "[package]\nname = 'nara-recipe-diagnostic-consumer'\nversion = '0.0.0'\nedition = '2024'\n\n\
         [package.metadata.nara]\nproject-tool-protocol = 1\n\n\
         [[bin]]\nname = 'headless'\npath = 'src/bin/headless.rs'\n\n\
         [dependencies]\nengine = {{ package = 'nara', path = {}, default-features = false, features = ['project-tooling'] }}\n\n\
         [workspace]\nresolver = '3'\n",
        serde_json::to_string(tools.repository.to_str().unwrap()).unwrap(),
    );
    fs::write(project.join("Cargo.toml"), &manifest).unwrap();
    fs::write(project.join("src/bin/headless.rs"), SOURCE).unwrap();
    fs::write(project.join("recipe-mode"), "duplicate").unwrap();
    let initialized = Command::new(&tools.cargo)
        .current_dir(project)
        .args(["generate-lockfile", "--offline"])
        .env("RUSTC", &tools.rustc)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr),
    );
    let lock = fs::read(project.join("Cargo.lock")).unwrap();
    let duplicate = failure(invoke_project_tool(ProjectToolRequest::Check {
        project: project.clone(),
        profile: None,
    }));
    assert_eq!(duplicate.stage(), ProjectToolFailureStage::Recipe);
    assert_eq!(duplicate.code(), "project.tool.recipe-duplicate-plugin");
    assert!(duplicate.summary().contains("consumer.recipe-duplicate"));
    assert_eq!(duplicate.operation(), "check");
    assert_eq!(fs::read(project.join("factory-calls")).unwrap(), b"1\n");

    let export = failure(invoke_project_tool(ProjectToolRequest::Export {
        project: project.clone(),
        parent: project.clone(),
        destination: "must-not-publish".into(),
    }));
    assert_eq!(export.operation(), "export");
    assert_eq!(export.stage(), duplicate.stage());
    assert_eq!(export.code(), duplicate.code());
    assert_eq!(export.summary(), duplicate.summary());
    assert!(!project.join("must-not-publish").exists());
    assert_eq!(fs::read(project.join("factory-calls")).unwrap(), b"1\n1\n");

    fs::write(project.join("recipe-mode"), "opaque").unwrap();
    let opaque = failure(invoke_project_tool(ProjectToolRequest::Schema {
        project: project.clone(),
        profile: None,
        write: true,
    }));
    assert_eq!(opaque.operation(), "schema-write");
    assert_eq!(opaque.stage(), ProjectToolFailureStage::Recipe);
    assert_eq!(opaque.code(), "project.tool.recipe-failed");
    assert_eq!(opaque.summary(), "Project recipe construction failed");
    assert!(!opaque.to_string().contains("private-formatter-canary"));
    assert_eq!(
        fs::read(project.join("factory-calls")).unwrap(),
        b"1\n1\n1\n"
    );
    assert!(!project.join("gameplay-was-run").exists());
    assert!(!project.join("plugin-was-built").exists());
    assert_eq!(
        fs::read(project.join("Cargo.toml")).unwrap(),
        manifest.as_bytes()
    );
    assert_eq!(fs::read(project.join("Cargo.lock")).unwrap(), lock);
    assert_eq!(
        fs::read(project.join("src/bin/headless.rs")).unwrap(),
        SOURCE.as_bytes()
    );
    println!(
        "RECIPE_DIAGNOSTICS_OK duplicate_plugin=true opaque_unformatted=true no_gameplay=true"
    );
}

fn failure(
    result: Result<engine::project_tooling::ProjectToolSummary, ProjectToolInvocationError>,
) -> ProjectToolFailure {
    match result {
        Err(ProjectToolInvocationError::Project(failure)) => failure,
        other => panic!("the compiled tool must return its structured recipe failure: {other:?}"),
    }
}

const SOURCE: &str = r#"use std::{fmt, fs::OpenOptions, io::Write, process::ExitCode};
use engine::{ProductRecipe, ProductRecipeError};
use engine::app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};

const DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("consumer.recipe-duplicate"), PluginCategory::Runtime,
);
#[derive(Default)]
struct DuplicatePlugin;
impl Plugin for DuplicatePlugin {
    fn declaration() -> &'static PluginDeclaration { &DECLARATION }
    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        std::fs::write("plugin-was-built", b"must not build").unwrap();
        Ok(())
    }
}

struct Opaque;
impl fmt::Debug for Opaque {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("private-formatter-canary");
    }
}
impl fmt::Display for Opaque {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("private-formatter-canary");
    }
}
impl std::error::Error for Opaque {}

fn called() {
    let mut file = OpenOptions::new().append(true).create(true).open("factory-calls").unwrap();
    writeln!(file, "1").unwrap();
}

fn recipe() -> Result<ProductRecipe, ProductRecipeError> {
    called();
    ProductRecipe::new().add_plugin::<DuplicatePlugin>()?.add_plugin::<DuplicatePlugin>()
}

fn main() -> ExitCode {
    let mode = std::fs::read_to_string("recipe-mode").unwrap();
    let exit = if mode == "opaque" {
        engine::project_tooling::run_project_tool_mode(std::env::args_os().skip(1), || {
            called();
            Err::<ProductRecipe, _>(Opaque)
        })
    } else {
        engine::project_tooling::run_project_tool_mode(std::env::args_os().skip(1), recipe)
    };
    if let Some(exit) = exit { return exit; }
    std::fs::write("gameplay-was-run", b"must not run").unwrap();
    ExitCode::SUCCESS
}
"#;
