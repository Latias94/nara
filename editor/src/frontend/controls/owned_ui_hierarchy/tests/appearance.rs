use nara_ui_kernel::{UiColor, UiPaintPrimitive, UiResolvedBrush};
use nara_ui_widgets::{UiAppearancePalette, UiControlSize, UiDensity};

use super::*;

fn appearance(scale: f32, density: UiDensity) -> UiAppearance {
    UiAppearance::new(
        UiAppearancePalette {
            panel: UiColor::rgba(0.08, 0.10, 0.12, 1.0),
            input: UiColor::rgba(0.03, 0.05, 0.07, 1.0),
            muted_text: UiColor::rgba(0.64, 0.69, 0.74, 1.0),
            selection: UiColor::rgba(0.16, 0.26, 0.36, 1.0),
            hover: UiColor::rgba(0.18, 0.28, 0.38, 1.0),
            pressed: UiColor::rgba(0.20, 0.30, 0.40, 1.0),
            ..Default::default()
        },
        density,
        scale,
    )
    .unwrap()
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
fn complete_appearance_adoption_preserves_form_fields_full_capacity_preedit_and_local_history() {
    let mut fixture = Fixture::new();
    for scale in [1.0, 2.0] {
        for density in [UiDensity::Normal, UiDensity::Compact] {
            let source = fixture.source();
            let history = fixture.history();
            let mut owner = controller();
            owner.set_appearance(UiAppearance::default());
            let frame = paint(&mut owner, &fixture, 800.0);
            let mut routing = RoutedInput::new(&frame);
            click_current(&mut owner, &mut routing, &fixture, "Add UI node");
            click_current(&mut owner, &mut routing, &fixture, "Kind: Text");
            let form = paint(&mut owner, &fixture, 800.0);
            let key = field(&form, "Text");
            let accepted = "q".repeat(FIELD_TEXT_BYTES);
            let token = set_field(&mut owner, &mut routing, &form, "Text", &accepted);
            owner
                .edit(&form, &key, token, &TextEdit::SelectAll)
                .unwrap();
            owner
                .edit(
                    &form,
                    &key,
                    token,
                    &TextEdit::Preedit {
                        text: "staged".into(),
                        cursor: Some((0, 6)),
                    },
                )
                .unwrap();
            let previous = paint(&mut owner, &fixture, 800.0);
            let input = owner.field(Field::Text).unwrap();
            let display = input.editor().display_text().to_owned();
            let selection = input.editor().selection();
            let composition = input.editor().composition_range();
            let old_paint = input.editor().paint_snapshot();
            let form_id = owner.form.as_ref().unwrap().identity;
            let fields = owner
                .form
                .as_ref()
                .unwrap()
                .fields
                .iter()
                .map(|field| field.input.key().clone())
                .collect::<Vec<_>>();
            let basis = owner.basis.clone();
            let effective = appearance(scale, density);
            owner.set_appearance(effective);
            assert!(owner.scroll(&previous, 1.0).is_err());
            assert!(owner.ime_area(&previous, &key, token).is_err());
            assert!(
                owner
                    .point(&previous, &key, token, UiPoint::new(12.0, 12.0), false)
                    .is_err()
            );
            let adopted = paint(&mut owner, &fixture, 800.0);
            assert!(!Arc::ptr_eq(&previous, &adopted));
            assert_eq!(owner.appearance, effective);
            assert_eq!(owner.basis, basis);
            assert_eq!(owner.form.as_ref().unwrap().identity, form_id);
            assert_eq!(
                owner
                    .form
                    .as_ref()
                    .unwrap()
                    .fields
                    .iter()
                    .map(|field| field.input.key().clone())
                    .collect::<Vec<_>>(),
                fields
            );
            assert_eq!(field(&adopted, "Text"), key);
            assert_eq!(owner.input_token(&adopted, &key).unwrap(), Some(token));
            let input = owner.field(Field::Text).unwrap();
            assert_eq!(
                input.editor().typography(),
                effective.typography(UiTextRole::Body).unwrap()
            );
            assert_eq!(input.editor().text(), accepted);
            assert_eq!(input.editor().display_text(), display);
            assert_eq!(input.editor().selection(), selection);
            assert_eq!(input.editor().composition_range(), composition);
            assert!(input.editor().can_undo());
            assert_eq!(
                input.appearance(),
                Some(&effective.field(true, true, false).unwrap())
            );
            assert_eq!(
                old_paint.same_source(&input.editor().paint_snapshot()),
                scale == 1.0
            );
            let field_rect = adopted
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(&key))
                .unwrap()
                .rect();
            let padding = input.appearance().unwrap().padding;
            let ime = owner.ime_area(&adopted, &key, token).unwrap();
            assert!(ime.max().y() + padding.bottom() <= field_rect.size().height() + 0.01);
            assert!(
                !owner
                    .invalidated_controls(&previous, &adopted)
                    .unwrap()
                    .contains(&key)
            );
            assert_role(&adopted, "Create UI node", effective, UiTextRole::Title);
            assert_role(&adopted, "Choose placement", effective, UiTextRole::Body);
            let StructureDialog::Create { placement, .. } = &owner.form.as_ref().unwrap().dialog
            else {
                unreachable!()
            };
            assert_role(
                &adopted,
                &placement_intent_label(Some(placement)),
                effective,
                UiTextRole::Caption,
            );
            assert!(!enabled(&adopted, "Apply structure change"));
            owner.set_appearance(effective);
            assert!(Arc::ptr_eq(&adopted, &paint(&mut owner, &fixture, 800.0)));
            owner
                .edit(&adopted, &key, token, &TextEdit::CancelComposition)
                .unwrap();
            owner.edit(&adopted, &key, token, &TextEdit::Undo).unwrap();
            assert_eq!(owner.field(Field::Text).unwrap().editor().text(), "Text");
            owner.edit(&adopted, &key, token, &TextEdit::Redo).unwrap();
            assert_eq!(owner.field(Field::Text).unwrap().editor().text(), accepted);
            assert_eq!(fixture.history(), history);
            assert_eq!(fixture.source(), source);
            let intent = owner.form.as_ref().unwrap().intent().unwrap();
            assert!(matches!(
                &intent,
                UiStructureIntent::Create {
                    parameters: UiCreateParameters::Text { content },
                    ..
                } if content == &accepted
            ));
            let expected = fixture.ui().lower_structure_intent(&intent).unwrap();
            let prepared =
                click_current(&mut owner, &mut routing, &fixture, "Apply structure change")
                    .unwrap();
            let action = apply(&mut owner, &mut fixture, prepared, 1);
            assert_eq!(action, expected);
            assert!(owner.form.is_none());
            let ui = fixture.ui();
            let selected = ui.selection().top_entity().unwrap();
            assert!(ui.projection().unwrap().fields().iter().any(|descriptor| {
                descriptor.key().entity() == selected && descriptor.label() == "Content"
                    && matches!(descriptor.value(), nara::ui::__private::UiAuthoringFieldValue::Text(value) if value == &accepted)
            }));
            assert_eq!(fixture.history(), history + 1);
        }
    }
}

