use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use dear_imgui_rs::{ConfigFlags, MouseButton, Ui};
use winit::platform::windows::EventLoopBuilderExtWindows;

use super::*;
use crate::{
    ImguiEditorPanel, ImguiEditorPanelContext,
    frontend::imgui::UiProject,
    workspace::{
        EditorPanelDescriptor, EditorPanelInstanceId, EditorPanelMultiplicity,
        EditorPanelPlacement, EditorPanelTarget,
    },
};

#[derive(Default)]
struct PanelObservations {
    created: u64,
    drafts: BTreeMap<EditorPanelInstanceId, (u64, String, [f32; 2])>,
    dropped: Vec<u64>,
}

struct NativeDraftPanel {
    serial: u64,
    draft: String,
    observations: Rc<RefCell<PanelObservations>>,
}

impl ImguiEditorPanel for NativeDraftPanel {
    fn show(&mut self, ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        if ui.button("Append to draft") {
            self.draft.push('x');
        }
        let (min, max) = ui.item_rect();
        self.observations.borrow_mut().drafts.insert(
            context.model().instance().id(),
            (
                self.serial,
                self.draft.clone(),
                [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5],
            ),
        );
        ui.text(&self.draft);
    }
}

impl Drop for NativeDraftPanel {
    fn drop(&mut self) {
        self.observations.borrow_mut().dropped.push(self.serial);
    }
}

#[derive(Debug)]
struct NativePanelResult {
    initial_drafts: (String, String),
    recovered_drafts: (String, String),
    reconstruction_preserved_objects: bool,
    reconstruction_replaced_gpu: bool,
    reconstruction_preserved_frame_counter: bool,
    close_retired_only_first: bool,
    reopened_first_is_fresh: bool,
    reopen_observations: Vec<String>,
    document_unchanged: bool,
    factories: u64,
    dropped: usize,
    presents: u64,
}

#[test]
#[ignore = "requires an interactive Windows desktop and real DX12 GPU reconstruction"]
fn native_composed_panels_keep_independent_drafts_through_gpu_reconstruction() {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut trajectory = PanelTestApplication {
        project: project.0.clone(),
        result: None,
    };
    event_loop.run_app(&mut trajectory).unwrap();
    let observed = trajectory
        .result
        .expect("the native panel trajectory must execute")
        .unwrap();
    assert_eq!(observed.initial_drafts, ("x".to_owned(), "xx".to_owned()));
    assert_eq!(observed.recovered_drafts, observed.initial_drafts);
    assert!(observed.reconstruction_preserved_objects);
    assert!(observed.reconstruction_replaced_gpu);
    assert!(observed.reconstruction_preserved_frame_counter);
    assert!(observed.close_retired_only_first);
    assert!(
        observed.reopened_first_is_fresh,
        "reopen timeline: {:?}; result: {observed:#?}",
        observed.reopen_observations
    );
    assert!(observed.document_unchanged);
    assert_eq!(observed.factories, 3);
    assert_eq!(observed.dropped, 3);
    assert!(observed.presents >= 12);
}

struct PanelTestApplication {
    project: std::path::PathBuf,
    result: Option<Result<NativePanelResult, String>>,
}

impl ApplicationHandler for PanelTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_panels(event_loop, &self.project));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn configure_test_input(host: &mut NativeEditorHost) -> Result<(), String> {
    let native = host
        .native
        .as_mut()
        .ok_or("the native composition did not install")?;
    let mut flags = native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
    flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
    native.context.io_mut().set_config_flags(flags);
    Ok(())
}

fn paint(
    host: &mut NativeEditorHost,
    event_loop: &ActiveEventLoop,
    point: [f32; 2],
    pressed: Option<bool>,
) -> Result<u64, String> {
    host.application.advance_project_open();
    host.application
        .drive_editor_frame(Duration::ZERO, false, false);
    let native = host
        .native
        .as_mut()
        .ok_or("the native owner is unavailable")?;
    let exited = native
        .redraw_with_input(event_loop, &mut host.application, |context| {
            let viewport = context.main_viewport().id();
            let io = context.io_mut();
            // Input is confined to the owned hidden Context; no system pointer/focus is changed.
            io.clear_events_queue();
            io.add_focus_event(true);
            io.add_mouse_viewport_event(viewport);
            io.add_mouse_pos_event(point);
            if let Some(pressed) = pressed {
                io.add_mouse_button_event(MouseButton::Left, pressed);
            }
        })
        .map_err(|error| error.to_string())?;
    if exited {
        return Err("a contributed draft unexpectedly exited the Editor".to_owned());
    }
    Ok(native.gpu.frame_transaction_stats().presents())
}

