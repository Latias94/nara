//! Owner-conformance fixtures exercise retained lanes without changing public Open policy.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use nara_scene::{
    PrefabDocument, SceneDocument, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    ScenePlacement, ScenePlacementOwner,
};

use super::*;

mod attempts;
mod content_refresh;
mod initial_open;
#[cfg(feature = "presentation-2d")]
mod preview_epoch;
mod reopen_failures;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct Fixture {
    root: PathBuf,
    capability: DirectoryCapability,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_document_custody_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = '6d57f8c5-867c-5463-b74a-057a8cabe2bc'\nnative_document_identity = 'project_location'\nname = \"Document custody\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[runtime]\npreset = \"local-headless\"\n[capabilities]\nrequested = [\"runtime-2d\"]\n").unwrap();
        let bundle = nara_reflect::AuthoringSchemaBundle::from_trusted_provider_definitions(
            built_in_schema_providers(),
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        let capability = Self::open_capability(&root);
        Self { root, capability }
    }

    fn capability(&self) -> DirectoryCapability {
        self.capability.share()
    }

    fn open_capability(root: &std::path::Path) -> DirectoryCapability {
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(root)
                .unwrap()
        };
        #[cfg(target_os = "linux")]
        let file = fs::File::open(root).unwrap();
        DirectoryCapability::from_host_handle(
            file,
            nara_fs::HostCapabilityOptions::new(
                nara_fs::CapabilityRights::ReadWrite,
                nara_fs::TrustMode::TrustedLocal,
            ),
        )
        .unwrap()
    }

    fn editor(&self) -> EditorProjectSession {
        EditorProjectSession::open_trusted_compiled(
            self.capability(),
            super::super::TrustedCompiledEditorProjectIntent::new(),
        )
        .unwrap()
    }

    fn authoring_editor(&self) -> EditorProjectSession {
        use crate::project_host::authoring::{
            AuthoringProjectAdmissionHost, PreparedAuthoringProject, project_authoring_adapters,
        };
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(self.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapters = project_authoring_adapters(schema.bundle()).unwrap();
        let receipt = PreparedAuthoringProject::new(schema, adapters)
            .admit_startup_closure()
            .unwrap()
            .publish();
        let mut editor = EditorProjectSession::open(receipt, None).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while editor.project_source_index_work_pending() {
            editor.drive_editor_frame(Duration::ZERO);
            assert!(
                Instant::now() < deadline,
                "the authoring source index did not settle"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        editor
    }

    fn two_prefabs(&self) -> (EditorProjectSession, [EditorDocumentId; 2]) {
        let anchors = ["p.json", "q.json"].map(|path| {
            fs::write(
                self.root.join("prefabs").join(path),
                PrefabDocument::default().to_json_string().unwrap(),
            )
            .unwrap();
            let mut anchor = SceneEntityRecord::new(SceneEntityId::new(path).unwrap());
            anchor.prefab = Some(nara_scene::PrefabInstance {
                source: nara_asset::AssetRef::path(path).unwrap(),
                overrides: Default::default(),
            });
            anchor
        });
        fs::write(
            self.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots(anchors)
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let mut editor = self.editor();
        let documents = ["p.json", "q.json"]
            .map(|name| self.add(&mut editor, EditorDocumentKind::Prefab, name));
        (editor, documents)
    }

    fn add(
        &self,
        editor: &mut EditorProjectSession,
        kind: EditorDocumentKind,
        name: &str,
    ) -> EditorDocumentId {
        let registry = editor
            .trusted_compiled()
            .unwrap()
            .plan
            .schema_validation()
            .registry();
        match kind {
            EditorDocumentKind::Scene => {
                fs::write(
                    self.root.join("scenes").join(name),
                    SceneDocument::default().to_json_string().unwrap(),
                )
                .unwrap();
                let directory = self
                    .capability()
                    .open_directory(&RelativePath::new("scenes").unwrap())
                    .unwrap();
                let opened = ScenePersistenceHost::open(&directory, name, registry).unwrap();
                let document = editor
                    .workspace
                    .open_scene_session(name, opened.session)
                    .unwrap()
                    .opened_document
                    .unwrap();
                assert!(
                    editor
                        .persistence_authority
                        .bind_opened_source_digest(&mut editor.workspace, document, opened.digest)
                        .applied
                );
                assert!(
                    editor
                        .scene_lanes
                        .insert(
                            document,
                            EditorScenePersistenceLane {
                                authoring_source: None,
                                project_path: Arc::from(format!("scenes/{name}")),
                                source_key: nara_scene::SourceDocumentKey::project_location(
                                    "6d57f8c5-867c-5463-b74a-057a8cabe2bc".parse().unwrap(),
                                    AssetPath::new(format!("scenes/{name}")).unwrap(),
                                    nara_scene::SourceDocumentKind::Scene,
                                ),
                                document,
                                host: editor
                                    .authoring_runtime
                                    .host_custody()
                                    .retain_scene(opened.host)
                                    .unwrap(),
                            }
                        )
                        .is_none()
                );
                document
            }
            EditorDocumentKind::Prefab => {
                fs::write(
                    self.root.join("prefabs").join(name),
                    PrefabDocument::default().to_json_string().unwrap(),
                )
                .unwrap();
                let directory = self
                    .capability()
                    .open_directory(&RelativePath::new("prefabs").unwrap())
                    .unwrap();
                let binding = EditorDocumentSourceBinding::new(name.to_owned());
                let opened =
                    PrefabPersistenceHost::open(&directory, binding.clone(), registry).unwrap();
                let document = editor
                    .workspace
                    .open_prefab_session(name, binding, opened.session)
                    .unwrap()
                    .opened_document
                    .unwrap();
                assert!(
                    editor
                        .persistence_authority
                        .bind_opened_source_digest(&mut editor.workspace, document, opened.digest)
                        .applied
                );
                assert!(
                    editor
                        .prefab_lanes
                        .insert(
                            document,
                            EditorPrefabPersistenceLane {
                                authoring_source: None,
                                project_path: Arc::from(format!("prefabs/{name}")),
                                source_key: nara_scene::SourceDocumentKey::project_location(
                                    "6d57f8c5-867c-5463-b74a-057a8cabe2bc".parse().unwrap(),
                                    AssetPath::new(format!("prefabs/{name}")).unwrap(),
                                    nara_scene::SourceDocumentKind::Prefab,
                                ),
                                document,
                                host: editor
                                    .authoring_runtime
                                    .host_custody()
                                    .retain_prefab(opened.host)
                                    .unwrap(),
                            }
                        )
                        .is_none()
                );
                document
            }
            EditorDocumentKind::Extension(_) => unreachable!(),
        }
    }

    fn rebind(&self, editor: &mut EditorProjectSession, document: EditorDocumentId) {
        let registry = editor
            .trusted_compiled()
            .unwrap()
            .plan
            .schema_validation()
            .registry();
        match editor.capture_persistence_lane(document).unwrap() {
            PersistenceLane::Scene(mut lane) => {
                let directory = self
                    .capability()
                    .open_directory(&RelativePath::new("scenes").unwrap())
                    .unwrap();
                let name = lane.project_path.strip_prefix("scenes/").unwrap();
                let opened = ScenePersistenceHost::open(&directory, name, registry).unwrap();
                lane.host = editor
                    .authoring_runtime
                    .host_custody()
                    .retain_scene(opened.host)
                    .unwrap();
                editor.scene_lanes.insert(document, lane);
            }
            PersistenceLane::Prefab(mut lane) => {
                let directory = self
                    .capability()
                    .open_directory(&RelativePath::new("prefabs").unwrap())
                    .unwrap();
                let name = lane.project_path.strip_prefix("prefabs/").unwrap();
                let opened = PrefabPersistenceHost::open(
                    &directory,
                    EditorDocumentSourceBinding::new(name.to_owned()),
                    registry,
                )
                .unwrap();
                lane.host = editor
                    .authoring_runtime
                    .host_custody()
                    .retain_prefab(opened.host)
                    .unwrap();
                editor.prefab_lanes.insert(document, lane);
            }
            PersistenceLane::Extension(_) => unreachable!(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara_document_custody_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn edit(editor: &mut EditorProjectSession, document: EditorDocumentId, name: &str) {
    let base_revision = editor.workspace.document_revision(document).unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new(name).unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let command = match editor.workspace.document_kind(document).unwrap() {
        EditorDocumentKind::Scene => EditorWorkspaceCommand::ApplyScenePatch {
            document,
            base_revision,
            patch,
        },
        EditorDocumentKind::Prefab => EditorWorkspaceCommand::ApplyPrefabPatch {
            document,
            base_revision,
            patch,
        },
        EditorDocumentKind::Extension(_) => unreachable!(),
    };
    let report = editor.apply_workspace_command(command);
    assert!(report.applied, "{:?}", report.diagnostics);
}

fn settle(editor: &mut EditorProjectSession) -> EditorPersistenceResult {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        editor.advance_persistence();
        if let Some(result) = editor.persistence_view().result() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "persistence did not physically return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn acknowledge(editor: &mut EditorProjectSession) {
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
            attempt: editor
                .persistence_view()
                .attempt()
                .expect("the test observed a terminal persistence result")
        }),
        EditorPersistenceRequestResult::Acknowledged { .. }
    ));
}

fn close(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if matches!(
            editor.advance_close(),
            EditorProjectCloseProgress::Complete(_)
        ) {
            return;
        }
        assert!(Instant::now() < deadline, "custody did not retire");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn independently_opened_scene_keeps_its_own_admitted_location_source() {
    let fixture = Fixture::new();
    fs::write(
        fixture.root.join("scenes/second.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let mut editor = fixture.authoring_editor();
    let startup = editor.scene_preview_document.unwrap();
    let index = editor.project_source_index_snapshot().unwrap();
    let result = editor.open_indexed_authoring_document(
        index,
        "scenes/second.scene.json",
        crate::project_host::authoring::AuthoringDocumentKind::Scene,
    );
    let super::super::document_open::EditorDocumentOpenRequestResult::Opened { document } = result
    else {
        panic!("the independent Scene should open: {result:?}");
    };
    let expected = nara_scene::SourceDocumentKey::project_location(
        editor
            .authoring_project()
            .unwrap()
            .settings()
            .project
            .source_universe,
        AssetPath::new("scenes/second.scene.json").unwrap(),
        nara_scene::SourceDocumentKind::Scene,
    );
    assert_eq!(editor.scene_lanes[&document].source_key, expected);
    assert_ne!(
        editor.scene_lanes[&document].source_key,
        editor.scene_lanes[&startup].source_key
    );
    assert_eq!(editor.scene_preview_document, Some(document));
    close(&mut editor);
}

#[test]
fn stable_editor_open_and_reopen_reject_missing_mismatched_duplicate_and_changed_metadata() {
    use super::super::document_open::EditorDocumentOpenRequestResult;
    use crate::project_host::authoring::AuthoringDocumentKind;
    use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};

    let fixture = Fixture::new();
    let manifest = fs::read_to_string(fixture.root.join("nara.toml"))
        .unwrap()
        .replace("project_location", "stable_metadata");
    fs::write(fixture.root.join("nara.toml"), manifest).unwrap();
    let startup_id = StableAssetId::parse_str("a48d60e2-8ede-4103-b721-f6a0533fb99f").unwrap();
    let opened_id = StableAssetId::parse_str("be8c37d3-2cd3-4cdf-9479-004f3cd0866d").unwrap();
    let successor_id = StableAssetId::parse_str("64a2efa1-70cb-4779-8386-dda7c9541bd2").unwrap();
    let metadata = |name: &str, declared: &str, stable_id, kind| {
        fs::write(
            fixture.root.join(format!("scenes/{name}.meta")),
            AssetMeta::new(
                AssetMetaIdentity::new(stable_id, AssetPath::new(declared).unwrap(), kind),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
    };
    metadata(
        "startup.scene.json",
        "startup.scene.json",
        startup_id,
        AssetSourceKind::Scene,
    );
    for name in ["missing", "mismatched", "second", "duplicate"] {
        fs::write(
            fixture.root.join(format!("scenes/{name}.scene.json")),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
    }
    metadata(
        "mismatched.scene.json",
        "other.scene.json",
        successor_id,
        AssetSourceKind::Scene,
    );
    metadata(
        "second.scene.json",
        "second.scene.json",
        opened_id,
        AssetSourceKind::Scene,
    );
    metadata(
        "duplicate.scene.json",
        "duplicate.scene.json",
        opened_id,
        AssetSourceKind::Scene,
    );
    let mut editor = fixture.authoring_editor();
    let index = editor.project_source_index_snapshot().unwrap();
    let startup = editor.scene_preview_document.unwrap();
    for path in ["scenes/missing.scene.json", "scenes/mismatched.scene.json"] {
        assert!(matches!(
            editor.open_indexed_authoring_document(
                Arc::clone(&index),
                path,
                AuthoringDocumentKind::Scene
            ),
            EditorDocumentOpenRequestResult::Rejected { .. },
        ));
        assert_eq!(editor.workspace.len(), 1);
        assert_eq!(editor.scene_preview_document, Some(startup));
    }
    assert!(!fixture.root.join("scenes/missing.scene.json.meta").exists());
    let opened = editor.open_indexed_authoring_document(
        Arc::clone(&index),
        "scenes/second.scene.json",
        AuthoringDocumentKind::Scene,
    );
    let EditorDocumentOpenRequestResult::Opened { document } = opened else {
        panic!("the independently admitted stable Scene should open: {opened:?}");
    };
    let expected = nara_scene::SourceDocumentKey::stable_asset(
        opened_id,
        nara_scene::SourceDocumentKind::Scene,
    );
    assert_eq!(editor.scene_lanes[&document].source_key, expected);
    assert_eq!(
        editor.open_indexed_authoring_document(
            index,
            "scenes/duplicate.scene.json",
            AuthoringDocumentKind::Scene
        ),
        EditorDocumentOpenRequestResult::Rejected {
            code: "project.editor.document-source-identity-conflict"
        },
    );
    assert_eq!(editor.workspace.len(), 2);
    let revision = editor.workspace.document_revision(document).unwrap();
    let gate = editor.hold_next_reopen(ReopenTestPhase::AfterPreparation);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.advance_persistence();
    assert!(gate.wait_until_started());
    metadata(
        "second.scene.json",
        "second.scene.json",
        successor_id,
        AssetSourceKind::Scene,
    );
    gate.release();
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Rejected {
            reason: EditorPersistenceRejection::SourceBindingMismatch,
            ..
        },
    ));
    assert_eq!(editor.workspace.document_revision(document), Some(revision));
    assert_eq!(editor.scene_lanes[&document].source_key, expected);
    close(&mut editor);
}

#[test]
fn indexed_open_rejects_loaded_but_unopened_prefab_and_image_identity_conflicts() {
    use super::super::document_open::EditorDocumentOpenRequestResult;
    use crate::project_host::authoring::AuthoringDocumentKind;
    use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};
    use nara_reflect::{ComponentTypeId, ComponentValue};
    for trusted in [false, true] {
        let fixture = Fixture::new();
        fs::write(
            fixture.root.join("nara.toml"),
            fs::read_to_string(fixture.root.join("nara.toml"))
                .unwrap()
                .replace("project_location", "stable_metadata"),
        )
        .unwrap();
        let startup_id = StableAssetId::parse_str("a48d60e2-8ede-4103-b721-f6a0533fb99f").unwrap();
        let prefab_id = StableAssetId::parse_str("be8c37d3-2cd3-4cdf-9479-004f3cd0866d").unwrap();
        let image_id = StableAssetId::parse_str("64a2efa1-70cb-4779-8386-dda7c9541bd2").unwrap();
        for (path, declaration, id, kind) in [
            (
                "scenes/startup.scene.json",
                "startup.scene.json",
                startup_id,
                AssetSourceKind::Scene,
            ),
            (
                "prefabs/loaded.prefab.json",
                "loaded.prefab.json",
                prefab_id,
                AssetSourceKind::Prefab,
            ),
            (
                "scenes/prefab-conflict.scene.json",
                "prefab-conflict.scene.json",
                prefab_id,
                AssetSourceKind::Scene,
            ),
            (
                "scenes/image-conflict.scene.json",
                "image-conflict.scene.json",
                image_id,
                AssetSourceKind::Scene,
            ),
        ] {
            fs::write(
                fixture.root.join(format!("{path}.meta")),
                AssetMeta::new(
                    AssetMetaIdentity::new(id, AssetPath::new(declaration).unwrap(), kind),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
        }
        fs::write(
            fixture.root.join("prefabs/loaded.prefab.json"),
            PrefabDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        for path in [
            "scenes/prefab-conflict.scene.json",
            "scenes/image-conflict.scene.json",
        ] {
            fs::write(
                fixture.root.join(path),
                SceneDocument::default().to_json_string().unwrap(),
            )
            .unwrap();
        }
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255_u8, 255, 255, 255]))
            .save(fixture.root.join("assets/loaded.png"))
            .unwrap();
        fs::write(
            fixture.root.join("assets/loaded.png.meta"),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    image_id,
                    AssetPath::new("loaded.png").unwrap(),
                    AssetSourceKind::Image,
                ),
                nara_image::ImageImportSettings::default(),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let registry = crate::project_host::authoring::first_party_authoring_preview_registry(
            built_in_schema_providers(),
        )
        .unwrap();
        let component = ComponentTypeId::new("nara.sprite.Sprite");
        let schema = registry.schema(&component).unwrap();
        let mut value = schema.authoring_initial_value().unwrap().clone();
        let ComponentValue::Map(fields) = &mut value else {
            panic!("Sprite is a map")
        };
        let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
            panic!("Sprite has material")
        };
        material.insert(
            "image".into(),
            ComponentValue::map([
                ("kind", ComponentValue::String("path".into())),
                ("value", ComponentValue::String("loaded.png".into())),
            ]),
        );
        let sprite = SceneEntityRecord::new(SceneEntityId::new("sprite").unwrap()).with_component(
            component,
            nara_scene::SceneComponentRecord::new(schema.version, value),
        );
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        anchor.prefab = Some(nara_scene::PrefabInstance {
            source: AssetRef::path("loaded.prefab.json").unwrap(),
            overrides: Default::default(),
        });
        fs::write(
            fixture.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots([sprite, anchor])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let mut editor = if trusted {
            fixture.editor()
        } else {
            fixture.authoring_editor()
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while editor.project_source_index_work_pending() {
            editor.drive_editor_frame(Duration::ZERO);
            assert!(Instant::now() < deadline, "source index must settle");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(editor.prefab_lanes.is_empty());
        match &editor.execution {
            EditorExecutionLane::Authoring(snapshot) => {
                assert_eq!(snapshot.prefab_sources().count(), 1);
                assert_eq!(snapshot.asset_sources().count(), 1);
            }
            EditorExecutionLane::TrustedCompiled(lane) => {
                assert_eq!(lane.snapshot.prefabs().len(), 1);
                assert!(
                    lane.snapshot
                        .asset_database()
                        .record_for_stable_id(image_id)
                        .is_some()
                );
            }
        }
        let index = editor.project_source_index_snapshot();
        let startup = editor.scene_preview_document;
        for (path, id) in [
            ("scenes/prefab-conflict.scene.json", prefab_id),
            ("scenes/image-conflict.scene.json", image_id),
        ] {
            if trusted {
                // Trusted compiled Hosts do not expose Browser Open. Exercise their same owner
                // conflict gate against the actual retained catalogue, not a nonexistent index.
                assert_eq!(
                    editor.exclude_native_source_key(
                        path,
                        &nara_scene::SourceDocumentKey::stable_asset(
                            id,
                            nara_scene::SourceDocumentKind::Scene,
                        )
                    ),
                    Err("project.editor.document-source-identity-conflict"),
                );
            } else {
                assert_eq!(
                    editor.open_indexed_authoring_document(
                        Arc::clone(index.as_ref().unwrap()),
                        path,
                        AuthoringDocumentKind::Scene,
                    ),
                    EditorDocumentOpenRequestResult::Rejected {
                        code: "project.editor.document-source-identity-conflict"
                    },
                );
            }
            assert_eq!(editor.workspace.len(), 1);
            assert_eq!(editor.scene_preview_document, startup);
        }
        if trusted {
            let document = startup.unwrap();
            let anchor = SceneEntityId::new("anchor").unwrap();
            assert!(
                editor
                    .apply_workspace_command(EditorWorkspaceCommand::SelectEntity {
                        document,
                        entity: Some(anchor.clone()),
                    })
                    .applied
            );
            let revision = editor.workspace.document_revision(document).unwrap();
            let report = editor.open_prefab_source(document, revision, &anchor);
            assert!(
                report.applied && report.opened_document.is_some(),
                "{report:?}"
            );
        } else {
            assert!(matches!(
                editor.open_indexed_authoring_document(
                    index.unwrap(),
                    "prefabs/loaded.prefab.json",
                    AuthoringDocumentKind::Prefab,
                ),
                EditorDocumentOpenRequestResult::Opened { .. }
            ));
        }
        assert_eq!(editor.workspace.len(), 2);
        close(&mut editor);
    }
}

#[test]
fn reopen_rejects_same_key_metadata_byte_and_file_replacements_after_preparation() {
    use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        for replace_object in [false, true] {
            let fixture = Fixture::new();
            fs::write(
                fixture.root.join("nara.toml"),
                fs::read_to_string(fixture.root.join("nara.toml"))
                    .unwrap()
                    .replace("project_location", "stable_metadata"),
            )
            .unwrap();
            let startup_id =
                StableAssetId::parse_str("a48d60e2-8ede-4103-b721-f6a0533fb99f").unwrap();
            fs::write(
                fixture.root.join("scenes/startup.scene.json.meta"),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        startup_id,
                        AssetPath::new("startup.scene.json").unwrap(),
                        AssetSourceKind::Scene,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
            let (path, logical, source_kind, document_kind) = match kind {
                EditorDocumentKind::Scene => (
                    "scenes/second.scene.json",
                    "second.scene.json",
                    AssetSourceKind::Scene,
                    crate::project_host::authoring::AuthoringDocumentKind::Scene,
                ),
                EditorDocumentKind::Prefab => (
                    "prefabs/second.prefab.json",
                    "second.prefab.json",
                    AssetSourceKind::Prefab,
                    crate::project_host::authoring::AuthoringDocumentKind::Prefab,
                ),
                _ => unreachable!(),
            };
            let bytes = if kind == EditorDocumentKind::Scene {
                SceneDocument::default().to_json_string().unwrap()
            } else {
                PrefabDocument::default().to_json_string().unwrap()
            };
            fs::write(fixture.root.join(path), &bytes).unwrap();
            let metadata_path = fixture.root.join(format!("{path}.meta"));
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    StableAssetId::parse_str("be8c37d3-2cd3-4cdf-9479-004f3cd0866d").unwrap(),
                    AssetPath::new(logical).unwrap(),
                    source_kind,
                ),
                (),
            )
            .to_json_string()
            .unwrap();
            fs::write(&metadata_path, &metadata).unwrap();
            let mut editor = fixture.authoring_editor();
            let index = editor.project_source_index_snapshot().unwrap();
            let super::super::document_open::EditorDocumentOpenRequestResult::Opened { document } =
                editor.open_indexed_authoring_document(index, path, document_kind)
            else {
                panic!("a stable native document must open")
            };
            edit(&mut editor, document, "unsaved");
            let revision = editor.workspace.document_revision(document);
            let old_session = match kind {
                EditorDocumentKind::Scene => editor
                    .workspace
                    .scene(document)
                    .unwrap()
                    .session()
                    .document()
                    .to_json_string()
                    .unwrap(),
                EditorDocumentKind::Prefab => editor
                    .workspace
                    .prefab(document)
                    .unwrap()
                    .session()
                    .document()
                    .to_json_string()
                    .unwrap(),
                _ => unreachable!(),
            };
            let gate = editor.hold_next_reopen(ReopenTestPhase::AfterPreparation);
            assert!(matches!(
                editor.request_persistence(EditorPersistenceCommand::Reopen {
                    document: Some(document)
                }),
                EditorPersistenceRequestResult::Accepted { .. }
            ));
            editor.advance_persistence();
            assert!(gate.wait_until_started());
            if replace_object {
                fs::rename(&metadata_path, metadata_path.with_extension("previous")).unwrap();
                fs::write(&metadata_path, &metadata).unwrap();
            } else {
                fs::write(&metadata_path, format!("{metadata}\n")).unwrap();
            }
            gate.release();
            assert!(matches!(
                settle(&mut editor),
                EditorPersistenceResult::Rejected {
                    reason: EditorPersistenceRejection::SourceBindingMismatch,
                    ..
                }
            ));
            assert_eq!(editor.workspace.document_revision(document), revision);
            let retained_session = match kind {
                EditorDocumentKind::Scene => editor
                    .workspace
                    .scene(document)
                    .unwrap()
                    .session()
                    .document()
                    .to_json_string()
                    .unwrap(),
                EditorDocumentKind::Prefab => editor
                    .workspace
                    .prefab(document)
                    .unwrap()
                    .session()
                    .document()
                    .to_json_string()
                    .unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(retained_session, old_session);
            assert_eq!(fs::read_to_string(fixture.root.join(path)).unwrap(), bytes);
            close(&mut editor);
        }
    }
}

#[test]
fn non_active_scene_and_prefab_persistence_keeps_each_captured_owner() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let preview = editor.scene_preview_document;
        let a = fixture.add(&mut editor, kind, "a.json");
        let b = fixture.add(&mut editor, kind, "b.json");
        editor.workspace.activate_document(a);
        edit(&mut editor, b, "captured-b");
        let captured_revision = editor.workspace.document_revision(b).unwrap();
        let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(b) }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        editor.advance_persistence();
        assert!(gate.wait_until_started());
        edit(&mut editor, a, "later-a");
        edit(&mut editor, b, "later-b");
        gate.release();
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Saved { document, revision, .. } if document == b && revision == captured_revision)
        );
        let directory = if kind == EditorDocumentKind::Scene {
            "scenes"
        } else {
            "prefabs"
        };
        let path = fixture.root.join(directory).join("b.json");
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("captured-b"));
        assert!(!saved.contains("later-b"));
        assert!(
            !fs::read_to_string(fixture.root.join(directory).join("a.json"))
                .unwrap()
                .contains("later-a")
        );
        assert_eq!(editor.workspace.document_is_dirty(b), Some(true));
        let a_revision = editor.workspace.document_revision(a);
        acknowledge(&mut editor);
        let gate = editor.hold_next_reopen(ReopenTestPhase::AfterPreparation);
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Reopen { document: Some(b) }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        editor.advance_persistence();
        assert!(gate.wait_until_started());
        edit(&mut editor, a, "reopen-unrelated-a");
        let later_a_revision = editor.workspace.document_revision(a);
        assert_ne!(later_a_revision, a_revision);
        gate.release();
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Opened { document, .. } if document == b)
        );
        assert_eq!(editor.workspace.document_is_dirty(b), Some(false));
        assert_eq!(editor.workspace.document_revision(a), later_a_revision);
        assert_eq!(editor.workspace.document_is_dirty(a), Some(true));
        assert_eq!(editor.workspace.active_document(), Some(a));
        assert_eq!(editor.scene_preview_document, preview);
        acknowledge(&mut editor);
        edit(&mut editor, b, "after-reopen-b");
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(b) }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Saved { document, .. } if document == b)
        );
        close(&mut editor);
    }
}

