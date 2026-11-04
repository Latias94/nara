use std::sync::Arc;

use nara_text::{
    TextEdit, TextEditError, TextEngine, TextLayoutError, TextLimits, bundled_font_face,
};
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiBrush, UiColorRole, UiCompiler, UiEnvironment, UiKey,
    UiLayer, UiLocalKey, UiNode, UiPaintPrimitive, UiRoot, UiRootKey, UiSemanticRole, UiStyle,
    UiSurfaceId, UiText, UiTheme, UiVisual,
};
use nara_ui_widgets::{UiButton, UiTextField, UiTextFieldError};

fn root() -> UiRootKey {
    UiRootKey::runtime("prepared-fields", "creation").unwrap()
}

fn form(
    compiler: &mut UiCompiler,
    first: &UiTextField,
    second: &UiTextField,
    revision: u64,
) -> Arc<nara_ui_kernel::UiFrame> {
    let root = root();
    let button = UiButton::new(
        UiKey::scoped(
            root.clone(),
            UiLocalKey::runtime("prepared-fields", "create").unwrap(),
        )
        .unwrap(),
        "Create UI element",
        UiActionId::new("create-ui-element").unwrap(),
    )
    .unwrap();
    let nodes = [
        UiNode::new(UiStyle::absolute(12.0, 0.0, 280.0, 24.0))
            .with_key(UiLocalKey::runtime("prepared-fields", "title").unwrap())
            .with_text(UiText::new("Name and initial text", 16.0).unwrap()),
        first
            .authored_node(&root, UiStyle::absolute(12.0, 30.0, 240.0, 36.0), 240.0)
            .unwrap(),
        second
            .authored_node(&root, UiStyle::absolute(12.0, 76.0, 240.0, 36.0), 240.0)
            .unwrap(),
        button
            .authored_node(
                &root,
                UiStyle::absolute(12.0, 122.0, 220.0, 36.0),
                UiVisual::role(UiColorRole::Panel),
                UiBrush::Role(UiColorRole::PrimaryText),
                16.0,
            )
            .unwrap(),
    ];
    let snapshot = UiAuthoringSnapshotBuilder::new(revision)
        .with_root(UiRoot::new(root, UiLayer::Base, UiStyle::fill()).with_children(nodes))
        .build()
        .unwrap();
    let environment = UiEnvironment::new(
        UiSurfaceId::new(92).unwrap(),
        640.0,
        200.0,
        1.25,
        1,
        1,
        1,
        1,
    )
    .unwrap();
    let frame = compiler
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    frame
}

fn assert_value(frame: &nara_ui_kernel::UiFrame, field: &UiTextField, expected: &str) {
    let semantic = frame
        .semantics()
        .iter()
        .find(|node| node.key() == field.key())
        .unwrap();
    assert_eq!(semantic.role(), UiSemanticRole::TextInput);
    assert_eq!(semantic.value(), Some(expected));
}

