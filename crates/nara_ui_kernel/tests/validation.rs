use nara_text::{TextLimits, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringError, UiAuthoringSnapshotBuilder, UiCompileError, UiCompiler, UiDiagnostic,
    UiEnvironment, UiEnvironmentError, UiInteraction, UiLayer, UiLimits, UiNode, UiRoot, UiRootKey,
    UiRootStatus, UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
};

#[test]
fn builder_preserves_a_known_root_identity_when_its_body_is_invalid() {
    let invalid = UiRoot::new(
        UiRootKey::runtime("test", "root").unwrap(),
        UiLayer::Base,
        UiStyle::absolute(f32::NAN, 0.0, 10.0, 10.0),
    );
    let valid = UiRoot::new(
        UiRootKey::runtime("test", "sibling").unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
        0.1, 0.2, 0.3, 1.0,
    )));
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(invalid)
        .with_root(valid)
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(
            &snapshot,
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 100.0, 100.0, 1.0, 1, 1, 1, 1)
                .unwrap(),
            UiTheme::default(),
        )
        .unwrap();

    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Absent);
    assert_eq!(frame.root_outcomes()[1].status(), UiRootStatus::Published);
    assert_eq!(frame.paint().len(), 1);
}

#[test]
fn duplicate_root_identity_rejects_the_whole_snapshot() {
    let key = UiRootKey::runtime("test", "root").unwrap();
    let result = UiAuthoringSnapshotBuilder::new(1)
        .with_root(UiRoot::new(key.clone(), UiLayer::Base, UiStyle::fill()))
        .with_root(UiRoot::new(key.clone(), UiLayer::Overlay, UiStyle::fill()))
        .build();
    assert!(matches!(
        result,
        Err(UiAuthoringError::DuplicateRootKey { key: duplicate }) if duplicate == key
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

#[test]
fn shaped_glyph_and_logical_paint_budgets_are_candidate_wide() {
    let text_root = |name: &str, text: &str| {
        UiRoot::new(
            UiRootKey::runtime("test", name).unwrap(),
            UiLayer::Base,
            UiStyle::fill(),
        )
        .with_text(UiText::new(text, 12.0).unwrap())
    };
    let text_snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(text_root("one", "A"))
        .with_root(text_root("two", "B"))
        .build()
        .unwrap();
    let text_limits = TextLimits {
        max_glyphs: 1,
        ..TextLimits::default()
    };
    let mut text_compiler = UiCompiler::with_text_limits(
        bundled_font_face().unwrap(),
        UiLimits::default(),
        text_limits,
    )
    .unwrap();
    assert!(matches!(
        text_compiler.compile(
            &text_snapshot,
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 100.0, 100.0, 1.0, 1, 1, 1, 1)
                .unwrap(),
            UiTheme::default(),
        ),
        Err(UiCompileError::ShapedGlyphLimitExceeded {
            actual: 2,
            maximum: 1
        })
    ));

    let painted_root = |name: &str| {
        UiRoot::new(
            UiRootKey::runtime("test", name).unwrap(),
            UiLayer::Base,
            UiStyle::fill(),
        )
        .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
            0.1, 0.2, 0.3, 1.0,
        )))
    };
    let paint_snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(painted_root("one"))
        .with_root(painted_root("two"))
        .build()
        .unwrap();
    let limits = UiLimits::default().with_max_paint_primitives(1).unwrap();
    let mut paint_compiler = UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap();
    assert!(matches!(
        paint_compiler.compile(
            &paint_snapshot,
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 100.0, 100.0, 1.0, 1, 1, 1, 1)
                .unwrap(),
            UiTheme::default(),
        ),
        Err(UiCompileError::LogicalPaintLimitExceeded {
            actual: 2,
            maximum: 1
        })
    ));
}

#[test]
fn text_measurement_reuses_an_exact_layout_request() {
    let text = "Nara";
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("test", "text-cache").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_text(UiText::new(text, 12.0).unwrap()),
        )
        .build()
        .unwrap();
    let text_limits = TextLimits {
        max_text_bytes: text.len(),
        max_glyphs: text.chars().count(),
        ..TextLimits::default()
    };
    let mut compiler = UiCompiler::with_text_limits(
        bundled_font_face().unwrap(),
        UiLimits::default(),
        text_limits,
    )
    .unwrap();

    let frame = compiler
        .compile(
            &snapshot,
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 100.0, 100.0, 1.0, 1, 1, 1, 1)
                .unwrap(),
            UiTheme::default(),
        )
        .unwrap();

    assert_eq!(frame.paint().len(), 1);
}

#[test]
fn text_work_bytes_are_candidate_wide() {
    let text_root = |name: &str, text: &str| {
        UiRoot::new(
            UiRootKey::runtime("test", name).unwrap(),
            UiLayer::Base,
            UiStyle::fill(),
        )
        .with_text(UiText::new(text, 12.0).unwrap())
    };
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(text_root("one", "A"))
        .with_root(text_root("two", "B"))
        .build()
        .unwrap();
    let text_limits = TextLimits {
        max_text_bytes: 1,
        ..TextLimits::default()
    };
    let mut compiler = UiCompiler::with_text_limits(
        bundled_font_face().unwrap(),
        UiLimits::default(),
        text_limits,
    )
    .unwrap();

    assert!(matches!(
        compiler.compile(
            &snapshot,
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 100.0, 100.0, 1.0, 1, 1, 1, 1)
                .unwrap(),
            UiTheme::default(),
        ),
        Err(UiCompileError::TextWorkLimitExceeded {
            actual: 2,
            maximum: 1
        })
    ));
}
