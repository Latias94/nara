use nara_text::{FontFace, TextEdit, TextEngine, bundled_font_face};
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiCustomLayout, UiEnvironment,
    UiFrame, UiKey, UiLayer, UiLocalKey, UiMesh, UiMeshVertex, UiNode, UiPaintPrimitive, UiPoint,
    UiRect, UiRoot, UiRootKey, UiRootStatus, UiSize, UiStyle, UiSurfaceId, UiTheme,
};
use nara_ui_widgets::{
    UiAppearance, UiButton, UiButtonState, UiButtonVariant, UiControlSize, UiDensity, UiIconLabel,
    UiPropertyRow, UiTextField, UiTextRole,
};

fn local(name: &str) -> UiLocalKey {
    UiLocalKey::runtime("measured-property", name).unwrap()
}

fn root() -> UiRootKey {
    UiRootKey::runtime("measured-property", "consumer").unwrap()
}

fn scoped(name: &str) -> UiKey {
    UiKey::scoped(root(), local(name)).unwrap()
}

fn rect(frame: &UiFrame, key: &UiKey) -> UiRect {
    frame
        .layouts()
        .iter()
        .find(|fact| fact.key() == Some(key))
        .unwrap()
        .rect()
}

fn baseline(frame: &UiFrame, key: &UiKey) -> f32 {
    frame
        .paint()
        .iter()
        .find_map(|primitive| match primitive {
            UiPaintPrimitive::Text {
                key: Some(painted),
                rect,
                layout,
                ..
            } if painted == key => Some(rect.origin().y() + layout.lines()[0].baseline()),
            _ => None,
        })
        .unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
}

fn icon(color: UiColor) -> UiMesh {
    UiMesh::new(
        [(1.0, 2.0), (15.0, 2.0), (8.0, 14.0)]
            .map(|(x, y)| UiMeshVertex::new(UiPoint::new(x, y), color))
            .to_vec(),
        vec![0, 1, 2],
    )
    .unwrap()
}

