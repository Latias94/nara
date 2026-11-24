use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{PrefabInspectionInput, try_inspect_authoring_prefab_instance_with_loader},
    PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityOrigin, PrefabEntityTarget,
    PrefabInstanceHop, PrefabOverlay, SceneComponentRecord, SceneDocumentCandidate, SceneEntityId,
};
use nara_tooling::{EditorDocumentSourceBinding, EditorPrefabComponentAddress, EditorWorkspace};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, AuthoringSourceOperationLimits, document,
    project_authoring_adapters,
};

const UNIVERSE: &str = "8e2540a5-77da-4761-851c-2f51ce30a3d8";
const SCENE: &str = "scenes/main.scene.json";
const BASE: &str = "prefabs/base.prefab.ron";
const CLOSED: &str = "prefabs/dependent.prefab.ron";
const INTERMEDIATE: &str = "prefabs/intermediate.prefab.ron";
const DETACHED: &str = "detached/scenes/unrelated.scene.json";

struct ScratchProject(PathBuf);

impl ScratchProject {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nara_component_apply_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        for directory in ["scenes", "prefabs", "assets", "detached/scenes"] {
            fs::create_dir_all(path.join(directory)).unwrap();
        }
        Self(path)
    }
}

impl Drop for ScratchProject {
    fn drop(&mut self) {
        let path = self.0.canonicalize().unwrap();
        let parent = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(path.parent(), Some(parent.as_path()));
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_component_apply_")
        );
        fs::remove_dir_all(path).unwrap();
    }
}

