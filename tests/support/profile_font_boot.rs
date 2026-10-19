use super::{
    HeadlessRun, HeadlessRunOutcome, NonZeroU32, TestProject, execute_to_terminal,
    profile_font_product::{ProfileFontOutcome, profile_font_recipe},
};

#[test]
fn ordinary_recipe_configures_the_saved_source_headless_ui_profile() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let manifest_path = project.path().join("nara.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap().replace(
        "requested = [\"runtime-2d\"]",
        "requested = [\"runtime-2d\", \"runtime-ui\", \"presentation-ui\"]",
    );
    std::fs::write(
        manifest_path,
        format!("{manifest}\n[ui.primary_surface]\nlogical_width = 320\nlogical_height = 180\n"),
    )
    .unwrap();
    let (_, plan, _) = project.candidate_plan_and_root();
    std::fs::write(
        project.path().join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let face = nara::ui::FontFace::from_bytes(
        include_bytes!("../../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let recipe = profile_font_recipe(face, None).unwrap();

    #[cfg(feature = "project-workflow")]
    nara::project_workflow::check_project(nara::project_workflow::ProjectCheckRequest::new(
        project.root_capability(),
        recipe.clone(),
    ))
    .unwrap();

    let mut run = HeadlessRun::<ProfileFontOutcome>::from_recipe(
        project.root_capability(),
        recipe,
        NonZeroU32::new(1).unwrap(),
        Vec::new(),
    );
    let report = execute_to_terminal(&mut run);
    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(ProfileFontOutcome {
            compiled_selected_face: true,
            rendered_selected_face: true,
            presented_selected_face: false,
        }),
        "{report:?}"
    );
}
