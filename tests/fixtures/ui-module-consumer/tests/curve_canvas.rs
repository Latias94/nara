use std::sync::Arc;

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiFrame,
    UiMesh, UiPaintPrimitive, UiPoint, UiRect, UiResolvedBrush, UiRootKey, UiSafeAreaInsets,
    UiSemanticRole, UiSize, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme,
};
use nara_ui_module_consumer::curve_canvas::CurveCanvas;

fn canvas() -> CurveCanvas {
    CurveCanvas::new(UiRootKey::runtime("external-library", "curve-panel").unwrap()).unwrap()
}

fn compiler() -> UiCompiler {
    UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap()
}

fn environment() -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(96).unwrap(),
        400.0,
        160.0,
        1.25,
        1,
        1,
        1,
        1,
    )
    .unwrap()
}

fn snapshot(canvas: &CurveCanvas, pan: UiPoint, selected: bool) -> UiAuthoringSnapshot {
    UiAuthoringSnapshotBuilder::new(1)
        .with_root(canvas.authored_root(pan, selected).unwrap())
        .build()
        .unwrap()
}

fn compile(
    compiler: &mut UiCompiler,
    snapshot: &UiAuthoringSnapshot,
    environment: UiEnvironment,
) -> Arc<UiFrame> {
    let frame = compiler
        .compile(snapshot, environment, UiTheme::default())
        .unwrap();
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    frame
}

fn curve<'a>(frame: &'a UiFrame, canvas: &CurveCanvas) -> &'a UiMesh {
    match frame
        .paint()
        .iter()
        .find(|primitive| primitive.key() == Some(canvas.curve_key()))
        .unwrap()
    {
        UiPaintPrimitive::Mesh { mesh, .. } => mesh,
        other => panic!("the external curve must remain a mesh, got {other:?}"),
    }
}

fn mesh_covers(mesh: &UiMesh, point: UiPoint) -> bool {
    let cross = |first: UiPoint, second: UiPoint, third: UiPoint| {
        (second.x() - first.x()) * (third.y() - first.y())
            - (second.y() - first.y()) * (third.x() - first.x())
    };
    mesh.indices().chunks_exact(3).any(|indices| {
        let first = mesh.vertices()[indices[0] as usize].position();
        let second = mesh.vertices()[indices[1] as usize].position();
        let third = mesh.vertices()[indices[2] as usize].position();
        if cross(first, second, third).abs() < f32::EPSILON {
            return false;
        }
        let sides = [
            cross(first, second, point),
            cross(second, third, point),
            cross(third, first, point),
        ];
        sides.iter().all(|side| *side >= 0.0) || sides.iter().all(|side| *side <= 0.0)
    })
}

#[test]
fn external_curve_mesh_stays_between_background_and_overlapping_ordinary_controls() {
    let canvas = canvas();
    let snapshot = snapshot(&canvas, UiPoint::default(), false);
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot, environment());
    let paint = frame.paint();
    assert_eq!(paint.len(), 6);
    assert!(matches!(paint[0], UiPaintPrimitive::Rect { .. }));
    assert!(matches!(paint[1], UiPaintPrimitive::Mesh { .. }));
    assert!(matches!(paint[2], UiPaintPrimitive::Rect { .. }));
    assert!(matches!(paint[3], UiPaintPrimitive::Text { .. }));
    assert!(matches!(paint[4], UiPaintPrimitive::Rect { .. }));
    assert!(matches!(paint[5], UiPaintPrimitive::Text { .. }));
    assert_eq!(paint[1].key(), Some(canvas.curve_key()));
    assert_eq!(paint[2].key(), Some(canvas.button().key()));
    assert_eq!(paint[3].key(), Some(canvas.button().key()));
    assert_eq!(paint[4].key(), Some(canvas.port_button().key()));
    assert_eq!(paint[5].key(), Some(canvas.port_button().key()));

    let mesh = curve(&frame, &canvas);
    assert!(mesh.vertices().len() > 4 && mesh.indices().len() > 6);
    assert_eq!(mesh.indices().len() % 3, 0);
    assert!(
        mesh.vertices()
            .iter()
            .all(|vertex| { vertex.color() == UiColor::rgba(0.10, 0.75, 1.0, 0.80) })
    );
    for point in [UiPoint::new(158.0, 48.0), UiPoint::new(299.0, 24.0)] {
        assert!(mesh_covers(mesh, point));
        assert!(frame.hit_test(point).is_some());
    }

    let retained = mesh.clone();
    let repeated = compile(&mut compiler, &snapshot, environment());
    let repeated_mesh = curve(&repeated, &canvas);
    assert_eq!(retained.vertices(), repeated_mesh.vertices());
    assert_eq!(retained.indices(), repeated_mesh.indices());
    assert!(std::ptr::eq(retained.vertices(), repeated_mesh.vertices()));
    assert!(std::ptr::eq(retained.indices(), repeated_mesh.indices()));
}

