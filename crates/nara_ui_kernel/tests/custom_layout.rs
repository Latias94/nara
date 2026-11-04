use nara_text::{TextEngine, TextLayout, TextLayoutRequest, TextLimits, bundled_font_face};
use nara_ui_kernel::{
    UiAlign, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiAvailableSpace, UiColor,
    UiCompileError, UiCompiler, UiCustomLayout, UiDimension, UiEdges, UiEnvironment,
    UiFlexDirection, UiFrame, UiInteraction, UiKey, UiLayer, UiLayout, UiLayoutChildren,
    UiLayoutConstraints, UiLayoutError, UiLayoutFact, UiLayoutOutput, UiLimits, UiLocalKey, UiNode,
    UiOverflow, UiPaintPrimitive, UiPoint, UiPositionMode, UiRect, UiRoot, UiRootDiagnostic,
    UiRootKey, UiRootStatus, UiSafeAreaInsets, UiSemanticRole, UiSemantics, UiSize, UiStyle,
    UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiText, UiTheme, UiVisual,
};

fn size(width: f32, height: f32) -> UiSize {
    UiSize::new(width, height).unwrap()
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> UiRect {
    UiRect::from_origin_size(x, y, width, height).unwrap()
}

fn key(name: &str) -> UiLocalKey {
    UiLocalKey::runtime("custom-layout", name).unwrap()
}

fn root_key(name: &str) -> UiRootKey {
    UiRootKey::runtime("custom-layout", name).unwrap()
}

fn scoped(root: &str, child: &str) -> UiKey {
    UiKey::scoped(root_key(root), key(child)).unwrap()
}

fn environment(scale: f32) -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(93).unwrap(),
        320.0,
        240.0,
        scale,
        1,
        1,
        1,
        1,
    )
    .unwrap()
}

fn compiler(limits: UiLimits) -> UiCompiler {
    UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap()
}

fn snapshot(revision: u64, roots: impl IntoIterator<Item = UiRoot>) -> UiAuthoringSnapshot {
    roots
        .into_iter()
        .fold(
            UiAuthoringSnapshotBuilder::new(revision),
            |builder, root| builder.with_root(root),
        )
        .build()
        .unwrap()
}

fn compile(compiler: &mut UiCompiler, source: &UiAuthoringSnapshot) -> std::sync::Arc<UiFrame> {
    compiler
        .compile(source, environment(1.0), UiTheme::default())
        .unwrap()
}

fn layout<'a>(frame: &'a UiFrame, key: &UiKey) -> &'a UiLayoutFact {
    frame
        .layouts()
        .iter()
        .find(|fact| fact.key() == Some(key))
        .unwrap()
}

fn status(frame: &UiFrame, name: &str) -> UiRootStatus {
    frame
        .root_outcomes()
        .iter()
        .find(|outcome| outcome.key() == &root_key(name))
        .unwrap()
        .status()
}

fn rejection<'a>(frame: &'a UiFrame, name: &str) -> &'a str {
    let outcome = frame
        .root_outcomes()
        .iter()
        .find(|outcome| outcome.key() == &root_key(name))
        .unwrap();
    let Some(UiRootDiagnostic::LayoutRejected { message }) = outcome.diagnostic() else {
        panic!("expected a layout rejection for {name}: {outcome:?}");
    };
    message
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

fn same_rect(actual: UiRect, expected: UiRect) {
    close(actual.origin().x(), expected.origin().x());
    close(actual.origin().y(), expected.origin().y());
    close(actual.size().width(), expected.size().width());
    close(actual.size().height(), expected.size().height());
}

fn available(width: f32, height: UiAvailableSpace) -> UiLayoutConstraints {
    UiLayoutConstraints::new(UiAvailableSpace::Definite(width), height).unwrap()
}

fn button(name: &str, style: UiStyle) -> UiNode {
    UiNode::new(style)
        .with_key(key(name))
        .with_visual(UiVisual::solid(UiColor::rgba(0.2, 0.4, 0.6, 1.0)))
        .with_interaction(UiInteraction::focus_target())
        .with_semantics(UiSemantics::new(UiSemanticRole::Button, name).unwrap())
}

struct FixedLayout {
    placements: Vec<Option<UiRect>>,
    content: UiSize,
    baseline: Option<f32>,
}

impl UiLayout for FixedLayout {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        assert_eq!(children.len(), self.placements.len());
        for (index, placement) in self.placements.iter().enumerate() {
            if let Some(rect) = placement {
                children.place(index, *rect)?;
            } else {
                children.hide(index)?;
            }
        }
        let output = UiLayoutOutput::new(self.content);
        match self.baseline {
            Some(baseline) => output.with_first_baseline(baseline),
            None => Ok(output),
        }
    }
}

fn fixed(placements: Vec<Option<UiRect>>, content: UiSize) -> UiCustomLayout {
    UiCustomLayout::new(FixedLayout {
        placements,
        content,
        baseline: None,
    })
}

const PARAGRAPH: &str = "Custom layouts measure wrapped text before choosing its final placement.";

fn text_oracle(text: &str, width: Option<f32>) -> TextLayout {
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
    let mut request = TextLayoutRequest::new(text, 12.0).unwrap();
    if let Some(width) = width {
        request = request.with_wrap_width(width).unwrap();
    }
    engine.layout(&request).unwrap()
}

struct IntrinsicTextLayout {
    natural: TextLayout,
    narrow: TextLayout,
    wide: TextLayout,
    subtree: TextLayout,
}

impl UiLayout for IntrinsicTextLayout {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        assert_eq!(children.len(), 4);
        let natural = children.measure(0, UiLayoutConstraints::MAX_CONTENT)?;
        let narrow = children.measure(0, available(64.0, UiAvailableSpace::MaxContent))?;
        let wide = children.measure(0, available(184.0, UiAvailableSpace::MaxContent))?;
        assert_eq!(
            children.measure(0, available(64.0, UiAvailableSpace::MaxContent))?,
            narrow,
            "a different intervening width must not contaminate a cached measurement",
        );
        close(natural.size().width(), self.natural.width() + 8.0);
        close(natural.size().height(), self.natural.height() + 18.0);
        close(narrow.size().height(), self.narrow.height() + 18.0);
        close(wide.size().height(), self.wide.height() + 18.0);
        close(narrow.size().width(), self.narrow.width() + 8.0);
        assert!(narrow.size().height() > wide.size().height());
        close(
            narrow.first_baseline().unwrap(),
            self.narrow.lines()[0].baseline() + 7.0,
        );
        close(
            natural.first_baseline().unwrap(),
            self.natural.lines()[0].baseline() + 7.0,
        );

        let subtree = children.measure(1, UiLayoutConstraints::MAX_CONTENT)?;
        close(subtree.size().width(), self.subtree.width().max(12.0) + 6.0);
        close(subtree.size().height(), self.subtree.height() + 9.0 + 14.0);
        let plain_box = children.measure(2, UiLayoutConstraints::MAX_CONTENT)?;
        assert_eq!(plain_box.size(), size(13.0, 9.0));
        assert_eq!(plain_box.first_baseline(), None);
        let custom_leaf = children.measure(3, UiLayoutConstraints::MAX_CONTENT)?;
        assert_eq!(custom_leaf.size(), size(18.0, 17.0));
        assert_eq!(custom_leaf.first_baseline(), Some(9.0));

        children.place(0, rect(2.0, 3.0, 64.0, narrow.size().height()))?;
        children.place(
            1,
            UiRect::new(UiPoint::new(90.0, 0.0), subtree.size()).unwrap(),
        )?;
        children.place(2, rect(90.0, 60.0, 13.0, 9.0))?;
        children.place(3, rect(90.0, 90.0, 18.0, 17.0))?;
        UiLayoutOutput::new(size(220.0, 220.0))
            .with_first_baseline(3.0 + narrow.first_baseline().unwrap())
    }
}

