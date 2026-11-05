use std::sync::Arc;

use nara_text::{TextEngine, TextLayout, TextLayoutRequest, TextTypography, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiAvailableSpace, UiColor, UiCompileError,
    UiCompiler, UiCustomLayout, UiDimension, UiEdges, UiEnvironment, UiFrame, UiKey, UiLayer,
    UiLayout, UiLayoutChildren, UiLayoutConstraints, UiLayoutError, UiLayoutMeasurement,
    UiLayoutOutput, UiLimits, UiLocalKey, UiNode, UiPaintPrimitive, UiRect, UiRoot,
    UiRootDiagnostic, UiRootKey, UiRootStatus, UiSize, UiStyle, UiSurfaceId, UiText, UiTheme,
    UiVisual,
};

const WORDS: &str = "One two three four five six seven eight nine ten.";

fn root() -> UiRootKey {
    UiRootKey::runtime("allocated-measurement", "kernel").unwrap()
}

fn label_key() -> UiLocalKey {
    UiLocalKey::runtime("allocated-measurement", "label").unwrap()
}

fn environment() -> UiEnvironment {
    UiEnvironment::new(UiSurfaceId::new(97).unwrap(), 320.0, 480.0, 1.0, 1, 1, 1, 1).unwrap()
}

fn source(layout: impl UiLayout, child: UiNode) -> UiAuthoringSnapshot {
    UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root(), UiLayer::Base, UiStyle::fill())
                .with_custom_layout(UiCustomLayout::new(layout))
                .with_child(child),
        )
        .build()
        .unwrap()
}

fn compiler(limits: UiLimits) -> UiCompiler {
    UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap()
}

fn compile(compiler: &mut UiCompiler, source: &UiAuthoringSnapshot) -> Arc<UiFrame> {
    compiler
        .compile(source, environment(), UiTheme::default())
        .unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

fn typography() -> TextTypography {
    TextTypography::new(14.0)
        .unwrap()
        .with_line_height(24.0)
        .unwrap()
}

fn oracle(width: f32) -> TextLayout {
    TextEngine::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .layout(
            &TextLayoutRequest::from_typography(WORDS, typography())
                .with_wrap_width(width)
                .unwrap(),
        )
        .unwrap()
}

fn same_measurement(actual: UiLayoutMeasurement, expected: UiLayoutMeasurement) {
    close(actual.size().width(), expected.size().width());
    close(actual.size().height(), expected.size().height());
    close(
        actual.first_baseline().unwrap(),
        expected.first_baseline().unwrap(),
    );
}

struct CompareWidths {
    narrow: UiLayoutMeasurement,
    wide: UiLayoutMeasurement,
    hinted_width: f32,
}

impl UiLayout for CompareWidths {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        let hinted = children.measure(
            0,
            UiLayoutConstraints {
                width: UiAvailableSpace::Definite(self.narrow.size().width()),
                height: UiAvailableSpace::MaxContent,
            },
        )?;
        close(hinted.size().width(), self.hinted_width);
        for expected in [self.narrow, self.wide, self.narrow, self.wide] {
            same_measurement(
                children.measure_at_width(0, expected.size().width())?,
                expected,
            );
        }
        children.place(
            0,
            UiRect::from_origin_size(
                12.0,
                10.0,
                self.narrow.size().width(),
                self.narrow.size().height(),
            )
            .unwrap(),
        )?;
        UiLayoutOutput::new(UiSize::new(320.0, 480.0).unwrap())
            .with_first_baseline(10.0 + self.narrow.first_baseline().unwrap())
    }
}

