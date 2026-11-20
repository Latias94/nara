use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    ProductRecipe,
    fs::{ContentDigest, DirectoryCapability, FsError, RelativeComponent, RelativePath},
    project_host::{
        ProjectContentDeliveryReceipt, StartupContentClosureReceipt, ingest_project_manifest,
        resolve_product_recipe,
    },
    project_tooling::host_fs,
    project_workflow::{ProjectCheckRequest, check_project},
    scene::SceneDocument,
};

use super::{
    ProjectExportBinaryTarget, ProjectExportBuildError, ProjectExportBuildPreparation,
    ProjectExportBuildReceipt, ProjectExportCargoArtifact, ProjectExportPlanError,
    ProjectExportProductAgreement, ProjectExportRequest, prepare_project_export,
    revalidate_delivery_sources,
};

#[path = "profile_fixture.rs"]
mod profile_fixture;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(nara_ecs::Component, nara_reflect::PersistentComponent)]
#[nara(
    id = "test.export.SourceText",
    version = 1,
    alias = "Source text",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct SourceText {
    #[nara(id = "source", alias = "Source", capabilities(scene, asset_ref))]
    pub source: nara_asset::AssetRef,
}

#[derive(nara_ecs::Component, nara_reflect::PersistentComponent)]
#[nara(
    id = "test.export.FutureTarget",
    version = 1,
    alias = "Future target",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct FutureTarget {
    #[nara(id = "target", alias = "Target", capabilities(scene, asset_ref), asset_demand = deferred)]
    pub target: nara_asset::AssetRef,
}

nara_reflect::persistent_components! { pub mod source_text_components = [SourceText, FutureTarget]; }

#[derive(Default)]
struct SourceTextPlugin;

const SOURCE_TEXT_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::new(
        nara_reflect::ComponentSchemaOwnerId::new("test.export.source-text"),
        nara_app::PluginSchemaProviderId::new("test.export.source-text.schema"),
        nara_reflect::ComponentSchemaProviderBindingId::new("test.export.source-text.native", 1),
        source_text_components::initial_catalog,
        source_text_components::register,
    );

impl nara_app::Plugin for SourceTextPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        static DECLARATION: nara_app::PluginDeclaration = nara_app::PluginDeclaration::new(
            nara_app::PluginId::new("test.export.source-text"),
            nara_app::PluginCategory::Runtime,
        )
        .provides_schema(&[SOURCE_TEXT_PROVIDER.id()]);
        &DECLARATION
    }
    fn build(&self, _: &mut nara_app::App) -> Result<(), nara_app::PluginError> {
        Ok(())
    }
}

struct SourceTextDomain;

impl nara_asset::AssetDomain for SourceTextDomain {
    type Product = Vec<u8>;
}

impl nara_asset::SourceAssetImporter for SourceTextDomain {
    type Settings = ();
    const SOURCE_KIND: &'static str = "test.export.text";
    fn limits() -> nara_asset::AssetImportLimits {
        let bytes = nara_core::ByteLimit::new(1024).unwrap();
        nara_asset::AssetImportLimits::new(bytes, bytes, bytes)
    }
    fn prepare<E>(
        source: &mut nara_asset::AssetImportSource<'_, E>,
    ) -> Result<Self::Product, nara_asset::AssetImportError<E>> {
        source.metadata().admit::<Self::Settings>().unwrap();
        Ok(source
            .read_dependency(
                &nara_asset::AssetPath::new("suffix.txt").unwrap(),
                nara_core::ByteLimit::new(1024).unwrap(),
            )?
            .to_vec())
    }
}

fn source_text_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(
            crate::SchemaContribution::<SourceTextPlugin>::for_default([SOURCE_TEXT_PROVIDER])
                .unwrap(),
        )
        .unwrap()
        .add_asset_domain::<SourceTextDomain>()
        .unwrap()
}

