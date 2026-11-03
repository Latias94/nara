//! Public live publication followed by ordinary editing, strict Check and actual Runtime ticks.

#[path = "../authored_upgrade_product.rs"]
pub(super) mod runtime_product;

use std::{path::Path, sync::mpsc};

use nara_engine::{
    core::ItemLimit,
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project::{ProjectAuthoringState, ProjectManifest},
    project_host::{AuthoringSourceOperationLimits, TrustedCompiledEditorProjectIntent},
};
use nara_tooling::{
    EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorPlayAction, EditorPlayRequestResult, EditorPlayState,
};

use runtime_product::{EnemyStatsSample, assert_v1_refuses_saved_v2, assert_v2_saved_product};

use super::*;

const PROFILE_INACTIVE: &str = "dormant/scenes/inactive.scene.json";
const PRODUCT_TRACKED: [&str; 9] = [
    "nara.toml",
    "schema/authoring.bundle.json",
    STARTUP,
    INACTIVE,
    PREFAB,
    LEAF,
    PROFILE_SCENE,
    PROFILE_PREFAB,
    PROFILE_INACTIVE,
];

/// Construct both controls before any Session capture. No published output is repaired for Check.
pub(super) fn product_project(surviving_unavailable: bool) -> project::Project {
    let mut startup = scene_document(false, false);
    let mut prefab = enemy_document(false, false);
    let mut dormant = profile_prefab(false);
    if !surviving_unavailable {
        for entity in startup
            .entities
            .iter_mut()
            .chain(prefab.entities.iter_mut())
            .chain(dormant.entities.iter_mut())
        {
            entity.components.remove(&ComponentTypeId::new(UNKNOWN));
        }
    }
    let project = project::Project::new(&runtime_product::v1_bundle(), &startup, "json");
    for directory in ["dormant/scenes", "dormant/prefabs"] {
        fs::create_dir_all(project.root().join(directory)).unwrap();
    }
    let manifest_path = project.root().join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        manifest_path,
        format!(
            "{manifest}{}",
            concat!(
                "\n[content]\nscenes = ['inactive.scene.json']\n",
                "[tasks.io]\nworkers = 1\npending_capacity = 16\n",
                "[tasks.compute]\nworkers = 1\npending_capacity = 16\n",
                "[tasks.async_compute]\nworkers = 1\npending_capacity = 16\n",
                "[profiles.dormant.paths]\nscenes = 'dormant/scenes'\nprefabs = 'dormant/prefabs'\n",
                "[profiles.dormant.startup]\ndefault_scene = 'closed.scene.json'\n",
            ),
        ),
    )
    .unwrap();
    for (path, encoded) in [
        (INACTIVE, startup.to_json_string().unwrap()),
        (PREFAB, prefab.to_json_string().unwrap()),
        (LEAF, leaf_document(false).to_json_string().unwrap()),
        (
            PROFILE_SCENE,
            profile_scene(false).to_json_string().unwrap(),
        ),
        // content.scenes is global, so this declared root must exist under both Scene roots.
        (
            PROFILE_INACTIVE,
            profile_scene(false).to_json_string().unwrap(),
        ),
        (PROFILE_PREFAB, dormant.to_json_string().unwrap()),
    ] {
        fs::write(project.root().join(path), encoded).unwrap();
    }
    project
}

fn sample(entity: &str, projected: bool, health: i64, armor: i64) -> EnemyStatsSample {
    EnemyStatsSample {
        source_entity: entity.into(),
        projected,
        health,
        armor,
    }
}

/// Literal Runtime oracle: two repeated instances, nested replacement and an added instance.
pub(super) fn expected_base_samples() -> Vec<EnemyStatsSample> {
    let mut samples = vec![sample("enemy", false, 109000, 7)];
    for _ in 0..2 {
        samples.extend([
            sample("enemy", true, 109000, 7),
            sample("leaf", true, 14000, 7),
            sample("armor-only", true, 9000, 7),
            sample("empty", true, 10000, 7),
            sample("replaced", true, 11000, 7),
            sample("added", true, -3000, 4),
            sample("leaf", true, 8000, 7),
            sample("armor-only", true, 9000, 7),
            sample("empty", true, 10000, 7),
            sample("replaced", true, 11000, 7),
        ]);
    }
    samples
}

/// Reusable hook for an actual product-ready recovered root, without another recovery driver.
pub(super) fn assert_saved_products(root: &Path) {
    assert_v2_saved_product(
        root,
        None,
        &["inactive.scene.json", "startup.scene.json"],
        &expected_base_samples(),
    );
    assert_v1_refuses_saved_v2(root, None);
    assert_v2_saved_product(
        root,
        Some("dormant"),
        &["closed.scene.json", "inactive.scene.json"],
        &[
            sample("profile-enemy", false, 19000, 7),
            sample("profile-enemy", true, 17000, 7),
        ],
    );
    assert_v1_refuses_saved_v2(root, Some("dormant"));
}

