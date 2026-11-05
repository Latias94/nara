use std::sync::Arc;

use nara_text::{
    FontFace, TextEdit, TextEditError, TextEngine, TextLayoutError, TextLimits, TextTypography,
    bundled_font_face,
};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiBorder, UiBrush, UiColor, UiCompiler, UiCustomLayout,
    UiDimension, UiEdges, UiEnvironment, UiInputEvent, UiInteractionSession, UiLayer, UiLayout,
    UiLayoutChildren, UiLayoutConstraints, UiLayoutError, UiLayoutMeasurement, UiLayoutOutput,
    UiNonActivation, UiPaintPrimitive, UiPoint, UiRect, UiResolvedBrush, UiRoot, UiRootKey,
    UiRouteStamp, UiSeatId, UiSemanticRole, UiSize, UiStyle, UiSurfaceId, UiTheme, UiVisual,
};
use nara_ui_widgets::{UiTextField, UiTextFieldAppearance, UiTextFieldError};

fn engine() -> TextEngine {
    TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap()
}

fn root() -> UiRootKey {
    UiRootKey::runtime("owned-text-field", "inspector").unwrap()
}

fn frame(field: &UiTextField, revision: u64) -> Arc<nara_ui_kernel::UiFrame> {
    frame_with_style(
        field,
        UiStyle::absolute(12.0, 10.0, 100.0, 36.0),
        100.0,
        revision,
    )
}

fn frame_with_style(
    field: &UiTextField,
    style: UiStyle,
    viewport_width: f32,
    revision: u64,
) -> Arc<nara_ui_kernel::UiFrame> {
    let root = root();
    let node = field.authored_node(&root, style, viewport_width).unwrap();
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

fn text_window(frame: &nara_ui_kernel::UiFrame) -> (UiRect, UiRect, &nara_text::TextLayout) {
    frame
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Text {
                rect,
                clip_rect: Some(clip),
                layout,
                ..
            } => Some((*rect, *clip, layout.as_ref())),
            _ => None,
        })
        .expect("the field must publish its prepared text window")
}

fn assert_rect_near(actual: UiRect, expected: UiRect) {
    for (actual, expected) in [
        (actual.origin().x(), expected.origin().x()),
        (actual.origin().y(), expected.origin().y()),
        (actual.max().x(), expected.max().x()),
        (actual.max().y(), expected.max().y()),
    ] {
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }
}

struct MeasureField {
    expected: UiLayoutMeasurement,
}

impl UiLayout for MeasureField {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        assert_eq!(children.len(), 1);
        let measured = children.measure(0, UiLayoutConstraints::MAX_CONTENT)?;
        assert_rect_near(
            UiRect::new(UiPoint::default(), measured.size()).unwrap(),
            UiRect::new(UiPoint::default(), self.expected.size()).unwrap(),
        );
        assert!(
            (measured.first_baseline().unwrap() - self.expected.first_baseline().unwrap()).abs()
                < 0.001
        );
        children.place(
            0,
            UiRect::new(UiPoint::new(12.0, 10.0), measured.size()).unwrap(),
        )?;
        UiLayoutOutput::new(UiSize::new(240.0, 100.0).unwrap())
            .with_first_baseline(10.0 + measured.first_baseline().unwrap())
    }
}

