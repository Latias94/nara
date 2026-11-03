use std::sync::Arc;

use nara_text::{TextEdit, TextEngine, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiInputEvent, UiInteractionSession,
    UiLayer, UiNonActivation, UiPaintPrimitive, UiPoint, UiRoot, UiRootKey, UiRouteStamp, UiSeatId,
    UiSemanticRole, UiStyle, UiSurfaceId, UiTheme,
};
use nara_ui_widgets::{UiTextField, UiTextFieldError};

fn engine() -> TextEngine {
    TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap()
}

fn root() -> UiRootKey {
    UiRootKey::runtime("owned-text-field", "inspector").unwrap()
}

fn frame(field: &UiTextField, revision: u64) -> Arc<nara_ui_kernel::UiFrame> {
    let root = root();
    let node = field
        .authored_node(&root, UiStyle::absolute(12.0, 10.0, 100.0, 36.0), 100.0)
        .unwrap();
    let snapshot = UiAuthoringSnapshotBuilder::new(revision)
        .with_root(UiRoot::new(root, UiLayer::Base, UiStyle::fill()).with_child(node))
        .build()
        .unwrap();
    let environment = UiEnvironment::new(
        UiSurfaceId::new(81).unwrap(),
        240.0,
        100.0,
        1.5,
        revision,
        revision,
        1,
        1,
    )
    .unwrap();
    UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap()
}

#[test]
fn text_lifetimes_reject_queued_input_after_blur_disable_and_same_property_recreation() {
    let mut engine = engine();
    let mut field = UiTextField::new(root(), "Name", "original", 16.0, &mut engine).unwrap();
    let first = field.focus().unwrap();
    assert_eq!(field.focus().unwrap(), first);
    field
        .edit(first, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    field
        .edit(
            first,
            &TextEdit::Preedit {
                text: "中文".into(),
                cursor: Some((0, 6)),
            },
            &mut engine,
        )
        .unwrap();
    assert_eq!(field.editor().text(), "original");
    assert_eq!(field.editor().display_text(), "中文");
    field.blur(&mut engine).unwrap();
    assert_eq!(field.editor().text(), "original");
    assert_eq!(field.editor().display_text(), "original");
    assert!(matches!(
        field.edit(first, &TextEdit::Commit("旧".into()), &mut engine),
        Err(UiTextFieldError::StaleInput)
    ));
    let second = field.focus().unwrap();
    assert_ne!(first, second);
    assert!(matches!(
        field.edit(first, &TextEdit::Insert("late".into()), &mut engine),
        Err(UiTextFieldError::StaleInput)
    ));
    field
        .edit(second, &TextEdit::Commit("中文".into()), &mut engine)
        .unwrap();
    assert_eq!(field.editor().text(), "中文");
    field.edit(second, &TextEdit::Undo, &mut engine).unwrap();
    assert_eq!(field.editor().text(), "original");
    field.edit(second, &TextEdit::Redo, &mut engine).unwrap();
    assert_eq!(field.editor().text(), "中文");
    field.set_enabled(false, &mut engine).unwrap();
    assert!(field.input_token().is_none());
    assert!(matches!(field.focus(), Err(UiTextFieldError::Disabled)));
    field.set_enabled(true, &mut engine).unwrap();
    let renewed = field.focus().unwrap();
    assert_ne!(renewed, second);
    let mut replacement =
        UiTextField::new(root(), "Name", "replacement", 16.0, &mut engine).unwrap();
    assert_ne!(field.key(), replacement.key());
    replacement.focus().unwrap();
    assert!(matches!(
        replacement.edit(renewed, &TextEdit::Commit("stale".into()), &mut engine),
        Err(UiTextFieldError::StaleInput)
    ));
    assert_eq!(replacement.editor().text(), "replacement");
}

#[test]
fn actual_presented_focus_facts_target_the_live_field_and_cannot_focus_its_replacement() {
    let mut engine = engine();
    let mut first = UiTextField::new(root(), "Name", "first", 16.0, &mut engine).unwrap();
    let surface = UiSurfaceId::new(81).unwrap();
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal.install_surface(surface, 1).unwrap();
    let receipt = authority.submit(&mut journal, frame(&first, 1), 1).unwrap();
    let mut interaction = UiInteractionSession::default();
    let dispatch = interaction
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(receipt),
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: first.key().clone(),
            },
        )
        .unwrap();
    first
        .reduce_focus(dispatch.consumed().unwrap(), &mut engine)
        .unwrap();
    let token = first.input_token().unwrap();
    first
        .edit(token, &TextEdit::Insert("x".into()), &mut engine)
        .unwrap();
    let mut replacement = UiTextField::new(root(), "Name", "second", 16.0, &mut engine).unwrap();
    let current = authority
        .submit(&mut journal, frame(&replacement, 2), 2)
        .unwrap();
    let dispatch = interaction
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(current),
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: first.key().clone(),
            },
        )
        .unwrap();
    if let Some(batch) = dispatch.consumed() {
        first.reduce_focus(batch, &mut engine).unwrap();
        replacement.reduce_focus(batch, &mut engine).unwrap();
    }
    assert!(first.input_token().is_none());
    let replacement_input = replacement.input_token().unwrap();
    assert!(matches!(
        replacement.edit(token, &TextEdit::Commit("late".into()), &mut engine),
        Err(UiTextFieldError::StaleInput)
    ));
    assert_eq!(replacement.editor().text(), "second");
    assert!(matches!(
        first.edit(token, &TextEdit::Commit("late".into()), &mut engine),
        Err(UiTextFieldError::StaleInput)
    ));
    let retired = interaction
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(current),
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: first.key().clone(),
            },
        )
        .unwrap();
    assert!(matches!(
        retired.rejection(),
        Some(UiNonActivation::TargetRetired { .. })
    ));
    let dispatch = interaction
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(current),
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: replacement.key().clone(),
            },
        )
        .unwrap();
    replacement
        .reduce_focus(dispatch.consumed().unwrap(), &mut engine)
        .unwrap();
    assert_eq!(replacement.input_token(), Some(replacement_input));
}

