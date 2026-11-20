//! Stock definitions first selected by the Badge's cooked dependency inventory.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use nara::{
    ProductRecipe,
    asset::{
        Asset, AssetPath, AssetProductRef, AssetRef, AssetReferenceDemand, AssetServer,
        AssetSourceKind, Assets, Handle, ProjectAssetDatabase,
    },
    asset_build::{ArtifactBuildObservation, build_source},
    core::Vec2,
    ecs::World,
    fs::{ContentDigest, FsError},
    image::{ImageAsset, ImageDomain, ImageExtent, ImageRuntimeCodec},
    material::{
        AlphaMode2d, Material2dAsset, Material2dDeclaration, Material2dDomain,
        Material2dRuntimeCodec, Material2dSource, MaterialParameterKind, MaterialParameterLayout,
        MaterialParameterValue, MaterialTextureDeclaration, MaterialUniformField,
    },
    project_host::{ProjectContentBudgetKind, ProjectContentDeliveryReceipt, ProjectDomainAssets},
    project_tooling::project_cook::{
        ProjectCookError, ProjectCookSourceSelection, prepare_project_cooked_content,
    },
    project_workflow::{ProjectCheckRequest, check_project, cooked_content::CookedContentError},
    runtime_content::{
        RuntimeCodecDefinitionId, RuntimeContentBase, RuntimeContentCatalogError,
        RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentError,
    },
    tilemap::{TileAtlasLayout, TileSet, TileSetDomain, TileSetRuntimeCodec, TileSetSource},
};

use super::{
    BADGE, BADGE_ID, BadgeReference, PNG_AFTER, PNG_BEFORE, SCENE, SCENE_ID, Scratch, UNUSED_ID,
    assert_image_identity_conflict, assert_png_observation, assert_raw_observation,
    badge::{BadgeArt, BadgeDomain, ImageDelivery, MAX_STOCK_DEPENDENCIES},
    badge_cook_request, build_request, implementation_digest, observe_cooked_badge, open_pack,
    open_store, product, recipe, required, write_image_meta, write_meta, write_pack, write_project,
};

const STOCK_ID: &str = "25920000-0000-4000-8000-000000000001";
const STOCK_IMAGE_ID: &str = "25920000-0000-4000-8000-000000000002";
const REASSIGNED_ID: &str = "25920000-0000-4000-8000-000000000003";
const STOCK_IMAGE: &str = "stock.png";
const STOCK_IMAGE_SOURCE: &str = "assets/stock.png";
const STOCK_IMAGE_META: &str = "assets/stock.png.meta";
const SHADER_PATH: &str = "palette.wgsl";
const SHADER_SOURCE: &str = "assets/palette.wgsl";
const SHADER: &str = include_str!("../../../material-consumer/src/palette.wgsl");
const MATERIAL_ID: &str = "consumer.cook-badge.palette";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stock {
    TileSet,
    Material,
}

impl Stock {
    fn file(self) -> &'static str {
        match self {
            Self::TileSet => "terrain.tileset.ron",
            Self::Material => "palette.material2d.json",
        }
    }

    fn recipe(self) -> ProductRecipe {
        match self {
            Self::TileSet => recipe()
                .add_runtime_tileset_codec::<TileSetRuntimeCodec>(RuntimeCodecDefinitionId::new(
                    "consumer.cook-badge.tileset",
                    1,
                ))
                .unwrap(),
            Self::Material => recipe()
                .add_asset_domain::<Material2dDomain>()
                .unwrap()
                .add_runtime_asset_codec::<Material2dDomain, Material2dRuntimeCodec>(
                    RuntimeCodecDefinitionId::new("consumer.cook-badge.material", 1),
                )
                .unwrap(),
        }
    }

    fn write_assets(self, assets: &Path, image: AssetRef) {
        fs::write(assets.join(STOCK_IMAGE), PNG_AFTER).unwrap();
        write_image_meta(assets, STOCK_IMAGE, STOCK_IMAGE_ID);
        let kind = match self {
            Self::TileSet => {
                let source = TileSetSource::new(image, atlas());
                fs::write(assets.join(self.file()), source.encode_ron_bytes().unwrap()).unwrap();
                "tileset"
            }
            Self::Material => {
                let source = palette_source(image);
                let mut encoded = source.encode().unwrap().into_vec();
                encoded.push(b'\n');
                fs::write(assets.join(self.file()), encoded).unwrap();
                fs::write(assets.join(SHADER_PATH), SHADER).unwrap();
                Material2dDomain::SOURCE_KIND
            }
        };
        write_meta(
            assets,
            self.file(),
            STOCK_ID,
            AssetSourceKind::Other(kind.into()),
            (),
        );
    }
}