#[test]
fn child_measurement_reports_real_field_baselines_and_minimum_heights_without_reshaping() {
    let primary = bundled_font_face().unwrap();
    let cjk = FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/NotoSansSC-subset.otf").to_vec(),
        0,
    )
    .unwrap();
    let automatic = TextTypography::new(20.0).unwrap();
    let short = automatic.with_line_height(6.0).unwrap();
    for (fonts, text, typography, baseline, minimum_height) in [
        (vec![primary.clone()], "M", short, 24.7, 36.0),
        (
            vec![primary.clone(), cjk.clone()],
            "A中",
            automatic,
            29.2,
            40.96,
        ),
        (
            vec![primary.clone(), cjk.clone()],
            "A中",
            short,
            29.2,
            40.96,
        ),
    ] {
        let mut engine = TextEngine::with_fonts(fonts.clone(), TextLimits::default()).unwrap();
        let field = UiTextField::with_typography(root(), "Measured", text, typography, &mut engine)
            .unwrap();
        if text == "A中" {
            assert_eq!(
                field.editor().layout().glyphs()[1].key().face_id(),
                cjk.id()
            );
        }
        let minimum = field.layout_measurement(160.0).unwrap();
        assert!((minimum.size().height() - minimum_height).abs() < 0.001);
        assert!((minimum.first_baseline().unwrap() - baseline).abs() < 0.001);
        let source = field.editor().paint_snapshot();
        for height in [
            UiDimension::Auto,
            UiDimension::Px(6.0),
            UiDimension::Px(70.0),
        ] {
            let expected_height = match height {
                UiDimension::Px(70.0) => 70.0,
                _ => minimum_height,
            };
            let expected = UiLayoutMeasurement::new(
                UiSize::new(160.0, expected_height).unwrap(),
                Some(baseline),
            )
            .unwrap();
            let node = field
                .authored_node(
                    &root(),
                    UiStyle::flex()
                        .with_width(UiDimension::Px(160.0))
                        .with_height(height),
                    160.0,
                )
                .unwrap();
            let snapshot = UiAuthoringSnapshotBuilder::new(1)
                .with_root(
                    UiRoot::new(root(), UiLayer::Base, UiStyle::fill())
                        .with_custom_layout(UiCustomLayout::new(MeasureField { expected }))
                        .with_child(node),
                )
                .build()
                .unwrap();
            let mut compiler = UiCompiler::with_fonts(fonts.clone(), Default::default()).unwrap();
            let frame = compiler
                .compile(
                    &snapshot,
                    UiEnvironment::new(
                        UiSurfaceId::new(81).unwrap(),
                        240.0,
                        100.0,
                        1.5,
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
            let outer = frame
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(field.key()))
                .unwrap()
                .rect();
            assert!((outer.size().height() - expected_height).abs() < 0.001);
            let (draw, clip, painted) = text_window(&frame);
            let painted_baseline = draw.origin().y() + painted.lines()[0].baseline();
            assert!((painted_baseline - outer.origin().y() - baseline).abs() < 0.001);
            assert!(clip.origin().y() >= outer.origin().y());
            assert!(clip.max().y() <= outer.max().y());
            assert_eq!(compiler.work_stats().text.layout_attempts, 0);
            assert!(source.same_source(&field.editor().paint_snapshot()));
        }
    }
}

