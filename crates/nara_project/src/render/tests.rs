use crate::{EffectiveWorldCanvas, ProjectManifest, ProjectPath, ProjectProfileError};
use nara_diagnostic::{Diagnostic, DiagnosticValueRef};

fn diagnostic_field(diagnostic: &Diagnostic) -> Option<&str> {
    diagnostic.fields().iter().find_map(|field| {
        if field.key().as_str() == "field"
            && let DiagnosticValueRef::Identifier(value) = field.value()
        {
            Some(value)
        } else {
            None
        }
    })
}

const MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Fixed world canvas"

[window]
width = 1280
height = 720

[render.world_canvas]
width = 320
height = 180

[ui.primary_surface]
logical_width = 960
logical_height = 540

[profiles.inherit]

[profiles.desktop.render.world_canvas]
width = 640
height = 360

[profiles.native.render]
clear_world_canvas = true

[profiles.server]
"#;

#[test]
fn world_canvas_profiles_replace_or_clear_without_changing_window_or_ui() {
    let load = ProjectManifest::parse_toml_str(MANIFEST);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let manifest = load.manifest.unwrap();
    let base = manifest.resolve_profile(None).unwrap();
    assert_eq!(
        base.render.world_canvas,
        Some(EffectiveWorldCanvas {
            width: 320,
            height: 180
        })
    );
    assert_eq!(
        manifest.resolve_profile(Some("inherit")).unwrap().render,
        base.render
    );
    let desktop = manifest.resolve_profile(Some("desktop")).unwrap();
    assert_eq!(
        desktop.render.world_canvas,
        Some(EffectiveWorldCanvas {
            width: 640,
            height: 360
        })
    );
    assert_eq!(desktop.window, base.window);
    assert_eq!(desktop.ui, base.ui);
    let native = manifest.resolve_profile(Some("native")).unwrap();
    assert!(native.render.world_canvas.is_none());
    assert_eq!(native.window, base.window);
    assert_eq!(native.ui, base.ui);

    let server = manifest.resolve_profile(Some("server")).unwrap();
    assert!(!server.window.enabled);
    assert_eq!(server.render, base.render);
    assert!(server.requested_capabilities.is_empty());
}

#[test]
fn omitted_world_canvas_preserves_native_resolution_defaults() {
    let manifest =
        ProjectManifest::parse_toml_str("schema_version = 1\n[project]\nname = 'Native world'\n")
            .manifest
            .unwrap();
    assert!(
        manifest
            .resolve_profile(None)
            .unwrap()
            .render
            .world_canvas
            .is_none()
    );
}

#[test]
fn invalid_base_and_profile_extents_are_rejected_with_exact_field_paths() {
    for (section, field) in [
        ("render.world_canvas", "render.world_canvas.width"),
        (
            "profiles.desktop.render.world_canvas",
            "profiles.desktop.render.world_canvas.width",
        ),
    ] {
        let source = format!(
            "schema_version = 1\n[project]\nname = 'Invalid canvas'\n[{section}]\nwidth = 0\nheight = 180\n"
        );
        let load = ProjectManifest::parse_toml_str(&source);
        assert!(load.has_errors());
        assert!(load.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "project.render.invalid-world-canvas-extent"
                && diagnostic_field(diagnostic) == Some(field)
        }));
        assert!(matches!(
            load.manifest.unwrap().resolve_profile(None),
            Err(ProjectProfileError::InvalidManifest { .. })
        ));
    }
    let load = ProjectManifest::parse_toml_str(&MANIFEST.replace("height = 180", "height = 0"));
    assert!(load.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.render.invalid-world-canvas-extent"
            && diagnostic_field(diagnostic) == Some("render.world_canvas.height")
    }));
}

#[test]
fn world_canvas_profile_replacement_requires_both_extents_and_cannot_also_clear() {
    let partial = MANIFEST.replace("width = 640\nheight = 360", "width = 640");
    let load = ProjectManifest::parse_toml_str(&partial);
    assert!(load.has_errors());
    assert!(load.manifest.is_none());

    let conflict = format!("{MANIFEST}\n[profiles.desktop.render]\nclear_world_canvas = true\n");
    let load = ProjectManifest::parse_toml_str(&conflict);
    assert!(load.has_errors());
    assert!(load.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.render.conflicting-world-canvas-patch"
    }));
}

#[test]
fn canonical_startup_repair_preserves_every_canvas_profile() {
    let mut manifest = ProjectManifest::parse_toml_str(MANIFEST).manifest.unwrap();
    let expected = [None, Some("desktop"), Some("native")]
        .map(|profile| manifest.resolve_profile(profile).unwrap().render);
    manifest
        .replace_startup_scene(
            Some("desktop"),
            ProjectPath::new("arranged.scene.json").unwrap(),
        )
        .unwrap();
    let bytes = manifest.to_canonical_toml_bytes().unwrap();
    let load = ProjectManifest::parse_toml_bytes(&bytes);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let decoded = load.manifest.unwrap();
    assert_eq!(decoded, manifest);
    for (profile, render) in [None, Some("desktop"), Some("native")]
        .into_iter()
        .zip(expected)
    {
        assert_eq!(decoded.resolve_profile(profile).unwrap().render, render);
    }
}