#[test]
fn prepared_reopen_rejects_rebound_or_retired_exact_host() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        for retired in [false, true] {
            let fixture = Fixture::new();
            let mut editor = fixture.editor();
            let document = fixture.add(&mut editor, kind, "original.json");
            edit(&mut editor, document, "working");
            let revision = editor.workspace.document_revision(document);
            let gate = editor.hold_next_reopen(ReopenTestPhase::AfterPreparation);
            assert!(matches!(
                editor.request_persistence(EditorPersistenceCommand::Reopen {
                    document: Some(document)
                }),
                EditorPersistenceRequestResult::Accepted { .. }
            ));
            editor.advance_persistence();
            assert!(gate.wait_until_started());
            if retired {
                match editor.capture_persistence_lane(document).unwrap() {
                    PersistenceLane::Scene(lane) => assert!(lane.host.retire()),
                    PersistenceLane::Prefab(lane) => assert!(lane.host.retire()),
                    PersistenceLane::Extension(_) => unreachable!(),
                }
            } else {
                fixture.rebind(&mut editor, document);
            }
            gate.release();
            assert_eq!(
                settle(&mut editor),
                EditorPersistenceResult::Rejected {
                    request: EditorPersistenceRequestKind::Reopen,
                    document: Some(document),
                    reason: EditorPersistenceRejection::SourceBindingMismatch
                }
            );
            assert_eq!(editor.workspace.document_revision(document), revision);
            assert_eq!(editor.workspace.document_is_dirty(document), Some(true));
            close(&mut editor);
        }
    }
}

