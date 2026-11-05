//! Independent read-only semantic inspection and accepted native observation qualification.
#![forbid(unsafe_code)]
#![cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]

use std::{fs, rc::Rc, sync::Arc, time::Duration};

use nara::{
    reflect::ComponentValue,
    tooling::{EditorAction, EditorActionOutcome, EditorPropertyValue},
};
use nara_editor::{
    EditorPanelHost, PanelActionDispatch,
    workspace::{
        EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement,
        EditorPanelSourceKind, EditorPanelTarget,
    },
};
use nara_editor_ui_host_consumer::{
    DraftPanel, Evidence, INITIAL,
    conformance::ConformancePanel,
    project::{Application, Project, SOURCE, current, entity, open_secondary, property, register},
};

#[test]
fn nara_panel_inspection_preserves_pending_edit_and_exact_original_result() {
    let project = Project::new();
    let mut application = Application::open(&project);
    let document = open_secondary(&mut application);
    let mut host = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    let instance = register(
        &mut host,
        &mut application,
        &evidence,
        "consumer.observed-caption",
    );
    let locator = current(&application)
        .workspace()
        .document(document)
        .unwrap()
        .source_binding
        .as_ref()
        .unwrap()
        .locator()
        .to_owned();
    assert!(application.panel_workspace_mut().layout_mut().set_target(
        instance,
        EditorPanelTarget::Source {
            kind: EditorPanelSourceKind::Scene,
            locator
        },
    ));
    let basis = application.document_query_basis(document).unwrap();
    let disk = fs::read(project.root().join(SOURCE)).unwrap();
    let mut captured = None;
    let candidate = host
        .prepare(&application, |panel, context| {
            panel.replace_for_cpu("inspected draft").unwrap();
            let prepared = panel
                .prepare_edit(context, &basis, entity("caption"))
                .unwrap();
            let id = context.commit_authoring_edit(prepared).unwrap();
            captured = Some((context.instance_token(), id));
        })
        .unwrap();
    let (token, id) = captured.unwrap();
    assert_eq!(token.kind(), "consumer.observed-caption");
    assert_eq!(token.document(), Some(document));
    assert_eq!(candidate.submissions().len(), 1);
    assert_eq!(
        candidate.submissions().collect::<Vec<_>>(),
        [(instance, id)]
    );
    let frame = current(&application);
    let foreign = EditorPanelHost::<DraftPanel>::new();
    assert!(
        foreign
            .inspect(&application, &token, |_, _| panic!(
                "foreign inspection visited the panel"
            ))
            .is_none()
    );
    for _ in 0..3 {
        assert_eq!(
            host.inspect(&application, &token, |panel, model| {
                assert_eq!(model.instance().id(), instance);
                assert_eq!(model.document().unwrap().document, document);
                assert!(std::ptr::eq(model.frame(), frame.as_ref()));
                (
                    panel.serial(),
                    panel.draft().to_owned(),
                    panel.input_token(),
                )
            })
            .map(|(serial, draft, input)| {
                assert!(input.is_some());
                (serial, draft)
            }),
            Some((1, "inspected draft".into()))
        );
        assert_eq!(
            candidate.submissions().collect::<Vec<_>>(),
            [(instance, id)]
        );
        assert!(evidence.finished.borrow().is_empty());
        assert!(evidence.dropped.borrow().is_empty());
        assert_eq!(evidence.created.get(), 1);
        assert!(Arc::ptr_eq(&frame, &current(&application)));
        assert_eq!(application.document_query_basis(document).unwrap(), basis);
    }
    host.submit_and_drive(&mut application, candidate, Duration::ZERO);
    {
        let finished = evidence.finished.borrow();
        assert_eq!(finished.len(), 1);
        assert_eq!((finished[0].0, finished[0].1), (1, id));
        assert!(matches!(
            &finished[0].2,
            PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied
        ));
    }
    let updated = application.document_query_basis(document).unwrap();
    assert_eq!(
        application
            .read_document_property(property(&updated))
            .unwrap()
            .value,
        EditorPropertyValue::Present(ComponentValue::String("inspected draft".into()))
    );
    assert_eq!(
        current(&application)
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        1
    );
    assert_eq!(fs::read(project.root().join(SOURCE)).unwrap(), disk);
    assert!(
        application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(instance, false)
    );
    assert!(
        application
            .panel_workspace_mut()
            .layout_mut()
            .reopen(instance)
    );
    assert!(!token.is_current(&application));
    assert!(
        host.inspect(&application, &token, |_, _| panic!(
            "retired inspection visited the panel"
        ))
        .is_none()
    );
    assert!(evidence.dropped.borrow().is_empty());
    let mut replacement = None;
    drop(
        host.prepare(&application, |_, context| {
            replacement = Some(context.instance_token())
        })
        .unwrap(),
    );
    let replacement = replacement.unwrap();
    assert_ne!(token, replacement);
    assert_eq!(*evidence.dropped.borrow(), [1]);
    assert!(
        host.inspect(&application, &token, |_, _| panic!(
            "old inspection reached its replacement"
        ))
        .is_none()
    );
    assert_eq!(
        host.inspect(&application, &replacement, |panel, _| (
            panel.serial(),
            panel.draft().to_owned()
        )),
        Some((2, INITIAL.into()))
    );
    assert_eq!(evidence.finished.borrow().len(), 1);
}

