use std::sync::Arc;

use nara_text::{
    FontFace, TextDirection, TextEdit, TextEditor, TextEngine, TextLayout, TextLimits, TextRect,
    TextTypography, bundled_font_face,
};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiAvailableSpace, UiBrush, UiColor, UiColorRole, UiCompiler,
    UiCustomLayout, UiDimension, UiEdges, UiEnvironment, UiFrame, UiKey, UiLayer, UiLayout,
    UiLayoutChildren, UiLayoutConstraints, UiLayoutError, UiLayoutMeasurement, UiLayoutOutput,
    UiNode, UiPaintPrimitive, UiPoint, UiRect, UiResolvedBrush, UiRoot, UiRootDiagnostic,
    UiRootKey, UiRootStatus, UiSize, UiStyle, UiSurfaceId, UiTheme, UiVisual,
};
use nara_ui_widgets::{UiTextField, UiTextFieldAppearance};

const ORIGIN: UiPoint = UiPoint::new(12.25, 9.5);
const SELECTION: UiColor = UiColor::rgba(0.2, 0.4, 0.8, 0.35);
const CARET: UiColor = UiColor::rgba(0.9, 0.2, 0.3, 1.0);

fn root() -> UiRootKey {
    UiRootKey::runtime("allocated-text-field", "property").unwrap()
}

fn appearance() -> UiTextFieldAppearance {
    UiTextFieldAppearance {
        padding: UiEdges::new(11.25, 7.5, 3.5, 9.25).unwrap(),
        foreground: UiBrush::Role(UiColorRole::PrimaryText),
        visual: UiVisual::default(),
        selection: UiBrush::Solid(SELECTION),
        caret: UiBrush::Solid(CARET),
    }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

fn same_rect(actual: UiRect, expected: UiRect) {
    close(actual.origin().x(), expected.origin().x());
    close(actual.origin().y(), expected.origin().y());
    close(actual.max().x(), expected.max().x());
    close(actual.max().y(), expected.max().y());
}

struct AllocateField {
    width: f32,
    preferred: UiLayoutMeasurement,
    minimum_width: f32,
    authored_width: Option<f32>,
}

impl UiLayout for AllocateField {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        assert_eq!(children.len(), 1);
        close(
            children.min_content_width(0)?,
            self.authored_width.unwrap_or(self.minimum_width),
        );
        for (space, expected_width) in [
            (UiAvailableSpace::MaxContent, self.preferred.size().width()),
            (UiAvailableSpace::MinContent, self.minimum_width),
            (UiAvailableSpace::Definite(37.25), 37.25),
        ] {
            let measured = children.measure(
                0,
                UiLayoutConstraints {
                    width: space,
                    height: UiAvailableSpace::MaxContent,
                },
            )?;
            close(
                measured.size().width(),
                self.authored_width.unwrap_or(expected_width),
            );
            close(measured.size().height(), self.preferred.size().height());
            close(
                measured.first_baseline().unwrap(),
                self.preferred.first_baseline().unwrap(),
            );
        }
        children.place(
            0,
            UiRect::new(
                ORIGIN,
                UiSize::new(self.width, self.preferred.size().height()).unwrap(),
            )
            .unwrap(),
        )?;
        UiLayoutOutput::new(UiSize::new(640.0, 160.0).unwrap())
            .with_first_baseline(ORIGIN.y() + self.preferred.first_baseline().unwrap())
    }
}

fn allocation(
    field: &UiTextField,
    preferred_width: f32,
    width: f32,
    authored_width: Option<f32>,
) -> UiCustomLayout {
    let padding = field.appearance().unwrap().padding;
    UiCustomLayout::new(AllocateField {
        width,
        preferred: field.layout_measurement(preferred_width).unwrap(),
        minimum_width: padding.left() + padding.right() + 1.0,
        authored_width,
    })
}

