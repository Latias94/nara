//! Project Cook packs the Runtime closure of a typed badge and retains its source inputs.

#![cfg(any(windows, target_os = "linux"))]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe, SchemaContribution,
    advanced_prelude::CurrentScene,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetPathScope, AssetProductRef, AssetRef,
        AssetReferenceDemand, AssetSourceKind, AssetSourcePath, Assets, ImportedProductId,
        SourceAssetBuild, SourceAssetImporter, StableAssetId,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildObservation,
        ArtifactBuildReason, ArtifactBuildStatus, ArtifactBuildStore, ArtifactContentDigest,
        ArtifactSourceRequest, build_source,
    },
    core::FormatVersion,
    ecs::Component,
    fs::{ContentDigest, DirectoryCapability, FsError, RelativePath},
    identity::EntityLookup,
    image::{ImageAsset, ImageDomain, ImageExtent, ImageImportSettings, ImageRuntimeCodec},
    project_host::{
        HeadlessSession, HeadlessSessionClose, ProjectContentBudgetKind, ProjectContentLimits,
        ProjectContentMemberKind, ProjectDomainAssets, ProjectSettingsCandidate,
        ingest_project_manifest, resolve_product_recipe,
    },
    project_tooling::{
        host_fs::{readable_directory, readable_file, writable_directory},
        project_cook::{ProjectCookError, ProjectCookRequest, prepare_project_cooked_content},
    },
    project_workflow::{
        ProjectCheckRequest, check_project,
        cooked_content::{CookedContentError, CookedContentLimits, PreparedRuntimeContent},
    },
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    runtime_content::{
        RuntimeCodecDefinitionId, RuntimeContentBase, RuntimeContentCatalogError,
        RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentError,
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentTarget,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord},
};
use serde::Serialize;

#[path = "support/project_cook_badge.rs"]
mod badge;

use badge::{BadgeArt, BadgeDomain, BadgeSettings, BadgeSource, ImageDelivery};

const SCENE: &str = "startup.scene.json";
const BADGE: &str = "badge.json";
const LATER_SCENE: &str = "later.scene.json";
const LATER_BADGE: &str = "later.badge.json";
const EXTRA_BADGE: &str = "extra.badge.json";
const PNG: &str = "badge.png";
const PNG_SOURCE: &str = "assets/badge.png";
const PNG_SIDECAR: &str = "assets/badge.png.meta";
const UNUSED_PNG: &str = "unused.png";
const SCENE_ID: &str = "25910000-0000-4000-8000-000000000001";
const BADGE_ID: &str = "25910000-0000-4000-8000-000000000002";
const IMAGE_ID: &str = "25910000-0000-4000-8000-000000000003";
const UNUSED_ID: &str = "25910000-0000-4000-8000-000000000004";
const LATER_SCENE_ID: &str = "25910000-0000-4000-8000-000000000006";
const LATER_BADGE_ID: &str = "25910000-0000-4000-8000-000000000007";
const EXTRA_BADGE_ID: &str = "25910000-0000-4000-8000-000000000009";
const EARLY_IMAGE_ID: &str = "25900000-0000-4000-8000-000000000003";
const PLACEMENT: &str = "consumer.cook-badge.Placement";
const PNG_BEFORE: &[u8] = include_bytes!("../../../images/valid-rgba-1x1.png");
const PNG_AFTER: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x01, 0x01, 0x05, 0x00, 0xfa, 0xff,
    0x00, 0xe8, 0x40, 0x18, 0xff, 0x05, 0x94, 0x02, 0x40, 0xad, 0x45, 0xa8, 0x9e, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[test]
fn project_cook_embeds_build_only_png_and_rebuilds_after_source_drift() {
    exercise_project_cook(ImageDelivery::Embedded);
}

#[test]
fn project_cook_retains_linked_image_and_rebuilds_after_source_drift() {
    exercise_project_cook(ImageDelivery::Linked);
}

#[test]
fn project_cook_resolves_an_image_first_required_by_the_only_scene_build() {
    exercise_project_cook(ImageDelivery::LinkedAtBuild);
}

