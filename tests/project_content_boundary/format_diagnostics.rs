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
    MissingRoots,
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
    Failure::MissingRoots,
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
            Self::MissingRoots | Self::RecordParent => "legacy-placement-shape",
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
                payload: (roots: [], entities: []),
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
            "payload": {"roots": [], "entities": []},
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
            Self::MissingRoots => {
                source["payload"].as_object_mut().unwrap().remove("roots");
            }
            Self::RecordParent => {
                source["payload"]["roots"] = serde_json::json!(["private_entity_sentinel"]);
                source["payload"]["entities"] = serde_json::json!([
                    {"id": "private_entity_sentinel", "parent": "private_parent_sentinel"}
                ]);
            }
            _ => unreachable!(),
        }
        serde_json::to_vec(&source).unwrap()
    }

    fn assert_diagnostic(self, diagnostic: &Diagnostic, prefab: bool, ordinary_open: bool) {
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
            Self::MissingRoots | Self::RecordParent => {
                let index = diagnostic
                    .fields()
                    .iter()
                    .find(|field| field.key().as_str() == "entity_index");
                if matches!(self, Self::RecordParent) {
                    assert_public(diagnostic, "entity_index", DiagnosticValueRef::Unsigned(0));
                } else {
                    assert!(index.is_none());
                }
                let summary = diagnostic.summary().as_str();
                assert!(summary.contains("ordered roots"), "{diagnostic:?}");
                assert!(summary.contains("parent-owned children"), "{diagnostic:?}");
                assert!(
                    summary.contains("sibling order cannot be inferred"),
                    "{diagnostic:?}"
                );
                if ordinary_open {
                    let detail = if matches!(self, Self::MissingRoots) {
                        "missing ordered roots"
                    } else {
                        "record-owned parent"
                    };
                    assert!(summary.contains(detail), "{diagnostic:?}");
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
            let code = match (prefab, failure) {
                (false, Failure::MissingRoots | Failure::RecordParent) => {
                    "project.content.scene-legacy-placement-shape"
                }
                (true, Failure::MissingRoots | Failure::RecordParent) => {
                    "project.content.prefab-legacy-placement-shape"
                }
                (false, _) => "project.content.scene-format-invalid",
                (true, _) => "project.content.prefab-format-invalid",
            };
            assert_eq!(diagnostic.code().as_str(), code, "{failure:?}: {error:?}");
            failure.assert_diagnostic(diagnostic, prefab, false);
        }
    }
}

#[cfg(feature = "tooling")]
#[test]
fn ordinary_open_format_diagnostics_redact_private_metadata_and_preserve_failure_classes() {
    use nara::{
        project_host::{
            EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        },
        tooling::{
            EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentOpenResult,
            EditorProjectBrowserAction, EditorProjectBrowserActionOutcome,
        },
    };
    use std::time::{Duration, Instant};

    for prefab in [false, true] {
        let project = TestProject::with_prefab_startup();
        let prefix = if prefab {
            "prefabs/u4.prefab"
        } else {
            "scenes/u4.scene"
        };
        let sources = if prefab {
            let document = nara::scene::PrefabDocument::default();
            [
                document.to_json_string().unwrap(),
                document.to_ron_string().unwrap(),
            ]
        } else {
            let document = nara::scene::SceneDocument::default();
            [
                document.to_json_string().unwrap(),
                document.to_ron_string().unwrap(),
            ]
        };
        for (encoding, source) in ["json", "ron"].into_iter().zip(sources) {
            std::fs::write(project.path().join(format!("{prefix}.{encoding}")), source).unwrap();
        }
        let mut host = EditorProjectOpenHost::new();
        let mut attempt = host.begin(project.writable_root_capability(), None);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut editor = loop {
            match host.advance(attempt) {
                EditorProjectOpenProgress::Opened(editor) => break editor,
                EditorProjectOpenProgress::Pending(next) => attempt = next,
                EditorProjectOpenProgress::Rejected(error) => {
                    panic!("fixture Open rejected: {error:?}")
                }
            }
            assert!(Instant::now() < deadline, "project Open did not settle");
            std::thread::sleep(Duration::from_millis(1));
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            if !frame.project_browser().has_pending_work() {
                assert!(frame.project_browser().snapshot().is_some());
                assert!(frame.project_browser().failure().is_none());
                break;
            }
            assert!(Instant::now() < deadline, "Browser index did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
        let before = editor.current_frame();
        let mut completed = Vec::new();
        for failure in FAILURES {
            let path = format!("{prefix}.{}", if failure.is_ron() { "ron" } else { "json" },);
            // Overwrite the indexed file in place to retain its admitted identity.
            std::fs::write(project.path().join(&path), failure.source(prefab)).unwrap();
            let frame = editor.current_frame();
            let report = editor
                .dispatch_editor_actions(EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::ProjectBrowser(
                        EditorProjectBrowserAction::Open {
                            relative_path: path.into(),
                        },
                    )],
                ))
                .unwrap();
            let request = match report.outcomes() {
                [
                    EditorActionOutcome::ProjectBrowser(
                        EditorProjectBrowserActionOutcome::Preparing { request },
                    ),
                ] => *request,
                other => panic!("ordinary Open was not admitted: {other:?}"),
            };
            let deadline = Instant::now() + Duration::from_secs(10);
            let frame = loop {
                let frame = editor.drive_editor_frame(Duration::ZERO);
                if frame.document_open().result().is_some() {
                    break frame;
                }
                assert!(Instant::now() < deadline, "document Open did not settle");
                std::thread::sleep(Duration::from_millis(1));
            };
            completed.push((failure, request, frame));
            let current = editor.current_frame();
            editor
                .dispatch_editor_actions(EditorActionBatch::new(
                    current.revision(),
                    [EditorAction::ProjectBrowser(
                        EditorProjectBrowserAction::AcknowledgeOpen { request },
                    )],
                ))
                .unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while !matches!(
            editor.advance_close(),
            EditorProjectCloseProgress::Complete(_)
        ) {
            assert!(Instant::now() < deadline, "Editor close did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
        for (failure, request, frame) in completed {
            assert_eq!(
                frame.document_open().result(),
                Some(EditorDocumentOpenResult::Rejected {
                    request,
                    code: "project.editor.document-source-format-invalid",
                }),
                "{failure:?}"
            );
            assert_eq!(frame.workspace(), before.workspace());
            let diagnostic = frame
                .diagnostics()
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code().as_str() == "project.editor.document-source-format-invalid"
                })
                .last()
                .expect("ordinary Open must retain the format diagnostic");
            failure.assert_diagnostic(diagnostic, prefab, true);
        }
    }
}
