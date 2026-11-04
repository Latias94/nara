use super::*;

use nara::tooling::EditorDocumentKind;
use nara_text::TextWorkStats;
use nara_ui_kernel::{
    UiColor, UiCompileWorkStats, UiInputEvent, UiInteractionSession, UiPointerId, UiResolvedBrush,
    UiRouteStamp, UiSeatId, UiThemePalette,
};

use crate::frontend::imgui::owned_work::{OwnedPanelWork, measure_calls};

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

const PRESSURE_WARMUPS: usize = 16;
const PRESSURE_SAMPLES: usize = 128;
const PRESSURE_DRAFT: &str = "retained R11a draft";

#[derive(Clone, Copy, Debug)]
enum InspectorPressure {
    LocalColor,
    ThemePalette,
    FontEnvironment,
    Dpi,
    Width,
}

impl InspectorPressure {
    const ALL: [Self; 5] = [
        Self::LocalColor,
        Self::ThemePalette,
        Self::FontEnvironment,
        Self::Dpi,
        Self::Width,
    ];

    fn workload(self) -> &'static str {
        match self {
            Self::LocalColor => "inspector_local_color",
            Self::ThemePalette => "inspector_theme_palette",
            Self::FontEnvironment => "inspector_font_environment_revision",
            Self::Dpi => "inspector_dpi",
            Self::Width => "inspector_width",
        }
    }

    fn scope(self) -> &'static str {
        match self {
            Self::LocalColor => {
                "Toggle_hover_alternates_secondary_text/panel;prepared_reducer_states"
            }
            Self::ThemePalette => "distinct_panel+primary_text_palette_and_matching_theme_revision",
            Self::FontEnvironment => {
                "font_revision_only;immutable_single_face;no_typography_or_fallback_change"
            }
            Self::Dpi => {
                "scale_alternates_1.5/1.0;synthetic_geometry+presentation_revision_advances"
            }
            Self::Width => {
                "logical_width_alternates_720/600;synthetic_geometry+presentation_revision_advances"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PressureInput {
    environment: UiEnvironment,
    theme: UiTheme,
    toggle_state: Option<UiButtonState>,
}

fn pressure_environment(
    width: f32,
    scale: f32,
    presentation: u64,
    theme: u64,
    font: u64,
) -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(91).unwrap(),
        width,
        1600.0,
        scale,
        1,
        presentation,
        theme,
        font,
    )
    .unwrap()
}

fn pressure_palette(index: usize) -> UiThemePalette {
    let channel = index as f32 / 1024.0;
    UiThemePalette {
        panel: UiColor::rgba(0.2 + channel, 0.15, 0.25, 1.0),
        primary_text: UiColor::rgba(0.6 + channel, 0.8, 0.9, 1.0),
        ..UiThemePalette::default()
    }
}

fn pressure_inputs(
    pressure: InspectorPressure,
    count: usize,
    toggle_states: [UiButtonState; 2],
) -> Vec<PressureInput> {
    (0..count)
        .map(|index| {
            let revision = index as u64 + 2;
            let mut input = PressureInput {
                environment: pressure_environment(600.0, 1.0, 1, 1, 1),
                theme: UiTheme::default(),
                toggle_state: None,
            };
            match pressure {
                InspectorPressure::LocalColor => {
                    input.toggle_state = Some(toggle_states[index % 2]);
                }
                InspectorPressure::ThemePalette => {
                    input.theme = UiTheme::new(revision, pressure_palette(index)).unwrap();
                    input.environment = pressure_environment(600.0, 1.0, 1, revision, 1);
                }
                InspectorPressure::FontEnvironment => {
                    input.environment = pressure_environment(600.0, 1.0, 1, 1, revision);
                }
                InspectorPressure::Dpi => {
                    let scale = if index % 2 == 0 { 1.5 } else { 1.0 };
                    input.environment = pressure_environment(600.0, scale, revision, 1, 1);
                }
                InspectorPressure::Width => {
                    let width = if index % 2 == 0 { 720.0 } else { 600.0 };
                    input.environment = pressure_environment(width, 1.0, revision, 1, 1);
                }
            }
            input
        })
        .collect()
}

