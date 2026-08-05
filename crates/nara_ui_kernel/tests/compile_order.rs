use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiDimension, UiEdges, UiEnvironment, UiFlexDirection,
    UiImageId, UiInteraction, UiLayer, UiLocalKey, UiNode, UiOverflow, UiPoint, UiResolvedBrush,
    UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
};

fn environment(scale_factor: f32) -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(1).unwrap(),
        200.0,
        100.0,
        scale_factor,
        1,
        1,
        1,
        1,
    )
    .unwrap()
}

fn compiler() -> UiCompiler {
    UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap()
}

fn key(value: &str) -> UiLocalKey {
    UiLocalKey::runtime("test", value).unwrap()
}

fn root_key(value: &str) -> UiRootKey {
    UiRootKey::runtime("test", value).unwrap()
}

fn hit_root(name: &str, color: f32) -> UiRoot {
    UiRoot::new(
        root_key(name),
        UiLayer::Base,
        UiStyle::absolute(0.0, 0.0, 100.0, 100.0),
    )
    .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
        color, 0.2, 0.3, 1.0,
    )))
    .with_interaction(UiInteraction::hit_target())
}

#[test]
fn same_layer_root_sequence_is_explicit_and_not_key_sorted() {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(hit_root("z-first", 0.1))
        .with_root(hit_root("a-second", 0.2))
        .build()
        .unwrap();
    let mut compiler = compiler();
    let frame = compiler
        .compile(&snapshot, environment(1.0), UiTheme::default())
        .unwrap();

    assert_eq!(
        frame.paint()[0].key().unwrap().root_key(),
        &root_key("z-first")
    );
    assert_eq!(
        frame.paint()[1].key().unwrap().root_key(),
        &root_key("a-second")
    );
    assert_eq!(
        frame
            .hit_test(UiPoint::new(10.0, 10.0))
            .unwrap()
            .key()
            .root_key(),
        &root_key("a-second")
    );
}

#[test]
fn local_z_then_structural_order_drives_paint_and_reverse_hit() {
    let low = UiNode::new(UiStyle::absolute(0.0, 0.0, 80.0, 80.0).with_z_index(0))
        .with_key(key("low"))
        .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
            0.1, 0.2, 0.3, 1.0,
        )))
        .with_interaction(UiInteraction::hit_target());
    let high = UiNode::new(UiStyle::absolute(0.0, 0.0, 80.0, 80.0).with_z_index(2))
        .with_key(key("high"))
        .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
            0.3, 0.2, 0.1, 1.0,
        )))
        .with_interaction(UiInteraction::hit_target());
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key("root"), UiLayer::Base, UiStyle::fill())
                .with_children([high, low]),
        )
        .build()
        .unwrap();
    let mut compiler = compiler();
    let frame = compiler
        .compile(&snapshot, environment(1.0), UiTheme::default())
        .unwrap();

    let paint_keys = frame
        .paint()
        .iter()
        .filter_map(|primitive| primitive.key())
        .collect::<Vec<_>>();
    assert_eq!(paint_keys[0].local_key(), Some(&key("low")));
    assert_eq!(paint_keys[1].local_key(), Some(&key("high")));
    assert_eq!(
        frame
            .hit_test(UiPoint::new(10.0, 10.0))
            .unwrap()
            .key()
            .local_key(),
        Some(&key("high"))
    );
}

#[test]
fn inherited_clip_is_shared_by_paint_and_hit_testing() {
    let child = UiNode::new(UiStyle::absolute(30.0, 30.0, 50.0, 50.0))
        .with_key(key("child"))
        .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
            0.3, 0.4, 0.5, 1.0,
        )))
        .with_interaction(UiInteraction::hit_target());
    let clip = UiNode::new(UiStyle::absolute(0.0, 0.0, 50.0, 50.0).with_overflow(UiOverflow::Clip))
        .with_child(child);
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(UiRoot::new(root_key("root"), UiLayer::Base, UiStyle::fill()).with_child(clip))
        .build()
        .unwrap();
    let mut compiler = compiler();
    let frame = compiler
        .compile(&snapshot, environment(1.0), UiTheme::default())
        .unwrap();

    assert!(frame.hit_test(UiPoint::new(40.0, 40.0)).is_some());
    assert!(frame.hit_test(UiPoint::new(60.0, 40.0)).is_none());
    assert!(frame.paint().last().unwrap().clip_rect().is_some());
}

#[test]
fn logical_geometry_is_stable_across_fractional_scales_and_physical_snapping_changes() {
    let child = UiNode::new(
        UiStyle::absolute(3.0, 5.0, 0.0, 0.0)
            .with_width(UiDimension::percent(0.5).unwrap())
            .with_height(UiDimension::percent(0.5).unwrap()),
    )
    .with_key(key("child"));
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(UiRoot::new(root_key("root"), UiLayer::Base, UiStyle::fill()).with_child(child))
        .build()
        .unwrap();
    let mut compiler = compiler();
    let frames = [1.0, 1.25, 1.5, 2.0]
        .into_iter()
        .map(|scale| {
            compiler
                .compile(&snapshot, environment(scale), UiTheme::default())
                .unwrap()
        })
        .collect::<Vec<_>>();

    for frame in &frames[1..] {
        assert_eq!(frames[0].layouts()[1].rect(), frame.layouts()[1].rect());
    }
    assert_ne!(
        frames[0].layouts()[1].physical_rect(),
        frames[1].layouts()[1].physical_rect()
    );
}