#[test]
fn foreign_frontend_inspection_keeps_same_controls_in_distinct_original_scopes() {
    let project = Project::new();
    let mut application = Application::open(&project);
    let document = open_secondary(&mut application);
    let evidence = Rc::new(Evidence::default());
    let observed = Rc::clone(&evidence);
    let mut host = EditorPanelHost::new();
    host.register_panel(
        &mut application,
        EditorPanelDescriptor::new(
            "consumer.observed-foreign",
            "User frontend",
            EditorPanelMultiplicity::Multiple,
            EditorPanelPlacement::Floating,
            true,
        ),
        move || ConformancePanel::new(Rc::clone(&observed)).unwrap(),
    )
    .unwrap();
    let first = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "consumer.observed-foreign")
        .unwrap()
        .id();
    let second = application
        .panel_workspace_mut()
        .open("consumer.observed-foreign", EditorPanelTarget::Workspace)
        .unwrap();
    let basis = application.document_query_basis(document).unwrap();
    let mut tokens = Vec::new();
    let mut id = None;
    let candidate = host
        .prepare(&application, |panel, context| {
            panel.focus_for_cpu().unwrap();
            tokens.push(context.instance_token());
            if context.model().instance().id() == first {
                id = Some(
                    context
                        .submit(EditorAction::ReadDocumentProperty(property(&basis)))
                        .unwrap(),
                );
            }
        })
        .unwrap();
    let id = id.unwrap();
    let first_token = tokens
        .iter()
        .find(|token| token.instance() == first)
        .unwrap();
    let second_token = tokens
        .iter()
        .find(|token| token.instance() == second)
        .unwrap();
    assert_eq!(candidate.submissions().len(), 1);
    assert_eq!(candidate.submissions().collect::<Vec<_>>(), [(first, id)]);
    assert_eq!(first_token.kind(), second_token.kind());
    assert_ne!(first_token, second_token);
    let first_snapshot = host
        .inspect(&application, first_token, |panel, _| panel.compose())
        .unwrap();
    let second_snapshot = host
        .inspect(&application, second_token, |panel, model| {
            assert!(model.document().is_none());
            panel.compose()
        })
        .unwrap();
    assert_eq!(first_snapshot.geometry(), second_snapshot.geometry());
    assert_eq!(first_snapshot.draft(), second_snapshot.draft());
    assert_ne!(first_snapshot.incarnation(), second_snapshot.incarnation());
    assert_ne!(first_snapshot.input_token(), second_snapshot.input_token());
    let foreign = EditorPanelHost::<ConformancePanel>::new();
    assert!(
        foreign
            .inspect(&application, first_token, |_, _| panic!(
                "foreign owner inspected retained controls"
            ))
            .is_none()
    );
    assert!(evidence.finished.borrow().is_empty());
    assert_eq!(evidence.created.get(), 2);
    host.submit_and_drive(&mut application, candidate, Duration::ZERO);
    let finished = evidence.finished.borrow();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].1, id);
    assert!(matches!(
        &finished[0].2,
        PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentProperty(Ok(read)))
            if read.value == EditorPropertyValue::Present(ComponentValue::String(INITIAL.into()))
    ));
    assert_eq!(
        host.inspect(&application, first_token, |panel, _| panel.compose()),
        Some(first_snapshot)
    );
    assert_eq!(
        host.inspect(&application, second_token, |panel, _| panel.compose()),
        Some(second_snapshot)
    );
}