#[test]
fn intrinsic_text_remeasures_subtrees_and_preserves_padding_baselines_and_final_wrap() {
    let natural = text_oracle(PARAGRAPH, None);
    let narrow = text_oracle(PARAGRAPH, Some(56.0));
    let wide = text_oracle(PARAGRAPH, Some(176.0));
    assert!(narrow.lines().len() > wide.lines().len());
    let expected_height = narrow.height();
    let source = snapshot(
        1,
        [
            UiRoot::new(root_key("intrinsic"), UiLayer::Base, UiStyle::fill()).with_child(
                UiNode::new(
                    UiStyle::absolute(8.0, 6.0, 240.0, 260.0)
                        .with_padding(UiEdges::new(11.0, 13.0, 17.0, 19.0).unwrap()),
                )
                .with_key(key("container"))
                .with_custom_layout(UiCustomLayout::new(IntrinsicTextLayout {
                    natural,
                    narrow,
                    wide,
                    subtree: text_oracle("Subtree", None),
                }))
                .with_children([
                    UiNode::new(
                        UiStyle::flex().with_padding(UiEdges::new(3.0, 5.0, 7.0, 11.0).unwrap()),
                    )
                    .with_key(key("paragraph"))
                    .with_text(UiText::new(PARAGRAPH, 12.0).unwrap()),
                    UiNode::new(
                        UiStyle::flex().with_padding(UiEdges::new(2.0, 4.0, 6.0, 8.0).unwrap()),
                    )
                    .with_key(key("subtree"))
                    .with_children([
                        UiNode::new(UiStyle::flex())
                            .with_key(key("subtree-text"))
                            .with_text(UiText::new("Subtree", 12.0).unwrap()),
                        UiNode::new(
                            UiStyle::flex()
                                .with_width(UiDimension::Px(12.0))
                                .with_height(UiDimension::Px(9.0)),
                        ),
                    ]),
                    UiNode::new(
                        UiStyle::flex()
                            .with_width(UiDimension::Px(13.0))
                            .with_height(UiDimension::Px(9.0)),
                    ),
                    UiNode::new(
                        UiStyle::flex().with_padding(UiEdges::new(1.0, 2.0, 3.0, 4.0).unwrap()),
                    )
                    .with_custom_layout(UiCustomLayout::new(FixedLayout {
                        placements: vec![],
                        content: size(15.0, 10.0),
                        baseline: Some(6.0),
                    })),
                ]),
            ),
        ],
    );
    let mut compiler = compiler(UiLimits::default());
    let frame = compile(&mut compiler, &source);
    assert_eq!(
        status(&frame, "intrinsic"),
        UiRootStatus::Published,
        "{:?}",
        frame.diagnostics()
    );
    assert!(frame.diagnostics().is_empty());
    // Root output describes natural content; authored outer dimensions still own allocation.
    same_rect(
        layout(&frame, &scoped("intrinsic", "container")).rect(),
        rect(8.0, 6.0, 240.0, 260.0),
    );
    same_rect(
        layout(&frame, &scoped("intrinsic", "paragraph")).rect(),
        rect(21.0, 26.0, 64.0, expected_height + 18.0),
    );
    let UiPaintPrimitive::Text { rect: text_rect, layout: shaped, .. } = frame
        .paint().iter().find(|paint| matches!(paint, UiPaintPrimitive::Text { key: Some(key), .. } if key == &scoped("intrinsic", "paragraph"))).unwrap()
    else { panic!("the placed paragraph must publish its text"); };
    same_rect(*text_rect, rect(24.0, 33.0, 56.0, expected_height));
    assert_eq!(shaped.as_ref(), &text_oracle(PARAGRAPH, Some(56.0)));
    let work = compiler.work_stats();
    assert!(work.custom_layout_callbacks > 0);
    assert!(work.custom_child_measurements >= 7);
    assert!(work.custom_child_placements >= 4);
}

struct PercentagePlacement;

impl UiLayout for PercentagePlacement {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        assert_eq!(children.len(), 2);
        let small = children.measure(0, available(100.0, UiAvailableSpace::Definite(60.0)))?;
        let large = children.measure(0, available(200.0, UiAvailableSpace::Definite(100.0)))?;
        assert_eq!(small.size(), size(50.0, 30.0));
        assert_eq!(large.size(), size(100.0, 50.0));
        assert_eq!(
            children.measure(0, available(100.0, UiAvailableSpace::Definite(60.0)))?,
            small,
        );
        children.place(0, rect(3.0, 5.0, 120.0, 40.0))?;
        children.place(1, rect(4.0, 60.0, 80.0, 30.0))?;
        Ok(UiLayoutOutput::new(size(124.0, 90.0)))
    }
}

struct HalfContent;

impl UiLayout for HalfContent {
    fn layout(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        let width = match constraints.width {
            UiAvailableSpace::Definite(value) => value,
            _ => 76.0,
        };
        let height = match constraints.height {
            UiAvailableSpace::Definite(value) => value,
            _ => 18.0,
        };
        children.place(0, rect(0.0, 0.0, width / 2.0, height / 2.0))?;
        Ok(UiLayoutOutput::new(size(width, height)))
    }
}

#[test]
fn nested_custom_and_flex_layouts_recompute_percentages_at_exact_outer_placements() {
    let source = snapshot(
        1,
        [
            UiRoot::new(root_key("nested"), UiLayer::Base, UiStyle::fill()).with_child(
                UiNode::new(
                    UiStyle::absolute(5.0, 7.0, 220.0, 180.0)
                        .with_padding(UiEdges::all(10.0).unwrap())
                        .with_align_items(UiAlign::Start),
                )
                .with_child(
                    UiNode::new(
                        UiStyle::flex()
                            .with_width(UiDimension::Percent(0.75))
                            .with_height(UiDimension::Px(130.0))
                            .with_padding(UiEdges::new(4.0, 6.0, 8.0, 2.0).unwrap()),
                    )
                    .with_key(key("custom"))
                    .with_custom_layout(UiCustomLayout::new(PercentagePlacement))
                    .with_children([
                        UiNode::new(
                            UiStyle::flex()
                                .with_width(UiDimension::Percent(0.5))
                                .with_height(UiDimension::Percent(0.5))
                                .with_padding(UiEdges::all(2.0).unwrap())
                                .with_align_items(UiAlign::Start),
                        )
                        .with_key(key("flex"))
                        .with_child(
                            UiNode::new(
                                UiStyle::flex()
                                    .with_width(UiDimension::Percent(0.5))
                                    .with_height(UiDimension::Percent(1.0)),
                            )
                            .with_key(key("flex-percent"))
                            .with_custom_layout(UiCustomLayout::new(HalfContent))
                            .with_child(
                                UiNode::new(UiStyle::flex())
                                    .with_key(key("measured-inner"))
                                    .with_child(
                                        UiNode::new(
                                            UiStyle::flex()
                                                .with_width(UiDimension::Percent(0.5))
                                                .with_height(UiDimension::Percent(0.5)),
                                        )
                                        .with_key(key("measured-percent")),
                                    ),
                            ),
                        ),
                        UiNode::new(
                            UiStyle::absolute(99.0, 99.0, 300.0, 200.0)
                                .with_padding(UiEdges::new(1.0, 3.0, 5.0, 7.0).unwrap()),
                        )
                        .with_key(key("inner-custom"))
                        .with_custom_layout(UiCustomLayout::new(HalfContent))
                        .with_child(
                            UiNode::new(UiStyle::flex())
                                .with_key(key("inner-flex"))
                                .with_child(
                                    UiNode::new(
                                        UiStyle::flex()
                                            .with_width(UiDimension::Percent(0.5))
                                            .with_height(UiDimension::Percent(0.5)),
                                    )
                                    .with_key(key("inner-percent")),
                                ),
                        ),
                    ]),
                ),
            ),
        ],
    );
    let mut compiler = compiler(UiLimits::default());
    let frame = compile(&mut compiler, &source);
    assert_eq!(
        status(&frame, "nested"),
        UiRootStatus::Published,
        "{:?}",
        frame.diagnostics()
    );
    for (name, expected) in [
        ("custom", rect(15.0, 17.0, 150.0, 130.0)),
        ("flex", rect(22.0, 30.0, 120.0, 40.0)),
        ("flex-percent", rect(24.0, 32.0, 58.0, 36.0)),
        ("measured-inner", rect(24.0, 32.0, 29.0, 18.0)),
        ("measured-percent", rect(24.0, 32.0, 14.5, 9.0)),
        ("inner-custom", rect(23.0, 85.0, 80.0, 30.0)),
        ("inner-flex", rect(24.0, 90.0, 38.0, 9.0)),
        ("inner-percent", rect(24.0, 90.0, 19.0, 4.5)),
    ] {
        same_rect(layout(&frame, &scoped("nested", name)).rect(), expected);
    }
    let repeated = compile(&mut compiler, &source);
    assert_eq!(repeated.layouts(), frame.layouts());
    assert!(repeated.diagnostics().is_empty());
}