fn compile(
    compiler: &mut UiCompiler,
    field: &UiNode,
    allocation: &UiCustomLayout,
    revision: u64,
    scale: f32,
) -> Arc<UiFrame> {
    compiler
        .compile(
            &UiAuthoringSnapshotBuilder::new(revision)
                .with_root(
                    UiRoot::new(root(), UiLayer::Base, UiStyle::fill())
                        .with_custom_layout(allocation.clone())
                        .with_child(field.clone()),
                )
                .build()
                .unwrap(),
            UiEnvironment::new(
                UiSurfaceId::new(96).unwrap(),
                640.0,
                160.0,
                scale,
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

fn text(frame: &UiFrame) -> (&UiKey, UiRect, UiRect, &TextLayout) {
    frame
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Text {
                key: Some(key),
                rect,
                clip_rect: Some(clip),
                layout,
                ..
            } => Some((key, *rect, *clip, layout.as_ref())),
            _ => None,
        })
        .unwrap()
}

fn rectangles(frame: &UiFrame, color: UiColor) -> Vec<UiRect> {
    frame
        .paint()
        .iter()
        .filter_map(|paint| match paint {
            UiPaintPrimitive::Rect {
                rect,
                brush: Some(UiResolvedBrush::Solid(actual)),
                ..
            } if *actual == color => Some(*rect),
            _ => None,
        })
        .collect()
}

fn translated(rect: TextRect, origin: UiPoint) -> UiRect {
    UiRect::from_origin_size(
        origin.x() + rect.x0,
        origin.y() + rect.y0,
        rect.x1 - rect.x0,
        rect.y1 - rect.y0,
    )
    .unwrap()
}

fn painted_point(frame: &UiFrame) -> (UiPoint, UiPoint) {
    let (_, draw, clip, painted) = text(frame);
    let glyph = painted
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
        painted.lines()[0].baseline(),
    );
    let transform = frame.environment().content_transform();
    let physical = transform
        .authored_to_physical(UiPoint::new(
            draw.origin().x() + point.x(),
            draw.origin().y() + point.y(),
        ))
        .unwrap();
    let presented = transform.physical_to_authored(physical).unwrap();
    (
        point,
        UiPoint::new(presented.x() - ORIGIN.x(), presented.y() - ORIGIN.y()),
    )
}

fn assert_owner(field: &UiTextField, reference: &TextEditor) {
    assert_eq!(field.editor().text(), reference.text());
    assert_eq!(field.editor().display_text(), reference.display_text());
    assert_eq!(field.editor().selection(), reference.selection());
    assert_eq!(field.editor().caret(), reference.caret());
    assert_eq!(
        field.editor().selection_rects(),
        reference.selection_rects()
    );
    assert_eq!(field.editor().ime_area(), reference.ime_area());
    assert_eq!(
        field.editor().composition_range(),
        reference.composition_range()
    );
    assert_eq!(field.editor().can_undo(), reference.can_undo());
    assert_eq!(field.editor().can_redo(), reference.can_redo());
    assert_eq!(field.editor().layout(), reference.layout());
}