#[test]
fn deferred_dependency_only_scenes_and_importer_reads_export_and_recheck_after_relocation() {
    use crate::project_host::{ProjectContentLoader, ProjectContentMemberKind};
    use nara_asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        ImportedProductId, StableAssetId,
    };
    use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, PersistentComponentProvider};

    let project = SourceProject::new();
    let manifest_path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("scenes = [\"b.scene.json\"]", "scenes = []");
    fs::write(&manifest_path, manifest).unwrap();
    let product = |id: u8| {
        AssetProductRef::new(
            StableAssetId::parse_str(format!("6c420000-0000-4000-8000-{id:012x}")).unwrap(),
            ImportedProductId::PRIMARY,
        )
        .unwrap()
    };
    let future_scene = |target| {
        SceneDocument::from_ordered_roots([nara_scene::SceneEntityRecord::new(
            nara_identity::SceneEntityId::new("door").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("test.export.FutureTarget"),
            nara_scene::SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                FutureTarget {
                    target: AssetRef::Product(target),
                }
                .__encode_persistent_component()
                .unwrap(),
            ),
        )])
    };
    for (name, id, scene) in [
        ("a.scene.json", 1, future_scene(product(2))),
        ("b.scene.json", 2, future_scene(product(3))),
        (
            "c.scene.json",
            3,
            SceneDocument::from_ordered_roots([nara_scene::SceneEntityRecord::new(
                nara_identity::SceneEntityId::new("text").unwrap(),
            )
            .with_component(
                ComponentTypeId::new("test.export.SourceText"),
                nara_scene::SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    SourceText {
                        source: AssetRef::path("text.txt").unwrap(),
                    }
                    .__encode_persistent_component()
                    .unwrap(),
                ),
            )]),
        ),
    ] {
        fs::write(
            project.root.join("scenes").join(name),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("scenes").join(format!("{name}.meta")),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    product(id).source(),
                    AssetPath::new(name).unwrap(),
                    AssetSourceKind::Scene,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
    }
    fs::write(project.root.join("assets/text.txt"), b"future source").unwrap();
    fs::write(
        project.root.join("assets/suffix.txt"),
        b"actual importer dependency",
    )
    .unwrap();
    fs::write(
        project.root.join("assets/text.txt.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                product(4).source(),
                AssetPath::new("text.txt").unwrap(),
                AssetSourceKind::Other(
                    <SourceTextDomain as nara_asset::SourceAssetImporter>::SOURCE_KIND.into(),
                ),
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, source_text_recipe()).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    let current = ProjectContentLoader::new(root.share())
        .unwrap()
        .load(&candidate, &plan)
        .unwrap();
    assert!(current.scene_dependencies().is_empty());
    assert!(
        current
            .domain_assets()
            .collection::<SourceTextDomain>()
            .is_none()
    );
    let recipe = source_text_recipe();
    let deliveries = ProjectExportRequest::check_deliveries(root.share(), &recipe).unwrap();
    let delivery = &deliveries[0];
    assert_eq!(
        delivery
            .scene_roots()
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>(),
        ["a.scene.json"]
    );
    for path in [
        "scenes/b.scene.json",
        "scenes/c.scene.json",
        "assets/text.txt",
        "assets/suffix.txt",
    ] {
        assert!(
            !delivery
                .startup_content()
                .members()
                .iter()
                .any(|member| member.path().as_str() == path)
        );
        assert!(
            delivery
                .members()
                .iter()
                .any(|member| member.path().as_str() == path),
            "{path}"
        );
    }
    assert_eq!(
        delivery
            .members()
            .iter()
            .filter(|member| member.kind() == ProjectContentMemberKind::Scene)
            .count(),
        3
    );
    drop(current);
    let prepare = |destination: &str| {
        let build = project
            .synthetic_build(
                startups(&deliveries),
                &delivery.schema_fingerprint().to_string(),
            )
            .unwrap();
        prepare_project_export(ProjectExportRequest::new(
            host_fs::writable_directory(&project.root).unwrap(),
            RelativeComponent::new(destination).unwrap(),
            deliveries.clone(),
            build,
            recipe.clone(),
        ))
    };
    let prepared = prepare("future-published").unwrap();
    let published = match prepared.publish() {
        super::ProjectExportOutcome::Published(receipt) => receipt,
        other => panic!("strict future delivery must publish: {other:?}"),
    };
    let relocated = project.root.join("future-published/project");
    for member in delivery.members() {
        assert_eq!(
            fs::read(relocated.join(member.path().as_str())).unwrap(),
            fs::read(project.root.join(member.path().as_str())).unwrap()
        );
    }
    let checked = check_project(ProjectCheckRequest::new(
        host_fs::readable_directory(&relocated).unwrap(),
        source_text_recipe(),
    ))
    .unwrap();
    assert_eq!(checked.members(), delivery.members());
    assert_eq!(
        published.plan().members().len(),
        delivery.members().len() + 2
    );
    fs::write(
        project.root.join("assets/suffix.txt"),
        b"changed future dependency",
    )
    .unwrap();
    assert!(prepare("changed-future").is_err());
    assert!(!project.root.join("changed-future").exists());
}

fn install_source_text_scene(project: &SourceProject, root: &DirectoryCapability) {
    use nara_reflect::PersistentComponentProvider;

    let metadata = nara_asset::AssetMeta::new(
        nara_asset::AssetMetaIdentity::new(
            nara_asset::StableAssetId::parse_str("4bc478ea-9882-49e8-91f1-62a2d5e9dc07").unwrap(),
            nara_asset::AssetPath::new("text.txt").unwrap(),
            nara_asset::AssetSourceKind::Other("test.export.text".to_owned()),
        ),
        (),
    );
    fs::write(
        project.root.join("assets/text.txt.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(project.root.join("assets/text.txt"), b"main source").unwrap();
    let component = SourceText {
        source: nara_asset::AssetRef::path("text.txt").unwrap(),
    };
    let scene = SceneDocument::from_ordered_roots([nara_scene::SceneEntityRecord::new(
        nara_identity::SceneEntityId::new("text").unwrap(),
    )
    .with_component(
        nara_reflect::ComponentTypeId::new("test.export.SourceText"),
        nara_scene::SceneComponentRecord::new(
            nara_reflect::ComponentSchemaVersion::ONE,
            component.__encode_persistent_component().unwrap(),
        ),
    )]);
    fs::write(
        project.root.join("scenes/a.scene.json"),
        scene.to_json_string().unwrap(),
    )
    .unwrap();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, source_text_recipe()).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
}

#[test]
fn domain_sources_export_exact_members_and_reject_changed_or_missing_dependencies() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    install_source_text_scene(&project, &root);
    fs::write(project.root.join("assets/suffix.txt"), b"original").unwrap();
    let recipe = source_text_recipe();
    let deliveries = ProjectExportRequest::check_deliveries(root.share(), &recipe).unwrap();
    let delivery = &deliveries[0];
    assert_eq!(
        delivery
            .members()
            .iter()
            .filter(|member| member.path().as_str() == "assets/suffix.txt")
            .count(),
        1
    );
    let prepare = |name: &str| {
        let build = project
            .synthetic_build(
                startups(&deliveries),
                &delivery.schema_fingerprint().to_string(),
            )
            .unwrap();
        prepare_project_export(ProjectExportRequest::new(
            host_fs::writable_directory(&project.root).unwrap(),
            RelativeComponent::new(name).unwrap(),
            deliveries.clone(),
            build,
            recipe.clone(),
        ))
    };
    let original = prepare("published").unwrap().publish();
    assert!(matches!(
        original,
        super::ProjectExportOutcome::Published(_)
    ));
    assert_eq!(
        fs::read(project.root.join("published/project/assets/suffix.txt")).unwrap(),
        b"original"
    );
    let prepared = prepare("changed").unwrap();
    fs::write(project.root.join("assets/suffix.txt"), b"modified").unwrap();
    assert!(matches!(
        prepared.publish(),
        super::ProjectExportOutcome::Rejected(_)
    ));
    assert!(!project.root.join("changed").exists());
    assert!(matches!(
        prepare("changed-before-prepare"),
        Err(ProjectExportPlanError::ContentChanged { .. })
    ));
    fs::remove_file(project.root.join("assets/suffix.txt")).unwrap();
    assert!(matches!(
        prepare("missing"),
        Err(ProjectExportPlanError::ContentChanged { .. })
    ));
    assert!(!project.root.join("missing").exists());
    assert_eq!(
        fs::read(project.root.join("published/project/assets/suffix.txt")).unwrap(),
        b"original"
    );
}

#[test]
fn export_merges_matching_input_and_raw_asset_uses_across_profiles() {
    use crate::project_host::ProjectContentMemberKind::{AssetSource, InputActionMap};

    use super::{ProjectExportMemberKind::Content, ProjectExportOutcome};

    const SHARED_INPUT: &str = "assets/suffix.txt";
    const OTHER_INPUT: &str = "assets/other.actions.json";
    let input: &[u8] =
        include_bytes!("../../../crates/nara_input/tests/fixtures/formats/v1/action-map.json");
    let digest = ContentDigest::of_bytes(input);
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    install_source_text_scene(&project, &root);
    for path in [SHARED_INPUT, OTHER_INPUT] {
        fs::write(project.root.join(path), input).unwrap();
    }
    let manifest_path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let recipe = source_text_recipe();

    for (name, headless, desktop) in [
        ("both-shared", SHARED_INPUT, SHARED_INPUT),
        ("headless-shared", SHARED_INPUT, OTHER_INPUT),
        ("desktop-shared", OTHER_INPUT, SHARED_INPUT),
        ("both-other", OTHER_INPUT, OTHER_INPUT),
    ] {
        fs::write(
            &manifest_path,
            format!(
                "{manifest}\n[input]\naction_map = '{headless}'\n\n\
                 [profiles.desktop.input]\naction_map = '{desktop}'\n"
            ),
        )
        .unwrap();
        let deliveries = ProjectExportRequest::check_deliveries(root.share(), &recipe).unwrap();
        for (delivery, input_path) in deliveries.iter().zip([headless, desktop]) {
            let expected_kind = if input_path == SHARED_INPUT {
                InputActionMap
            } else {
                AssetSource
            };
            let shared = delivery
                .members()
                .iter()
                .find(|member| member.path().as_str() == SHARED_INPUT)
                .unwrap();
            assert_eq!((shared.kind(), shared.digest()), (expected_kind, digest));
        }

        let build = project
            .synthetic_build(
                startups(&deliveries),
                &deliveries[0].schema_fingerprint().to_string(),
            )
            .unwrap();
        let prepared = prepare_project_export(ProjectExportRequest::new(
            host_fs::writable_directory(&project.root).unwrap(),
            RelativeComponent::new(name).unwrap(),
            deliveries.clone(),
            build,
            recipe.clone(),
        ))
        .unwrap_or_else(|error| panic!("{name} must prepare: {error:?}"));
        let shared_kind = if [headless, desktop].contains(&SHARED_INPUT) {
            InputActionMap
        } else {
            AssetSource
        };
        let includes_other = [headless, desktop].contains(&OTHER_INPUT);
        for (path, kind) in [
            (SHARED_INPUT, Some(shared_kind)),
            (OTHER_INPUT, includes_other.then_some(InputActionMap)),
        ] {
            let exported = format!("project/{path}");
            let mut members = prepared
                .plan()
                .members()
                .iter()
                .filter(|member| member.path().as_str() == exported);
            assert_eq!(
                members
                    .next()
                    .map(|member| (member.kind(), member.digest())),
                kind.map(|kind| (Content(kind), digest)),
                "{name}: {exported}"
            );
            assert!(members.next().is_none(), "{name}: duplicate {exported}");
        }
        let published = match prepared.publish() {
            ProjectExportOutcome::Published(receipt) => receipt,
            other => panic!("{name} must publish: {other:?}"),
        };
        let plan = published.plan();
        assert_eq!(
            plan.delivery_digest(),
            super::digest_profile_deliveries(&deliveries)
        );
        for (target, startup) in ProjectExportBinaryTarget::ALL
            .into_iter()
            .zip(startups(&deliveries))
        {
            assert_eq!(
                plan.startup_content_digest(target),
                startup.content_digest()
            );
            assert_eq!(
                plan.startup_closure_plan_digest(target),
                startup.closure_plan_digest()
            );
        }

        let relocated = project.root.join(format!("relocated-{name}"));
        fs::rename(project.root.join(name), &relocated).unwrap();
        let relocated = relocated.join("project");
        assert_eq!(fs::read(relocated.join(SHARED_INPUT)).unwrap(), input);
        if includes_other {
            assert_eq!(fs::read(relocated.join(OTHER_INPUT)).unwrap(), input);
        } else {
            assert!(!relocated.join(OTHER_INPUT).exists());
        }
        let relocated_root = host_fs::readable_directory(&relocated).unwrap();
        let rechecked = ProjectExportRequest::check_deliveries(relocated_root, &recipe).unwrap();
        for (checked, original) in rechecked.iter().zip(&deliveries) {
            assert_ne!(
                checked.project_root_identity(),
                original.project_root_identity()
            );
            assert_eq!(checked.members(), original.members());
            assert_eq!(checked.delivery_digest(), original.delivery_digest());
            assert_eq!(
                checked.startup_content().content_closure().stamp(),
                original.startup_content().content_closure().stamp()
            );
        }
    }
}

#[test]
fn cyclic_scenes_and_input_export_exact_sources_and_check_after_relocation() {
    use nara_asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        ImportedProductId, StableAssetId,
    };
    use nara_input::{
        ActionBinding, ActionContext, ActionId, ActionMapBuilder, ActionMapFileCandidate,
        ActionMapLimits, KeyCode, MouseButton,
    };
    use nara_reflect::PersistentComponentProvider;

    use crate::project_host::{ProjectContentLoader, ProjectContentMemberKind};

    let project = SourceProject::new();
    let input_path = "input/player.actions.json";
    let manifest_path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("scenes = [\"b.scene.json\"]", "scenes = []");
    fs::write(
        &manifest_path,
        format!("{manifest}\n[input]\naction_map = '{input_path}'\n"),
    )
    .unwrap();
    let primary = |id| {
        AssetProductRef::new(
            StableAssetId::parse_str(id).unwrap(),
            ImportedProductId::PRIMARY,
        )
        .unwrap()
    };
    let a = primary("5422b495-16b7-4f5c-b567-85811976c4a1");
    let b = primary("5422b495-16b7-4f5c-b567-85811976c4b2");
    let c = primary("5422b495-16b7-4f5c-b567-85811976c4c3");
    let input_product = primary("5422b495-16b7-4f5c-b567-85811976c4d4");
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    let menu = ActionContext::new("menu").unwrap();
    builder.declare_context(menu.clone(), false).unwrap();
    for binding in [
        ActionBinding::key(ActionId::new("jump").unwrap(), KeyCode::Space),
        ActionBinding::mouse(ActionId::new("fire").unwrap(), MouseButton::Left),
        ActionBinding::key(ActionId::new("confirm").unwrap(), KeyCode::Enter).with_context(menu),
    ] {
        builder.bind(binding).unwrap();
    }
    let input_file =
        ActionMapFileCandidate::new(input_product, builder.compile().unwrap()).unwrap();
    let input_bytes = input_file.to_json_string().unwrap().into_bytes();
    // The manifest selects a project-relative source outside Assets, with intrinsic identity only.
    fs::create_dir(project.root.join("input")).unwrap();
    fs::write(project.root.join(input_path), &input_bytes).unwrap();
    assert!(!project.root.join(format!("{input_path}.meta")).exists());
    let linked_scene = |target| {
        let component = SourceText {
            source: AssetRef::Product(target),
        };
        SceneDocument::from_ordered_roots([nara_scene::SceneEntityRecord::new(
            nara_identity::SceneEntityId::new("door").unwrap(),
        )
        .with_component(
            nara_reflect::ComponentTypeId::new("test.export.SourceText"),
            nara_scene::SceneComponentRecord::new(
                nara_reflect::ComponentSchemaVersion::ONE,
                component.__encode_persistent_component().unwrap(),
            ),
        )])
    };
    let scene_a = linked_scene(b);
    let scene_b = linked_scene(a);
    let scene_c = SceneDocument::default();
    for (name, product, scene) in [
        ("a.scene.json", a, &scene_a),
        ("b.scene.json", b, &scene_b),
        ("c.scene.json", c, &scene_c),
    ] {
        fs::write(
            project.root.join("scenes").join(name),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                product.source(),
                AssetPath::new(name).unwrap(),
                AssetSourceKind::Scene,
            ),
            (),
        );
        fs::write(
            project.root.join("scenes").join(format!("{name}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }
    let root = host_fs::readable_directory(&project.root).unwrap();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    assert!(candidate.settings().content.scenes.is_empty());
    assert_eq!(candidate.input_requirement().unwrap().as_str(), input_path);
    let plan = resolve_product_recipe(&candidate, source_text_recipe()).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let recipe = source_text_recipe();
    let deliveries = ProjectExportRequest::check_deliveries(root.share(), &recipe).unwrap();
    let delivery = &deliveries[0];
    assert_eq!(
        delivery
            .scene_roots()
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>(),
        ["a.scene.json"]
    );
    assert_eq!(
        delivery
            .members()
            .iter()
            .map(|member| (member.kind(), member.path().as_str()))
            .collect::<Vec<_>>(),
        [
            (ProjectContentMemberKind::InputActionMap, input_path),
            (ProjectContentMemberKind::ProjectManifest, "nara.toml"),
            (ProjectContentMemberKind::Scene, "scenes/a.scene.json"),
            (
                ProjectContentMemberKind::AssetMetadata,
                "scenes/a.scene.json.meta",
            ),
            (ProjectContentMemberKind::Scene, "scenes/b.scene.json"),
            (
                ProjectContentMemberKind::AssetMetadata,
                "scenes/b.scene.json.meta",
            ),
            (
                ProjectContentMemberKind::AuthoringSchemaBundle,
                "schema/authoring.bundle.json",
            ),
        ]
    );
    assert_eq!(delivery.startup_content().members(), delivery.members());
    let input_member = delivery
        .members()
        .iter()
        .find(|member| member.kind() == ProjectContentMemberKind::InputActionMap)
        .unwrap();
    assert_eq!(
        input_member.digest(),
        crate::fs::ContentDigest::of_bytes(&input_bytes)
    );
    assert_eq!(
        input_member.byte_len(),
        u64::try_from(input_bytes.len()).unwrap()
    );

    // Synthetic artifacts prove source custody and publication only; no process runs here.
    let prepare = |destination: &str| {
        let build = project
            .synthetic_build(
                startups(&deliveries),
                &delivery.schema_fingerprint().to_string(),
            )
            .unwrap();
        prepare_project_export(ProjectExportRequest::new(
            host_fs::writable_directory(&project.root).unwrap(),
            RelativeComponent::new(destination).unwrap(),
            deliveries.clone(),
            build,
            recipe.clone(),
        ))
    };
    let prepared = prepare("published").unwrap();
    // Catalogue discovery does not make an unrelated Scene part of the exact source receipt.
    fs::write(
        project.root.join("scenes/c.scene.json"),
        b"{ unrelated Scene C is not demanded",
    )
    .unwrap();
    let published = match prepared.publish() {
        super::ProjectExportOutcome::Published(receipt) => receipt,
        other => panic!("source export must publish: {other:?}"),
    };
    let published_path = project.root.join("published");
    assert_eq!(
        published.plan().members().len(),
        delivery.members().len() + 2
    );
    let exported_input = published
        .plan()
        .members()
        .iter()
        .find(|member| {
            member.kind()
                == super::ProjectExportMemberKind::Content(ProjectContentMemberKind::InputActionMap)
        })
        .unwrap();
    assert_eq!(
        exported_input.path().as_str(),
        format!("project/{input_path}")
    );
    assert_eq!(exported_input.digest(), input_member.digest());
    assert_eq!(
        fs::read(published_path.join(exported_input.path().as_str())).unwrap(),
        input_bytes
    );
    assert!(
        !published_path
            .join("project")
            .join(format!("{input_path}.meta"))
            .exists()
    );
    let published_bytes = published
        .plan()
        .members()
        .iter()
        .map(|member| {
            let bytes = fs::read(published_path.join(member.path().as_str())).unwrap();
            assert_eq!(crate::fs::ContentDigest::of_bytes(&bytes), member.digest());
            (member.path().clone(), bytes)
        })
        .collect::<Vec<_>>();
    for path in ["scenes/c.scene.json", "scenes/c.scene.json.meta"] {
        assert!(!published_path.join("project").join(path).exists());
    }
    drop(published);

    for (path, missing) in [
        ("scenes/b.scene.json", false),
        ("scenes/b.scene.json.meta", false),
        (input_path, false),
        (input_path, true),
    ] {
        let prepared = prepare("published").unwrap();
        let root_entries = entries(&project.root);
        let original = fs::read(project.root.join(path)).unwrap();
        if missing {
            fs::remove_file(project.root.join(path)).unwrap();
        } else {
            let mut changed = original.clone();
            *changed.iter_mut().find(|byte| **byte == b' ').unwrap() = b'\t';
            if path == input_path {
                let changed_input = ActionMapFileCandidate::decode_json_bytes(&changed).unwrap();
                assert_eq!(changed_input.product(), input_product);
            }
            fs::write(project.root.join(path), changed).unwrap();
        }
        match prepare("rejected-before-prepare") {
            Err(ProjectExportPlanError::ContentChanged { path: rejected, .. }) => {
                assert_eq!(rejected.as_str(), path);
            }
            other => panic!("changed or missing source must refuse preparation: {other:?}"),
        }
        let rejection = match prepared.publish() {
            super::ProjectExportOutcome::Rejected(rejection) => rejection,
            other => panic!("changed or missing source must refuse publication: {other:?}"),
        };
        assert_eq!(
            rejection.kind(),
            super::ProjectExportRejectionKind::SourceChanged
        );
        match rejection.source_change() {
            Some(ProjectExportPlanError::ContentChanged {
                path: changed,
                source,
            }) => {
                assert_eq!(changed.as_str(), path);
                if missing {
                    assert!(matches!(
                        source,
                        FsError::Io { source, .. }
                            if source.kind() == std::io::ErrorKind::NotFound
                    ));
                } else {
                    assert!(matches!(source, FsError::DigestMismatch { .. }));
                }
            }
            other => panic!("unexpected dependency source error: {other:?}"),
        }
        assert_eq!(entries(&project.root), root_entries);
        for (path, bytes) in &published_bytes {
            assert_eq!(
                fs::read(published_path.join(path.as_str())).unwrap(),
                *bytes
            );
        }
        fs::write(project.root.join(path), original).unwrap();
    }

    // Move the actual export within this owned temporary directory, then invalidate the originals.
    let relocated = project.root.join("relocated-export");
    fs::rename(&published_path, &relocated).unwrap();
    for path in ["scenes/a.scene.json", "scenes/b.scene.json"] {
        fs::write(project.root.join(path), b"{ original Scene is unavailable").unwrap();
    }
    fs::write(
        project.root.join(input_path),
        b"{ original Input is unavailable",
    )
    .unwrap();
    let exported_root = host_fs::readable_directory(&relocated.join("project")).unwrap();
    assert_ne!(exported_root.identity(), root.identity());
    let manifest = exported_root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, source_text_recipe()).unwrap();
    let loader = ProjectContentLoader::new(exported_root.share()).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let input = snapshot.input_action_map().unwrap();
    assert_eq!(input.product(), input_product);
    assert_eq!(input.path().as_str(), input_path);
    assert_eq!(input.source_digest(), input_member.digest());
    assert_eq!(
        input.definition().contexts(),
        input_file.definition().contexts()
    );
    assert_eq!(
        input.definition().bindings(),
        input_file.definition().bindings()
    );
    assert_eq!(
        fs::read(relocated.join("project").join(input_path)).unwrap(),
        input_bytes
    );
    assert_eq!(snapshot.selected_scene_path().as_str(), "a.scene.json");
    assert_eq!(snapshot.selected_scene_product(), Some(a));
    assert_eq!(snapshot.scene(), &scene_a);
    assert_eq!(snapshot.scene_dependencies().len(), 1);
    let dependency = &snapshot.scene_dependencies()[0];
    assert_eq!(dependency.path().as_str(), "scenes/b.scene.json");
    assert_eq!(dependency.document(), &scene_b);
    for (product, path) in [(a, "scenes/a.scene.json"), (b, "scenes/b.scene.json")] {
        let record = snapshot
            .asset_resolution()
            .resolve_ref(&AssetRef::Product(product))
            .unwrap();
        assert_eq!(record.path().as_str(), path);
        assert_eq!(record.source_kind(), &AssetSourceKind::Scene);
    }
    assert!(
        snapshot
            .asset_resolution()
            .resolve_ref(&AssetRef::Product(c))
            .is_err()
    );
    let checked = check_project(ProjectCheckRequest::new(
        exported_root,
        source_text_recipe(),
    ))
    .unwrap();
    assert_ne!(
        checked.project_root_identity(),
        delivery.project_root_identity()
    );
    assert_eq!(checked.scene_roots(), delivery.scene_roots());
    assert_eq!(checked.members(), delivery.members());
    assert_eq!(checked.delivery_digest(), delivery.delivery_digest());
    assert_eq!(
        checked.startup_content().content_digest(),
        delivery.startup_content().content_digest()
    );
    assert_eq!(
        checked.startup_content().members(),
        delivery.startup_content().members()
    );
}

#[test]
fn export_delivers_distinct_later_level_images_for_equal_startups() {
    let project = SourceProject::new();
    fs::write(
        project.root.join("scenes/a.scene.json"),
        include_bytes!("../../../starters/desktop-2d/v1/scenes/startup.scene.json"),
    )
    .unwrap();
    profile_fixture::install_profiled_level(&project.root);
    profile_fixture::assert_profiled_level_images(&project.root);

    let root = host_fs::readable_directory(&project.root).unwrap();
    let deliveries = project.check(&root);
    let [headless, desktop] = startups(&deliveries);
    assert_eq!(headless.content_digest(), desktop.content_digest());
    assert_eq!(
        headless.closure_plan_digest(),
        desktop.closure_plan_digest()
    );
    assert_ne!(
        deliveries[0].delivery_digest(),
        deliveries[1].delivery_digest()
    );

    let request = |destination: &str, checked: [ProjectContentDeliveryReceipt; 2]| {
        let build = project
            .synthetic_build(
                startups(&deliveries),
                &headless.schema_fingerprint().to_string(),
            )
            .unwrap();
        ProjectExportRequest::new(
            host_fs::writable_directory(&project.root).unwrap(),
            RelativeComponent::new(destination).unwrap(),
            checked,
            build,
            ProductRecipe::new(),
        )
    };
    assert!(matches!(
        prepare_project_export(request(
            "wrong-profile",
            [deliveries[0].clone(), deliveries[0].clone()],
        )),
        Err(ProjectExportPlanError::Build(
            ProjectExportBuildError::StartupProfileMismatch {
                target: ProjectExportBinaryTarget::Desktop,
            }
        ))
    ));

    let stale = prepare_project_export(request("stale", deliveries.clone())).unwrap();
    let desktop_image = project.root.join("prefabs/flag.png");
    let original = fs::read(&desktop_image).unwrap();
    fs::remove_file(&desktop_image).unwrap();
    let rejection = match stale.publish() {
        super::ProjectExportOutcome::Rejected(rejection) => rejection,
        other => panic!("a stale desktop-only image must refuse publication: {other:?}"),
    };
    assert!(matches!(
        rejection.source_change(),
        Some(ProjectExportPlanError::ContentChanged { path, .. })
            if path.as_str() == "prefabs/flag.png"
    ));
    assert!(!project.root.join("stale").exists());
    fs::write(&desktop_image, original).unwrap();

    let prepared =
        prepare_project_export(request("published-profiles", deliveries.clone())).unwrap();
    let expected = deliveries
        .iter()
        .flat_map(ProjectContentDeliveryReceipt::members)
        .map(|member| {
            (
                format!("project/{}", member.path()),
                (member.kind(), member.digest()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let planned = prepared
        .plan()
        .members()
        .iter()
        .filter_map(|member| match member.kind() {
            super::ProjectExportMemberKind::Content(kind) => {
                Some((member.path().as_str(), (kind, member.digest())))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        planned,
        expected
            .iter()
            .map(|(path, facts)| (path.as_str(), *facts))
            .collect::<Vec<_>>(),
    );
    for path in [
        "project/assets/flag.png",
        "project/assets/flag.png.meta",
        "project/prefabs/flag.png",
        "project/prefabs/flag.png.meta",
    ] {
        assert!(expected.contains_key(path), "{path}");
    }

    match prepared.publish() {
        super::ProjectExportOutcome::Published(_) => {}
        other => panic!("the complete dual-profile export must publish: {other:?}"),
    }
    let relocated = project.root.join("relocated-profiles");
    fs::rename(project.root.join("published-profiles"), &relocated).unwrap();
    fs::remove_file(project.root.join("assets/flag.png")).unwrap();
    fs::remove_file(desktop_image).unwrap();
    profile_fixture::assert_profiled_level_images(&relocated.join("project"));
}

#[test]
fn export_rejects_staged_stable_id_collision_between_profiles() {
    let project = SourceProject::new();
    fs::write(
        project.root.join("scenes/a.scene.json"),
        include_bytes!("../../../starters/desktop-2d/v1/scenes/startup.scene.json"),
    )
    .unwrap();
    let manifest_path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("prefabs = \"prefabs\"", "prefabs = \"desktop-assets\"");
    fs::write(
        manifest_path,
        format!(
            "{manifest}\n[profiles.desktop.paths]\nassets = 'desktop-assets'\nprefabs = 'an-other'\n"
        ),
    )
    .unwrap();
    let desktop_assets = project.root.join("desktop-assets");
    fs::create_dir(&desktop_assets).unwrap();
    fs::create_dir(project.root.join("an-other")).unwrap();
    profile_fixture::write_flag_scene(
        &project.root.join("scenes"),
        nara_asset::AssetRef::stable_id(profile_fixture::FLAG_ID).unwrap(),
    );
    profile_fixture::write_flag(
        &project.root.join("assets"),
        profile_fixture::FLAG_ID,
        [255, 0, 0, 255],
    );

    let root = host_fs::readable_directory(&project.root).unwrap();
    let recipe = ProductRecipe::new();
    let headless = check_project(ProjectCheckRequest::new(root.share(), recipe.clone())).unwrap();
    profile_fixture::write_flag(&desktop_assets, profile_fixture::FLAG_ID, [0, 0, 255, 255]);
    let desktop = check_project(
        ProjectCheckRequest::new(root.share(), recipe.clone()).with_profile("desktop"),
    )
    .unwrap();

    let deliveries = [headless, desktop];
    let build = project
        .synthetic_build(
            startups(&deliveries),
            &deliveries[0].schema_fingerprint().to_string(),
        )
        .unwrap();
    let prepared = prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(&project.root).unwrap(),
        RelativeComponent::new("conflicting-identities").unwrap(),
        deliveries,
        build,
        recipe,
    ))
    .unwrap();
    for path in [
        "project/assets/flag.png",
        "project/assets/flag.png.meta",
        "project/desktop-assets/flag.png",
        "project/desktop-assets/flag.png.meta",
    ] {
        assert!(
            prepared
                .plan()
                .members()
                .iter()
                .any(|member| member.path().as_str() == path),
            "{path}"
        );
    }

    let before = entries(&project.root);
    let orphan = match prepared.publish() {
        super::ProjectExportOutcome::Orphaned(orphan) => orphan,
        other => panic!("the staged stable identity collision must reject publication: {other:?}"),
    };
    let Some(super::ProjectExportFailure::Staging(super::ProjectExportStagingError::Delivery {
        target,
        source,
    })) = &orphan.failure
    else {
        panic!("the rejection must come from the staged loader: {orphan:?}");
    };
    assert_eq!(*target, ProjectExportBinaryTarget::Headless);
    let error = source.content_error().unwrap();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.asset-identity-conflict"
    }));
    let mut expected = before;
    expected.push(
        orphan
            .publication_orphan()
            .staging_name()
            .to_str()
            .unwrap()
            .into(),
    );
    expected.sort_unstable();
    assert_eq!(entries(&project.root), expected);
    assert!(!project.root.join("conflicting-identities").exists());
    let failure = crate::project_tooling::test_export_orphan_protocol_roundtrip(orphan);
    assert_eq!(
        failure.stage(),
        crate::project_tooling::ProjectToolFailureStage::Publication
    );
    assert_eq!(failure.code(), "project.tool.export-orphaned");
    assert!(failure.summary().contains("staged default profile"));
    assert!(
        failure
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code() == "project.content.asset-identity-conflict" })
    );
}

#[test]
fn export_rejects_conflicting_profile_observations_of_a_shared_later_scene() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let [headless, _] = project.check(&root);
    let later_scene = project.root.join("scenes/b.scene.json");
    let mut bytes = fs::read(&later_scene).unwrap();
    bytes.push(b'\n');
    fs::write(later_scene, bytes).unwrap();
    let [_, desktop] = project.check(&root);
    let deliveries = [headless, desktop];
    let build = project
        .synthetic_build(
            startups(&deliveries),
            &deliveries[0].schema_fingerprint().to_string(),
        )
        .unwrap();
    let before = entries(&project.root);
    let error = prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(&project.root).unwrap(),
        RelativeComponent::new("conflict").unwrap(),
        deliveries,
        build,
        ProductRecipe::new(),
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        ProjectExportPlanError::ContentObservationConflict { path }
            if path.as_str() == "scenes/b.scene.json"
    ));
    assert_eq!(entries(&project.root), before);
}