#[test]
fn short_line_height_preserves_real_ink_and_all_editing_geometry_in_a_generous_field() {
    let mut engine = engine();
    let typography = TextTypography::new(20.0)
        .unwrap()
        .with_line_height(6.0)
        .unwrap();
    let mut field =
        UiTextField::with_typography(root(), "Short line", "M", typography, &mut engine).unwrap();
    let selection_color = UiColor::rgba(0.2, 0.4, 0.8, 0.35);
    let caret_color = UiColor::rgba(0.9, 0.2, 0.3, 1.0);
    field.set_appearance(UiTextFieldAppearance {
        padding: UiEdges::all(6.0).unwrap(),
        foreground: UiBrush::Role(nara_ui_kernel::UiColorRole::PrimaryText),
        visual: UiVisual::default(),
        selection: UiBrush::Solid(selection_color),
        caret: UiBrush::Solid(caret_color),
    });
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::End { extend: false }, &mut engine)
        .unwrap();
    let source = field.editor().paint_snapshot();
    let line = &field.editor().layout().lines()[0];
    let block_min = line.block_min();
    let block_height = line.block_max() - block_min;
    assert_eq!(source.height(), 6.0);
    assert!(block_min < 0.0 && block_height > source.height());

    let style = UiStyle::absolute(12.0, 10.0, 160.0, 40.0);
    let first = frame_with_style(&field, style, 160.0, 1);
    assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
    let (draw, clip, painted) = text_window(&first);
    assert_eq!(painted.typography(), typography);
    assert_eq!(painted.lines(), field.editor().layout().lines());
    assert_eq!(painted.glyphs().len(), 1);
    assert!((draw.origin().y() - (10.0 + 6.0 - block_min)).abs() < 0.001);
    assert!((clip.size().height() - block_height).abs() < 0.001);
    assert_eq!(clip.origin().y(), 16.0);
    assert!(clip.max().y() < 50.0);

    let scale = 1.5;
    let mut covered_pixels = 0;
    let mut ink_above_origin = false;
    for glyph in painted.glyphs() {
        let raster = engine.rasterize(glyph.key()).unwrap();
        let x0 = ((draw.origin().x() + glyph.x()) * scale).round() + raster.left() as f32;
        let y0 = ((draw.origin().y() + glyph.y()) * scale).round() - raster.top() as f32;
        for (pixel, coverage) in raster.mask().iter().enumerate() {
            if *coverage == 0 {
                continue;
            }
            covered_pixels += 1;
            let x = x0 + (pixel % raster.width() as usize) as f32;
            let y = y0 + (pixel / raster.width() as usize) as f32;
            ink_above_origin |= y < (draw.origin().y() * scale).round();
            assert!(x >= (clip.origin().x() * scale).round());
            assert!(x + 1.0 <= (clip.max().x() * scale).round());
            assert!(y >= (clip.origin().y() * scale).round());
            assert!(y + 1.0 <= (clip.max().y() * scale).round());
        }
    }
    assert!(covered_pixels > 0 && ink_above_origin);

    let glyph = &painted.glyphs()[0];
    let point = UiPoint::new(
        glyph.x() + glyph.advance() * 0.25,
        painted.lines()[0].baseline(),
    );
    let mut reference = field.editor().clone();
    reference
        .edit(
            &mut engine,
            &TextEdit::Point {
                x: point.x(),
                y: point.y(),
                extend: false,
            },
        )
        .unwrap();
    field
        .point(
            token,
            UiPoint::new(
                draw.origin().x() - 12.0 + point.x(),
                draw.origin().y() - 10.0 + point.y(),
            ),
            160.0,
            false,
            &mut engine,
        )
        .unwrap();
    assert_eq!(field.editor().selection(), reference.selection());
    assert_eq!(field.editor().caret(), reference.caret());
    assert!(source.same_source(&field.editor().paint_snapshot()));

    let viewport = UiRect::from_origin_size(6.0, 6.0, 148.0, block_height).unwrap();
    let area = field.editor().ime_area();
    let expected = UiRect::from_origin_size(
        draw.origin().x() - 12.0 + area.x0,
        draw.origin().y() - 10.0 + area.y0,
        area.x1 - area.x0,
        area.y1 - area.y0,
    )
    .unwrap()
    .intersect(viewport)
    .unwrap();
    assert_rect_near(field.ime_area(160.0).unwrap(), expected);
    field
        .edit(token, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    let selected = frame_with_style(&field, style, 160.0, 2);
    for color in [selection_color, caret_color] {
        let rect = selected
            .paint()
            .iter()
            .find_map(|paint| match paint {
                UiPaintPrimitive::Rect {
                    rect,
                    brush: Some(UiResolvedBrush::Solid(actual)),
                    ..
                } if *actual == color => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert!((rect.origin().y() - 16.0).abs() < 0.001);
        assert!((rect.size().height() - block_height).abs() < 0.001);
    }
}

#[test]
fn padding_change_during_scrolled_preedit_keeps_one_origin_and_the_complete_editing_state() {
    let mut engine = engine();
    let typography = TextTypography::new(14.0)
        .unwrap()
        .with_line_height(20.0)
        .unwrap();
    let original = "abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut field =
        UiTextField::with_typography(root(), "Padding", original, typography, &mut engine).unwrap();
    let mut appearance = UiTextFieldAppearance {
        padding: UiEdges::new(7.0, 5.0, 3.0, 9.0).unwrap(),
        foreground: UiBrush::Role(nara_ui_kernel::UiColorRole::PrimaryText),
        visual: UiVisual::default(),
        selection: UiBrush::Solid(UiColor::rgba(0.2, 0.4, 0.8, 0.35)),
        caret: UiBrush::Solid(UiColor::rgba(0.9, 0.2, 0.3, 1.0)),
    };
    field.set_appearance(appearance.clone());
    let token = field.focus().unwrap();
    let key = field.key().clone();
    for action in [
        TextEdit::End { extend: false },
        TextEdit::Insert("x".into()),
        TextEdit::Insert("y".into()),
        TextEdit::Undo,
        TextEdit::Preedit {
            text: "e\u{301}".into(),
            cursor: Some((3, 3)),
        },
    ] {
        field.edit(token, &action, &mut engine).unwrap();
    }
    assert!(
        field.editor().is_composing() && field.editor().can_undo() && field.editor().can_redo()
    );
    let mut reference = field.editor().clone();
    let source = field.editor().paint_snapshot();
    let style = UiStyle::absolute(12.0, 10.0, 160.0, 70.0);
    let first = frame_with_style(&field, style, 160.0, 1);
    assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
    let (old_draw, old_clip, old_layout) = text_window(&first);
    assert!(old_draw.origin().x() < old_clip.origin().x());

    appearance.padding = UiEdges::new(17.0, 11.0, 8.0, 2.0).unwrap();
    field.set_appearance(appearance.clone());
    assert_eq!(field.key(), &key);
    assert_eq!(field.input_token(), Some(token));
    assert_eq!(field.editor().text(), reference.text());
    assert_eq!(field.editor().display_text(), reference.display_text());
    assert_eq!(field.editor().selection(), reference.selection());
    assert_eq!(field.editor().caret(), reference.caret());
    assert_eq!(
        field.editor().composition_range(),
        reference.composition_range()
    );
    assert!(source.same_source(&field.editor().paint_snapshot()));
    let adopted = frame_with_style(&field, style, 160.0, 2);
    assert!(
        adopted.diagnostics().is_empty(),
        "{:?}",
        adopted.diagnostics()
    );
    let (draw, clip, layout) = text_window(&adopted);
    assert_eq!(clip.origin(), UiPoint::new(29.0, 18.0));
    assert!((draw.origin().x() - old_draw.origin().x() + 6.0).abs() < 0.001);
    assert!((draw.origin().y() - old_draw.origin().y() - 5.0).abs() < 0.001);
    assert!(draw.origin().x() < clip.origin().x());
    assert_eq!(layout.lines(), old_layout.lines());
    let viewport = UiRect::from_origin_size(17.0, 8.0, 132.0, 20.0).unwrap();
    let area = reference.ime_area();
    let expected = UiRect::from_origin_size(
        draw.origin().x() - 12.0 + area.x0,
        draw.origin().y() - 10.0 + area.y0,
        area.x1 - area.x0,
        area.y1 - area.y0,
    )
    .unwrap()
    .intersect(viewport)
    .unwrap();
    assert_rect_near(field.ime_area(160.0).unwrap(), expected);

    let glyph = layout
        .glyphs()
        .iter()
        .find(|glyph| {
            glyph.advance() > 0.0
                && draw.origin().x() + glyph.x() > clip.origin().x()
                && draw.origin().x() + glyph.x() + glyph.advance() < clip.max().x()
        })
        .unwrap();
    let point = UiPoint::new(
        glyph.x() + glyph.advance() * 0.25,
        layout.lines()[0].baseline(),
    );
    reference
        .edit(
            &mut engine,
            &TextEdit::Point {
                x: point.x(),
                y: point.y(),
                extend: false,
            },
        )
        .unwrap();
    field
        .point(
            token,
            UiPoint::new(
                draw.origin().x() - 12.0 + point.x(),
                draw.origin().y() - 10.0 + point.y(),
            ),
            160.0,
            false,
            &mut engine,
        )
        .unwrap();
    assert_eq!(field.editor().selection(), reference.selection());
    assert_eq!(field.editor().caret(), reference.caret());
    assert_eq!(field.editor().text(), format!("{original}x"));
    assert!(!field.editor().is_composing());
    for action in [TextEdit::Undo, TextEdit::Redo, TextEdit::Redo] {
        assert_eq!(
            field.edit(token, &action, &mut engine).unwrap(),
            reference.edit(&mut engine, &action).unwrap()
        );
        assert_eq!(field.editor().text(), reference.text());
        assert_eq!(field.editor().selection(), reference.selection());
        assert_eq!(field.editor().caret(), reference.caret());
    }
    assert_eq!(field.editor().text(), format!("{original}xy"));
    assert_eq!(field.input_token(), Some(token));
    assert_eq!(field.appearance(), Some(&appearance));
    assert_eq!(text_window(&first).0, old_draw);
    assert_eq!(text_window(&first).1, old_clip);
}

#[test]
fn adopted_appearance_and_typography_share_paint_pointer_and_ime_coordinates() {
    let mut engine = engine();
    let typography = TextTypography::new(14.0)
        .unwrap()
        .with_line_height(20.0)
        .unwrap();
    let mut field = UiTextField::with_typography(
        root(),
        "Styled",
        "abcdefghijklmnopqrstuvwxyz q\u{301}\u{327} abc אבג",
        typography,
        &mut engine,
    )
    .unwrap();
    let key = field.key().clone();
    let token = field.focus().unwrap();
    let padding = UiEdges::new(11.0, 7.0, 3.0, 9.0).unwrap();
    let foreground = UiColor::rgba(0.9, 0.8, 0.7, 1.0);
    let background = UiColor::rgba(0.1, 0.12, 0.15, 1.0);
    let border_color = UiColor::rgba(0.6, 0.5, 0.4, 1.0);
    let selection_color = UiColor::rgba(0.4, 0.3, 0.2, 0.35);
    let caret_color = UiColor::rgba(0.7, 0.3, 0.2, 1.0);
    let appearance = UiTextFieldAppearance {
        padding,
        foreground: UiBrush::Solid(foreground),
        visual: UiVisual::solid(background)
            .with_border(UiBorder::new(2.0, UiBrush::Solid(border_color)).unwrap()),
        selection: UiBrush::Solid(selection_color),
        caret: UiBrush::Solid(caret_color),
    };
    field.set_appearance(appearance.clone());
    field
        .edit(token, &TextEdit::End { extend: false }, &mut engine)
        .unwrap();
    let style =
        UiStyle::absolute(12.0, 10.0, 160.0, 70.0).with_padding(UiEdges::all(30.0).unwrap());
    let first = frame_with_style(&field, style, 160.0, 1);
    assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
    let (origin, clip, local_point) = first
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Text {
                rect,
                clip_rect: Some(clip),
                layout,
                color,
                ..
            } => {
                assert_eq!(*color, foreground);
                assert_eq!(layout.typography(), typography);
                assert_eq!(rect.origin().y(), 10.0 + padding.top());
                assert_eq!(
                    clip.origin(),
                    UiPoint::new(12.0 + padding.left(), 10.0 + padding.top())
                );
                assert!((clip.size().height() - 20.0).abs() < 0.001);
                let visible = layout
                    .glyphs()
                    .iter()
                    .find(|glyph| {
                        glyph.advance() > 0.0
                            && rect.origin().x() + glyph.x() > clip.origin().x()
                            && rect.origin().x() + glyph.x() + glyph.advance() < clip.max().x()
                    })
                    .unwrap();
                Some((
                    rect.origin(),
                    *clip,
                    UiPoint::new(
                        visible.x() + visible.advance() * 0.25,
                        layout.lines()[0].baseline(),
                    ),
                ))
            }
            _ => None,
        })
        .unwrap();
    assert!(origin.x() < clip.origin().x());
    assert!(first.paint().iter().any(|paint| matches!(paint,
        UiPaintPrimitive::Rect { brush: Some(UiResolvedBrush::Solid(color)), border: Some(border), .. }
            if *color == background && border.width() == 2.0 && border.color() == border_color
    )));
    assert!(first.paint().iter().any(|paint| matches!(paint,
        UiPaintPrimitive::Rect { brush: Some(UiResolvedBrush::Solid(color)), .. }
            if *color == caret_color
    )));
    let mut reference = field.editor().clone();
    reference
        .edit(
            &mut engine,
            &TextEdit::Point {
                x: local_point.x(),
                y: local_point.y(),
                extend: false,
            },
        )
        .unwrap();
    field
        .point(
            token,
            UiPoint::new(
                origin.x() - 12.0 + local_point.x(),
                origin.y() - 10.0 + local_point.y(),
            ),
            160.0,
            false,
            &mut engine,
        )
        .unwrap();
    assert_eq!(field.editor().selection(), reference.selection());
    assert_eq!(field.editor().caret(), reference.caret());
    let pointed = frame_with_style(&field, style, 160.0, 2);
    let (origin, clip) = pointed
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Text {
                rect,
                clip_rect: Some(clip),
                ..
            } => Some((rect.origin(), *clip)),
            _ => None,
        })
        .unwrap();
    let ime = field.editor().ime_area();
    let expected = UiRect::from_origin_size(
        origin.x() + ime.x0,
        origin.y() + ime.y0,
        ime.x1 - ime.x0,
        ime.y1 - ime.y0,
    )
    .unwrap()
    .intersect(clip)
    .unwrap();
    let actual = field.ime_area(160.0).unwrap();
    assert!((actual.origin().x() + 12.0 - expected.origin().x()).abs() < 0.001);
    assert!((actual.origin().y() + 10.0 - expected.origin().y()).abs() < 0.001);
    assert!((actual.size().width() - expected.size().width()).abs() < 0.001);
    assert!((actual.size().height() - expected.size().height()).abs() < 0.001);
    field
        .edit(token, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    let selected = frame_with_style(&field, style, 160.0, 3);
    assert!(selected.paint().iter().any(|paint| matches!(paint,
        UiPaintPrimitive::Rect { rect, brush: Some(UiResolvedBrush::Solid(color)), .. }
            if *color == selection_color && rect.origin().x() == 12.0 + padding.left()
    )));
    field
        .edit(
            token,
            &TextEdit::Preedit {
                text: "e\u{301}".into(),
                cursor: Some((3, 3)),
            },
            &mut engine,
        )
        .unwrap();
    let before = field.editor().paint_snapshot();
    let display = field.editor().display_text().to_owned();
    let larger = TextTypography::new(28.0)
        .unwrap()
        .with_line_height(40.0)
        .unwrap();
    field.reconfigure(&mut engine, larger).unwrap();
    assert_eq!(field.key(), &key);
    assert_eq!(field.input_token(), Some(token));
    assert!(field.enabled());
    assert_eq!(field.appearance(), Some(&appearance));
    assert!(field.editor().is_composing());
    assert_eq!(field.editor().display_text(), display);
    assert!(!before.same_source(&field.editor().paint_snapshot()));
    let enlarged = frame_with_style(&field, style, 160.0, 4);
    assert!(enlarged.diagnostics().is_empty());
    let window_height =
        field.layout_measurement(160.0).unwrap().size().height() - padding.top() - padding.bottom();
    assert!(enlarged.paint().iter().any(|paint| matches!(paint,
        UiPaintPrimitive::Text { rect, layout, .. }
            if layout.typography() == larger && (rect.size().height() - window_height).abs() < 0.001
    )));
    assert!(first.paint().iter().any(|paint| matches!(paint,
        UiPaintPrimitive::Text { layout, .. } if layout.typography() == typography
    )));
}

