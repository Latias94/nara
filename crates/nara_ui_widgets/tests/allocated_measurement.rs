use std::sync::Arc;

use nara_text::{TextEngine, TextLayout, TextLayoutRequest, TextTypography, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiCustomLayout, UiDimension, UiEdges,
    UiEnvironment, UiFrame, UiKey, UiLayer, UiLocalKey, UiNode, UiPaintPrimitive, UiRect, UiRoot,
    UiRootKey, UiRootStatus, UiSize, UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
};
use nara_ui_widgets::{UiAnchoredPopup, UiControlGrid, UiIconLabel, UiPropertyRow, UiTextField};

const WORDS: &str = "One two three four five six seven eight nine ten.";
const GAP: f32 = 6.0;

fn root() -> UiRootKey {
    UiRootKey::runtime("allocated-measurement", "widgets").unwrap()
}

fn local(name: &str) -> UiLocalKey {
    UiLocalKey::runtime("allocated-measurement", name).unwrap()
}

fn scoped(name: &str) -> UiKey {
    UiKey::scoped(root(), local(name)).unwrap()
}

fn typography() -> TextTypography {
    TextTypography::new(14.0)
        .unwrap()
        .with_line_height(24.0)
        .unwrap()
}

fn text(name: &str, content: &str, width: UiDimension) -> UiNode {
    UiNode::new(
        UiStyle::flex()
            .with_width(width)
            .with_padding(UiEdges::new(4.0, 4.0, 3.0, 5.0).unwrap()),
    )
    .with_key(local(name))
    .with_text(UiText::from_typography(content, typography()))
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

fn compile(node: UiRoot, width: f32) -> Arc<UiFrame> {
    let source = UiAuthoringSnapshotBuilder::new(1)
        .with_root(node)
        .build()
        .unwrap();
    let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(
            &source,
            UiEnvironment::new(UiSurfaceId::new(98).unwrap(), width, 480.0, 1.0, 1, 1, 1, 1)
                .unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    assert_eq!(
        frame.root_outcomes()[0].status(),
        UiRootStatus::Published,
        "{:?}",
        frame.diagnostics()
    );
    frame
}

fn rect(frame: &UiFrame, key: &UiKey) -> UiRect {
    frame
        .layouts()
        .iter()
        .find(|fact| fact.key() == Some(key))
        .unwrap()
        .rect()
}

fn painted<'a>(frame: &'a UiFrame, key: &UiKey) -> (UiRect, &'a TextLayout) {
    frame
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Text {
                key: Some(painted),
                rect,
                layout,
                ..
            } if painted == key => Some((*rect, layout.as_ref())),
            _ => None,
        })
        .unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

fn text_bottom(draw: UiRect, layout: &TextLayout) -> f32 {
    draw.origin().y()
        + layout
            .lines()
            .iter()
            .map(|line| line.block_max())
            .fold(0.0_f32, f32::max)
}

