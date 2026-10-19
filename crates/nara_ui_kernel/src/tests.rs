use std::sync::Arc;

use nara_text::bundled_font_face;

use crate::{
    UiAuthoringSnapshotBuilder, UiBrush, UiColorRole, UiCompiler, UiDimension, UiEdges,
    UiEnvironment, UiInteraction, UiLayer, UiLocalKey, UiNode, UiOverflow, UiPaintPrimitive,
    UiRect, UiRoot, UiRootKey, UiRootStatus, UiRouteStamp, UiSemanticRole, UiSemantics, UiStyle,
    UiSurfaceId, UiText, UiTextAlignment, UiTextOverflow, UiTheme, UiVisual,
};

#[test]
fn dynamic_text_content_preserves_authored_presentation() {
    let authored = UiText::new("Score", 16.0)
        .unwrap()
        .with_brush(UiBrush::Role(UiColorRole::Warning))
        .with_alignment(UiTextAlignment::Center)
        .with_overflow(UiTextOverflow::Clip)
        .with_semantic_text("Authored score");

    let projected = authored.clone().with_content("Score 000300");

    assert_eq!(projected.content(), "Score 000300");
    assert_eq!(projected.logical_size(), authored.logical_size());
    assert_eq!(projected.brush(), authored.brush());
    assert_eq!(projected.alignment(), authored.alignment());
    assert_eq!(projected.overflow(), authored.overflow());
    assert_eq!(projected.semantic_text(), authored.semantic_text());
}

#[test]
fn public_surface_compiles_without_renderer_or_ecs_types() {
    let root_key = UiRootKey::runtime("reference", "hud").unwrap();
    let node_key = UiLocalKey::runtime("reference", "score").unwrap();
    let root = UiRoot::new(root_key, UiLayer::Base, UiStyle::fill()).with_child(
        UiNode::new(UiStyle::absolute(12.0, 8.0, 160.0, 28.0))
            .with_key(node_key)
            .with_visual(UiVisual::solid(crate::UiColor::rgba(0.1, 0.2, 0.3, 1.0)))
            .with_text(
                UiText::new("Score", 16.0)
                    .unwrap()
                    .with_alignment(UiTextAlignment::Start),
            ),
    );
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root)
        .build()
        .unwrap();
    let surface = UiSurfaceId::new(7).unwrap();
    let environment = UiEnvironment::new(surface, 320.0, 180.0, 1.0, 2, 3, 1, 5).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    assert_eq!(frame.root_outcomes().len(), 1);
    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Published);
    assert_eq!(frame.paint().len(), 2);
    assert!(frame.hit_test(crate::UiPoint::new(20.0, 20.0)).is_none());

    let (mut journal, authority) =
        crate::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(surface, environment.slot_generation().get())
        .unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(&frame), 10)
        .unwrap();
    assert_eq!(
        journal
            .resolve(UiRouteStamp::from_receipt(receipt))
            .unwrap()
            .revision(),
        frame.revision()
    );
}