#[test]
#[allow(
    clippy::result_large_err,
    reason = "The public Cook contribution callback returns ProjectCookError."
)]
fn project_cook_observes_the_build_selected_image_separately_from_the_raw_import_input() {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = recipe();
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::LinkedAtBuild,
        None,
        BadgeReference::Path,
        &recipe,
    );
    let selected = product(UNUSED_ID);
    let selected_path = "assets/unused.png";
    let selected_sidecar = "assets/unused.png.meta";
    fs::write(project.join(selected_path), PNG_AFTER).unwrap();
    fs::write(
        project.join("assets").join(BADGE),
        serde_json::to_vec(&BadgeSource {
            png: AssetPath::new(PNG).unwrap(),
            image: selected,
        })
        .unwrap(),
    )
    .unwrap();

    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let request = build_request(&root, BADGE, implementation);
    let build = || build_source::<BadgeDomain>(&store, &request, |_| Ok(())).unwrap();
    let built = build();
    assert_eq!(
        built.group().members()[0].dependencies,
        [(selected, AssetReferenceDemand::Immediate)]
    );
    assert!(built.group().observations().iter().all(|observation| !matches!(observation,
        ArtifactBuildObservation::Raw {path, ..} if [selected_path, selected_sidecar].contains(&path.as_str()))));

    let cook = |recipe: ProductRecipe| {
        prepare_project_cooked_content(
            ProjectCookRequest {
                root: &root,
                candidate: &candidate,
                recipe,
                target: RuntimeContentTarget::new("consumer-headless", "badge").unwrap(),
                store: &store,
                implementation,
                limits: CookedContentLimits::default(),
            },
            |context| {
                assert!(context.snapshot().images().is_empty());
                context.add_group::<BadgeDomain>(built.group())
            },
        )
    };
    let (content, selection) = cook(recipe.clone()).unwrap().into_parts();
    assert_raw_observation(selection.observations(), selected_path, PNG_AFTER);
    assert_raw_observation(
        selection.observations(),
        selected_sidecar,
        &fs::read(project.join(selected_sidecar)).unwrap(),
    );
    assert_png_observation(selection.observations(), PNG_BEFORE);
    assert!(content.base().member(product(IMAGE_ID)).is_none());
    let image = content
        .base()
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(selected)
        .unwrap();
    assert_eq!(image.product().pixels(), [232, 64, 24, 255]);
    selection.verify(&project).unwrap();

    let receipt = selection.delivery_receipt();
    assert!(
        receipt
            .members()
            .iter()
            .all(|member| { ![selected_path, selected_sidecar].contains(&member.path().as_str()) })
    );
    let source_files = receipt.members().len();
    let recipe_with_file_limit = |limit| {
        recipe.clone().with_source_content_limits(
            ProjectContentLimits::default()
                .with_limit(ProjectContentBudgetKind::Files, limit)
                .unwrap(),
        )
    };
    let over_limit = cook(recipe_with_file_limit(source_files + 1));
    assert!(matches!(over_limit, Err(ProjectCookError::Source(error))
        if error.diagnostics().iter().any(|diagnostic| diagnostic.code().as_str() == "project.delivery.member-limit")));
    let (_, exact) = cook(recipe_with_file_limit(source_files + 2))
        .unwrap()
        .into_parts();
    exact.verify(&project).unwrap();

    fs::write(project.join(selected_path), PNG_BEFORE).unwrap();
    assert!(matches!(selection.verify(&project),
        Err(ProjectCookError::Filesystem(FsError::DigestMismatch { expected, actual }))
        if expected == ContentDigest::of_bytes(PNG_AFTER) && actual == ContentDigest::of_bytes(PNG_BEFORE)));
    let reused = build();
    assert_eq!(reused.status(), ArtifactBuildStatus::Hit);
    assert_eq!(reused.group().digest(), built.group().digest());
    let (content, selection) = cook(recipe.clone()).unwrap().into_parts();
    selection.verify(&project).unwrap();
    let image = content
        .base()
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(selected)
        .unwrap();
    assert_eq!(image.product().pixels(), [24, 120, 220, 255]);
}

#[test]
fn project_cook_reacquires_the_earlier_image_without_reimporting_its_scene() {
    exercise_cross_scene_images(None);
}

#[test]
fn project_cook_can_acquire_known_and_new_images_in_either_product_order() {
    for extra_image in [UNUSED_ID, EARLY_IMAGE_ID] {
        exercise_cross_scene_images(Some(extra_image));
    }
}

#[test]
#[allow(
    clippy::result_large_err,
    reason = "The public Cook contribution callback returns ProjectCookError."
)]
fn project_cook_rejects_a_duplicate_image_product_for_either_embedded_path() {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = recipe();
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::Embedded,
        Some(ImageDelivery::LinkedAtBuild),
        BadgeReference::Path,
        &recipe,
    );
    let assets = project.join("assets");
    fs::write(assets.join(UNUSED_PNG), PNG_AFTER).unwrap();
    write_image_meta(&assets, UNUSED_PNG, IMAGE_ID);

    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let request = build_request(&root, LATER_BADGE, implementation);
    let linked = build_source::<BadgeDomain>(&store, &request, |_| Ok(()))
        .unwrap()
        .into_group();
    let image = product(IMAGE_ID);
    let [member] = linked.members() else {
        panic!("the later Badge build must produce one member");
    };
    assert_eq!(member.products, [product(LATER_BADGE_ID)]);
    assert_eq!(
        member.dependencies,
        [(image, AssetReferenceDemand::Immediate)]
    );

    for (png, pixel) in [(PNG, [24, 120, 220, 255]), (UNUSED_PNG, [232, 64, 24, 255])] {
        write_badge_body(&assets, BADGE, png, image);
        let result = prepare_project_cooked_content(
            badge_cook_request(&root, &candidate, &recipe, &store, implementation),
            |context| match context.snapshot().selected_scene_path().as_str() {
                SCENE => {
                    let [available] = context.snapshot().images() else {
                        panic!("the startup source snapshot must contain exactly one Image");
                    };
                    assert_eq!(available.path().as_str(), format!("assets/{png}"));
                    assert_eq!(available.image().product(), Some(image));
                    assert_eq!(available.image().pixels(), pixel);
                    context.add_source_assets::<BadgeDomain>()
                }
                LATER_SCENE => {
                    assert!(context.snapshot().images().is_empty());
                    context.add_group::<BadgeDomain>(&linked)
                }
                path => panic!("unexpected Scene contribution: {path}"),
            },
        );
        assert_image_identity_conflict(result, png);
    }
}