struct PressurePanel {
    owner: OwnedInspector,
    fields: FieldEditors,
    frame: Arc<UiFrame>,
    label: UiKey,
    toggle: UiKey,
    toggle_field: FieldKey,
    toggle_states: [UiButtonState; 2],
}

impl PressurePanel {
    fn new(model: &EditorFrameModel) -> Self {
        let mut owner = new_controller();
        let mut fields = FieldEditors::default();
        let frame = paint(&mut owner, &mut fields, model);
        let label = input(&owner, &frame, "label");
        // One local edit during setup; no sample edits or publishes source/history.
        replace(&mut owner, &mut fields, &frame, &label, PRESSURE_DRAFT);
        let frame = paint(&mut owner, &mut fields, model);
        let toggle = boolean(&owner, &frame);
        let toggle_field = owner.live_field(&frame, &toggle).unwrap().key.clone();
        let toggle_states = prepared_toggle_states(&mut owner, &frame, &toggle, &toggle_field);
        Self {
            owner,
            fields,
            frame,
            label,
            toggle,
            toggle_field,
            toggle_states,
        }
    }

    fn change(&mut self, model: &EditorFrameModel, input: PressureInput) {
        if let Some(state) = input.toggle_state {
            let control = self.owner.controls.get_mut(&self.toggle_field).unwrap();
            control.buttons[InspectorOperation::Toggle.index()] = state;
        }
        self.fields.begin_paint(model);
        let next = self
            .owner
            .compile(
                model,
                &mut self.fields,
                input.environment,
                input.theme,
                false,
                None,
            )
            .unwrap();
        // Every call must reach compilation, including when two visual states alternate.
        assert!(!Arc::ptr_eq(&self.frame, &next));
        assert!(next.revision().get() > self.frame.revision().get());
        assert!(next.diagnostics().is_empty(), "{:?}", next.diagnostics());
        assert_eq!(next.root_outcomes().len(), 1);
        assert_eq!(next.root_outcomes()[0].status(), UiRootStatus::Published);
        self.frame = next;
    }

    fn assert_local_history(&mut self) {
        let key = field_key(&self.owner, &self.frame, &self.label);
        let token = self
            .owner
            .input_token(&self.frame, &self.label)
            .unwrap()
            .unwrap();
        self.owner
            .edit(
                &self.frame,
                &self.label,
                token,
                &TextEdit::Undo,
                &mut self.fields,
            )
            .unwrap();
        assert_eq!(self.fields.drafts[&key].candidate.copy_text(), "before");
        self.owner
            .edit(
                &self.frame,
                &self.label,
                token,
                &TextEdit::Redo,
                &mut self.fields,
            )
            .unwrap();
        assert_eq!(
            self.fields.drafts[&key].candidate.copy_text(),
            PRESSURE_DRAFT
        );
    }
}

fn prepared_toggle_states(
    owner: &mut OwnedInspector,
    frame: &Arc<UiFrame>,
    toggle: &UiKey,
    field: &FieldKey,
) -> [UiButtonState; 2] {
    let semantic = frame
        .semantics()
        .iter()
        .find(|node| node.key() == toggle)
        .unwrap();
    assert!(semantic.enabled());
    let rect = semantic.rect();
    let inside = UiPoint::new(
        rect.origin().x() + rect.size().width() / 2.0,
        rect.origin().y() + rect.size().height() / 2.0,
    );
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(frame.environment().surface_id(), 1)
        .unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(frame), 1)
        .unwrap();
    let mut interaction = UiInteractionSession::default();
    // This CPU-only journal prepares real reducer states. It supplies no native/OS evidence.
    // Routing and state preparation stay outside the measured compile calls.
    [inside, UiPoint::new(0.0, 0.0)].map(|position| {
        let dispatch = interaction
            .dispatch(
                &journal,
                UiRouteStamp::from_receipt(receipt),
                UiInputEvent::PointerMoved {
                    seat: UiSeatId::PRIMARY,
                    pointer: UiPointerId::PRIMARY,
                    position,
                },
            )
            .unwrap();
        let intents = owner.dispatch(frame, dispatch.consumed().unwrap()).unwrap();
        assert!(intents.intents().is_empty());
        let state = owner.controls[field].buttons[InspectorOperation::Toggle.index()];
        assert_eq!(state.hovered(), position == inside);
        assert!(!state.pressed() && !state.focused());
        state
    })
}

