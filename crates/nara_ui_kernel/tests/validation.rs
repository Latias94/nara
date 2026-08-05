use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringError, UiAuthoringSnapshotBuilder, UiCompiler, UiDiagnostic, UiEnvironment,
    UiEnvironmentError, UiInteraction, UiLayer, UiLimits, UiNode, UiRoot, UiRootKey, UiStyle,
    UiSurfaceId, UiTheme,
};

#[test]
fn builder_rejects_non_finite_style_before_publication() {
    let root = UiRoot::new(
        UiRootKey::runtime("test", "root").unwrap(),
        UiLayer::Base,
        UiStyle::absolute(f32::NAN, 0.0, 10.0, 10.0),
    );
    assert!(matches!(
        UiAuthoringSnapshotBuilder::new(1).with_root(root).build(),
        Err(UiAuthoringError::Style(_))
    ));
}

#[test]
fn builder_limits_may_only_reduce_hard_ceilings() {
    assert!(UiLimits::default().with_max_nodes(0).is_err());
    assert!(UiLimits::default().with_max_nodes(4_097).is_err());
    let limits = UiLimits::default().with_max_nodes(1).unwrap();
    let root = UiRoot::new(
        UiRootKey::runtime("test", "root").unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_child(UiNode::new(UiStyle::fill()));
    assert!(matches!(
        UiAuthoringSnapshotBuilder::new(1)
            .with_limits(limits)
            .with_root(root)
            .build(),
        Err(UiAuthoringError::NodeLimitExceeded { .. })
    ));
}

#[test]
fn environment_rejects_invalid_extent_scale_and_zero_revisions() {
    let surface = UiSurfaceId::new(1).unwrap();
    assert!(matches!(
        UiEnvironment::new(surface, 0.0, 100.0, 1.0, 1, 1, 1, 1),
        Err(UiEnvironmentError::InvalidLogicalSize { .. })
    ));
    assert!(matches!(
        UiEnvironment::new(surface, 100.0, 100.0, f32::INFINITY, 1, 1, 1, 1),
        Err(UiEnvironmentError::InvalidScaleFactor { .. })
    ));
    assert!(matches!(
        UiEnvironment::new(surface, 100.0, 100.0, 1.0, 0, 1, 1, 1),
        Err(UiEnvironmentError::ZeroRevision { .. })
    ));
}

#[test]
fn theme_revision_mismatch_prevents_compilation() {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(UiRoot::new(
            UiRootKey::runtime("test", "root").unwrap(),
            UiLayer::Base,
            UiStyle::fill(),
        ))
        .build()
        .unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 100.0, 100.0, 1.0, 1, 1, 2, 1).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    assert!(
        compiler
            .compile(&snapshot, environment, UiTheme::default())
            .is_err()
    );
}

#[test]
fn root_diagnostics_are_bounded_and_report_truncation() {
    let invalid_root = |name: &str| {
        UiRoot::new(
            UiRootKey::runtime("test", name).unwrap(),
            UiLayer::Base,
            UiStyle::fill(),
        )
        .with_child(
            UiNode::new(UiStyle::absolute(0.0, 0.0, 10.0, 10.0))
                .with_interaction(UiInteraction::hit_target()),
        )
    };
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(invalid_root("one"))
        .with_root(invalid_root("two"))
        .with_root(invalid_root("three"))
        .build()
        .unwrap();
    let limits = UiLimits::default().with_max_diagnostics(2).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap();
    let frame = compiler
        .compile(
            &snapshot,
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 100.0, 100.0, 1.0, 1, 1, 1, 1)
                .unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    assert_eq!(frame.diagnostics().len(), 2);
    assert!(matches!(
        frame.diagnostics().last(),
        Some(UiDiagnostic::Truncated { omitted: 2 })
    ));
}