#[test]
#[allow(
    clippy::result_large_err,
    reason = "The public Cook contribution callback returns ProjectCookError."
)]
fn project_cook_allows_path_only_embedding_with_a_duplicate_image_id() {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = recipe();
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::Embedded,
        None,
        BadgeReference::Path,
        &recipe,
    );
    let assets = project.join("assets");
    fs::write(assets.join(UNUSED_PNG), PNG_AFTER).unwrap();
    write_image_meta(&assets, UNUSED_PNG, IMAGE_ID);
    write_badge_body(&assets, BADGE, UNUSED_PNG, product(IMAGE_ID));

    let store = open_store(&scratch.0);
    let (content, selection) = prepare_project_cooked_content(
        badge_cook_request(&root, &candidate, &recipe, &store, implementation_digest()),
        |context| context.add_source_assets::<BadgeDomain>(),
    )
    .unwrap()
    .into_parts();
    selection.verify(&project).unwrap();
    observe_player(
        reopen_pack(content, &scratch.0.join("embedded.pack"), &recipe),
        None,
        PNG_AFTER,
        [232, 64, 24, 255],
    );
}

#[test]
fn project_cook_rechecks_image_ownership_when_the_build_first_selects_the_image() {
    exercise_image_ownership_recheck(false);
}

#[test]
fn project_cook_rechecks_image_ownership_for_stock_encoded_from_the_scene_snapshot() {
    exercise_image_ownership_recheck(true);
}

#[allow(
    clippy::result_large_err,
    reason = "The public Cook contribution callback returns ProjectCookError."
)]
fn exercise_image_ownership_recheck(embed_in_startup: bool) {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = recipe();
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::LinkedAtBuild,
        None,
        BadgeReference::Path,
        &recipe,
    );
    let assets = project.join("assets");
    let image = product(IMAGE_ID);
    if embed_in_startup {
        write_badge_source(
            &assets,
            EXTRA_BADGE,
            EXTRA_BADGE_ID,
            image,
            ImageDelivery::Embedded,
        );
        let document = SceneDocument::from_ordered_roots([
            badge_entity(placement_id(), AssetRef::path(BADGE).unwrap()),
            badge_entity(
                SceneEntityId::new("embedded-badge-placement").unwrap(),
                AssetRef::path(EXTRA_BADGE).unwrap(),
            ),
        ]);
        fs::write(
            project.join("scenes").join(SCENE),
            document.to_json_string().unwrap(),
        )
        .unwrap();
    }
    fs::write(assets.join(UNUSED_PNG), PNG_AFTER).unwrap();
    let unused_sidecar = assets.join("unused.png.meta");
    fs::remove_file(&unused_sidecar).unwrap();

    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let cook = || {
        prepare_project_cooked_content(
            badge_cook_request(&root, &candidate, &recipe, &store, implementation),
            |context| {
                assert_eq!(
                    context.snapshot().images().len(),
                    usize::from(embed_in_startup)
                );
                context.add_source_assets::<BadgeDomain>()
            },
        )
    };
    let (_, without_sidecar) = cook().unwrap().into_parts();
    without_sidecar.verify(&project).unwrap();
    write_image_meta(&assets, UNUSED_PNG, UNUSED_ID);
    let (_, with_sidecar) = cook().unwrap().into_parts();
    with_sidecar.verify(&project).unwrap();

    write_image_meta(&assets, UNUSED_PNG, IMAGE_ID);
    assert_image_identity_conflict(without_sidecar.verify(&project), "an added sidecar");
    assert_image_identity_conflict(with_sidecar.verify(&project), "an edited sidecar");
    assert_image_identity_conflict(cook(), "a fresh Cook with the conflicting sidecar");

    write_image_meta(&assets, UNUSED_PNG, EARLY_IMAGE_ID);
    with_sidecar.verify(&project).unwrap();
    cook().unwrap();
    fs::remove_file(&unused_sidecar).unwrap();
    let prefab_root = project.join("prefabs");
    fs::write(prefab_root.join(UNUSED_PNG), PNG_AFTER).unwrap();
    write_image_meta(&prefab_root, UNUSED_PNG, IMAGE_ID);
    assert_image_identity_conflict(without_sidecar.verify(&project), "a claim in another root");
    assert_image_identity_conflict(cook(), "a fresh Cook with a claim in another root");
    fs::remove_file(prefab_root.join("unused.png.meta")).unwrap();
    without_sidecar.verify(&project).unwrap();
    let (content, selection) = cook().unwrap().into_parts();
    selection.verify(&project).unwrap();
    let base = reopen_pack(content, &scratch.0.join("retried.pack"), &recipe);
    let decoded = base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image)
        .unwrap();
    assert_eq!(decoded.product().pixels(), [24, 120, 220, 255]);
    let complete = base.validate_deep().unwrap();
    assert_eq!(complete.members(), 3 + usize::from(embed_in_startup));
}

fn assert_image_identity_conflict<T>(result: Result<T, ProjectCookError>, case: &str) {
    let Err(error) = result else {
        panic!("expected an Image identity conflict for {case}");
    };
    assert!(
        matches!(&error, ProjectCookError::Source(source) if source.diagnostics().iter().any(|diagnostic|
            diagnostic.code().as_str() == "project.content.asset-identity-conflict")),
        "expected an Image identity conflict for {case}, got {error:?}"
    );
}