fn assert_paint_geometry(frame: &UiFrame, field: &UiTextField, width: f32) {
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    let outer = frame
        .layouts()
        .iter()
        .find(|fact| fact.key() == Some(field.key()))
        .unwrap()
        .rect();
    let measured = field.layout_measurement(width).unwrap();
    same_rect(outer, UiRect::new(ORIGIN, measured.size()).unwrap());
    let padding = field.appearance().unwrap().padding;
    let viewport = UiRect::from_origin_size(
        ORIGIN.x() + padding.left(),
        ORIGIN.y() + padding.top(),
        width - padding.left() - padding.right(),
        measured.size().height() - padding.top() - padding.bottom(),
    )
    .unwrap();
    let (_, draw, clip, painted) = text(frame);
    close(
        draw.origin().x(),
        viewport.origin().x()
            - field
                .editor()
                .caret()
                .map_or(0.0, |caret| (caret.x1 - viewport.size().width()).max(0.0)),
    );
    close(
        draw.origin().y() + painted.lines()[0].baseline(),
        ORIGIN.y() + measured.first_baseline().unwrap(),
    );
    assert_eq!(painted.lines(), field.editor().layout().lines());
    let text_box = UiRect::from_origin_size(
        draw.origin().x(),
        viewport.origin().y(),
        field.editor().layout().width().max(1.0),
        viewport.size().height(),
    )
    .unwrap();
    same_rect(clip, viewport.intersect(text_box).unwrap());
    let expected = field
        .editor()
        .selection_rects()
        .iter()
        .filter_map(|selection| translated(*selection, draw.origin()).intersect(viewport))
        .collect::<Vec<_>>();
    let actual = rectangles(frame, SELECTION);
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.into_iter().zip(expected) {
        same_rect(actual, expected);
    }
    let caret = field
        .editor()
        .caret()
        .and_then(|caret| translated(caret, draw.origin()).intersect(viewport));
    let actual = rectangles(frame, CARET);
    assert_eq!(actual.len(), usize::from(caret.is_some()));
    if let Some(caret) = caret {
        same_rect(actual[0], caret);
    }
    let ime = field.ime_area(width).unwrap();
    same_rect(
        UiRect::new(
            UiPoint::new(ORIGIN.x() + ime.origin().x(), ORIGIN.y() + ime.origin().y()),
            ime.size(),
        )
        .unwrap(),
        translated(field.editor().ime_area(), draw.origin())
            .intersect(viewport)
            .unwrap(),
    );
    let semantic = frame
        .semantics()
        .iter()
        .find(|semantic| semantic.key() == field.key())
        .unwrap();
    assert_eq!(semantic.value(), Some(field.editor().text()));
    same_rect(semantic.rect(), outer);
}

#[test]
fn exact_allocation_overrides_intrinsic_preferences_available_hints_and_authored_widths() {
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
    let typography = TextTypography::new(20.0)
        .unwrap()
        .with_line_height(6.0)
        .unwrap();
    let original = format!("{}e\u{301} abc אבג end", "0123456789 ".repeat(12));
    let mut field =
        UiTextField::with_typography(root(), "Value", &original, typography, &mut engine).unwrap();
    field.set_appearance(appearance());
    let token = field.focus().unwrap();
    let key = field.key().clone();
    let preferred_width = 160.5;
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let mut revision = 1;
    let mut text_key = None;
    for authored_width in [None, Some(224.5)] {
        let style =
            UiStyle::flex().with_width(authored_width.map_or(UiDimension::Auto, UiDimension::Px));
        for width in [76.375, 318.625] {
            field
                .edit(token, &TextEdit::SelectAll, &mut engine)
                .unwrap();
            let source = field.editor().paint_snapshot();
            let captured = field
                .authored_node(&root(), style, preferred_width)
                .unwrap();
            assert_eq!(
                captured,
                field
                    .authored_node(&root(), style, preferred_width)
                    .unwrap()
            );
            let allocation = allocation(&field, preferred_width, width, authored_width);
            let first = compile(&mut compiler, &captured, &allocation, revision, 1.375);
            revision += 1;
            assert_paint_geometry(&first, &field, width);
            let (painted_key, draw, _, _) = text(&first);
            if let Some(key) = &text_key {
                assert_eq!(painted_key, key);
            } else {
                text_key = Some(painted_key.clone());
            }
            assert!(field.editor().layout().lines()[0].block_min() < 0.0);
            assert!(draw.origin().y() > ORIGIN.y() + appearance().padding.top());
            let warm = compiler.work_stats();
            let repeated = compile(&mut compiler, &captured, &allocation, revision, 1.375);
            revision += 1;
            assert_eq!(first.paint(), repeated.paint());
            assert_eq!(first.layouts(), repeated.layouts());
            assert_eq!(first.hit_regions(), repeated.hit_regions());
            assert_eq!(first.focus_order(), repeated.focus_order());
            assert_eq!(first.semantics(), repeated.semantics());
            assert_eq!(compiler.work_stats().text.layout_attempts, 0);
            assert!(compiler.work_stats().custom_layout_callbacks > warm.custom_layout_callbacks);
            assert_eq!(
                captured,
                field
                    .authored_node(&root(), style, preferred_width)
                    .unwrap()
            );

            let (point, presented) = painted_point(&first);
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
                .point(token, presented, width, false, &mut engine)
                .unwrap();
            assert_owner(&field, &reference);
            assert_eq!(field.key(), &key);
            assert_eq!(field.input_token(), Some(token));
            assert!(source.same_source(&field.editor().paint_snapshot()));
            let old = compile(&mut compiler, &captured, &allocation, revision, 1.375);
            revision += 1;
            assert_eq!(old.paint(), first.paint());
        }
    }
}

