use nara_text::{
    FontFace, TextEditor, TextEngine, TextLimits, TextPaintSnapshot, bundled_font_face,
};
use nara_ui_kernel::{
    UiAlign, UiAuthoringError, UiAuthoringSnapshotBuilder, UiCompileError, UiCompiler, UiDimension,
    UiEdges, UiEnvironment, UiLayer, UiLimits, UiLocalKey, UiNode, UiOverflow, UiPaintPrimitive,
    UiPreparedText, UiRoot, UiRootKey, UiRootStatus, UiStyle, UiSurfaceId, UiText, UiTextOverflow,
    UiTheme,
};

fn environment() -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(91).unwrap(),
        320.0,
        180.0,
        1.25,
        1,
        1,
        1,
        1,
    )
    .unwrap()
}

fn source(text: &str) -> TextPaintSnapshot {
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
    TextEditor::new(&mut engine, text, 16.0)
        .unwrap()
        .paint_snapshot()
}

fn node(name: &str, snapshot: &TextPaintSnapshot, width: f32) -> UiNode {
    UiNode::new(UiStyle::absolute(0.0, 0.0, width, 32.0))
        .with_key(UiLocalKey::runtime("prepared-test", name).unwrap())
        .with_prepared_text(UiPreparedText::new(snapshot.clone()))
}

fn root(name: &str, nodes: impl IntoIterator<Item = UiNode>) -> UiRoot {
    UiRoot::new(
        UiRootKey::runtime("prepared-test", name).unwrap(),
        UiLayer::Base,
        UiStyle::fill().with_overflow(UiOverflow::Clip),
    )
    .with_children(nodes)
}

fn compiler(limits: TextLimits) -> UiCompiler {
    UiCompiler::with_text_limits(bundled_font_face().unwrap(), UiLimits::default(), limits).unwrap()
}

#[test]
fn prepared_measurement_uses_complete_metrics_and_paint_uses_final_inherited_geometry() {
    let snapshot = source(&"MW".repeat(24));
    let text_key = UiLocalKey::runtime("prepared-test", "measured").unwrap();
    let authored = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("prepared-test", "measure").unwrap(),
                UiLayer::Base,
                UiStyle::fill().with_align_items(UiAlign::Start),
            )
            .with_child(
                UiNode::new(
                    UiStyle::flex()
                        .with_flex_shrink(0.0)
                        .with_padding(UiEdges::all(3.0).unwrap()),
                )
                .with_key(text_key.clone())
                .with_prepared_text(UiPreparedText::new(snapshot.clone())),
            ),
        )
        .build()
        .unwrap();
    let mut compiler = compiler(TextLimits::default());
    let measured = compiler
        .compile(&authored, environment(), UiTheme::default())
        .unwrap();
    assert!(measured.diagnostics().is_empty());
    let outer = measured
        .layouts()
        .iter()
        .find(|layout| {
            layout
                .key()
                .is_some_and(|key| key.local_key() == Some(&text_key))
        })
        .unwrap()
        .rect();
    assert!((outer.size().width() - snapshot.width() - 6.0).abs() < 0.001);
    assert!((outer.size().height() - snapshot.height() - 6.0).abs() < 0.001);

    let authored = UiAuthoringSnapshotBuilder::new(2)
        .with_root(root(
            "clip",
            [
                UiNode::new(
                    UiStyle::absolute(20.25, 10.5, 38.0, 32.0).with_overflow(UiOverflow::Clip),
                )
                .with_child(
                    UiNode::new(UiStyle::absolute(-40.0, 0.0, snapshot.width(), 32.0))
                        .with_key(UiLocalKey::runtime("prepared-test", "window").unwrap())
                        .with_prepared_text(UiPreparedText::new(snapshot.clone())),
                ),
                UiNode::new(UiStyle::absolute(80.0, 10.0, 100.0, 32.0))
                    .with_key(UiLocalKey::runtime("prepared-test", "chrome").unwrap())
                    .with_text(
                        UiText::new("Name", 16.0)
                            .unwrap()
                            .with_overflow(UiTextOverflow::Clip),
                    ),
            ],
        ))
        .build()
        .unwrap();
    // The full snapshot itself fits this receiver. Treating reuse as fresh full shaping would
    // leave no allowance for the real chrome, even though the window output is small.
    let mut compiler = compiler_with_glyphs(snapshot.glyph_count());
    let frame = compiler
        .compile(&authored, environment(), UiTheme::default())
        .unwrap();
    assert!(frame.diagnostics().is_empty());
    let UiPaintPrimitive::Text {
        rect,
        clip_rect: Some(clip),
        layout,
        ..
    } = &frame.paint()[0]
    else {
        panic!("the prepared window must publish a clipped text primitive")
    };
    assert_eq!(rect.origin().x(), -19.75);
    assert_eq!(clip.origin().x(), 20.25);
    assert_eq!(clip.max().x(), 58.25);
    assert_eq!(layout.width(), snapshot.width());
    assert!(!layout.glyphs().is_empty());
    assert!(layout.glyphs().len() < snapshot.glyph_count());
    assert!(
        frame
            .paint()
            .iter()
            .filter_map(|paint| match paint {
                UiPaintPrimitive::Text { layout, .. } => Some(layout.glyphs().len()),
                _ => None,
            })
            .sum::<usize>()
            < snapshot.glyph_count()
    );
}