fn click(
    host: &mut NativeEditorHost,
    event_loop: &ActiveEventLoop,
    point: [f32; 2],
) -> Result<u64, String> {
    let mut presents = paint(host, event_loop, point, None)?;
    presents += paint(host, event_loop, point, Some(true))?;
    presents += paint(host, event_loop, point, Some(false))?;
    Ok(presents)
}

fn run_native_panels(
    event_loop: &ActiveEventLoop,
    project: &std::path::Path,
) -> Result<NativePanelResult, String> {
    let application = EditorApplication::open_trusted_compiled(
        crate::EditorLaunchOptions::new(project).with_profile("desktop"),
        nara::project_host::TrustedCompiledEditorProjectIntent::new(),
    )
    .map_err(|error| error.to_string())?;
    let observations = Rc::new(RefCell::new(PanelObservations::default()));
    let shared = Rc::clone(&observations);
    let mut composition = EditorNativeComposition::new();
    composition
        .register_panel(
            EditorPanelDescriptor::new(
                "sample.native-draft",
                "Native draft",
                EditorPanelMultiplicity::Multiple,
                EditorPanelPlacement::Left,
                true,
            ),
            move || {
                let serial = {
                    let mut observed = shared.borrow_mut();
                    observed.created += 1;
                    observed.created
                };
                NativeDraftPanel {
                    serial,
                    draft: String::new(),
                    observations: Rc::clone(&shared),
                }
            },
        )
        .map_err(|error| error.to_string())?;
    // This is the production constructor called by EditorNativeComposition::run and the trusted
    // runner. It receives the descriptor/factory pair, rather than an installed test frontend.
    let mut host = NativeEditorHost::new(application, SupervisedPlayWake::default(), composition)
        .map_err(|error| error.to_string())?;
    host.window = Some(
        super::lifecycle_tests::hidden_window(event_loop, "Nara native composed panel proof")
            .map_err(|error| error.to_string())?,
    );
    let result = (|| {
        let first = host
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "sample.native-draft")
            .ok_or("the paired panel descriptor was not installed")?
            .id();
        let builtin_ids = host
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .filter(|panel| panel.kind() != "sample.native-draft")
            .map(|panel| panel.id())
            .collect::<Vec<_>>();
        for id in builtin_ids {
            host.application
                .panel_workspace_mut()
                .layout_mut()
                .set_visible(id, false);
        }
        let second = host
            .application
            .panel_workspace_mut()
            .open("sample.native-draft", EditorPanelTarget::ActiveDocument)
            .map_err(|error| error.to_string())?;
        host.application
            .panel_workspace_mut()
            .layout_mut()
            .place(second, EditorPanelPlacement::Right);
        host.resumed(event_loop);
        if let Some(error) = &host.error {
            return Err(error.to_string());
        }
        configure_test_input(&mut host)?;
        let mut presents = 0;
        for _ in 0..4 {
            presents += paint(&mut host, event_loop, [-1000.0, -1000.0], None)?;
        }
        let before_frame = host
            .application
            .current_frame()
            .ok_or("the ordinary native startup did not publish a document")?;
        let document = before_frame
            .workspace()
            .active_document
            .ok_or("startup has no document")?;
        let before_document = before_frame
            .workspace()
            .document(document)
            .cloned()
            .ok_or("the startup document is missing")?;
        let first_before = observations
            .borrow()
            .drafts
            .get(&first)
            .cloned()
            .ok_or("the first composed panel was not painted")?;
        let second_before = observations
            .borrow()
            .drafts
            .get(&second)
            .cloned()
            .ok_or("the second composed panel was not painted")?;
        presents += click(&mut host, event_loop, first_before.2)?;
        presents += click(&mut host, event_loop, second_before.2)?;
        presents += click(&mut host, event_loop, second_before.2)?;
        let initial_drafts = (
            observations.borrow().drafts[&first].1.clone(),
            observations.borrow().drafts[&second].1.clone(),
        );
        let old_backend = host.native.as_ref().unwrap().gpu.backend_instance_id();
        let old_frame = host.native.as_ref().unwrap().frame_index;
        let old_next_view = host.native.as_ref().unwrap().next_view;
        host.replace_native(event_loop);
        configure_test_input(&mut host)?;
        let reconstruction_preserved_frame_counter = host.native.as_ref().unwrap().frame_index
            == old_frame
            && host.native.as_ref().unwrap().next_view == old_next_view;
        for _ in 0..4 {
            presents += paint(&mut host, event_loop, [-1000.0, -1000.0], None)?;
        }
        let reconstruction_replaced_gpu =
            host.native.as_ref().unwrap().gpu.backend_instance_id() != old_backend;
        let reconstruction_preserved_objects = observations.borrow().created == 2
            && observations.borrow().dropped.is_empty()
            && observations.borrow().drafts[&first].0 == first_before.0
            && observations.borrow().drafts[&second].0 == second_before.0;
        let recovered_drafts = (
            observations.borrow().drafts[&first].1.clone(),
            observations.borrow().drafts[&second].1.clone(),
        );
        host.application
            .panel_workspace_mut()
            .layout_mut()
            .close(first);
        presents += paint(&mut host, event_loop, [-1000.0, -1000.0], None)?;
        let close_retired_only_first = observations.borrow().dropped == [first_before.0]
            && observations.borrow().drafts[&second].0 == second_before.0
            && observations.borrow().drafts[&second].1 == "xx";
        host.application
            .panel_workspace_mut()
            .layout_mut()
            .reopen(first);
        // Dear ImGui may defer a reopened dock window's contents while resolving its geometry.
        // Wait for a new painter observation, never treat the last pre-close observation as a
        // reopened object. The finite bound still fails if the production window cannot reopen.
        observations.borrow_mut().drafts.remove(&first);
        let mut reopen_observations = Vec::new();
        for attempt in 0..8 {
            presents += paint(&mut host, event_loop, [-1000.0, -1000.0], None)?;
            let observed = observations.borrow();
            reopen_observations.push(format!(
                "attempt={attempt}; frame={}; created={}; dropped={:?}; reopened={:?}; sibling={:?}; layout={:?}",
                host.native.as_ref().unwrap().frame_index,
                observed.created,
                observed.dropped,
                observed.drafts.get(&first),
                observed.drafts.get(&second),
                host.application.panel_workspace().layout().instance(first),
            ));
            if observed.drafts.contains_key(&first) {
                break;
            }
        }
        let reopened_first_is_fresh = observations
            .borrow()
            .drafts
            .get(&first)
            .is_some_and(|(serial, draft, _)| *serial != first_before.0 && draft.is_empty());
        let document_unchanged = host
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document)
            == Some(&before_document);
        Ok(NativePanelResult {
            initial_drafts,
            recovered_drafts,
            reconstruction_preserved_objects,
            reconstruction_replaced_gpu,
            reconstruction_preserved_frame_counter,
            close_retired_only_first,
            reopened_first_is_fresh,
            reopen_observations,
            document_unchanged,
            factories: observations.borrow().created,
            dropped: 0,
            presents,
        })
    })();
    let graphics = host
        .native
        .as_mut()
        .map_or(Ok(()), NativeImgui::shutdown_graphics)
        .map_err(|error| error.to_string());
    let session = super::canvas_tests::retire_application(&mut host.application);
    drop(host);
    match (result, graphics, session) {
        (Ok(mut observed), Ok(()), Ok(())) => {
            observed.dropped = observations.borrow().dropped.len();
            Ok(observed)
        }
        (result, graphics, session) => {
            let mut failures = Vec::new();
            if let Err(error) = result {
                failures.push(error);
            }
            if let Err(error) = graphics {
                failures.push(format!("native cleanup failed: {error}"));
            }
            if let Err(error) = session {
                failures.push(format!("Session cleanup failed: {error}"));
            }
            Err(failures.join("; "))
        }
    }
}