fn observed_drafts(fields: &FieldEditors) -> HashMap<FieldKey, (DisplayedDraft, Option<String>)> {
    fields
        .drafts
        .iter()
        .map(|(key, draft)| {
            (
                key.clone(),
                (DisplayedDraft::observe(draft), draft.error.clone()),
            )
        })
        .collect()
}

fn fixture_sources(project: &InteractionProject) -> [Vec<u8>; 2] {
    [EditorDocumentKind::Scene, EditorDocumentKind::Prefab]
        .map(|kind| fs::read(project.source(kind)).unwrap())
}

fn assert_pressure_source(
    project: &InteractionProject,
    application: &EditorApplication,
    model: &EditorFrameModel,
    sources: &[Vec<u8>; 2],
) {
    let current = application.current_frame().unwrap();
    assert_eq!(current.revision(), model.revision());
    // Includes exact source revisions/values, selection, dirty state and Undo/Redo history.
    assert_eq!(current.workspace(), model.workspace());
    assert_eq!(&fixture_sources(project), sources);
}

fn assert_pressure_output(
    panel: &PressurePanel,
    baseline: &UiFrame,
    pressure: InspectorPressure,
    input: PressureInput,
) {
    let frame = &panel.frame;
    assert_eq!(frame.environment(), input.environment);
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    assert_eq!(frame.root_outcomes().len(), 1);
    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Published);
    assert_eq!(frame.layouts().len(), baseline.layouts().len());
    assert_eq!(frame.semantics().len(), baseline.semantics().len());
    for (before, after) in baseline.semantics().iter().zip(frame.semantics()) {
        assert_eq!(before.key(), after.key());
        assert_eq!(before.parent(), after.parent());
        assert_eq!(before.role(), after.role());
        assert_eq!(before.name(), after.name());
        assert_eq!(before.value(), after.value());
        assert_eq!(before.enabled(), after.enabled());
        assert_eq!(before.selected(), after.selected());
        assert_eq!(before.expanded(), after.expanded());
        assert_eq!(before.actions(), after.actions());
    }
    let field = frame
        .semantics()
        .iter()
        .find(|node| node.key() == &panel.label)
        .unwrap();
    assert_eq!(field.value(), Some(PRESSURE_DRAFT));
    assert_eq!(
        field.rect().size().width(),
        input.environment.logical_size().width() - 24.0,
    );
    assert_eq!(field.rect().size().height(), 36.0);
    for layout in frame.layouts() {
        assert_eq!(
            input.environment.physical_rect(layout.rect()),
            Some(layout.physical_rect()),
        );
    }
    match pressure {
        InspectorPressure::LocalColor
        | InspectorPressure::ThemePalette
        | InspectorPressure::FontEnvironment => {
            assert_eq!(baseline.layouts(), frame.layouts());
            assert_eq!(baseline.semantics(), frame.semantics());
        }
        InspectorPressure::Dpi => {
            if input.environment.scale_factor() != baseline.environment().scale_factor() {
                assert!(
                    baseline
                        .layouts()
                        .iter()
                        .zip(frame.layouts())
                        .any(|(before, after)| before.physical_rect() != after.physical_rect())
                );
            }
        }
        InspectorPressure::Width => {
            let before = baseline
                .semantics()
                .iter()
                .find(|node| node.key() == &panel.label)
                .unwrap();
            assert_eq!(
                field.rect().size().width() - before.rect().size().width(),
                input.environment.logical_size().width()
                    - baseline.environment().logical_size().width(),
            );
        }
    }
    let palette = input.theme.palette();
    let expected = if input.toggle_state.is_some_and(UiButtonState::hovered) {
        palette.secondary_text
    } else {
        palette.panel
    };
    let brush = frame.paint().iter().find_map(|primitive| match primitive {
        UiPaintPrimitive::Rect {
            key: Some(key),
            brush,
            ..
        } if key == &panel.toggle => brush.as_ref(),
        _ => None,
    });
    assert_eq!(brush, Some(&UiResolvedBrush::Solid(expected)));
    let mut text_primitives = 0;
    for primitive in frame.paint() {
        if let UiPaintPrimitive::Text { color, layout, .. } = primitive {
            text_primitives += 1;
            assert_eq!(*color, palette.primary_text);
            assert!(
                layout
                    .glyphs()
                    .iter()
                    .all(|glyph| glyph.key().face_id() == panel.owner.engine.font_face_id())
            );
        }
    }
    assert!(text_primitives > 0);
}

