use std::sync::Arc;

use nara_text::bundled_font_face;

use crate::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiDimension, UiEnvironment, UiLayer, UiLocalKey,
    UiNode, UiRoot, UiRootKey, UiRootStatus, UiRouteStamp, UiStyle, UiSurfaceId, UiText,
    UiTextAlignment, UiTheme, UiVisual,
};

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
