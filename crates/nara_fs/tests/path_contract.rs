use std::path::Path;

use nara_fs::{PathValidationError, RelativeComponent, RelativePath};

#[test]
fn accepts_only_canonical_relative_components() {
    let path = RelativePath::new(Path::new("assets/textures/player.png")).unwrap();

    assert_eq!(path.len(), 3);
    assert!(!path.is_empty());
}

#[test]
fn rejects_lexical_traversal_and_ambiguous_segments() {
    for invalid in [
        "",
        ".",
        "..",
        "assets/../secret",
        "/absolute",
        "assets//player.png",
        "assets/player.png/",
        "assets\\player.png",
        "assets:stream",
    ] {
        assert!(
            RelativePath::new(Path::new(invalid)).is_err(),
            "{invalid:?} must be rejected"
        );
    }
}

#[test]
fn component_rejects_windows_device_and_stream_names_on_every_host() {
    for invalid in [
        "CON",
        "con.txt",
        "NUL",
        "COM1",
        "LPT9",
        "asset:stream",
        "trail.",
        "trail ",
    ] {
        assert!(
            RelativeComponent::new(invalid).is_err(),
            "{invalid:?} must be rejected"
        );
    }
}

#[test]
fn validation_errors_do_not_echo_host_paths() {
    let error = RelativePath::new(Path::new("../private/key.pem")).unwrap_err();

    assert!(matches!(error, PathValidationError::ParentTraversal));
    assert!(!error.to_string().contains("key.pem"));
}
