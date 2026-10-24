use nara_diagnostic::{DiagnosticFieldClass, DiagnosticValueRef};

use crate::{MAX_PROJECT_CONTENT_SCENES, ProjectManifest, ProjectProfileError};

const MINIMAL: &str = "schema_version = 1\n[project]\nsource_universe = '2649c813-2b09-54bb-98b6-35e506c33d16'\nnative_document_identity = 'project_location'\nname = 'Content roots'\n";
const DECLARED: &str = r#"
schema_version = 1
[project]
source_universe = '2649c813-2b09-54bb-98b6-35e506c33d16'
native_document_identity = 'project_location'
name = "Content roots"
[paths]
scenes = "levels"
[startup]
default_scene = "level-a.scene.json"
[content]
scenes = ["level-a.scene.json", "chapter/level-b.scene.ron"]
[profiles.desktop.paths]
scenes = "desktop-levels"
[profiles.desktop.startup]
default_scene = "chapter/level-b.scene.ron"
[profiles.server]
"#;

#[test]
fn declared_roots_are_project_wide_and_relative_to_each_effective_scene_directory() {
    let load = ProjectManifest::parse_toml_str(DECLARED);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let manifest = load.manifest.unwrap();
    let base = manifest.resolve_profile(None).unwrap();
    let desktop = manifest.resolve_profile(Some("desktop")).unwrap();
    let server = manifest.resolve_profile(Some("server")).unwrap();
    assert_eq!(base.paths.scenes.as_str(), "levels");
    assert_eq!(desktop.paths.scenes.as_str(), "desktop-levels");
    assert_eq!(
        base.startup.default_scene.as_ref().unwrap().as_str(),
        "level-a.scene.json"
    );
    assert_eq!(
        desktop.startup.default_scene.as_ref().unwrap().as_str(),
        "chapter/level-b.scene.ron"
    );
    assert_eq!(base.content, desktop.content);
    assert_eq!(base.content, server.content);
    assert_eq!(
        base.content
            .scenes
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>(),
        ["level-a.scene.json", "chapter/level-b.scene.ron"]
    );
    assert!(!server.window.enabled);
}

#[test]
fn absent_and_explicit_empty_content_have_the_same_canonical_bytes() {
    let absent = ProjectManifest::parse_toml_str(MINIMAL).manifest.unwrap();
    let empty = ProjectManifest::parse_toml_str(&format!("{MINIMAL}\n[content]\nscenes = []\n"))
        .manifest
        .unwrap();
    let bytes = absent.to_canonical_toml_bytes().unwrap();
    assert_eq!(bytes, empty.to_canonical_toml_bytes().unwrap());
    assert!(!String::from_utf8(bytes).unwrap().contains("[content]"));
    assert!(
        absent
            .resolve_profile(None)
            .unwrap()
            .content
            .scenes
            .is_empty()
    );
}

#[test]
fn canonical_startup_repair_preserves_declared_roots_and_profiles() {
    let mut manifest = ProjectManifest::parse_toml_str(DECLARED).manifest.unwrap();
    let content = manifest.content.clone();
    let project = manifest.project.clone();
    let profiles = manifest.profiles.clone();
    manifest
        .replace_startup_scene(
            None,
            crate::ProjectPath::new("repaired.scene.json").unwrap(),
        )
        .unwrap();
    let bytes = manifest.to_canonical_toml_bytes().unwrap();
    let load = ProjectManifest::parse_toml_bytes(&bytes);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let reloaded = load.manifest.unwrap();
    assert_eq!(reloaded, manifest);
    assert_eq!(reloaded.content, content);
    assert_eq!(reloaded.project, project);
    assert_eq!(reloaded.profiles, profiles);
    assert_eq!(reloaded.to_canonical_toml_bytes().unwrap(), bytes);
}

#[test]
fn declaration_limit_excludes_the_implicit_startup_root() {
    let mut manifest = ProjectManifest::parse_toml_str(MINIMAL).manifest.unwrap();
    manifest.startup.default_scene = Some("startup.scene.json".to_owned());
    manifest.content.scenes = (0..MAX_PROJECT_CONTENT_SCENES)
        .map(|index| format!("level-{index}.scene.json"))
        .collect();
    let bytes = manifest.to_canonical_toml_bytes().unwrap();
    let load = ProjectManifest::parse_toml_bytes(&bytes);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    assert_eq!(
        load.manifest
            .unwrap()
            .resolve_profile(None)
            .unwrap()
            .content
            .scenes
            .len(),
        MAX_PROJECT_CONTENT_SCENES
    );

    manifest
        .content
        .scenes
        .push("one-too-many.scene.json".to_owned());
    let source = toml::to_string(&manifest).unwrap();
    let load = ProjectManifest::parse_toml_str(&source);
    assert!(load.has_errors());
    assert!(
        load.diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "project.content.scene-limit")
    );
    assert!(matches!(
        manifest.resolve_profile(None),
        Err(ProjectProfileError::InvalidManifest { .. })
    ));
    assert!(manifest.to_canonical_toml_bytes().is_err());
}

#[test]
fn repeated_declarations_are_rejected_without_exposing_the_source_path() {
    let private_path = "private-level-token/level-a.scene.json";
    let source = format!("{MINIMAL}\n[content]\nscenes = ['{private_path}', '{private_path}']\n");
    let load = ProjectManifest::parse_toml_str(&source);
    assert!(load.has_errors());
    let diagnostic = load
        .diagnostics
        .iter()
        .find(|entry| entry.code().as_str() == "project.content.duplicate-scene")
        .unwrap();
    assert!(
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == "path"
                && field.class() == DiagnosticFieldClass::Sensitive)
    );
    assert!(
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == "field"
                && matches!(
                    field.value(),
                    DiagnosticValueRef::Identifier("content.scenes.1")
                ))
    );
    assert!(!format!("{:?}", load.diagnostics).contains("private-level-token"));
}

#[test]
fn invalid_or_escaping_paths_cannot_enter_effective_settings() {
    for path in [
        "",
        "../private-level-token",
        "chapter/../../private-level-token",
        "/private-level-token",
        "C:\\private-level-token",
        "chapter//level.scene.json",
        "./level.scene.json",
    ] {
        let mut manifest = ProjectManifest::parse_toml_str(MINIMAL).manifest.unwrap();
        manifest.content.scenes = vec![path.to_owned()];
        let source = toml::to_string(&manifest).unwrap();
        let load = ProjectManifest::parse_toml_str(&source);
        assert!(load.has_errors(), "path must reject: {path:?}");
        assert!(
            load.diagnostics
                .iter()
                .any(|entry| entry.code().as_str() == "project.path.invalid")
        );
        assert!(matches!(
            manifest.resolve_profile(None),
            Err(ProjectProfileError::InvalidManifest { .. })
        ));
        assert!(!format!("{:?}", load.diagnostics).contains("private-level-token"));
    }
}

#[test]
fn unknown_content_fields_and_profile_content_overlays_are_not_admitted() {
    for suffix in [
        "[content]\nscene = ['level.scene.json']",
        "[content]\nscenes = [123]",
        "[profiles.desktop.content]\nscenes = ['level.scene.json']",
    ] {
        let load = ProjectManifest::parse_toml_str(&format!("{MINIMAL}\n{suffix}\n"));
        assert!(load.manifest.is_none());
        assert!(load.has_errors());
        assert!(
            load.diagnostics
                .iter()
                .any(|entry| entry.code().as_str() == "project.manifest.parse")
        );
    }
}