#[test]
fn property_and_icon_labels_wrap_at_allocated_width_without_intruding_on_the_field() {
    for width in [UiDimension::Px(400.0), UiDimension::Percent(0.5)] {
        for with_icon in [false, true] {
            let mut engine =
                TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
            let field = UiTextField::with_typography(
                root(),
                "Value",
                "retained value",
                typography(),
                &mut engine,
            )
            .unwrap();
            let label = text("label", WORDS, width);
            let label = if with_icon {
                UiNode::new(UiStyle::flex())
                    .with_key(local("icon-label"))
                    .with_custom_layout(UiCustomLayout::new(
                        UiIconLabel::new(UiSize::new(16.0, 16.0).unwrap(), -12.0, GAP).unwrap(),
                    ))
                    .with_children([
                        UiNode::new(UiStyle::flex())
                            .with_key(local("icon"))
                            .with_visual(UiVisual::solid(UiColor::rgba(0.2, 0.6, 0.7, 1.0))),
                        label,
                    ])
            } else {
                label
            };
            let row = UiNode::new(UiStyle::flex().with_width(UiDimension::Px(100.0)))
                .with_key(local("row"))
                .with_custom_layout(UiCustomLayout::new(UiPropertyRow::new(120.0, GAP).unwrap()))
                .with_children([
                    label,
                    field
                        .authored_node(&root(), UiStyle::flex(), 220.0)
                        .unwrap(),
                    text("action", "Apply", UiDimension::Auto),
                    text("status", "Draft", UiDimension::Auto),
                ]);
            let frame = compile(
                UiRoot::new(root(), UiLayer::Base, UiStyle::fill()).with_child(row),
                320.0,
            );
            let label_rect = rect(&frame, &scoped("label"));
            let (draw, shaped) = painted(&frame, &scoped("label"));
            let expected = oracle(if with_icon { 70.0 } else { 92.0 });
            assert!(shaped.lines().len() > 1);
            assert_eq!(shaped.lines(), expected.lines());
            close(label_rect.size().height(), expected.height() + 8.0);
            let field_rect = rect(&frame, field.key());
            assert!(text_bottom(draw, shaped) + 5.0 + GAP <= field_rect.origin().y() + 0.001);
            assert!(label_rect.max().y() + GAP <= field_rect.origin().y() + 0.001);
            if with_icon {
                let icon = rect(&frame, &scoped("icon"));
                close(
                    icon.origin().y() + 12.0,
                    draw.origin().y() + shaped.lines()[0].baseline(),
                );
                close(
                    rect(&frame, &scoped("icon-label")).size().height(),
                    label_rect.size().height(),
                );
            }
            let row_rect = rect(&frame, &scoped("row"));
            assert!(
                rect(&frame, &scoped("action")).origin().y() >= field_rect.max().y() + GAP - 0.001
            );
            assert!(rect(&frame, &scoped("status")).max().y() <= row_rect.max().y() + 0.001);
            assert_eq!(field.editor().text(), "retained value");
        }
    }
}

#[test]
fn grid_and_popup_reserve_wrapped_height_for_pixel_and_percentage_width_children() {
    let expected = oracle(92.0);
    assert!(expected.lines().len() > 1);
    for width in [UiDimension::Px(400.0), UiDimension::Percent(0.5)] {
        let grid = compile(
            UiRoot::new(root(), UiLayer::Base, UiStyle::fill())
                .with_custom_layout(UiCustomLayout::new(UiControlGrid::new(80.0, GAP).unwrap()))
                .with_children([
                    text("label", WORDS, width),
                    text("following", "Next", UiDimension::Auto),
                ]),
            100.0,
        );
        let first = rect(&grid, &scoped("label"));
        let following = rect(&grid, &scoped("following"));
        let (draw, shaped) = painted(&grid, &scoped("label"));
        assert_eq!(shaped.lines(), expected.lines());
        close(first.size().height(), expected.height() + 8.0);
        close(following.origin().y(), first.max().y() + GAP);
        assert!(text_bottom(draw, shaped) + 5.0 + GAP <= following.origin().y() + 0.001);

        let popup = compile(
            UiRoot::new(root(), UiLayer::Base, UiStyle::fill())
                .with_custom_layout(UiCustomLayout::new(
                    UiAnchoredPopup::new(
                        UiRect::from_origin_size(80.0, 10.0, 20.0, 14.0).unwrap(),
                        100.0,
                        8.0,
                        4.0,
                    )
                    .unwrap(),
                ))
                .with_child(text("label", WORDS, width)),
            116.0,
        );
        let outer = rect(&popup, &scoped("label"));
        let (draw, shaped) = painted(&popup, &scoped("label"));
        assert_eq!(shaped.lines(), expected.lines());
        close(outer.size().width(), 100.0);
        close(outer.size().height(), expected.height() + 8.0);
        close(outer.origin().y(), 28.0);
        assert!(text_bottom(draw, shaped) + 5.0 <= outer.max().y() + 0.001);
    }
}