#[test]
fn nested_flex_and_absolute_layout_share_one_logical_tree() {
    let absolute = UiNode::new(UiStyle::absolute(5.0, 3.0, 10.0, 8.0)).with_key(key("absolute"));
    let first = UiNode::new(
        UiStyle::flex()
            .with_width(UiDimension::px(40.0).unwrap())
            .with_height(UiDimension::px(20.0).unwrap())
            .with_flex_shrink(0.0),
    )
    .with_key(key("first"));
    let second = UiNode::new(
        UiStyle::flex()
            .with_width(UiDimension::px(30.0).unwrap())
            .with_height(UiDimension::px(20.0).unwrap())
            .with_flex_shrink(0.0),
    )
    .with_key(key("second"))
    .with_child(absolute);
    let root_style = UiStyle::fill()
        .with_flex_direction(UiFlexDirection::Row)
        .with_padding(UiEdges::all(10.0).unwrap())
        .with_gap(5.0);
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key("root"), UiLayer::Base, root_style).with_children([first, second]),
        )
        .build()
        .unwrap();
    let mut compiler = compiler();
    let frame = compiler
        .compile(&snapshot, environment(1.0), UiTheme::default())
        .unwrap();

    assert_eq!(frame.layouts()[1].rect().origin(), UiPoint::new(10.0, 10.0));
    assert_eq!(frame.layouts()[2].rect().origin(), UiPoint::new(55.0, 10.0));
    assert_eq!(frame.layouts()[3].rect().origin(), UiPoint::new(60.0, 13.0));
}

#[test]
fn typed_layers_override_input_sequence() {
    let colored = |name, layer, color| {
        UiRoot::new(root_key(name), layer, UiStyle::fill()).with_visual(UiVisual::solid(
            nara_ui_kernel::UiColor::rgba(color, 0.2, 0.3, 1.0),
        ))
    };
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(colored("modal", UiLayer::Modal, 0.1))
        .with_root(colored("base", UiLayer::Base, 0.2))
        .with_root(colored("overlay", UiLayer::Overlay, 0.3))
        .build()
        .unwrap();
    let mut compiler = compiler();
    let frame = compiler
        .compile(&snapshot, environment(1.0), UiTheme::default())
        .unwrap();
    let keys = frame
        .paint()
        .iter()
        .map(|primitive| primitive.key().unwrap().root_key())
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        vec![&root_key("base"), &root_key("overlay"), &root_key("modal")]
    );
}

#[test]
fn ancestor_stacking_context_orders_its_subtree_as_one_unit() {
    let child = |name: &str, z| {
        UiNode::new(UiStyle::absolute(0.0, 0.0, 40.0, 40.0).with_z_index(z))
            .with_key(key(name))
            .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
                0.2, 0.3, 0.4, 1.0,
            )))
    };
    let low_context = UiNode::new(
        UiStyle::absolute(0.0, 0.0, 40.0, 40.0)
            .with_z_index(0)
            .with_stacking_context(true),
    )
    .with_key(key("low-context"))
    .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
        0.1, 0.1, 0.1, 1.0,
    )))
    .with_child(child("low-child", 100));
    let high_context = UiNode::new(
        UiStyle::absolute(0.0, 0.0, 40.0, 40.0)
            .with_z_index(1)
            .with_stacking_context(true),
    )
    .with_key(key("high-context"))
    .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
        0.2, 0.2, 0.2, 1.0,
    )))
    .with_child(child("high-child", -100));
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key("root"), UiLayer::Base, UiStyle::fill())
                .with_children([high_context, low_context]),
        )
        .build()
        .unwrap();
    let mut compiler = compiler();
    let frame = compiler
        .compile(&snapshot, environment(1.0), UiTheme::default())
        .unwrap();
    let keys = frame
        .paint()
        .iter()
        .map(|primitive| primitive.key().unwrap().local_key().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        vec![
            &key("low-context"),
            &key("low-child"),
            &key("high-context"),
            &key("high-child"),
        ]
    );
}

#[test]
fn image_brush_retains_backend_neutral_identity() {
    let image = UiImageId::new("project://ui/terminal-panel").unwrap();
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key("root"), UiLayer::Base, UiStyle::fill()).with_visual(
                UiVisual::image(
                    image.clone(),
                    nara_ui_kernel::UiColor::rgba(1.0, 1.0, 1.0, 1.0),
                ),
            ),
        )
        .build()
        .unwrap();
    let mut compiler = compiler();
    let frame = compiler
        .compile(&snapshot, environment(1.0), UiTheme::default())
        .unwrap();
    let nara_ui_kernel::UiPaintPrimitive::Rect {
        brush: Some(UiResolvedBrush::Image { image: actual, .. }),
        ..
    } = &frame.paint()[0]
    else {
        panic!("expected an image-backed rect primitive");
    };
    assert_eq!(actual, &image);
}
