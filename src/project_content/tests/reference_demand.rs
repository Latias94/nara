use nara_asset::{
    AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource, AssetReferenceDemand,
    SourceAssetImporter,
};
use nara_reflect::{ComponentValueShape, encode_asset_reference_value};

use super::*;

const IMMEDIATE: &str = "test.content.Immediate";
const DEFERRED: &str = "test.content.Deferred";
const B: &str = "137317af-05cf-42cc-9192-b5d66e25aa80";
const C: &str = "f236538a-3487-4075-abf2-1007ce72c8fa";
const D: &str = "56cc452c-0cd6-4b86-b906-37b6e69152a0";

fn demand_project() -> LazyContentProject {
    let project = LazyContentProject::new();
    // The incumbent lazy-Prefab fixture deliberately poisons an unrelated identity sidecar.
    // These Product lookups intentionally exercise the real complete identity catalogue.
    fs::remove_file(project.root.join("assets/unused.png.meta")).unwrap();
    let manifest = project.root.join("nara.toml");
    let source = fs::read_to_string(&manifest).unwrap();
    fs::write(
        manifest,
        format!("{source}\n[authoring]\nschema_bundle = \"schema.bundle.json\"\n"),
    )
    .unwrap();
    project
}

fn demand_inputs(
    project: &LazyContentProject,
    recipe: ProductRecipe,
) -> (ProjectSettingsCandidate, RuntimePlan, ProjectContentLoader) {
    let (candidate, plan, loader) = project.inputs_with_recipe(recipe);
    let bundle_path = candidate
        .settings()
        .authoring_schema_bundle
        .as_ref()
        .unwrap();
    fs::write(
        project.root.join(bundle_path.as_str()),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    (candidate, plan, loader)
}

fn reference_owner() -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("test.project-content.demand").unwrap(),
        ComponentSchemaCatalog {
            components: [
                (IMMEDIATE, AssetReferenceDemand::Immediate),
                (DEFERRED, AssetReferenceDemand::Deferred),
            ]
            .into_iter()
            .map(|(name, demand)| {
                ComponentSchema::new(
                    ComponentTypeId::new(name),
                    name,
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("target"),
                    "Target",
                    ComponentFieldPath::empty(),
                    ComponentValueShape::asset_ref(demand),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_capability(ComponentCapability::AssetRef)])
            })
            .collect(),
            ..Default::default()
        },
        None,
    )
    .unwrap()
}

fn product(stable: &str) -> AssetProductRef {
    AssetProductRef::new(stable.parse().unwrap(), ImportedProductId::PRIMARY).unwrap()
}

fn document(
    owner: &ComponentSchemaOwnerSnapshot,
    references: &[(&str, AssetRef)],
) -> SceneDocument {
    SceneDocument::from_ordered_roots(references.iter().enumerate().map(
        |(index, (component, reference))| {
            SceneEntityRecord::new(SceneEntityId::new(format!("target-{index}")).unwrap())
                .with_component(
                    ComponentTypeId::new(*component),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        encode_asset_reference_value(reference),
                    )
                    .with_owner(owner.owner_id().clone()),
                )
        },
    ))
}