fn text_work_delta(before: TextWorkStats, after: TextWorkStats) -> TextWorkStats {
    TextWorkStats {
        layout_attempts: after.layout_attempts - before.layout_attempts,
        layout_source_bytes: after.layout_source_bytes - before.layout_source_bytes,
        shaped_glyphs: after.shaped_glyphs - before.shaped_glyphs,
        raster_attempts: after.raster_attempts - before.raster_attempts,
        raster_output_bytes: after.raster_output_bytes - before.raster_output_bytes,
        paint_window_attempts: after.paint_window_attempts - before.paint_window_attempts,
        paint_window_glyphs_scanned: after.paint_window_glyphs_scanned
            - before.paint_window_glyphs_scanned,
    }
}

fn panel_work_delta(before: OwnedPanelWork, after: OwnedPanelWork) -> OwnedPanelWork {
    OwnedPanelWork {
        compile_calls: after.compile_calls - before.compile_calls,
        built_snapshots: after.built_snapshots - before.built_snapshots,
        authored_nodes: after.authored_nodes - before.authored_nodes,
        reused_frames: after.reused_frames - before.reused_frames,
    }
}

fn compiler_work_delta(
    before: UiCompileWorkStats,
    after: UiCompileWorkStats,
) -> UiCompileWorkStats {
    UiCompileWorkStats {
        compile_attempts: after.compile_attempts - before.compile_attempts,
        published_frames: after.published_frames - before.published_frames,
        root_compile_attempts: after.root_compile_attempts - before.root_compile_attempts,
        layout_nodes_created: after.layout_nodes_created - before.layout_nodes_created,
        layout_passes: after.layout_passes - before.layout_passes,
        measurement_callbacks: after.measurement_callbacks - before.measurement_callbacks,
        retained_roots: after.retained_roots - before.retained_roots,
        text: text_work_delta(before.text, after.text),
    }
}

fn assert_full_compile_work(panel: OwnedPanelWork, compiler: UiCompileWorkStats, calls: usize) {
    let calls = calls as u64;
    assert_eq!(panel.compile_calls, calls);
    assert_eq!(panel.built_snapshots, calls);
    assert_eq!(panel.reused_frames, 0);
    assert_eq!(compiler.compile_attempts, calls);
    assert_eq!(compiler.published_frames, calls);
    assert_eq!(compiler.root_compile_attempts, calls);
    assert_eq!(compiler.layout_passes, calls);
    assert_eq!(compiler.layout_nodes_created, panel.authored_nodes);
    assert!(compiler.measurement_callbacks > 0);
    assert!(compiler.text.layout_attempts > 0);
    assert!(compiler.text.layout_source_bytes > 0);
    assert!(compiler.text.shaped_glyphs > 0);
    assert_eq!(compiler.retained_roots, 0);
}

