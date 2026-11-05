use std::sync::Arc;

use nara_text::{
    FontFace, TextEditor, TextEngine, TextLayout, TextLimits, TextTypography, bundled_font_face,
};
use nara_ui_kernel::{
    UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiBrush, UiColor, UiColorRole, UiCompileError,
    UiCompiler, UiCustomLayout, UiEnvironment, UiFrame, UiLayer, UiLayout, UiLayoutChildren,
    UiLayoutConstraints, UiLayoutError, UiLayoutOutput, UiLimits, UiLocalKey, UiNode,
    UiPaintPrimitive, UiPreparedText, UiRoot, UiRootKey, UiRootStatus, UiSize, UiStyle,
    UiSurfaceId, UiText, UiTextAlignment, UiTextOverflow, UiTheme, UiVisual,
};

fn environment(width: f32, scale: f32, theme: UiTheme) -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(704).unwrap(),
        width,
        480.0,
        scale,
        1,
        1,
        theme.revision().get(),
        1,
    )
    .unwrap()
}

fn root(name: &str) -> UiRoot {
    UiRoot::new(
        UiRootKey::runtime("text-reuse", name).unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
}

fn text(content: &str) -> UiText {
    UiText::new(content, 14.0)
        .unwrap()
        .with_overflow(UiTextOverflow::Clip)
}

fn snapshot(limits: UiLimits, roots: impl IntoIterator<Item = UiRoot>) -> UiAuthoringSnapshot {
    roots
        .into_iter()
        .fold(
            UiAuthoringSnapshotBuilder::new(1).with_limits(limits),
            |builder, root| builder.with_root(root),
        )
        .build()
        .unwrap()
}

fn compile(compiler: &mut UiCompiler, source: &UiAuthoringSnapshot) -> Arc<UiFrame> {
    let frame = compiler
        .compile(
            source,
            environment(320.0, 1.0, UiTheme::default()),
            UiTheme::default(),
        )
        .unwrap();
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    assert!(
        frame
            .root_outcomes()
            .iter()
            .all(|outcome| outcome.status() == UiRootStatus::Published)
    );
    frame
}

fn painted_text(frame: &UiFrame) -> &Arc<TextLayout> {
    frame
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Text { layout, .. } => Some(layout),
            _ => None,
        })
        .unwrap()
}

#[test]
fn warm_palette_and_hover_colors_reuse_layout_while_publishing_new_paint() {
    let limits = UiLimits::default();
    let authored = |hovered| {
        let color = if hovered {
            UiColor::rgba(0.18, 0.24, 0.32, 1.0)
        } else {
            UiColor::rgba(0.06, 0.08, 0.12, 1.0)
        };
        snapshot(
            limits,
            [root("control")
                .with_visual(UiVisual::solid(color))
                .with_text(
                    text("Apply complete value")
                        .with_brush(UiBrush::Role(UiColorRole::PrimaryText)),
                )],
        )
    };
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap();
    let first = compile(&mut compiler, &authored(false));
    let warm = compiler.work_stats();
    assert!(warm.text.layout_attempts > 0);
    let mut palette = UiTheme::default().palette();
    palette.primary_text = UiColor::rgba(0.7, 0.8, 0.9, 1.0);
    let theme = UiTheme::new(2, palette).unwrap();

    for hovered in [false, true, false] {
        let frame = compiler
            .compile(&authored(hovered), environment(320.0, 1.0, theme), theme)
            .unwrap();
        assert!(frame.diagnostics().is_empty());
        assert_eq!(compiler.work_stats().text, warm.text);
        assert!(compiler.work_stats().layout_passes > warm.layout_passes);
        assert!(Arc::ptr_eq(painted_text(&first), painted_text(&frame)));
        assert_eq!(frame.layouts(), first.layouts());
        assert_eq!(frame.semantics(), first.semantics());
        assert_ne!(frame.paint(), first.paint());
        assert!(frame.paint().iter().any(|paint| matches!(
            paint,
            UiPaintPrimitive::Text { color, .. } if *color == palette.primary_text
        )));
    }
    let restored = compile(&mut compiler, &authored(false));
    assert_eq!(restored.paint(), first.paint());
    assert_eq!(compiler.work_stats().text, warm.text);
}

