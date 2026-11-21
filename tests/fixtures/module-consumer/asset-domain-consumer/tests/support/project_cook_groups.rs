use std::{fs, path::Path};

use nara::asset_build::VerifiedArtifactGroup;
use nara_asset::{
    AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource, AssetRuntimeError,
    AssetRuntimeLimits, RuntimeAssetCodec,
};
use nara_asset_domain_consumer::{TextBundleDomain, TextBundleSettings};
use nara_core::ByteLimit;
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use serde::{Deserialize, Serialize};

use super::{
    ArtifactBuildDefinition, ArtifactBuildObservation, ArtifactBuildStatus, ArtifactBuildStore,
    ArtifactContentDigest, ArtifactSourceRequest, AssetPath, AssetPathScope, AssetProductRef,
    AssetRef, AssetReferenceDemand, AssetSourceKind, AssetSourcePath, BADGE, BADGE_ID, BadgeArt,
    BadgeDomain, BadgeReference, CookedContentError, DirectoryCapability, EXTRA_BADGE,
    EXTRA_BADGE_ID, FormatVersion, ImageDelivery, ImageDomain, ImageRuntimeCodec,
    ImportedProductId, LATER_BADGE, LATER_BADGE_ID, LATER_SCENE, LATER_SCENE_ID, PNG_AFTER,
    PNG_BEFORE, ProductRecipe, ProjectCheckRequest, ProjectContentBudgetKind, ProjectCookError,
    RelativePath, RuntimeCodecDefinitionId, RuntimeContentBase, RuntimeContentCatalogError,
    RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentError, SCENE, SCENE_ID,
    SceneDocument, Scratch, SourceAssetBuild, SourceAssetImporter, assert_raw_observation,
    badge_cook_request, build_request, build_source, check_project, implementation_digest,
    open_pack, open_store, prepare_project_cooked_content, product, recipe, reopen_pack, required,
    write_badge_source, write_image_meta, write_meta, write_pack, write_project,
};

const TEXTURE: &str = "build-only-texture.png";
const TEXTURE_ID: &str = "25940000-0000-4000-8000-000000000001";

#[test]
fn project_cook_admits_groups_first_reached_through_built_products() {
    for demand in [
        AssetReferenceDemand::Immediate,
        AssetReferenceDemand::Deferred,
    ] {
        exercise_groups(demand);
    }
}