#[test]
fn exact_width_overrides_authored_width_and_repeated_probes_preserve_final_text_geometry() {
    let padding = UiEdges::new(5.0, 7.0, 3.0, 9.0).unwrap();
    let narrow_text = oracle(88.0);
    let wide_text = oracle(228.0);
    assert!(narrow_text.lines().len() > wide_text.lines().len());
    for (width, hinted_width) in [
        (UiDimension::Px(400.0), 400.0),
        (UiDimension::Percent(0.5), 50.0),
    ] {
        for height in [UiDimension::Auto, UiDimension::Px(160.0)] {
            let expected = |width, text: &TextLayout| {
                UiLayoutMeasurement::new(
                    UiSize::new(
                        width,
                        match height {
                            UiDimension::Px(height) => height,
                            _ => text.height() + padding.top() + padding.bottom(),
                        },
                    )
                    .unwrap(),
                    Some(padding.top() + text.lines()[0].baseline()),
                )
                .unwrap()
            };
            let narrow = expected(100.0, &narrow_text);
            let source = source(
                CompareWidths {
                    narrow,
                    wide: expected(240.0, &wide_text),
                    hinted_width,
                },
                UiNode::new(
                    UiStyle::flex()
                        .with_width(width)
                        .with_height(height)
                        .with_padding(padding),
                )
                .with_key(label_key())
                .with_text(UiText::from_typography(WORDS, typography())),
            );
            let frame = compile(&mut compiler(UiLimits::default()), &source);
            assert_eq!(
                frame.root_outcomes()[0].status(),
                UiRootStatus::Published,
                "{:?}",
                frame.diagnostics()
            );
            let key = UiKey::scoped(root(), label_key()).unwrap();
            let outer = frame
                .layouts()
                .iter()
                .find(|fact| fact.key() == Some(&key))
                .unwrap()
                .rect();
            close(outer.origin().x(), 12.0);
            close(outer.origin().y(), 10.0);
            assert_eq!(outer.size(), narrow.size());
            let (draw, painted) = frame
                .paint()
                .iter()
                .find_map(|paint| match paint {
                    UiPaintPrimitive::Text {
                        key: Some(painted),
                        rect,
                        layout,
                        ..
                    } if painted == &key => Some((*rect, layout)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(painted.lines(), narrow_text.lines());
            close(draw.size().width(), 88.0);
            close(
                draw.origin().y() + painted.lines()[0].baseline(),
                10.0 + narrow.first_baseline().unwrap(),
            );
            assert!(draw.origin().y() + painted.height() <= outer.max().y() + 0.001);
        }
    }
}

struct SwallowInvalidWidth {
    width: f32,
    after_placement: bool,
    expected: UiLayoutError,
}

impl UiLayout for SwallowInvalidWidth {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if self.after_placement {
            children.place(0, UiRect::from_origin_size(0.0, 0.0, 64.0, 32.0).unwrap())?;
        }
        if children.measure_at_width(0, self.width) != Err(self.expected.clone()) {
            return Err(UiLayoutError::Rejected {
                message: "unexpected allocated-width error".into(),
            });
        }
        let _ = children.hide(0);
        Ok(UiLayoutOutput::new(UiSize::new(100.0, 100.0).unwrap()))
    }
}

struct RepeatedWidth {
    width: f32,
    repeats: usize,
}

impl UiLayout for RepeatedWidth {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        let measured = children.measure_at_width(0, self.width)?;
        for _ in 1..self.repeats {
            assert_eq!(children.measure_at_width(0, self.width)?, measured);
        }
        children.place(
            0,
            UiRect::from_origin_size(0.0, 0.0, self.width, measured.size().height()).unwrap(),
        )?;
        Ok(UiLayoutOutput::new(measured.size()))
    }
}

#[test]
fn invalid_allocated_widths_and_post_placement_probes_keep_the_original_root_poison() {
    let child = UiNode::new(UiStyle::flex().with_padding(UiEdges::all(8.0).unwrap()))
        .with_visual(UiVisual::solid(UiColor::rgba(0.2, 0.4, 0.6, 1.0)));
    for (width, after_placement, expected) in [
        (-1.0, false, UiLayoutError::InvalidConstraints),
        (f32::NAN, false, UiLayoutError::InvalidConstraints),
        (f32::INFINITY, false, UiLayoutError::InvalidConstraints),
        (
            15.0,
            false,
            UiLayoutError::PlacementSizeMismatch { child: 0 },
        ),
        (64.0, true, UiLayoutError::DuplicatePlacement { child: 0 }),
    ] {
        let source = source(
            SwallowInvalidWidth {
                width,
                after_placement,
                expected: expected.clone(),
            },
            child.clone(),
        );
        let frame = compile(&mut compiler(UiLimits::default()), &source);
        assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Absent);
        assert!(
            matches!(frame.root_outcomes()[0].diagnostic(),
                Some(UiRootDiagnostic::LayoutRejected { message }) if message.contains(&expected.to_string())
            ),
            "{:?}",
            frame.diagnostics()
        );
        assert!(frame.paint().is_empty());
    }
    let boundary = source(
        RepeatedWidth {
            width: 16.0,
            repeats: 2,
        },
        child,
    );
    let accepted = compile(&mut compiler(UiLimits::default()), &boundary);
    assert_eq!(
        accepted.root_outcomes()[0].status(),
        UiRootStatus::Published,
        "{:?}",
        accepted.diagnostics()
    );
}

#[test]
fn repeated_allocated_width_cache_hits_consume_the_existing_candidate_budget() {
    let child = UiNode::new(UiStyle::flex())
        .with_key(label_key())
        .with_text(UiText::new("Budget", 14.0).unwrap());
    let ordinary = source(
        RepeatedWidth {
            width: 64.0,
            repeats: 1,
        },
        child.clone(),
    );
    let mut reference = compiler(UiLimits::default());
    compile(&mut reference, &ordinary);
    let maximum = reference.work_stats().layout_work_units as usize + 8;
    let mut restricted = compiler(UiLimits::default().with_max_layout_work(maximum).unwrap());
    let first = compile(&mut restricted, &ordinary);
    let before = restricted.work_stats();
    let repeated = source(
        RepeatedWidth {
            width: 64.0,
            repeats: 128,
        },
        child,
    );
    assert!(
        matches!(restricted.compile(&repeated, environment(), UiTheme::default()),
            Err(UiCompileError::LayoutWorkLimitExceeded { actual, maximum: limit })
                if actual == maximum + 1 && limit == maximum
        )
    );
    let refused = restricted.work_stats();
    assert!(refused.custom_child_measurements > before.custom_child_measurements + 1);
    assert_eq!(refused.text.layout_attempts, before.text.layout_attempts);
    assert_eq!(refused.published_frames, before.published_frames);
    let recovered = compile(&mut restricted, &ordinary);
    assert_eq!(
        recovered.root_outcomes()[0].status(),
        UiRootStatus::Published
    );
    assert_eq!(recovered.paint(), first.paint());
    assert_eq!(recovered.revision().get(), first.revision().get() + 1);
}