pub(super) fn exercise(stock: Stock) {
    let image = product(STOCK_IMAGE_ID);
    let by_identity = match stock {
        Stock::TileSet => AssetRef::StableId(image.source()),
        Stock::Material => AssetRef::Product(image),
    };
    for (demand, image_ref) in [
        (
            AssetReferenceDemand::Immediate,
            AssetRef::path(STOCK_IMAGE).unwrap(),
        ),
        (AssetReferenceDemand::Deferred, by_identity),
    ] {
        exercise_demand(stock, demand, image_ref);
    }
}

#[allow(
    clippy::result_large_err,
    reason = "The Cook contribution and error assertions use the public ProjectCookError."
)]
fn exercise_demand(stock: Stock, demand: AssetReferenceDemand, image_ref: AssetRef) {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = stock.recipe();
    let selected = product(STOCK_ID);
    let badge_reference = match stock {
        Stock::TileSet => BadgeReference::Path,
        Stock::Material => BadgeReference::Product,
    };
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::StockAtBuild(Box::new([(selected, demand)])),
        None,
        badge_reference,
        &recipe,
    );
    stock.write_assets(&project.join("assets"), image_ref);

    let stock_source = format!("assets/{}", stock.file());
    let stock_meta = format!("{stock_source}.meta");
    let files: Vec<_> = [
        stock_source.as_str(),
        stock_meta.as_str(),
        STOCK_IMAGE_SOURCE,
        STOCK_IMAGE_META,
    ]
    .into_iter()
    .chain((stock == Stock::Material).then_some(SHADER_SOURCE))
    .collect();
    let checked = check_project(ProjectCheckRequest::new(root.share(), recipe.clone())).unwrap();
    let assert_absent = |receipt: &ProjectContentDeliveryReceipt| {
        let [startup] = receipt.scene_roots() else {
            panic!("expected a single authored Scene root");
        };
        assert_eq!(startup.as_str(), SCENE);
        for &path in &files {
            assert!(
                receipt
                    .members()
                    .iter()
                    .all(|member| member.path().as_str() != path),
                "authored delivery selected {path}"
            );
        }
    };
    assert_absent(&checked);
    drop(checked);

    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let request = build_request(&root, BADGE, implementation);
    let group = build_source::<BadgeDomain>(&store, &request, |_| Ok(()))
        .expect("the Badge source and build must be valid")
        .into_group();
    let [member] = group.members() else {
        panic!("expected one built Badge member");
    };
    assert_eq!(member.products, [product(BADGE_ID)]);
    assert_eq!(member.dependencies, [(selected, demand)]);
    assert_png_observation(group.observations(), PNG_BEFORE);
    for &path in &files {
        assert!(
            group.observations().iter().all(|observation| !matches!(observation,
                ArtifactBuildObservation::Raw { path: observed, .. } if observed.as_str() == path
            )),
            "the Badge build read {path}"
        );
    }

    let cook = || {
        let mut visits = 0;
        let result = prepare_project_cooked_content(
            badge_cook_request(&root, &candidate, &recipe, &store, implementation),
            |context| {
                visits += 1;
                let snapshot = context.snapshot();
                assert_eq!(snapshot.selected_scene_path().as_str(), SCENE);
                assert_eq!(snapshot.selected_scene_product(), Some(product(SCENE_ID)));
                assert!(snapshot.scene_dependencies().is_empty());
                assert!(snapshot.images().is_empty());
                assert!(snapshot.tilesets().is_empty());
                assert!(
                    snapshot
                        .domain_assets()
                        .collection::<Material2dDomain>()
                        .is_none_or(|collection| collection.iter().next().is_none())
                );
                context.add_group::<BadgeDomain>(&group)
            },
        );
        assert_eq!(
            visits,
            1,
            "Cook contribution count, error: {:?}",
            result.as_ref().err()
        );
        result
    };
    let (content, selection) = cook()
        .unwrap_or_else(|error| panic!("Cook should deliver {stock:?} with {demand:?}: {error:?}"))
        .into_parts();
    assert_absent(selection.delivery_receipt());
    selection.verify(&project).unwrap();
    for &path in &files {
        assert_source_drift(&project, &selection, path);
    }

    if (stock, demand) == (Stock::TileSet, AssetReferenceDemand::Immediate) {
        let image_metadata = project.join(STOCK_IMAGE_META);
        let original = fs::read(&image_metadata).unwrap();
        write_image_meta(&project.join("assets"), STOCK_IMAGE, BADGE_ID);
        let result = cook();
        fs::write(image_metadata, original).unwrap();
        assert_image_identity_conflict(result, "a texture path claiming the Badge group's ID");
    }

    if (stock, demand) == (Stock::TileSet, AssetReferenceDemand::Deferred) {
        let meta_path = project.join(&stock_meta);
        let original = fs::read_to_string(&meta_path).unwrap();
        let reassigned = original.replace(STOCK_ID, REASSIGNED_ID);
        assert_ne!(original, reassigned);
        fs::write(&meta_path, reassigned).unwrap();
        let result = cook();
        fs::write(&meta_path, original).unwrap();
        let Err(error) = result else {
            panic!("Cook accepted the missing Reference target {selected}");
        };
        assert!(
            matches!(&error, ProjectCookError::Content(CookedContentError::Runtime(
                RuntimeContentError::Catalog(RuntimeContentCatalogError::MissingDependencyProduct {
                    product: missing, ..
                })
            )) if *missing == selected),
            "unexpected missing target error: {error:?}"
        );
    }
    selection.verify(&project).unwrap();

    let pack = scratch.0.join("stock.pack");
    let expectation = write_pack(content, &pack);
    drop((selection, group, request, store, root));
    let relocated = scratch.relocate_pack(&project, &pack);
    let base = open_pack(&relocated, &expectation, &recipe);
    observe_stock(base, stock, demand);
}