#[test]
fn inspector_change_pressure_recompiles_and_preserves_source_history_and_drafts() {
    let (project, application) = application();
    let model = application.current_frame().unwrap();
    let sources = fixture_sources(&project);
    for pressure in InspectorPressure::ALL {
        let mut panel = PressurePanel::new(&model);
        let baseline = Arc::clone(&panel.frame);
        let drafts = observed_drafts(&panel.fields);
        let token = panel.owner.input_token(&panel.frame, &panel.label).unwrap();
        let inputs = pressure_inputs(pressure, 3, panel.toggle_states);
        let before = panel.owner.work;
        let compiler = panel.owner.compiler.work_stats();
        for input in inputs {
            panel.change(&model, input);
            assert_pressure_output(&panel, &baseline, pressure, input);
            assert!(observed_drafts(&panel.fields) == drafts);
            assert_eq!(
                panel.owner.input_token(&panel.frame, &panel.label).unwrap(),
                token,
            );
        }
        assert_full_compile_work(
            panel_work_delta(before, panel.owner.work),
            compiler_work_delta(compiler, panel.owner.compiler.work_stats()),
            3,
        );
        panel.assert_local_history();
        assert_pressure_source(&project, &application, &model, &sources);
    }
}

#[test]
fn inspector_changed_palette_at_the_same_theme_revision_refuses_frame_reuse() {
    let (project, application) = application();
    let model = application.current_frame().unwrap();
    let sources = fixture_sources(&project);
    let mut panel = PressurePanel::new(&model);
    let baseline = Arc::clone(&panel.frame);
    let drafts = observed_drafts(&panel.fields);
    let before = panel.owner.work;
    let compiler = panel.owner.compiler.work_stats();
    let input = PressureInput {
        environment: baseline.environment(),
        theme: UiTheme::new(1, pressure_palette(0)).unwrap(),
        toggle_state: None,
    };
    // Current semantics refuse the unchanged-frame path, not the valid compile itself.
    panel.change(&model, input);
    assert_pressure_output(&panel, &baseline, InspectorPressure::ThemePalette, input);
    assert_full_compile_work(
        panel_work_delta(before, panel.owner.work),
        compiler_work_delta(compiler, panel.owner.compiler.work_stats()),
        1,
    );
    assert!(observed_drafts(&panel.fields) == drafts);
    panel.assert_local_history();
    assert_pressure_source(&project, &application, &model, &sources);
}

#[test]
fn inspector_mismatched_theme_revision_refuses_without_replacing_the_frame() {
    let (project, application) = application();
    let model = application.current_frame().unwrap();
    let sources = fixture_sources(&project);
    let mut panel = PressurePanel::new(&model);
    let drafts = observed_drafts(&panel.fields);
    let compiler = panel.owner.compiler.work_stats();
    panel.fields.begin_paint(&model);
    let result = panel.owner.compile(
        &model,
        &mut panel.fields,
        panel.frame.environment(),
        UiTheme::new(2, pressure_palette(0)).unwrap(),
        false,
        None,
    );
    assert!(matches!(
        result,
        Err(OwnedInspectorError::Compile(
            UiCompileError::ThemeRevisionMismatch {
                environment: 1,
                theme: 2,
            }
        ))
    ));
    assert!(Arc::ptr_eq(
        panel.owner.compiled_frame().unwrap(),
        &panel.frame,
    ));
    assert!(observed_drafts(&panel.fields) == drafts);
    let work = compiler_work_delta(compiler, panel.owner.compiler.work_stats());
    assert_eq!(work.compile_attempts, 1);
    assert_eq!(work.published_frames, 0);
    assert_eq!(work.root_compile_attempts, 0);
    assert_eq!(work.layout_nodes_created, 0);
    assert_eq!(work.layout_passes, 0);
    assert_eq!(work.text, TextWorkStats::default());
    let recovered = paint(&mut panel.owner, &mut panel.fields, &model);
    assert!(Arc::ptr_eq(&recovered, &panel.frame));
    panel.assert_local_history();
    assert_pressure_source(&project, &application, &model, &sources);
}

