use super::project_content_fixture::TestProject;
use nara::diagnostic::{
    Diagnostic, DiagnosticFieldClass, DiagnosticValueRef, PublicDiagnosticIdentifier,
};

const PRIVATE_KIND: &str = "private_document_sentinel";
const PRIVATE_PRERELEASE: &str = "private-prerelease-sentinel";
const PRIVATE_BUILD: &str = "privatebuildsentinel";

#[derive(Clone, Copy, Debug)]
enum Failure {
    UnknownKind,
    KnownKind,
    EnginePrerelease,
    EngineBuild,
    EnginePrereleaseAndBuild,
    MissingPlacements,
    RecordParent,
    RonAttribute,
    RonCharacter,
    HeaderType,
    PayloadType,
}

const FAILURES: [Failure; 11] = [
    Failure::UnknownKind,
    Failure::KnownKind,
    Failure::EnginePrerelease,
    Failure::EngineBuild,
    Failure::EnginePrereleaseAndBuild,
    Failure::MissingPlacements,
    Failure::RecordParent,
    Failure::RonAttribute,
    Failure::RonCharacter,
    Failure::HeaderType,
    Failure::PayloadType,
];

impl Failure {
    fn reason(self) -> &'static str {
        match self {
            Self::UnknownKind | Self::KnownKind => "kind-mismatch",
            Self::EnginePrerelease | Self::EngineBuild | Self::EnginePrereleaseAndBuild => {
                "engine-version-too-old"
            }
            Self::MissingPlacements | Self::RecordParent => "payload-invalid",
            Self::RonAttribute | Self::RonCharacter => "syntax-invalid",
            Self::HeaderType => "header-invalid",
            Self::PayloadType => "payload-invalid",
        }
    }

    fn is_ron(self) -> bool {
        matches!(
            self,
            Self::RonAttribute | Self::RonCharacter | Self::HeaderType | Self::PayloadType
        )
    }

    fn source(self, prefab: bool) -> Vec<u8> {
        let kind = if prefab { "prefab" } else { "scene" };
        if self.is_ron() {
            let source = format!(
                r#"(
                kind: "{kind}",
                format_version: 1,
                engine_min_version: "0.1.0",
                generator: (name: "test", version: "0.1.0"),
                payload: (placements: [], entities: []),
            )"#
            );
            return match self {
                Self::RonAttribute => format!("#bogus\n{source}"),
                Self::RonCharacter => source.replace("entities: []", "entities: 'ab'"),
                Self::HeaderType => source.replace("format_version: 1", "format_version: true"),
                Self::PayloadType => source.replace("entities: []", "entities: true"),
                _ => unreachable!(),
            }
            .into_bytes();
        }
        let mut source = serde_json::json!({
            "kind": kind,
            "format_version": 1,
            "engine_min_version": "0.1.0",
            "generator": {"name": "test", "version": "0.1.0"},
            "payload": {"placements": [], "entities": []},
        });
        match self {
            Self::UnknownKind => source["kind"] = serde_json::json!(PRIVATE_KIND),
            Self::KnownKind => {
                source["kind"] = serde_json::json!(if prefab { "scene" } else { "prefab" });
            }
            Self::EnginePrerelease => {
                source["engine_min_version"] =
                    serde_json::json!(format!("999.2.3-{PRIVATE_PRERELEASE}"));
            }
            Self::EngineBuild => {
                source["engine_min_version"] =
                    serde_json::json!(format!("999.2.3+{PRIVATE_BUILD}"));
            }
            Self::EnginePrereleaseAndBuild => {
                source["engine_min_version"] =
                    serde_json::json!(format!("999.2.3-{PRIVATE_PRERELEASE}+{PRIVATE_BUILD}"));
            }
            Self::MissingPlacements => {
                source["payload"]
                    .as_object_mut()
                    .unwrap()
                    .remove("placements");
            }
            Self::RecordParent => {
                source["payload"]["placements"] = serde_json::json!([{
                    "target": {"through": [], "entity": {
                        "kind": "source_entity", "entity": "private_entity_sentinel"
                    }},
                    "placement": {"parent": {"kind": "root"}, "slot": {"kind": "last"}}
                }]);
                source["payload"]["entities"] = serde_json::json!([
                    {"id": "private_entity_sentinel", "parent": "private_parent_sentinel"}
                ]);
            }
            _ => unreachable!(),
        }
        serde_json::to_vec(&source).unwrap()
    }

    fn assert_diagnostic(self, diagnostic: &Diagnostic, prefab: bool) {
        assert_public(
            diagnostic,
            "reason",
            DiagnosticValueRef::Identifier(self.reason()),
        );
        match self {
            Self::UnknownKind | Self::KnownKind => {
                assert_public(
                    diagnostic,
                    "expected_kind",
                    DiagnosticValueRef::Identifier(if prefab { "prefab" } else { "scene" }),
                );
                let observed = diagnostic
                    .fields()
                    .iter()
                    .find(|field| field.key().as_str() == "observed_kind")
                    .unwrap();
                if matches!(self, Self::UnknownKind) {
                    // This input passes the lexical guard, so only source classification protects it.
                    assert!(PublicDiagnosticIdentifier::new(PRIVATE_KIND).is_ok());
                    assert_eq!(observed.class(), DiagnosticFieldClass::Sensitive);
                    assert_eq!(observed.value(), DiagnosticValueRef::Redacted);
                } else {
                    assert_eq!(observed.class(), DiagnosticFieldClass::Public);
                    assert_eq!(
                        observed.value(),
                        DiagnosticValueRef::Identifier(if prefab { "scene" } else { "prefab" })
                    );
                }
            }
            Self::EnginePrerelease | Self::EngineBuild | Self::EnginePrereleaseAndBuild => {
                assert_public(
                    diagnostic,
                    "required_engine",
                    DiagnosticValueRef::Identifier("999.2.3"),
                );
                assert_public(
                    diagnostic,
                    "current_engine",
                    DiagnosticValueRef::Identifier("0.1.0"),
                );
            }
            Self::MissingPlacements | Self::RecordParent => {
                assert_public(
                    diagnostic,
                    "encoding",
                    DiagnosticValueRef::Identifier("json"),
                );
                for key in ["line", "column"] {
                    assert!(diagnostic.fields().iter().any(|field| {
                        field.key().as_str() == key
                            && field.class() == DiagnosticFieldClass::Public
                            && matches!(field.value(), DiagnosticValueRef::Unsigned(value) if value > 0)
                    }), "{diagnostic:?}");
                }
            }
            Self::RonAttribute | Self::RonCharacter | Self::HeaderType | Self::PayloadType => {
                assert_public(
                    diagnostic,
                    "encoding",
                    DiagnosticValueRef::Identifier("ron"),
                );
                for key in ["line", "column"] {
                    assert!(
                        diagnostic.fields().iter().any(|field| {
                            field.key().as_str() == key
                                && field.class() == DiagnosticFieldClass::Public
                                && matches!(field.value(), DiagnosticValueRef::Unsigned(value) if value > 0)
                        }),
                        "{diagnostic:?}"
                    );
                }
            }
        }
        for rendered in [
            format!("{diagnostic:?}"),
            serde_json::to_string(diagnostic).unwrap(),
        ] {
            for private in [
                PRIVATE_KIND,
                PRIVATE_PRERELEASE,
                PRIVATE_BUILD,
                "private_entity_sentinel",
                "private_parent_sentinel",
            ] {
                assert!(!rendered.contains(private), "{self:?}: {rendered}");
            }
        }
    }
}

