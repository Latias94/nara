use nara_text::{TextLimits, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompileError, UiCompiler, UiEnvironment, UiLayer, UiLocalKey,
    UiNode, UiRoot, UiRootKey, UiRootStatus, UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
};

fn environment() -> UiEnvironment {
    environment_at_scale(1.0)
}

fn environment_at_scale(scale_factor: f32) -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(1).unwrap(),
        320.0,
        180.0,
        scale_factor,
        1,
        1,
        1,
        1,
    )
    .unwrap()
}

fn text_root(name: &str, text: &str) -> UiRoot {
    UiRoot::new(
        UiRootKey::runtime("text-budget", name).unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_text(UiText::new(text, 12.0).unwrap())
}

fn visual_root(name: &str) -> UiRoot {
    UiRoot::new(
        UiRootKey::runtime("text-budget", name).unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_visual(UiVisual::solid(nara_ui_kernel::UiColor::rgba(
        0.2, 0.3, 0.4, 1.0,
    )))
}

fn invalid_root(name: &str) -> UiRoot {
    let duplicate = || {
        UiNode::new(UiStyle::fill())
            .with_key(UiLocalKey::runtime("text-budget", "duplicate").unwrap())
    };
    UiRoot::new(
        UiRootKey::runtime("text-budget", name).unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_children([duplicate(), duplicate()])
}

#[test]
fn repeated_failed_layout_is_cached_and_charged_once() {
    let text = "A\nB";
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(text_root("one", text))
        .with_root(text_root("two", text))
        .build()
        .unwrap();
    let text_limits = TextLimits {
        max_text_bytes: text.len(),
        max_lines: 1,
        ..TextLimits::default()
    };
    let mut compiler = UiCompiler::with_text_limits(
        bundled_font_face().unwrap(),
        Default::default(),
        text_limits,
    )
    .unwrap();

    let frame = compiler
        .compile(&snapshot, environment(), UiTheme::default())
        .unwrap();

    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Absent);
    assert_eq!(frame.root_outcomes()[1].status(), UiRootStatus::Absent);
}

#[test]
fn distinct_failed_layouts_consume_input_work_budget() {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(text_root("one", "A\nB"))
        .with_root(text_root("two", "C\nD"))
        .build()
        .unwrap();
    let text_limits = TextLimits {
        max_text_bytes: 5,
        max_lines: 1,
        ..TextLimits::default()
    };
    let mut compiler = UiCompiler::with_text_limits(
        bundled_font_face().unwrap(),
        Default::default(),
        text_limits,
    )
    .unwrap();

    assert!(matches!(
        compiler.compile(&snapshot, environment(), UiTheme::default()),
        Err(UiCompileError::TextWorkLimitExceeded {
            actual: 6,
            maximum: 5
        })
    ));
}

#[test]
fn text_budget_rejection_does_not_advance_revision_or_replace_last_good_visual() {
    let key = UiRootKey::runtime("text-budget", "one").unwrap();
    let baseline = UiAuthoringSnapshotBuilder::new(1)
        .with_root(visual_root("one"))
        .build()
        .unwrap();
    let text_limits = TextLimits {
        max_text_bytes: 5,
        max_lines: 1,
        ..TextLimits::default()
    };
    let mut compiler = UiCompiler::with_text_limits(
        bundled_font_face().unwrap(),
        Default::default(),
        text_limits,
    )
    .unwrap();
    let first = compiler
        .compile(&baseline, environment(), UiTheme::default())
        .unwrap();

    let rejected = UiAuthoringSnapshotBuilder::new(2)
        .with_root(text_root("one", "A\nB"))
        .with_root(text_root("two", "C\nD"))
        .build()
        .unwrap();
    assert!(matches!(
        compiler.compile(&rejected, environment(), UiTheme::default()),
        Err(UiCompileError::TextWorkLimitExceeded { .. })
    ));

    let fallback = UiAuthoringSnapshotBuilder::new(3)
        .with_root(text_root("one", "A\nB"))
        .build()
        .unwrap();
    let retained = compiler
        .compile(&fallback, environment(), UiTheme::default())
        .unwrap();

    assert_eq!(retained.revision().get(), first.revision().get() + 1);
    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert_eq!(retained.paint(), first.paint());
    assert_eq!(retained.paint()[0].key().unwrap().root_key(), &key);
}

#[test]
fn text_budget_rejection_preserves_root_cache_eviction_order() {
    let limits = nara_ui_kernel::UiLimits::default()
        .with_max_cache_roots(2)
        .unwrap();
    let text_limits = TextLimits {
        max_text_bytes: 5,
        max_lines: 1,
        ..TextLimits::default()
    };
    let mut compiler =
        UiCompiler::with_text_limits(bundled_font_face().unwrap(), limits, text_limits).unwrap();

    let baseline = UiAuthoringSnapshotBuilder::new(1)
        .with_root(visual_root("a"))
        .with_root(visual_root("b"))
        .build()
        .unwrap();
    compiler
        .compile(&baseline, environment_at_scale(1.0), UiTheme::default())
        .unwrap();

    let age_a = UiAuthoringSnapshotBuilder::new(2)
        .with_root(visual_root("a"))
        .with_root(invalid_root("b"))
        .build()
        .unwrap();
    let aged = compiler
        .compile(&age_a, environment_at_scale(1.25), UiTheme::default())
        .unwrap();
    assert_eq!(aged.root_outcomes()[0].status(), UiRootStatus::Published);
    assert_eq!(aged.root_outcomes()[1].status(), UiRootStatus::Absent);

    let rejected = UiAuthoringSnapshotBuilder::new(3)
        .with_root(text_root("b", "A\nB"))
        .with_root(text_root("overflow", "C\nD"))
        .build()
        .unwrap();
    assert!(matches!(
        compiler.compile(&rejected, environment_at_scale(1.0), UiTheme::default()),
        Err(UiCompileError::TextWorkLimitExceeded { .. })
    ));

    let evict_oldest = UiAuthoringSnapshotBuilder::new(4)
        .with_root(invalid_root("a"))
        .with_root(invalid_root("b"))
        .with_root(visual_root("c"))
        .build()
        .unwrap();
    compiler
        .compile(&evict_oldest, environment_at_scale(1.5), UiTheme::default())
        .unwrap();

    let probe_b = UiAuthoringSnapshotBuilder::new(5)
        .with_root(invalid_root("b"))
        .build()
        .unwrap();
    let probe = compiler
        .compile(&probe_b, environment_at_scale(1.0), UiTheme::default())
        .unwrap();

    assert_eq!(probe.root_outcomes()[0].status(), UiRootStatus::Absent);
}