#[test]
fn complete_4096_byte_mixed_font_preedit_survives_repeated_probes_and_final_resize() {
    let primary = bundled_font_face().unwrap();
    let cjk = FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/NotoSansSC-subset.otf").to_vec(),
        0,
    )
    .unwrap();
    let fonts = vec![primary.clone(), cjk.clone()];
    let mut engine = TextEngine::with_fonts(
        fonts.clone(),
        TextLimits {
            max_text_bytes: 4096,
            ..TextLimits::default()
        },
    )
    .unwrap();
    let seed = "0123456789 e\u{301} אבג 中 ";
    let mut original = seed.repeat(4096 / seed.len());
    original.push_str(&"x".repeat(4096 - original.len()));
    let display = format!("Z{}", &original[1..]);
    assert_eq!(original.len(), 4096);
    assert_eq!(display.len(), 4096);
    let mut field = UiTextField::new(root(), "Full value", "initial", 20.0, &mut engine).unwrap();
    field.set_appearance(appearance());
    let token = field.focus().unwrap();
    for action in [
        TextEdit::SelectAll,
        TextEdit::Commit(original.clone()),
        TextEdit::SelectAll,
        TextEdit::Commit("replacement".into()),
        TextEdit::Undo,
        TextEdit::Preedit {
            text: display.clone(),
            cursor: Some((4096, 4096)),
        },
    ] {
        field.edit(token, &action, &mut engine).unwrap();
    }
    assert_eq!(field.editor().text(), original);
    assert_eq!(field.editor().display_text(), display);
    assert!(field.editor().is_composing());
    assert!(field.editor().can_undo() && field.editor().can_redo());
    assert!(
        field
            .editor()
            .layout()
            .glyphs()
            .iter()
            .any(|glyph| glyph.key().face_id() == cjk.id())
    );
    assert!(
        field
            .editor()
            .layout()
            .runs()
            .iter()
            .any(|run| run.direction() == TextDirection::RightToLeft)
    );
    let source = field.editor().paint_snapshot();
    let mut reference = field.editor().clone();
    let captured = field
        .authored_node(&root(), UiStyle::flex(), 160.5)
        .unwrap();
    let mut compiler = UiCompiler::with_fonts(fonts, Default::default()).unwrap();
    let mut text_key = None;
    for (index, width) in [55.75, 318.625, 55.75].into_iter().enumerate() {
        let allocation = allocation(&field, 160.5, width, None);
        let frame = compile(
            &mut compiler,
            &captured,
            &allocation,
            index as u64 + 1,
            1.25,
        );
        assert_paint_geometry(&frame, &field, width);
        let (key, _, _, painted) = text(&frame);
        if let Some(previous) = &text_key {
            assert_eq!(key, previous);
        } else {
            text_key = Some(key.clone());
        }
        assert!(painted.glyphs().len() < field.editor().layout().glyphs().len());
        assert_owner(&field, &reference);
        assert!(source.same_source(&field.editor().paint_snapshot()));
        assert_eq!(field.input_token(), Some(token));
        assert_eq!(
            captured,
            field
                .authored_node(&root(), UiStyle::flex(), 160.5)
                .unwrap()
        );
        assert_eq!(compiler.work_stats().text.layout_attempts, 0);
    }
    let allocation = allocation(&field, 160.5, 318.625, None);
    let mut missing_font = UiCompiler::new(primary, Default::default()).unwrap();
    let refused = compile(&mut missing_font, &captured, &allocation, 1, 1.25);
    assert_eq!(refused.root_outcomes()[0].status(), UiRootStatus::Absent);
    assert!(matches!(
        refused.root_outcomes()[0].diagnostic(),
        Some(UiRootDiagnostic::TextRejected { .. })
    ));
    assert_owner(&field, &reference);
    assert!(source.same_source(&field.editor().paint_snapshot()));
    let presented = compile(&mut compiler, &captured, &allocation, 4, 1.25);
    let (point, presented) = painted_point(&presented);
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
        .point(token, presented, 318.625, false, &mut engine)
        .unwrap();
    assert_owner(&field, &reference);
    assert!(!field.editor().is_composing());
    assert_eq!(field.input_token(), Some(token));
    for action in [TextEdit::Undo, TextEdit::Redo, TextEdit::Redo] {
        assert_eq!(
            field.edit(token, &action, &mut engine).unwrap(),
            reference.edit(&mut engine, &action).unwrap(),
        );
        assert_owner(&field, &reference);
    }
    assert_eq!(field.editor().text(), "replacement");
}