#[test]
fn percent_layout_is_logical_and_scale_independent() {
    let root = UiRoot::new(
        UiRootKey::runtime("test", "root").unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_child(
        UiNode::new(
            UiStyle::absolute(0.0, 0.0, 0.5, 0.5)
                .with_width(UiDimension::percent(0.5).unwrap())
                .with_height(UiDimension::percent(0.5).unwrap()),
        )
        .with_key(UiLocalKey::runtime("test", "child").unwrap()),
    );
    let snapshot = UiAuthoringSnapshotBuilder::new(11)
        .with_root(root)
        .build()
        .unwrap();
    let surface = UiSurfaceId::new(1).unwrap();
    let face = bundled_font_face().unwrap();
    let mut compiler = UiCompiler::new(face, Default::default()).unwrap();
    let first = compiler
        .compile(
            &snapshot,
            UiEnvironment::new(surface, 200.0, 100.0, 1.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    let second = compiler
        .compile(
            &snapshot,
            UiEnvironment::new(surface, 200.0, 100.0, 2.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    assert_eq!(first.layouts()[1].rect(), second.layouts()[1].rect());
    assert_ne!(
        first.layouts()[1].physical_rect(),
        second.layouts()[1].physical_rect()
    );
}

#[test]
fn padded_text_paints_the_same_content_box_used_for_auto_height() {
    let mut text_engine =
        nara_text::TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let mut previous_lines = 0;
    for padding in [0.0, 20.0] {
        let text = UiText::new("One two three four five six seven eight", 16.0).unwrap();
        let width = 100.0 - padding * 2.0;
        let expected = text_engine
            .layout(&text.request(Some(width), 1.0).unwrap())
            .unwrap();
        let node = UiNode::new(
            UiStyle::absolute(10.0, 12.0, 100.0, 1.0)
                .with_height(UiDimension::auto())
                .with_padding(UiEdges::symmetric(padding, 5.0).unwrap()),
        )
        .with_key(UiLocalKey::runtime("text-box", "auto-height").unwrap())
        .with_text(text);
        let frame = compile_text_box(node, 1.0);
        let outer = frame.layouts()[1].rect();
        let UiPaintPrimitive::Text {
            rect,
            physical_rect,
            layout,
            ..
        } = &frame.paint()[0]
        else {
            panic!("the text node must publish text paint")
        };
        assert_eq!(
            *rect,
            UiRect::from_origin_size(10.0 + padding, 17.0, width, expected.height()).unwrap()
        );
        assert_eq!(*physical_rect, frame.environment().snap(*rect));
        assert_eq!(layout.as_ref(), &expected);
        assert_eq!(outer.size().height(), layout.height() + 10.0);
        assert!(layout.lines().len() > previous_lines);
        previous_lines = layout.lines().len();
    }
}

#[test]
fn asymmetric_padded_flow_text_sizes_its_flex_parent_and_following_sibling() {
    let text = UiText::new("One two three four five six seven eight", 16.0).unwrap();
    let mut text_engine =
        nara_text::TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let expected = text_engine
        .layout(&text.request(Some(60.0), 1.0).unwrap())
        .unwrap();
    let label = UiNode::new(
        UiStyle::flex()
            .with_width(UiDimension::px(100.0).unwrap())
            .with_padding(UiEdges::new(8.0, 32.0, 3.0, 9.0).unwrap()),
    )
    .with_key(UiLocalKey::runtime("text-box", "flow-label").unwrap())
    .with_text(text);
    let sibling = UiNode::new(
        UiStyle::flex()
            .with_width(UiDimension::px(100.0).unwrap())
            .with_height(UiDimension::px(8.0).unwrap()),
    );
    let parent = UiNode::new(
        UiStyle::absolute(10.0, 12.0, 140.0, 1.0)
            .with_height(UiDimension::auto())
            .with_padding(UiEdges::new(7.0, 13.0, 9.0, 11.0).unwrap()),
    )
    .with_children([label, sibling]);
    let frame = compile_text_box(parent, 1.0);
    let UiPaintPrimitive::Text { rect, layout, .. } = &frame.paint()[0] else {
        panic!("the flow label must publish text")
    };
    assert_eq!(
        *rect,
        UiRect::from_origin_size(25.0, 24.0, 60.0, expected.height()).unwrap()
    );
    assert_eq!(layout.as_ref(), &expected);
    let label_rect = frame.layouts()[2].rect();
    let sibling_rect = frame.layouts()[3].rect();
    assert_eq!(label_rect.size().height(), expected.height() + 12.0);
    assert_eq!(sibling_rect.origin().y(), label_rect.max().y());
    assert_eq!(
        frame.layouts()[1].rect().size().height(),
        expected.height() + 40.0
    );
}

#[test]
fn padded_button_text_clips_inside_nested_content_without_shrinking_hit_bounds() {
    let button_key = UiLocalKey::runtime("text-box", "button").unwrap();
    let button = UiNode::new(
        UiStyle::absolute(40.0, 0.0, 100.0, 50.0)
            .with_padding(UiEdges::symmetric(20.0, 5.0).unwrap()),
    )
    .with_key(button_key.clone())
    .with_visual(UiVisual::role(UiColorRole::Panel))
    .with_text(
        UiText::new("Padded button", 16.0)
            .unwrap()
            .with_overflow(UiTextOverflow::Clip),
    )
    .with_interaction(UiInteraction::focus_target())
    .with_semantics(UiSemantics::new(UiSemanticRole::Button, "Padded button").unwrap());
    let parent =
        UiNode::new(UiStyle::absolute(10.0, 10.0, 70.0, 30.0).with_overflow(UiOverflow::Clip))
            .with_child(button);
    for scale in [1.0, 1.25] {
        let frame = compile_text_box(parent.clone(), scale);
        let outer = UiRect::from_origin_size(50.0, 10.0, 100.0, 50.0).unwrap();
        let content = UiRect::from_origin_size(70.0, 15.0, 60.0, 40.0).unwrap();
        let clip = UiRect::from_origin_size(70.0, 15.0, 10.0, 25.0).unwrap();
        assert_eq!(frame.layouts()[2].rect(), outer);
        assert_eq!(frame.paint()[0].rect(), outer);
        let UiPaintPrimitive::Text {
            rect,
            physical_rect,
            clip_rect,
            ..
        } = &frame.paint()[1]
        else {
            panic!("the button background must precede its text")
        };
        assert_eq!(*rect, content);
        assert_eq!(*physical_rect, frame.environment().snap(content));
        assert_eq!(*clip_rect, Some(clip));
        assert_eq!(frame.hit_regions()[0].rect(), outer);
        assert_eq!(frame.focus_order()[0].rect(), outer);
        assert_eq!(frame.semantics()[0].rect(), outer);
        assert_eq!(
            frame
                .hit_test(crate::UiPoint::new(55.0, 12.0))
                .unwrap()
                .key()
                .local_key(),
            Some(&button_key)
        );
    }
}

#[test]
fn padded_text_with_no_visible_content_keeps_outer_interaction() {
    for padding in [20.0, 50.0] {
        let button = UiNode::new(
            UiStyle::absolute(40.0, 0.0, 100.0, 50.0)
                .with_padding(UiEdges::symmetric(padding, 5.0).unwrap()),
        )
        .with_key(UiLocalKey::runtime("text-box", "empty-content").unwrap())
        .with_visual(UiVisual::role(UiColorRole::Panel))
        .with_text(
            UiText::new("Invisible text", 16.0)
                .unwrap()
                .with_overflow(UiTextOverflow::Clip),
        )
        .with_interaction(UiInteraction::hit_target());
        let parent =
            UiNode::new(UiStyle::absolute(10.0, 10.0, 50.0, 30.0).with_overflow(UiOverflow::Clip))
                .with_child(button);
        let frame = compile_text_box(parent, 1.0);
        assert_eq!(
            frame.paint().len(),
            1,
            "empty or fully clipped content must not publish text"
        );
        assert!(matches!(frame.paint()[0], UiPaintPrimitive::Rect { .. }));
        assert!(frame.hit_test(crate::UiPoint::new(55.0, 12.0)).is_some());
    }
}

fn compile_text_box(node: UiNode, scale: f32) -> Arc<crate::UiFrame> {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("text-box", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(node),
        )
        .build()
        .unwrap();
    let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(
            &snapshot,
            UiEnvironment::new(
                UiSurfaceId::new(1).unwrap(),
                320.0,
                180.0,
                scale,
                1,
                1,
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    assert!(
        frame.diagnostics().is_empty(),
        "text-box compilation rejected: {:?}",
        frame.diagnostics()
    );
    assert!(!frame.layouts().is_empty());
    frame
}