#[test]
fn queued_save_cannot_acquire_a_replacement_host_after_admission() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let document = fixture.add(&mut editor, kind, "queued.json");
        edit(&mut editor, document, "must-not-publish");
        let EditorPersistenceRequestResult::Accepted { attempt } =
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(document),
            })
        else {
            panic!("queued Save must admit its own attempt")
        };
        match editor.capture_persistence_lane(document).unwrap() {
            PersistenceLane::Scene(lane) => assert!(lane.host.retire()),
            PersistenceLane::Prefab(lane) => assert!(lane.host.retire()),
            PersistenceLane::Extension(_) => unreachable!(),
        }
        fixture.rebind(&mut editor, document);
        let result = settle(&mut editor);
        assert_eq!(
            result,
            EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                reason: EditorPersistenceRejection::Busy,
            }
        );
        assert_eq!(
            editor.persistence_view(),
            EditorPersistenceView::Terminal { attempt, result }
        );
        let directory = if kind == EditorDocumentKind::Scene {
            "scenes"
        } else {
            "prefabs"
        };
        assert!(
            !fs::read_to_string(fixture.root.join(directory).join("queued.json"))
                .unwrap()
                .contains("must-not-publish")
        );
        assert_eq!(editor.workspace.document_is_dirty(document), Some(true));
        close(&mut editor);
    }
}