fn compiler_with_glyphs(max_glyphs: usize) -> UiCompiler {
    compiler(TextLimits {
        max_glyphs,
        ..TextLimits::default()
    })
}

#[test]
fn prepared_sources_pay_whole_bytes_once_per_identity_across_roots_even_when_clipped() {
    let snapshot = source(&"M".repeat(20));
    let limits = TextLimits {
        max_text_bytes: 20,
        ..TextLimits::default()
    };
    let mut compiler = compiler(limits);
    let reused = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root("one", [node("one", &snapshot, 12.0)]))
        .with_root(root("two", [node("two", &snapshot, 12.0)]))
        .build()
        .unwrap();
    let first = compiler
        .compile(&reused, environment(), UiTheme::default())
        .unwrap();
    assert!(first.diagnostics().is_empty());
    assert_eq!(first.root_outcomes().len(), 2);

    let independent = source(&"M".repeat(20));
    assert!(!snapshot.same_source(&independent));
    let rejected = UiAuthoringSnapshotBuilder::new(2)
        .with_root(root("one", [node("one", &snapshot, 12.0)]))
        .with_root(root(
            "two",
            [UiNode::new(UiStyle::absolute(1000.0, 0.0, 12.0, 32.0))
                .with_key(UiLocalKey::runtime("prepared-test", "clipped").unwrap())
                .with_prepared_text(UiPreparedText::new(independent))],
        ))
        .build()
        .unwrap();
    assert!(matches!(
        compiler.compile(&rejected, environment(), UiTheme::default()),
        Err(UiCompileError::TextWorkLimitExceeded {
            actual: 40,
            maximum: 20
        })
    ));
    let recovered = compiler
        .compile(&reused, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(recovered.revision().get(), first.revision().get() + 1);
    assert_eq!(recovered.paint(), first.paint());
}

#[test]
fn prepared_and_ordinary_sources_share_candidate_input_admission() {
    let snapshot = source(&"M".repeat(20));
    let mut compiler = compiler(TextLimits {
        max_text_bytes: 21,
        ..TextLimits::default()
    });
    let authored = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root(
            "one",
            [
                node("prepared", &snapshot, 12.0),
                UiNode::new(UiStyle::absolute(30.0, 0.0, 100.0, 32.0))
                    .with_key(UiLocalKey::runtime("prepared-test", "chrome").unwrap())
                    .with_text(
                        UiText::new("AB", 16.0)
                            .unwrap()
                            .with_overflow(UiTextOverflow::Clip),
                    ),
            ],
        ))
        .build()
        .unwrap();
    assert!(matches!(
        compiler.compile(&authored, environment(), UiTheme::default()),
        Err(UiCompileError::TextWorkLimitExceeded {
            actual: 22,
            maximum: 21
        })
    ));
}