#[test]
fn export_revalidation_rejects_a_changed_nonstartup_delivery_scene_before_staging() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let deliveries = project.check(&root);
    let delivery = &deliveries[0];
    assert_eq!(
        delivery
            .scene_roots()
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>(),
        ["a.scene.json", "b.scene.json"]
    );
    assert!(
        delivery
            .members()
            .iter()
            .any(|member| { member.path().as_str() == "scenes/b.scene.json" })
    );
    assert!(
        delivery
            .startup_content()
            .members()
            .iter()
            .all(|member| { member.path().as_str() != "scenes/b.scene.json" })
    );
    let build = project
        .synthetic_build(
            startups(&deliveries),
            &delivery.schema_fingerprint().to_string(),
        )
        .unwrap();
    let prepared = prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(&project.root).unwrap(),
        RelativeComponent::new("changed-delivery").unwrap(),
        deliveries,
        build,
        ProductRecipe::new(),
    ))
    .unwrap();
    let root_entries = entries(&project.root);
    let scene_entries = entries(&project.root.join("scenes"));
    let startup_bytes = fs::read(project.root.join("scenes/a.scene.json")).unwrap();
    let manifest_bytes = fs::read(project.root.join("nara.toml")).unwrap();

    // Keep B valid, the byte length equal, and startup unchanged: delivery owns exact bytes.
    let second_scene = project.root.join("scenes/b.scene.json");
    let mut changed_bytes = fs::read(&second_scene).unwrap();
    *changed_bytes
        .iter_mut()
        .find(|byte| **byte == b' ')
        .unwrap() = b'\t';
    fs::write(&second_scene, changed_bytes).unwrap();

    let rejection = match prepared.publish() {
        super::ProjectExportOutcome::Rejected(rejection) => rejection,
        other => panic!("changed delivery must refuse publication: {other:?}"),
    };
    match rejection.source_change() {
        Some(ProjectExportPlanError::ContentChanged { path, source }) => {
            assert_eq!(path.as_str(), "scenes/b.scene.json");
            assert!(matches!(source, FsError::DigestMismatch { .. }));
        }
        other => panic!("unexpected export source error: {other:?}"),
    }
    assert_eq!(
        fs::read(project.root.join("scenes/a.scene.json")).unwrap(),
        startup_bytes
    );
    assert_eq!(
        fs::read(project.root.join("nara.toml")).unwrap(),
        manifest_bytes
    );
    assert_eq!(entries(&project.root), root_entries);
    assert_eq!(entries(&project.root.join("scenes")), scene_entries);
}

