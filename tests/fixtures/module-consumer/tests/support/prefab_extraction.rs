use std::{
    collections::BTreeMap,
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::Arc,
    time::{Duration, Instant},
};

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetPathScope, AssetRef,
    AssetSourceKind, AssetSourcePath, StableAssetId,
};
use nara_engine::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_host::{
        EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession, advanced::built_in_schema_providers,
    },
};
use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget, EntityReference};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentRegistryBuilder,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringFieldEdit, CompiledSceneDocument, CompiledSceneEntity,
    PrefabAddition, PrefabComponentIntent, PrefabDocument, PrefabDocumentCandidate,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop, PrefabOverlay,
    PrefabPropertyClaim, PrefabPropertyIntent, ResolvedPrefabSource, SceneComponentBindings,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
    SourceDocumentKey, SourceDocumentKind,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId, EditorDocumentMutation,
    EditorDocumentOpenResult, EditorDocumentTabModel, EditorNavigationAction,
    EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorProjectBrowserAction, EditorProjectBrowserActionOutcome, EditorPropertyRead,
    EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
};

pub const STARTUP: &str = "scenes/startup.scene.json";
pub const INACTIVE: &str = "scenes/inactive.scene.ron";
pub const P: &str = "prefabs/actor.prefab.json";
pub const Q: &str = "prefabs/extracted.prefab.ron";
pub const Q_META: &str = "prefabs/extracted.prefab.ron.meta";
pub const WRAPPER: &str = "prefabs/wrapper.prefab.json";
pub const LEAF: &str = "prefabs/leaf.prefab.json";
pub const GUARD: &str = "prefabs/guard.prefab.ron";
pub const CLOSED: &str = "prefabs/closed.prefab.ron";
pub const DORMANT: &str = "dormant/scenes/closed.scene.json";
pub const BUNDLE: &str = "schema/authoring.bundle.json";
pub const SOURCES: [&str; 8] = [STARTUP, INACTIVE, P, WRAPPER, LEAF, GUARD, CLOSED, DORMANT];
const UNIVERSE: &str = "f39f374d-682d-4c9a-a0b6-f396be37996e";
const OWNER: &str = "consumer.prefab-extraction";
const WAIT: Duration = Duration::from_secs(30);

pub fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

pub fn payload_type() -> ComponentTypeId {
    ComponentTypeId::new("consumer.PrefabExtractionPayload")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()
}

pub fn key(path: &str) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        SourceDocumentKind::Prefab,
    )
}

pub fn target_metadata() -> AssetMetaIdentity {
    metadata(
        "extracted.prefab.ron",
        "c7ef034d-1c5f-4e69-8369-349f3c780873",
    )
}

fn metadata(alias: &str, stable: &str) -> AssetMetaIdentity {
    AssetMetaIdentity::new(
        StableAssetId::parse_str(stable).unwrap(),
        AssetPath::new(alias).unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn local(entity: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(entity) }
}

fn reference(route: &[(&str, &str)], entity: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: route
                .iter()
                .map(|(anchor, path)| PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                    expected_base: key(path),
                })
                .collect(),
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(entity))),
        },
    }
}

fn record(entity: &str, amount: u64, references: Vec<EntityReference>) -> SceneEntityRecord {
    SceneEntityRecord::new(id(entity))
        .with_component(
            payload_type(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("label", ComponentValue::String(entity.into())),
                    ("amount", ComponentValue::U64(amount)),
                    (
                        "references",
                        ComponentValue::List(
                            references
                                .into_iter()
                                .map(ComponentValue::EntityReference)
                                .collect(),
                        ),
                    ),
                ]),
            )
            .with_owner(owner()),
        )
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "translation",
                        ComponentValue::map([
                            ("x", ComponentValue::f64(amount as f64).unwrap()),
                            ("y", ComponentValue::f64(-2.0).unwrap()),
                        ]),
                    ),
                    ("rotation", ComponentValue::f64(0.25).unwrap()),
                    (
                        "scale",
                        ComponentValue::map([
                            ("x", ComponentValue::f64(2.0).unwrap()),
                            ("y", ComponentValue::f64(3.0).unwrap()),
                        ]),
                    ),
                ]),
            )
            .with_owner(
                PersistentComponentSchemaOwnerId::try_new(
                    nara_transform::TRANSFORM_SCHEMA_OWNER_ID.as_str(),
                )
                .unwrap(),
            ),
        )
}