/// Opens a fresh compiled Editor owner over the actual published source and runs its fixed tick.
pub(super) fn assert_saved_editor_play(root: &Path) {
    let (observations, receiver) = mpsc::sync_channel(1);
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000);
    }
    let root = DirectoryCapability::from_host_handle(
        options.open(root).unwrap(),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap();
    let mut editor = EditorProjectSession::open_trusted_compiled(
        root,
        TrustedCompiledEditorProjectIntent::new()
            .with_recipe(runtime_product::v2_play_recipe(observations)),
    )
    .unwrap();
    let document = editor.current_frame().workspace().active_document.unwrap();
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    assert!(matches!(
        dispatch(
            &mut editor,
            EditorAction::Play(EditorPlayAction::Play {
                document,
                base_revision
            })
        ),
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted),
    ));
    let deadline = Instant::now() + WAIT;
    let outcome = loop {
        editor.drive_editor_frame(Duration::from_millis(20));
        if let Ok(outcome) = receiver.try_recv() {
            break outcome;
        }
        assert!(
            Instant::now() < deadline,
            "v2 Editor Play must execute the actual fixed tick: {:?}",
            editor.current_frame().play()
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    runtime_product::assert_outcome(&outcome, &expected_base_samples());
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Running
    );
    assert!(matches!(
        dispatch(&mut editor, EditorAction::Play(EditorPlayAction::Stop)),
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted),
    ));
    wait(&mut editor, |editor| {
        editor.current_frame().play().state() == EditorPlayState::Empty
    });
    project::close(&mut editor);
}

fn publish(project: &project::Project) -> (EditorProjectSession, [EditorDocumentId; 3]) {
    let (mut editor, documents) = open_dirty(project);
    let operation = editor
        .begin_authored_schema_upgrade_with_limits(
            AuthoringSourceOperationLimits::default()
                .with_host_work(ItemLimit::new(200_000_000).unwrap())
                .with_open_handles(ItemLimit::new(128).unwrap()),
        )
        .unwrap();
    wait(&mut editor, |editor| {
        !matches!(
            editor.authored_schema_upgrade_view().phase,
            EditorAuthoredSchemaUpgradePhase::Draining
                | EditorAuthoredSchemaUpgradePhase::Capturing,
        )
    });
    assert_eq!(
        editor.authored_schema_upgrade_view().phase,
        EditorAuthoredSchemaUpgradePhase::AwaitingProposal,
    );
    let proposal = proposed(&editor, operation);
    editor
        .prepare_authored_schema_upgrade(operation, proposal)
        .unwrap();
    settle_preparation(&mut editor);
    let review = editor.authored_schema_upgrade_review(operation).unwrap();
    assert_eq!(review.source_count(), 7);
    assert_eq!(review.document_count(), 3);
    let reviewed: Vec<_> = review
        .sources()
        .map(|source| (source.project_path.to_owned(), source.successor.to_vec()))
        .collect();
    editor.confirm_authored_schema_upgrade(operation).unwrap();
    wait(&mut editor, |editor| {
        !editor.authored_schema_upgrade_view().work_pending
    });
    let view = editor.authored_schema_upgrade_view();
    assert_eq!(
        view.phase,
        EditorAuthoredSchemaUpgradePhase::Terminal,
        "{view:?}",
    );
    assert_eq!(
        view.outcome,
        Some(EditorAuthoredSchemaUpgradeOutcome::Adopted),
        "{view:?}; {:?}",
        editor.authored_schema_upgrade_diagnostics(),
    );
    assert!(!view.writes_fenced);
    let manifest =
        ProjectManifest::parse_toml_bytes(&fs::read(project.root().join("nara.toml")).unwrap());
    assert_eq!(
        manifest.manifest.unwrap().authoring.state,
        ProjectAuthoringState::Ready,
    );
    for (path, bytes) in reviewed {
        assert_eq!(
            fs::read(project.root().join(&path)).unwrap(),
            bytes,
            "{path}"
        );
    }
    for document in documents {
        project::assert_history(&editor, document, 0, 0);
        assert!(
            !editor
                .current_frame()
                .workspace()
                .document(document)
                .unwrap()
                .dirty
        );
        project::assert_value(
            &editor,
            document,
            "enemy",
            COMPONENT,
            "health",
            ComponentValue::map([("milli", ComponentValue::I64(109000))]),
        );
    }
    (editor, documents)
}

fn edit_milli(editor: &mut EditorProjectSession, document: EditorDocumentId, value: i64) {
    let basis = editor.document_query_basis(document).unwrap();
    let prepared = editor
        .prepare_authoring_property_edit(
            &basis,
            &[AuthoringPropertyEdit {
                entity: id("enemy"),
                component: component(),
                component_version: V2,
                property: health_property_v2(),
                value: ComponentValue::I64(value),
            }],
        )
        .unwrap();
    project::applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let revision = editor.current_frame().revision();
    let outcome = editor
        .dispatch_editor_actions(EditorActionBatch::new(revision, [action]))
        .unwrap()
        .into_outcomes()
        .into_iter()
        .next()
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    outcome
}