#[test]
fn export_rejects_a_build_agreement_for_a_changed_desktop_startup() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let manifest_path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        manifest_path,
        format!("{manifest}\n[profiles.desktop.startup]\ndefault_scene = 'desktop.scene.json'\n"),
    )
    .unwrap();
    let desktop_scene = project.root.join("scenes/desktop.scene.json");
    fs::copy(project.root.join("scenes/a.scene.json"), &desktop_scene).unwrap();
    let original = project.check(&root);
    let build = project
        .synthetic_build(
            startups(&original),
            &original[0].schema_fingerprint().to_string(),
        )
        .unwrap();
    build.revalidate(startups(&original)).unwrap();
    let root_entries = entries(&project.root);
    let mut startup_bytes = fs::read(&desktop_scene).unwrap();
    startup_bytes.push(b'\n');
    fs::write(&desktop_scene, startup_bytes).unwrap();

    let current = project.check(&root);
    assert_eq!(original[0].delivery_digest(), current[0].delivery_digest());
    assert_ne!(
        original[1].startup_content().content_digest(),
        current[1].startup_content().content_digest()
    );
    let error = prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(&project.root).unwrap(),
        RelativeComponent::new("candidate").unwrap(),
        current,
        build,
        ProductRecipe::new(),
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        ProjectExportPlanError::Build(ProjectExportBuildError::ProductAgreementMismatch {
            target: ProjectExportBinaryTarget::Desktop
        })
    ));
    assert_eq!(entries(&project.root), root_entries);
}

