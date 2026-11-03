use nara_text::{TextEdit, TextEngine, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiLayer, UiRoot, UiRootKey,
    UiSemanticRole, UiStyle, UiSurfaceId, UiTheme,
};
use nara_ui_widgets::{UiTextField, UiTextFieldError};

#[test]
fn independent_host_edits_and_compiles_a_field_without_document_or_platform_authority() {
    let face = bundled_font_face().unwrap();
    let mut engine = TextEngine::new(face.clone(), Default::default()).unwrap();
    let root = UiRootKey::runtime("external-text-host", "inspector").unwrap();
    let mut field = UiTextField::new(root.clone(), "Value", "12", 18.0, &mut engine).unwrap();
    let input = field.focus().unwrap();
    field
        .edit(input, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    field
        .edit(
            input,
            &TextEdit::Preedit {
                text: "候选".into(),
                cursor: Some((0, 6)),
            },
            &mut engine,
        )
        .unwrap();
    assert_eq!(field.editor().text(), "12");
    field.blur(&mut engine).unwrap();
    assert_eq!(field.editor().display_text(), "12");
    let current = field.focus().unwrap();
    assert!(matches!(
        field.edit(input, &TextEdit::Commit("old".into()), &mut engine),
        Err(UiTextFieldError::StaleInput)
    ));
    field
        .edit(current, &TextEdit::Commit("34".into()), &mut engine)
        .unwrap();
    assert_eq!(field.editor().text(), "34");
    field.edit(current, &TextEdit::Undo, &mut engine).unwrap();
    assert_eq!(field.editor().text(), "12");
    field.edit(current, &TextEdit::Redo, &mut engine).unwrap();
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill()).with_child(
                field
                    .authored_node(&root, UiStyle::absolute(8.0, 8.0, 160.0, 40.0), 160.0)
                    .unwrap(),
            ),
        )
        .build()
        .unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(91).unwrap(), 200.0, 100.0, 1.0, 1, 1, 1, 1).unwrap();
    let frame = UiCompiler::new(face, Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    assert!(frame.diagnostics().is_empty());
    let semantics = frame
        .semantics()
        .iter()
        .find(|node| node.key() == field.key())
        .unwrap();
    assert_eq!(semantics.role(), UiSemanticRole::TextInput);
    assert_eq!(semantics.value(), Some("34"));
    assert!(
        frame
            .focus_order()
            .iter()
            .any(|fact| fact.key() == field.key())
    );
}

#[test]
fn independent_field_and_compiler_share_the_exact_ordered_fallback_faces() {
    let primary = bundled_font_face().unwrap();
    let fallback = nara_text::FontFace::from_bytes(
        include_bytes!("../../../../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let expected = [primary.id(), fallback.id()];
    let faces = vec![primary, fallback];
    let mut engine = TextEngine::with_fonts(faces.clone(), Default::default()).unwrap();
    let root = UiRootKey::runtime("external-text-host", "fallback").unwrap();
    let mut field = UiTextField::new(root.clone(), "Value", "A", 18.0, &mut engine).unwrap();
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::End { extend: false }, &mut engine)
        .unwrap();
    field
        .edit(token, &TextEdit::Insert("\u{2229}".into()), &mut engine)
        .unwrap();
    assert_eq!(field.editor().text(), "A\u{2229}");
    assert_eq!(
        field
            .editor()
            .layout()
            .glyphs()
            .iter()
            .map(|glyph| glyph.key().face_id())
            .collect::<Vec<_>>(),
        expected
    );
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill()).with_child(
                field
                    .authored_node(&root, UiStyle::absolute(8.0, 8.0, 160.0, 40.0), 160.0)
                    .unwrap(),
            ),
        )
        .build()
        .unwrap();
    let frame = UiCompiler::with_fonts(faces, Default::default())
        .unwrap()
        .compile(
            &snapshot,
            UiEnvironment::new(
                UiSurfaceId::new(92).unwrap(),
                200.0,
                100.0,
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
    assert!(frame.diagnostics().is_empty());
    let painted = frame
        .paint()
        .iter()
        .find_map(|primitive| match primitive {
            nara_ui_kernel::UiPaintPrimitive::Text { layout, .. } => Some(layout),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        painted
            .glyphs()
            .iter()
            .map(|glyph| glyph.key().face_id())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(painted.width(), field.editor().layout().width());
}