#[allow(
    clippy::result_large_err,
    reason = "The public Cook contribution returns ProjectCookError."
)]
fn exercise_groups(demand: AssetReferenceDemand) {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let assets = project.join("assets");
    let recipe = recipe();
    let a = product(BADGE_ID);
    let b = product(LATER_BADGE_ID);
    let image = product(TEXTURE_ID);
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::StockAtBuild(Box::new([(b, demand)])),
        None,
        BadgeReference::Path,
        &recipe,
    );
    let image_delivery =
        ImageDelivery::StockAtBuild(Box::new([(image, AssetReferenceDemand::Immediate)]));
    write_badge_source(
        &assets,
        LATER_BADGE,
        LATER_BADGE_ID,
        image,
        image_delivery.clone(),
    );
    let texture = assets.join(TEXTURE);
    fs::write(&texture, PNG_BEFORE).unwrap();
    write_image_meta(&assets, TEXTURE, TEXTURE_ID);

    let b_path = format!("assets/{LATER_BADGE}");
    let b_metadata = format!("{b_path}.meta");
    let image_path = format!("assets/{TEXTURE}");
    let image_metadata = format!("{image_path}.meta");
    let checked = check_project(ProjectCheckRequest::new(root.share(), recipe.clone())).unwrap();
    for path in [&b_path, &b_metadata, &image_path, &image_metadata] {
        assert!(
            checked
                .members()
                .iter()
                .all(|member| member.path().as_str() != path.as_str())
        );
    }

    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let build = |path| {
        let request = build_request(&root, path, implementation);
        let built = build_source::<BadgeDomain>(&store, &request, |_| Ok(())).unwrap();
        assert_eq!(built.status(), ArtifactBuildStatus::Built);
        let hit = build_source::<BadgeDomain>(&store, &request, |_| Ok(())).unwrap();
        assert_eq!(hit.status(), ArtifactBuildStatus::Hit);
        assert_eq!(hit.group().digest(), built.group().digest());
        hit.into_group()
    };
    let a_group = build(BADGE);
    let b_group = build(LATER_BADGE);
    assert_eq!(a_group.members()[0].dependencies, [(b, demand)]);
    assert_eq!(
        b_group.members()[0].dependencies,
        [(image, AssetReferenceDemand::Immediate)]
    );
    assert!(b_group.observations().iter().all(|observation| {
        !matches!(observation, ArtifactBuildObservation::Raw { path, .. }
            if path.as_str() == image_path.as_str())
    }));

    let cook = |recipe: &ProductRecipe, groups: &[&VerifiedArtifactGroup]| {
        prepare_project_cooked_content(
            badge_cook_request(&root, &candidate, recipe, &store, implementation),
            |context| {
                let snapshot = context.snapshot();
                assert!(
                    snapshot
                        .asset_resolution()
                        .resolve_ref(&AssetRef::Product(b))
                        .is_err()
                );
                assert!(snapshot.images().is_empty());
                for group in groups {
                    context.add_group::<BadgeDomain>(group)?;
                }
                Ok(())
            },
        )
    };
    let (content, selection) = cook(&recipe, &[&a_group, &b_group]).unwrap().into_parts();
    selection.verify(&project).unwrap();
    for path in [&b_path, &b_metadata, &image_path, &image_metadata] {
        assert_raw_observation(
            selection.observations(),
            path,
            &fs::read(project.join(path)).unwrap(),
        );
    }
    let first = scratch.0.join("a-then-b.pack");
    let expected = write_pack(content, &first);
    let second = scratch.0.join("b-then-a.pack");
    let (content, reverse_selection) = cook(&recipe, &[&b_group, &a_group]).unwrap().into_parts();
    reverse_selection.verify(&project).unwrap();
    let reverse_expected = write_pack(content, &second);
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    assert_decoded(
        &open_pack(&first, &expected, &recipe),
        demand,
        [24, 120, 220, 255],
    );
    assert_decoded(
        &open_pack(&second, &reverse_expected, &recipe),
        demand,
        [24, 120, 220, 255],
    );

    if demand == AssetReferenceDemand::Immediate {
        let missing = cook(&recipe, &[&a_group])
            .err()
            .expect("the B Product is required");
        assert!(
            matches!(
                &missing,
                ProjectCookError::Content(CookedContentError::Runtime(RuntimeContentError::Catalog(
                    RuntimeContentCatalogError::MissingDependencyProduct { product, .. }
                ))) if *product == b
            ),
            "{missing:?}"
        );

        let b_file = project.join(&b_path);
        let original = fs::read(&b_file).unwrap();
        let mut changed = original.clone();
        changed.push(b'\n');
        fs::write(&b_file, changed).unwrap();
        assert!(selection.verify(&project).is_err());
        let stale = cook(&recipe, &[&b_group, &a_group])
            .err()
            .expect("stale group was used");
        assert!(
            matches!(&stale, ProjectCookError::Invalid(message)
                if message.contains("does not match the selected source build")
                    && message.contains(b_path.as_str())),
            "{stale:?}"
        );
        fs::write(&b_file, original).unwrap();

        let duplicate = project.join("prefabs");
        write_badge_source(
            &duplicate,
            LATER_BADGE,
            LATER_BADGE_ID,
            image,
            image_delivery,
        );
        assert_identity_conflict(selection.verify(&project));
        assert_identity_conflict(cook(&recipe, &[&a_group, &b_group]));
        fs::remove_file(duplicate.join(format!("{LATER_BADGE}.meta"))).unwrap();

        let count = selection
            .observations()
            .iter()
            .filter(|observation| {
                matches!(
                    observation,
                    ArtifactBuildObservation::Raw {
                        digest: Some(_),
                        ..
                    }
                )
            })
            .count();
        let limited = |count| {
            recipe.clone().with_source_content_limits(
                recipe
                    .source_content_limits()
                    .with_limit(ProjectContentBudgetKind::Files, count)
                    .unwrap(),
            )
        };
        cook(&limited(count), &[&a_group, &b_group]).unwrap();
        let over_limit = cook(&limited(count - 1), &[&a_group, &b_group]);
        assert!(matches!(&over_limit, Err(ProjectCookError::Source(source))
            if source.diagnostics().iter().any(|diagnostic|
                diagnostic.code().as_str() == "project.delivery.member-limit")));

        write_badge_source(
            &assets,
            EXTRA_BADGE,
            EXTRA_BADGE_ID,
            image,
            ImageDelivery::Embedded,
        );
        let unused = build(EXTRA_BADGE);
        let extra = cook(&recipe, &[&unused, &a_group, &b_group])
            .err()
            .expect("unused group was used");
        assert!(
            matches!(&extra, ProjectCookError::Invalid(message)
                if message.contains("supplied artifact group is unreachable")
                    && message.contains(EXTRA_BADGE_ID)),
            "{extra:?}"
        );
    }

    selection.verify(&project).unwrap();
    fs::write(texture, PNG_AFTER).unwrap();
    assert!(selection.verify(&project).is_err());
    let (changed, current) = cook(&recipe, &[&b_group, &a_group]).unwrap().into_parts();
    current.verify(&project).unwrap();
    assert_raw_observation(current.observations(), &image_path, PNG_AFTER);
    let updated = scratch.0.join("updated-image.pack");
    let updated_expected = write_pack(changed, &updated);
    assert_decoded(
        &open_pack(&updated, &updated_expected, &recipe),
        demand,
        [232, 64, 24, 255],
    );
}