#[test]
fn startup_build_agreement_does_not_require_observing_nonstartup_delivery_content() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let original = project.check(&root);
    let build = project
        .synthetic_build(
            startups(&original),
            &original[0].schema_fingerprint().to_string(),
        )
        .unwrap();
    let old_plan =
        super::plan_export(RelativeComponent::new("old").unwrap(), &original, &build).unwrap();
    let root_entries = entries(&project.root);
    let mut second_bytes = fs::read(project.root.join("scenes/b.scene.json")).unwrap();
    second_bytes.push(b'\n');
    fs::write(project.root.join("scenes/b.scene.json"), second_bytes).unwrap();

    let current = project.check(&root);
    assert_ne!(original[0].delivery_digest(), current[0].delivery_digest());
    assert_eq!(
        original[0].startup_content().closure_plan_digest(),
        current[0].startup_content().closure_plan_digest()
    );
    // Product agreement is intentionally startup-scoped; complete delivery validation remains a
    // separate export gate and must reject this changed non-startup source.
    build.revalidate(startups(&current)).unwrap();
    prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(&project.root).unwrap(),
        RelativeComponent::new("current").unwrap(),
        current,
        build,
        ProductRecipe::new(),
    ))
    .unwrap();
    assert!(matches!(
        revalidate_delivery_sources(&root, &old_plan),
        Err(ProjectExportPlanError::ContentChanged { ref path, .. })
            if path.as_str() == "scenes/b.scene.json"
    ));
    assert_eq!(entries(&project.root), root_entries);
}

