use std::sync::Arc;

use nara_text::{TextEdit, TextEngine, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiFrame, UiPoint, UiRoot, UiRootKey,
    UiSemanticRole, UiSurfaceId, UiTheme,
};
use nara_ui_module_consumer::keyed_editors::{EditorRow, KeyedEditors};
use nara_ui_widgets::{UiStateHandle, UiTextField, UiTextFieldError};

fn engine() -> TextEngine {
    TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap()
}

fn panel(instance: &str) -> KeyedEditors {
    KeyedEditors::new(UiRootKey::runtime("independent-composite", instance).unwrap()).unwrap()
}

fn row<'a>(id: &'a str, initial: &'a str) -> EditorRow<'a> {
    EditorRow {
        id,
        document: "document-one",
        schema: 1,
        initial,
        replace: false,
    }
}

fn frame(root: UiRoot) -> Arc<UiFrame> {
    UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(
            &UiAuthoringSnapshotBuilder::new(1)
                .with_root(root)
                .build()
                .unwrap(),
            UiEnvironment::new(
                UiSurfaceId::new(94).unwrap(),
                280.0,
                220.0,
                1.25,
                1,
                1,
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
        )
        .unwrap()
}

fn value(panel: &KeyedEditors, field: &UiStateHandle<UiTextField>) -> String {
    panel.state.get(field).unwrap().editor().text().to_owned()
}

#[test]
fn independent_same_label_composite_preserves_drafts_selection_preedit_and_history_by_key() {
    let mut engine = engine();
    let mut panel = panel("first");
    let (_, first) = panel
        .compose(&[row("alpha", "Alpha"), row("beta", "Beta")], &mut engine)
        .unwrap();
    assert_ne!(
        panel.state.get(&first[0]).unwrap().key(),
        panel.state.get(&first[1]).unwrap().key()
    );
    panel.state.get_mut(&first[0]).unwrap().focus().unwrap();
    let token = panel.state.text_input(&first[0]).unwrap().unwrap();
    panel
        .state
        .edit_text(&token, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    panel
        .state
        .edit_text(&token, &TextEdit::Commit("Draft α".into()), &mut engine)
        .unwrap();
    panel
        .state
        .edit_text(&token, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    panel
        .state
        .edit_text(
            &token,
            &TextEdit::Preedit {
                text: "e\u{301}".into(),
                cursor: Some((0, 3)),
            },
            &mut engine,
        )
        .unwrap();
    let selection = panel.state.get(&first[0]).unwrap().editor().selection();
    for _ in 0..3 {
        let (root, reordered) = panel
            .compose(
                &[
                    row("beta", "ignored initial"),
                    row("inserted", "Inserted"),
                    row("alpha", "ignored"),
                ],
                &mut engine,
            )
            .unwrap();
        assert_eq!(reordered[0], first[1]);
        assert_eq!(reordered[2], first[0]);
        let field = panel.state.get(&reordered[2]).unwrap();
        assert_eq!(field.editor().text(), "Draft α");
        assert_eq!(field.editor().display_text(), "e\u{301}");
        assert_eq!(field.editor().selection(), selection);
        assert_eq!(
            panel.state.text_input(&reordered[2]).unwrap().as_ref(),
            Some(&token)
        );
        let frame = frame(root);
        assert!(frame.diagnostics().is_empty());
        let inputs: Vec<_> = frame
            .semantics()
            .iter()
            .filter(|node| node.role() == UiSemanticRole::TextInput)
            .collect();
        assert_eq!(inputs.len(), 3);
        assert!(inputs.iter().all(|node| node.name() == "Value"));
        assert!(
            inputs
                .iter()
                .any(|node| node.key() == field.key() && node.value() == Some("Draft α"))
        );
    }
    panel
        .state
        .edit_text(&token, &TextEdit::CancelComposition, &mut engine)
        .unwrap();
    panel
        .state
        .point_text(&token, UiPoint::new(6.0, 6.0), 220.0, false, &mut engine)
        .unwrap();
    assert_eq!(
        panel
            .state
            .get(&first[0])
            .unwrap()
            .editor()
            .selection()
            .focus,
        0
    );
    panel
        .state
        .edit_text(&token, &TextEdit::Undo, &mut engine)
        .unwrap();
    assert_eq!(value(&panel, &first[0]), "Alpha");
    panel
        .state
        .edit_text(&token, &TextEdit::Redo, &mut engine)
        .unwrap();
    assert_eq!(value(&panel, &first[0]), "Draft α");
    assert_eq!(value(&panel, &first[1]), "Beta");
}

#[test]
fn independent_filter_and_same_position_replacement_retire_inputs_without_publishing_preedit() {
    let mut engine = engine();
    let mut panel = panel("filter");
    let (_, original) = panel
        .compose(&[row("alpha", "Alpha"), row("beta", "Beta")], &mut engine)
        .unwrap();
    let raw = panel.state.get_mut(&original[0]).unwrap().focus().unwrap();
    let token = panel.state.text_input(&original[0]).unwrap().unwrap();
    panel
        .state
        .edit_text(&token, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    panel
        .state
        .edit_text(
            &token,
            &TextEdit::Preedit {
                text: "uncommitted".into(),
                cursor: Some((0, 11)),
            },
            &mut engine,
        )
        .unwrap();
    let (_, filtered) = panel
        .compose(&[row("beta", "ignored")], &mut engine)
        .unwrap();
    assert_eq!(filtered[0], original[1]);
    assert!(panel.state.get(&original[0]).is_err());
    assert!(
        panel
            .state
            .edit_text(&token, &TextEdit::Commit("late".into()), &mut engine)
            .is_err()
    );
    let (_, replacement) = panel
        .compose(&[row("gamma", "Gamma"), row("alpha", "Alpha")], &mut engine)
        .unwrap();
    for field in &replacement {
        assert_ne!(field, &original[0]);
        panel.state.get_mut(field).unwrap().focus().unwrap();
        assert!(matches!(
            panel.state.get_mut(field).unwrap().edit(
                raw,
                &TextEdit::Commit("late".into()),
                &mut engine
            ),
            Err(UiTextFieldError::StaleInput)
        ));
    }
    assert_eq!(value(&panel, &replacement[0]), "Gamma");
    assert_eq!(value(&panel, &replacement[1]), "Alpha");
}

#[test]
fn independent_binding_and_schema_changes_require_explicit_replacement_with_atomic_membership() {
    let mut engine = engine();
    let mut panel = panel("binding");
    let (_, fields) = panel
        .compose(&[row("alpha", "Alpha"), row("beta", "Beta")], &mut engine)
        .unwrap();
    panel.state.get_mut(&fields[0]).unwrap().focus().unwrap();
    let old_input = panel.state.text_input(&fields[0]).unwrap().unwrap();
    for (document, schema) in [("document-two", 1), ("document-one", 2)] {
        let changed = EditorRow {
            document,
            schema,
            ..row("alpha", "Changed")
        };
        assert!(
            panel
                .compose(&[row("inserted", "must roll back"), changed], &mut engine)
                .is_err()
        );
        assert_eq!(panel.state.len(), 2);
        assert_eq!(value(&panel, &fields[0]), "Alpha");
        assert_eq!(value(&panel, &fields[1]), "Beta");
    }
    let changed = EditorRow {
        document: "document-two",
        schema: 2,
        replace: true,
        ..row("alpha", "Changed")
    };
    let (_, replacement) = panel
        .compose(&[changed, row("beta", "ignored")], &mut engine)
        .unwrap();
    assert_ne!(replacement[0], fields[0]);
    assert_eq!(replacement[1], fields[1]);
    assert!(
        panel
            .state
            .edit_text(&old_input, &TextEdit::Insert("late".into()), &mut engine)
            .is_err()
    );
    assert_eq!(value(&panel, &replacement[0]), "Changed");
}
