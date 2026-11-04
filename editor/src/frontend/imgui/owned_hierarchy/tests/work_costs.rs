use super::*;

#[test]
fn unchanged_hierarchy_rebuilds_visible_description_without_recompiling_layout() {
    let (_project, application) = application(EditorDocumentKind::Scene, 128);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let work = owner.work;
    let compiler = owner.compiler.work_stats();
    for _ in 0..8 {
        assert!(Arc::ptr_eq(&frame, &paint(&mut owner, &model)));
    }
    assert_eq!(owner.work.compile_calls - work.compile_calls, 8);
    assert_eq!(owner.work.built_snapshots - work.built_snapshots, 8);
    assert!(owner.work.authored_nodes > work.authored_nodes);
    assert_eq!(owner.work.reused_frames - work.reused_frames, 8);
    assert_eq!(owner.compiler.work_stats(), compiler);
    assert!(frame.layouts().len() < 128);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}

#[test]
#[ignore = "explicit 10k/100k read-only projection CPU baseline; run serially with captured output"]
fn measure_owned_hierarchy_logical_rows_at_a_fixed_viewport() {
    let (_project, application) = application(EditorDocumentKind::Scene, 1);
    let original = application.current_frame().unwrap();
    for count in [10_000, 100_000] {
        let mut workspace = original.workspace().clone();
        let inspector = &mut workspace.active_scene.as_mut().unwrap().editor.inspector;
        let row = inspector.entities[0].clone();
        inspector.entities = (0..count)
            .map(|index| {
                let mut row = row.clone();
                row.id = entity(index);
                row.selected = index == 0;
                row.live_locator = None;
                row
            })
            .collect();
        let model = EditorFrameClock::new().publish(
            workspace,
            original.project().clone(),
            original.diagnostics().clone(),
            original.reopen_target(),
            original.field_feedback().cloned(),
        );
        assert_eq!(
            model
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .editor
                .inspector
                .entities
                .len(),
            count,
        );
        let mut owner = controller();
        let mut paint = || {
            owner
                .compile(&model, environment(800.0), UiTheme::default(), true)
                .unwrap()
        };
        let frame = paint();
        for _ in 0..16 {
            assert!(Arc::ptr_eq(&frame, &paint()));
        }
        let before = owner.work;
        let compiler = owner.compiler.work_stats();
        let elapsed = crate::frontend::imgui::owned_work::measure_calls(
            &format!("hierarchy_read_only_projection_{count}"),
            || {
                let next = owner
                    .compile(&model, environment(800.0), UiTheme::default(), true)
                    .unwrap();
                assert!(Arc::ptr_eq(&frame, &next));
            },
            512,
        );
        assert_eq!(owner.compiler.work_stats(), compiler);
        assert!(frame.layouts().len() < 128);
        println!(
            "UI_WORK hierarchy_read_only_projection_unchanged logical_rows={count} samples=512 percentile_ns={elapsed:?} live_layouts={} before={before:?} after={:?} compiler={compiler:?}",
            frame.layouts().len(),
            owner.work,
        );
        assert_eq!(
            application.current_frame().unwrap().workspace(),
            original.workspace()
        );
    }
}
