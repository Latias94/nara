use nara_text::{TextLimits, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompileError, UiCompileWorkStats, UiCompiler,
    UiDimension, UiEnvironment, UiFrameJournal, UiInteraction, UiJournalError, UiLayer, UiLimits,
    UiLocalKey, UiNode, UiRoot, UiRootKey, UiRootStatus, UiRouteStamp, UiSemanticRole, UiSemantics,
    UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
};

fn environment() -> UiEnvironment {
    UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 320.0, 180.0, 1.0, 1, 1, 1, 1).unwrap()
}

fn root(name: &str) -> UiRoot {
    UiRoot::new(
        UiRootKey::runtime("work-stats", name).unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
}

fn text_root(name: &str, text: &str) -> UiRoot {
    root(name).with_child(
        UiNode::new(UiStyle::flex().with_width(UiDimension::px(120.0).unwrap()))
            .with_key(UiLocalKey::runtime("work-stats", "label").unwrap())
            .with_text(UiText::new(text, 12.0).unwrap()),
    )
}

fn interactive_root(name: &str) -> UiRoot {
    root(name).with_child(
        UiNode::new(UiStyle::absolute(10.0, 10.0, 100.0, 40.0))
            .with_key(UiLocalKey::runtime("work-stats", "button").unwrap())
            .with_visual(UiVisual::solid(UiColor::rgba(0.2, 0.3, 0.4, 1.0)))
            .with_interaction(UiInteraction::focus_target())
            .with_semantics(UiSemantics::new(UiSemanticRole::Button, "Retry").unwrap()),
    )
}

fn rejected_root(name: &str) -> UiRoot {
    let duplicate = UiNode::new(UiStyle::flex())
        .with_key(UiLocalKey::runtime("work-stats", "duplicate").unwrap());
    root(name).with_children([
        duplicate.clone(),
        duplicate.with_child(UiNode::new(UiStyle::flex())),
    ])
}

#[test]
fn identical_compiles_repeat_layout_and_reuse_text_while_preserving_frame_contents() {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(text_root("label", "Unchanged"))
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), UiLimits::default()).unwrap();
    assert_eq!(compiler.work_stats(), UiCompileWorkStats::default());

    let first = compiler
        .compile(&snapshot, environment(), UiTheme::default())
        .unwrap();
    let first_stats = compiler.work_stats();
    assert_eq!(first_stats.compile_attempts, 1);
    assert_eq!(first_stats.published_frames, 1);
    assert_eq!(first_stats.root_compile_attempts, 1);
    assert_eq!(
        first_stats.layout_nodes_created,
        snapshot.source_node_count() as u64
    );
    assert_eq!(first_stats.layout_passes, 1);
    assert!(first_stats.measurement_callbacks > 0);
    assert!(first_stats.text.layout_attempts > 0);
    assert_eq!(first_stats.retained_roots, 0);

    let second = compiler
        .compile(&snapshot, environment(), UiTheme::default())
        .unwrap();
    let second_stats = compiler.work_stats();
    assert_eq!(
        second_stats.compile_attempts,
        first_stats.compile_attempts + 1
    );
    assert_eq!(
        second_stats.published_frames,
        first_stats.published_frames + 1
    );
    assert_eq!(
        second_stats.root_compile_attempts,
        first_stats.root_compile_attempts + 1
    );
    assert_eq!(
        second_stats.layout_nodes_created - first_stats.layout_nodes_created,
        snapshot.source_node_count() as u64
    );
    assert_eq!(second_stats.layout_passes, first_stats.layout_passes + 1);
    assert!(second_stats.measurement_callbacks > first_stats.measurement_callbacks);
    assert_eq!(
        second_stats.text.layout_attempts,
        first_stats.text.layout_attempts
    );
    assert_eq!(second_stats.retained_roots, 0);
    assert_eq!(second.revision().get(), first.revision().get() + 1);
    assert_eq!(second.source_revision(), first.source_revision());
    assert_eq!(second.root_outcomes(), first.root_outcomes());
    assert_eq!(second.layouts(), first.layouts());
    assert_eq!(second.paint(), first.paint());
    assert_eq!(second.hit_regions(), first.hit_regions());
    assert_eq!(second.focus_order(), first.focus_order());
    assert_eq!(second.semantics(), first.semantics());
    assert_eq!(second.diagnostics(), first.diagnostics());
}