pub(super) fn ordinary_followup(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    edit_milli(editor, document, 113000);
    project::replay(editor, document, false);
    project::assert_value(
        editor,
        document,
        "enemy",
        COMPONENT,
        "health",
        ComponentValue::map([("milli", ComponentValue::I64(109000))]),
    );
    project::replay(editor, document, true);
    project::save(editor, document);
    edit_milli(editor, document, 117000);
    let target = editor.current_frame().reopen_target();
    assert!(matches!(
        dispatch(
            editor,
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target })
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. }),
    ));
    wait(editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Opened { .. }),
    ));
    project::assert_value(
        editor,
        document,
        "enemy",
        COMPONENT,
        "health",
        ComponentValue::map([("milli", ComponentValue::I64(113000))]),
    );
    let attempt = editor.current_frame().persistence().attempt().unwrap();
    assert!(matches!(
        dispatch(
            editor,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { .. }),
    ));
    // Restore the reviewed value through ordinary editing so subsequent literal Runtime
    // oracles still consume the same authored meaning, without rewriting fixture files.
    edit_milli(editor, document, 109000);
    project::save(editor, document);
}

fn assert_literal_saved_sources(root: &Path, surviving_unavailable: bool) {
    let mut startup = scene_document(true, true);
    let mut prefab = enemy_document(true, true);
    let mut dormant = profile_prefab(true);
    if !surviving_unavailable {
        for entity in startup
            .entities
            .iter_mut()
            .chain(prefab.entities.iter_mut())
            .chain(dormant.entities.iter_mut())
        {
            entity.components.remove(&ComponentTypeId::new(UNKNOWN));
        }
    }
    // These builders use literal milli oracles. They are comparisons, never replacement files.
    for (path, expected) in [
        (STARTUP, startup.clone()),
        (INACTIVE, startup),
        (PROFILE_SCENE, profile_scene(true)),
        (PROFILE_INACTIVE, profile_scene(true)),
    ] {
        let actual = SceneDocumentCandidate::decode_json_bytes(&fs::read(root.join(path)).unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        assert_eq!(actual, expected, "{path}");
    }
    for (path, expected) in [
        (PREFAB, prefab),
        (LEAF, leaf_document(true)),
        (PROFILE_PREFAB, dormant),
    ] {
        let actual =
            PrefabDocumentCandidate::decode_json_bytes(&fs::read(root.join(path)).unwrap())
                .unwrap()
                .into_composition_source()
                .unwrap();
        assert_eq!(actual, expected, "{path}");
    }
}

#[test]
fn live_published_v2_sources_check_run_and_refuse_the_fixed_v1_product() {
    let project = product_project(false);
    runtime_product::assert_v1_saved_product(
        project.root(),
        Some("dormant"),
        &["closed.scene.json", "inactive.scene.json"],
        &[
            sample("profile-enemy", false, 19, 7),
            sample("profile-enemy", true, 17, 7),
        ],
    );
    let (mut editor, documents) = publish(&project);
    assert_literal_saved_sources(project.root(), false);
    let published = PRODUCT_TRACKED.map(|path| fs::read(project.root().join(path)).unwrap());
    assert_saved_products(project.root());
    assert_saved_editor_play(project.root());
    assert_eq!(
        PRODUCT_TRACKED.map(|path| fs::read(project.root().join(path)).unwrap()),
        published,
    );
    ordinary_followup(&mut editor, documents[0]);
    project::close(&mut editor);
    drop(editor);
    let mut reopened = project.open();
    let document = reopened
        .current_frame()
        .workspace()
        .active_document
        .unwrap();
    project::assert_history(&reopened, document, 0, 0);
    project::assert_value(
        &reopened,
        document,
        "enemy",
        COMPONENT,
        "health",
        ComponentValue::map([("milli", ComponentValue::I64(109000))]),
    );
    project::close(&mut reopened);
    drop(reopened);
    assert_saved_products(project.root());
}

#[test]
fn live_published_unavailable_survivors_remain_preserved_and_refuse_v2_execution() {
    let project = product_project(true);
    let (mut editor, _) = publish(&project);
    assert_literal_saved_sources(project.root(), true);
    project::close(&mut editor);
    drop(editor);
    let published = PRODUCT_TRACKED.map(|path| fs::read(project.root().join(path)).unwrap());
    let saved = project.read();
    let enemy = saved
        .entities
        .iter()
        .find(|entity| entity.id == id("enemy"))
        .unwrap();
    assert_eq!(
        enemy.components[&ComponentTypeId::new(UNKNOWN)],
        unknown_record(),
    );
    runtime_product::assert_v2_unavailable_refusal(project.root(), None);
    runtime_product::assert_v2_unavailable_refusal(project.root(), Some("dormant"));
    assert_v1_refuses_saved_v2(project.root(), None);
    assert_eq!(
        PRODUCT_TRACKED.map(|path| fs::read(project.root().join(path)).unwrap()),
        published,
    );
}