#[allow(
    clippy::result_large_err,
    reason = "The public Cook contribution callback returns ProjectCookError."
)]
fn exercise_cross_scene_images(extra_image_id: Option<&str>) {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = recipe();
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::Embedded,
        Some(ImageDelivery::LinkedAtBuild),
        BadgeReference::Path,
        &recipe,
    );
    let assets = project.join("assets");
    let extra_image = extra_image_id.map(product);
    if let Some(id) = extra_image_id {
        fs::write(assets.join("unused.png"), PNG_AFTER).unwrap();
        write_meta(
            &assets,
            "unused.png",
            id,
            AssetSourceKind::Image,
            ImageImportSettings::default(),
        );
        write_badge_source(
            &assets,
            EXTRA_BADGE,
            EXTRA_BADGE_ID,
            product(id),
            ImageDelivery::LinkedAtBuild,
        );
        let document = SceneDocument::from_ordered_roots([
            badge_entity(placement_id(), AssetRef::path(LATER_BADGE).unwrap()),
            badge_entity(
                SceneEntityId::new("extra-badge-placement").unwrap(),
                AssetRef::path(EXTRA_BADGE).unwrap(),
            ),
        ]);
        fs::write(
            project.join("scenes").join(LATER_SCENE),
            document.to_json_string().unwrap(),
        )
        .unwrap();
    } else {
        fs::write(assets.join("unused.png.meta"), b"invalid").unwrap();
    }
    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let build_group = |path| {
        let request = build_request(&root, path, implementation);
        build_source::<BadgeDomain>(&store, &request, |_| Ok(()))
            .unwrap()
            .into_group()
    };
    let embedded_group = build_group(BADGE);
    let linked_group = build_group(LATER_BADGE);
    let extra_group = extra_image.map(|_| build_group(EXTRA_BADGE));
    let imports_before = badge::embedded_imports();
    let mut contributions = Vec::new();
    let image = product(IMAGE_ID);
    let roots = [product(SCENE_ID), product(LATER_SCENE_ID)];
    let cook = |recipe, contributions: &mut Vec<_>| {
        prepare_project_cooked_content(
            ProjectCookRequest {
                root: &root,
                candidate: &candidate,
                recipe,
                target: RuntimeContentTarget::new("consumer-headless", "badge").unwrap(),
                store: &store,
                implementation,
                limits: CookedContentLimits::default(),
            },
            |context| {
                let snapshot = context.snapshot();
                contributions.push(snapshot.selected_scene_product().unwrap());
                let group = match snapshot.selected_scene_path().as_str() {
                    SCENE => {
                        let [available] = snapshot.images() else {
                            panic!("the startup source snapshot must contain exactly one Image");
                        };
                        assert_eq!(available.image().product(), Some(image));
                        &embedded_group
                    }
                    LATER_SCENE => {
                        assert!(snapshot.images().is_empty());
                        if let Some(group) = &extra_group {
                            context.add_group::<BadgeDomain>(group)?;
                        }
                        &linked_group
                    }
                    path => panic!("unexpected Scene contribution: {path}"),
                };
                context.add_group::<BadgeDomain>(group)
            },
        )
    };
    let prepared = cook(recipe.clone(), &mut contributions)
        .expect("Project Cook must recover the earlier stock Image for the later Badge");
    assert_eq!(contributions, roots);
    assert_eq!(badge::embedded_imports() - imports_before, 1);

    let (content, selection) = prepared.into_parts();
    selection.verify(&project).unwrap();
    if extra_image.is_none() {
        let exact_limit = selection.delivery_receipt().members().len();
        let sidecars = ["assets", "scenes", "prefabs"]
            .into_iter()
            .flat_map(|directory| fs::read_dir(project.join(directory)).unwrap())
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "meta")
            })
            .count();
        assert!(sidecars < exact_limit);
        for index in sidecars..exact_limit {
            let name = format!("extra-{index}.png");
            fs::write(assets.join(&name), PNG_AFTER).unwrap();
            write_meta(
                &assets,
                &name,
                &format!("00000000-0000-4001-8000-{index:012x}"),
                AssetSourceKind::Image,
                ImageImportSettings::default(),
            );
        }
        let exact = recipe.clone().with_source_content_limits(
            ProjectContentLimits::default()
                .with_limit(ProjectContentBudgetKind::Files, exact_limit)
                .unwrap(),
        );
        let (_, exact) = cook(exact, &mut Vec::new()).unwrap().into_parts();
        exact.verify(&project).unwrap();

        fs::copy(
            assets.join(format!("extra-{sidecars}.png.meta")),
            assets.join(".meta"),
        )
        .unwrap();
        selection.verify(&project).unwrap();
        let (_, unrelated) = cook(recipe.clone(), &mut Vec::new()).unwrap().into_parts();
        unrelated.verify(&project).unwrap();
    }
    let base = reopen_pack(content, &scratch.0.join("later.pack"), &recipe);
    let catalog = base.catalog();
    let [startup, later] = roots;
    let embedded = product(BADGE_ID);
    let linked = product(LATER_BADGE_ID);
    assert_eq!(catalog.startup_scene(), startup);
    assert_eq!(catalog.scenes(), roots);
    let member_count = if extra_image.is_some() { 7 } else { 5 };
    assert_eq!(catalog.members().len(), member_count);
    for (member, dependency) in [
        (startup, Some(embedded)),
        (embedded, None),
        (linked, Some(image)),
        (image, None),
    ] {
        assert_eq!(
            catalog.member_for_product(member).unwrap().dependencies(),
            dependency.map(required).as_slice()
        );
    }
    let mut later_dependencies = vec![required(linked)];
    if let Some(extra_image) = extra_image {
        let extra_badge = product(EXTRA_BADGE_ID);
        later_dependencies.push(required(extra_badge));
        assert_eq!(
            catalog
                .member_for_product(extra_badge)
                .unwrap()
                .dependencies(),
            [required(extra_image)]
        );
        assert!(
            catalog
                .member_for_product(extra_image)
                .unwrap()
                .dependencies()
                .is_empty()
        );
        let selected = base
            .acquire_asset::<ImageDomain, ImageRuntimeCodec>(extra_image)
            .unwrap();
        assert_eq!(selected.product().pixels(), [232, 64, 24, 255]);
    }
    assert_eq!(
        catalog.member_for_product(later).unwrap().dependencies(),
        later_dependencies
    );
    let known = base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image)
        .unwrap();
    assert_eq!(known.product().pixels(), [24, 120, 220, 255]);
    let complete = base.validate_deep().unwrap();
    assert_eq!(complete.members(), member_count);
    assert_eq!(complete.products(), member_count);
}