fn assert_public(diagnostic: &Diagnostic, key: &str, value: DiagnosticValueRef<'_>) {
    assert!(
        diagnostic.fields().iter().any(|field| {
            field.key().as_str() == key
                && field.class() == DiagnosticFieldClass::Public
                && field.value() == value
        }),
        "missing {key}={value:?}: {diagnostic:?}"
    );
}

#[test]
fn check_format_diagnostics_redact_private_metadata_and_preserve_failure_classes() {
    for prefab in [false, true] {
        for failure in FAILURES {
            let project = TestProject::with_prefab_startup();
            let json_path = if prefab {
                "prefabs/enemy.prefab.json"
            } else {
                "scenes/startup.scene.json"
            };
            let path = if failure.is_ron() {
                let ron_path = json_path.replace(".json", ".ron");
                if prefab {
                    let scene_path = project.path().join("scenes/startup.scene.json");
                    let scene = std::fs::read_to_string(&scene_path).unwrap();
                    std::fs::write(
                        scene_path,
                        scene.replace("enemy.prefab.json", "enemy.prefab.ron"),
                    )
                    .unwrap();
                } else {
                    let manifest_path = project.path().join("nara.toml");
                    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
                    std::fs::write(
                        manifest_path,
                        manifest.replace("startup.scene.json", "startup.scene.ron"),
                    )
                    .unwrap();
                }
                ron_path
            } else {
                json_path.to_owned()
            };
            std::fs::write(project.path().join(path), failure.source(prefab)).unwrap();
            let error = nara::project_workflow::check_project(
                nara::project_workflow::ProjectCheckRequest::new(
                    project.root_capability(),
                    nara::ProductRecipe::new(),
                ),
            )
            .unwrap_err();
            let diagnostic = error
                .content_error()
                .unwrap()
                .diagnostics()
                .iter()
                .next()
                .unwrap();
            let code = match prefab {
                false => "project.content.scene-format-invalid",
                true => "project.content.prefab-format-invalid",
            };
            assert_eq!(diagnostic.code().as_str(), code, "{failure:?}: {error:?}");
            failure.assert_diagnostic(diagnostic, prefab);
        }
    }
}
