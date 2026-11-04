use super::*;

#[test]
fn unchanged_inspector_reuses_the_frame_after_rebuilding_its_description() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut owner = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut owner, &mut fields, &model);
    let work = owner.work;
    let compiler = owner.compiler.work_stats();
    for _ in 0..8 {
        let next = paint(&mut owner, &mut fields, &model);
        assert!(Arc::ptr_eq(&frame, &next));
    }
    assert_eq!(owner.work.compile_calls - work.compile_calls, 8);
    assert_eq!(owner.work.built_snapshots - work.built_snapshots, 8);
    assert!(owner.work.authored_nodes > work.authored_nodes);
    assert_eq!(owner.work.reused_frames - work.reused_frames, 8);
    assert_eq!(owner.compiler.work_stats(), compiler);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}

#[test]
fn inspector_local_edit_reports_real_compile_work_without_publishing_document_content() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut owner = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut owner, &mut fields, &model);
    let field = input(&owner, &frame, "label");
    let before = owner.compiler.work_stats();
    replace(&mut owner, &mut fields, &frame, &field, "work baseline");
    let changed = paint(&mut owner, &mut fields, &model);
    assert!(!Arc::ptr_eq(&frame, &changed));
    let after = owner.compiler.work_stats();
    assert!(after.compile_attempts > before.compile_attempts);
    assert!(after.layout_nodes_created > before.layout_nodes_created);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}

#[test]
#[ignore = "explicit CPU work baseline; run serially with captured output"]
fn measure_owned_inspector_unchanged_and_local_edit_work() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut owner = new_controller();
    let mut fields = FieldEditors::default();
    let mut frame = paint(&mut owner, &mut fields, &model);
    for _ in 0..16 {
        frame = paint(&mut owner, &mut fields, &model);
    }
    let before = owner.work;
    let compiler = owner.compiler.work_stats();
    let elapsed = crate::frontend::imgui::owned_work::measure_calls(
        "inspector_unchanged",
        || {
            assert!(Arc::ptr_eq(&frame, &paint(&mut owner, &mut fields, &model),));
        },
        512,
    );
    assert_eq!(owner.compiler.work_stats(), compiler);
    println!(
        "UI_WORK inspector_unchanged samples=512 percentile_ns={elapsed:?} before={before:?} after={:?} compiler={compiler:?}",
        owner.work,
    );
    let field = input(&owner, &frame, "label");
    let mut edits = 0_u64;
    let elapsed = crate::frontend::imgui::owned_work::measure_calls(
        "inspector_local_edit",
        || {
            edits += 1;
            replace(
                &mut owner,
                &mut fields,
                &frame,
                &field,
                &format!("changed local input {edits}"),
            );
            frame = paint(&mut owner, &mut fields, &model);
        },
        128,
    );
    println!(
        "UI_WORK inspector_local_edit samples=128 percentile_ns={elapsed:?} work={:?} compiler={:?} editor_text={:?}",
        owner.work,
        owner.compiler.work_stats(),
        owner.engine.work_stats(),
    );
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}