#[allow(
    clippy::result_large_err,
    reason = "The public Cook error is preserved for typed assertions."
)]
fn exercise_project_cook(image_delivery: ImageDelivery) {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = recipe();
    let (root, candidate) = write_project(
        &project,
        image_delivery,
        None,
        BadgeReference::Product,
        &recipe,
    );

    let checked = check_project(ProjectCheckRequest::new(root.share(), recipe.clone())).unwrap();
    let png_was_read = checked.members().iter().any(|member| {
        member.path().as_str() == PNG_SOURCE
            && member.kind() == ProjectContentMemberKind::AssetSource
            && member.digest() == ContentDigest::of_bytes(PNG_BEFORE)
    });
    assert!(
        png_was_read,
        "source import consumes the PNG as a raw input"
    );
    let image_was_selected = checked.members().iter().any(|member| {
        member.path().as_str() == PNG_SIDECAR
            && member.kind() == ProjectContentMemberKind::AssetMetadata
    });
    assert_eq!(
        image_was_selected,
        image_delivery != ImageDelivery::LinkedAtBuild
    );

    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let request = build_request(&root, BADGE, implementation);
    let build = || build_source::<BadgeDomain>(&store, &request, |_| Ok(())).unwrap();
    let built = build();
    assert_eq!(built.status(), ArtifactBuildStatus::Built);
    assert!(built.publication_receipt().is_some());
    let hit = build();
    assert_eq!(hit.status(), ArtifactBuildStatus::Hit);
    assert_eq!(hit.reason(), &ArtifactBuildReason::Reused);
    assert_eq!(hit.group().digest(), built.group().digest());
    let group = hit.into_group();
    drop(built);

    let image = (image_delivery != ImageDelivery::Embedded).then(|| product(IMAGE_ID));
    let edge = image.map(|image| (image, AssetReferenceDemand::Immediate));
    let [member] = group.members() else {
        panic!("the badge build must produce one member");
    };
    assert_eq!(member.products, [product(BADGE_ID)]);
    assert_eq!(member.dependencies.as_slice(), edge.as_slice());
    assert_png_observation(group.observations(), PNG_BEFORE);

    let cook_request = || ProjectCookRequest {
        root: &root,
        candidate: &candidate,
        recipe: recipe.clone(),
        target: RuntimeContentTarget::new("consumer-headless", "badge").unwrap(),
        store: &store,
        implementation,
        limits: CookedContentLimits::default(),
    };
    let cook_retained = || {
        let mut visits = 0;
        let result = prepare_project_cooked_content(cook_request(), |context| {
            visits += 1;
            if image_delivery == ImageDelivery::LinkedAtBuild {
                assert!(context.snapshot().images().is_empty());
            }
            context.add_group::<BadgeDomain>(&group)
        });
        assert_eq!(visits, 1);
        result
    };
    let first = cook_retained().expect("Project Cook must deliver the badge's Runtime closure");
    let (content, selection) = first.into_parts();
    assert_png_observation(selection.observations(), PNG_BEFORE);
    selection.verify(&project).unwrap();
    observe_player(
        reopen_pack(content, &scratch.0.join("first.pack"), &recipe),
        image,
        PNG_BEFORE,
        [24, 120, 220, 255],
    );

    if image_delivery == ImageDelivery::LinkedAtBuild {
        let sidecar_path = project.join(PNG_SIDECAR);
        let sidecar = fs::read(&sidecar_path).unwrap();
        assert_raw_observation(selection.observations(), PNG_SIDECAR, &sidecar);
        let replacement = String::from_utf8(sidecar.clone())
            .unwrap()
            .replace(IMAGE_ID, "25910000-0000-4000-8000-000000000008");
        let changed = ContentDigest::of_bytes(replacement.as_bytes());
        let original = ContentDigest::of_bytes(&sidecar);
        assert_eq!(changed.length(), original.length());
        assert_ne!(changed, original);
        fs::write(&sidecar_path, &replacement).unwrap();
        let drift = selection.verify(&project).unwrap_err();
        assert!(
            matches!(drift, ProjectCookError::Filesystem(FsError::DigestMismatch { expected, actual })
            if expected == original && actual == changed),
            "{drift:?}"
        );
        let Err(error) = cook_retained() else {
            panic!("a changed Image identity satisfied the original Product");
        };
        assert!(
            matches!(
                error,
                ProjectCookError::Content(CookedContentError::Runtime(RuntimeContentError::Catalog(
                    RuntimeContentCatalogError::MissingDependencyProduct { product: missing, .. }
                ))) if missing == product(IMAGE_ID)
            ),
            "{error:?}"
        );
        fs::write(&sidecar_path, &sidecar).unwrap();
        selection.verify(&project).unwrap();

        let drift = prepare_project_cooked_content(cook_request(), |context| {
            context.add_group::<BadgeDomain>(&group)?;
            fs::write(&sidecar_path, &replacement).unwrap();
            Ok(())
        });
        fs::write(&sidecar_path, &sidecar).unwrap();
        let Err(error) = drift else {
            panic!("an admitted Image sidecar changed before its late read");
        };
        assert!(
            matches!(error, ProjectCookError::Source(ref source)
            if source.diagnostics().iter().any(|entry|
                entry.code().as_str() == "project.content.asset-metadata-source-changed")),
            "{error:?}"
        );
    }

    fs::write(project.join(PNG_SOURCE), PNG_AFTER).unwrap();
    let drift = selection.verify(&project).unwrap_err();
    assert!(
        matches!(drift, ProjectCookError::Filesystem(FsError::DigestMismatch { expected, actual })
            if expected == ContentDigest::of_bytes(PNG_BEFORE)
                && actual == ContentDigest::of_bytes(PNG_AFTER)),
        "{drift:?}"
    );
    assert!(
        cook_retained().is_err(),
        "a later Cook accepted the stale badge group"
    );

    let rebuilt = build();
    assert_eq!(rebuilt.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        rebuilt.reason(),
        &ArtifactBuildReason::RawInputChanged(AssetSourcePath::new(PNG_SOURCE).unwrap()),
    );
    assert_ne!(rebuilt.group().digest(), group.digest());
    assert_png_observation(rebuilt.group().observations(), PNG_AFTER);
    drop(selection);

    let second = prepare_project_cooked_content(cook_request(), |context| {
        context.add_source_assets::<BadgeDomain>()
    })
    .expect("Project Cook must see the rebuilt badge after PNG drift");
    let (content, selection) = second.into_parts();
    assert_png_observation(selection.observations(), PNG_AFTER);
    selection.verify(&project).unwrap();
    let pack = scratch.0.join("second.pack");
    let base = if image_delivery == ImageDelivery::LinkedAtBuild {
        let expectation = write_pack(content, &pack);
        drop((selection, request, root));
        let original = project.canonicalize().unwrap();
        assert_eq!(original.parent(), Some(scratch.0.as_path()));
        let retired = scratch.0.join("retired-project");
        assert!(!retired.exists());
        fs::rename(original, retired).unwrap();
        assert!(!project.exists());
        let relocated = scratch.0.join("relocated");
        fs::create_dir(&relocated).unwrap();
        let published = relocated.join("content.pack");
        fs::rename(&pack, &published).unwrap();
        open_pack(&published, &expectation, &recipe)
    } else {
        reopen_pack(content, &pack, &recipe)
    };
    observe_player(base, image, PNG_AFTER, [232, 64, 24, 255]);
}