#[cfg(feature = "native")]
#[test]
#[ignore = "requires a native desktop, hardware GPU and both prebuilt native consumer examples"]
fn independent_frontends_qualify_observed_operations_through_original_native_owners() {
    use std::{
        fs::File,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        time::Instant,
    };

    let backend = std::env::var("WGPU_BACKEND").expect("select WGPU_BACKEND explicitly");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    for (example, variable, baseline, marker) in [
        (
            "panel_surface",
            "NARA_EDITOR_PANEL_SURFACE_EXE",
            "EDITOR_UI_HOST_OK ",
            "EDITOR_UI_OBSERVATION_OK original_journal_reducer=true typed_focus_apply=true observed_key_text_ime=true exact_scope=true stale_output_instance=true foreign_host=true pending_submission_preserved=true exact_result_ids=true",
        ),
        (
            "conformance_surface",
            "NARA_EDITOR_CONFORMANCE_SURFACE_EXE",
            "EDITOR_FOREIGN_UI_OK ",
            "EDITOR_FOREIGN_UI_OBSERVATION_OK original_controls_reducer=true typed_focus_apply=true observed_key_text_ime=true two_panel_scope=true occluded_pointer=true stale_output_geometry_instance=true foreign_host=true capability_denied=true pending_submission_preserved=true exact_result_ids=true",
        ),
    ] {
        let executable = std::env::var_os(variable)
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .and_then(Path::parent)
                    .unwrap()
                    .join("examples")
                    .join(format!("{example}{}", std::env::consts::EXE_SUFFIX))
            });
        assert!(
            executable.is_absolute() && executable.is_file(),
            "prebuild native {example}: {}",
            executable.display()
        );
        let logs = tempfile::Builder::new()
            .prefix("nara-ui-observation-")
            .tempdir()
            .unwrap();
        let stdout = logs.path().join("stdout.log");
        let stderr = logs.path().join("stderr.log");
        let mut child = Command::new(executable)
            .arg("--observed-operations")
            .stdin(Stdio::null())
            .stdout(File::create(&stdout).unwrap())
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(90);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let path = logs.keep();
                    panic!(
                        "native observation {example} failed or timed out: {result:?}; logs: {}",
                        path.display()
                    );
                }
            }
        };
        for path in [&stdout, &stderr] {
            assert!(fs::metadata(path).unwrap().len() <= 64 * 1024);
        }
        let output = fs::read_to_string(&stdout).unwrap();
        let errors = fs::read_to_string(&stderr).unwrap();
        if !status.success() {
            let path = logs.keep();
            panic!(
                "native observation {example} failed: {status}; logs: {}\n{output}\n{errors}",
                path.display()
            );
        }
        assert!(
            output.lines().any(|line| line.starts_with(baseline)),
            "complete {example} trajectory did not finish:\n{output}\n{errors}"
        );
        assert!(
            output.lines().any(|line| line == marker),
            "observed {example} qualification did not finish:\n{output}\n{errors}"
        );
        if example == "conformance_surface" {
            assert!(output.lines().any(|line| line == "EDITOR_FOREIGN_UI_OBSERVATION_EDGE_OK modal_outside_original_provider=true modal_wheel_refused=true ime_disabled_without_scope=true stale_scope_commit_refused=true wrong_button_capture_preserved=true"),
                "native observation edge cases did not finish:\n{output}\n{errors}");
        }
    }
}