#[test]
fn existing_buttons_supply_matching_hit_focus_and_semantic_actions_for_custom_content() {
    let canvas = canvas();
    let frame = compile(
        &mut compiler(),
        &snapshot(&canvas, UiPoint::default(), true),
        environment(),
    );
    assert_eq!(frame.hit_regions().len(), 3);
    assert_eq!(
        frame
            .focus_order()
            .iter()
            .map(|fact| fact.key())
            .collect::<Vec<_>>(),
        [
            canvas.viewport_key(),
            canvas.button().key(),
            canvas.port_button().key()
        ]
    );
    for (button, point, selected) in [
        (canvas.button(), UiPoint::new(158.0, 48.0), Some(true)),
        (canvas.port_button(), UiPoint::new(299.0, 24.0), None),
    ] {
        assert_eq!(frame.hit_test(point).unwrap().key(), button.key());
        let semantic = frame
            .semantics()
            .iter()
            .find(|semantic| semantic.key() == button.key())
            .unwrap();
        assert_eq!(semantic.role(), UiSemanticRole::Button);
        assert_eq!(semantic.name(), button.label());
        assert_eq!(semantic.actions(), std::slice::from_ref(button.action()));
        assert_eq!(semantic.selected(), selected);
        assert!(semantic.enabled() && semantic.rect().contains(point));
        let focus = frame
            .focus_order()
            .iter()
            .find(|focus| focus.key() == button.key())
            .unwrap();
        assert_eq!(focus.rect(), semantic.rect());
    }
    assert!(
        frame
            .semantics()
            .iter()
            .all(|semantic| semantic.key() != canvas.curve_key())
    );
    let independent =
        CurveCanvas::new(UiRootKey::runtime("external-library", "another-curve-panel").unwrap())
            .unwrap();
    assert_ne!(canvas.button().key(), independent.button().key());
    assert_eq!(canvas.button().action(), independent.button().action());
}