#[test]
fn root_local_retention_is_failure_fallback_without_interactive_reuse() {
    let baseline = UiAuthoringSnapshotBuilder::new(1)
        .with_root(interactive_root("retained"))
        .with_root(text_root("live", "Live"))
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), UiLimits::default()).unwrap();
    let first = compiler
        .compile(&baseline, environment(), UiTheme::default())
        .unwrap();
    let before = compiler.work_stats();
    assert_eq!(first.hit_regions().len(), 1);
    assert_eq!(first.focus_order().len(), 1);

    let fallback = UiAuthoringSnapshotBuilder::new(2)
        .with_root(rejected_root("retained"))
        .with_root(text_root("live", "Live"))
        .build()
        .unwrap();
    assert_eq!(fallback.roots().len(), 1);
    assert_eq!(fallback.source_node_count(), 6);
    let retained = compiler
        .compile(&fallback, environment(), UiTheme::default())
        .unwrap();
    let after = compiler.work_stats();

    assert_eq!(after.compile_attempts, before.compile_attempts + 1);
    assert_eq!(after.published_frames, before.published_frames + 1);
    assert_eq!(
        after.root_compile_attempts,
        before.root_compile_attempts + 2
    );
    assert_eq!(after.retained_roots, before.retained_roots + 1);
    assert_eq!(after.layout_passes, before.layout_passes + 1);
    assert_eq!(after.layout_nodes_created, before.layout_nodes_created + 2);
    assert!(after.measurement_callbacks > before.measurement_callbacks);
    assert_eq!(after.text.layout_attempts, before.text.layout_attempts);
    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert_eq!(
        retained.root_outcomes()[1].status(),
        UiRootStatus::Published
    );
    assert_eq!(retained.layouts(), first.layouts());
    assert_eq!(retained.paint(), first.paint());
    assert!(retained.hit_regions().is_empty());
    assert!(retained.focus_order().is_empty());
    assert_eq!(retained.semantics().len(), 1);
    assert!(
        retained
            .semantics()
            .iter()
            .all(|node| node.key().root_key() != baseline.roots()[0].key())
    );
}

#[test]
fn early_global_rejections_count_attempts_without_layout_or_publication() {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(text_root("label", "Unchanged"))
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), UiLimits::default()).unwrap();
    let before = compiler.work_stats();
    let mismatched_theme =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 320.0, 180.0, 1.0, 1, 1, 2, 1).unwrap();
    assert!(matches!(
        compiler.compile(&snapshot, mismatched_theme, UiTheme::default()),
        Err(UiCompileError::ThemeRevisionMismatch { .. })
    ));
    assert_eq!(
        compiler.work_stats(),
        UiCompileWorkStats {
            compile_attempts: before.compile_attempts + 1,
            ..before
        }
    );

    let strict = UiAuthoringSnapshotBuilder::new(2)
        .with_limits(UiLimits::default().with_max_nodes(2).unwrap())
        .with_root(text_root("label", "Unchanged"))
        .build()
        .unwrap();
    assert!(matches!(
        compiler.compile(&strict, environment(), UiTheme::default()),
        Err(UiCompileError::SnapshotProfileMismatch)
    ));
    assert_eq!(
        compiler.work_stats(),
        UiCompileWorkStats {
            compile_attempts: before.compile_attempts + 2,
            ..before
        }
    );

    let accepted = compiler
        .compile(&snapshot, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(accepted.revision().get(), 1);
    assert_eq!(compiler.work_stats().published_frames, 1);
}

#[test]
fn rejected_candidate_counts_layout_without_committing_revision_or_cache() {
    let original = UiVisual::solid(UiColor::rgba(0.2, 0.3, 0.4, 1.0));
    let replacement = UiVisual::solid(UiColor::rgba(0.8, 0.7, 0.6, 1.0));
    let baseline = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root("retained").with_visual(original))
        .build()
        .unwrap();
    let limits = UiLimits::default().with_max_paint_primitives(1).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap();
    let first = compiler
        .compile(&baseline, environment(), UiTheme::default())
        .unwrap();
    let before = compiler.work_stats();
    let rejected = UiAuthoringSnapshotBuilder::new(2)
        .with_root(root("retained").with_visual(replacement.clone()))
        .with_root(root("excess").with_visual(replacement))
        .build()
        .unwrap();

    assert!(matches!(
        compiler.compile(&rejected, environment(), UiTheme::default()),
        Err(UiCompileError::LogicalPaintLimitExceeded {
            actual: 2,
            maximum: 1
        })
    ));
    let after_rejection = compiler.work_stats();
    assert_eq!(
        after_rejection.compile_attempts,
        before.compile_attempts + 1
    );
    assert_eq!(after_rejection.published_frames, before.published_frames);
    assert_eq!(
        after_rejection.root_compile_attempts,
        before.root_compile_attempts + 2
    );
    assert_eq!(after_rejection.layout_passes, before.layout_passes + 2);
    assert_eq!(
        after_rejection.layout_nodes_created - before.layout_nodes_created,
        rejected.source_node_count() as u64
    );
    assert_eq!(after_rejection.retained_roots, before.retained_roots);

    let fallback = UiAuthoringSnapshotBuilder::new(3)
        .with_root(rejected_root("retained"))
        .build()
        .unwrap();
    let retained = compiler
        .compile(&fallback, environment(), UiTheme::default())
        .unwrap();
    let after_fallback = compiler.work_stats();
    assert_eq!(retained.revision().get(), first.revision().get() + 1);
    assert_eq!(retained.paint(), first.paint());
    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert_eq!(
        after_fallback.retained_roots,
        after_rejection.retained_roots + 1
    );
    assert_eq!(after_fallback.layout_passes, after_rejection.layout_passes);
    assert_eq!(
        after_fallback.layout_nodes_created,
        after_rejection.layout_nodes_created
    );
}