#[test]
fn build_binding_rejects_a_product_schema_fingerprint_different_from_its_startup() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let checked = project.check(&root);
    let wrong_schema = "00".repeat(32);
    assert_ne!(checked[0].schema_fingerprint().to_string(), wrong_schema);
    let error = project
        .synthetic_build(startups(&checked), &wrong_schema)
        .unwrap_err();
    assert!(matches!(
        error,
        ProjectExportBuildError::ProductAgreementMismatch {
            target: ProjectExportBinaryTarget::Headless
        }
    ));
    assert!(!project.root.join("candidate").exists());
}

#[test]
fn export_stages_captured_executable_bytes_after_cargo_outputs_change_or_disappear() {
    let project = SourceProject::new();
    let manifest_path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        manifest_path,
        format!("{manifest}\n[profiles.desktop.paths]\nprefabs = 'desktop-prefabs'\n"),
    )
    .unwrap();
    fs::create_dir(project.root.join("desktop-prefabs")).unwrap();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let checked = project.check(&root);
    let build = project
        .synthetic_build(
            startups(&checked),
            &checked[0].schema_fingerprint().to_string(),
        )
        .unwrap();
    let executables = build.executables();
    for executable in executables {
        let bytes = format!("synthetic {:?} bytes", executable.target());
        assert_eq!(
            executable.digest(),
            ContentDigest::of_bytes(bytes.as_bytes())
        );
        assert_eq!(
            executable.build_provenance_digest(),
            ContentDigest::of_bytes(b"synthetic export build provenance; no Cargo build ran")
        );
    }
    fs::write(
        project.root.join("build-products/headless"),
        b"replaced before preparation",
    )
    .unwrap();
    fs::remove_file(project.root.join("build-products/desktop")).unwrap();
    build.revalidate(startups(&checked)).unwrap();
    let prepared = prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(&project.root).unwrap(),
        RelativeComponent::new("captured").unwrap(),
        checked,
        build,
        ProductRecipe::new(),
    ))
    .unwrap();
    fs::remove_file(project.root.join("build-products/headless")).unwrap();
    let published = match prepared.publish() {
        super::ProjectExportOutcome::Published(receipt) => receipt,
        other => panic!("captured executable bytes must remain exportable: {other:?}"),
    };
    assert!(
        project
            .root
            .join("captured/project/desktop-prefabs")
            .is_dir()
    );
    assert!(project.root.join("captured/project/prefabs").is_dir());
    for executable in executables {
        let kind = match executable.target() {
            ProjectExportBinaryTarget::Headless => {
                super::ProjectExportMemberKind::HeadlessExecutable
            }
            ProjectExportBinaryTarget::Desktop => super::ProjectExportMemberKind::DesktopExecutable,
        };
        let member = published
            .plan()
            .members()
            .iter()
            .find(|member| member.kind() == kind)
            .unwrap();
        assert_eq!(member.digest(), executable.digest());
        assert_eq!(
            fs::read(project.root.join("captured").join(member.path().as_str())).unwrap(),
            format!("synthetic {:?} bytes", executable.target()).as_bytes()
        );
    }
}