fn assert_identity_conflict<T>(result: Result<T, ProjectCookError>) {
    let error = result
        .err()
        .expect("a second source claims the late group's UUID");
    assert!(
        matches!(&error, ProjectCookError::Source(source) if source.diagnostics().iter()
            .any(|entry| entry.code().as_str() == "project.content.asset-identity-conflict")),
        "{error:?}"
    );
}

fn assert_decoded(base: &RuntimeContentBase, demand: AssetReferenceDemand, pixels: [u8; 4]) {
    let a = product(BADGE_ID);
    let b = product(LATER_BADGE_ID);
    let image = product(TEXTURE_ID);
    let role = match demand {
        AssetReferenceDemand::Immediate => RuntimeContentDependencyRole::Required,
        AssetReferenceDemand::Deferred => RuntimeContentDependencyRole::Reference,
    };
    let catalog = base.catalog();
    assert_eq!(catalog.members().len(), 4);
    assert_eq!(
        catalog
            .member_for_product(product(SCENE_ID))
            .unwrap()
            .dependencies(),
        [required(a)]
    );
    assert_eq!(
        catalog.member_for_product(a).unwrap().dependencies(),
        [RuntimeContentDependency::new(b, role)]
    );
    assert_eq!(
        catalog.member_for_product(b).unwrap().dependencies(),
        [required(image)]
    );
    let report = base.validate_deep().unwrap();
    assert_eq!((report.members(), report.products()), (4, 4));
    for (id, target, demand) in [(a, b, demand), (b, image, AssetReferenceDemand::Immediate)] {
        let acquired = base.acquire_asset::<BadgeDomain, BadgeDomain>(id).unwrap();
        assert_eq!(acquired.product().product, id);
        assert!(matches!(&acquired.product().art, BadgeArt::Stock(edges)
            if edges.as_ref() == [(target, demand)]));
    }
    let decoded = base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image)
        .unwrap();
    assert!(decoded.product().source().is_none());
    assert_eq!(decoded.product().pixels(), pixels);
}