fn child(entity: &str, parent: &str) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        PrefabEntityTarget::source(id(entity)),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id(parent),
        ))),
    )
}

fn instance(entity: &str, alias: &str) -> SceneEntityRecord {
    let mut record = record(entity, 1, vec![]);
    record.prefab = Some(PrefabInstance::forest(AssetRef::path(alias).unwrap()));
    record
}

fn child_placement_overlay() -> PrefabOverlay {
    let mut overlay = PrefabOverlay::new(key(P));
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::source(id("child")),
        ScenePlacement::first(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id("branch"),
        ))),
    ));
    overlay
}

fn repeated_scene() -> SceneDocument {
    let mut left = instance("left", "actor.prefab.json");
    let mut overlay = PrefabOverlay::new(key(P));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("child")),
        component: payload_type(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
            intent: PrefabPropertyIntent::Set(ComponentValue::U64(77)),
        }],
    });
    overlay.additions.push(PrefabAddition {
        id: id("badge"),
        components: record("badge", 90, vec![]).components,
        prefab: None,
    });
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(id("badge")),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id("branch"),
        ))),
    ));
    left.prefab.as_mut().unwrap().overrides = Some(overlay);
    SceneDocument::from_ordered_roots([
        left,
        instance("right", "actor.prefab.json"),
        instance("wrapped", "wrapper.prefab.json"),
        record("child", 100, vec![]),
        record(
            "watcher",
            200,
            vec![
                reference(&[("left", P)], "child"),
                reference(&[("right", P)], "child"),
                reference(&[("wrapped", WRAPPER), ("inner", P)], "child"),
                local("child"),
                reference(&[("left", P)], "branch"),
            ],
        ),
    ])
}

pub struct Project {
    pub root: tempfile::TempDir,
    bundle: AuthoringSchemaBundle,
    bindings: SceneComponentBindings,
}