#[allow(
    clippy::result_large_err,
    reason = "The Cook contribution returns the public ProjectCookError."
)]
pub(super) fn exercise_shared_material_file_limit() {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = Stock::Material.recipe();
    let materials: Vec<_> = (0..MAX_STOCK_DEPENDENCIES)
        .map(|index| {
            (
                format!("palette-{index}.material2d.json"),
                format!("25930000-0000-4000-8000-{index:012x}"),
            )
        })
        .collect();
    let dependencies = materials
        .iter()
        .map(|(_, id)| (product(id), AssetReferenceDemand::Immediate))
        .collect();
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::StockAtBuild(dependencies),
        None,
        BadgeReference::Path,
        &recipe,
    );
    let assets = project.join("assets");
    fs::write(assets.join(STOCK_IMAGE), PNG_AFTER).unwrap();
    write_image_meta(&assets, STOCK_IMAGE, STOCK_IMAGE_ID);
    fs::write(assets.join(SHADER_PATH), SHADER).unwrap();
    let material = palette_source(AssetRef::path(STOCK_IMAGE).unwrap())
        .encode()
        .unwrap();
    for (file, id) in &materials {
        fs::write(assets.join(file), &material).unwrap();
        write_meta(
            &assets,
            file,
            id,
            AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.into()),
            (),
        );
    }

    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let cook = |recipe| {
        prepare_project_cooked_content(
            badge_cook_request(&root, &candidate, &recipe, &store, implementation),
            |context| context.add_source_assets::<BadgeDomain>(),
        )
    };
    let unrestricted = cook(recipe.clone()).unwrap();
    let source = unrestricted.source();
    let files: BTreeSet<_> = source
        .members()
        .iter()
        .map(|member| member.path().as_str())
        .chain(
            unrestricted
                .observations()
                .iter()
                .filter_map(|observation| match observation {
                    ArtifactBuildObservation::Raw {
                        path,
                        digest: Some(_),
                    } => Some(path.as_str()),
                    _ => None,
                }),
        )
        .collect();
    let limit = files.len();
    assert_eq!(limit, source.members().len() + 2 * materials.len() + 3);
    assert!(3 * materials.len() > limit);
    assert_raw_observation(
        unrestricted.observations(),
        SHADER_SOURCE,
        SHADER.as_bytes(),
    );
    assert_raw_observation(unrestricted.observations(), STOCK_IMAGE_SOURCE, PNG_AFTER);
    let base = unrestricted.content().base();
    assert_eq!(base.catalog().members().len(), materials.len() + 3);
    let expected_pack = base.catalog_digest();
    drop(unrestricted);

    let bounded = |limit| {
        recipe.clone().with_source_content_limits(
            recipe
                .source_content_limits()
                .with_limit(ProjectContentBudgetKind::Files, limit)
                .unwrap(),
        )
    };
    let (content, selection) = cook(bounded(limit)).unwrap().into_parts();
    assert_eq!(content.base().catalog_digest(), expected_pack);
    selection.verify(&project).unwrap();
    let Err(error) = cook(bounded(limit - 1)) else {
        panic!("Cook accepted a file limit one below the {limit} unique files");
    };
    assert!(
        matches!(&error, ProjectCookError::Source(source) if source.diagnostics().iter()
            .any(|entry| entry.code().as_str() == "project.delivery.member-limit")),
        "unexpected one-short file-limit error: {error:?}"
    );
}