#[test]
fn compact_adoption_reveals_the_active_field_and_keeps_choices_and_apply_cancel_reachable() {
    let fixture = Fixture::new();
    for density in [UiDensity::Normal, UiDensity::Compact] {
        let mut owner = controller();
        owner.set_appearance(appearance(1.0, UiDensity::Normal));
        let frame = paint(&mut owner, &fixture, 208.0);
        let mut routing = RoutedInput::new(&frame);
        click_current(&mut owner, &mut routing, &fixture, "Add UI node");
        click_current(&mut owner, &mut routing, &fixture, "Kind: Text");
        let form = paint(&mut owner, &fixture, 208.0);
        let key = field(&form, "Text");
        let token = set_field(&mut owner, &mut routing, &form, "Text", "Draft");
        owner
            .edit(
                &form,
                &key,
                token,
                &TextEdit::Preedit {
                    text: "preview".into(),
                    cursor: None,
                },
            )
            .unwrap();
        let previous = paint(&mut owner, &fixture, 208.0);
        let effective = appearance(2.0, density);
        owner.set_appearance(effective);
        let adopted = paint(&mut owner, &fixture, 208.0);
        assert_eq!(owner.input_token(&adopted, &key).unwrap(), Some(token));
        assert!(owner.is_composing(&adopted, &key, token).unwrap());
        assert!(owner.form_scroll > 0.0);
        assert!(owner.ime_area(&previous, &key, token).is_err());
        for name in ["Apply structure change", "Cancel structure change"] {
            let key = button(&adopted, name);
            let layout = adopted
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(&key))
                .unwrap();
            assert!(
                layout.rect().size().height()
                    >= effective
                        .control_metrics(UiControlSize::Small)
                        .unwrap()
                        .height()
            );
        }
        assert_reachable(&adopted, &button(&adopted, "Cancel structure change"));
        owner
            .edit(&adopted, &key, token, &TextEdit::CancelComposition)
            .unwrap();
        let committed = paint(&mut owner, &fixture, 208.0);
        owner.scroll(&committed, -f32::MAX).unwrap();
        let current = paint(&mut owner, &fixture, 208.0);
        owner
            .activate(
                &current,
                &button(&current, "Choose placement"),
                &fixture.model(),
                &fixture.ui(),
                false,
            )
            .unwrap();
        let choices = paint(&mut owner, &fixture, 80.0);
        assert_reachable(&choices, &button(&choices, "Close choices"));
        assert!(
            choices
                .semantics()
                .iter()
                .any(|node| node.name().starts_with("Placement: "))
        );
        if owner.retained(&choices).unwrap().scroll_limit > 0.0 {
            owner.scroll(&choices, f32::MAX).unwrap();
        }
        let last = paint(&mut owner, &fixture, 80.0);
        let key = last
            .semantics()
            .iter()
            .rev()
            .find(|node| node.name().starts_with("Placement: "))
            .unwrap()
            .key();
        assert_reachable(&last, key);
        assert!(owner.escape(&last).unwrap());
        let compact = paint(&mut owner, &fixture, 80.0);
        assert_reachable(&compact, &button(&compact, "Apply structure change"));
        assert_reachable(&compact, &button(&compact, "Cancel structure change"));
        assert_eq!(owner.form.as_ref().unwrap().value(Field::Text), "Draft");
    }
}