#[test]
fn horizontal_text_caret_selection_and_candidate_geometry_share_the_clipped_field_coordinates() {
    let mut engine = engine();
    let mut field = UiTextField::new(
        root(),
        "Long value",
        "abcdefghijklmnopqrstuvwxyz",
        16.0,
        &mut engine,
    )
    .unwrap();
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::End { extend: false }, &mut engine)
        .unwrap();
    let painted = frame(&field, 1);
    assert!(
        painted.diagnostics().is_empty(),
        "{:?}",
        painted.diagnostics()
    );
    let text = painted
        .paint()
        .iter()
        .find_map(|primitive| match primitive {
            UiPaintPrimitive::Text {
                rect, clip_rect, ..
            } => Some((*rect, *clip_rect)),
            _ => None,
        })
        .unwrap();
    assert!(text.0.origin().x() < 12.0);
    let clip = text.1.unwrap();
    assert_eq!(clip.origin(), UiPoint::new(18.0, 16.0));
    assert_eq!(clip.size().width(), 87.0);
    let semantic = painted
        .semantics()
        .iter()
        .find(|node| node.key() == field.key())
        .unwrap();
    assert_eq!(semantic.role(), UiSemanticRole::TextInput);
    assert_eq!(semantic.value(), Some("abcdefghijklmnopqrstuvwxyz"));
    let area = field.ime_area(100.0).unwrap();
    assert!(area.origin().x() >= 0.0 && area.max().x() <= 100.0);
    field
        .point(
            token,
            UiPoint::new(90.0, area.origin().y()),
            100.0,
            false,
            &mut engine,
        )
        .unwrap();
    assert!(field.editor().selection().focus > 10);
    field
        .edit(token, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    let selected = frame(&field, 2);
    assert!(selected.diagnostics().is_empty());
    let highlight = selected
        .paint()
        .iter()
        .find_map(|primitive| match primitive {
            UiPaintPrimitive::Rect {
                rect,
                brush: Some(nara_ui_kernel::UiResolvedBrush::Solid(color)),
                ..
            } if color.components()[3] < 0.5 => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert!((highlight.origin().x() - clip.origin().x()).abs() < 0.001);
    assert!((highlight.max().x() - clip.max().x()).abs() < 0.001);
    assert!(
        selected
            .paint()
            .iter()
            .filter(|primitive| matches!(primitive, UiPaintPrimitive::Rect { .. }))
            .count()
            >= 2
    );
    field
        .edit(
            token,
            &TextEdit::Preedit {
                text: "新".into(),
                cursor: Some((3, 3)),
            },
            &mut engine,
        )
        .unwrap();
    let composing = frame(&field, 3);
    let semantic = composing
        .semantics()
        .iter()
        .find(|node| node.key() == field.key())
        .unwrap();
    assert_eq!(semantic.value(), Some("abcdefghijklmnopqrstuvwxyz"));
    assert!(
        composing
            .semantics()
            .iter()
            .all(|node| node.name() != "新" && node.value() != Some("新"))
    );
    assert_eq!(field.editor().display_text(), "新");
    assert!(
        field
            .authored_node(&root(), UiStyle::fill(), f32::NAN)
            .is_err()
    );
    assert!(
        field
            .authored_node(
                &UiRootKey::runtime("wrong", "root").unwrap(),
                UiStyle::fill(),
                100.0
            )
            .is_err()
    );
}

#[test]
fn empty_values_compile_and_clear_undo_preserves_the_semantic_value() {
    let mut engine = engine();
    let mut field = UiTextField::new(root(), "Name", "", 16.0, &mut engine).unwrap();
    let initial = frame(&field, 1);
    assert!(initial.diagnostics().is_empty());
    assert_eq!(
        initial
            .semantics()
            .iter()
            .find(|node| node.key() == field.key())
            .unwrap()
            .value(),
        Some("")
    );
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::Insert("draft".into()), &mut engine)
        .unwrap();
    field
        .edit(token, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    field
        .edit(token, &TextEdit::Backspace, &mut engine)
        .unwrap();
    let cleared = frame(&field, 2);
    assert!(cleared.diagnostics().is_empty());
    assert_eq!(
        cleared
            .semantics()
            .iter()
            .find(|node| node.key() == field.key())
            .unwrap()
            .value(),
        Some("")
    );
    field.edit(token, &TextEdit::Undo, &mut engine).unwrap();
    let restored = frame(&field, 3);
    assert!(restored.diagnostics().is_empty());
    assert_eq!(
        restored
            .semantics()
            .iter()
            .find(|node| node.key() == field.key())
            .unwrap()
            .value(),
        Some("draft")
    );
}