#[test]
#[ignore = "explicit CPU change-pressure baseline; run serially with captured output"]
fn measure_owned_inspector_change_pressure() {
    let (project, application) = application();
    let model = application.current_frame().unwrap();
    let sources = fixture_sources(&project);
    let face = bundled_font_face().unwrap();
    for pressure in InspectorPressure::ALL {
        let mut panel = PressurePanel::new(&model);
        let baseline = Arc::clone(&panel.frame);
        let drafts = observed_drafts(&panel.fields);
        let token = panel.owner.input_token(&panel.frame, &panel.label).unwrap();
        // Environment/palette construction and hover routing are outside all observation windows.
        // Adjacent inputs differ; alternating colors/geometry never become unchanged-frame calls.
        let inputs = pressure_inputs(
            pressure,
            PRESSURE_WARMUPS + PRESSURE_SAMPLES,
            panel.toggle_states,
        );
        assert!(inputs.windows(2).all(|pair| pair[0] != pair[1]));
        for input in &inputs[..PRESSURE_WARMUPS] {
            panel.change(&model, *input);
        }
        let before = panel.owner.work;
        let compiler = panel.owner.compiler.work_stats();
        let editor_text = panel.owner.engine.work_stats();
        let mut samples = inputs[PRESSURE_WARMUPS..].iter();
        let elapsed = measure_calls(
            pressure.workload(),
            || panel.change(&model, *samples.next().unwrap()),
            PRESSURE_SAMPLES,
        );
        assert!(samples.next().is_none());
        let owner_delta = panel_work_delta(before, panel.owner.work);
        let compiler_delta = compiler_work_delta(compiler, panel.owner.compiler.work_stats());
        let editor_text_delta = text_work_delta(editor_text, panel.owner.engine.work_stats());
        assert_full_compile_work(owner_delta, compiler_delta, PRESSURE_SAMPLES);
        assert_pressure_output(&panel, &baseline, pressure, *inputs.last().unwrap());
        assert!(observed_drafts(&panel.fields) == drafts);
        assert_eq!(
            panel.owner.input_token(&panel.frame, &panel.label).unwrap(),
            token,
        );
        println!(
            "UI_WORK {} warmups={} samples={} percentile_ns={elapsed:?} scope={} first_measured={:?} last_measured={:?} owner_delta={owner_delta:?} compiler_delta={compiler_delta:?} editor_text_delta={editor_text_delta:?}",
            pressure.workload(),
            PRESSURE_WARMUPS,
            PRESSURE_SAMPLES,
            pressure.scope(),
            inputs[PRESSURE_WARMUPS],
            inputs.last().unwrap(),
        );
        println!(
            "UI_WORK_SCOPE {} boundary=prepared_local_state+begin_paint+description+compile+frame_replacement+publication_and_fresh_frame_assertions font_family={:?} face_index={} face_id={:?} font_sha256={} font_bytes={} font_count=1 logical_text_size={} limits=defaults height=1600 route_revision=1 native=false editor_direct_Parley_work=excluded compiler_text_is_nested=true allocation_observer={}",
            pressure.workload(),
            face.family_name(),
            face.face_index(),
            face.id(),
            nara_text::BUNDLED_FONT_SHA256,
            face.byte_len(),
            TEXT_SIZE,
            cfg!(feature = "ui-work-allocation-probes"),
        );
        // History checks are outside timing/allocation windows and their counter deltas.
        panel.assert_local_history();
        assert_pressure_source(&project, &application, &model, &sources);
    }
}