impl Project {
    pub fn new() -> Self {
        let schema = ComponentSchema::new(
            payload_type(),
            "Extraction payload",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("label"),
                "Label",
                ComponentFieldPath::from_fields(["label"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::required(
                ComponentFieldId::new("amount"),
                "Amount",
                ComponentFieldPath::from_fields(["amount"]),
                ComponentValueKind::U64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::required(
                ComponentFieldId::new("references"),
                "References",
                ComponentFieldPath::from_fields(["references"]),
                ComponentValueShape::list(ComponentValueShape::EntityRef),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef),
        ]);
        let builtins =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap();
        let bundle = AuthoringSchemaBundle::new(
            builtins
                .owners()
                .iter()
                .map(AuthoringSchemaBundleOwner::from)
                .chain([AuthoringSchemaBundleOwner::new(
                    owner(),
                    ComponentSchemaCatalog {
                        components: vec![schema],
                        ..Default::default()
                    },
                    None,
                )]),
        )
        .unwrap();
        let bindings = SceneComponentBindings::new(
            ComponentRegistryBuilder::new().freeze().unwrap(),
            bundle.owners().iter().cloned(),
        )
        .unwrap();
        let project = Self {
            root: tempfile::tempdir().unwrap(),
            bundle,
            bindings,
        };
        for directory in ["assets", "scenes", "prefabs", "schema", "dormant/scenes"] {
            fs::create_dir_all(project.root.path().join(directory)).unwrap();
        }
        fs::write(
            project.root.path().join("nara.toml"),
            format!(
                r#"schema_version = 1
[project]
name = "Independent public Prefab extraction"
source_universe = "{UNIVERSE}"
native_document_identity = "project_location"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
state = "ready"
schema_bundle = "{BUNDLE}"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
[profiles.dormant.paths]
scenes = "dormant/scenes"
[profiles.dormant.startup]
default_scene = "closed.scene.json"
"#
            ),
        )
        .unwrap();
        fs::write(
            project.root.path().join(BUNDLE),
            project.bundle.to_json_string().unwrap(),
        )
        .unwrap();
        let mut twig = instance("twig", "leaf.prefab.json");
        twig.components = record("twig", 40, vec![]).components;
        let prefab = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("trunk")),
                child("branch", "trunk"),
                child("child", "branch"),
                child("twig", "branch"),
                SceneEntityPlacement::root(id("outside")),
            ],
            [
                record("trunk", 10, vec![]),
                record("branch", 20, vec![local("child"), local("outside")]),
                record(
                    "child",
                    30,
                    vec![
                        local("branch"),
                        local("outside"),
                        reference(&[("twig", LEAF)], "leaf"),
                    ],
                ),
                twig,
                record("outside", 50, vec![local("branch"), local("child")]),
            ],
        );
        fs::write(
            project.root.path().join(P),
            prefab.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.path().join(format!("{P}.meta")),
            AssetMeta::new(
                metadata("actor.prefab.json", "76f2b747-f3e5-41c9-9ed9-c7802f7564d9"),
                0_u32,
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.path().join(LEAF),
            PrefabDocument::from_ordered_roots([record("leaf", 60, vec![local("leaf")])])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let mut inner = instance("inner", "actor.prefab.json");
        inner.prefab.as_mut().unwrap().overrides = Some(child_placement_overlay());
        fs::write(
            project.root.path().join(WRAPPER),
            PrefabDocument::from_ordered_roots([inner])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let mut closed = instance("closed", "actor.prefab.json");
        closed.prefab.as_mut().unwrap().overrides = Some(child_placement_overlay());
        fs::write(
            project.root.path().join(CLOSED),
            PrefabDocument::from_ordered_roots([closed])
                .to_ron_string()
                .unwrap(),
        )
        .unwrap();
        for path in [STARTUP, DORMANT] {
            fs::write(
                project.root.path().join(path),
                repeated_scene().to_json_string().unwrap(),
            )
            .unwrap();
        }
        fs::write(
            project.root.path().join(INACTIVE),
            SceneDocument::from_ordered_roots([record("target", 5, vec![])])
                .to_ron_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.path().join(GUARD),
            PrefabDocument::from_ordered_roots([record("target", 6, vec![])])
                .to_ron_string()
                .unwrap(),
        )
        .unwrap();
        project
    }

    pub fn bytes(&self, path: &str) -> Vec<u8> {
        fs::read(self.root.path().join(path)).unwrap()
    }

    pub fn prefab(&self, path: &str) -> PrefabDocument {
        let bytes = self.bytes(path);
        let candidate = if path.ends_with(".ron") {
            PrefabDocumentCandidate::decode_ron_bytes(&bytes)
        } else {
            PrefabDocumentCandidate::decode_json_bytes(&bytes)
        }
        .unwrap();
        candidate
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path(path).unwrap()),
                &self.bundle,
                &[],
            )
            .unwrap()
            .document()
            .clone()
    }