fn assert_source_drift(project: &Path, selection: &ProjectCookSourceSelection, path: &str) {
    let file = project.join(path);
    let original = fs::read(&file).unwrap();
    assert_raw_observation(selection.observations(), path, &original);
    let changed = if path == STOCK_IMAGE_SOURCE {
        PNG_BEFORE.to_vec()
    } else {
        let mut changed = original.clone();
        let whitespace = changed
            .iter_mut()
            .find(|byte| matches!(**byte, b' ' | b'\n'))
            .unwrap();
        *whitespace = b'\t';
        changed
    };
    let expected = ContentDigest::of_bytes(&original);
    let actual = ContentDigest::of_bytes(&changed);
    assert_eq!(actual.length(), expected.length(), "{path}");
    assert_ne!(actual, expected, "{path}");
    fs::write(&file, changed).unwrap();
    let drift = selection.verify(project);
    fs::write(file, original).unwrap();
    assert!(
        matches!(drift, Err(ProjectCookError::Filesystem(FsError::DigestMismatch {
            expected: before, actual: after
        })) if before == expected && after == actual),
        "Cook selection must reject changed bytes at {path}: {drift:?}"
    );
}

fn observe_stock(base: RuntimeContentBase, stock: Stock, demand: AssetReferenceDemand) {
    let scene = product(SCENE_ID);
    let badge = product(BADGE_ID);
    let selected = product(STOCK_ID);
    let image = product(STOCK_IMAGE_ID);
    let immediate = demand == AssetReferenceDemand::Immediate;
    let role = if immediate {
        RuntimeContentDependencyRole::Required
    } else {
        RuntimeContentDependencyRole::Reference
    };
    let catalog = base.catalog();
    assert_eq!(catalog.startup_scene(), scene);
    assert_eq!(catalog.scenes(), [scene]);
    assert!(catalog.addressable_roots().is_empty());
    assert_eq!(catalog.members().len(), 4);
    let inventory: [(AssetProductRef, &[RuntimeContentDependency]); 4] = [
        (scene, &[required(badge)]),
        (badge, &[RuntimeContentDependency::new(selected, role)]),
        (selected, &[required(image)]),
        (image, &[]),
    ];
    for (product, expected) in inventory {
        let member = catalog.member_for_product(product).unwrap();
        assert_eq!(member.products(), [product]);
        assert_eq!(member.dependencies(), expected);
    }
    assert!(base.member(product(UNUSED_ID)).is_none());
    let complete = base.validate_deep().unwrap();
    assert_eq!(complete.members(), 4);
    assert_eq!(complete.products(), 4);
    assert_eq!(complete.catalog_digest(), base.catalog_digest());

    let decoded_image = base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image)
        .unwrap();
    assert_image(decoded_image.product());
    match stock {
        Stock::TileSet => {
            let acquired = base
                .acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(selected)
                .unwrap();
            let descriptor = acquired.product();
            assert_eq!(descriptor.product(), selected);
            assert_eq!(descriptor.image_product(), image);
            assert_eq!(descriptor.atlas(), atlas());
            descriptor
                .validate_image_extent(decoded_image.product().extent())
                .unwrap();
        }
        Stock::Material => {
            let acquired = base
                .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(selected)
                .unwrap();
            assert_material(acquired.product());
        }
    }
    drop(decoded_image);

    observe_cooked_badge(base, |world, badge| {
        assert!(
            matches!(&badge.art, BadgeArt::Stock(dependencies)
                if dependencies.as_ref() == [(selected, demand)]),
            "unexpected cooked Badge: {:?}",
            badge.art
        );
        assert!(!world.contains_resource::<ProjectAssetDatabase>());
        let published_image = published::<ImageAsset>(world, image, immediate);
        if let Some((_, image)) = published_image {
            assert_image(image);
        }
        let domains = world.resource::<ProjectDomainAssets>();
        let reference = AssetRef::Product(selected);
        let resident = match stock {
            Stock::TileSet => {
                if let Some((_, tileset)) = published::<TileSet>(world, selected, immediate) {
                    assert_eq!(tileset.atlas(), Some(atlas()));
                    assert_eq!(
                        tileset.material().image,
                        published_image.map(|(handle, _)| handle)
                    );
                }
                domains
                    .collection::<TileSetDomain>()
                    .is_some_and(|assets| assets.get(&reference).is_some())
            }
            Stock::Material => {
                if let Some((_, material)) =
                    published::<Material2dAsset>(world, selected, immediate)
                {
                    assert_material(material);
                }
                domains
                    .collection::<Material2dDomain>()
                    .is_some_and(|assets| assets.get(&reference).is_some())
            }
        };
        assert_eq!(resident, immediate);
    });
}