#[test]
fn project_cook_admits_a_registered_cross_domain_subproduct_across_scene_callbacks() {
    exercise_cross_domain_group(AssetReferenceDemand::Immediate);
    exercise_cross_domain_group(AssetReferenceDemand::Deferred);
}

#[allow(
    clippy::result_large_err,
    reason = "The public Cook contribution returns ProjectCookError."
)]
fn exercise_cross_domain_group(demand: AssetReferenceDemand) {
    const BUNDLE: &str = "linked.bundle.json";
    const BUNDLE_ID: &str = "25940000-0000-4000-8000-000000000003";
    const BUNDLE_BODY: &[u8] =
        br#"{"suffix":"bundle-suffix.txt","entries":[{"name":"subtitle","text":"Hello"}]}"#;
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let assets = project.join("assets");
    let a = product(BADGE_ID);
    let b = product(BUNDLE_ID);
    let sub = ImportedProductId::parse_subproduct("25940000-0000-4000-8000-000000000002").unwrap();
    let auxiliary = AssetProductRef::new(b.source(), sub).unwrap();
    let without_source_domain = build_link_recipe()
        .add_runtime_asset_codec::<TextBundleDomain, TextBundleDomain>(
            RuntimeCodecDefinitionId::new("consumer.cook-built-link.bundle", 1),
        )
        .unwrap();
    let recipe = without_source_domain
        .clone()
        .add_asset_domain::<TextBundleDomain>()
        .unwrap();
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::Embedded,
        Some(ImageDelivery::Embedded),
        BadgeReference::Path,
        &recipe,
    );
    fs::write(
        project.join("scenes").join(LATER_SCENE),
        SceneDocument::from_ordered_roots([])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    write_build_link(&assets, (BADGE, BADGE_ID), auxiliary, demand, None);
    fs::write(assets.join(BUNDLE), BUNDLE_BODY).unwrap();
    fs::write(assets.join("bundle-suffix.txt"), " from B").unwrap();
    write_meta(
        &assets,
        BUNDLE,
        BUNDLE_ID,
        AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
        TextBundleSettings::initial(BUNDLE_BODY, 64, [sub]).unwrap(),
    );
    check_project(ProjectCheckRequest::new(root.share(), recipe.clone())).unwrap();

    let entries: usize = ["assets", "scenes", "prefabs"]
        .into_iter()
        .map(|directory| fs::read_dir(project.join(directory)).unwrap().count())
        .sum();
    let limits = recipe
        .source_content_limits()
        .with_limit(ProjectContentBudgetKind::DirectoryEntries, 4 * entries)
        .unwrap();
    let recipe = recipe.with_source_content_limits(limits);
    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let link_group = build_group::<BuildLinkDomain>(&root, &store, BADGE, implementation);
    let bundle_group = build_group::<TextBundleDomain>(&root, &store, BUNDLE, implementation);
    assert_eq!(link_group.members()[0].dependencies, [(auxiliary, demand)]);
    assert_eq!(bundle_group.members()[0].products, [b, auxiliary]);

    let cook = |recipe: &ProductRecipe, bundle_first| {
        let mut visited = 0;
        let result = prepare_project_cooked_content(
            badge_cook_request(&root, &candidate, recipe, &store, implementation),
            |context| {
                let snapshot = context.snapshot();
                let startup = snapshot.selected_scene_path().as_str() == SCENE;
                assert_eq!(
                    snapshot
                        .domain_assets()
                        .collection::<BuildLinkDomain>()
                        .is_some(),
                    startup
                );
                assert!(
                    snapshot
                        .domain_assets()
                        .collection::<TextBundleDomain>()
                        .is_none()
                );
                assert!(
                    snapshot
                        .asset_resolution()
                        .resolve_ref(&AssetRef::Product(auxiliary))
                        .is_err()
                );
                visited += 1;
                if bundle_first {
                    context.add_group::<TextBundleDomain>(&bundle_group)?;
                    context.add_group::<BuildLinkDomain>(&link_group)?;
                } else {
                    context.add_group::<BuildLinkDomain>(&link_group)?;
                    context.add_group::<TextBundleDomain>(&bundle_group)?;
                }
                Ok(())
            },
        );
        if result.is_ok() {
            assert_eq!(visited, 2);
        }
        result
    };
    let unregistered = cook(&without_source_domain, true)
        .err()
        .expect("a Runtime codec alone cannot admit a source domain");
    assert!(matches!(&unregistered, ProjectCookError::Invalid(message)
        if message.contains("artifact group has no selected source")
            && message.contains(TextBundleDomain::SOURCE_KIND)));

    let first = scratch.0.join("link-first.pack");
    let (content, selection) = cook(&recipe, false).unwrap().into_parts();
    selection.verify(&project).unwrap();
    let expectation = write_pack(content, &first);
    let second = scratch.0.join("bundle-first.pack");
    let (content, reverse_selection) = cook(&recipe, true).unwrap().into_parts();
    reverse_selection.verify(&project).unwrap();
    write_pack(content, &second);
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());

    drop((selection, reverse_selection, store, root));
    let pack = scratch.relocate_pack(&project, &first);
    let base = open_pack(&pack, &expectation, &recipe);
    let report = base.validate_deep().unwrap();
    assert_eq!((report.members(), report.products()), (4, 5));
    let role = match demand {
        AssetReferenceDemand::Immediate => RuntimeContentDependencyRole::Required,
        AssetReferenceDemand::Deferred => RuntimeContentDependencyRole::Reference,
    };
    let catalog = base.catalog();
    assert_eq!(
        catalog.member_for_product(a).unwrap().dependencies(),
        [RuntimeContentDependency::new(auxiliary, role)]
    );
    assert_eq!(
        catalog.member_for_product(auxiliary).unwrap().products(),
        [b, auxiliary]
    );
    let bundle = base
        .acquire_asset::<TextBundleDomain, TextBundleDomain>(auxiliary)
        .unwrap();
    let subtitle = bundle.product().get(&auxiliary).unwrap();
    assert_eq!(subtitle.name, "subtitle");
    assert_eq!(subtitle.text, "Hello from B");
}

