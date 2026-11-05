use nara_ui_kernel::{UiColor, UiPaintPrimitive, UiResolvedBrush};
use nara_ui_widgets::{UiAppearancePalette, UiControlSize, UiDensity};

use super::*;

fn appearance(scale: f32, density: UiDensity) -> UiAppearance {
    UiAppearance::new(
        UiAppearancePalette {
            panel: UiColor::rgba(0.07, 0.09, 0.11, 1.0),
            input: UiColor::rgba(0.02, 0.04, 0.06, 1.0),
            muted_text: UiColor::rgba(0.63, 0.68, 0.73, 1.0),
            selection: UiColor::rgba(0.15, 0.25, 0.35, 1.0),
            hover: UiColor::rgba(0.17, 0.27, 0.37, 1.0),
            pressed: UiColor::rgba(0.19, 0.29, 0.39, 1.0),
            ..Default::default()
        },
        density,
        scale,
    )
    .unwrap()
}

fn rect(frame: &UiFrame, key: &UiKey) -> UiRect {
    frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(key))
        .unwrap()
        .rect()
}

fn assert_role(frame: &UiFrame, name: &str, appearance: UiAppearance, role: UiTextRole) {
    let key = frame
        .semantics()
        .iter()
        .find(|node| node.name() == name)
        .unwrap()
        .key();
    let (layout, color) = frame
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Text {
                key: Some(current),
                layout,
                color,
                ..
            } if current == key => Some((layout, *color)),
            _ => None,
        })
        .unwrap();
    assert_eq!(layout.typography(), appearance.typography(role).unwrap());
    assert_eq!(
        color,
        if role == UiTextRole::Caption {
            appearance.palette().muted_text
        } else {
            appearance.palette().text
        }
    );
}

fn assert_reachable(frame: &UiFrame, key: &UiKey) {
    let hit = frame
        .hit_regions()
        .iter()
        .find(|hit| hit.key() == key)
        .unwrap();
    let layout = frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(key))
        .unwrap();
    let visible = layout
        .clip_rect()
        .map_or(hit.rect(), |clip| hit.rect().intersect(clip).unwrap());
    let point = UiPoint::new(
        visible.origin().x() + visible.size().width() * 0.5,
        visible.origin().y() + visible.size().height() * 0.5,
    );
    assert_eq!(frame.hit_test(point).map(|hit| hit.key()), Some(key));
}