fn direct_save(
    lane: &PersistenceLane,
    checkpoint: nara_tooling::EditorPersistenceCheckpoint,
) -> PersistenceSaveOutcome {
    match lane {
        PersistenceLane::Scene(lane) => lane
            .host
            .try_with(|host| host.save_owned(SceneSaveCandidate { checkpoint }, None))
            .unwrap(),
        PersistenceLane::Prefab(lane) => lane
            .host
            .try_with(|host| host.save_owned(PrefabSaveCandidate { checkpoint }, None))
            .unwrap(),
        PersistenceLane::Extension(_) => unreachable!(),
    }
}

#[test]
fn rejected_acknowledgement_marks_only_the_original_published_host_uncertain() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let document = fixture.add(&mut editor, kind, "uncertain.json");
        edit(&mut editor, document, "published");
        let original = editor.capture_persistence_lane(document).unwrap();
        let gate = editor.hold_next_save(SaveTestPhase::AfterPublication);
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(document)
            }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        editor.advance_persistence();
        assert!(gate.wait_until_started());
        fixture.rebind(&mut editor, document);
        let replacement = editor.capture_persistence_lane(document).unwrap();
        let original_retry = editor
            .persistence_authority
            .capture(&editor.workspace, document)
            .unwrap();
        let replacement_retry = editor
            .persistence_authority
            .capture(&editor.workspace, document)
            .unwrap();
        assert!(editor.workspace.__discard_document(document).applied);
        gate.release();
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::PersistenceUncertain { document: target, .. } if target == document)
        );
        assert!(matches!(
            direct_save(&original, original_retry),
            PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile)
        ));
        assert!(matches!(
            direct_save(&replacement, replacement_retry),
            PersistenceSaveOutcome::Saved { .. }
        ));
        close(&mut editor);
    }
}