#[test]
fn text_measurement_failure_is_observed_before_root_fallback() {
    let text_limits = TextLimits {
        max_lines: 1,
        ..TextLimits::default()
    };
    let mut compiler = UiCompiler::with_text_limits(
        bundled_font_face().unwrap(),
        UiLimits::default(),
        text_limits,
    )
    .unwrap();
    let baseline = UiAuthoringSnapshotBuilder::new(1)
        .with_root(text_root("label", "Valid"))
        .build()
        .unwrap();
    let first = compiler
        .compile(&baseline, environment(), UiTheme::default())
        .unwrap();
    let before = compiler.work_stats();
    let invalid = UiAuthoringSnapshotBuilder::new(2)
        .with_root(text_root("label", "First\nSecond"))
        .build()
        .unwrap();
    assert_eq!(invalid.roots().len(), 1);

    let retained = compiler
        .compile(&invalid, environment(), UiTheme::default())
        .unwrap();
    let after = compiler.work_stats();
    assert_eq!(after.compile_attempts, before.compile_attempts + 1);
    assert_eq!(after.published_frames, before.published_frames + 1);
    assert_eq!(
        after.root_compile_attempts,
        before.root_compile_attempts + 1
    );
    assert_eq!(after.layout_passes, before.layout_passes + 1);
    assert_eq!(
        after.layout_nodes_created - before.layout_nodes_created,
        invalid.source_node_count() as u64
    );
    assert!(after.measurement_callbacks > before.measurement_callbacks);
    assert!(after.text.layout_attempts > before.text.layout_attempts);
    assert_eq!(after.retained_roots, before.retained_roots + 1);
    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert_eq!(retained.paint(), first.paint());
}

#[test]
fn observations_are_detached_owner_local_and_grant_no_presentation_authority() {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(interactive_root("button"))
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), UiLimits::default()).unwrap();
    let idle = UiCompiler::new(bundled_font_face().unwrap(), UiLimits::default()).unwrap();
    let frame = compiler
        .compile(&snapshot, environment(), UiTheme::default())
        .unwrap();
    let observed = compiler.work_stats();
    assert_eq!(observed.published_frames, 1);
    assert_eq!(compiler.work_stats(), observed);
    assert_eq!(idle.work_stats(), UiCompileWorkStats::default());

    let mut detached = observed;
    detached.compile_attempts = u64::MAX;
    detached.published_frames = u64::MAX;
    assert_ne!(detached, compiler.work_stats());
    assert_eq!(compiler.work_stats(), observed);
    assert_eq!(snapshot.source_node_count(), 2);
    assert_eq!(snapshot.source_revision(), frame.source_revision());
    assert_eq!(frame.revision().get(), 1);

    let environment = frame.environment();
    let journal = UiFrameJournal::default();
    assert_eq!(
        journal
            .resolve(UiRouteStamp::new(
                environment.surface_id(),
                environment.slot_generation(),
                environment.route_revision(),
                environment.geometry_generation(),
                observed.published_frames,
            ))
            .unwrap_err(),
        UiJournalError::CompiledButNotPresented {
            surface: environment.surface_id()
        }
    );
    assert_eq!(compiler.work_stats(), observed);
}

#[test]
fn source_node_count_includes_original_counts_of_rejected_envelopes() {
    let empty = UiAuthoringSnapshotBuilder::new(1).build().unwrap();
    assert_eq!(empty.source_node_count(), 0);

    let snapshot = UiAuthoringSnapshotBuilder::new(2)
        .with_root(text_root("ready", "Valid"))
        .with_root(rejected_root("rejected"))
        .build()
        .unwrap();
    assert_eq!(snapshot.roots().len(), 1);
    assert_eq!(snapshot.source_node_count(), 6);
    assert_eq!(snapshot.clone().source_node_count(), 6);
    assert_eq!(snapshot.source_node_count(), 6);
}
