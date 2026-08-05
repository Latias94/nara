use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiInteraction, UiLayer, UiLocalKey,
    UiNode, UiPoint, UiRoot, UiRootKey, UiRootStatus, UiSemanticRole, UiSemantics, UiStyle,
    UiSurfaceId, UiTheme, UiVisual,
};

fn environment(scale: f32) -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(9).unwrap(),
        320.0,
        180.0,
        scale,
        1,
        1,
        1,
        1,
    )
    .unwrap()
}

fn root_key() -> UiRootKey {
    UiRootKey::runtime("reference", "terminal").unwrap()
}

fn local_key() -> UiLocalKey {
    UiLocalKey::runtime("reference", "retry").unwrap()
}

fn valid_root() -> UiRoot {
    UiRoot::new(root_key(), UiLayer::Modal, UiStyle::fill()).with_child(
        UiNode::new(UiStyle::absolute(100.0, 60.0, 120.0, 44.0))
            .with_key(local_key())
            .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
                0.1, 0.3, 0.5, 1.0,
            )))
            .with_interaction(UiInteraction::focus_target())
            .with_semantics(
                UiSemantics::new(UiSemanticRole::Button, "Retry")
                    .unwrap()
                    .with_action(nara_ui_kernel::UiActionId::new("retry").unwrap())
                    .unwrap(),
            ),
    )
}

fn duplicate_key_root() -> UiRoot {
    let node = || {
        UiNode::new(UiStyle::absolute(100.0, 60.0, 120.0, 44.0))
            .with_key(local_key())
            .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
                0.5, 0.2, 0.1, 1.0,
            )))
            .with_interaction(UiInteraction::focus_target())
    };
    UiRoot::new(root_key(), UiLayer::Modal, UiStyle::fill()).with_children([node(), node()])
}

#[test]
fn failed_root_retains_only_environment_compatible_visuals() {
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let valid = UiAuthoringSnapshotBuilder::new(1)
        .with_root(valid_root())
        .build()
        .unwrap();
    let first = compiler
        .compile(&valid, environment(1.0), UiTheme::default())
        .unwrap();
    assert_eq!(first.root_outcomes()[0].status(), UiRootStatus::Published);
    assert!(first.hit_test(UiPoint::new(110.0, 70.0)).is_some());
    assert_eq!(first.focus_order().len(), 1);
    assert_eq!(first.semantics().len(), 1);

    let invalid = UiAuthoringSnapshotBuilder::new(2)
        .with_root(duplicate_key_root())
        .build()
        .unwrap();
    let retained = compiler
        .compile(&invalid, environment(1.0), UiTheme::default())
        .unwrap();
    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert_eq!(retained.paint(), first.paint());
    assert!(retained.hit_regions().is_empty());
    assert!(retained.focus_order().is_empty());
    assert!(retained.semantics().is_empty());

    let incompatible = compiler
        .compile(&invalid, environment(1.25), UiTheme::default())
        .unwrap();
    assert_eq!(
        incompatible.root_outcomes()[0].status(),
        UiRootStatus::Absent
    );
    assert!(incompatible.paint().is_empty());
}

#[test]
fn invalid_root_does_not_block_an_unrelated_valid_root() {
    let other = UiRoot::new(
        UiRootKey::runtime("reference", "hud").unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
        0.2, 0.2, 0.2, 1.0,
    )));
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(duplicate_key_root())
        .with_root(other)
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot, environment(1.0), UiTheme::default())
        .unwrap();

    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Absent);
    assert_eq!(frame.root_outcomes()[1].status(), UiRootStatus::Published);
    assert_eq!(frame.paint().len(), 1);
}

#[test]
fn removing_a_root_retires_its_last_good_candidate() {
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let valid = UiAuthoringSnapshotBuilder::new(1)
        .with_root(valid_root())
        .build()
        .unwrap();
    compiler
        .compile(&valid, environment(1.0), UiTheme::default())
        .unwrap();
    compiler
        .compile(
            &UiAuthoringSnapshotBuilder::new(2).build().unwrap(),
            environment(1.0),
            UiTheme::default(),
        )
        .unwrap();
    let invalid = UiAuthoringSnapshotBuilder::new(3)
        .with_root(duplicate_key_root())
        .build()
        .unwrap();
    let frame = compiler
        .compile(&invalid, environment(1.0), UiTheme::default())
        .unwrap();
    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Absent);
}

#[test]
fn required_interaction_without_a_key_fails_closed() {
    let root = UiRoot::new(root_key(), UiLayer::Base, UiStyle::fill()).with_child(
        UiNode::new(UiStyle::absolute(0.0, 0.0, 20.0, 20.0))
            .with_interaction(UiInteraction::hit_target()),
    );
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root)
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot, environment(1.0), UiTheme::default())
        .unwrap();
    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Absent);
    assert!(frame.hit_regions().is_empty());
}