#[test]
fn appearance_adopts_real_typography_without_replacing_filter_composition_or_history() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let (_project, application) = application(kind, 3);
        let model = application.current_frame().unwrap();
        for scale in [1.0, 2.0] {
            for density in [UiDensity::Normal, UiDensity::Compact] {
                let mut owner = controller();
                owner.set_appearance(UiAppearance::default());
                let frame = paint(&mut owner, &model);
                let field = input(&frame);
                let mut routing = RoutedInput::new(&frame);
                routing.focus(&mut owner, &frame, &field);
                let token = owner.input_token(&frame, &field).unwrap().unwrap();
                owner
                    .edit(
                        &frame,
                        &field,
                        token,
                        &TextEdit::Insert("entity-0001".into()),
                    )
                    .unwrap();
                owner
                    .edit(&frame, &field, token, &TextEdit::Insert("x".into()))
                    .unwrap();
                owner.edit(&frame, &field, token, &TextEdit::Undo).unwrap();
                owner
                    .edit(
                        &frame,
                        &field,
                        token,
                        &TextEdit::Preedit {
                            text: "preview".into(),
                            cursor: Some((0, 7)),
                        },
                    )
                    .unwrap();
                let previous = paint(&mut owner, &model);
                let display = owner.filter.editor().display_text().to_owned();
                let selection = owner.filter.editor().selection();
                let composition = owner.filter.editor().composition_range();
                let old_paint = owner.filter.editor().paint_snapshot();
                let projection = owner.projection_work;
                let basis = owner.basis.clone();
                let effective = appearance(scale, density);
                owner.set_appearance(effective);
                assert!(owner.scroll(&previous, 1.0).is_err());
                assert!(owner.ime_area(&previous, &field, token).is_err());
                assert!(
                    owner
                        .point(&previous, &field, token, UiPoint::new(12.0, 12.0), false)
                        .is_err()
                );
                let adopted = paint(&mut owner, &model);
                assert!(!Arc::ptr_eq(&previous, &adopted));
                assert_eq!(owner.appearance, effective);
                assert_eq!(owner.retained(&adopted).unwrap().appearance, effective);
                assert_eq!(owner.basis, basis);
                assert_eq!(owner.projection_work.refreshes, projection.refreshes);
                assert_eq!(
                    owner.projection_work.filter_refreshes,
                    projection.filter_refreshes
                );
                assert_eq!(input(&adopted), field);
                assert_eq!(owner.input_token(&adopted, &field).unwrap(), Some(token));
                assert_eq!(owner.filter.editor().text(), "entity-0001");
                assert_eq!(owner.filter.editor().display_text(), display);
                assert_eq!(owner.filter.editor().selection(), selection);
                assert_eq!(owner.filter.editor().composition_range(), composition);
                assert!(owner.filter.editor().can_undo());
                assert!(owner.filter.editor().can_redo());
                assert_eq!(
                    old_paint.same_source(&owner.filter.editor().paint_snapshot()),
                    scale == 1.0
                );
                assert_eq!(
                    owner.filter.appearance(),
                    Some(&effective.field(true, true, false).unwrap())
                );
                let padding = owner.filter.appearance().unwrap().padding;
                let field_rect = rect(&adopted, &field);
                let ime = owner.ime_area(&adopted, &field, token).unwrap();
                assert!(ime.max().y() + padding.bottom() <= field_rect.size().height() + 0.01);
                assert_role(&adopted, "+ Root", effective, UiTextRole::Body);
                assert_role(&adopted, "Hierarchy", effective, UiTextRole::Title);
                assert_role(
                    &adopted,
                    "Source declarations; resolved hierarchy is unavailable in this view",
                    effective,
                    UiTextRole::Caption,
                );
                assert!(
                    !owner
                        .invalidated_controls(&previous, &adopted)
                        .unwrap()
                        .contains(&field)
                );
                owner.set_appearance(effective);
                assert!(Arc::ptr_eq(&adopted, &paint(&mut owner, &model)));
                owner
                    .edit(&adopted, &field, token, &TextEdit::CancelComposition)
                    .unwrap();
                owner
                    .edit(&adopted, &field, token, &TextEdit::Redo)
                    .unwrap();
                assert_eq!(owner.filter.editor().text(), "entity-0001x");
                owner
                    .edit(&adopted, &field, token, &TextEdit::Undo)
                    .unwrap();
                assert_eq!(owner.filter.editor().text(), "entity-0001");
                assert_eq!(
                    application.current_frame().unwrap().workspace(),
                    model.workspace()
                );
            }
        }
    }
}

#[test]
fn small_viewport_text_scaling_preserves_active_filter_until_explicit_blur() {
    let (_project, application) = application(EditorDocumentKind::Scene, 3);
    let model = application.current_frame().unwrap();
    let viewport = environment(180.0);
    for density in [UiDensity::Normal, UiDensity::Compact] {
        let mut owner = controller();
        owner.set_appearance(appearance(1.0, density));
        let frame = owner
            .compile(&model, viewport, UiTheme::default(), false)
            .unwrap();
        let field = input(&frame);
        let mut routing = RoutedInput::new(&frame);
        routing.focus(&mut owner, &frame, &field);
        let token = owner.input_token(&frame, &field).unwrap().unwrap();
        owner
            .edit(&frame, &field, token, &TextEdit::Insert("entity".into()))
            .unwrap();
        owner
            .edit(&frame, &field, token, &TextEdit::Insert("-0001".into()))
            .unwrap();
        owner.edit(&frame, &field, token, &TextEdit::Undo).unwrap();
        owner
            .edit(
                &frame,
                &field,
                token,
                &TextEdit::Preedit {
                    text: "preview".into(),
                    cursor: Some((0, 7)),
                },
            )
            .unwrap();
        let previous = owner
            .compile(&model, viewport, UiTheme::default(), false)
            .unwrap();
        let display = owner.filter.editor().display_text().to_owned();
        let selection = owner.filter.editor().selection();
        let composition = owner.filter.editor().composition_range();
        owner.set_appearance(appearance(2.0, density));
        let adopted = owner
            .compile(&model, viewport, UiTheme::default(), false)
            .unwrap();
        assert_eq!(input(&adopted), field);
        assert_reachable(&adopted, &field);
        assert!(rect(&adopted, &field).max().y() <= viewport.logical_size().height());
        routing.focus(&mut owner, &adopted, &field);
        assert_eq!(owner.input_token(&adopted, &field).unwrap(), Some(token));
        assert_eq!(owner.filter.editor().text(), "entity");
        assert_eq!(owner.filter.editor().display_text(), display);
        assert_eq!(owner.filter.editor().selection(), selection);
        assert_eq!(owner.filter.editor().composition_range(), composition);
        assert!(owner.filter.editor().can_undo());
        assert!(owner.filter.editor().can_redo());
        assert!(owner.ime_area(&adopted, &field, token).is_ok());
        assert!(owner.ime_area(&previous, &field, token).is_err());
        assert!(
            !owner
                .invalidated_controls(&previous, &adopted)
                .unwrap()
                .contains(&field)
        );
        owner
            .edit(&adopted, &field, token, &TextEdit::CancelComposition)
            .unwrap();
        owner
            .edit(&adopted, &field, token, &TextEdit::Redo)
            .unwrap();
        assert_eq!(owner.filter.editor().text(), "entity-0001");
        owner
            .edit(&adopted, &field, token, &TextEdit::Undo)
            .unwrap();
        assert_eq!(owner.filter.editor().text(), "entity");
        owner
            .edit(
                &adopted,
                &field,
                token,
                &TextEdit::Preedit {
                    text: "cancel on blur".into(),
                    cursor: None,
                },
            )
            .unwrap();
        let composing = owner
            .compile(&model, viewport, UiTheme::default(), false)
            .unwrap();
        owner.blur(&composing, &field).unwrap();
        assert!(owner.filter.input_token().is_none());
        assert!(!owner.filter.editor().is_composing());
        assert!(
            owner
                .edit(&composing, &field, token, &TextEdit::Commit("late".into()))
                .is_err()
        );
        let blurred = owner
            .compile(&model, viewport, UiTheme::default(), false)
            .unwrap();
        assert!(
            !blurred
                .semantics()
                .iter()
                .any(|node| node.role() == UiSemanticRole::TextInput)
        );
        assert_eq!(owner.filter.editor().text(), "entity");
    }
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}

