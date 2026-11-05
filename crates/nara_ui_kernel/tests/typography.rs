use nara_text::{TextEditor, TextEngine, TextLimits, TextTypography, bundled_font_face};
use nara_ui_kernel::{
    UiAlign, UiAuthoringSnapshotBuilder, UiCompiler, UiDimension, UiEdges, UiEnvironment, UiLayer,
    UiLocalKey, UiNode, UiPaintPrimitive, UiPreparedText, UiRoot, UiRootKey, UiStyle, UiSurfaceId,
    UiText, UiTextOverflow, UiTheme,
};

#[test]
fn ordinary_and_prepared_text_share_typography_measurement_baselines_and_request_identity() {
    let body = TextTypography::new(14.0)
        .unwrap()
        .with_line_height(20.0)
        .unwrap();
    let taller = body.with_line_height(30.0).unwrap();
    let title = TextTypography::new(16.0)
        .unwrap()
        .with_line_height(24.0)
        .unwrap();
    let content = "A q\u{301}\u{327} abc אבג";
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
    let mut nodes = Vec::new();
    let mut expected = Vec::new();
    for (index, typography) in [body, taller, title].into_iter().enumerate() {
        let editor = TextEditor::with_typography(&mut engine, content, typography).unwrap();
        let snapshot = editor.paint_snapshot();
        let prepared = UiPreparedText::new(snapshot.clone());
        assert_eq!(prepared.typography(), typography);
        let text = UiText::new("old content", 12.0)
            .unwrap()
            .with_typography(typography)
            .with_content(content)
            .with_overflow(UiTextOverflow::Clip);
        assert_eq!(text.typography(), typography);
        assert_eq!(text.logical_size(), typography.logical_size());
        for (kind, plain) in [("plain", true), ("prepared", false)] {
            let key = UiLocalKey::runtime("typography", format!("{index}-{kind}")).unwrap();
            let node = UiNode::new(
                UiStyle::flex()
                    .with_width(UiDimension::px(280.0).unwrap())
                    .with_flex_shrink(0.0)
                    .with_padding(UiEdges::all(3.0).unwrap()),
            )
            .with_key(key.clone());
            nodes.push(if plain {
                node.with_text(text.clone())
            } else {
                node.with_prepared_text(prepared.clone())
            });
            expected.push((key, typography, editor.layout().clone()));
        }
    }
    let authoring = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("typography", "shared").unwrap(),
                UiLayer::Base,
                UiStyle::fill().with_align_items(UiAlign::Start),
            )
            .with_children(nodes),
        )
        .build()
        .unwrap();
    let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(
            &authoring,
            UiEnvironment::new(
                UiSurfaceId::new(701).unwrap(),
                320.0,
                320.0,
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
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    for (key, typography, original) in expected {
        let measured = frame
            .layouts()
            .iter()
            .find(|fact| {
                fact.key()
                    .is_some_and(|key_value| key_value.local_key() == Some(&key))
            })
            .unwrap()
            .rect();
        assert!((measured.size().height() - original.height() - 6.0).abs() < 0.001);
        let (rect, layout) = frame
            .paint()
            .iter()
            .find_map(|paint| match paint {
                UiPaintPrimitive::Text {
                    key: Some(key_value),
                    rect,
                    layout,
                    ..
                } if key_value.local_key() == Some(&key) => Some((rect, layout)),
                _ => None,
            })
            .unwrap();
        assert_eq!(layout.typography(), typography);
        assert_eq!(layout.lines(), original.lines());
        assert_eq!(layout.height(), original.height());
        assert!((rect.size().height() - original.height()).abs() < 0.001);
        let mut remaining = original.glyphs().iter();
        for painted in layout.glyphs() {
            remaining
                .find(|shaped| {
                    painted.x() == shaped.x()
                        && painted.y() == shaped.y()
                        && painted.key().face_id() == shaped.key().face_id()
                        && painted.key().glyph_id() == shaped.key().glyph_id()
                })
                .expect("paint must retain the original positioned glyph order");
            assert_eq!(
                painted.key().physical_size(),
                typography.logical_size() * 1.25
            );
        }
    }
}
