use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompileError, UiCompiler, UiEnvironment, UiLayer, UiLimits,
    UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme,
};

fn environment(revision: u64) -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(1).expect("test surface must be non-zero"),
        100.0,
        100.0,
        1.0,
        revision,
        1,
        1,
        1,
    )
    .expect("test environment must be valid")
}

fn snapshot(revision: u64, limits: UiLimits) -> nara_ui_kernel::UiAuthoringSnapshot {
    UiAuthoringSnapshotBuilder::new(revision)
        .with_limits(limits)
        .with_root(UiRoot::new(
            UiRootKey::runtime("profile-test", "root").expect("test key must be valid"),
            UiLayer::Base,
            UiStyle::fill(),
        ))
        .build()
        .expect("test snapshot must be valid")
}

#[test]
fn stricter_snapshot_rejects_wider_compiler_profile_before_revision() {
    let strict_limits = UiLimits::default()
        .with_max_nodes(1)
        .expect("strict profile must be valid");
    let strict_snapshot = snapshot(1, strict_limits);
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), UiLimits::default()).unwrap();

    assert!(matches!(
        compiler.compile(&strict_snapshot, environment(1), UiTheme::default()),
        Err(UiCompileError::SnapshotProfileMismatch)
    ));

    let frame = compiler
        .compile(
            &snapshot(2, UiLimits::default()),
            environment(2),
            UiTheme::default(),
        )
        .expect("a matching profile must still compile after rejection");
    assert_eq!(frame.revision().get(), 1);
}

#[test]
fn wider_snapshot_rejects_stricter_compiler_profile_before_revision() {
    let strict_limits = UiLimits::default()
        .with_max_nodes(1)
        .expect("strict profile must be valid");
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), strict_limits).unwrap();

    assert!(matches!(
        compiler.compile(
            &snapshot(1, UiLimits::default()),
            environment(1),
            UiTheme::default()
        ),
        Err(UiCompileError::SnapshotProfileMismatch)
    ));

    let strict_snapshot = snapshot(2, strict_limits);
    let frame = compiler
        .compile(&strict_snapshot, environment(2), UiTheme::default())
        .expect("a matching profile must still compile after rejection");
    assert_eq!(frame.revision().get(), 1);
}

#[test]
fn compile_only_diagnostic_limit_does_not_change_snapshot_profile() {
    let snapshot = snapshot(1, UiLimits::default());
    let limits = UiLimits::default()
        .with_max_diagnostics(1)
        .expect("compile-only limit must be valid");
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap();

    let frame = compiler
        .compile(&snapshot, environment(1), UiTheme::default())
        .expect("compile-only limits must not mismatch the authoring profile");
    assert_eq!(frame.revision().get(), 1);
}