fn write_scene(project: &LazyContentProject, path: &str, stable: &str, scene: &SceneDocument) {
    fs::write(project.root.join(path), scene.to_json_string().unwrap()).unwrap();
    let (root, alias) = path.split_once('/').unwrap();
    fs::write(
        project.root.join(format!("{path}.meta")),
        AssetMeta::new(
            AssetMetaIdentity::new(
                stable.parse().unwrap(),
                AssetPath::new(alias).unwrap(),
                AssetSourceKind::Scene,
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(root, "scenes" | "assets"));
}

fn write_image(project: &LazyContentProject, name: &str, stable: &str) {
    fs::write(
        project.root.join(format!("assets/{name}")),
        include_bytes!("../../../tests/fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();
    fs::write(
        project.root.join(format!("assets/{name}.meta")),
        AssetMeta::new(
            AssetMetaIdentity::new(
                stable.parse().unwrap(),
                AssetPath::new(name).unwrap(),
                AssetSourceKind::Image,
            ),
            nara_image::ImageImportSettings::default(),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn current_source_prepares_only_immediate_data_and_strict_delivery_covers_future_scenes() {
    let project = demand_project();
    let owner = reference_owner();
    let a = document(
        &owner,
        &[
            (DEFERRED, AssetRef::Product(product(B))),
            (IMMEDIATE, AssetRef::Product(product(D))),
            (IMMEDIATE, AssetRef::path("shared.png").unwrap()),
        ],
    );
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        a.to_json_string().unwrap(),
    )
    .unwrap();
    write_scene(
        &project,
        "scenes/b.scene.json",
        B,
        &document(
            &owner,
            &[
                (DEFERRED, AssetRef::Product(product(C))),
                (IMMEDIATE, AssetRef::path("heavy.png").unwrap()),
                (IMMEDIATE, AssetRef::path("shared.png").unwrap()),
            ],
        ),
    );
    // C is dependency-only and lives under Assets, so delivery must follow its actual reader
    // binding rather than treating every Scene dependency as a public travel-root alias.
    write_scene(
        &project,
        "assets/c.scene.json",
        C,
        &document(&owner, &[(DEFERRED, AssetRef::Product(product(B)))]),
    );
    write_scene(
        &project,
        "scenes/d.scene.json",
        D,
        &SceneDocument::default(),
    );
    write_image(
        &project,
        "shared.png",
        "82eb8703-ec01-472d-9416-d162661d96df",
    );
    write_image(
        &project,
        "heavy.png",
        "20d8a107-1901-412b-95c7-5ec35531bb4a",
    );
    let (candidate, plan, loader) = demand_inputs(&project, data_recipe(&owner));
    assert!(candidate.settings().content.scenes.is_empty());

    let agreement =
        prepare_runtime_schema_agreement(&loader.project_root(), &candidate, &plan).unwrap();
    let current = loader
        .load_with_runtime_agreement(&candidate, &plan, agreement)
        .unwrap();
    assert_eq!(current.images().len(), 1);
    assert_eq!(current.images()[0].path().as_str(), "assets/shared.png");
    assert_eq!(current.scene_dependencies().len(), 1);
    assert_eq!(
        current.scene_dependencies()[0].path().as_str(),
        "scenes/d.scene.json"
    );
    assert!(
        current
            .asset_resolution()
            .resolve_ref(&AssetRef::Product(product(B)))
            .is_ok(),
        "Deferred identity remains available to the actual component/materializer",
    );
    let ordinary = current.content_closure();
    assert!(ordinary.members().iter().any(|member| {
        member.path().as_str() == "scenes/b.scene.json.meta"
            && member.kind() == ProjectContentMemberKind::AssetMetadata
    }));
    assert!(!ordinary.members().iter().any(|member| {
        matches!(
            member.path().as_str(),
            "scenes/b.scene.json" | "assets/c.scene.json" | "assets/heavy.png"
        )
    }));
    let ordinary_digest = current.content_digest();
    drop(current);

    let delivery = loader.load_delivery(&candidate, &plan).unwrap();
    assert_eq!(delivery.startup_content().content_digest(), ordinary_digest);
    assert_eq!(delivery.startup_content().members(), ordinary.members());
    assert_eq!(
        delivery.scene_roots().len(),
        1,
        "data membership grants no travel root"
    );
    for path in [
        "scenes/b.scene.json",
        "assets/c.scene.json",
        "scenes/d.scene.json",
        "assets/shared.png",
        "assets/heavy.png",
    ] {
        assert!(
            delivery
                .members()
                .iter()
                .any(|member| member.path().as_str() == path),
            "{path}"
        );
    }
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
    drop((ordinary, delivery));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn editor_immediate_retargeting_requires_actual_prepared_native_content() {
    for (path, alias, kind) in [
        (
            "scenes/b.scene.json",
            "b.scene.json",
            AssetSourceKind::Scene,
        ),
        ("assets/b.png", "b.png", AssetSourceKind::Image),
        (
            "assets/b.tileset.ron",
            "b.tileset.ron",
            AssetSourceKind::Other("tileset".into()),
        ),
    ] {
        let project = demand_project();
        let owner = reference_owner();
        let original = document(
            &owner,
            &[
                (DEFERRED, AssetRef::Product(product(B))),
                (IMMEDIATE, AssetRef::Product(product(D))),
            ],
        );
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            original.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join(path),
            b"readable future target with an invalid payload",
        )
        .unwrap();
        fs::write(
            project.root.join(format!("{path}.meta")),
            AssetMeta::new(
                AssetMetaIdentity::new(product(B).source(), AssetPath::new(alias).unwrap(), kind),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        write_scene(
            &project,
            "scenes/d.scene.json",
            D,
            &SceneDocument::default(),
        );
        let (candidate, plan, loader) = demand_inputs(&project, data_recipe(&owner));
        let snapshot = loader.load(&candidate, &plan).unwrap();
        let bindings = plan.schema_validation().bindings();
        let original_source = snapshot
            .prepare_editor_startup_scene(snapshot.selected_scene_source_key(), &original, bindings)
            .unwrap();
        drop(original_source);
        let revision = snapshot.revision();
        let digest = snapshot.content_digest();
        let rewritten = document(
            &owner,
            &[
                (DEFERRED, AssetRef::Product(product(B))),
                (IMMEDIATE, AssetRef::Product(product(B))),
            ],
        );
        let result = snapshot.prepare_editor_startup_scene(
            snapshot.selected_scene_source_key(),
            &rewritten,
            bindings,
        );
        let failure = result
            .err()
            .expect("resolved identity cannot replace missing Immediate data");
        assert!(
            failure.diagnostics().iter().any(|diagnostic| {
                diagnostic.code().as_str() == "project.content.asset-not-prepared"
            }),
            "{failure:?}"
        );
        assert_eq!(snapshot.revision(), revision);
        assert_eq!(snapshot.content_digest(), digest);
        assert_eq!(snapshot.scene_dependencies().len(), 1);
        assert!(snapshot.images().is_empty());
        assert!(snapshot.tilesets().is_empty());
        snapshot
            .prepare_editor_startup_scene(snapshot.selected_scene_source_key(), &original, bindings)
            .unwrap();
        drop(snapshot);
        assert_eq!(loader.budget_snapshot().active_generations(), 0);
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    }
}

#[test]
fn deferred_body_changes_do_not_claim_current_payload_validity_or_change_the_receipt() {
    let project = demand_project();
    let owner = reference_owner();
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        document(&owner, &[(DEFERRED, AssetRef::Product(product(B)))])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    write_scene(
        &project,
        "scenes/b.scene.json",
        B,
        &SceneDocument::default(),
    );
    let (candidate, plan, loader) = demand_inputs(&project, data_recipe(&owner));
    let current = loader.load(&candidate, &plan).unwrap();
    let original = current.content_digest();
    drop(current);
    fs::write(
        project.root.join("scenes/b.scene.json"),
        b"malformed future Scene",
    )
    .unwrap();
    let current = loader.load(&candidate, &plan).unwrap();
    assert_eq!(current.content_digest(), original);
    assert!(current.scene_dependencies().is_empty());
    let selected_path = nara_project::ProjectPath::new("b.scene.json").unwrap();
    assert_eq!(
        loader
            .load_scene(&candidate, &plan, &selected_path)
            .unwrap_err()
            .kind(),
        ProjectContentErrorKind::SceneFormat,
    );
    assert_eq!(
        loader.load_delivery(&candidate, &plan).unwrap_err().kind(),
        ProjectContentErrorKind::SceneFormat,
    );
    assert_eq!(
        current.content_digest(),
        original,
        "failed future preparation preserves A"
    );
    drop(current);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn a_present_deferred_reference_still_requires_a_readable_source_and_exact_metadata() {
    let project = demand_project();
    let owner = reference_owner();
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        document(&owner, &[(DEFERRED, AssetRef::Product(product(B)))])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    write_scene(
        &project,
        "scenes/b.scene.json",
        B,
        &SceneDocument::default(),
    );
    let (candidate, plan, loader) = demand_inputs(&project, data_recipe(&owner));
    let frozen = loader.load(&candidate, &plan).unwrap();
    let frozen_digest = frozen.content_digest();
    let frozen_plan = frozen.content_closure().closure_plan_digest();
    fs::remove_file(project.root.join("scenes/b.scene.json")).unwrap();
    assert_eq!(frozen.content_digest(), frozen_digest);
    assert_eq!(frozen.content_closure().closure_plan_digest(), frozen_plan);
    assert!(
        frozen
            .asset_resolution()
            .resolve_ref(&AssetRef::Product(product(B)))
            .is_ok()
    );
    assert!(
        loader.load(&candidate, &plan).is_err(),
        "metadata cannot prove source presence"
    );
    // The previous capture remains usable as frozen data; only a new request claims that the
    // target can be resolved now. No filesystem watch or body digest participates in that data.
    drop(frozen);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    fs::write(project.root.join("scenes/b.scene.json"), []).unwrap();
    // An empty target is readable; semantic decoding still belongs to strict validation/request.
    let current = loader.load(&candidate, &plan).unwrap();
    assert_eq!(
        loader.load_delivery(&candidate, &plan).unwrap_err().kind(),
        ProjectContentErrorKind::SceneFormat,
    );
    drop(current);
    fs::remove_file(project.root.join("scenes/b.scene.json.meta")).unwrap();
    assert!(loader.load(&candidate, &plan).is_err());
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

struct PrimaryOnlyDomain;

static PRIMARY_ONLY_IMPORTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

impl AssetDomain for PrimaryOnlyDomain {
    type Product = ();
}

impl SourceAssetImporter for PrimaryOnlyDomain {
    type Settings = serde_json::Value;
    const SOURCE_KIND: &'static str = "test.demand-primary-only";

    fn limits() -> AssetImportLimits {
        let limit = ByteLimit::new(1024).unwrap();
        AssetImportLimits::new(limit, limit, limit)
    }

    fn prepare<E>(_: &mut AssetImportSource<'_, E>) -> Result<(), AssetImportError<E>> {
        PRIMARY_ONLY_IMPORTS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn deferred_compound_selection_is_resolved_without_metadata_inventing_membership() {
    let project = demand_project();
    let owner = reference_owner();
    let auxiliary =
        ImportedProductId::parse_subproduct("dd71a12b-cef1-4f19-94a3-0bac74e51973").unwrap();
    let reference = AssetRef::Product(AssetProductRef::new(B.parse().unwrap(), auxiliary).unwrap());
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        document(&owner, &[(DEFERRED, reference.clone())])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(project.root.join("assets/future.bundle"), b"actual source").unwrap();
    fs::write(
        project.root.join("assets/future.bundle.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                B.parse().unwrap(),
                AssetPath::new("future.bundle").unwrap(),
                AssetSourceKind::Other(PrimaryOnlyDomain::SOURCE_KIND.to_owned()),
            ),
            // Importer-private metadata is not a Host membership proof.
            serde_json::json!({ "claimed_product": auxiliary.to_string() }),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let recipe = data_recipe(&owner)
        .add_asset_domain::<PrimaryOnlyDomain>()
        .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, recipe);
    assert_eq!(PRIMARY_ONLY_IMPORTS.load(Ordering::SeqCst), 0);
    let current = loader.load(&candidate, &plan).unwrap();
    assert!(current.asset_resolution().resolve_ref(&reference).is_ok());
    assert!(
        current
            .domain_assets()
            .collection::<PrimaryOnlyDomain>()
            .is_none()
    );
    assert_eq!(PRIMARY_ONLY_IMPORTS.load(Ordering::SeqCst), 0);
    let failure = loader.load_delivery(&candidate, &plan).unwrap_err();
    assert!(
        failure.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "project.content.asset-product-missing"
        }),
        "{failure:?}",
    );
    assert_eq!(PRIMARY_ONLY_IMPORTS.load(Ordering::SeqCst), 1);
    drop(current);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn immediate_occurrence_wins_without_duplicate_source_preparation() {
    let project = demand_project();
    let owner = reference_owner();
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        document(
            &owner,
            &[
                (DEFERRED, AssetRef::Product(product(B))),
                (IMMEDIATE, AssetRef::Product(product(B))),
            ],
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    write_scene(
        &project,
        "scenes/b.scene.json",
        B,
        &SceneDocument::default(),
    );
    let (candidate, plan, loader) = demand_inputs(&project, data_recipe(&owner));
    let current = loader.load(&candidate, &plan).unwrap();
    assert_eq!(current.scene_dependencies().len(), 1);
    assert_eq!(
        current.scene_dependencies()[0].path().as_str(),
        "scenes/b.scene.json"
    );
    assert_eq!(
        current
            .content_closure()
            .members()
            .iter()
            .filter(|member| { member.path().as_str() == "scenes/b.scene.json" })
            .count(),
        1
    );
    drop(current);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}