#[test]
fn ordinary_nested_flex_absolute_percent_and_padding_match_an_independent_taffy_tree() {
    use taffy::{
        AlignItems, AvailableSpace, Dimension, FlexDirection, LengthPercentage,
        LengthPercentageAuto, Position, Rect, Size, Style, TaffyTree,
    };

    let source = snapshot(
        1,
        [UiRoot::new(
            root_key("oracle"),
            UiLayer::Base,
            UiStyle::flex()
                .with_width(UiDimension::Px(200.0))
                .with_height(UiDimension::Px(120.0))
                .with_padding(UiEdges::all(10.0).unwrap())
                .with_flex_direction(UiFlexDirection::Row)
                .with_align_items(UiAlign::Start)
                .with_gap(6.0),
        )
        .with_children([
            UiNode::new(
                UiStyle::flex()
                    .with_width(UiDimension::Percent(0.5))
                    .with_height(UiDimension::Percent(1.0))
                    .with_padding(UiEdges::new(4.0, 6.0, 3.0, 5.0).unwrap())
                    .with_gap(2.0)
                    .with_align_items(UiAlign::Start),
            )
            .with_key(key("column"))
            .with_children([
                UiNode::new(
                    UiStyle::flex()
                        .with_width(UiDimension::Percent(0.5))
                        .with_height(UiDimension::Px(12.0)),
                )
                .with_key(key("flow")),
                UiNode::new(
                    UiStyle::flex()
                        .with_position_mode(UiPositionMode::Absolute)
                        .with_left(UiDimension::Percent(0.25))
                        .with_top(UiDimension::Percent(0.5))
                        .with_width(UiDimension::Percent(0.25))
                        .with_height(UiDimension::Px(10.0)),
                )
                .with_key(key("absolute")),
            ]),
            UiNode::new(
                UiStyle::flex()
                    .with_flex_grow(1.0)
                    .with_height(UiDimension::Px(60.0))
                    .with_padding(UiEdges::all(2.0).unwrap()),
            )
            .with_key(key("growing"))
            .with_child(UiNode::new(UiStyle::fill()).with_key(key("fill"))),
        ])],
    );

    // Deliberately author the oracle separately; do not reuse Nara's private style lowering.
    let mut oracle: TaffyTree<()> = TaffyTree::new();
    oracle.disable_rounding();
    let flow = oracle
        .new_leaf(Style {
            size: Size {
                width: Dimension::percent(0.5),
                height: Dimension::length(12.0),
            },
            ..Default::default()
        })
        .unwrap();
    let absolute = oracle
        .new_leaf(Style {
            position: Position::Absolute,
            inset: Rect {
                left: LengthPercentageAuto::percent(0.25),
                top: LengthPercentageAuto::percent(0.5),
                right: LengthPercentageAuto::auto(),
                bottom: LengthPercentageAuto::auto(),
            },
            size: Size {
                width: Dimension::percent(0.25),
                height: Dimension::length(10.0),
            },
            ..Default::default()
        })
        .unwrap();
    let column = oracle
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                align_items: Some(AlignItems::Start),
                size: Size {
                    width: Dimension::percent(0.5),
                    height: Dimension::percent(1.0),
                },
                padding: Rect {
                    left: LengthPercentage::length(4.0),
                    right: LengthPercentage::length(6.0),
                    top: LengthPercentage::length(3.0),
                    bottom: LengthPercentage::length(5.0),
                },
                gap: Size {
                    width: LengthPercentage::length(2.0),
                    height: LengthPercentage::length(2.0),
                },
                ..Default::default()
            },
            &[flow, absolute],
        )
        .unwrap();
    let fill = oracle
        .new_leaf(Style {
            size: Size {
                width: Dimension::percent(1.0),
                height: Dimension::percent(1.0),
            },
            ..Default::default()
        })
        .unwrap();
    let growing = oracle
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                size: Size {
                    width: Dimension::auto(),
                    height: Dimension::length(60.0),
                },
                padding: Rect {
                    left: LengthPercentage::length(2.0),
                    right: LengthPercentage::length(2.0),
                    top: LengthPercentage::length(2.0),
                    bottom: LengthPercentage::length(2.0),
                },
                ..Default::default()
            },
            &[fill],
        )
        .unwrap();
    let root = oracle
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Row,
                align_items: Some(AlignItems::Start),
                size: Size {
                    width: Dimension::length(200.0),
                    height: Dimension::length(120.0),
                },
                padding: Rect {
                    left: LengthPercentage::length(10.0),
                    right: LengthPercentage::length(10.0),
                    top: LengthPercentage::length(10.0),
                    bottom: LengthPercentage::length(10.0),
                },
                gap: Size {
                    width: LengthPercentage::length(6.0),
                    height: LengthPercentage::length(6.0),
                },
                ..Default::default()
            },
            &[column, growing],
        )
        .unwrap();
    oracle
        .compute_layout(
            root,
            Size {
                width: AvailableSpace::Definite(320.0),
                height: AvailableSpace::Definite(240.0),
            },
        )
        .unwrap();

    let mut compiler = compiler(UiLimits::default());
    let frame = compile(&mut compiler, &source);
    assert_eq!(status(&frame, "oracle"), UiRootStatus::Published);
    assert_eq!(frame.layouts().len(), 6);
    for (key, path) in [
        (UiKey::root(root_key("oracle")).unwrap(), vec![root]),
        (scoped("oracle", "column"), vec![root, column]),
        (scoped("oracle", "flow"), vec![root, column, flow]),
        (scoped("oracle", "absolute"), vec![root, column, absolute]),
        (scoped("oracle", "growing"), vec![root, growing]),
        (scoped("oracle", "fill"), vec![root, growing, fill]),
    ] {
        let mut x = 0.0;
        let mut y = 0.0;
        for node in &path {
            let result = oracle.layout(*node).unwrap();
            x += result.location.x;
            y += result.location.y;
        }
        let result = oracle.layout(*path.last().unwrap()).unwrap();
        same_rect(
            layout(&frame, &key).rect(),
            rect(x, y, result.size.width, result.size.height),
        );
    }
    assert_eq!(compiler.work_stats().custom_layout_callbacks, 0);
    assert!(frame.diagnostics().is_empty());
}