fn published<T: Asset>(
    world: &World,
    product: AssetProductRef,
    present: bool,
) -> Option<(Handle<T>, &T)> {
    let server = world.resource::<AssetServer>();
    let assets = world.resource::<Assets<T>>();
    let handle = server.get_product_handle::<T>(product);
    assert_eq!(
        handle.is_some(),
        present,
        "unexpected publication for {product}"
    );
    assert_eq!(assets.len(), usize::from(present));
    handle.map(|handle| {
        assert!(server.path(handle.id()).is_none());
        (handle, assets.get(handle).unwrap())
    })
}

fn assert_image(image: &ImageAsset) {
    assert_eq!(image.product(), Some(product(STOCK_IMAGE_ID)));
    assert!(image.source().is_none());
    assert_eq!(image.extent(), ImageExtent::new(1, 1));
    assert_eq!(image.pixels(), [232, 64, 24, 255]);
}

fn assert_material(material: &Material2dAsset) {
    assert_eq!(material.product(), product(STOCK_ID));
    assert_eq!(material.textures().len(), 1);
    assert_eq!(
        material.textures()["palette"],
        AssetRef::Product(product(STOCK_IMAGE_ID))
    );
    let generation = material.generation();
    assert_eq!(generation.declaration().id(), MATERIAL_ID);
    assert_eq!(generation.declaration().version(), 1);
    assert!(generation.inputs().is_empty());
    assert_eq!(
        generation.shader().user_source().strip_suffix('\n'),
        Some(SHADER)
    );
}

fn atlas() -> TileAtlasLayout {
    TileAtlasLayout::grid(Vec2::ONE, 1, 1)
}

fn palette_source(image: AssetRef) -> Material2dSource {
    let parameters = [
        ("palette.offset", "x", 0, 0.0),
        ("palette.strength", "y", 4, 1.0),
        ("palette.reserved-z", "z", 8, 0.0),
        ("palette.reserved-w", "w", 12, 0.0),
    ];
    let layout = MaterialParameterLayout::new(
        parameters
            .into_iter()
            .map(|(id, name, offset, _)| {
                MaterialUniformField::new(id, name, MaterialParameterKind::F32, offset).unwrap()
            })
            .collect(),
        16,
    )
    .unwrap();
    let declaration = Material2dDeclaration::new(
        MATERIAL_ID,
        1,
        AlphaMode2d::Blend,
        layout,
        vec![MaterialTextureDeclaration::new("palette", "material_texture", 1).unwrap()],
    )
    .unwrap();
    Material2dSource::new(
        declaration,
        vec![AssetPath::new(SHADER_PATH).unwrap()],
        parameters
            .into_iter()
            .map(|(id, _, _, value)| (id.to_owned(), MaterialParameterValue::F32(value)))
            .collect(),
        BTreeMap::from([("palette".to_owned(), image)]),
    )
    .unwrap()
}