#[test]
fn rejected_typography_and_font_adoption_preserves_field_focus_style_and_hidden_origin() {
    let mona = FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/MonaSans-VariableFont.ttf").to_vec(),
        0,
    )
    .unwrap();
    let mono = FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let limits = TextLimits {
        max_glyphs: 2,
        ..TextLimits::default()
    };
    let mut original = TextEngine::new(mona, limits).unwrap();
    let mut receiving = TextEngine::new(mono, limits).unwrap();
    let typography = TextTypography::new(14.0)
        .unwrap()
        .with_line_height(20.0)
        .unwrap();
    let mut field =
        UiTextField::with_typography(root(), "Ligature", "ffi", typography, &mut original).unwrap();
    let key = field.key().clone();
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::SelectAll, &mut original)
        .unwrap();
    field
        .edit(
            token,
            &TextEdit::Preedit {
                text: "x".into(),
                cursor: None,
            },
            &mut original,
        )
        .unwrap();
    let paint = field.editor().paint_snapshot();
    let selection = field.editor().selection();
    let ime = field.ime_area(100.0).unwrap();
    let larger = TextTypography::new(28.0)
        .unwrap()
        .with_line_height(40.0)
        .unwrap();
    assert!(matches!(
        field.reconfigure(&mut receiving, larger),
        Err(UiTextFieldError::Text(TextEditError::Layout(
            TextLayoutError::GlyphLimitExceeded {
                actual: 3,
                maximum: 2,
            }
        )))
    ));
    assert_eq!(field.key(), &key);
    assert_eq!(field.input_token(), Some(token));
    assert!(field.enabled());
    assert_eq!(field.editor().typography(), typography);
    assert_eq!(field.editor().text(), "ffi");
    assert_eq!(field.editor().display_text(), "x");
    assert_eq!(field.editor().selection(), selection);
    assert_eq!(field.ime_area(100.0).unwrap(), ime);
    assert!(paint.same_source(&field.editor().paint_snapshot()));
    field
        .edit(token, &TextEdit::CancelComposition, &mut original)
        .unwrap();
    field
        .edit(token, &TextEdit::Commit("A".into()), &mut original)
        .unwrap();
    field.reconfigure(&mut original, larger).unwrap();
    assert_eq!(field.input_token(), Some(token));
    field.edit(token, &TextEdit::Undo, &mut original).unwrap();
    assert_eq!(field.editor().text(), "ffi");
    assert_eq!(field.editor().selection_bytes(), 0..3);
    assert_eq!(field.editor().layout().typography(), larger);
    field.edit(token, &TextEdit::Redo, &mut original).unwrap();
    assert_eq!(field.editor().text(), "A");
    assert_eq!(field.editor().layout().typography(), larger);
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