#[test]
fn close_releases_only_the_exact_document_lane_and_preserves_preview_binding() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let preview = editor.scene_preview_document;
        let a = fixture.add(&mut editor, kind, "remaining.json");
        let b = fixture.add(&mut editor, kind, "closing.json");
        let remaining = editor.capture_persistence_lane(a).unwrap();
        let closing = editor.capture_persistence_lane(b).unwrap();
        editor.workspace.activate_document(a);
        assert_eq!(
            editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: b }),
            EditorWorkspaceIntentRequestResult::Accepted
        );
        editor.advance_workspace_retirement();
        assert_eq!(
            editor.workspace_intent_result,
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::CloseDocument { document: b }
            })
        );
        assert!(editor.capture_persistence_lane(b).is_err());
        assert!(editor.workspace.document_kind(b).is_none());
        match closing {
            PersistenceLane::Scene(lane) => assert!(lane.host.try_with(|_| ()).is_none()),
            PersistenceLane::Prefab(lane) => assert!(lane.host.try_with(|_| ()).is_none()),
            PersistenceLane::Extension(_) => unreachable!(),
        }
        match remaining {
            PersistenceLane::Scene(lane) => assert!(lane.host.try_with(|_| ()).is_some()),
            PersistenceLane::Prefab(lane) => assert!(lane.host.try_with(|_| ()).is_some()),
            PersistenceLane::Extension(_) => unreachable!(),
        }
        assert_eq!(editor.workspace.active_document(), Some(a));
        assert_eq!(editor.scene_preview_document, preview);
        edit(&mut editor, a, "still-usable");
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(a) }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Saved { document, .. } if document == a)
        );
        close(&mut editor);
    }
}

#[cfg(windows)]
struct ReleaseCleanup(crate::project_host::persistence::CleanupTestControl);

#[cfg(windows)]
impl Drop for ReleaseCleanup {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[cfg(windows)]
#[test]
fn held_cleanup_keeps_only_its_document_open_while_another_owner_can_save() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let a = fixture.add(&mut editor, kind, "usable.json");
        let b = fixture.add(&mut editor, kind, "held.json");
        editor.workspace.activate_document(a);
        edit(&mut editor, b, "failed-save-b");
        let cleanup = ReleaseCleanup(match kind {
            EditorDocumentKind::Scene => editor.test_fail_next_save_after_write(b),
            EditorDocumentKind::Prefab => editor.test_fail_next_prefab_save_after_write(b),
            EditorDocumentKind::Extension(_) => unreachable!(),
        });
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(b) }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Failed { document: Some(document), .. } if document == b)
        );
        acknowledge(&mut editor);
        let discard = |editor: &mut EditorProjectSession| {
            assert_eq!(
                editor
                    .request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: b }),
                EditorWorkspaceIntentRequestResult::Accepted
            );
            let prompt = editor.workspace_intent_view().decision_prompt().unwrap();
            assert_eq!(
                editor.resolve_workspace_intent(prompt, EditorCloseDecision::Discard),
                EditorWorkspaceIntentRequestResult::Accepted
            );
            editor.advance_workspace_retirement();
        };
        discard(&mut editor);
        assert_eq!(
            editor.workspace_intent_result,
            Some(EditorWorkspaceIntentResult::Rejected {
                intent: EditorWorkspaceIntent::CloseDocument { document: b },
                reason: EditorWorkspaceIntentRejection::PersistenceRejected(
                    EditorPersistenceRejection::CleanupPending
                ),
            })
        );
        assert!(editor.acknowledge_workspace_intent_result());
        assert_eq!(editor.workspace.document_kind(b), Some(kind));
        assert!(editor.capture_persistence_lane(b).is_ok());
        edit(&mut editor, a, "saved-a");
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(a) }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Saved { document, .. } if document == a)
        );
        assert_eq!(editor.workspace.active_document(), Some(a));
        acknowledge(&mut editor);
        cleanup.0.release();
        discard(&mut editor);
        assert_eq!(
            editor.workspace_intent_result,
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::CloseDocument { document: b }
            })
        );
        assert!(editor.capture_persistence_lane(b).is_err());
        assert_eq!(editor.workspace.document_kind(b), None);
        close(&mut editor);
    }
}

