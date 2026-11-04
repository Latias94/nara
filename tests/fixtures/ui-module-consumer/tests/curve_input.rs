use std::sync::Arc;

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCaptureContinuity, UiCompiler, UiEnvironment, UiFrame, UiKey,
    UiMesh, UiPaintPrimitive, UiPoint, UiPointerButton, UiRect, UiRootKey, UiSurfaceId, UiTheme,
};
use nara_ui_module_consumer::curve_canvas::{CurveCanvas, CurveCanvasState};

fn canvas(name: &str) -> CurveCanvas {
    CurveCanvas::new(UiRootKey::runtime("external-curve-input", name).unwrap()).unwrap()
}

fn compile(canvas: &CurveCanvas, state: &CurveCanvasState, selected: bool) -> Arc<UiFrame> {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            canvas
                .authored_view(state.pan(), state.zoom(), selected)
                .unwrap(),
        )
        .build()
        .unwrap();
    let environment = UiEnvironment::new(
        UiSurfaceId::new(97).unwrap(),
        400.0,
        160.0,
        1.25,
        1,
        1,
        1,
        1,
    )
    .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    frame
}

fn mesh<'a>(frame: &'a UiFrame, canvas: &CurveCanvas) -> &'a UiMesh {
    frame
        .paint()
        .iter()
        .find_map(|primitive| match primitive {
            UiPaintPrimitive::Mesh { key, mesh, .. }
                if key.as_ref() == Some(canvas.curve_key()) =>
            {
                Some(mesh)
            }
            _ => None,
        })
        .unwrap()
}

fn semantic_rect(frame: &UiFrame, key: &UiKey) -> UiRect {
    frame
        .semantics()
        .iter()
        .find(|node| node.key() == key)
        .unwrap()
        .rect()
}

#[test]
fn viewport_declares_canvas_input_without_covering_its_ordinary_child_controls() {
    let canvas = canvas("viewport");
    let state = CurveCanvasState::new(&canvas);
    assert_eq!(state.pan(), UiPoint::default());
    assert_eq!(state.zoom(), 1.0);
    assert!(!state.is_dragging());
    let frame = compile(&canvas, &state, false);
    let viewport = frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(canvas.viewport_key()))
        .unwrap();
    assert_eq!(
        viewport.rect(),
        UiRect::from_origin_size(0.0, 0.0, 320.0, 100.0).unwrap()
    );
    let interaction = viewport.interaction();
    assert!(interaction.hit_testable() && interaction.focusable() && interaction.enabled());
    assert!(interaction.pointer_motion() && interaction.scroll());
    assert_eq!(
        interaction.capture_continuity(),
        UiCaptureContinuity::TargetGeometry
    );
    for button in [
        UiPointerButton::Primary,
        UiPointerButton::Secondary,
        UiPointerButton::Middle,
    ] {
        assert!(interaction.pointer_buttons().contains(button));
    }
    assert!(
        !interaction
            .pointer_buttons()
            .contains(UiPointerButton::Other(4))
    );
    assert_eq!(frame.hit_regions().len(), 3);
    assert_eq!(
        frame
            .focus_order()
            .iter()
            .map(|focus| focus.key())
            .collect::<Vec<_>>(),
        [
            canvas.viewport_key(),
            canvas.button().key(),
            canvas.port_button().key()
        ],
    );
    for (point, target) in [
        (UiPoint::new(4.0, 94.0), canvas.viewport_key()),
        (UiPoint::new(158.0, 48.0), canvas.button().key()),
        (UiPoint::new(299.0, 24.0), canvas.port_button().key()),
    ] {
        assert_eq!(frame.hit_test(point).unwrap().key(), target);
    }
    assert!(frame.hit_test(UiPoint::new(340.0, 94.0)).is_none());
}

#[test]
fn explicit_view_updates_reject_invalid_inputs_atomically_and_preserve_the_unit_zoom_entry() {
    let canvas = canvas("view-admission");
    let mut state = CurveCanvasState::new(&canvas);
    assert!(!state.set_view(UiPoint::default(), 1.0).unwrap());
    assert!(state.set_view(UiPoint::new(24.0, -8.0), 2.0).unwrap());
    let accepted = state.clone();
    for (pan, zoom) in [
        (UiPoint::new(f32::NAN, 0.0), 1.0),
        (UiPoint::new(0.0, f32::INFINITY), 1.0),
        (UiPoint::new(f32::NEG_INFINITY, 0.0), 1.0),
        (UiPoint::new(CurveCanvasState::MAX_PAN + 1.0, 0.0), 1.0),
        (UiPoint::new(0.0, -CurveCanvasState::MAX_PAN - 1.0), 1.0),
        (UiPoint::default(), f32::NAN),
        (UiPoint::default(), f32::INFINITY),
        (UiPoint::default(), f32::NEG_INFINITY),
        (UiPoint::default(), 0.0),
        (UiPoint::default(), -1.0),
        (UiPoint::default(), CurveCanvasState::MIN_ZOOM * 0.5),
        (UiPoint::default(), CurveCanvasState::MAX_ZOOM + 1.0),
    ] {
        assert!(
            state.set_view(pan, zoom).is_err(),
            "pan={pan:?}, zoom={zoom}"
        );
        assert_eq!(state, accepted);
        assert!(canvas.authored_view(pan, zoom, false).is_err());
    }
    assert!(!state.set_view(accepted.pan(), accepted.zoom()).unwrap());
    let pan = UiPoint::new(7.0, -3.0);
    assert_eq!(
        canvas.authored_root(pan, true).unwrap(),
        canvas.authored_view(pan, 1.0, true).unwrap()
    );
    assert!(state.set_view(UiPoint::default(), 1.0).unwrap());
    assert_eq!(state, CurveCanvasState::new(&canvas));
}