#[test]
fn fractional_content_transform_maps_one_clipped_geometry_into_every_projection() {
    let transform = UiSurfaceContentTransform::new(
        size(200.0, 100.0),
        size(400.0, 300.0),
        UiSafeAreaInsets::new(10.0, 20.0, 30.0, 40.0).unwrap(),
        1.25,
    )
    .unwrap();
    let environment = UiEnvironment::for_surface(
        UiSurfaceId::new(93).unwrap(),
        transform,
        UiSurfaceGenerations::new(1, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap();
    let source = snapshot(
        1,
        [
            UiRoot::new(root_key("mapped"), UiLayer::Base, UiStyle::fill()).with_child(
                UiNode::new(
                    UiStyle::absolute(10.25, 5.5, 80.0, 50.0)
                        .with_padding(UiEdges::new(5.5, 4.5, 3.25, 6.75).unwrap())
                        .with_overflow(UiOverflow::Clip),
                )
                .with_key(key("mapped-container"))
                .with_custom_layout(fixed(
                    vec![Some(rect(-12.5, 2.25, 100.25, 55.5))],
                    size(70.0, 40.0),
                ))
                .with_child(
                    button("mapped-button", UiStyle::absolute(100.0, 100.0, 1.0, 1.0)).with_child(
                        UiNode::new(UiStyle::absolute(2.0, 3.0, 40.0, 18.0))
                            .with_key(key("mapped-text"))
                            .with_text(UiText::new("Mapped", 12.0).unwrap()),
                    ),
                ),
            ),
        ],
    );
    let mut compiler = compiler(UiLimits::default());
    let frame = compiler
        .compile(&source, environment, UiTheme::default())
        .unwrap();
    assert_eq!(
        status(&frame, "mapped"),
        UiRootStatus::Published,
        "{:?}",
        frame.diagnostics()
    );
    let button_key = scoped("mapped", "mapped-button");
    let bounds = rect(3.25, 11.0, 100.25, 55.5);
    let clip = rect(10.25, 5.5, 80.0, 50.0);
    let fact = layout(&frame, &button_key);
    same_rect(fact.rect(), bounds);
    assert_eq!(fact.clip_rect(), Some(clip));
    assert!(fact.visible());
    let UiPaintPrimitive::Rect {
        rect: painted,
        physical_rect,
        clip_rect,
        ..
    } = frame
        .paint()
        .iter()
        .find(|paint| paint.key() == Some(&button_key))
        .unwrap()
    else {
        panic!("the custom placement must publish its background");
    };
    assert_eq!(*painted, bounds);
    assert_eq!(*clip_rect, Some(clip));
    assert_eq!(*physical_rect, fact.physical_rect());
    assert_eq!(*physical_rect, environment.physical_rect(bounds).unwrap());
    assert_eq!(
        (
            physical_rect.min_x(),
            physical_rect.min_y(),
            physical_rect.max_x(),
            physical_rect.max_y()
        ),
        (16, 70, 196, 170),
    );
    let physical_clip = environment.physical_rect(clip).unwrap();
    assert_eq!(
        (
            physical_clip.min_x(),
            physical_clip.min_y(),
            physical_clip.max_x(),
            physical_clip.max_y()
        ),
        (28, 60, 172, 150),
    );
    assert_eq!(frame.hit_regions().len(), 1);
    assert_eq!(frame.hit_regions()[0].key(), &button_key);
    assert_eq!(frame.hit_regions()[0].rect(), bounds);
    assert_eq!(frame.focus_order().len(), 1);
    assert_eq!(frame.focus_order()[0].key(), &button_key);
    assert_eq!(frame.focus_order()[0].rect(), bounds);
    let semantic = frame
        .semantics()
        .iter()
        .find(|node| node.key() == &button_key)
        .unwrap();
    assert_eq!(semantic.rect(), bounds);
    assert_eq!(semantic.clip_rect(), Some(clip));
    let text_key = scoped("mapped", "mapped-text");
    let text_semantic = frame
        .semantics()
        .iter()
        .find(|node| node.key() == &text_key)
        .unwrap();
    assert_eq!(text_semantic.parent(), Some(&button_key));
    assert_eq!(layout(&frame, &text_key).route_parent(), Some(&button_key));
    for paint in frame.paint() {
        let physical_rect = match paint {
            UiPaintPrimitive::Rect { physical_rect, .. }
            | UiPaintPrimitive::Text { physical_rect, .. } => physical_rect,
        };
        assert_eq!(paint.clip_rect(), Some(clip));
        assert_eq!(
            *physical_rect,
            environment.physical_rect(paint.rect()).unwrap()
        );
    }
    for authored in [
        UiPoint::new(20.0, 20.0),
        UiPoint::new(5.0, 20.0),
        UiPoint::new(20.0, 60.0),
    ] {
        let physical = transform.authored_to_physical(authored).unwrap();
        let platform = UiPoint::new(physical.x() / 1.25, physical.y() / 1.25);
        let mapped = transform.platform_logical_to_authored(platform).unwrap();
        close(mapped.x(), authored.x());
        close(mapped.y(), authored.y());
        assert_eq!(
            frame.hit_test(mapped).map(|hit| hit.key()),
            clip.contains(authored).then_some(&button_key)
        );
    }
    assert!(frame.diagnostics().is_empty());
}

struct ToggleBranch {
    hidden: bool,
}

impl UiLayout for ToggleBranch {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        // Even a measured subtree must disappear from all projections after an explicit hide.
        children.measure(0, UiLayoutConstraints::MAX_CONTENT)?;
        if self.hidden {
            children.hide(0)?;
        } else {
            children.place(0, rect(12.0, 14.0, 80.0, 40.0))?;
        }
        children.place(1, rect(120.0, 14.0, 40.0, 30.0))?;
        Ok(UiLayoutOutput::new(size(180.0, 80.0)))
    }
}

fn toggled_root(hidden: bool) -> UiRoot {
    UiRoot::new(root_key("visibility"), UiLayer::Base, UiStyle::fill())
        .with_custom_layout(UiCustomLayout::new(ToggleBranch { hidden }))
        .with_children([
            button("branch", UiStyle::flex()).with_child(
                button("descendant", UiStyle::absolute(3.0, 4.0, 30.0, 20.0))
                    .with_text(UiText::new("Child", 12.0).unwrap()),
            ),
            button("survivor", UiStyle::flex()),
        ])
}

#[test]
fn explicitly_hidden_measured_subtrees_leave_no_stale_paint_hits_focus_or_semantics() {
    let mut compiler = compiler(UiLimits::default());
    let visible = compile(&mut compiler, &snapshot(1, [toggled_root(false)]));
    assert_eq!(status(&visible, "visibility"), UiRootStatus::Published);
    assert_eq!(visible.hit_regions().len(), 3);
    assert_eq!(visible.focus_order().len(), 3);
    assert_eq!(visible.semantics().len(), 3);
    assert_eq!(visible.paint().len(), 4);
    let before = compiler.work_stats();
    let hidden = compile(&mut compiler, &snapshot(2, [toggled_root(true)]));
    assert_eq!(status(&hidden, "visibility"), UiRootStatus::Published);
    assert!(hidden.diagnostics().is_empty());
    let survivor = scoped("visibility", "survivor");
    assert_eq!(hidden.paint().len(), 1);
    assert_eq!(hidden.paint()[0].key(), Some(&survivor));
    assert_eq!(hidden.hit_regions().len(), 1);
    assert_eq!(hidden.hit_regions()[0].key(), &survivor);
    assert_eq!(hidden.focus_order().len(), 1);
    assert_eq!(hidden.focus_order()[0].key(), &survivor);
    assert_eq!(hidden.semantics().len(), 1);
    assert_eq!(hidden.semantics()[0].key(), &survivor);
    for name in ["branch", "descendant"] {
        assert!(
            hidden
                .layouts()
                .iter()
                .filter(|fact| fact.key() == Some(&scoped("visibility", name)))
                .all(|fact| !fact.visible())
        );
    }
    assert!(hidden.hit_test(UiPoint::new(16.0, 19.0)).is_none());
    assert_eq!(
        hidden.hit_test(UiPoint::new(125.0, 20.0)).unwrap().key(),
        &survivor
    );
    assert!(compiler.work_stats().custom_child_measurements > before.custom_child_measurements);
    let recovered = compile(&mut compiler, &snapshot(3, [toggled_root(false)]));
    assert_eq!(status(&recovered, "visibility"), UiRootStatus::Published);
    assert_eq!(recovered.layouts(), visible.layouts());
    assert_eq!(recovered.paint(), visible.paint());
    assert_eq!(recovered.hit_regions(), visible.hit_regions());
    assert_eq!(recovered.focus_order(), visible.focus_order());
    assert_eq!(recovered.semantics(), visible.semantics());
}

#[derive(Clone, Copy, Debug)]
enum InvalidWorkflow {
    MeasureForeignChild,
    PlaceForeignChild,
    HideForeignChild,
    NanConstraint,
    NegativeConstraint,
    InfiniteConstraint,
    PlaceTwice,
    PlaceThenHide,
    HideThenPlace,
    HideTwice,
    MeasureAfterPlace,
    MeasureAfterHide,
    MissingChild,
    ImpossiblePadding,
}

impl InvalidWorkflow {
    fn expected(self) -> UiLayoutError {
        match self {
            Self::MeasureForeignChild | Self::PlaceForeignChild | Self::HideForeignChild => {
                UiLayoutError::InvalidChild {
                    child: 2,
                    children: 2,
                }
            }
            Self::NanConstraint | Self::NegativeConstraint | Self::InfiniteConstraint => {
                UiLayoutError::InvalidConstraints
            }
            Self::MissingChild => UiLayoutError::MissingPlacement { child: 0 },
            Self::ImpossiblePadding => UiLayoutError::PlacementSizeMismatch { child: 0 },
            _ => UiLayoutError::DuplicatePlacement { child: 0 },
        }
    }
}

impl UiLayout for InvalidWorkflow {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        let allocation = rect(0.0, 0.0, 40.0, 40.0);
        let result = match self {
            Self::MeasureForeignChild => children
                .measure(2, UiLayoutConstraints::MAX_CONTENT)
                .map(|_| ()),
            Self::PlaceForeignChild => children.place(2, allocation),
            Self::HideForeignChild => children.hide(2),
            Self::NanConstraint | Self::NegativeConstraint | Self::InfiniteConstraint => {
                let value = match self {
                    Self::NanConstraint => f32::NAN,
                    Self::NegativeConstraint => -1.0,
                    _ => f32::INFINITY,
                };
                children
                    .measure(
                        0,
                        UiLayoutConstraints {
                            width: UiAvailableSpace::Definite(value),
                            height: UiAvailableSpace::MaxContent,
                        },
                    )
                    .map(|_| ())
            }
            Self::PlaceTwice | Self::PlaceThenHide | Self::MeasureAfterPlace => {
                children.place(0, allocation)?;
                match self {
                    Self::PlaceTwice => children.place(0, allocation),
                    Self::PlaceThenHide => children.hide(0),
                    _ => children
                        .measure(0, UiLayoutConstraints::MAX_CONTENT)
                        .map(|_| ()),
                }
            }
            Self::HideThenPlace | Self::HideTwice | Self::MeasureAfterHide => {
                children.hide(0)?;
                match self {
                    Self::HideThenPlace => children.place(0, allocation),
                    Self::HideTwice => children.hide(0),
                    _ => children
                        .measure(0, UiLayoutConstraints::MAX_CONTENT)
                        .map(|_| ()),
                }
            }
            Self::MissingChild => {
                children.hide(1)?;
                return Ok(UiLayoutOutput::new(size(80.0, 60.0)));
            }
            Self::ImpossiblePadding => children.place(0, rect(0.0, 0.0, 15.0, 15.0)),
        };
        // Check the request's public error without turning a wrong result into a passing panic.
        // The outer test requires the original error's Display text, not this rejection message.
        if result != Err(self.expected()) {
            return Err(UiLayoutError::Rejected {
                message: format!("unexpected request result: {result:?}"),
            });
        }
        // Deliberately swallow every error and return a valid output. The poisoned root must fail.
        for index in 0..children.len() {
            let _ = children.hide(index);
        }
        Ok(UiLayoutOutput::new(size(80.0, 60.0)))
    }
}