#[test]
fn measured_icon_property_retains_full_editing_state_through_real_allocations() {
    let fonts = vec![
        bundled_font_face().unwrap(),
        FontFace::from_bytes(
            include_bytes!("../../nara_text/tests/fixtures/NotoSansSC-subset.otf").to_vec(),
            0,
        )
        .unwrap(),
    ];
    let mut engine = TextEngine::with_fonts(fonts.clone(), Default::default()).unwrap();
    let mut compiler = UiCompiler::with_fonts(fonts, Default::default()).unwrap();
    let value = format!("{}x", "中".repeat(1365));
    assert_eq!(value.len(), 4096);
    let mut field = UiTextField::new(root(), "Property", "before", 14.0, &mut engine).unwrap();
    let token = field.focus().unwrap();
    for edit in [
        TextEdit::SelectAll,
        TextEdit::Commit(value.clone()),
        TextEdit::End { extend: false },
        TextEdit::Preedit {
            text: "中文 e\u{301}".into(),
            cursor: Some((0, 6)),
        },
    ] {
        field.edit(token, &edit, &mut engine).unwrap();
    }
    let selection = field.editor().selection();
    let composition = field.editor().composition_range();
    let row = UiCustomLayout::new(UiPropertyRow::new(160.0, 6.0).unwrap());
    let icon_layout = UiCustomLayout::new(
        UiIconLabel::new(UiSize::new(16.0, 16.0).unwrap(), -12.0, 6.0).unwrap(),
    );
    let mesh = icon(UiAppearance::default().palette().accent);
    let button =
        UiButton::new(scoped("action"), "Apply", UiActionId::new("apply").unwrap()).unwrap();
    for (iteration, (width, density, text_scale)) in [
        (1400.0, UiDensity::Normal, 1.0),
        (300.0, UiDensity::Normal, 1.0),
        (110.0, UiDensity::Compact, 2.0),
        (1400.0, UiDensity::Compact, 2.0),
    ]
    .into_iter()
    .enumerate()
    {
        let appearance = UiAppearance::default()
            .with_density(density)
            .with_text_scale(text_scale)
            .unwrap();
        let typography = appearance.typography(UiTextRole::Body).unwrap();
        field.reconfigure(&mut engine, typography).unwrap();
        field.set_appearance(appearance.field(true, true, true).unwrap());
        let source = UiAuthoringSnapshotBuilder::new(iteration as u64 + 1)
            .with_root(
                UiRoot::new(root(), UiLayer::Base, UiStyle::fill())
                    .with_custom_layout(row.clone())
                    .with_children([
                        UiNode::new(UiStyle::flex())
                            .with_key(local("label-group"))
                            .with_custom_layout(icon_layout.clone())
                            .with_children([
                                UiNode::new(UiStyle::flex())
                                    .with_key(local("icon"))
                                    .with_mesh(mesh.clone()),
                                UiNode::new(UiStyle::flex())
                                    .with_key(local("label"))
                                    .with_text(
                                        appearance
                                            .text("属性 e\u{301}", UiTextRole::Body, false)
                                            .unwrap(),
                                    ),
                            ]),
                        field
                            .authored_node(&root(), UiStyle::flex(), 420.0)
                            .unwrap(),
                        appearance
                            .button(
                                &button,
                                UiButtonState::default(),
                                UiButtonVariant::Primary,
                                UiControlSize::Regular,
                                false,
                            )
                            .unwrap()
                            .authored_node(&button, &root(), UiStyle::flex())
                            .unwrap(),
                        UiNode::new(UiStyle::flex())
                            .with_key(local("status"))
                            .with_text(
                                appearance
                                    .text("Retained draft", UiTextRole::Caption, true)
                                    .unwrap(),
                            ),
                    ]),
            )
            .build()
            .unwrap();
        let frame = compiler
            .compile(
                &source,
                UiEnvironment::new(
                    UiSurfaceId::new(90).unwrap(),
                    width,
                    900.0,
                    1.25,
                    1,
                    1,
                    1,
                    1,
                )
                .unwrap(),
                UiTheme::default(),
            )
            .unwrap();
        assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Published);
        assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
        let field_rect = rect(&frame, field.key());
        let label_rect = rect(&frame, &scoped("label-group"));
        let action_rect = rect(&frame, button.key());
        let status_rect = rect(&frame, &scoped("status"));
        let icon_rect = rect(&frame, &scoped("icon"));
        close(
            icon_rect.origin().y(),
            baseline(&frame, &scoped("label")) - 12.0,
        );
        assert_eq!(icon_rect.size(), UiSize::new(16.0, 16.0).unwrap());
        assert_eq!(
            frame
                .semantics()
                .iter()
                .find(|node| node.key() == field.key())
                .unwrap()
                .value(),
            Some(value.as_str())
        );
        assert_eq!(field.input_token(), Some(token));
        assert_eq!(field.editor().selection(), selection);
        assert_eq!(field.editor().composition_range(), composition);
        if width > 1000.0 {
            let prepared_key = frame
                .semantics()
                .iter()
                .find(|node| node.parent().is_some_and(|parent| parent == field.key()))
                .map(|node| node.key())
                .unwrap();
            let field_baseline = baseline(&frame, prepared_key);
            for key in [&scoped("label"), button.key(), &scoped("status")] {
                close(baseline(&frame, key), field_baseline);
            }
            assert!(field_rect.size().width() > 420.0);
        } else {
            close(field_rect.size().width(), width);
            assert!(field_rect.origin().y() >= label_rect.max().y());
            assert!(action_rect.origin().y() >= field_rect.max().y());
            assert!(status_rect.origin().y() >= field_rect.max().y());
            if width == 110.0 {
                assert!(status_rect.origin().y() >= action_rect.max().y());
            }
        }
        let ime = field.ime_area(field_rect.size().width()).unwrap();
        assert!(ime.origin().x() >= 0.0 && ime.max().x() <= field_rect.size().width());
        assert!(
            frame
                .paint()
                .iter()
                .any(|paint| matches!(paint, UiPaintPrimitive::Mesh { .. }))
        );
        let before = compiler.work_stats();
        let repeated = compiler
            .compile(&source, frame.environment(), UiTheme::default())
            .unwrap();
        assert_eq!(repeated.paint(), frame.paint());
        assert_eq!(
            compiler.work_stats().text.layout_attempts,
            before.text.layout_attempts
        );
        assert!(compiler.work_stats().layout_work_units > before.layout_work_units);
    }
    field
        .edit(token, &TextEdit::CancelComposition, &mut engine)
        .unwrap();
    field.edit(token, &TextEdit::Undo, &mut engine).unwrap();
    assert_eq!(field.editor().text(), "before");
    field.edit(token, &TextEdit::Redo, &mut engine).unwrap();
    assert_eq!(field.editor().text(), value);
}