#[test]
fn typography_wrapping_alignment_and_target_scale_are_complete_request_dependencies() {
    let limits = UiLimits::default();
    let theme = UiTheme::default();
    let body = TextTypography::new(14.0)
        .unwrap()
        .with_line_height(20.0)
        .unwrap();
    let larger = TextTypography::new(18.0)
        .unwrap()
        .with_line_height(26.0)
        .unwrap();
    let content = "A complete wrapped value with several words and combining e\u{301}";
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap();
    for (typography, width, scale, alignment, overflow) in [
        (
            body,
            240.0,
            1.0,
            UiTextAlignment::Start,
            UiTextOverflow::Wrap,
        ),
        (
            larger,
            240.0,
            1.0,
            UiTextAlignment::Start,
            UiTextOverflow::Wrap,
        ),
        (
            body.with_line_height(30.0).unwrap(),
            240.0,
            1.0,
            UiTextAlignment::Start,
            UiTextOverflow::Wrap,
        ),
        (
            body.with_auto_line_height(),
            240.0,
            1.0,
            UiTextAlignment::Start,
            UiTextOverflow::Wrap,
        ),
        (
            body,
            120.0,
            1.0,
            UiTextAlignment::Start,
            UiTextOverflow::Wrap,
        ),
        (
            body,
            240.0,
            1.5,
            UiTextAlignment::Start,
            UiTextOverflow::Wrap,
        ),
        (body, 240.0, 1.0, UiTextAlignment::End, UiTextOverflow::Wrap),
        (
            body,
            240.0,
            1.0,
            UiTextAlignment::Start,
            UiTextOverflow::Clip,
        ),
    ] {
        let source = snapshot(
            limits,
            [root("value").with_text(
                UiText::from_typography(content, typography)
                    .with_alignment(alignment)
                    .with_overflow(overflow),
            )],
        );
        let environment = environment(width, scale, theme);
        let before = compiler.work_stats();
        let first = compiler.compile(&source, environment, theme).unwrap();
        let shaped = compiler.work_stats();
        assert!(first.diagnostics().is_empty());
        assert!(shaped.text.layout_attempts > before.text.layout_attempts);
        let second = compiler.compile(&source, environment, theme).unwrap();
        assert_eq!(compiler.work_stats().text, shaped.text);
        assert_eq!(second.paint(), first.paint());
        assert_eq!(painted_text(&second).typography(), typography);
        assert!(
            painted_text(&second)
                .glyphs()
                .iter()
                .all(|glyph| { glyph.key().physical_size() == typography.logical_size() * scale })
        );
        let mut fresh = UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap();
        let rebuilt = fresh.compile(&source, environment, theme).unwrap();
        assert_eq!(rebuilt.layouts(), second.layouts());
        assert_eq!(rebuilt.paint(), second.paint());
        assert_eq!(rebuilt.semantics(), second.semantics());
    }
}

#[test]
fn reuse_stays_with_the_original_ordered_fonts_and_immutable_engine_policy() {
    let sans = bundled_font_face().unwrap();
    let mono = FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let source = snapshot(UiLimits::default(), [root("fonts").with_text(text("M\nW"))]);
    for fonts in [
        vec![sans.clone(), mono.clone()],
        vec![mono.clone(), sans.clone()],
    ] {
        let expected = fonts[0].id();
        let mut compiler = UiCompiler::with_fonts(fonts, UiLimits::default()).unwrap();
        let first = compile(&mut compiler, &source);
        let warm = compiler.work_stats();
        assert!(warm.text.layout_attempts > 0);
        assert!(
            painted_text(&first)
                .glyphs()
                .iter()
                .all(|glyph| { glyph.key().face_id() == expected })
        );
        let second = compile(&mut compiler, &source);
        assert_eq!(compiler.work_stats().text, warm.text);
        assert!(Arc::ptr_eq(painted_text(&first), painted_text(&second)));
    }
    let mut limited = UiCompiler::with_fonts_and_text_limits(
        vec![sans, mono],
        UiLimits::default(),
        TextLimits {
            max_lines: 1,
            ..TextLimits::default()
        },
    )
    .unwrap();
    let refused = limited
        .compile(
            &source,
            environment(320.0, 1.0, UiTheme::default()),
            UiTheme::default(),
        )
        .unwrap();
    assert_eq!(refused.root_outcomes()[0].status(), UiRootStatus::Absent);
    assert!(!refused.diagnostics().is_empty());
    assert!(limited.work_stats().text.layout_attempts > 0);
}

