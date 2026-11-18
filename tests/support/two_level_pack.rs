use std::collections::BTreeMap;

use nara::{
    image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat},
    runtime_content::{
        RuntimeContentDependencyRole, RuntimeContentMember, RuntimeContentMemberKind,
    },
    scene::SourceDocumentOrigin,
};
use two_level_game::{
    animation::{atlas_product, clips_product},
    levels::LevelId,
    markers::marker_product,
    side_room,
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
        expected.insert(
            side_room::scene_product(),
            RuntimeContentDependencyRole::Reference,
        );
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
    assert_eq!(member.products(), [side_room::scene_product()]);
    let RuntimeContentMemberKind::Scene { source } = member.kind() else {
        panic!("side room requires a processed Scene member");
    };
    let SourceDocumentOrigin::ProjectLocation { universe, path } = source.origin() else {
        panic!("side room retains its ProjectLocation identity");
    };
    assert_eq!(universe.to_string(), "be3dea0c-93ef-5607-ab84-af715a1f98b3");
    assert_eq!(path.as_str(), format!("scenes/{}", side_room::SCENE_PATH));
    assert_eq!(member.dependencies().len(), 2);
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
        ]),
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