    pub fn snapshot(&self, path: &str) -> Vec<EntityObservation> {
        let bytes = self.bytes(path);
        let candidate = if path.ends_with(".ron") {
            SceneDocumentCandidate::decode_ron_bytes(&bytes)
        } else {
            SceneDocumentCandidate::decode_json_bytes(&bytes)
        }
        .unwrap();
        let document = candidate
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path(path).unwrap()),
                &self.bundle,
                &[],
            )
            .unwrap();
        let mut sources = Vec::new();
        for path in [P, WRAPPER, LEAF, Q] {
            if !self.root.path().join(path).exists() {
                assert_eq!(path, Q);
                continue;
            }
            let mut source = ResolvedPrefabSource::bound(Arc::new(self.prefab(path)), key(path));
            let metadata = self.root.path().join(format!("{path}.meta"));
            if metadata.exists() {
                let identity =
                    AssetMetaIdentityCandidate::decode_json_bytes(&fs::read(metadata).unwrap())
                        .unwrap()
                        .into_identity();
                assert_eq!(format!("prefabs/{}", identity.path()), path);
                let record = identity.into_record(&AssetPathScope::under(
                    AssetSourcePath::new("prefabs").unwrap(),
                ));
                source = source.with_source_asset(&record).unwrap();
                sources.push((AssetRef::StableId(record.stable_id()), source.clone()));
            }
            sources.push((
                AssetRef::path(path.strip_prefix("prefabs/").unwrap()).unwrap(),
                source,
            ));
        }
        let report = self
            .bindings
            .try_compile_scene_with_loader(
                document.document(),
                Default::default(),
                None,
                |request, _| {
                    Ok::<_, std::convert::Infallible>(
                        sources
                            .iter()
                            .find(|(alias, _)| alias == request)
                            .map(|(_, source)| source.clone()),
                    )
                },
            )
            .unwrap();
        assert!(!report.diagnostics.has_errors(), "{report:?}");
        assert!(report.failure.is_none(), "{report:?}");
        observe(&report.candidate.unwrap())
    }

    pub fn run(&self, test: impl FnOnce(&mut EditorProjectSession)) {
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(self.root.path())
                .unwrap()
        };
        #[cfg(not(windows))]
        let handle = fs::File::open(self.root.path()).unwrap();
        let capability = DirectoryCapability::from_host_handle(
            handle,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        let mut host = EditorProjectOpenHost::new();
        let mut attempt = host.begin(capability, None);
        let deadline = Instant::now() + WAIT;
        let mut editor = loop {
            match host.advance(attempt) {
                EditorProjectOpenProgress::Opened(editor) => break editor,
                EditorProjectOpenProgress::Pending(next) => attempt = next,
                EditorProjectOpenProgress::Rejected(error) => {
                    panic!("public Open failed: {:?}", error.diagnostics())
                }
            }
            assert!(Instant::now() < deadline, "public Open timed out");
            std::thread::sleep(Duration::from_millis(1));
        };
        loop {
            let report = host.poll_shutdown();
            if report.is_complete() {
                assert!(!report.has_cleanup_failure(), "{report:?}");
                break;
            }
            assert!(Instant::now() < deadline, "Open Host did not retire");
            std::thread::sleep(Duration::from_millis(1));
        }
        editor.drive_editor_frame(Duration::ZERO);
        let result = catch_unwind(AssertUnwindSafe(|| test(&mut editor)));
        let deadline = Instant::now() + WAIT;
        loop {
            if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
                assert!(!receipt.has_cleanup_failure(), "{receipt:?}");
                break;
            }
            assert!(Instant::now() < deadline, "public Close did not retire");
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Err(panic) = result {
            resume_unwind(panic);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityObservation {
    pub path: String,
    pub amount: u64,
    pub children: Vec<String>,
    pub references: Vec<String>,
    pub transform: ComponentValue,
}

fn payload(entity: &CompiledSceneEntity) -> &ComponentValue {
    entity.components()[&payload_type()].value()
}

fn observe(document: &CompiledSceneDocument) -> Vec<EntityObservation> {
    fn visit(
        document: &CompiledSceneDocument,
        entity: &SceneEntityId,
        parent: &str,
        paths: &mut BTreeMap<SceneEntityId, String>,
        order: &mut Vec<SceneEntityId>,
    ) {
        let record = document
            .entities()
            .iter()
            .find(|record| record.id() == entity)
            .unwrap();
        let ComponentValue::String(label) = payload(record).get("label").unwrap() else {
            panic!("expected an explicit label")
        };
        let path = if parent.is_empty() {
            label.clone()
        } else {
            format!("{parent}/{label}")
        };
        assert!(paths.insert(entity.clone(), path.clone()).is_none());
        order.push(entity.clone());
        for child in record.children() {
            visit(document, child, &path, paths, order);
        }
    }
    let mut paths = BTreeMap::new();
    let mut order = Vec::new();
    for root in document.roots() {
        visit(document, root, "", &mut paths, &mut order);
    }
    assert_eq!(paths.len(), document.entities().len());
    order.iter().map(|entity| {
        let record = document.entities().iter().find(|record| record.id() == entity).unwrap();
        let ComponentValue::U64(amount) = payload(record).get("amount").unwrap() else { panic!("expected an explicit amount") };
        let ComponentValue::List(references) = payload(record).get("references").unwrap() else { panic!("expected explicit references") };
        EntityObservation {
            path: paths[entity].clone(),
            amount: *amount,
            children: record.children().iter().map(|child| paths[child].clone()).collect(),
            references: references.iter().map(|reference| {
                let ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) = reference else { panic!("the real compiler must resolve authored references: {reference:?}") };
                paths[entity].clone()
            }).collect(),
            transform: record.components()[&ComponentTypeId::new("nara.transform.Transform2d")].value().clone(),
        }
    }).collect()
}

pub fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let frame = editor.current_frame().revision();
    let mut outcomes = editor
        .dispatch_editor_actions(EditorActionBatch::new(frame, [action]))
        .unwrap()
        .into_outcomes()
        .into_iter();
    let outcome = outcomes.next().unwrap();
    assert!(outcomes.next().is_none());
    editor.drive_editor_frame(Duration::ZERO);
    outcome
}