#[test]
fn warm_layouts_still_charge_complete_candidate_source_with_prepared_and_ordinary_text() {
    let limits = UiLimits::default();
    let policy = TextLimits {
        max_text_bytes: 3,
        ..TextLimits::default()
    };
    let mut compiler =
        UiCompiler::with_text_limits(bundled_font_face().unwrap(), limits, policy).unwrap();
    let source = snapshot(limits, [root("one").with_text(text("AA"))]);
    let first = compile(&mut compiler, &source);
    let warm = compiler.work_stats();
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
    let prepared = TextEditor::new(&mut engine, "BB", 14.0)
        .unwrap()
        .paint_snapshot();
    let prepared_node = UiNode::new(UiStyle::absolute(0.0, 0.0, 1.0, 32.0))
        .with_key(UiLocalKey::runtime("text-reuse", "prepared").unwrap())
        .with_prepared_text(UiPreparedText::new(prepared));
    for candidate in [
        snapshot(
            limits,
            [
                root("one").with_text(text("AA")),
                root("two").with_text(text("BB")),
            ],
        ),
        snapshot(
            limits,
            [
                root("prepared").with_child(prepared_node),
                root("one").with_text(text("AA")),
            ],
        ),
    ] {
        let mut cold =
            UiCompiler::with_text_limits(bundled_font_face().unwrap(), limits, policy).unwrap();
        assert!(matches!(
            cold.compile(
                &candidate,
                environment(320.0, 1.0, UiTheme::default()),
                UiTheme::default(),
            ),
            Err(UiCompileError::TextWorkLimitExceeded {
                actual: 4,
                maximum: 3
            })
        ));
        let attempts = compiler.work_stats().text.layout_attempts;
        assert!(matches!(
            compiler.compile(
                &candidate,
                environment(320.0, 1.0, UiTheme::default()),
                UiTheme::default(),
            ),
            Err(UiCompileError::TextWorkLimitExceeded {
                actual: 4,
                maximum: 3
            })
        ));
        assert_eq!(compiler.work_stats().text.layout_attempts, attempts);
        assert_eq!(
            compiler.work_stats().published_frames,
            warm.published_frames
        );
    }
    let recovered = compile(&mut compiler, &source);
    assert_eq!(recovered.revision().get(), first.revision().get() + 1);
    assert_eq!(recovered.paint(), first.paint());
}

struct MeasureAndHide;

impl UiLayout for MeasureAndHide {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        for index in 0..children.len() {
            children.measure(index, UiLayoutConstraints::MAX_CONTENT)?;
            children.hide(index)?;
        }
        Ok(UiLayoutOutput::new(UiSize::new(0.0, 0.0).unwrap()))
    }
}

#[test]
fn warm_measurements_charge_glyphs_and_remaining_budget_failures_do_not_poison_recovery() {
    let limits = UiLimits::default();
    let policy = TextLimits {
        max_glyphs: 5,
        ..TextLimits::default()
    };
    let mut compiler =
        UiCompiler::with_text_limits(bundled_font_face().unwrap(), limits, policy).unwrap();
    compile(
        &mut compiler,
        &snapshot(limits, [root("warm").with_text(text("AAAA"))]),
    );
    let children = ["AAAA", "BB"]
        .into_iter()
        .enumerate()
        .map(|(index, content)| {
            UiNode::new(UiStyle::flex())
                .with_key(UiLocalKey::runtime("text-reuse", index.to_string()).unwrap())
                .with_text(text(content))
        });
    let candidate = snapshot(
        limits,
        [root("measured")
            .with_custom_layout(UiCustomLayout::new(MeasureAndHide))
            .with_children(children)],
    );
    let attempts = compiler.work_stats().text.layout_attempts;
    assert!(matches!(
        compiler.compile(
            &candidate,
            environment(320.0, 1.0, UiTheme::default()),
            UiTheme::default(),
        ),
        Err(UiCompileError::ShapedGlyphLimitExceeded {
            actual: 6,
            maximum: 5
        })
    ));
    assert_eq!(compiler.work_stats().text.layout_attempts, attempts + 1);
    let before_recovery = compiler.work_stats().text.layout_attempts;
    let recovery = snapshot(limits, [root("recovered").with_text(text("BB"))]);
    let recovered = compile(&mut compiler, &recovery);
    assert_eq!(painted_text(&recovered).glyphs().len(), 2);
    assert!(compiler.work_stats().text.layout_attempts > before_recovery);
    let warm = compiler.work_stats().text;
    compile(&mut compiler, &recovery);
    assert_eq!(compiler.work_stats().text, warm);
}