#[test]
fn two_full_4096_byte_fields_and_real_chrome_compile_to_bounded_windows() {
    let mut engine = TextEngine::new(
        bundled_font_face().unwrap(),
        TextLimits {
            max_text_bytes: 4096,
            ..TextLimits::default()
        },
    )
    .unwrap();
    let original = "0123456789abcdef".repeat(256);
    let other = "FEDCBA9876543210".repeat(256);
    assert_eq!(original.len(), 4096);
    assert_eq!(other.len(), 4096);
    let mut first = UiTextField::new(root(), "Name", &original, 16.0, &mut engine).unwrap();
    let second = UiTextField::new(root(), "Initial text", &other, 16.0, &mut engine).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let initial = form(&mut compiler, &first, &second, 1);
    assert_value(&initial, &first, &original);
    assert_value(&initial, &second, &other);
    let output = initial
        .paint()
        .iter()
        .filter_map(|paint| match paint {
            UiPaintPrimitive::Text { layout, .. } => Some(layout.glyphs().len()),
            _ => None,
        })
        .sum::<usize>();
    assert!(output > 0 && output < 4096);
    assert!(
        output < first.editor().layout().glyphs().len() + second.editor().layout().glyphs().len()
    );
    assert!(initial.semantics().iter().any(|node| node.role() == UiSemanticRole::Button
        && node.name() == "Create UI element"));

    let token = first.focus().unwrap();
    first
        .edit(token, &TextEdit::End { extend: false }, &mut engine)
        .unwrap();
    let end = form(&mut compiler, &first, &second, 2);
    assert_eq!(first.editor().selection().focus, 4096);
    assert_value(&end, &first, &original);
    let origin = end
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Text {
                key: Some(key),
                rect,
                ..
            } if key
                .local_key()
                .is_some_and(|local| local != first.key().local_key().unwrap())
                && rect.origin().x() < 0.0 =>
            {
                Some(rect.origin().x())
            }
            _ => None,
        })
        .unwrap();
    assert!(origin < -1000.0);
    let ime = first.ime_area(240.0).unwrap();
    assert!(ime.origin().x() >= 6.0 && ime.max().x() <= 240.0);
    first
        .edit(token, &TextEdit::Home { extend: false }, &mut engine)
        .unwrap();
    assert_eq!(first.editor().selection().focus, 0);
    let home = form(&mut compiler, &first, &second, 3);
    assert_value(&home, &first, &original);
    first.blur(&mut engine).unwrap();
    let blurred = form(&mut compiler, &first, &second, 4);
    assert_value(&blurred, &first, &original);
    let renewed = first.focus().unwrap();
    assert_ne!(token, renewed);
    assert!(matches!(
        first.edit(token, &TextEdit::Insert("stale".into()), &mut engine),
        Err(UiTextFieldError::StaleInput)
    ));
    first
        .edit(renewed, &TextEdit::End { extend: false }, &mut engine)
        .unwrap();
    form(&mut compiler, &first, &second, 5);
    assert_eq!(first.editor().selection().focus, 4096);
}

#[test]
fn full_field_copy_commit_undo_and_oversized_refusal_keep_the_complete_owner_state() {
    let mut engine = TextEngine::new(
        bundled_font_face().unwrap(),
        TextLimits {
            max_text_bytes: 4096,
            ..TextLimits::default()
        },
    )
    .unwrap();
    let original = "0123456789abcdef".repeat(256);
    let mut field = UiTextField::new(root(), "Name", &original, 16.0, &mut engine).unwrap();
    let second = UiTextField::new(root(), "Initial text", "", 16.0, &mut engine).unwrap();
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    assert_eq!(field.editor().selection_bytes(), 0..4096);
    let copied = field.editor().text()[field.editor().selection_bytes()].to_owned();
    assert_eq!(copied, original);
    field
        .edit(token, &TextEdit::Commit("replacement".into()), &mut engine)
        .unwrap();
    assert_eq!(field.editor().text(), "replacement");
    field.edit(token, &TextEdit::Undo, &mut engine).unwrap();
    assert_eq!(field.editor().text(), original);
    assert_eq!(field.editor().selection_bytes(), 0..4096);
    let before = field.editor().paint_snapshot();
    let selection = field.editor().selection();
    let ime = field.editor().ime_area();
    assert!(field.editor().can_redo());
    assert!(matches!(
        field.edit(token, &TextEdit::Commit("x".repeat(4097)), &mut engine),
        Err(UiTextFieldError::Text(TextEditError::Layout(
            TextLayoutError::TextBytesExceeded {
                actual: 4097,
                maximum: 4096,
            }
        )))
    ));
    assert_eq!(field.editor().text(), original);
    assert_eq!(field.editor().selection(), selection);
    assert_eq!(field.editor().ime_area(), ime);
    assert!(before.same_source(&field.editor().paint_snapshot()));
    assert!(field.editor().can_redo());
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let refused = form(&mut compiler, &field, &second, 1);
    assert_value(&refused, &field, &original);
    field
        .edit(
            token,
            &TextEdit::Preedit {
                text: "draft".into(),
                cursor: Some((5, 5)),
            },
            &mut engine,
        )
        .unwrap();
    let preedit = form(&mut compiler, &field, &second, 2);
    assert_value(&preedit, &field, &original);
    assert!(
        preedit
            .semantics()
            .iter()
            .all(|node| node.name() != "draft")
    );
    field
        .edit(token, &TextEdit::Commit(String::new()), &mut engine)
        .unwrap();
    let cleared = form(&mut compiler, &field, &second, 3);
    assert_value(&cleared, &field, "");
    field.edit(token, &TextEdit::Undo, &mut engine).unwrap();
    let restored = form(&mut compiler, &field, &second, 4);
    assert_value(&restored, &field, &original);
}