fn invalid_root(name: &str, workflow: InvalidWorkflow) -> UiRoot {
    UiRoot::new(root_key(name), UiLayer::Base, UiStyle::fill())
        .with_custom_layout(UiCustomLayout::new(workflow))
        .with_children([
            button(
                "padded",
                UiStyle::flex().with_padding(UiEdges::all(8.0).unwrap()),
            )
            .with_child(button("grandchild", UiStyle::flex())),
            button("other", UiStyle::flex()),
        ])
}

#[test]
fn invalid_child_geometry_and_assignment_workflows_poison_only_the_owning_root() {
    let cases = [
        InvalidWorkflow::MeasureForeignChild,
        InvalidWorkflow::PlaceForeignChild,
        InvalidWorkflow::HideForeignChild,
        InvalidWorkflow::NanConstraint,
        InvalidWorkflow::NegativeConstraint,
        InvalidWorkflow::InfiniteConstraint,
        InvalidWorkflow::PlaceTwice,
        InvalidWorkflow::PlaceThenHide,
        InvalidWorkflow::HideThenPlace,
        InvalidWorkflow::HideTwice,
        InvalidWorkflow::MeasureAfterPlace,
        InvalidWorkflow::MeasureAfterHide,
        InvalidWorkflow::MissingChild,
        InvalidWorkflow::ImpossiblePadding,
    ];
    let mut roots = cases
        .iter()
        .enumerate()
        .map(|(index, workflow)| invalid_root(&format!("invalid-{index}"), *workflow))
        .collect::<Vec<_>>();
    roots.push(
        UiRoot::new(root_key("root-text"), UiLayer::Base, UiStyle::fill())
            .with_custom_layout(fixed(vec![], size(10.0, 10.0)))
            .with_text(UiText::new("Text belongs in a child", 12.0).unwrap()),
    );
    roots.push(
        UiRoot::new(root_key("node-text"), UiLayer::Base, UiStyle::fill()).with_child(
            UiNode::new(UiStyle::flex())
                .with_key(key("invalid-text"))
                .with_custom_layout(fixed(vec![], size(10.0, 10.0)))
                .with_text(UiText::new("Text belongs in a child", 12.0).unwrap()),
        ),
    );
    roots.push(
        UiRoot::new(root_key("healthy"), UiLayer::Base, UiStyle::fill()).with_child(button(
            "healthy-button",
            UiStyle::absolute(200.0, 10.0, 40.0, 30.0),
        )),
    );
    let mut compiler = compiler(UiLimits::default());
    let frame = compile(&mut compiler, &snapshot(1, roots));
    for (index, workflow) in cases.iter().enumerate() {
        let name = format!("invalid-{index}");
        assert_eq!(status(&frame, &name), UiRootStatus::Absent, "{workflow:?}");
        assert!(
            rejection(&frame, &name).contains(&workflow.expected().to_string()),
            "{workflow:?}: {}",
            rejection(&frame, &name)
        );
    }
    for name in ["root-text", "node-text"] {
        assert_eq!(status(&frame, name), UiRootStatus::Absent);
        assert!(rejection(&frame, name).contains("text"));
    }
    assert_eq!(status(&frame, "healthy"), UiRootStatus::Published);
    assert!(
        frame
            .layouts()
            .iter()
            .all(|fact| fact.root_key() == &root_key("healthy"))
    );
    assert_eq!(frame.paint().len(), 1);
    assert_eq!(frame.hit_regions().len(), 1);
    assert_eq!(frame.focus_order().len(), 1);
    assert_eq!(frame.semantics().len(), 1);
    assert_eq!(
        frame.hit_test(UiPoint::new(210.0, 20.0)).unwrap().key(),
        &scoped("healthy", "healthy-button")
    );
}