fn admit_project(path: &Path) -> AuthoringProjectSessionSnapshot {
    let mut open = fs::OpenOptions::new();
    open.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        open.share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0200_0000 | 0x0020_0000);
    }
    let root = DirectoryCapability::from_host_handle(
        open.open(path).unwrap(),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap();
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(root, None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let adapters = project_authoring_adapters(schema.bundle()).unwrap();
    PreparedAuthoringProject::with_scene_limits(schema, adapters, SceneFileLimits::default())
        .admit_startup_closure()
        .unwrap()
        .publish()
        .into_session_snapshot()
        .unwrap()
}

fn key(path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn observe(
    project: &AuthoringProjectSessionSnapshot,
    path: &str,
    kind: SourceDocumentKind,
    bytes: &[u8],
) -> (document::AuthoringDocumentSourceFact, FileSourceObservation) {
    let root = project.project_root();
    let relative = RelativePath::new(path).unwrap();
    let (parent, leaf) = root.resolve_file_parent(&relative).unwrap();
    let file = parent.open_child_file(&leaf).unwrap();
    let native = NativeDocumentSourcePolicy::new(project.settings())
        .admit(&root, path, kind, file.identity())
        .unwrap();
    let digest = ContentDigest::of_bytes(bytes);
    let source = document::AuthoringDocumentSourceFact::from_native_observation(
        path,
        file.identity(),
        file.parent_identity(),
        digest,
        &native,
    )
    .unwrap();
    let observed = FileSourceObservation::from_read(file, parent, digest).unwrap();
    (source, observed)
}

#[test]
fn project_component_apply_review_checks_the_closed_prefab_and_encodes_both_existing_files() {
    let component = ComponentTypeId::new("test.ComponentPromotion");
    let owner = PersistentComponentSchemaOwnerId::try_new("test.component-promotion").unwrap();
    let schema = ComponentSchema::new(component.clone(), "Promotion", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("score"),
            "Score",
            ComponentFieldPath::from_fields(["score"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let base_key = key(BASE, SourceDocumentKind::Prefab);
    let scene_key = key(SCENE, SourceDocumentKind::Scene);
    let anchor = SceneEntityId::new("mount").unwrap();
    let hero = SceneEntityId::new("hero").unwrap();
    let reference = AssetRef::path("base.prefab.ron").unwrap();
    let record = || {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("score", ComponentValue::U64(77))]),
        )
        .with_owner(owner.clone())
    };
    let instance = |claimed| {
        let mut entity = SceneEntityRecord::new(anchor.clone());
        let mut prefab = PrefabInstance::forest(reference.clone());
        if claimed {
            let mut overlay = PrefabOverlay::new(base_key.clone());
            overlay.components.push(PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Add,
                target: PrefabEntityTarget::source(hero.clone()),
                component: component.clone(),
                record: record(),
            });
            prefab.overrides = Some(overlay);
        }
        entity.prefab = Some(prefab);
        entity
    };
    let selected = PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(anchor.clone()),
            expected_base: base_key.clone(),
        }],
        entity: PrefabEntityOrigin::SourceEntity(hero.clone()),
    };
    let original_scene = SceneDocument::from_ordered_roots([instance(true)])
        .to_json_string()
        .unwrap();
    let original_base = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(hero.clone())])
        .to_ron_string()
        .unwrap();
    let address = EditorPrefabComponentAddress {
        entity: selected.clone(),
        component: component.clone(),
    };
    let intermediate_request = AssetRef::path("intermediate.prefab.ron").unwrap();
    let mut guard_root = SceneEntityRecord::new(SceneEntityId::new("guard").unwrap());
    guard_root.prefab = Some(PrefabInstance::forest(intermediate_request.clone()));
    let guard = PrefabDocument::from_ordered_roots([guard_root]);
    let closed = guard.to_ron_string().unwrap();
    let mut unrelated_root = SceneEntityRecord::new(SceneEntityId::new("unrelated").unwrap());
    unrelated_root.prefab = Some(PrefabInstance::forest(
        AssetRef::path("missing.prefab.ron").unwrap(),
    ));
    let unrelated = SceneDocument::from_ordered_roots([unrelated_root]);
    let unrelated_bytes = unrelated.to_json_string().unwrap();
    for conflicts in [false, true] {
        let scratch = ScratchProject::new();
        let root = &scratch.0;
        fs::write(
            root.join("nara.toml"),
            format!(
                r#"schema_version = 1
[project]
name = "Paired component Apply"
source_universe = "{UNIVERSE}"
native_document_identity = "project_location"
[authoring]
state = "ready"
schema_bundle = "schema.json"
[startup]
default_scene = "main.scene.json"
[profiles.detached.paths]
scenes = "detached/scenes"
[profiles.detached.startup]
clear_default_scene = true
"#
            ),
        )
        .unwrap();
        fs::write(root.join("schema.json"), bundle.to_json_string().unwrap()).unwrap();
        fs::write(root.join(SCENE), &original_scene).unwrap();
        fs::write(root.join(BASE), &original_base).unwrap();
        let intermediate = Arc::new(PrefabDocument::from_ordered_roots([instance(conflicts)]));
        fs::write(
            root.join(INTERMEDIATE),
            intermediate.to_ron_string().unwrap(),
        )
        .unwrap();
        fs::write(root.join(CLOSED), &closed).unwrap();
        fs::write(root.join(DETACHED), &unrelated_bytes).unwrap();

        let project = admit_project(root);
        let basis = project.startup_scene.schema_basis();
        let (mut workspace, authority) = EditorWorkspace::new_hosted();
        let containing = workspace
            .open_authoring_scene_document(
                "Containing",
                EditorDocumentSourceBinding::new(SCENE),
                SceneDocumentCandidate::decode_json_str(&original_scene)
                    .unwrap()
                    .admit_authoring_with_basis(
                        document::document_source(&ProjectPath::new(SCENE).unwrap()).unwrap(),
                        &basis,
                    )
                    .unwrap()
                    .publish(),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let destination = workspace
            .open_authoring_prefab_document(
                "Destination",
                EditorDocumentSourceBinding::new(BASE),
                PrefabDocumentCandidate::decode_ron_str(&original_base)
                    .unwrap()
                    .admit_authoring_with_basis(
                        document::document_source(&ProjectPath::new(BASE).unwrap()).unwrap(),
                        &basis,
                    )
                    .unwrap()
                    .publish(),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let saved = authority
            .capture_prefab_component_apply(
                &workspace,
                &workspace.document_query_basis(containing).unwrap(),
                &scene_key,
                &workspace.document_query_basis(destination).unwrap(),
                &base_key,
                |_| Ok(()),
            )
            .unwrap();
        let source = saved.source_pair();
        let shared = source.shared_containing_scene().unwrap();
        let original_destination = ResolvedPrefabSource::bound_authoring_source(
            source.shared_destination(),
            basis.clone(),
            base_key.clone(),
        );
        let load = |requested: &AssetRef, _: &mut SceneCompilationWorkingSet| {
            assert_eq!(requested, &reference);
            Ok::<_, std::convert::Infallible>(Some(original_destination.clone()))
        };
        let inspection = try_inspect_authoring_prefab_instance_with_loader(
            PrefabInspectionInput::WorkingScene(&shared, source.containing_revision()),
            &anchor,
            &basis,
            Default::default(),
            load,
        )
        .unwrap();
        if conflicts {
            let bridge = ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(&intermediate),
                basis.clone(),
                key(INTERMEDIATE, SourceDocumentKind::Prefab),
            );
            let original_guard = try_validate_authoring_saved_prefab_with_loader(
                &guard,
                &basis,
                Default::default(),
                |requested, working| {
                    if requested == &intermediate_request {
                        Ok(Some(bridge.clone()))
                    } else {
                        load(requested, working)
                    }
                },
            )
            .unwrap();
            assert!(
                original_guard.failure.is_none() && !original_guard.diagnostics.has_errors(),
                "{original_guard:?}"
            );
        } else {
            let original_unrelated = try_compile_authoring_scene_with_loader(
                &unrelated,
                &basis,
                Default::default(),
                None,
                |request, _| {
                    assert_eq!(request, &AssetRef::path("missing.prefab.ron").unwrap());
                    Ok::<_, std::convert::Infallible>(None)
                },
            )
            .unwrap();
            assert!(original_unrelated.candidate().is_none());
            assert!(original_unrelated.diagnostics().has_errors());
        }
        let first = observe(
            &project,
            SCENE,
            SourceDocumentKind::Scene,
            original_scene.as_bytes(),
        );
        let second = observe(
            &project,
            BASE,
            SourceDocumentKind::Prefab,
            original_base.as_bytes(),
        );
        let open = vec![Some(first.0.clone()), Some(second.0.clone())];
        let limits = AuthoringSourceOperationLimits::default();
        let inputs = project
            .admit_source_operation_inputs(0, 32 * 1024, limits.host_work(), limits.open_handles())
            .unwrap();
        let input =
            project.capture_component_apply_project_review(inputs, open.clone(), [first, second]);
        let review =
            input.prepare_selected(saved, &inspection, &address, &selected, Default::default());
        if conflicts {
            let Err(ComponentApplyProjectReviewError::SavedGeneration(
                ComponentApplySavedGenerationError::Composition {
                    path,
                    profile: None,
                    failure: Some(failure),
                    ..
                },
            )) = review
            else {
                panic!("the transitive dependent should refuse Review before the intermediate");
            };
            assert_eq!(path, CLOSED);
            assert_eq!(
                failure.reason(),
                &nara_scene::SceneCompilationFailureKind::Component
            );
        } else {
            let mut review = review.unwrap();
            let configurations = review.inventory.configurations();
            let default = configurations
                .iter()
                .find(|configuration| configuration.profile_name().is_none())
                .unwrap();
            assert!(!default.participant_paths().any(|path| path == DETACHED));
            let detached = configurations
                .iter()
                .find(|configuration| configuration.profile_name() == Some("detached"))
                .unwrap();
            assert!(detached.participant_paths().any(|path| path == DETACHED));
            assert!(detached.participant_paths().any(|path| path == BASE));
            review.revalidate_project(&project, &open).unwrap();
            review.workspace().validate(&workspace).unwrap();
            let [containing_output, destination_output] = review.outputs();
            let containing = SceneDocumentCandidate::decode_json_bytes(containing_output.bytes())
                .unwrap()
                .into_composition_source()
                .unwrap();
            let destination = PrefabDocumentCandidate::decode_ron_bytes(destination_output.bytes())
                .unwrap()
                .into_composition_source()
                .unwrap();
            assert!(
                containing.entities[0]
                    .prefab
                    .as_ref()
                    .unwrap()
                    .overrides
                    .as_ref()
                    .is_none_or(|overlay| overlay.components.is_empty())
            );
            assert_eq!(destination.entities[0].components[&component], record());
        }
        assert_eq!(
            fs::read(root.join(SCENE)).unwrap(),
            original_scene.as_bytes()
        );
        assert_eq!(fs::read(root.join(BASE)).unwrap(), original_base.as_bytes());
        assert_eq!(fs::read(root.join(CLOSED)).unwrap(), closed.as_bytes());
        assert_eq!(
            fs::read(root.join(DETACHED)).unwrap(),
            unrelated_bytes.as_bytes()
        );
    }
}
