use std::{
    fs,
    path::Path,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use super::profile_font_product::{
    PROFILE_FONT_NATIVE_PROOF, PROFILE_FONT_PROOF, WORLD_CANVAS_NATIVE_PROOF, profile_font_recipe,
};

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
    let project = parent.join("font-profile");

    let mut create = Command::new(env!("CARGO_BIN_EXE_nara"));
    create
        .args(["new", "desktop-2d/v1", "font-profile", "--parent"])
        .arg(&parent)
        .arg("--local-checkout")
        .arg(checkout);
    successful(create.output().unwrap(), "create the ordinary starter");

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
        include_bytes!("../../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf"),
    )
    .unwrap();
    fs::write(
        project.join("profile-font-LICENSE"),
        include_bytes!("../../crates/nara_text/tests/fixtures/FiraMono-LICENSE"),
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
        include_bytes!("../../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
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

    let mut check = command(checkout, env!("CARGO_BIN_EXE_nara"));
    check.arg("check").arg(&project);
    let checked = successful(check.output().unwrap(), "compiled project Check");
    assert!(checked.contains("Project check passed"));
    assert!(
        !checked.contains(PROFILE_FONT_PROOF),
        "Check must not start gameplay"
    );

    let mut export = command(checkout, env!("CARGO_BIN_EXE_nara"));
    export
        .arg("export")
        .arg(&project)
        .arg("--parent")
        .arg(&parent)
        .args(["--destination", "font-candidate"]);
    let exported = successful(
        export.output().unwrap(),
        "saved-source Export and its product acceptance",
    );
    assert!(exported.contains("2 exact product observations"));

    // These binaries resolve their sibling exported project, including its saved UI profile.
    // Their shared fixture asserts actual shaping and rasterization before reporting success.
    let packaged = parent.join("font-candidate");
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
        run.current_dir(&parent).args(arguments);
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
    println!(
        "profile_font_native: compiled_check=true exported_startup=true native_first_present=true artifact={}",
        packaged.display()
    );
}

fn command(checkout: &Path, program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1");
    command
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