#[test]
fn explicit_scene_selection_owns_preview_binding_and_close_never_falls_back() {
    use nara_tooling::{EditorAction, EditorActionBatch, EditorNavigationAction};
    let fixture = Fixture::new();
    let mut editor = fixture.editor();
    let a = editor.scene_preview_document.unwrap();
    let b = fixture.add(&mut editor, EditorDocumentKind::Scene, "second.json");
    let prefab = fixture.add(&mut editor, EditorDocumentKind::Prefab, "inspected.json");
    let select = |editor: &mut EditorProjectSession, document| {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Navigate(
                    EditorNavigationAction::SetActiveDocument { document },
                )],
            ))
            .unwrap();
    };
    select(&mut editor, b);
    let bound_b = editor.scene_preview_document;
    let bound_frame = editor.drive_editor_frame(Duration::ZERO);
    select(&mut editor, prefab);
    let retained_b = editor.scene_preview_document;
    let prefab_frame = editor.drive_editor_frame(Duration::ZERO);
    select(&mut editor, a);
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: a }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.advance_workspace_retirement();
    let after_close = editor.scene_preview_document;
    let active_after_close = editor.workspace.active_document();
    assert!(editor.acknowledge_workspace_intent_result());
    select(&mut editor, b);
    let reselected_b = editor.scene_preview_document;
    close(&mut editor);
    assert_eq!(bound_b, Some(b));
    assert_eq!(retained_b, Some(b));
    assert_eq!(bound_frame.scene_preview_document(), Some(b));
    assert_eq!(prefab_frame.scene_preview_document(), Some(b));
    assert_eq!(
        prefab_frame.workspace().active_scene.as_ref().unwrap().kind,
        EditorDocumentKind::Prefab
    );
    assert_eq!(after_close, None);
    assert_eq!(active_after_close, Some(b));
    assert_eq!(reselected_b, Some(b));
}

struct SourceProbe {
    bytes: Box<[u8]>,
    state: nara_core::DocumentContentState,
}

fn probe_error() -> nara_tooling::EditorExtensionDocumentError {
    nara_tooling::EditorExtensionDocumentError::new(
        nara_diagnostic::DiagnosticCode::new("custody.probe-read-only").unwrap(),
        nara_diagnostic::SafeSummary::new("The source probe has no editable commands").unwrap(),
    )
}

impl nara_tooling::EditorExtensionDocument for SourceProbe {
    type Command = ();
    type View = ();
    const TYPE_ID: &'static str = "nara.test.SourceProbe";
    fn title(&self) -> &str {
        "Source probe"
    }
    fn content_state(&self) -> nara_core::DocumentContentState {
        self.state
    }
    fn history_status(&self) -> nara_core::DocumentHistoryStatus {
        Default::default()
    }
    fn view(&self) {}
    fn apply(&mut self, _: &()) -> Result<(), nara_tooling::EditorExtensionDocumentError> {
        Err(probe_error())
    }
    fn undo(&mut self) -> Result<(), nara_tooling::EditorExtensionDocumentError> {
        Err(probe_error())
    }
    fn redo(&mut self) -> Result<(), nara_tooling::EditorExtensionDocumentError> {
        Err(probe_error())
    }
    fn encode(&self) -> Result<Vec<u8>, nara_tooling::EditorExtensionDocumentError> {
        Ok(self.bytes.to_vec())
    }
    fn decode(bytes: &[u8]) -> Result<Self, nara_tooling::EditorExtensionDocumentError> {
        Ok(Self {
            bytes: bytes.into(),
            state: nara_core::DocumentRevision::initial().content_state(),
        })
    }
}

impl Fixture {
    fn add_probe(&self, editor: &mut EditorProjectSession, name: &str) -> EditorDocumentId {
        fs::create_dir_all(self.root.join("probes")).unwrap();
        let source = format!("probes/{name}");
        fs::write(
            self.root.join(&source),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        editor
            .open_extension_document::<SourceProbe>(&source)
            .unwrap()
    }
}

#[test]
fn source_exclusion_checks_later_owners_across_every_source_domain() {
    use nara_tooling::EditorExtensionDocument;
    for kind in [
        EditorDocumentKind::Scene,
        EditorDocumentKind::Prefab,
        EditorDocumentKind::Extension(SourceProbe::TYPE_ID),
    ] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let source = match kind {
            EditorDocumentKind::Scene | EditorDocumentKind::Prefab => {
                fixture.add(&mut editor, kind, "first.json");
                fixture.add(&mut editor, kind, "owner.json");
                format!(
                    "{}/owner.json",
                    if kind == EditorDocumentKind::Scene {
                        "scenes"
                    } else {
                        "prefabs"
                    }
                )
            }
            EditorDocumentKind::Extension(_) => {
                editor.register_extension_document::<SourceProbe>().unwrap();
                fixture.add_probe(&mut editor, "first.json");
                fixture.add_probe(&mut editor, "owner.json");
                "probes/owner.json".to_owned()
            }
        };
        let active = editor.workspace.active_document();
        for alias in [
            "scenes/alias.scene.json",
            "prefabs/alias.prefab.json",
            "alias.extension.json",
        ] {
            fs::hard_link(fixture.root.join(&source), fixture.root.join(alias)).unwrap();
            let current = fixture
                .capability()
                .open_file(&RelativePath::new(alias).unwrap())
                .unwrap();
            assert_eq!(
                editor.exclude_source_aliases(alias, &current),
                Err(SourceAdmissionError::AlreadyOpen),
                "owner: {kind:?}, candidate: {alias}"
            );
            assert_eq!(editor.workspace.active_document(), active);
        }
        close(&mut editor);
    }
}

#[test]
fn source_exclusion_refuses_busy_or_retired_hosts_in_every_domain() {
    use nara_tooling::EditorExtensionDocument;
    for kind in [
        EditorDocumentKind::Scene,
        EditorDocumentKind::Prefab,
        EditorDocumentKind::Extension(SourceProbe::TYPE_ID),
    ] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let document = if matches!(kind, EditorDocumentKind::Extension(_)) {
            editor.register_extension_document::<SourceProbe>().unwrap();
            fixture.add_probe(&mut editor, "busy.json")
        } else {
            fixture.add(&mut editor, kind, "busy.json")
        };
        fs::write(fixture.root.join("candidate.json"), b"unclaimed").unwrap();
        let current = fixture
            .capability()
            .open_file(&RelativePath::new("candidate.json").unwrap())
            .unwrap();
        let lane = editor.capture_persistence_lane(document).unwrap();
        let result = match &lane {
            PersistenceLane::Scene(lane) => lane
                .host
                .try_with(|_| editor.exclude_source_aliases("candidate.json", &current))
                .unwrap(),
            PersistenceLane::Prefab(lane) => lane
                .host
                .try_with(|_| editor.exclude_source_aliases("candidate.json", &current))
                .unwrap(),
            PersistenceLane::Extension(lane) => lane
                .host
                .try_with(|_| editor.exclude_source_aliases("candidate.json", &current))
                .unwrap(),
        };
        assert_eq!(result, Err(SourceAdmissionError::HostUnavailable));
        match &lane {
            PersistenceLane::Scene(lane) => assert!(lane.host.retire()),
            PersistenceLane::Prefab(lane) => assert!(lane.host.retire()),
            PersistenceLane::Extension(lane) => assert!(lane.host.retire()),
        }
        assert_eq!(
            editor.exclude_source_aliases("candidate.json", &current),
            Err(SourceAdmissionError::HostUnavailable)
        );
        close(&mut editor);
    }
}