#[test]
fn every_warm_draw_still_pays_output_glyph_and_paint_budgets() {
    let theme = UiTheme::default();
    for (limits, policy, glyph_budget) in [
        (
            UiLimits::default(),
            TextLimits {
                max_glyphs: 5,
                ..TextLimits::default()
            },
            true,
        ),
        (
            UiLimits::default().with_max_paint_primitives(1).unwrap(),
            TextLimits::default(),
            false,
        ),
    ] {
        let mut compiler =
            UiCompiler::with_text_limits(bundled_font_face().unwrap(), limits, policy).unwrap();
        let source = snapshot(limits, [root("one").with_text(text("AAAA"))]);
        let first = compile(&mut compiler, &source);
        let warm = compiler.work_stats();
        let repeated = snapshot(
            limits,
            [
                root("one").with_text(text("AAAA")),
                root("two").with_text(text("AAAA")),
            ],
        );
        let error = compiler
            .compile(&repeated, environment(320.0, 1.0, theme), theme)
            .unwrap_err();
        if glyph_budget {
            assert!(matches!(
                error,
                UiCompileError::ShapedGlyphLimitExceeded {
                    actual: 8,
                    maximum: 5
                }
            ));
        } else {
            assert!(matches!(
                error,
                UiCompileError::LogicalPaintLimitExceeded {
                    actual: 2,
                    maximum: 1
                }
            ));
        }
        assert_eq!(compiler.work_stats().text, warm.text);
        let recovered = compile(&mut compiler, &source);
        assert_eq!(recovered.revision().get(), first.revision().get() + 1);
        assert_eq!(recovered.paint(), first.paint());
    }
}

#[test]
fn entry_source_byte_and_glyph_limits_each_evict_and_rebuild_equivalent_output() {
    let body = TextTypography::new(14.0).unwrap();
    for (limits, policy, first_text, second_text, second_typography) in [
        (
            UiLimits::default().with_max_nodes(1).unwrap(),
            TextLimits::default(),
            "",
            "",
            TextTypography::new(16.0).unwrap(),
        ),
        (
            UiLimits::default(),
            TextLimits {
                max_text_bytes: 3,
                ..TextLimits::default()
            },
            "AA",
            "BB",
            body,
        ),
        (
            UiLimits::default(),
            TextLimits {
                max_glyphs: 3,
                ..TextLimits::default()
            },
            "AA",
            "BB",
            body,
        ),
    ] {
        let mut compiler =
            UiCompiler::with_text_limits(bundled_font_face().unwrap(), limits, policy).unwrap();
        let source = snapshot(limits, [root("value").with_text(text(first_text))]);
        let first = compile(&mut compiler, &source);
        let replacement = snapshot(
            limits,
            [root("value").with_text(text(second_text).with_typography(second_typography))],
        );
        compile(&mut compiler, &replacement);
        let before = compiler.work_stats().text.layout_attempts;
        let rebuilt = compile(&mut compiler, &source);
        assert!(compiler.work_stats().text.layout_attempts > before);
        assert_eq!(rebuilt.layouts(), first.layouts());
        assert_eq!(rebuilt.paint(), first.paint());
        assert_eq!(rebuilt.semantics(), first.semantics());
        let warm = compiler.work_stats().text;
        compile(&mut compiler, &source);
        assert_eq!(compiler.work_stats().text, warm);
    }
}