#[test]
fn too_narrow_exact_allocation_refuses_the_root_retains_inert_visuals_and_recovers() {
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
    let mut field = UiTextField::new(root(), "Draft", "committed", 20.0, &mut engine).unwrap();
    field.set_appearance(appearance());
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::SelectAll, &mut engine)
        .unwrap();
    field
        .edit(
            token,
            &TextEdit::Preedit {
                text: "e\u{301} abc אבג draft".into(),
                cursor: None,
            },
            &mut engine,
        )
        .unwrap();
    let source = field.editor().paint_snapshot();
    let reference = field.editor().clone();
    let captured = field
        .authored_node(&root(), UiStyle::flex(), 160.5)
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let padding = appearance().padding;
    let too_narrow = allocation(&field, 160.5, padding.left() + padding.right(), None);
    let absent = compile(&mut compiler, &captured, &too_narrow, 1, 1.375);
    assert_eq!(absent.root_outcomes()[0].status(), UiRootStatus::Absent);
    assert!(matches!(
        absent.root_outcomes()[0].diagnostic(),
        Some(UiRootDiagnostic::LayoutRejected { .. })
    ));
    assert!(absent.paint().is_empty());
    assert_owner(&field, &reference);
    assert!(source.same_source(&field.editor().paint_snapshot()));
    let valid = allocation(&field, 160.5, 76.375, None);
    let first = compile(&mut compiler, &captured, &valid, 2, 1.375);
    assert_paint_geometry(&first, &field, 76.375);
    let retained = compile(&mut compiler, &captured, &too_narrow, 3, 1.375);
    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert!(matches!(
        retained.root_outcomes()[0].diagnostic(),
        Some(UiRootDiagnostic::LayoutRejected { .. })
    ));
    assert_eq!(retained.paint(), first.paint());
    assert_eq!(retained.layouts(), first.layouts());
    assert!(retained.hit_regions().is_empty());
    assert!(retained.focus_order().is_empty());
    assert!(retained.semantics().is_empty());
    assert_owner(&field, &reference);
    assert!(source.same_source(&field.editor().paint_snapshot()));
    assert_eq!(field.input_token(), Some(token));
    assert_eq!(
        captured,
        field
            .authored_node(&root(), UiStyle::flex(), 160.5)
            .unwrap()
    );
    let resized = allocation(&field, 160.5, 318.625, None);
    let recovered = compile(&mut compiler, &captured, &resized, 4, 1.375);
    assert_paint_geometry(&recovered, &field, 318.625);
    assert_eq!(
        recovered.root_outcomes()[0].status(),
        UiRootStatus::Published
    );
    assert!(!recovered.hit_regions().is_empty());
    assert!(!recovered.focus_order().is_empty());
    assert_owner(&field, &reference);
    assert!(source.same_source(&field.editor().paint_snapshot()));
    assert_eq!(field.input_token(), Some(token));
}