#[test]
fn panned_geometry_and_controls_share_fractional_surface_mapping_and_fixed_viewport_clip() {
    let canvas = canvas();
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(400.0, 160.0).unwrap(),
        UiSize::new(581.0, 307.0).unwrap(),
        UiSafeAreaInsets::new(11.0, 7.0, 9.0, 5.0).unwrap(),
        1.25,
    )
    .unwrap();
    assert_ne!(transform.physical_content_origin(), UiPoint::default());
    assert_ne!(
        transform.content_scale_factor(),
        transform.platform_scale_factor()
    );
    let environment = UiEnvironment::for_surface(
        UiSurfaceId::new(96).unwrap(),
        transform,
        UiSurfaceGenerations::new(1, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap();
    let frame = compile(
        &mut compiler(),
        &snapshot(&canvas, UiPoint::new(60.0, 8.0), false),
        environment,
    );
    let viewport = UiRect::from_origin_size(0.0, 0.0, 320.0, 100.0).unwrap();
    let mesh = curve(&frame, &canvas);
    assert!(mesh.bounds().max().x() > viewport.max().x());
    let primitive = frame
        .paint()
        .iter()
        .find(|primitive| primitive.key() == Some(canvas.curve_key()))
        .unwrap();
    assert_eq!(primitive.clip_rect(), Some(viewport));
    match primitive {
        UiPaintPrimitive::Mesh {
            rect,
            physical_rect,
            ..
        } => {
            assert_eq!(*rect, viewport);
            assert_eq!(*physical_rect, environment.physical_rect(viewport).unwrap());
        }
        _ => unreachable!(),
    }
    let port = frame
        .semantics()
        .iter()
        .find(|semantic| semantic.key() == canvas.port_button().key())
        .unwrap();
    assert_eq!(port.rect().origin(), UiPoint::new(304.0, 20.0));
    assert_eq!(port.clip_rect(), Some(viewport));
    assert_eq!(
        port.rect().intersect(viewport).unwrap().size().width(),
        16.0
    );
    let physical_inside = transform
        .authored_to_physical(UiPoint::new(312.0, 32.0))
        .unwrap();
    let logical_inside = transform.physical_to_authored(physical_inside).unwrap();
    assert_eq!(
        frame.hit_test(logical_inside).unwrap().key(),
        canvas.port_button().key()
    );
    let physical_outside = transform
        .authored_to_physical(UiPoint::new(340.0, 32.0))
        .unwrap();
    let logical_outside = transform.physical_to_authored(physical_outside).unwrap();
    assert!(port.rect().contains(logical_outside));
    assert!(frame.hit_test(logical_outside).is_none());
}

#[test]
fn caller_owned_pan_and_selected_inputs_change_complete_descriptions_without_retargeting_keys() {
    let canvas = canvas();
    let original = snapshot(&canvas, UiPoint::default(), false);
    let panned = snapshot(&canvas, UiPoint::new(8.0, -4.0), true);
    let deselected = snapshot(&canvas, UiPoint::new(8.0, -4.0), false);
    assert_ne!(original, panned);
    assert_ne!(panned, deselected);
    let mut compiler = compiler();
    let first = compile(&mut compiler, &original, environment());
    let second = compile(&mut compiler, &panned, environment());
    let third = compile(&mut compiler, &deselected, environment());
    let before = curve(&first, &canvas);
    let after = curve(&second, &canvas);
    assert_ne!(before, after);
    assert_eq!(before.indices(), after.indices());
    assert_eq!(before.vertices().len(), after.vertices().len());
    for (before, after) in before.vertices().iter().zip(after.vertices()) {
        assert_eq!(
            after.position(),
            UiPoint::new(before.position().x() + 8.0, before.position().y() - 4.0)
        );
        assert_eq!(before.color(), after.color());
    }
    assert_eq!(after, curve(&third, &canvas));
    for (frame, selected, origin) in [
        (&first, false, UiPoint::new(112.0, 30.0)),
        (&second, true, UiPoint::new(120.0, 26.0)),
        (&third, false, UiPoint::new(120.0, 26.0)),
    ] {
        let semantic = frame
            .semantics()
            .iter()
            .find(|semantic| semantic.key() == canvas.button().key())
            .unwrap();
        assert_eq!(semantic.selected(), Some(selected));
        assert_eq!(semantic.rect().origin(), origin);
        let brush = frame.paint().iter().find_map(|primitive| match primitive {
            UiPaintPrimitive::Rect { key, brush, .. }
                if key.as_ref() == Some(canvas.button().key()) =>
            {
                brush.as_ref()
            }
            _ => None,
        });
        assert_eq!(
            brush,
            Some(&UiResolvedBrush::Solid(if selected {
                UiColor::rgba(0.95, 0.45, 0.12, 1.0)
            } else {
                UiColor::rgba(0.25, 0.30, 0.36, 1.0)
            }))
        );
    }
    for pan in [
        UiPoint::new(f32::NAN, 0.0),
        UiPoint::new(0.0, f32::INFINITY),
    ] {
        assert!(canvas.authored_root(pan, false).is_err());
    }
}