#[test]
fn project_cook_replays_a_scene_demanded_by_a_late_group_at_the_file_limit() {
    exercise_late_scene_replay(BadgeReference::Path);
}

#[test]
fn project_cook_replays_a_scene_after_uuid_discovery_at_the_directory_limit() {
    exercise_late_scene_replay(BadgeReference::Product);
}

#[allow(
    clippy::result_large_err,
    reason = "The public Cook contribution returns ProjectCookError."
)]
fn exercise_late_scene_replay(reference: BadgeReference) {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let assets = project.join("assets");
    let recipe = build_link_recipe();
    let (root, candidate) =
        write_project(&project, ImageDelivery::Embedded, None, reference, &recipe);
    let indexed = matches!(reference, BadgeReference::Product);
    if indexed {
        for index in 0..256 {
            fs::write(assets.join(format!("unreferenced-{index:03}.txt")), "").unwrap();
        }
    }
    let a = product(BADGE_ID);
    let b = product(LATER_BADGE_ID);
    let scene = product(LATER_SCENE_ID);
    write_build_link(
        &assets,
        (BADGE, BADGE_ID),
        b,
        AssetReferenceDemand::Immediate,
        Some(LATER_SCENE),
    );
    write_build_link(
        &assets,
        (LATER_BADGE, LATER_BADGE_ID),
        scene,
        AssetReferenceDemand::Deferred,
        None,
    );
    fs::write(
        assets.join(LATER_SCENE),
        SceneDocument::from_ordered_roots([])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    write_meta(
        &assets,
        LATER_SCENE,
        LATER_SCENE_ID,
        AssetSourceKind::Scene,
        (),
    );
    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let a_group = build_group::<BuildLinkDomain>(&root, &store, BADGE, implementation);
    let b_group = build_group::<BuildLinkDomain>(&root, &store, LATER_BADGE, implementation);
    let cook = |recipe: &ProductRecipe| {
        let mut visits = 0;
        let prepared = prepare_project_cooked_content(
            badge_cook_request(&root, &candidate, recipe, &store, implementation),
            |context| {
                let snapshot = context.snapshot();
                let [available] = snapshot.scene_dependencies() else {
                    panic!("the source-only dependency must make the Scene available");
                };
                assert_eq!(available.path().as_str(), format!("assets/{LATER_SCENE}"));
                assert!(
                    snapshot
                        .asset_resolution()
                        .resolve_ref(&AssetRef::Product(b))
                        .is_err()
                );
                visits += 1;
                context.add_group::<BuildLinkDomain>(&b_group)?;
                context.add_group::<BuildLinkDomain>(&a_group)
            },
        )
        .unwrap();
        assert_eq!(visits, 1);
        prepared.into_parts()
    };
    let delivered = {
        let (_, selection) = cook(&recipe);
        selection.delivery_receipt().members().len()
    };
    let (kind, limit) = if indexed {
        (ProjectContentBudgetKind::DirectoryEntries, 800)
    } else {
        (ProjectContentBudgetKind::Files, delivered + 2)
    };
    let recipe = recipe.clone().with_source_content_limits(
        recipe
            .source_content_limits()
            .with_limit(kind, limit)
            .unwrap()
            .with_limit(ProjectContentBudgetKind::DirectoryDepth, 3)
            .unwrap(),
    );
    let (content, selection) = cook(&recipe);
    assert_eq!(selection.delivery_receipt().members().len(), delivered);
    selection.verify(&project).unwrap();

    let base = reopen_pack(content, &scratch.0.join("scene-replay.pack"), &recipe);
    let catalog = base.catalog();
    assert_eq!(catalog.scenes(), [product(SCENE_ID)]);
    assert_eq!(
        catalog.member_for_product(a).unwrap().dependencies(),
        [required(b)]
    );
    assert_eq!(
        catalog.member_for_product(b).unwrap().dependencies(),
        [RuntimeContentDependency::new(
            scene,
            RuntimeContentDependencyRole::Reference,
        )]
    );
    let report = base.validate_deep().unwrap();
    assert_eq!((report.members(), report.products()), (4, 4));
}

fn build_link_recipe() -> ProductRecipe {
    recipe()
        .add_asset_domain::<BuildLinkDomain>()
        .unwrap()
        .add_runtime_asset_codec::<BuildLinkDomain, BuildLinkDomain>(RuntimeCodecDefinitionId::new(
            "consumer.cook-built-link.codec",
            1,
        ))
        .unwrap()
}

fn write_build_link(
    assets: &Path,
    (name, id): (&str, &str),
    target: AssetProductRef,
    demand: AssetReferenceDemand,
    source_scene: Option<&str>,
) {
    let link = BuildLink {
        product: product(id),
        target,
        demand,
        source_scene: None,
    };
    fs::write(assets.join(name), serde_json::to_vec(&link).unwrap()).unwrap();
    write_meta(
        assets,
        name,
        id,
        AssetSourceKind::Other(BuildLinkDomain::SOURCE_KIND.into()),
        source_scene.map(|path| AssetPath::new(path).unwrap()),
    );
}

fn build_group<D: SourceAssetBuild>(
    root: &DirectoryCapability,
    store: &ArtifactBuildStore,
    path: &str,
    implementation: ArtifactContentDigest,
) -> VerifiedArtifactGroup
where
    D::Settings: Serialize,
{
    let request = ArtifactSourceRequest::new(
        root.open_directory_exact(&RelativePath::new("assets").unwrap())
            .unwrap(),
        AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
        AssetPath::new(path).unwrap(),
        ArtifactBuildDefinition {
            id: D::BUILD_ID.to_owned(),
            revision: FormatVersion::new(D::BUILD_REVISION).unwrap(),
            implementation,
        },
    );
    build_source::<D>(store, &request, |_| Ok(()))
        .unwrap()
        .into_group()
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BuildLink {
    product: AssetProductRef,
    target: AssetProductRef,
    demand: AssetReferenceDemand,
    #[serde(skip)]
    source_scene: Option<AssetRef>,
}

struct BuildLinkDomain;

impl AssetDomain for BuildLinkDomain {
    type Product = BuildLink;
}

impl SourceAssetImporter for BuildLinkDomain {
    const SOURCE_KIND: &'static str = "consumer.cook-built-link";
    type Settings = Option<AssetPath>;

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            ByteLimit::new(1024).unwrap(),
            ByteLimit::new(4096).unwrap(),
            ByteLimit::new(1024).unwrap(),
        )
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<BuildLink, AssetImportError<E>> {
        let mut link: BuildLink = serde_json::from_slice(source.bytes())
            .map_err(|_| AssetImportError::domain(invalid_build_link()))?;
        let metadata = source
            .metadata()
            .admit::<Self::Settings>()
            .map_err(|_| AssetImportError::domain(invalid_build_link()))?;
        let (identity, settings) = metadata.into_meta().into_parts();
        if link.product.source() != identity.stable_id()
            || link.product.product() != ImportedProductId::PRIMARY
        {
            return Err(AssetImportError::domain(invalid_build_link()));
        }
        link.source_scene = settings.map(AssetRef::Path);
        Ok(link)
    }

    fn visit_source_dependencies<E>(
        link: &BuildLink,
        mut visit: impl FnMut(&AssetRef, &AssetSourceKind, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        if let Some(scene) = &link.source_scene {
            visit(
                scene,
                &AssetSourceKind::Scene,
                AssetReferenceDemand::Immediate,
            )?;
        }
        Ok(())
    }
}

impl SourceAssetBuild for BuildLinkDomain {
    type Codec = Self;
    const BUILD_ID: &'static str = "consumer.cook-built-link.build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(settings: Self::Settings) -> Result<Self::Settings, Diagnostic> {
        Ok(settings)
    }

    fn visit_products<E>(
        link: &BuildLink,
        mut visit: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        visit(link.product)
    }
}

impl RuntimeAssetCodec<BuildLink> for BuildLinkDomain {
    const RUNTIME_KIND: &'static str = "consumer_cook_built_link";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(
            ByteLimit::new(1024).unwrap(),
            ByteLimit::new(4096).unwrap(),
            ByteLimit::new(size_of::<BuildLink>()).unwrap(),
        )
    }

    fn encode_runtime(link: &BuildLink) -> Result<Box<[u8]>, AssetRuntimeError> {
        serde_json::to_vec(link)
            .map(Vec::into_boxed_slice)
            .map_err(|_| AssetRuntimeError::new(invalid_build_link()))
    }

    fn decode_runtime(bytes: &[u8]) -> Result<BuildLink, AssetRuntimeError> {
        serde_json::from_slice(bytes).map_err(|_| AssetRuntimeError::new(invalid_build_link()))
    }

    fn runtime_retained_bytes(_: &BuildLink) -> Result<usize, AssetRuntimeError> {
        Ok(size_of::<BuildLink>())
    }

    fn visit_runtime_dependencies<E>(
        link: &BuildLink,
        mut visit: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        visit(link.target, link.demand)
    }

    fn contains_runtime_product(link: &BuildLink, product: AssetProductRef) -> bool {
        link.product == product
    }
}

fn invalid_build_link() -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new("consumer.cook-built-link.invalid").unwrap(),
        SafeSummary::new("The built link is invalid").unwrap(),
    )
}