#[test]
fn explicit_palette_and_caller_theme_keep_their_own_exact_values() {
    let fixture = Fixture::new();
    let mut owner = controller();
    let theme = UiTheme::new(43, UiTheme::default().palette()).unwrap();
    let environment = UiEnvironment::new(
        UiSurfaceId::new(97).unwrap(),
        700.0,
        800.0,
        1.5,
        1,
        1,
        43,
        1,
    )
    .unwrap();
    owner
        .compile(&fixture.model(), &fixture.ui(), environment, theme, false)
        .unwrap();
    assert_eq!(
        owner.appearance.palette().text,
        theme.palette().primary_text
    );
    let effective = appearance(2.0, UiDensity::Compact);
    owner.set_appearance(effective);
    let frame = owner
        .compile(&fixture.model(), &fixture.ui(), environment, theme, false)
        .unwrap();
    assert_eq!(owner.retained(&frame).unwrap().theme, theme);
    assert_eq!(owner.retained(&frame).unwrap().appearance, effective);
    assert_eq!(owner.appearance, effective);
    assert_role(&frame, "UI Hierarchy", effective, UiTextRole::Title);
    assert_role(&frame, "Add UI node", effective, UiTextRole::Body);
    assert!(frame.paint().iter().any(|paint| matches!(paint,
        UiPaintPrimitive::Rect { brush: Some(UiResolvedBrush::Solid(color)), .. }
            if *color == effective.palette().panel
    )));
}