#[test]
fn active_filter_after_text_scaling_still_retires_on_mode_and_document_changes() {
    let (_project, application) = application(EditorDocumentKind::Scene, 3);
    let model = application.current_frame().unwrap();
    let viewport = environment(180.0);
    for exit in [Some("Parent"), Some("Remove Entity"), None] {
        let mut owner = controller();
        owner.set_appearance(appearance(1.0, UiDensity::Normal));
        let frame = owner
            .compile(&model, viewport, UiTheme::default(), false)
            .unwrap();
        let field = input(&frame);
        let mut routing = RoutedInput::new(&frame);
        routing.focus(&mut owner, &frame, &field);
        let token = owner.input_token(&frame, &field).unwrap().unwrap();
        owner
            .edit(
                &frame,
                &field,
                token,
                &TextEdit::Preedit {
                    text: "preview".into(),
                    cursor: None,
                },
            )
            .unwrap();
        owner.set_appearance(appearance(2.0, UiDensity::Normal));
        let adopted = owner
            .compile(&model, viewport, UiTheme::default(), false)
            .unwrap();
        assert_eq!(owner.input_token(&adopted, &field).unwrap(), Some(token));
        if let Some(name) = exit {
            assert!(activate(&mut owner, &adopted, name, &model).is_none());
            owner
                .compile(&model, viewport, UiTheme::default(), false)
                .unwrap();
        } else {
            let mut workspace = model.workspace().clone();
            workspace.active_document = None;
            workspace.active_scene = None;
            let changed = republish(&model, workspace);
            owner
                .compile(&changed, viewport, UiTheme::default(), false)
                .unwrap();
            assert_ne!(owner.filter.key(), &field);
        }
        assert!(owner.filter.input_token().is_none());
        assert!(!owner.filter.editor().is_composing());
        assert!(
            owner
                .edit(&adopted, &field, token, &TextEdit::Commit("late".into()))
                .is_err()
        );
    }
}

