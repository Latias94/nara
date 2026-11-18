use std::collections::BTreeMap;

use nara::{
    asset::{AssetRef, RuntimeAssetCodec},
    image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat},
    material::{Material2dAsset, Material2dRuntimeCodec},
    runtime_content::{
        RuntimeContentDependencyRole, RuntimeContentMember, RuntimeContentMemberKind,
    },
    scene::{SourceDocumentKind, SourceDocumentOrigin},
};
use two_level_game::{
    animation::{atlas_product, clips_product},
    levels::LevelId,
    markers::marker_product,
    region_material, side_room,
    sound_bank::bank_product,
};

pub fn assert_level_dependencies(member: &RuntimeContentMember, level: LevelId) {
    let mut expected = BTreeMap::from([
        (
            level.image_product(),
            RuntimeContentDependencyRole::Required,
        ),
        (atlas_product(), RuntimeContentDependencyRole::Required),
        (bank_product(), RuntimeContentDependencyRole::Required),
        (clips_product(), RuntimeContentDependencyRole::Required),
        (marker_product(), RuntimeContentDependencyRole::Required),
    ]);
    if level == LevelId::Meadow {
        for product in [
            side_room::scene_product(),
            side_room::lookout_scene_product(),
        ] {
            expected.insert(product, RuntimeContentDependencyRole::Reference);
        }
    }
    assert_eq!(member.dependencies().len(), expected.len());
    assert_eq!(
        member
            .dependencies()
            .iter()
            .map(|edge| (edge.product(), edge.role()))
            .collect::<BTreeMap<_, _>>(),
        expected,
    );
}

pub fn assert_room_member(member: &RuntimeContentMember) {
    let (_, scene_path) = [
        (side_room::scene_product(), side_room::SCENE_PATH),
        (
            side_room::lookout_scene_product(),
            side_room::LOOKOUT_SCENE_PATH,
        ),
    ]
    .into_iter()
    .find(|(product, _)| member.products() == [*product])
    .expect("expected one authored Meadow region Product");
    let RuntimeContentMemberKind::Scene { source } = member.kind() else {
        panic!("region requires a processed Scene member");
    };
    assert_eq!(source.kind(), SourceDocumentKind::Scene);
    let SourceDocumentOrigin::ProjectLocation { universe, path } = source.origin() else {
        panic!("region retains its ProjectLocation identity");
    };
    assert_eq!(universe.to_string(), "be3dea0c-93ef-5607-ab84-af715a1f98b3");
    assert_eq!(path.as_str(), format!("scenes/{scene_path}"));
    assert_eq!(member.dependencies().len(), 3);
    assert_eq!(
        member
            .dependencies()
            .iter()
            .map(|edge| (edge.product(), edge.role()))
            .collect::<BTreeMap<_, _>>(),
        BTreeMap::from([
            (
                side_room::image_product(),
                RuntimeContentDependencyRole::Required
            ),
            (
                LevelId::Meadow.image_product(),
                RuntimeContentDependencyRole::Required
            ),
            (
                region_material::material_product(),
                RuntimeContentDependencyRole::Required
            ),
        ]),
    );
}

pub fn assert_room_material_member(member: &RuntimeContentMember, material: &Material2dAsset) {
    assert_eq!(member.products(), [region_material::material_product()]);
    assert!(matches!(
        member.kind(),
        RuntimeContentMemberKind::Asset { format, version }
            if format.as_str() == Material2dRuntimeCodec::RUNTIME_KIND
                && version.get() == Material2dRuntimeCodec::RUNTIME_VERSION
    ));
    let mut expected = BTreeMap::new();
    Material2dRuntimeCodec::visit_runtime_dependencies(material, |product, demand| {
        assert_eq!(demand, nara::asset::AssetReferenceDemand::Immediate);
        expected.insert(product, RuntimeContentDependencyRole::Required);
        Ok::<_, std::convert::Infallible>(())
    })
    .unwrap();
    assert_eq!(
        expected,
        BTreeMap::from([(
            region_material::palette_product(),
            RuntimeContentDependencyRole::Required,
        )])
    );
    assert_eq!(member.dependencies().len(), expected.len());
    assert_eq!(
        member
            .dependencies()
            .iter()
            .map(|edge| (edge.product(), edge.role()))
            .collect::<BTreeMap<_, _>>(),
        expected,
    );
    assert_room_material(material);
}

pub fn assert_room_material(material: &Material2dAsset) {
    assert_eq!(material.product(), region_material::material_product());
    assert_eq!(
        material.generation().declaration(),
        &region_material::declaration()
    );
    assert_eq!(
        material.default_parameters(),
        &region_material::default_parameters()
    );
    assert!(material.generation().inputs().is_empty());
    assert_eq!(material.generation().sequence(), 0);
    assert_eq!(
        material.generation().shader().user_source(),
        format!("{}\n", region_material::SHADER)
    );
    assert_eq!(
        material.textures(),
        &BTreeMap::from([(
            region_material::PALETTE_ID.to_owned(),
            AssetRef::Product(region_material::palette_product()),
        )])
    );
}

pub fn assert_room_image(image: &ImageAsset) {
    assert_eq!(image.product(), Some(side_room::image_product()));
    assert_eq!(image.extent(), ImageExtent::new(2, 2));
    assert_eq!(image.format(), ImageFormat::Rgba8);
    assert_eq!(image.color_space(), ImageColorSpace::Srgb);
    assert_eq!(image.pixels(), [64, 224, 180, 255].repeat(4));
    assert!(image.source().is_none());
}