#[test]
fn build_receipt_accepts_explicit_profiles_and_feature_sets() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let checked = project.check(&root);
    let schema = checked[0].schema_fingerprint().to_string();
    let custom = project
        .synthetic_build_with_selection(
            startups(&checked),
            &schema,
            "shipping",
            &["game", "domain", "game"],
        )
        .unwrap();
    assert_eq!(custom.profile(), "shipping");
    assert_eq!(custom.features(), ["domain", "game"]);
    custom.revalidate(startups(&checked)).unwrap();
    let minimal = project
        .synthetic_build_with_selection(startups(&checked), &schema, "dev", &[])
        .unwrap();
    assert_eq!(minimal.profile(), "dev");
    assert!(minimal.features().is_empty());
    minimal.revalidate(startups(&checked)).unwrap();
}

#[test]
fn build_binding_rejects_superseded_artifacts_even_when_product_agreements_still_match() {
    for stale_role in [
        None,
        Some(ProjectExportBinaryTarget::Headless),
        Some(ProjectExportBinaryTarget::Desktop),
    ] {
        let project = SourceProject::new();
        let root = host_fs::readable_directory(&project.root).unwrap();
        let original = project.check(&root);
        let schema = original[0].schema_fingerprint().to_string();
        let inputs = project.write_synthetic_build_inputs();
        let old_preparation =
            ProjectExportBuildPreparation::capture(startups(&original), inputs.clone()).unwrap();
        let ([old_headless, old_desktop], [old_headless_agreement, old_desktop_agreement]) =
            project.synthetic_artifacts(
                startups(&original),
                &schema,
                "shipping",
                &["desktop"],
                &old_preparation,
            );

        fs::write(
            project.root.join("src/lib.rs"),
            "pub fn revision() -> u8 { 2 }\n",
        )
        .unwrap();
        let current = project.check(&root);
        assert_eq!(original[0].manifest_digest(), current[0].manifest_digest());
        assert_eq!(
            original[0].schema_fingerprint(),
            current[0].schema_fingerprint()
        );
        assert_eq!(
            original[0].startup_content().content_digest(),
            current[0].startup_content().content_digest()
        );
        assert_eq!(
            original[0].startup_content().closure_plan_digest(),
            current[0].startup_content().closure_plan_digest()
        );
        assert_eq!(
            original[0].startup_content().runtime_schema_bundle_digest(),
            current[0].startup_content().runtime_schema_bundle_digest()
        );
        let current_preparation =
            ProjectExportBuildPreparation::capture(startups(&current), inputs).unwrap();
        assert_ne!(
            old_preparation.build_input_digest(),
            current_preparation.build_input_digest()
        );
        current_preparation.verify_build_inputs(&root).unwrap();
        let ([new_headless, new_desktop], [new_headless_agreement, new_desktop_agreement]) =
            project.synthetic_artifacts(
                startups(&current),
                &schema,
                "shipping",
                &["desktop"],
                &current_preparation,
            );
        let (artifacts, agreements, expected_role) = match stale_role {
            None => (
                [old_headless, old_desktop],
                [old_headless_agreement, old_desktop_agreement],
                ProjectExportBinaryTarget::Headless,
            ),
            Some(ProjectExportBinaryTarget::Headless) => (
                [old_headless, new_desktop],
                [old_headless_agreement, new_desktop_agreement],
                ProjectExportBinaryTarget::Headless,
            ),
            Some(ProjectExportBinaryTarget::Desktop) => (
                [new_headless, old_desktop],
                [new_headless_agreement, old_desktop_agreement],
                ProjectExportBinaryTarget::Desktop,
            ),
        };
        assert!(matches!(
            current_preparation.bind(artifacts, agreements),
            Err(ProjectExportBuildError::ArtifactBuildInputsMismatch { target })
                if target == expected_role
        ));
        assert!(!project.root.join("candidate").exists());
    }
}

#[test]
fn build_binding_requires_the_exact_declared_inventory_not_only_overlapping_digests() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let checked = project.check(&root);
    project.write_synthetic_build_inputs();
    fs::write(project.root.join("src/extra.rs"), b"extra").unwrap();
    for captured_paths in [
        vec!["Cargo.toml", "Cargo.lock"],
        vec!["Cargo.toml", "Cargo.lock", "src/lib.rs", "src/extra.rs"],
        vec!["Cargo.toml", "Cargo.lock", "src/extra.rs"],
    ] {
        let captured_preparation = ProjectExportBuildPreparation::capture(
            startups(&checked),
            captured_paths
                .into_iter()
                .map(|path| RelativePath::new(path).unwrap()),
        )
        .unwrap();
        let (artifacts, agreements) = project.synthetic_artifacts(
            startups(&checked),
            &checked[0].schema_fingerprint().to_string(),
            "shipping",
            &["desktop"],
            &captured_preparation,
        );
        let preparation = ProjectExportBuildPreparation::capture(
            startups(&checked),
            ["Cargo.toml", "Cargo.lock", "src/lib.rs"].map(|path| RelativePath::new(path).unwrap()),
        )
        .unwrap();
        assert!(matches!(
            preparation.bind(artifacts, agreements),
            Err(ProjectExportBuildError::ArtifactBuildInputsMismatch {
                target: ProjectExportBinaryTarget::Headless,
            })
        ));
    }
}

#[test]
fn build_preparation_revalidates_only_its_declared_dynamic_inventory() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let checked = project.check(&root);
    project.write_synthetic_build_inputs();
    fs::write(project.root.join("src/selected.rs"), b"selected input A").unwrap();
    let paths = ["Cargo.toml", "Cargo.lock", "src/selected.rs"];
    let preparation = ProjectExportBuildPreparation::capture(
        startups(&checked),
        paths.map(|path| RelativePath::new(path).unwrap()),
    )
    .unwrap();
    let reordered = ProjectExportBuildPreparation::capture(
        startups(&checked),
        paths
            .into_iter()
            .rev()
            .map(|path| RelativePath::new(path).unwrap()),
    )
    .unwrap();
    assert_eq!(
        preparation.build_input_digest(),
        reordered.build_input_digest()
    );
    fs::write(project.root.join("src/lib.rs"), b"unlisted source changed").unwrap();
    fs::write(project.root.join("src/unlisted.rs"), b"new unlisted source").unwrap();
    preparation
        .verify_build_inputs(&host_fs::readable_directory(&project.root).unwrap())
        .unwrap();
    fs::write(project.root.join("src/selected.rs"), b"selected input B").unwrap();
    assert!(matches!(
        preparation.verify_build_inputs(&root),
        Err(ProjectExportBuildError::BuildInputsChanged)
    ));
}

#[test]
fn build_preparation_rejects_replacing_an_original_file_with_identical_bytes() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let checked = project.check(&root);
    let inputs = project.write_synthetic_build_inputs();
    let preparation = ProjectExportBuildPreparation::capture(startups(&checked), inputs).unwrap();
    let original = project.root.join("Cargo.lock");
    let bytes = fs::read(&original).unwrap();
    fs::rename(&original, project.root.join("previous.lock")).unwrap();
    fs::write(&original, bytes).unwrap();
    assert!(matches!(
        preparation.verify_build_inputs(&root),
        Err(ProjectExportBuildError::BuildInputsChanged)
    ));
}