#[test]
fn compact_actions_and_complete_parent_order_choices_remain_reachable_at_both_text_scales() {
    let (_project, application) = application(EditorDocumentKind::Scene, 64);
    let model = application.current_frame().unwrap();
    for scale in [1.0, 2.0] {
        for density in [UiDensity::Normal, UiDensity::Compact] {
            for height in [80.0, 32.0] {
                let mut owner = controller();
                let effective = appearance(scale, density);
                owner.set_appearance(effective);
                let mut frame = owner
                    .compile(&model, environment(height), UiTheme::default(), false)
                    .unwrap();
                for name in ["+ Root", "+ Child", "Remove Entity", "Parent", "Order"] {
                    let key = button(&frame, name);
                    assert_reachable(&frame, &key);
                    assert!(
                        rect(&frame, &key).size().height()
                            >= effective
                                .control_metrics(UiControlSize::Small)
                                .unwrap()
                                .height()
                    );
                }
                for (menu, last) in [
                    ("Parent", "Parent entity: entity-0063"),
                    ("Order", "Order: After entity-0063"),
                ] {
                    activate(&mut owner, &frame, menu, &model);
                    let choices = owner
                        .compile(&model, environment(height), UiTheme::default(), false)
                        .unwrap();
                    assert_reachable(&choices, &button(&choices, "Close choices"));
                    assert!(owner.scroll(&choices, f32::MAX).unwrap());
                    let bottom = owner
                        .compile(&model, environment(height), UiTheme::default(), false)
                        .unwrap();
                    assert_reachable(&bottom, &button(&bottom, last));
                    assert_reachable(&bottom, &button(&bottom, "Close choices"));
                    activate(&mut owner, &bottom, "Close choices", &model);
                    frame = owner
                        .compile(&model, environment(height), UiTheme::default(), false)
                        .unwrap();
                }
                activate(&mut owner, &frame, "Remove Entity", &model);
                let confirmation = owner
                    .compile(&model, environment(height), UiTheme::default(), false)
                    .unwrap();
                for name in ["Confirm removal", "Cancel removal"] {
                    assert_reachable(&confirmation, &button(&confirmation, name));
                }
            }
        }
    }
}

#[test]
fn explicit_palette_survives_a_distinct_caller_theme_and_reference_mapping_stays_available() {
    let (_project, application) = application(EditorDocumentKind::Scene, 2);
    let model = application.current_frame().unwrap();
    let theme = UiTheme::new(41, UiTheme::default().palette()).unwrap();
    let environment = UiEnvironment::new(
        UiSurfaceId::new(97).unwrap(),
        700.0,
        800.0,
        1.5,
        1,
        1,
        41,
        1,
    )
    .unwrap();
    let mut owner = controller();
    let reference = owner.compile(&model, environment, theme, false).unwrap();
    assert_eq!(
        owner.appearance.palette().text,
        theme.palette().primary_text
    );
    let effective = appearance(2.0, UiDensity::Compact);
    owner.set_appearance(effective);
    let styled = owner.compile(&model, environment, theme, false).unwrap();
    assert!(!Arc::ptr_eq(&reference, &styled));
    assert_eq!(owner.retained(&styled).unwrap().theme, theme);
    assert_eq!(owner.appearance, effective);
    assert!(styled.paint().iter().any(|paint| matches!(paint,
        UiPaintPrimitive::Rect { brush: Some(UiResolvedBrush::Solid(color)), .. }
            if *color == effective.palette().panel
    )));
    assert_role(&styled, "+ Root", effective, UiTextRole::Body);
}

#[test]
fn source_count_remains_visible_while_browsing_under_scaled_typography() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let (_project, application) = application(kind, 64);
        let model = application.current_frame().unwrap();
        for scale in [1.0, 2.0] {
            for density in [UiDensity::Normal, UiDensity::Compact] {
                let mut owner = controller();
                owner.set_appearance(appearance(scale, density));
                let top = owner
                    .compile(&model, environment(800.0), UiTheme::default(), false)
                    .unwrap();
                assert!(owner.scroll(&top, f32::MAX).unwrap());
                let bottom = owner
                    .compile(&model, environment(800.0), UiTheme::default(), false)
                    .unwrap();
                for frame in [&top, &bottom] {
                    let count = frame
                        .semantics()
                        .iter()
                        .find(|node| node.name() == "64 of 64 source entities")
                        .expect("source filtering feedback remains visible during scrolling");
                    let layout = frame
                        .layouts()
                        .iter()
                        .find(|layout| layout.key() == Some(count.key()))
                        .unwrap();
                    assert!(layout.visible());
                    assert!(layout.rect().max().y() <= frame.environment().logical_size().height());
                }
                assert!(
                    !bottom
                        .semantics()
                        .iter()
                        .any(|node| node.name() == "Select entity-0000")
                );
                assert!(
                    bottom
                        .semantics()
                        .iter()
                        .any(|node| node.name() == "Select entity-0063")
                );
                assert!(Arc::ptr_eq(
                    &bottom,
                    &owner
                        .compile(&model, environment(800.0), UiTheme::default(), false)
                        .unwrap()
                ));
            }
        }
        assert_eq!(
            application.current_frame().unwrap().workspace(),
            model.workspace()
        );
    }
}