pub fn applied(outcome: EditorActionOutcome) {
    assert!(
        matches!(&outcome, EditorActionOutcome::Workspace(report) if report.applied),
        "{outcome:?}"
    );
}

pub fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "public workflow timed out: {:?}; {:?}",
            editor.prefab_extraction_view(),
            editor.prefab_extraction_diagnostics()
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn open_source(editor: &mut EditorProjectSession, path: &str) -> EditorDocumentId {
    wait(editor, |editor| {
        !editor.current_frame().project_browser().has_pending_work()
    });
    let outcome = dispatch(
        editor,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: path.into(),
        }),
    );
    let EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
        request,
    }) = outcome
    else {
        panic!("public Browser Open refused {path}: {outcome:?}")
    };
    wait(editor, |editor| {
        editor.current_frame().document_open().result().is_some()
    });
    let result = editor.current_frame().document_open().result().unwrap();
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = result
    else {
        panic!("public Browser Open failed: {result:?}")
    };
    assert_eq!(completed, request);
    assert!(
        matches!(dispatch(editor, EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request })), EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::OpenAcknowledged { request: acknowledged }) if acknowledged == request)
    );
    document
}

pub fn activate(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    applied(dispatch(
        editor,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document }),
    ));
}

pub fn tab(editor: &EditorProjectSession, document: EditorDocumentId) -> EditorDocumentTabModel {
    editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone()
}

pub fn edit_amount(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    entity: &str,
    amount: u64,
) {
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    let edit = AuthoringFieldEdit::single(
        id(entity),
        payload_type(),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("amount"),
        ComponentValue::U64(amount),
    )
    .unwrap();
    applied(dispatch(
        editor,
        EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
            document,
            base_revision,
            edit,
        }),
    ));
}

pub fn amount(editor: &EditorProjectSession, document: EditorDocumentId, entity: &str) -> u64 {
    let observation = editor
        .read_document_property(EditorPropertyRead {
            target: EditorPropertyTarget {
                basis: editor.document_query_basis(document).unwrap(),
                entity: id(entity),
                component: payload_type(),
                property: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
            },
            limits: EditorPropertyReadLimits::new(4096, 16, 4096).unwrap(),
        })
        .unwrap();
    let EditorPropertyValue::Present(ComponentValue::U64(amount)) = observation.value else {
        panic!("expected a readable source amount: {observation:?}")
    };
    amount
}

pub fn replay(editor: &mut EditorProjectSession, document: EditorDocumentId, undo: bool) {
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    let mutation = if undo {
        EditorDocumentMutation::Undo {
            document,
            base_revision,
        }
    } else {
        EditorDocumentMutation::Redo {
            document,
            base_revision,
        }
    };
    applied(dispatch(editor, EditorAction::Mutate(mutation)));
}

pub fn save(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    let outcome = dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision,
        }),
    );
    let EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt }) =
        outcome
    else {
        panic!("public Save refused: {outcome:?}")
    };
    wait(editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    let result = editor.current_frame().persistence().result().unwrap();
    assert!(
        matches!(result, EditorPersistenceResult::Saved { document: saved, revision, .. } if saved == document && revision == base_revision),
        "{result:?}"
    );
    assert!(
        matches!(dispatch(editor, EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt })), EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { attempt: acknowledged }) if acknowledged == attempt)
    );
}