fn assert_png_observation(observations: &[ArtifactBuildObservation], png: &[u8]) {
    assert_raw_observation(observations, PNG_SOURCE, png);
}

fn assert_raw_observation(observations: &[ArtifactBuildObservation], path: &str, bytes: &[u8]) {
    let expected = ArtifactBuildObservation::Raw {
        path: AssetSourcePath::new(path).unwrap(),
        digest: Some(ArtifactContentDigest::of_bytes(bytes)),
    };
    assert!(
        observations.contains(&expected),
        "missing source observation: {expected:?}"
    );
}

fn reopen_pack(
    content: PreparedRuntimeContent,
    pack: &Path,
    recipe: &ProductRecipe,
) -> RuntimeContentBase {
    let expectation = write_pack(content, pack);
    open_pack(pack, &expectation, recipe)
}

fn write_pack(content: PreparedRuntimeContent, pack: &Path) -> RuntimeContentExpectation {
    content
        .base()
        .write_pack(&mut File::create_new(pack).unwrap())
        .unwrap();
    content.expectation().clone()
}

fn open_pack(
    pack: &Path,
    expectation: &RuntimeContentExpectation,
    recipe: &ProductRecipe,
) -> RuntimeContentBase {
    RuntimeContentBase::open_pack(
        readable_file(pack).unwrap(),
        expectation,
        recipe.clone(),
        RuntimeContentLimits::default(),
    )
    .unwrap()
}

fn required(target: AssetProductRef) -> RuntimeContentDependency {
    RuntimeContentDependency::new(target, RuntimeContentDependencyRole::Required)
}