#[test]
fn source_exclusion_observes_replacement_and_preserves_live_source_custody() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let document = fixture.add(&mut editor, kind, "saved.json");
        let directory = if kind == EditorDocumentKind::Scene {
            "scenes"
        } else {
            "prefabs"
        };
        let source = fixture.root.join(directory).join("saved.json");
        fs::hard_link(&source, fixture.root.join("old.json")).unwrap();
        edit(&mut editor, document, "new-source");
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(document)
            }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        assert!(matches!(
            settle(&mut editor),
            EditorPersistenceResult::Saved { .. }
        ));
        let old = fixture
            .capability()
            .open_file(&RelativePath::new("old.json").unwrap())
            .unwrap();
        assert_eq!(editor.exclude_source_aliases("old.json", &old), Ok(()));
        fs::hard_link(&source, fixture.root.join("current.json")).unwrap();
        let current = fixture
            .capability()
            .open_file(&RelativePath::new("current.json").unwrap())
            .unwrap();
        let renamed = fs::rename(fixture.root.join(directory), fixture.root.join("moved"));
        // Windows refuses this directory rename while its accepted leaf remains open.
        // Retain the source proof rather than dropping it to make the rename succeed.
        #[cfg(windows)]
        assert_eq!(renamed.unwrap_err().raw_os_error(), Some(5));
        #[cfg(not(windows))]
        renamed.unwrap();
        assert_eq!(
            editor.exclude_source_aliases("current.json", &current),
            Err(SourceAdmissionError::AlreadyOpen)
        );
        close(&mut editor);
        #[cfg(windows)]
        fs::rename(fixture.root.join(directory), fixture.root.join("moved")).unwrap();
    }
}

#[test]
fn trusted_play_divergence_follows_its_source_while_inspecting_a_prefab() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor();
    let startup_key = editor
        .trusted_compiled()
        .unwrap()
        .snapshot
        .selected_scene_source_key()
        .clone();
    let source = fixture.add(&mut editor, EditorDocumentKind::Scene, "play-selected.json");
    edit(&mut editor, source, "play-source");
    let expected_source_key = editor.scene_lanes[&source].source_key.clone();
    assert_ne!(expected_source_key, startup_key);
    let prefab = fixture.add(&mut editor, EditorDocumentKind::Prefab, "inspected.json");
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument { document: source })
            .applied
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.play_view().state() != EditorPlayState::Running {
        assert!(
            Instant::now() < deadline,
            "trusted Play did not reach Running: {:?}",
            editor.play_view()
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    {
        let runtime = editor
            .trusted_compiled_mut()
            .unwrap()
            .runtime_host
            .running_runtime_mut()
            .unwrap();
        let current = crate::startup_scene::CurrentScene::get(runtime.world()).unwrap();
        current
            .source_view()
            .with_expanded_scene(|candidate, _| {
                assert_eq!(candidate.source_key(), &expected_source_key);
            })
            .unwrap();
        let mut count = 0;
        for entity in runtime.world().iter_entities() {
            if let Some(materialized) = entity.get::<nara_scene::SceneEntitySource>() {
                count += 1;
                assert_eq!(materialized.containing_source(), &expected_source_key);
                assert_eq!(
                    materialized.occurrence(),
                    &nara_scene::SceneOccurrenceKey::new("nara.product", "current-scene").unwrap(),
                );
            }
        }
        assert_eq!(count, 1);
    }
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument { document: prefab })
            .applied
    );
    edit(&mut editor, source, "source-changed");
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let out_of_date = frame.play().is_out_of_date();
    let observed_revision = frame.play().current_revision();
    let expected_revision = editor.workspace.document_revision(source);
    let _ = editor.request_play(EditorPlayCommand::AcknowledgeResult);
    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.play_view().state() != EditorPlayState::Empty {
        assert!(Instant::now() < deadline, "trusted Stop did not retire");
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    close(&mut editor);
    assert!(
        out_of_date,
        "navigation must not hide a later edit of the retained Play source"
    );
    assert_eq!(observed_revision, expected_revision);
}

#[test]
fn failed_prefab_publication_restores_the_exact_active_document_and_preview() {
    use nara_tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorNavigationAction,
    };
    let fixture = Fixture::new();
    let anchor_id = SceneEntityId::new("anchor").unwrap();
    let mut anchor = SceneEntityRecord::new(anchor_id.clone());
    anchor.prefab = Some(nara_scene::PrefabInstance {
        source: nara_asset::AssetRef::path("rollback.prefab.json").unwrap(),
        overrides: Default::default(),
    });
    fs::write(
        fixture.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.root.join("prefabs/rollback.prefab.json"),
        PrefabDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let mut editor = fixture.editor();
    let scene = editor.workspace.active_document().unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::SelectEntity {
                document: scene,
                entity: Some(anchor_id.clone())
            })
            .applied
    );
    let active = fixture.add(&mut editor, EditorDocumentKind::Scene, "active.json");
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument { document: active })
            .applied
    );
    let frame = editor.drive_editor_frame(Duration::ZERO);
    editor.test_reject_next_prefab_source_binding();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::OpenPrefabSource {
                    document: scene,
                    base_revision: editor.workspace.document_revision(scene).unwrap(),
                    entity: anchor_id,
                },
            )],
        ))
        .unwrap();
    let after = editor.drive_editor_frame(Duration::ZERO);
    let binding = editor.scene_preview_document;
    close(&mut editor);
    assert!(
        matches!(&report.outcomes()[0], EditorActionOutcome::Workspace(report) if !report.applied)
    );
    assert_eq!(after.workspace(), frame.workspace());
    assert_eq!(after.workspace().active_document, Some(active));
    assert_eq!(binding, Some(active));
}

#[test]
fn later_prefab_refresh_cannot_erase_a_failed_closed_source_obligation() {
    for reopen_current_source in [false, true] {
        let fixture = Fixture::new();
        let (mut editor, [p, q]) = fixture.two_prefabs();
        let original_content = editor.trusted_compiled().unwrap().snapshot.revision();
        edit(&mut editor, p, "saved-p");
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save { document: Some(p) }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        assert!(matches!(
            settle(&mut editor),
            EditorPersistenceResult::Saved { .. }
        ));
        acknowledge(&mut editor);
        let saved_p = fs::read(fixture.root.join("prefabs/p.json")).unwrap();
        let mut external_p = saved_p.clone();
        external_p.extend_from_slice(b"\n");
        fs::write(fixture.root.join("prefabs/p.json"), external_p).unwrap();
        editor.advance_project_content_refresh();
        assert!(matches!(
            editor.project_content,
            EditorProjectContentView::Stale { .. }
        ));
        assert_eq!(
            editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: p }),
            EditorWorkspaceIntentRequestResult::Accepted
        );
        editor.advance_workspace_retirement();
        assert!(editor.workspace.prefab(p).is_none());
        assert!(editor.acknowledge_workspace_intent_result());

        let command = if reopen_current_source {
            EditorPersistenceCommand::Reopen { document: Some(q) }
        } else {
            edit(&mut editor, q, "saved-q");
            EditorPersistenceCommand::Save { document: Some(q) }
        };
        assert!(matches!(
            editor.request_persistence(command),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        assert!(matches!(
            settle(&mut editor),
            EditorPersistenceResult::Saved { .. } | EditorPersistenceResult::Opened { .. }
        ));
        acknowledge(&mut editor);
        editor.advance_project_content_refresh();
        let after_q = editor.project_content;
        let retained_content = editor.trusted_compiled().unwrap().snapshot.revision();

        fs::write(fixture.root.join("prefabs/p.json"), saved_p).unwrap();
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Reopen { document: Some(q) }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        assert!(matches!(
            settle(&mut editor),
            EditorPersistenceResult::Opened { .. }
        ));
        acknowledge(&mut editor);
        editor.advance_project_content_refresh();
        let reconciled = editor.project_content;
        close(&mut editor);

        assert!(
            matches!(
                after_q,
                EditorProjectContentView::Stale {
                    failure: nara_tooling::EditorProjectContentRefreshFailure::SavedSourceChanged,
                    ..
                }
            ),
            "Q must not erase P's failed obligation after P closes: {after_q:?}; Reopen: {reopen_current_source}"
        );
        assert_eq!(retained_content, original_content);
        assert!(
            reconciled.is_current(),
            "all retained obligations should now match: {reconciled:?}"
        );
    }
}

