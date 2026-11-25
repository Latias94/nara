use std::{
    fs,
    path::Path,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    project_tooling::{ProjectToolRequest, ProjectToolSummary, invoke_project_tool},
    project_workflow::ProjectStarter,
};

use super::profile_font_product::{
    PROFILE_FONT_NATIVE_PROOF, PROFILE_FONT_PROOF, WORLD_CANVAS_NATIVE_PROOF, profile_font_recipe,
};

#[path = "authored_project.rs"]
mod authored_project;

static NEXT_TRAJECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
#[ignore = "builds and exports one independent UI product and executes its real native desktop first-presentation canary"]
fn fixed_world_canvas_and_font_recipe_survive_check_export_and_native_restart() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT_TRAJECTORY.fetch_add(1, Ordering::Relaxed);
    let parent = checkout.join("target").join(format!(
        "nara-profile-font-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&parent).unwrap();
    let project = authored_project::create_authored_project(
        checkout,
        &parent,
        "font-profile",
        ProjectStarter::Desktop2dV1,
    );

    let cargo_path = project.join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).unwrap().replace(
        "features = [\"runtime-2d\", \"serde\", \"project-tooling\"]",
        "features = [\"runtime-2d\", \"serde\", \"project-tooling\", \"presentation-ui\"]",
    );
    assert!(cargo.contains("\"project-tooling\", \"presentation-ui\""));
    fs::write(cargo_path, cargo).unwrap();
    let manifest_path = project.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap()
        .replace("requested = [\"runtime-2d\"]", "requested = [\"runtime-2d\", \"runtime-ui\", \"presentation-ui\"]")
        .replace("requested = [\"runtime-2d\", \"presentation-2d\", \"desktop-winit\", \"render-wgpu\"]",
            "requested = [\"runtime-2d\", \"presentation-2d\", \"runtime-ui\", \"presentation-ui\", \"desktop-winit\", \"render-wgpu\"]");
    fs::write(
        manifest_path,
        format!("{manifest}\n[ui.primary_surface]\nlogical_width = 320\nlogical_height = 180\n\n[render.world_canvas]\nwidth = 320\nheight = 180\n"),
    )
    .unwrap();

    fs::write(
        project.join("profile-font.ttf"),
        include_bytes!("../../crates/nara_ui/tests/text/fixtures/FiraMono-Medium.ttf"),
    )
    .unwrap();
    fs::write(
        project.join("profile-font-LICENSE"),
        include_bytes!("../../crates/nara_ui/tests/text/fixtures/FiraMono-LICENSE"),
    )
    .unwrap();
    fs::write(
        project.join("src/profile_font.rs"),
        include_str!("profile_font_product.rs"),
    )
    .unwrap();
    let library_path = project.join("src/lib.rs");
    let library = fs::read_to_string(&library_path).unwrap().replace(
        "    nara::ProductRecipe::new()",
        "    profile_font::profile_font_recipe(nara::ui::FontFace::from_bytes(include_bytes!(\"../profile-font.ttf\").to_vec(), 0).unwrap(), None).unwrap()",
    );
    assert!(library.contains("profile_font::profile_font_recipe"));
    fs::write(library_path, format!("{library}\nmod profile_font;\n")).unwrap();

    // Recreate the saved data-only schema bundle from the ordinary recipe before strict Check.
    let project_root = nara::project_tooling::host_fs::readable_directory(&project).unwrap();
    let manifest = project_root
        .open_file(&nara::fs::RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = nara::project_host::ingest_project_manifest(&manifest, None).unwrap();
    let face = nara::ui::FontFace::from_bytes(
        include_bytes!("../../crates/nara_ui/tests/text/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let plan = nara::project_host::resolve_product_recipe(
        &candidate,
        profile_font_recipe(face, None).unwrap(),
    )
    .unwrap();
    fs::write(
        project.join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    // Product output would invalidate the compiled tool's structured Check response.
    let checked = invoke_project_tool(ProjectToolRequest::Check {
        project: project.clone(),
        profile: None,
    })
    .unwrap();
    assert!(
        matches!(
            checked,
            ProjectToolSummary::Checked {
                content_members: 1..
            }
        ),
        "{checked:?}"
    );

    let exported = invoke_project_tool(ProjectToolRequest::Export {
        project,
        parent: parent.clone(),
        destination: "font-candidate".into(),
    })
    .unwrap();
    assert!(
        matches!(
            exported,
            ProjectToolSummary::Exported {
                product_observations: 2,
                ..
            }
        ),
        "{exported:?}"
    );

    // These binaries resolve their sibling exported project, including its saved UI profile.
    // Their shared fixture asserts actual shaping and rasterization before reporting success.
    let packaged = parent.join("font-candidate");
    let unrelated = parent.join("unrelated-cwd");
    fs::create_dir(&unrelated).unwrap();
    for (label, target, arguments, success) in [
        (
            "headless",
            "headless",
            Vec::<&str>::new(),
            "starter_headless: ok",
        ),
        (
            "desktop",
            "desktop",
            vec!["--verify-first-present"],
            "starter_desktop: ok first_present_verified=true",
        ),
        (
            "desktop restart",
            "desktop",
            vec!["--verify-first-present"],
            "starter_desktop: ok first_present_verified=true",
        ),
    ] {
        let executable = packaged
            .join("bin")
            .join(format!("{target}{}", std::env::consts::EXE_SUFFIX));
        let mut run = Command::new(executable);
        run.current_dir(&unrelated).args(arguments);
        let output = successful(run.output().unwrap(), label);
        assert!(
            output.contains(PROFILE_FONT_PROOF),
            "{target} did not prove its selected font: {output}"
        );
        if target == "desktop" {
            assert!(
                output.contains(WORLD_CANVAS_NATIVE_PROOF),
                "{label} omitted the canvas/picking proof: {output}"
            );
            assert!(
                output.contains(PROFILE_FONT_NATIVE_PROOF),
                "desktop omitted its owner-produced UI presentation: {output}"
            );
        }
        assert!(output.contains(success), "{output}");
    }
    assert!(fs::read_dir(&unrelated).unwrap().next().is_none());
    println!(
        "profile_font_native: compiled_check=true exported_startup=true native_first_present=true artifact={}",
        packaged.display()
    );
}

fn successful(output: Output, operation: &str) -> String {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
