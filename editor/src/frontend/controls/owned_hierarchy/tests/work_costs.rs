use super::*;
use crate::frontend::controls::{OwnedPanelWork, measure_calls};
use std::hint::black_box;

fn compile_probe(
    owner: &mut OwnedHierarchy,
    model: &EditorFrameModel,
    read_only: bool,
) -> Arc<UiFrame> {
    black_box(
        black_box(owner)
            .compile(
                black_box(model),
                black_box(environment(800.0)),
                black_box(UiTheme::default()),
                black_box(read_only),
            )
            .unwrap(),
    )
}

fn assert_unchanged_work(
    owner: &OwnedHierarchy,
    before: OwnedPanelWork,
    projection: HierarchyProjectionWork,
    samples: u64,
) {
    assert_eq!(owner.work.compile_calls - before.compile_calls, samples);
    assert_eq!(owner.work.built_snapshots, before.built_snapshots);
    assert_eq!(owner.work.authored_nodes, before.authored_nodes);
    assert_eq!(owner.work.reused_frames - before.reused_frames, samples);
    assert_eq!(owner.projection_work.refreshes, projection.refreshes);
    assert_eq!(owner.projection_work.source_rows, projection.source_rows);
    assert_eq!(
        owner.projection_work.filter_refreshes,
        projection.filter_refreshes,
    );
    assert_eq!(owner.projection_work.filter_rows, projection.filter_rows);
    assert_eq!(
        owner.projection_work.early_reused_frames - projection.early_reused_frames,
        samples,
    );
    assert_eq!(
        owner.projection_work.retained_index_bytes,
        projection.retained_index_bytes,
    );
}

#[cfg(feature = "ui-work-allocation-probes")]
fn assert_allocation_free_compile_loop(
    workload: &str,
    owner: &mut OwnedHierarchy,
    model: &EditorFrameModel,
    frame: &Arc<UiFrame>,
    read_only: bool,
) {
    let before = owner.work;
    let projection = owner.projection_work;
    let compiler = owner.compiler.work_stats();
    let ((), allocation) = crate::allocation_probe::observe(|| {
        for _ in 0..512 {
            let next = compile_probe(owner, model, read_only);
            assert!(Arc::ptr_eq(frame, &next));
        }
    });
    println!(
        "UI_ALLOC_LOOP workload={workload} compile_calls=512 observation_windows=1 scope=current_thread_successful_System_requests work={allocation:?}",
    );
    assert_eq!(
        allocation,
        crate::allocation_probe::AllocationWork::default()
    );
    assert_unchanged_work(owner, before, projection, 512);
    assert_eq!(owner.compiler.work_stats(), compiler);
}

#[test]
fn unchanged_hierarchy_reuses_before_description_projection_and_compiler_work() {
    let (_project, application) = application(EditorDocumentKind::Scene, 128);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let work = owner.work;
    let projection = owner.projection_work;
    let compiler = owner.compiler.work_stats();
    for _ in 0..8 {
        assert!(Arc::ptr_eq(&frame, &paint(&mut owner, &model)));
    }
    assert_unchanged_work(&owner, work, projection, 8);
    assert_eq!(owner.compiler.work_stats(), compiler);
    assert_eq!(owner.projection_work.retained_index_bytes, 0);
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
        let mut paint = || compile_probe(&mut owner, &model, true);
        let frame = paint();
        for _ in 0..16 {
            assert!(Arc::ptr_eq(&frame, &paint()));
        }
        let before = owner.work;
        let projection = owner.projection_work;
        let compiler = owner.compiler.work_stats();
        let elapsed = measure_calls(
            &format!("hierarchy_read_only_projection_{count}"),
            || {
                let next = compile_probe(&mut owner, &model, true);
                assert!(Arc::ptr_eq(&frame, &next));
            },
            512,
        );
        assert_unchanged_work(&owner, before, projection, 512);
        assert_eq!(owner.compiler.work_stats(), compiler);
        assert_eq!(owner.projection_work.retained_index_bytes, 0);
        assert!(frame.layouts().len() < 128);
        println!(
            "UI_WORK hierarchy_read_only_projection_unchanged logical_rows={count} warmups=16 samples=512 probe=black_box percentile_ns={elapsed:?} live_layouts={} before={before:?} after={:?} projection_before={projection:?} projection_after={:?} compiler={compiler:?}",
            frame.layouts().len(),
            owner.work,
            owner.projection_work,
        );
        #[cfg(feature = "ui-work-allocation-probes")]
        assert_allocation_free_compile_loop(
            &format!("hierarchy_read_only_projection_{count}"),
            &mut owner,
            &model,
            &frame,
            true,
        );
        assert_eq!(
            application.current_frame().unwrap().workspace(),
            original.workspace()
        );
    }
}

#[test]
#[ignore = "explicit admitted Application/Session compile-only CPU probe; run serially with captured output"]
fn measure_owned_hierarchy_admitted_application_at_a_fixed_viewport() {
    for count in [128, 1024] {
        let (project, application) = application(EditorDocumentKind::Scene, count);
        let model = application.current_frame().unwrap();
        let source = fs::read(project.source(EditorDocumentKind::Scene)).unwrap();
        let scene = model.workspace().active_scene.as_ref().unwrap();
        assert_eq!(scene.editor.inspector.entities.len(), count);
        let document = scene.document;
        let history = model.workspace().document(document).unwrap().history;
        let mut owner = controller();
        let frame = paint(&mut owner, &model);
        for _ in 0..16 {
            assert!(Arc::ptr_eq(
                &frame,
                &compile_probe(&mut owner, &model, false)
            ));
        }
        let before = owner.work;
        let projection = owner.projection_work;
        let compiler = owner.compiler.work_stats();
        let elapsed = measure_calls(
            &format!("hierarchy_admitted_application_{count}"),
            || {
                assert!(Arc::ptr_eq(
                    &frame,
                    &compile_probe(&mut owner, &model, false)
                ));
            },
            512,
        );
        assert_unchanged_work(&owner, before, projection, 512);
        assert_eq!(owner.compiler.work_stats(), compiler);
        assert_eq!(owner.projection_work.retained_index_bytes, 0);
        assert!(frame.layouts().len() < 128);
        println!(
            "UI_WORK hierarchy_admitted_application_unchanged logical_rows={count} warmups=16 samples=512 probe=black_box percentile_ns={elapsed:?} live_layouts={} before={before:?} after={:?} projection_before={projection:?} projection_after={:?} compiler={compiler:?}",
            frame.layouts().len(),
            owner.work,
            owner.projection_work,
        );
        #[cfg(feature = "ui-work-allocation-probes")]
        assert_allocation_free_compile_loop(
            &format!("hierarchy_admitted_application_{count}"),
            &mut owner,
            &model,
            &frame,
            false,
        );
        let unchanged = application.current_frame().unwrap();
        assert_eq!(unchanged.revision(), model.revision());
        assert_eq!(unchanged.workspace(), model.workspace());
        assert_eq!(
            unchanged.workspace().document(document).unwrap().history,
            history,
        );
        assert_eq!(
            fs::read(project.source(EditorDocumentKind::Scene)).unwrap(),
            source,
        );
    }
}