#[test]
fn repeated_prepared_draws_charge_output_in_each_root_and_failure_preserves_last_good() {
    let snapshot = source("MMMM");
    let mut compiler = compiler_with_glyphs(4);
    let valid = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root("one", [node("prepared", &snapshot, 100.0)]))
        .build()
        .unwrap();
    let first = compiler
        .compile(&valid, environment(), UiTheme::default())
        .unwrap();
    assert!(first.diagnostics().is_empty());
    for same_root in [true, false] {
        let mut candidate = UiAuthoringSnapshotBuilder::new(2);
        if same_root {
            candidate = candidate.with_root(root(
                "one",
                [
                    node("prepared", &snapshot, 100.0),
                    node("second", &snapshot, 100.0),
                ],
            ));
        } else {
            candidate = candidate
                .with_root(root("one", [node("prepared", &snapshot, 100.0)]))
                .with_root(root("two", [node("second", &snapshot, 100.0)]));
        }
        assert!(matches!(
            compiler.compile(
                &candidate.build().unwrap(),
                environment(),
                UiTheme::default()
            ),
            Err(UiCompileError::ShapedGlyphLimitExceeded {
                actual: 8,
                maximum: 4
            })
        ));
    }
    let fallback = FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let mut foreign = TextEngine::new(fallback, TextLimits::default()).unwrap();
    let foreign = TextEditor::new(&mut foreign, "M", 16.0)
        .unwrap()
        .paint_snapshot();
    let invalid = UiAuthoringSnapshotBuilder::new(3)
        .with_root(root("one", [node("prepared", &foreign, 100.0)]))
        .build()
        .unwrap();
    let retained = compiler
        .compile(&invalid, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(retained.revision().get(), first.revision().get() + 1);
    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert_eq!(retained.paint(), first.paint());
    assert!(retained.semantics().is_empty());
    let recovered = compiler
        .compile(&valid, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(
        recovered.root_outcomes()[0].status(),
        UiRootStatus::Published
    );
    assert!(recovered.diagnostics().is_empty());
}

#[test]
fn prepared_windows_and_ordinary_chrome_share_the_output_ceiling() {
    let snapshot = source("MMM");
    let mut compiler = compiler_with_glyphs(4);
    let authored = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root(
            "one",
            [
                node("prepared", &snapshot, 100.0),
                UiNode::new(UiStyle::absolute(100.0, 0.0, 100.0, 32.0))
                    .with_key(UiLocalKey::runtime("prepared-test", "chrome").unwrap())
                    .with_text(
                        UiText::new("AA", 16.0)
                            .unwrap()
                            .with_overflow(UiTextOverflow::Clip),
                    ),
            ],
        ))
        .build()
        .unwrap();
    assert!(matches!(
        compiler.compile(&authored, environment(), UiTheme::default()),
        Err(UiCompileError::ShapedGlyphLimitExceeded {
            actual: 5,
            maximum: 4
        })
    ));
}

#[test]
fn repeated_snapshot_handles_still_consume_authored_candidate_nodes() {
    let snapshot = source("M");
    let limits = UiLimits::default().with_max_nodes(2).unwrap();
    assert!(matches!(
        UiAuthoringSnapshotBuilder::new(1)
            .with_limits(limits)
            .with_root(root(
                "one",
                [node("one", &snapshot, 10.0), node("two", &snapshot, 10.0)]
            ))
            .build(),
        Err(UiAuthoringError::NodeLimitExceeded {
            actual: 3,
            maximum: 2
        })
    ));
    // Explicit fixed dimensions must not make Taffy measurement a prerequisite for admission.
    let authored = UiAuthoringSnapshotBuilder::new(2)
        .with_root(root(
            "one",
            [UiNode::new(
                UiStyle::flex()
                    .with_width(UiDimension::Px(10.0))
                    .with_height(UiDimension::Px(32.0)),
            )
            .with_key(UiLocalKey::runtime("prepared-test", "fixed").unwrap())
            .with_prepared_text(UiPreparedText::new(source("MM")))],
        ))
        .build()
        .unwrap();
    let mut compiler = compiler(TextLimits {
        max_text_bytes: 1,
        ..TextLimits::default()
    });
    assert!(matches!(
        compiler.compile(&authored, environment(), UiTheme::default()),
        Err(UiCompileError::TextWorkLimitExceeded {
            actual: 2,
            maximum: 1
        })
    ));
}