struct FailAfterPlacement {
    panic: bool,
    message: String,
}

impl UiLayout for FailAfterPlacement {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        children.measure(0, UiLayoutConstraints::MAX_CONTENT)?;
        children.place(0, rect(70.0, 50.0, 20.0, 20.0))?;
        if self.panic {
            panic!("{}", self.message);
        }
        Err(UiLayoutError::Rejected {
            message: self.message.clone(),
        })
    }
}

fn recoverable_root(custom: UiCustomLayout, color: f32) -> UiRoot {
    UiRoot::new(
        root_key("recoverable"),
        UiLayer::Base,
        UiStyle::absolute(0.0, 0.0, 120.0, 80.0),
    )
    .with_visual(UiVisual::solid(UiColor::rgba(color, 0.3, 0.4, 1.0)))
    .with_custom_layout(custom)
    .with_child(button("action", UiStyle::flex()))
}

fn live_sibling() -> UiRoot {
    UiRoot::new(root_key("sibling"), UiLayer::Base, UiStyle::fill()).with_child(
        UiNode::new(UiStyle::absolute(200.0, 10.0, 40.0, 30.0))
            .with_key(key("sibling-action"))
            .with_visual(UiVisual::solid(UiColor::rgba(0.1, 0.2, 0.3, 1.0)))
            .with_interaction(UiInteraction::focus_target())
            .with_semantics(UiSemantics::new(UiSemanticRole::Button, "Live sibling").unwrap()),
    )
}

#[test]
fn panic_and_rejection_bound_diagnostics_retain_compatible_inert_visuals_and_recover() {
    for panic in [false, true] {
        let mut compiler = compiler(UiLimits::default().with_max_diagnostics(1).unwrap());
        let good = snapshot(
            1,
            [
                recoverable_root(
                    fixed(vec![Some(rect(10.0, 12.0, 60.0, 30.0))], size(100.0, 60.0)),
                    0.2,
                ),
                live_sibling(),
            ],
        );
        let first = compile(&mut compiler, &good);
        assert_eq!(status(&first, "recoverable"), UiRootStatus::Published);
        assert_eq!(first.hit_regions().len(), 2);
        let bad = snapshot(
            2,
            [
                recoverable_root(
                    UiCustomLayout::new(FailAfterPlacement {
                        panic,
                        message: format!(
                            "{}: {}",
                            if panic { "panic" } else { "rejected" },
                            "界🙂".repeat(4096)
                        ),
                    }),
                    0.9,
                ),
                live_sibling(),
            ],
        );
        let retained = compile(&mut compiler, &bad);
        assert_eq!(
            status(&retained, "recoverable"),
            UiRootStatus::RetainedVisual
        );
        assert_eq!(status(&retained, "sibling"), UiRootStatus::Published);
        assert_eq!(retained.paint(), first.paint());
        assert_eq!(retained.layouts(), first.layouts());
        assert_eq!(retained.diagnostics().len(), 1);
        let message = rejection(&retained, "recoverable");
        assert!(!message.is_empty());
        assert!(
            message.len() <= 1024,
            "unbounded layout diagnostic: {} bytes",
            message.len()
        );
        assert!(message.contains(if panic { "panic" } else { "rejected" }));
        let sibling_key = scoped("sibling", "sibling-action");
        assert_eq!(retained.hit_regions().len(), 1);
        assert_eq!(retained.hit_regions()[0].key(), &sibling_key);
        assert_eq!(retained.focus_order().len(), 1);
        assert_eq!(retained.focus_order()[0].key(), &sibling_key);
        assert_eq!(retained.semantics().len(), 1);
        assert_eq!(retained.semantics()[0].key(), &sibling_key);
        assert!(retained.hit_test(UiPoint::new(20.0, 20.0)).is_none());
        assert_eq!(
            retained.hit_test(UiPoint::new(210.0, 20.0)).unwrap().key(),
            &sibling_key
        );

        let incompatible = compiler
            .compile(&bad, environment(1.25), UiTheme::default())
            .unwrap();
        assert_eq!(status(&incompatible, "recoverable"), UiRootStatus::Absent);
        assert_eq!(status(&incompatible, "sibling"), UiRootStatus::Published);
        assert!(
            incompatible
                .layouts()
                .iter()
                .all(|fact| fact.root_key() == &root_key("sibling"))
        );
        assert_eq!(incompatible.paint().len(), 1);

        let recovered = compile(
            &mut compiler,
            &snapshot(
                3,
                [
                    recoverable_root(
                        fixed(vec![Some(rect(30.0, 15.0, 40.0, 35.0))], size(100.0, 60.0)),
                        0.7,
                    ),
                    live_sibling(),
                ],
            ),
        );
        assert_eq!(status(&recovered, "recoverable"), UiRootStatus::Published);
        assert!(recovered.diagnostics().is_empty());
        assert_ne!(recovered.paint(), first.paint());
        assert_eq!(recovered.hit_regions().len(), 2);
        assert_eq!(recovered.focus_order().len(), 2);
        assert_eq!(recovered.semantics().len(), 2);
        same_rect(
            layout(&recovered, &scoped("recoverable", "action")).rect(),
            rect(30.0, 15.0, 40.0, 35.0),
        );
        assert_eq!(recovered.revision().get(), first.revision().get() + 3);
    }
}

struct RepeatedMeasurement {
    repeats: usize,
    reject: bool,
}

impl UiLayout for RepeatedMeasurement {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        let constraints = available(100.0, UiAvailableSpace::MaxContent);
        let measured = children.measure(0, constraints)?;
        for _ in 1..self.repeats {
            assert_eq!(children.measure(0, constraints)?, measured);
        }
        children.place(0, rect(4.0, 6.0, 80.0, 30.0))?;
        if self.reject {
            return Err(UiLayoutError::Rejected {
                message: "local budget rejection".into(),
            });
        }
        Ok(UiLayoutOutput::new(size(100.0, 50.0)))
    }
}

fn budget_root(name: &str, repeats: usize, reject: bool, color: f32) -> UiRoot {
    UiRoot::new(
        root_key(name),
        UiLayer::Base,
        UiStyle::absolute(0.0, 0.0, 120.0, 60.0),
    )
    .with_visual(UiVisual::solid(UiColor::rgba(color, 0.3, 0.4, 1.0)))
    .with_custom_layout(UiCustomLayout::new(RepeatedMeasurement { repeats, reject }))
    .with_child(
        button(
            "budget-child",
            UiStyle::flex()
                .with_width(UiDimension::Px(80.0))
                .with_height(UiDimension::Px(30.0)),
        )
        .with_child(
            UiNode::new(UiStyle::flex())
                .with_key(key("budget-text"))
                .with_text(UiText::new("Budget", 12.0).unwrap()),
        ),
    )
}

fn budget_source(repeats: usize, failed_root: bool) -> UiAuthoringSnapshot {
    let mut roots = Vec::new();
    if failed_root {
        // Local failure precedes valid roots, so its reached work must survive their admission.
        roots.push(budget_root("budget-failed", repeats, true, 0.5));
    }
    roots.push(budget_root("budget-one", repeats, false, 0.8));
    roots.push(budget_root("budget-two", repeats, false, 0.6));
    snapshot(2, roots)
}

