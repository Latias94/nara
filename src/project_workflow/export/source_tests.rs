use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    ProductRecipe,
    fs::{DirectoryCapability, FsError, RelativeComponent, RelativePath},
    project_host::{StartupContentClosureReceipt, ingest_project_manifest, resolve_product_recipe},
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

nara_reflect::persistent_components! { pub mod source_text_components = [SourceText]; }

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
fn domain_sources_export_exact_members_and_reject_changed_or_missing_dependencies() {
    use nara_reflect::PersistentComponentProvider;
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
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
    fs::write(project.root.join("assets/suffix.txt"), b"original").unwrap();
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
    let check =
        || check_project(ProjectCheckRequest::new(root.share(), source_text_recipe())).unwrap();
    let delivery = check();
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
                delivery.startup_content(),
                &delivery.schema_fingerprint().to_string(),
            )
            .unwrap();
        prepare_project_export(ProjectExportRequest::new(
            host_fs::writable_directory(&project.root).unwrap(),
            RelativeComponent::new(name).unwrap(),
            delivery.clone(),
            build,
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
    let delivery =
        check_project(ProjectCheckRequest::new(root.share(), source_text_recipe())).unwrap();
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
                delivery.startup_content(),
                &delivery.schema_fingerprint().to_string(),
            )
            .unwrap();
        prepare_project_export(ProjectExportRequest::new(
            host_fs::writable_directory(&project.root).unwrap(),
            RelativeComponent::new(destination).unwrap(),
            delivery.clone(),
            build,
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
fn export_revalidation_rejects_a_changed_nonstartup_delivery_scene_before_staging() {
    let project = SourceProject::new();
    let delivery = check_project(ProjectCheckRequest::new(
        host_fs::readable_directory(&project.root).unwrap(),
        ProductRecipe::new(),
    ))
    .unwrap();
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
    revalidate_delivery_sources(&delivery).unwrap();
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

    let error = revalidate_delivery_sources(&delivery).unwrap_err();
    match error {
        ProjectExportPlanError::ContentChanged { path, source } => {
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
fn export_rejects_a_build_agreement_for_a_different_freshly_checked_startup() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let original = project.check(&root);
    let build = project
        .synthetic_build(
            original.startup_content(),
            &original.schema_fingerprint().to_string(),
        )
        .unwrap();
    build.revalidate(original.startup_content()).unwrap();
    let root_entries = entries(&project.root);
    let second_bytes = fs::read(project.root.join("scenes/b.scene.json")).unwrap();
    let mut startup_bytes = fs::read(project.root.join("scenes/a.scene.json")).unwrap();
    startup_bytes.push(b'\n');
    fs::write(project.root.join("scenes/a.scene.json"), startup_bytes).unwrap();

    let current = project.check(&root);
    assert_eq!(original.manifest_digest(), current.manifest_digest());
    assert_eq!(original.schema_fingerprint(), current.schema_fingerprint());
    assert_ne!(
        original.startup_content().content_digest(),
        current.startup_content().content_digest()
    );
    assert_ne!(
        original.startup_content().closure_plan_digest(),
        current.startup_content().closure_plan_digest()
    );
    revalidate_delivery_sources(&current).unwrap();
    let error = prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(&project.root).unwrap(),
        RelativeComponent::new("candidate").unwrap(),
        current,
        build,
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        ProjectExportPlanError::Build(ProjectExportBuildError::ProductAgreementMismatch {
            target: ProjectExportBinaryTarget::Headless
        })
    ));
    assert_eq!(entries(&project.root), root_entries);
    assert_eq!(
        fs::read(project.root.join("scenes/b.scene.json")).unwrap(),
        second_bytes
    );
}

#[test]
fn startup_build_agreement_does_not_require_observing_nonstartup_delivery_content() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let original = project.check(&root);
    let build = project
        .synthetic_build(
            original.startup_content(),
            &original.schema_fingerprint().to_string(),
        )
        .unwrap();
    let root_entries = entries(&project.root);
    let mut second_bytes = fs::read(project.root.join("scenes/b.scene.json")).unwrap();
    second_bytes.push(b'\n');
    fs::write(project.root.join("scenes/b.scene.json"), second_bytes).unwrap();

    let current = project.check(&root);
    assert_ne!(original.delivery_digest(), current.delivery_digest());
    assert_eq!(
        original.startup_content().closure_plan_digest(),
        current.startup_content().closure_plan_digest()
    );
    // Product agreement is intentionally startup-scoped; complete delivery validation remains a
    // separate export gate and must reject this changed non-startup source.
    build.revalidate(current.startup_content()).unwrap();
    revalidate_delivery_sources(&current).unwrap();
    assert!(matches!(
        revalidate_delivery_sources(&original),
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
    assert_ne!(checked.schema_fingerprint().to_string(), wrong_schema);
    let error = project
        .synthetic_build(checked.startup_content(), &wrong_schema)
        .unwrap_err();
    assert!(matches!(
        error,
        ProjectExportBuildError::ProductAgreementMismatch {
            target: ProjectExportBinaryTarget::Headless
        }
    ));
    assert!(!project.root.join("candidate").exists());
}

struct SourceProject {
    root: PathBuf,
    temporary_parent: PathBuf,
}

impl SourceProject {
    fn check(
        &self,
        root: &DirectoryCapability,
    ) -> crate::project_host::ProjectContentDeliveryReceipt {
        check_project(ProjectCheckRequest::new(root.share(), ProductRecipe::new())).unwrap()
    }

    // These files and observation facts are synthetic. They test owner matching only; no product
    // process runs here. The compiled CLI trajectory separately proves actual executable startup.
    fn synthetic_build(
        &self,
        startup: &StartupContentClosureReceipt,
        schema_fingerprint: &str,
    ) -> Result<ProjectExportBuildReceipt, ProjectExportBuildError> {
        fs::create_dir_all(self.root.join("src/bin")).unwrap();
        fs::create_dir_all(self.root.join("build-products")).unwrap();
        for (path, bytes) in [
            (
                "Cargo.toml",
                "[package]\nname = \"owner-test\"\nversion = \"0.1.0\"\n",
            ),
            ("Cargo.lock", "version = 4\n"),
            ("src/lib.rs", "// Synthetic build input.\n"),
            ("src/bin/headless.rs", "fn main() {}\n"),
            ("src/bin/desktop.rs", "fn main() {}\n"),
        ] {
            fs::write(self.root.join(path), bytes).unwrap();
        }
        let preparation = ProjectExportBuildPreparation::capture(startup)?;
        let root = startup.project_root();
        let [(headless, headless_agreement), (desktop, desktop_agreement)] = [
            ProjectExportBinaryTarget::Headless,
            ProjectExportBinaryTarget::Desktop,
        ]
        .map(|target| {
            let path = format!("build-products/{}", target.cargo_target_name());
            fs::write(self.root.join(&path), format!("synthetic {target:?} bytes")).unwrap();
            let file = root.open_file(&RelativePath::new(path).unwrap()).unwrap();
            let digest = file.digest(1024).unwrap();
            let artifact = ProjectExportCargoArtifact::new(
                target,
                "synthetic-owner-package",
                "x86_64-unknown-linux-gnu",
                "release",
                ["desktop".to_owned()],
                file,
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
        preparation.bind([headless, desktop], [headless_agreement, desktop_agreement])
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
schema_bundle = "schema/authoring.bundle.json"

[capabilities]
requested = ["runtime-2d"]
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