#[test]
fn fully_clipped_and_hidden_prepared_sources_still_enforce_raster_extent_and_keep_last_good() {
    let mut owner = TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
    let snapshot = TextEditor::new(&mut owner, "W", 64.0)
        .unwrap()
        .paint_snapshot();
    let mut compiler = compiler(TextLimits {
        max_glyph_extent: 4,
        ..TextLimits::default()
    });
    let baseline = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root(
            "extent",
            [
                UiNode::new(UiStyle::absolute(0.0, 0.0, 40.0, 40.0)).with_visual(
                    nara_ui_kernel::UiVisual::solid(nara_ui_kernel::UiColor::rgba(
                        0.2, 0.3, 0.4, 1.0,
                    )),
                ),
            ],
        ))
        .build()
        .unwrap();
    let first = compiler
        .compile(&baseline, environment(), UiTheme::default())
        .unwrap();
    assert!(first.diagnostics().is_empty());
    let text_key = || UiLocalKey::runtime("prepared-test", "extent").unwrap();
    let outside = UiNode::new(UiStyle::absolute(
        1000.25,
        0.5,
        snapshot.width(),
        snapshot.height(),
    ))
    .with_key(text_key())
    .with_prepared_text(UiPreparedText::new(snapshot.clone()));
    let hidden = UiNode::new(
        UiStyle::absolute(0.25, 0.5, snapshot.width(), snapshot.height()).with_visible(false),
    )
    .with_key(text_key())
    .with_prepared_text(UiPreparedText::new(snapshot.clone()));
    let zero_width = UiNode::new(UiStyle::absolute(0.25, 0.5, 0.0, snapshot.height()))
        .with_key(text_key())
        .with_prepared_text(UiPreparedText::new(snapshot.clone()));
    let empty_content = UiNode::new(
        UiStyle::absolute(0.25, 0.5, 4.0, 4.0).with_padding(UiEdges::all(2.0).unwrap()),
    )
    .with_key(text_key())
    .with_prepared_text(UiPreparedText::new(snapshot.clone()));
    let invisible_parent =
        UiNode::new(UiStyle::absolute(1000.25, 0.5, 80.0, 80.0).with_overflow(UiOverflow::Clip))
            .with_child(
                UiNode::new(UiStyle::absolute(
                    0.0,
                    0.0,
                    snapshot.width(),
                    snapshot.height(),
                ))
                .with_key(text_key())
                .with_prepared_text(UiPreparedText::new(snapshot)),
            );
    for (index, invalid) in [outside, hidden, zero_width, empty_content, invisible_parent]
        .into_iter()
        .enumerate()
    {
        let authored = UiAuthoringSnapshotBuilder::new(2 + index as u64)
            .with_root(root("extent", [invalid]))
            .build()
            .unwrap();
        let retained = compiler
            .compile(&authored, environment(), UiTheme::default())
            .unwrap();
        assert_eq!(
            retained.root_outcomes()[0].status(),
            UiRootStatus::RetainedVisual
        );
        assert!(matches!(retained.root_outcomes()[0].diagnostic(),
            Some(nara_ui_kernel::UiRootDiagnostic::TextRejected { message })
                if message.contains("maximum extent is 4")));
        assert_eq!(retained.paint(), first.paint());
        assert!(retained.semantics().is_empty());
    }
    let recovered = compiler
        .compile(&baseline, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(
        recovered.root_outcomes()[0].status(),
        UiRootStatus::Published
    );
    assert!(recovered.diagnostics().is_empty());
}