#[test]
fn candidate_budget_counts_cached_and_failed_work_without_reducing_ordinary_capacity() {
    let capacity = snapshot(
        1,
        [
            UiRoot::new(root_key("capacity"), UiLayer::Base, UiStyle::fill()).with_children(
                (0..4095).map(|index| {
                    UiNode::new(UiStyle::absolute(
                        (index % 64) as f32 * 2.0,
                        (index / 64) as f32 * 2.0,
                        2.0,
                        2.0,
                    ))
                    .with_visual(UiVisual::solid(UiColor::rgba(0.2, 0.3, 0.4, 1.0)))
                }),
            ),
        ],
    );
    assert_eq!(capacity.source_node_count(), 4096);
    let mut ordinary = compiler(UiLimits::default());
    let full = compile(&mut ordinary, &capacity);
    assert_eq!(status(&full, "capacity"), UiRootStatus::Published);
    assert!(full.diagnostics().is_empty());
    assert_eq!(full.layouts().len(), 4096);
    assert!(full.layouts().iter().all(UiLayoutFact::visible));
    assert_eq!(full.paint().len(), 4095);
    assert_eq!(ordinary.work_stats().custom_layout_callbacks, 0);
    assert!(ordinary.work_stats().layout_work_units > 0);
    assert!(ordinary.work_stats().layout_work_units <= 1_048_576);

    let mut once = compiler(UiLimits::default());
    compile(&mut once, &budget_source(1, false));
    let single = once.work_stats();
    let mut twice = compiler(UiLimits::default());
    compile(&mut twice, &budget_source(2, false));
    let duplicate = twice.work_stats();
    assert_eq!(
        duplicate.custom_layout_callbacks,
        single.custom_layout_callbacks
    );
    assert_eq!(
        duplicate.custom_child_measurements,
        single.custom_child_measurements * 2
    );
    assert!(duplicate.layout_work_units > single.layout_work_units);

    let source = budget_source(2, true);
    let mut probe = compiler(UiLimits::default());
    let measured = compile(&mut probe, &source);
    assert_eq!(status(&measured, "budget-failed"), UiRootStatus::Absent);
    assert_eq!(status(&measured, "budget-one"), UiRootStatus::Published);
    assert_eq!(status(&measured, "budget-two"), UiRootStatus::Published);
    let work = probe.work_stats();
    assert!(work.layout_work_units > duplicate.layout_work_units);
    assert!(work.custom_child_measurements > duplicate.custom_child_measurements);
    assert!(work.custom_child_placements > duplicate.custom_child_placements);
    let actual = usize::try_from(work.layout_work_units).unwrap();
    let mut exact = compiler(UiLimits::default().with_max_layout_work(actual).unwrap());
    let accepted = compile(&mut exact, &source);
    assert_eq!(accepted.root_outcomes(), measured.root_outcomes());
    assert_eq!(accepted.layouts(), measured.layouts());
    assert_eq!(accepted.paint(), measured.paint());
    assert_eq!(exact.work_stats().layout_work_units, actual as u64);

    let mut short = compiler(
        UiLimits::default()
            .with_max_layout_work(actual - 1)
            .unwrap(),
    );
    let baseline_source = snapshot(1, [budget_root("budget-one", 1, false, 0.1)]);
    let baseline = compile(&mut short, &baseline_source);
    let before = short.work_stats();
    assert!(matches!(
        short.compile(&source, environment(1.0), UiTheme::default()),
        Err(UiCompileError::LayoutWorkLimitExceeded { actual: reached, maximum })
            if reached == actual && maximum == actual - 1
    ));
    let rejected = short.work_stats();
    assert_eq!(
        rejected.layout_work_units - before.layout_work_units,
        actual as u64
    );
    assert_eq!(rejected.compile_attempts, before.compile_attempts + 1);
    assert_eq!(
        rejected.root_compile_attempts,
        before.root_compile_attempts + 3
    );
    assert_eq!(rejected.published_frames, before.published_frames);
    assert!(rejected.custom_layout_callbacks > before.custom_layout_callbacks);
    assert!(rejected.custom_child_measurements > before.custom_child_measurements);
    assert!(rejected.custom_child_placements > before.custom_child_placements);

    // A successful earlier root in the globally refused candidate must not replace last-good.
    let retained = compile(
        &mut short,
        &snapshot(3, [budget_root("budget-one", 1, true, 0.9)]),
    );
    assert_eq!(
        status(&retained, "budget-one"),
        UiRootStatus::RetainedVisual
    );
    assert_eq!(retained.paint(), baseline.paint());
    assert_eq!(retained.layouts(), baseline.layouts());
    assert!(retained.hit_regions().is_empty());
    assert!(retained.focus_order().is_empty());
    assert!(retained.semantics().is_empty());
    assert_eq!(retained.revision().get(), baseline.revision().get() + 1);
    assert!(short.work_stats().layout_work_units > rejected.layout_work_units);
    let recovered = compile(&mut short, &baseline_source);
    assert_eq!(status(&recovered, "budget-one"), UiRootStatus::Published);
    assert_eq!(recovered.revision().get(), retained.revision().get() + 1);
    assert_eq!(recovered.paint(), baseline.paint());
    assert_eq!(recovered.hit_regions(), baseline.hit_regions());
}

struct GenuineBaselines {
    baseline: f32,
}

impl UiLayout for GenuineBaselines {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        let maximum = children.measure(0, UiLayoutConstraints::MAX_CONTENT)?;
        let minimum = children.measure(
            0,
            UiLayoutConstraints {
                width: UiAvailableSpace::MinContent,
                height: UiAvailableSpace::MaxContent,
            },
        )?;
        assert!(minimum.size().width() < maximum.size().width());
        assert!(minimum.size().height() > maximum.size().height());
        close(maximum.first_baseline().unwrap(), self.baseline + 3.0);
        let prepared = children.measure(1, UiLayoutConstraints::MAX_CONTENT)?;
        close(prepared.first_baseline().unwrap(), self.baseline + 3.0);
        assert_eq!(prepared, maximum);
        let wrapped = children.measure(2, UiLayoutConstraints::MAX_CONTENT)?;
        close(
            wrapped.first_baseline().unwrap(),
            3.0 + 7.0 + 2.0 + self.baseline,
        );
        assert_eq!(
            children
                .measure(3, UiLayoutConstraints::MAX_CONTENT)?
                .first_baseline(),
            None
        );
        assert_eq!(
            children
                .measure(4, UiLayoutConstraints::MAX_CONTENT)?
                .first_baseline(),
            None
        );
        for child in 0..children.len() {
            children.hide(child)?;
        }
        Ok(UiLayoutOutput::new(size(100.0, 100.0)))
    }
}

