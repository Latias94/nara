use super::*;
use nara_ui_widgets::UiDensity;

fn property_label(owner: &OwnedInspector, key: &FieldKey, part: &str) -> UiKey {
    UiKey::scoped(
        owner.root.clone(),
        UiLocalKey::runtime(
            "nara.inspector.label",
            format!("{}.{part}", owner.controls[key].identity),
        )
        .unwrap(),
    )
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

fn text_baseline(frame: &UiFrame, key: &UiKey) -> f32 {
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

fn status<'a>(owner: &OwnedInspector, frame: &'a UiFrame, key: &FieldKey) -> &'a str {
    let key = property_label(owner, key, "status");
    frame
        .semantics()
        .iter()
        .find(|node| node.key() == &key)
        .unwrap()
        .name()
}

#[test]
fn scalar_property_measures_real_children_and_reuses_compatible_layouts() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut owner = new_controller();
    let mut fields = FieldEditors::default();
    for density in [UiDensity::Normal, UiDensity::Compact] {
        for text_scale in [1.0, 2.0] {
            owner.set_appearance(
                UiAppearance::default()
                    .with_density(density)
                    .with_text_scale(text_scale)
                    .unwrap(),
            );
            for width in [1200.0, 220.0] {
                let frame = paint_size(&mut owner, &mut fields, &model, width, 1600.0);
                assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
                assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Published);
                let field = input(&owner, &frame, "label");
                let key = field_key(&owner, &frame, &field);
                let path = property_label(&owner, &key, "path");
                let feedback = property_label(&owner, &key, "status");
                let apply = frame
                    .semantics()
                    .iter()
                    .filter(|node| {
                        matches!(owner.binding(&frame, node.key()),
                            Ok(InspectorBinding::Operation {
                                field: target,
                                operation: InspectorOperation::Apply,
                            }) if target == field
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(apply.len(), 1, "the measured row owns the original Apply");
                for operation in InspectorOperation::FOOTER {
                    assert_eq!(
                        owner
                            .retained(&frame)
                            .unwrap()
                            .buttons
                            .values()
                            .filter(|button| {
                                button.field == field
                                    && matches!(button.action,
                                        InspectorButtonAction::Operation(actual) if actual == operation
                                    )
                            })
                            .count(),
                        1,
                    );
                }
                let label = semantic_rect(&frame, &path);
                let editor = semantic_rect(&frame, &field);
                let action = apply[0].rect();
                let feedback_rect = semantic_rect(&frame, &feedback);
                let icon_key = UiKey::scoped(
                    owner.root.clone(),
                    UiLocalKey::runtime(
                        "nara.inspector.property-icon",
                        owner.controls[&key].identity.to_string(),
                    )
                    .unwrap(),
                )
                .unwrap();
                let icon = frame
                    .paint()
                    .iter()
                    .find_map(|paint| match paint {
                        UiPaintPrimitive::Mesh {
                            key: Some(key),
                            rect,
                            ..
                        } if key == &icon_key => Some(rect),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(icon.size(), UiSize::new(12.0, 12.0).unwrap());
                assert!((icon.origin().y() + 10.0 - text_baseline(&frame, &path)).abs() < 0.02);
                assert!(!frame.semantics().iter().any(|node| node.key() == &icon_key));
                assert_eq!(status(&owner, &frame, &key), "Unchanged");
                if width > 1000.0 {
                    assert!(editor.size().width() > 240.0);
                    for gap in [
                        editor.origin().x() - label.max().x(),
                        action.origin().x() - editor.max().x(),
                        feedback_rect.origin().x() - action.max().x(),
                    ] {
                        assert!((gap - owner.appearance.gap()).abs() < 0.02);
                    }
                    let baseline = editor.origin().y()
                        + owner.controls[&key]
                            .text()
                            .unwrap()
                            .layout_measurement(editor.size().width())
                            .unwrap()
                            .first_baseline()
                            .unwrap();
                    for key in [&path, apply[0].key(), &feedback] {
                        assert!((text_baseline(&frame, key) - baseline).abs() < 0.02);
                    }
                } else {
                    assert_eq!(
                        editor.size().width(),
                        width - 2.0 * owner.appearance.panel_inset(),
                    );
                    assert!(editor.size().width() < 240.0);
                    assert!(editor.origin().y() >= label.max().y());
                    assert!(action.origin().y() >= editor.max().y());
                    assert!(feedback_rect.origin().y() >= editor.max().y());
                    assert!(
                        action.max().x() <= feedback_rect.origin().x()
                            || action.max().y() <= feedback_rect.origin().y()
                    );
                }
                let repeated = paint_size(&mut owner, &mut fields, &model, width, 1600.0);
                assert!(Arc::ptr_eq(&frame, &repeated));
            }
            let property_layout = owner.property_layout.1.clone();
            let action_layout = owner.action_layout.1.clone();
            let palette = owner.appearance.palette();
            owner.set_appearance(
                owner
                    .appearance
                    .with_palette(UiAppearancePalette {
                        panel: palette.raised,
                        ..palette
                    })
                    .unwrap(),
            );
            paint_size(&mut owner, &mut fields, &model, 220.0, 1600.0);
            assert_eq!(owner.property_layout.1, property_layout);
            assert_eq!(owner.action_layout.1, action_layout);
        }
    }
}

#[test]
fn scalar_point_and_ime_use_the_accepted_allocation_for_the_complete_buffer() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut owner = new_controller();
    let mut fields = FieldEditors::default();
    let mut frame = paint(&mut owner, &mut fields, &model);
    let field = input(&owner, &frame, "label");
    let key = field_key(&owner, &frame, &field);
    let basis = fields.drafts[&key].basis.clone();
    let value = "W".repeat(4096);
    replace(&mut owner, &mut fields, &frame, &field, &value);
    let token = owner.input_token(&frame, &field).unwrap().unwrap();
    for width in [1200.0, 220.0] {
        owner
            .edit(
                &frame,
                &field,
                token,
                &TextEdit::End { extend: false },
                &mut fields,
            )
            .unwrap();
        frame = paint_size(&mut owner, &mut fields, &model, width, 1600.0);
        let rect = semantic_rect(&frame, &field);
        let text = owner.controls[&key].text().unwrap();
        let ime = owner.ime_area(&frame, &field, token).unwrap();
        assert_eq!(ime, text.ime_area(rect.size().width()).unwrap());
        assert_ne!(ime, text.ime_area(240.0).unwrap());
        assert!(ime.origin().x() >= 0.0 && ime.max().x() <= rect.size().width());
        let text_key = frame
            .semantics()
            .iter()
            .find(|node| node.parent() == Some(&field) && node.role() == UiSemanticRole::Text)
            .unwrap()
            .key();
        let origin = frame
            .paint()
            .iter()
            .find_map(|primitive| match primitive {
                UiPaintPrimitive::Text {
                    key: Some(key),
                    rect,
                    ..
                } if key == text_key => Some(rect.origin()),
                _ => None,
            })
            .unwrap();
        assert_eq!(text.editor().layout().glyphs().len(), value.len());
        let index = value.len() - 8;
        let glyph = &text.editor().layout().glyphs()[index];
        let point = UiPoint::new(
            origin.x() + glyph.x() + 0.1 * glyph.advance() - rect.origin().x(),
            0.5 * rect.size().height(),
        );
        assert!(point.x() > 0.0 && point.x() < rect.size().width());
        owner
            .point(&frame, &field, token, point, false, &mut fields)
            .unwrap();
        assert_eq!(
            owner.controls[&key]
                .text()
                .unwrap()
                .editor()
                .selection_bytes(),
            index..index,
        );
        assert_eq!(fields.drafts[&key].basis, basis);
        assert_eq!(fields.drafts[&key].candidate.copy_text(), value);
    }
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace(),
    );
}

#[test]
fn scalar_status_follows_validation_composition_and_exact_submission() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut owner = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut owner, &mut fields, &model);
    let field = input(&owner, &frame, "value");
    let key = field_key(&owner, &frame, &field);
    let basis = fields.drafts[&key].basis.clone();
    replace(&mut owner, &mut fields, &frame, &field, "-");
    let frame = paint(&mut owner, &mut fields, &model);
    assert_eq!(status(&owner, &frame, &key), "Invalid input");
    replace(&mut owner, &mut fields, &frame, &field, "25");
    let frame = paint(&mut owner, &mut fields, &model);
    assert_eq!(status(&owner, &frame, &key), "Modified");
    let modified_threshold = frame.environment().logical_size().width()
        - semantic_rect(&frame, &field).size().width()
        + 120.0;
    let layout = owner.property_layout.1.clone();
    let token = owner.input_token(&frame, &field).unwrap().unwrap();
    owner
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Preedit {
                text: "6".into(),
                cursor: Some((0, 1)),
            },
            &mut fields,
        )
        .unwrap();
    let frame = paint(&mut owner, &mut fields, &model);
    assert_eq!(status(&owner, &frame, &key), "Composing...");
    let composing_threshold = frame.environment().logical_size().width()
        - semantic_rect(&frame, &field).size().width()
        + 120.0;
    assert!((composing_threshold - modified_threshold).abs() < 0.02);
    let mut frame = frame;
    for width in [modified_threshold - 1.0, modified_threshold + 1.0] {
        owner
            .edit(
                &frame,
                &field,
                token,
                &TextEdit::Preedit {
                    text: "6".into(),
                    cursor: Some((0, 1)),
                },
                &mut fields,
            )
            .unwrap();
        frame = paint_size(&mut owner, &mut fields, &model, width, 1600.0);
        let path = property_label(&owner, &key, "path");
        let rect = semantic_rect(&frame, &field);
        if width < modified_threshold {
            assert!(rect.origin().y() >= semantic_rect(&frame, &path).max().y());
        } else {
            assert!(rect.origin().x() > semantic_rect(&frame, &path).max().x());
        }
        let copy = owner
            .retained(&frame)
            .unwrap()
            .buttons
            .iter()
            .find(|(_, button)| {
                button.field == field && matches!(button.action, InspectorButtonAction::Copy)
            })
            .map(|(key, _)| key.clone())
            .unwrap();
        let copy_rect = semantic_rect(&frame, &copy);
        assert_eq!(owner.property_layout.1, layout);
        assert_eq!(
            owner.ime_area(&frame, &field, token).unwrap(),
            owner.controls[&key]
                .text()
                .unwrap()
                .ime_area(rect.size().width())
                .unwrap()
        );
        assert_eq!(fields.drafts[&key].candidate.copy_text(), "25");
        owner
            .edit(
                &frame,
                &field,
                token,
                &TextEdit::CancelComposition,
                &mut fields,
            )
            .unwrap();
        frame = paint_size(&mut owner, &mut fields, &model, width, 1600.0);
        assert_eq!(semantic_rect(&frame, &field), rect);
        assert_eq!(semantic_rect(&frame, &copy), copy_rect);
        assert_eq!(owner.property_layout.1, layout);
    }
    let prepared = owner
        .operate(
            &frame,
            &field,
            InspectorOperation::Apply,
            &model,
            &mut fields,
        )
        .unwrap()
        .unwrap();
    assert!(matches!(prepared.action,
        EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
            base_revision,
            command: SceneInspectorCommand::SetField { value: ComponentValue::U64(25), .. },
            ..
        }) if base_revision == basis.target.base_revision()
    ));
    let frame = paint(&mut owner, &mut fields, &model);
    assert_eq!(status(&owner, &frame, &key), "Submitting...");
    assert_eq!(fields.drafts[&key].basis, basis);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace(),
    );
}