#[test]
fn build_preparation_rejects_invalid_inventories_before_reading_files() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let checked = project.check(&root);
    for (provided, missing) in [("Cargo.toml", "Cargo.lock"), ("Cargo.lock", "Cargo.toml")] {
        let error = ProjectExportBuildPreparation::capture(
            startups(&checked),
            [RelativePath::new(provided).unwrap()],
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ProjectExportBuildError::MissingBuildInput { path } if path == missing
        ));
    }
    let error = ProjectExportBuildPreparation::capture(
        startups(&checked),
        ["Cargo.toml", "Cargo.lock", "Cargo.toml"].map(|path| RelativePath::new(path).unwrap()),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ProjectExportBuildError::DuplicateBuildInput { path } if path == "Cargo.toml"
    ));
    let error = ProjectExportBuildPreparation::capture(
        startups(&checked),
        (0..1025).map(|index| RelativePath::new(format!("declared/{index}.rs")).unwrap()),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ProjectExportBuildError::BuildInputCountExceeded
    ));
    assert!(!project.root.join("Cargo.toml").exists());
    assert!(!project.root.join("Cargo.lock").exists());
}

#[test]
fn build_preparation_bounds_total_declared_bytes_and_rejects_later_growth() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let checked = project.check(&root);
    let half_limit = 32 * 1024 * 1024;
    for path in ["Cargo.toml", "Cargo.lock"] {
        fs::File::create(project.root.join(path))
            .unwrap()
            .set_len(half_limit)
            .unwrap();
    }
    let inputs = || ["Cargo.toml", "Cargo.lock"].map(|path| RelativePath::new(path).unwrap());
    let preparation = ProjectExportBuildPreparation::capture(startups(&checked), inputs()).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(project.root.join("Cargo.toml"))
        .unwrap()
        .set_len(half_limit + 1)
        .unwrap();
    assert!(matches!(
        preparation.verify_build_inputs(&root),
        Err(ProjectExportBuildError::BuildInputsChanged)
    ));
    let error = ProjectExportBuildPreparation::capture(startups(&checked), inputs()).unwrap_err();
    assert!(matches!(
        error,
        ProjectExportBuildError::BuildInputByteLimitExceeded
    ));
}

struct SourceProject {
    root: PathBuf,
    temporary_parent: PathBuf,
}

fn startups(deliveries: &[ProjectContentDeliveryReceipt; 2]) -> [&StartupContentClosureReceipt; 2] {
    deliveries
        .each_ref()
        .map(ProjectContentDeliveryReceipt::startup_content)
}

impl SourceProject {
    fn check(&self, root: &DirectoryCapability) -> [ProjectContentDeliveryReceipt; 2] {
        ProjectExportRequest::check_deliveries(root.share(), &ProductRecipe::new()).unwrap()
    }

    // These files and observation facts are synthetic. They test owner matching only; no product
    // process runs here. The compiled CLI trajectory separately proves actual executable startup.
    fn synthetic_build(
        &self,
        startups: [&StartupContentClosureReceipt; 2],
        schema_fingerprint: &str,
    ) -> Result<ProjectExportBuildReceipt, ProjectExportBuildError> {
        self.synthetic_build_with_selection(startups, schema_fingerprint, "release", &["desktop"])
    }

    fn synthetic_build_with_selection(
        &self,
        startups: [&StartupContentClosureReceipt; 2],
        schema_fingerprint: &str,
        profile: &str,
        features: &[&str],
    ) -> Result<ProjectExportBuildReceipt, ProjectExportBuildError> {
        let inputs = self.write_synthetic_build_inputs();
        let preparation = ProjectExportBuildPreparation::capture(startups, inputs)?;
        let (artifacts, agreements) = self.synthetic_artifacts(
            startups,
            schema_fingerprint,
            profile,
            features,
            &preparation,
        );
        preparation.bind(artifacts, agreements)
    }

    fn synthetic_artifacts(
        &self,
        startups: [&StartupContentClosureReceipt; 2],
        schema_fingerprint: &str,
        profile: &str,
        features: &[&str],
        preparation: &ProjectExportBuildPreparation,
    ) -> (
        [ProjectExportCargoArtifact; 2],
        [ProjectExportProductAgreement; 2],
    ) {
        fs::create_dir_all(self.root.join("build-products")).unwrap();
        let root = startups[0].project_root();
        let [(headless, headless_agreement), (desktop, desktop_agreement)] =
            ProjectExportBinaryTarget::ALL.map(|target| {
                let startup = startups[target.index()];
                let path = format!("build-products/{}", target.cargo_target_name());
                fs::write(self.root.join(&path), format!("synthetic {target:?} bytes")).unwrap();
                let file = root.open_file(&RelativePath::new(path).unwrap()).unwrap();
                let digest = file.digest(1024).unwrap();
                let artifact = ProjectExportCargoArtifact::from_file(
                    target,
                    "synthetic-owner-package",
                    "x86_64-unknown-linux-gnu",
                    profile,
                    features.iter().map(|feature| (*feature).to_owned()),
                    file,
                    preparation,
                )
                .unwrap();
                let agreement = ProjectExportProductAgreement::new(
                    target,
                    digest,
                    startup.manifest_digest(),
                    startup.content_digest(),
                    startup.closure_plan_digest(),
                    startup.runtime_schema_bundle_digest().unwrap(),
                    schema_fingerprint,
                    crate::fs::ContentDigest::of_bytes(b"synthetic observation facts"),
                )
                .unwrap();
                (artifact, agreement)
            });
        ([headless, desktop], [headless_agreement, desktop_agreement])
    }

    fn write_synthetic_build_inputs(&self) -> [RelativePath; 5] {
        fs::create_dir_all(self.root.join("src/bin")).unwrap();
        [
            (
                "Cargo.toml",
                "[package]\nname = \"owner-test\"\nversion = \"0.1.0\"\n",
            ),
            ("Cargo.lock", "version = 4\n"),
            ("src/lib.rs", "// Synthetic build input.\n"),
            ("src/bin/headless.rs", "fn main() {}\n"),
            ("src/bin/desktop.rs", "fn main() {}\n"),
        ]
        .map(|(path, bytes)| {
            fs::write(self.root.join(path), bytes).unwrap();
            RelativePath::new(path).unwrap()
        })
    }

    fn new() -> Self {
        let temporary_parent = std::env::temp_dir().canonicalize().unwrap();
        let root = temporary_parent.join(format!(
            "nara_export_sources_{}_{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let project = Self {
            root,
            temporary_parent,
        };
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(project.root.join(directory)).unwrap();
        }
        fs::write(
            project.root.join("nara.toml"),
            r#"schema_version = 1

[project]
source_universe = '5e1088f6-adb1-50fa-ad05-913801be66b0'
native_document_identity = 'project_location'
name = "Export Source Test"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "a.scene.json"

[content]
scenes = ["b.scene.json"]

[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"

[capabilities]
requested = ["runtime-2d"]

[profiles.desktop]
"#,
        )
        .unwrap();
        let scene = SceneDocument::default().to_json_string().unwrap();
        for name in ["a.scene.json", "b.scene.json"] {
            fs::write(project.root.join("scenes").join(name), &scene).unwrap();
        }
        let root = host_fs::readable_directory(&project.root).unwrap();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        project
    }
}

impl Drop for SourceProject {
    fn drop(&mut self) {
        let relative = self.root.strip_prefix(&self.temporary_parent).unwrap();
        assert_eq!(relative.components().count(), 1);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn entries(directory: &Path) -> Vec<std::ffi::OsString> {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