#[test]
fn intrinsic_minimum_differs_from_maximum_and_only_genuine_baselines_cross_the_public_interface() {
    use nara_text::TextEditor;
    use nara_ui_kernel::UiPreparedText;

    let text = "first widestword last";
    let natural = text_oracle(text, None);
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let editor = TextEditor::new(&mut engine, text, 12.0).unwrap();
    let snapshot_text = editor.paint_snapshot();
    let source = snapshot(
        1,
        [
            UiRoot::new(root_key("real-baselines"), UiLayer::Base, UiStyle::fill())
                .with_custom_layout(UiCustomLayout::new(GenuineBaselines {
                    baseline: natural.lines()[0].baseline(),
                }))
                .with_children([
                    UiNode::new(UiStyle::flex().with_padding(UiEdges::all(3.0).unwrap()))
                        .with_key(key("plain"))
                        .with_text(UiText::new(text, 12.0).unwrap()),
                    UiNode::new(UiStyle::flex().with_padding(UiEdges::all(3.0).unwrap()))
                        .with_key(key("prepared"))
                        .with_prepared_text(UiPreparedText::new(snapshot_text)),
                    UiNode::new(
                        UiStyle::flex()
                            .with_padding(UiEdges::all(3.0).unwrap())
                            .with_gap(2.0),
                    )
                    .with_child(UiNode::new(
                        UiStyle::flex()
                            .with_width(UiDimension::Px(5.0))
                            .with_height(UiDimension::Px(7.0)),
                    ))
                    .with_child(
                        UiNode::new(UiStyle::flex())
                            .with_key(key("nested-text"))
                            .with_text(UiText::new(text, 12.0).unwrap()),
                    ),
                    UiNode::new(UiStyle::flex().with_padding(UiEdges::all(3.0).unwrap()))
                        .with_child(UiNode::new(
                            UiStyle::flex()
                                .with_width(UiDimension::Px(5.0))
                                .with_height(UiDimension::Px(7.0)),
                        )),
                    UiNode::new(UiStyle::flex())
                        .with_key(key("paint-only-container-text"))
                        .with_text(UiText::new(text, 12.0).unwrap())
                        .with_child(UiNode::new(
                            UiStyle::flex().with_height(UiDimension::Px(7.0)),
                        )),
                ]),
        ],
    );
    let frame = compile(&mut compiler(Default::default()), &source);
    assert_eq!(
        status(&frame, "real-baselines"),
        UiRootStatus::Published,
        "{:?}",
        frame.diagnostics()
    );
    assert!(frame.diagnostics().is_empty());
}

struct CaptureOwner(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Drop for CaptureOwner {
    fn drop(&mut self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

struct CapturedColumn(std::sync::Arc<CaptureOwner>);

impl UiLayout for CapturedColumn {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        let _keep_code_capture = &self.0;
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        for child in 0..children.len() {
            let measured = children.measure(child, UiLayoutConstraints::MAX_CONTENT)?;
            children.place(
                child,
                UiRect::new(UiPoint::new(0.0, height), measured.size()).unwrap(),
            )?;
            width = width.max(measured.size().width());
            height += measured.size().height();
        }
        Ok(UiLayoutOutput::new(size(width, height)))
    }
}

#[test]
fn nominal_layout_identity_never_substitutes_for_current_children_or_retains_code_in_frames_and_fallback()
 {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let drops = Arc::new(AtomicUsize::new(0));
    let owner = Arc::new(CaptureOwner(drops.clone()));
    let weak = Arc::downgrade(&owner);
    let description = UiCustomLayout::new(CapturedColumn(owner.clone()));
    let cloned = description.clone();
    let independent = UiCustomLayout::new(CapturedColumn(owner.clone()));
    assert_eq!(description, cloned);
    assert_ne!(description, independent);
    let make = |text: &str| {
        snapshot(
            1,
            [
                UiRoot::new(root_key("capture"), UiLayer::Base, UiStyle::fill())
                    .with_custom_layout(description.clone())
                    .with_child(
                        UiNode::new(UiStyle::flex())
                            .with_key(key("text"))
                            .with_text(UiText::new(text, 12.0).unwrap()),
                    ),
            ],
        )
    };
    let first_source = make("Short");
    let changed_source = make("Much longer current child");
    let mut compiler = compiler(Default::default());
    let first = compile(&mut compiler, &first_source);
    let changed = compile(&mut compiler, &changed_source);
    assert!(changed.diagnostics().is_empty());
    assert!(
        changed
            .semantics()
            .iter()
            .any(|node| node.name() == "Much longer current child")
    );
    assert!(
        layout(&changed, &scoped("capture", "text"))
            .rect()
            .size()
            .width()
            > layout(&first, &scoped("capture", "text"))
                .rect()
                .size()
                .width()
    );
    drop((
        first_source,
        changed_source,
        description,
        cloned,
        independent,
        owner,
    ));
    assert!(weak.upgrade().is_none());
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    let invalid = snapshot(
        2,
        [UiRoot::new(
            root_key("capture"),
            UiLayer::Base,
            UiStyle::fill().with_gap(f32::NAN),
        )],
    );
    let retained = compile(&mut compiler, &invalid);
    assert_eq!(status(&retained, "capture"), UiRootStatus::RetainedVisual);
    assert_eq!(retained.paint(), changed.paint());
    assert!(retained.hit_regions().is_empty());
    assert!(weak.upgrade().is_none());
    assert!(!first.paint().is_empty() && !changed.paint().is_empty());
}

struct SwallowAndExhaust;

impl UiLayout for SwallowAndExhaust {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        assert!(
            children
                .measure(children.len(), UiLayoutConstraints::MAX_CONTENT)
                .is_err()
        );
        for _ in 0..64 {
            assert!(
                children
                    .measure(0, UiLayoutConstraints::MAX_CONTENT)
                    .is_err()
            );
        }
        Ok(UiLayoutOutput::new(size(1.0, 1.0)))
    }
}

#[test]
fn swallowed_local_error_cannot_mask_later_candidate_work_exhaustion() {
    let source = snapshot(
        1,
        [
            UiRoot::new(root_key("poison-budget"), UiLayer::Base, UiStyle::fill())
                .with_custom_layout(UiCustomLayout::new(SwallowAndExhaust))
                .with_child(UiNode::new(UiStyle::flex())),
        ],
    );
    let mut compiler = compiler(UiLimits::default().with_max_layout_work(16).unwrap());
    assert!(matches!(
        compiler.compile(&source, environment(1.0), UiTheme::default()),
        Err(UiCompileError::LayoutWorkLimitExceeded {
            actual: 17,
            maximum: 16
        })
    ));
    assert!(compiler.work_stats().layout_work_units > 16);
    assert_eq!(compiler.work_stats().published_frames, 0);
}

#[test]
fn exact_flex_allocations_reject_main_axis_padding_overflow_and_accept_the_exact_boundary() {
    for direction in [UiFlexDirection::Column, UiFlexDirection::Row] {
        let column = direction == UiFlexDirection::Column;
        let padding = if column {
            UiEdges::new(0.0, 0.0, 10.0, 10.0)
        } else {
            UiEdges::new(10.0, 10.0, 0.0, 0.0)
        }
        .unwrap();
        let make = |extent: f32, revision| {
            snapshot(
                revision,
                [
                    UiRoot::new(root_key("padding-boundary"), UiLayer::Base, UiStyle::fill())
                        .with_custom_layout(fixed(
                            vec![Some(if column {
                                rect(0.0, 0.0, 40.0, extent)
                            } else {
                                rect(0.0, 0.0, extent, 40.0)
                            })],
                            size(60.0, 60.0),
                        ))
                        .with_child(
                            button(
                                "padded-flex",
                                UiStyle::flex()
                                    .with_padding(padding)
                                    .with_flex_direction(direction),
                            )
                            .with_child(UiNode::new(UiStyle::flex())),
                        ),
                ],
            )
        };
        let mut compiler = compiler(Default::default());
        let rejected = compile(&mut compiler, &make(15.0, 1));
        assert_eq!(status(&rejected, "padding-boundary"), UiRootStatus::Absent);
        assert!(
            rejection(&rejected, "padding-boundary")
                .contains(&UiLayoutError::PlacementSizeMismatch { child: 0 }.to_string())
        );
        assert!(
            rejected.paint().is_empty()
                && rejected.hit_regions().is_empty()
                && rejected.focus_order().is_empty()
                && rejected.semantics().is_empty()
        );
        let accepted = compile(&mut compiler, &make(20.0, 2));
        assert_eq!(
            status(&accepted, "padding-boundary"),
            UiRootStatus::Published
        );
        assert!(accepted.diagnostics().is_empty());
        let size = layout(&accepted, &scoped("padding-boundary", "padded-flex"))
            .rect()
            .size();
        assert_eq!(if column { size.height() } else { size.width() }, 20.0);
    }
}