fn observe_player(
    base: RuntimeContentBase,
    image: Option<AssetProductRef>,
    expected_png: &[u8],
    expected_pixel: [u8; 4],
) {
    let catalog_digest = base.catalog_digest();
    let catalog = base.catalog();
    let scene = product(SCENE_ID);
    let badge = product(BADGE_ID);
    let image_edge = image.map(required);
    let members = 2 + usize::from(image.is_some());
    assert_eq!(catalog.startup_scene(), scene);
    assert_eq!(catalog.scenes(), [scene]);
    assert!(catalog.addressable_roots().is_empty());
    assert_eq!(catalog.members().len(), members);
    assert_eq!(
        catalog.member_for_product(scene).unwrap().dependencies(),
        [required(badge)]
    );
    assert_eq!(
        catalog.member_for_product(badge).unwrap().dependencies(),
        image_edge.as_slice()
    );
    assert_eq!(
        catalog.member_for_product(product(IMAGE_ID)).is_some(),
        image.is_some()
    );
    assert!(catalog.member_for_product(product(UNUSED_ID)).is_none());
    let complete = base.validate_deep().unwrap();
    assert_eq!(complete.members(), members);
    assert_eq!(complete.products(), members);
    assert_eq!(complete.catalog_digest(), catalog_digest);

    let mut player = HeadlessSession::from_runtime_content(base);
    player.start().expect("the cooked Player must start");
    assert!(player.frame(Duration::ZERO).unwrap().is_none());
    player
        .observe(|world| {
            let current = CurrentScene::get(world).unwrap();
            let EntityLookup::Resolved(entity) = current.receipt().resolve(world, &placement_id())
            else {
                panic!("the authored badge placement must exist in the Player");
            };
            let placed = world.get::<Placement>(entity).unwrap();
            assert_eq!(placed.badge.as_product(), Some(badge));
            let badges = world
                .resource::<ProjectDomainAssets>()
                .collection::<BadgeDomain>()
                .unwrap();
            assert_eq!(badges.iter().len(), 1);
            assert_eq!(badges.source_records().len(), 0);
            let decoded = badges.get(&placed.badge).unwrap();
            assert_eq!(decoded.product, badge);
            let images = world.resource::<Assets<ImageAsset>>();
            match (&decoded.art, image) {
                (BadgeArt::Embedded(png), None) => {
                    assert_eq!(png.as_ref(), expected_png);
                    assert!(images.is_empty());
                }
                (BadgeArt::Linked(target), Some(expected)) => {
                    assert_eq!(*target, expected);
                    assert_eq!(images.len(), 1);
                    let (_, published) = images.iter().next().unwrap();
                    assert_eq!(published.product(), Some(*target));
                    assert!(published.source().is_none());
                    assert_eq!(published.extent(), ImageExtent::new(1, 1));
                    assert_eq!(published.pixels(), expected_pixel);
                }
                (art, expected) => panic!("unexpected decoded badge {art:?} for {expected:?}"),
            }
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match player.close() {
            HeadlessSessionClose::Complete => break,
            HeadlessSessionClose::Failed => {
                panic!("Player close failed: {:?}", player.diagnostics());
            }
            HeadlessSessionClose::Pending => {
                assert!(Instant::now() < deadline, "Player close timed out");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.cook-badge.Placement",
    version = 1,
    alias = "Badge Placement",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct Placement {
    #[nara(id = "badge", alias = "Badge", capabilities(scene, asset_ref))]
    badge: AssetRef,
}

nara::persistent_components! { mod components = [Placement]; }

const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.cook-badge.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.cook-badge"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.cook-badge.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
struct BadgePlugin;

impl Plugin for BadgePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.cook-badge"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<BadgePlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
        .add_asset_domain::<BadgeDomain>()
        .unwrap()
        .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "consumer.cook-badge.image",
            1,
        ))
        .unwrap()
        .add_runtime_asset_codec::<BadgeDomain, BadgeDomain>(RuntimeCodecDefinitionId::new(
            "consumer.cook-badge.codec",
            1,
        ))
        .unwrap()
}

#[derive(Clone, Copy)]
enum BadgeReference {
    Path,
    Product,
}

fn write_project(
    project: &Path,
    image_delivery: ImageDelivery,
    later_image_delivery: Option<ImageDelivery>,
    reference: BadgeReference,
    recipe: &ProductRecipe,
) -> (DirectoryCapability, ProjectSettingsCandidate) {
    for directory in ["assets", "scenes", "prefabs"] {
        fs::create_dir_all(project.join(directory)).unwrap();
    }
    let mut manifest = r#"schema_version = 1
[project]
source_universe = '25910000-0000-4000-8000-000000000005'
native_document_identity = 'project_location'
name = "Badge Cook consumer"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
state = "ready"
schema_bundle = "authoring.bundle.json"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
[tasks.io]
workers = 1
pending_capacity = 16
[tasks.compute]
workers = 1
pending_capacity = 16
[tasks.async_compute]
workers = 1
pending_capacity = 16
"#
    .to_owned();
    write_badge_scene(
        project,
        (SCENE, SCENE_ID),
        (BADGE, BADGE_ID),
        image_delivery,
        reference,
    );
    if let Some(image_delivery) = later_image_delivery {
        manifest.push_str(&format!("\n[content]\nscenes = [\"{LATER_SCENE}\"]\n"));
        write_badge_scene(
            project,
            (LATER_SCENE, LATER_SCENE_ID),
            (LATER_BADGE, LATER_BADGE_ID),
            image_delivery,
            reference,
        );
    }
    fs::write(project.join("nara.toml"), manifest).unwrap();

    let assets = project.join("assets");
    for (name, id) in [(PNG, IMAGE_ID), (UNUSED_PNG, UNUSED_ID)] {
        fs::write(assets.join(name), PNG_BEFORE).unwrap();
        write_image_meta(&assets, name, id);
    }

    let root = readable_directory(project).unwrap();
    let manifest = root
        .open_file_exact(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
    let bundle = plan
        .schema_validation()
        .authoring_schema_bundle()
        .to_json_string()
        .unwrap();
    fs::write(project.join("authoring.bundle.json"), bundle).unwrap();
    (root, candidate)
}

fn write_badge_scene(
    project: &Path,
    (scene, scene_id): (&str, &str),
    (badge, badge_id): (&str, &str),
    image_delivery: ImageDelivery,
    reference: BadgeReference,
) {
    let reference = match reference {
        BadgeReference::Path => AssetRef::path(badge).unwrap(),
        BadgeReference::Product => AssetRef::Product(product(badge_id)),
    };
    let document = SceneDocument::from_ordered_roots([badge_entity(placement_id(), reference)]);
    let scenes = project.join("scenes");
    fs::write(scenes.join(scene), document.to_json_string().unwrap()).unwrap();
    write_meta(&scenes, scene, scene_id, AssetSourceKind::Scene, ());

    write_badge_source(
        &project.join("assets"),
        badge,
        badge_id,
        product(IMAGE_ID),
        image_delivery,
    );
}

fn badge_entity(id: SceneEntityId, badge: AssetRef) -> SceneEntityRecord {
    SceneEntityRecord::new(id).with_component(
        ComponentTypeId::new(PLACEMENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            Placement { badge }.__encode_persistent_component().unwrap(),
        ),
    )
}

fn write_badge_source(
    assets: &Path,
    badge: &str,
    badge_id: &str,
    image: AssetProductRef,
    image_delivery: ImageDelivery,
) {
    write_badge_body(assets, badge, PNG, image);
    write_meta(
        assets,
        badge,
        badge_id,
        AssetSourceKind::Other(BadgeDomain::SOURCE_KIND.into()),
        BadgeSettings { image_delivery },
    );
}

fn write_badge_body(assets: &Path, badge: &str, png: &str, image: AssetProductRef) {
    let source = BadgeSource {
        png: AssetPath::new(png).unwrap(),
        image,
    };
    fs::write(assets.join(badge), serde_json::to_vec(&source).unwrap()).unwrap();
}

fn write_image_meta(assets: &Path, png: &str, id: &str) {
    write_meta(
        assets,
        png,
        id,
        AssetSourceKind::Image,
        ImageImportSettings::default(),
    );
}

fn badge_cook_request<'a>(
    root: &'a DirectoryCapability,
    candidate: &'a ProjectSettingsCandidate,
    recipe: &ProductRecipe,
    store: &'a ArtifactBuildStore,
    implementation: ArtifactContentDigest,
) -> ProjectCookRequest<'a> {
    ProjectCookRequest {
        root,
        candidate,
        recipe: recipe.clone(),
        target: RuntimeContentTarget::new("consumer-headless", "badge").unwrap(),
        store,
        implementation,
        limits: CookedContentLimits::default(),
    }
}

fn open_store(parent: &Path) -> ArtifactBuildStore {
    let cache = parent.join("cache");
    fs::create_dir(&cache).unwrap();
    ArtifactBuildStore::open(
        writable_directory(&cache).unwrap(),
        ArtifactBuildLimits::default(),
    )
    .unwrap()
}

fn build_request(
    root: &DirectoryCapability,
    badge: &str,
    implementation: ArtifactContentDigest,
) -> ArtifactSourceRequest {
    ArtifactSourceRequest::new(
        root.open_directory_exact(&RelativePath::new("assets").unwrap())
            .unwrap(),
        AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
        AssetPath::new(badge).unwrap(),
        ArtifactBuildDefinition {
            id: BadgeDomain::BUILD_ID.to_owned(),
            revision: FormatVersion::new(BadgeDomain::BUILD_REVISION).unwrap(),
            implementation,
        },
    )
}

fn write_meta<T: Serialize>(dir: &Path, name: &str, id: &str, kind: AssetSourceKind, settings: T) {
    let meta = AssetMeta::new(
        AssetMetaIdentity::new(product(id).source(), AssetPath::new(name).unwrap(), kind),
        settings,
    );
    fs::write(
        dir.join(format!("{name}.meta")),
        meta.to_json_string().unwrap(),
    )
    .unwrap();
}

fn product(id: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(id).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn placement_id() -> SceneEntityId {
    SceneEntityId::new("badge-placement").unwrap()
}

fn implementation_digest() -> ArtifactContentDigest {
    static DIGEST: OnceLock<ArtifactContentDigest> = OnceLock::new();
    *DIGEST.get_or_init(|| {
        let path = std::env::current_exe().unwrap();
        let length = fs::metadata(&path).unwrap().len();
        let digest = readable_file(&path).unwrap().digest(length).unwrap();
        ArtifactContentDigest::from_content(digest)
    })
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "nara-cook-badge-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(path) = self.0.canonicalize()
            && path == self.0
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}