#[test]
fn refresh_checks_later_sources_and_supersedes_only_the_same_source_obligation() {
    let fixture = Fixture::new();
    let (mut editor, [p, q]) = fixture.two_prefabs();
    let original_content = editor.trusted_compiled().unwrap().snapshot.revision();
    for (document, entity) in [(p, "p-first"), (q, "q-saved"), (p, "p-later")] {
        edit(&mut editor, document, entity);
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(document)
            }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        assert!(matches!(
            settle(&mut editor),
            EditorPersistenceResult::Saved { .. }
        ));
        acknowledge(&mut editor);
    }
    let saved_q = fs::read(fixture.root.join("prefabs/q.json")).unwrap();
    let mut external_q = saved_q.clone();
    external_q.extend_from_slice(b"\n");
    fs::write(fixture.root.join("prefabs/q.json"), external_q).unwrap();
    editor.advance_project_content_refresh();
    let changed_source = editor.project_content;
    let changed_content = editor.trusted_compiled().unwrap().snapshot.revision();
    fs::write(fixture.root.join("prefabs/q.json"), saved_q).unwrap();
    let scene = fixture.root.join("scenes/startup.scene.json");
    let startup = fs::read(&scene).unwrap();
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("p.json").unwrap());
    anchor.prefab = Some(nara_scene::PrefabInstance {
        source: nara_asset::AssetRef::path("p.json").unwrap(),
        overrides: Default::default(),
    });
    fs::write(
        &scene,
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen { document: Some(p) }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Opened { .. }
    ));
    acknowledge(&mut editor);
    editor.advance_project_content_refresh();
    let missing_source = editor.project_content;
    let missing_content = editor.trusted_compiled().unwrap().snapshot.revision();
    let missing_diagnostic = editor.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.saved-prefab-outside-closure"
    });
    fs::write(scene, startup).unwrap();
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen { document: Some(p) }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Opened { .. }
    ));
    acknowledge(&mut editor);
    editor.advance_project_content_refresh();
    let reconciled = editor.project_content;
    close(&mut editor);
    for state in [changed_source, missing_source] {
        assert!(
            matches!(state, EditorProjectContentView::Stale {
            document,
            failure: nara_tooling::EditorProjectContentRefreshFailure::SavedSourceChanged,
            ..
        } if document == p),
            "the representative is P but Q must still reject the complete refresh: {state:?}"
        );
    }
    assert_eq!(changed_content, original_content);
    assert_eq!(missing_content, original_content);
    assert!(missing_diagnostic);
    assert!(
        reconciled.is_current(),
        "the latest P and saved Q must reconcile together: {reconciled:?}"
    );
}

#[test]
fn expected_prefab_copy_is_budgeted_before_consuming_the_remaining_inputs() {
    use crate::project_host::{
        ProjectContentBudgetKind, ProjectContentErrorKind, ProjectContentLimits,
        ProjectContentLoader,
    };
    use std::cell::Cell;
    for (limit, expected_visits) in [(1, 1), (512, 2)] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let lane = editor.trusted_compiled().unwrap();
        let limits = ProjectContentLimits::default()
            .with_limit(ProjectContentBudgetKind::WorkBytes, limit)
            .unwrap();
        let loader = ProjectContentLoader::with_limits(fixture.capability(), limits).unwrap();
        let agreement = crate::project_host::runtime_agreement::prepare_runtime_schema_agreement(
            &fixture.capability(),
            &lane.content_candidate,
            &lane.plan,
        )
        .unwrap();
        let paths = ["first.json", "second.json", "third.json"].map(|file| {
            nara_asset::AssetPath::new(format!("{}{}", "nested/".repeat(30), file)).unwrap()
        });
        let visited = Cell::new(0);
        let result = loader.load_with_expected_prefab_digests_and_runtime_agreement(
            &lane.content_candidate,
            &lane.plan,
            paths.iter().map(|path| {
                visited.set(visited.get() + 1);
                (path, ContentDigest::of_bytes(b"expected"))
            }),
            agreement,
        );
        let snapshot = loader.budget_snapshot();
        close(&mut editor);
        assert_eq!(
            result.err().unwrap().kind(),
            ProjectContentErrorKind::BudgetExceeded
        );
        assert_eq!(
            visited.get(),
            expected_visits,
            "budget rejection must not eagerly copy the entire source set"
        );
        assert_eq!(snapshot.active_reservations(), 0);
        assert_eq!(snapshot.active_generations(), 0);
        for kind in ProjectContentBudgetKind::ALL {
            assert_eq!(snapshot.active(*kind), 0, "{kind:?} charge leaked");
        }
        assert_eq!(
            snapshot.high_water(ProjectContentBudgetKind::WorkBytes) > 0,
            limit > 1
        );
    }
}

#[test]
fn trusted_anchor_open_uses_its_retained_prefab_directory_after_rename() {
    use nara_tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorNavigationAction,
    };
    let fixture = Fixture::new();
    let anchor_id = SceneEntityId::new("anchor").unwrap();
    let mut anchor = SceneEntityRecord::new(anchor_id.clone());
    anchor.prefab = Some(nara_scene::PrefabInstance {
        source: nara_asset::AssetRef::path("retained.prefab.json").unwrap(),
        overrides: Default::default(),
    });
    fs::write(
        fixture.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.root.join("prefabs/retained.prefab.json"),
        PrefabDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let mut editor = fixture.editor();
    let scene = editor.workspace.active_document().unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::SelectEntity {
                document: scene,
                entity: Some(anchor_id.clone())
            })
            .applied
    );
    fs::rename(
        fixture.root.join("prefabs"),
        fixture.root.join("moved-prefabs"),
    )
    .unwrap();
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let result = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::OpenPrefabSource {
                    document: scene,
                    base_revision: editor.workspace.document_revision(scene).unwrap(),
                    entity: anchor_id,
                },
            )],
        ))
        .unwrap();
    close(&mut editor);
    assert!(
        matches!(&result.outcomes()[0], EditorActionOutcome::Workspace(report) if report.applied && report.opened_document.is_some()),
        "{:?}",
        result.outcomes()
    );
}