#[test]
fn zoom_maps_mesh_and_control_geometry_together_while_retaining_fixed_label_fonts() {
    let canvas = canvas("zoomed-geometry");
    let mut state = CurveCanvasState::new(&canvas);
    let original = compile(&canvas, &state, false);
    let pan = UiPoint::new(10.0, 7.0);
    let zoom = 0.5;
    state.set_view(pan, zoom).unwrap();
    let transformed = compile(&canvas, &state, true);
    let original_mesh = mesh(&original, &canvas);
    let transformed_mesh = mesh(&transformed, &canvas);
    assert_eq!(original_mesh.indices(), transformed_mesh.indices());
    assert_eq!(
        original_mesh.vertices().len(),
        transformed_mesh.vertices().len()
    );
    for (before, after) in original_mesh
        .vertices()
        .iter()
        .zip(transformed_mesh.vertices())
    {
        assert_eq!(
            after.position(),
            UiPoint::new(
                before.position().x() * zoom + pan.x(),
                before.position().y() * zoom + pan.y(),
            )
        );
        assert_eq!(before.color(), after.color());
    }
    for (key, font_size) in [
        (canvas.button().key(), 14.0),
        (canvas.port_button().key(), 10.0),
    ] {
        let before = semantic_rect(&original, key);
        let after = semantic_rect(&transformed, key);
        assert_eq!(
            after.origin(),
            UiPoint::new(
                before.origin().x() * zoom + pan.x(),
                before.origin().y() * zoom + pan.y(),
            )
        );
        assert_eq!(after.size().width(), before.size().width() * zoom);
        assert_eq!(after.size().height(), before.size().height() * zoom);
        let center = UiPoint::new(
            after.origin().x() + after.size().width() * 0.5,
            after.origin().y() + after.size().height() * 0.5,
        );
        assert_eq!(transformed.hit_test(center).unwrap().key(), key);
        for frame in [&original, &transformed] {
            let layout = frame
                .paint()
                .iter()
                .find_map(|primitive| match primitive {
                    UiPaintPrimitive::Text {
                        key: text_key,
                        layout,
                        ..
                    } if text_key.as_ref() == Some(key) => Some(layout),
                    _ => None,
                })
                .unwrap();
            assert!(!layout.glyphs().is_empty());
            assert!(
                layout
                    .glyphs()
                    .iter()
                    .all(|glyph| glyph.key().physical_size() == font_size * 1.25)
            );
        }
    }
    let viewport = UiRect::from_origin_size(0.0, 0.0, 320.0, 100.0).unwrap();
    for frame in [&original, &transformed] {
        let primitive = frame
            .paint()
            .iter()
            .find(|primitive| primitive.key() == Some(canvas.curve_key()))
            .unwrap();
        assert_eq!(primitive.rect(), viewport);
        assert_eq!(primitive.clip_rect(), Some(viewport));
    }
    let selected = transformed
        .semantics()
        .iter()
        .find(|semantic| semantic.key() == canvas.button().key())
        .unwrap();
    assert_eq!(selected.selected(), Some(true));
}

#[test]
fn every_admitted_view_extreme_fits_the_complete_mesh_coordinate_contract() {
    let canvas = canvas("view-extremes");
    let mut state = CurveCanvasState::new(&canvas);
    for zoom in [CurveCanvasState::MIN_ZOOM, CurveCanvasState::MAX_ZOOM] {
        for pan_x in [-CurveCanvasState::MAX_PAN, CurveCanvasState::MAX_PAN] {
            for pan_y in [-CurveCanvasState::MAX_PAN, CurveCanvasState::MAX_PAN] {
                state.set_view(UiPoint::new(pan_x, pan_y), zoom).unwrap();
                let frame = compile(&canvas, &state, false);
                let mesh = mesh(&frame, &canvas);
                assert!(mesh.vertices().iter().all(|vertex| {
                    let point = vertex.position();
                    point.x().is_finite()
                        && point.y().is_finite()
                        && point.x().abs() <= 16_384.0
                        && point.y().abs() <= 16_384.0
                }));
                let viewport = frame
                    .layouts()
                    .iter()
                    .find(|layout| layout.key() == Some(canvas.viewport_key()))
                    .unwrap();
                assert_eq!(
                    viewport.rect(),
                    UiRect::from_origin_size(0.0, 0.0, 320.0, 100.0).unwrap()
                );
                assert_eq!(
                    frame.hit_test(UiPoint::new(4.0, 94.0)).unwrap().key(),
                    canvas.viewport_key()
                );
            }
        }
    }
}

#[test]
fn view_state_and_selection_remain_independent_between_exact_viewport_instances() {
    let first = canvas("first");
    let second = canvas("second");
    assert_ne!(first.viewport_key(), second.viewport_key());
    assert_ne!(first.viewport_key(), first.button().key());
    assert_ne!(first.viewport_key(), first.port_button().key());
    let mut first_state = CurveCanvasState::new(&first);
    let second_state = CurveCanvasState::new(&second);
    first_state.set_view(UiPoint::new(9.0, -2.0), 0.75).unwrap();
    assert_eq!(second_state.pan(), UiPoint::default());
    assert_eq!(second_state.zoom(), 1.0);
    let before = first_state.clone();
    let selected = compile(&first, &first_state, true);
    let unselected = compile(&first, &first_state, false);
    assert_eq!(first_state, before);
    assert_eq!(mesh(&selected, &first), mesh(&unselected, &first));
    for (frame, expected) in [(&selected, true), (&unselected, false)] {
        assert_eq!(
            frame
                .semantics()
                .iter()
                .find(|semantic| semantic.key() == first.button().key())
                .unwrap()
                .selected(),
            Some(expected)
        );
    }
}
