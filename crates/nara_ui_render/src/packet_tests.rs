use std::sync::Arc;

use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetId, AssetPath, AssetVersion, Assets, Handle,
    ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest, ImportProfile,
    ImportSettingsHash, ImportedAssetType, ImporterId, ImporterVersion, SourceHash, StableAssetId,
};
use nara_image::{
    ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageSourceMetadata,
    PreparedImageResource, PreparedImageSnapshot, image_descriptor_hash, image_resource_key,
};
use nara_render::{RenderResourceKey, RenderResourceKind, RenderResourceSnapshot, RenderTarget};
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiBorder, UiBrush, UiColor, UiCompiler, UiCornerRadii,
    UiEnvironment, UiImageId, UiLayer, UiLocalKey, UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId,
    UiText, UiTheme, UiVisual,
};
use nara_window::WindowId;

use crate::{
    UiPacketFill, UiPacketPrimitive, UiPacketUv, UiPreparedImages, UiPresentationContentError,
    UiPresentationLimits, UiPresentationPlanner,
};

fn frame(surface: u64, revision: u64, nodes: Vec<UiNode>) -> Arc<nara_ui_kernel::UiFrame> {
    let snapshot = UiAuthoringSnapshotBuilder::new(revision)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("packet-test", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_children(nodes),
        )
        .build()
        .unwrap();
    let environment = UiEnvironment::new(
        UiSurfaceId::new(surface).unwrap(),
        320.0,
        180.0,
        1.0,
        revision,
        1,
        1,
        1,
    )
    .unwrap();
    UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap()
}

fn text_node(key: &str, text: &str, visual: Option<UiVisual>) -> UiNode {
    let mut node = UiNode::new(UiStyle::absolute(8.0, 6.0, 280.0, 40.0))
        .with_key(UiLocalKey::runtime("packet-test", key).unwrap())
        .with_text(UiText::new(text, 18.0).unwrap());
    if let Some(visual) = visual {
        node = node.with_visual(visual);
    }
    node
}

fn solid_node(key: &str, x: f32) -> UiNode {
    UiNode::new(UiStyle::absolute(x, 0.0, 32.0, 32.0))
        .with_key(UiLocalKey::runtime("packet-test", key).unwrap())
        .with_visual(UiVisual::solid(UiColor::rgba(0.1, 0.2, 0.3, 1.0)))
}

fn captured_image_snapshot() -> PreparedImageSnapshot {
    let stable_id = StableAssetId::parse_str("d7961f69-0e25-41d5-8893-0ef85ab19615").unwrap();
    let source_hash = SourceHash::from_bytes(b"UI planner image fixture");
    let artifact = ImportArtifactRecord::new(ImportArtifactKey::new(
        stable_id,
        source_hash,
        ImportDependencyDigest::empty(),
        ImporterId::new("nara-ui-planner-test").unwrap(),
        ImporterVersion::new(1),
        ImportSettingsHash::default(),
        ImportProfile::default(),
        ImportedAssetType::new("image").unwrap(),
        ArtifactLabel::default(),
        ArtifactFormatVersion::new(1),
    ))
    .unwrap();
    let image = ImageAsset::new(
        ImageSourceMetadata::new(
            stable_id,
            AssetPath::new("ui/planner.png").unwrap(),
            source_hash,
            artifact,
        ),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![240, 16, 120, 255],
    )
    .unwrap();
    let handle = Handle::new(AssetId::from_raw(17));
    let mut images = Assets::<ImageAsset>::default();
    assert!(images.insert(handle, image).is_none());
    let image = images.get(handle).unwrap();
    let identity = RenderResourceSnapshot::new(
        image_resource_key(handle),
        AssetVersion::from_raw(1),
        images.slot_revision(handle),
        image_descriptor_hash(image),
    );
    PreparedImageSnapshot::new(identity, PreparedImageResource::from_image(image))
}

fn captured_image_snapshot_with_kind(kind: RenderResourceKind) -> PreparedImageSnapshot {
    let snapshot = captured_image_snapshot();
    let identity = RenderResourceSnapshot::new(
        RenderResourceKey::new(snapshot.identity().key().asset_id(), kind),
        snapshot.identity().asset_version(),
        snapshot.identity().slot_revision().clone(),
        snapshot.identity().descriptor_hash(),
    );
    PreparedImageSnapshot::new(identity, snapshot.resource().clone())
}

#[test]
fn packet_preserves_rect_then_nonblank_glyph_order() {
    let packet = UiPresentationPlanner::default()
        .plan(
            frame(
                1,
                1,
                vec![text_node(
                    "label",
                    "Nara",
                    Some(UiVisual::solid(UiColor::rgba(0.1, 0.2, 0.3, 1.0))),
                )],
            ),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();

    assert!(matches!(
        packet.primitives().first(),
        Some(UiPacketPrimitive::Rect { .. })
    ));
    assert!(
        packet
            .primitives()
            .iter()
            .skip(1)
            .any(|primitive| matches!(primitive, UiPacketPrimitive::Glyph { .. }))
    );
}

#[test]
fn packet_uv_contract_uses_top_left_origin_and_positive_extent() {
    assert_eq!(UiPacketUv::FULL.top_left(), [0.0, 0.0]);
    assert_eq!(UiPacketUv::FULL.extent(), [1.0, 1.0]);

    let packet = UiPresentationPlanner::default()
        .plan(
            frame(1, 1, vec![text_node("label", "Nara", None)]),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();
    let uv = packet
        .primitives()
        .iter()
        .find_map(|primitive| match primitive {
            UiPacketPrimitive::Glyph { uv, .. } => Some(*uv),
            UiPacketPrimitive::Rect { .. } => None,
        })
        .unwrap();
    assert!(
        uv.top_left()
            .into_iter()
            .all(|coordinate| coordinate >= 0.0)
    );
    assert!(uv.extent().into_iter().all(|length| length > 0.0));
    assert!(uv.top_left()[0] + uv.extent()[0] <= 1.0);
    assert!(uv.top_left()[1] + uv.extent()[1] <= 1.0);
}

#[test]
fn uniform_radius_and_border_remain_one_rect_primitive() {
    let visual = UiVisual::solid(UiColor::rgba(0.2, 0.3, 0.4, 1.0))
        .with_border(UiBorder::new(2.0, UiBrush::Solid(UiColor::rgba(0.8, 0.7, 0.6, 1.0))).unwrap())
        .with_radii(UiCornerRadii::all(6.0).unwrap());
    let node = UiNode::new(UiStyle::absolute(0.0, 0.0, 40.0, 30.0))
        .with_key(UiLocalKey::runtime("packet-test", "rounded").unwrap())
        .with_visual(visual);
    let limits = UiPresentationLimits::default()
        .with_max_expanded_primitives(1)
        .unwrap();
    let packet = UiPresentationPlanner::new(limits)
        .unwrap()
        .plan(
            frame(1, 1, vec![node]),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();

    assert_eq!(packet.primitives().len(), 1);
    let UiPacketPrimitive::Rect {
        fill,
        border,
        corner_radius,
        ..
    } = &packet.primitives()[0]
    else {
        panic!("rounded visual must close into one rect")
    };
    assert!(matches!(fill, Some(UiPacketFill::Solid { .. })));
    assert_eq!(border.unwrap().width(), 2.0);
    assert_eq!(*corner_radius, 6.0);
}

#[test]
fn primitive_budget_accepts_exact_limit_and_rejects_limit_plus_one() {
    let nodes = vec![solid_node("one", 0.0), solid_node("two", 40.0)];
    let exact_limits = UiPresentationLimits::default()
        .with_max_expanded_primitives(2)
        .unwrap();
    UiPresentationPlanner::new(exact_limits)
        .unwrap()
        .plan(
            frame(1, 1, nodes.clone()),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();

    let limited = UiPresentationLimits::default()
        .with_max_expanded_primitives(1)
        .unwrap();
    let error = UiPresentationPlanner::new(limited)
        .unwrap()
        .plan(
            frame(1, 1, nodes),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        UiPresentationContentError::PrimitiveLimitExceeded { limit: 1 }
    );
}

#[test]
fn duplicate_image_and_glyph_references_consume_one_reference_each() {
    let first_image_id = UiImageId::new("project://ui/planner.png").unwrap();
    let second_image_id = UiImageId::new("stable:planner-image-alias").unwrap();
    let mut images = UiPreparedImages::default();
    let snapshot = captured_image_snapshot();
    images.insert(first_image_id.clone(), snapshot.clone());
    images.insert(second_image_id.clone(), snapshot);
    let limits = UiPresentationLimits::default()
        .with_max_referenced_images(1)
        .unwrap()
        .with_max_referenced_glyphs(1)
        .unwrap();
    let first_visual = UiVisual::image(first_image_id, UiColor::rgba(1.0, 1.0, 1.0, 1.0));
    let second_visual = UiVisual::image(second_image_id, UiColor::rgba(1.0, 1.0, 1.0, 1.0));
    let packet = UiPresentationPlanner::new(limits)
        .unwrap()
        .plan(
            frame(
                1,
                1,
                vec![
                    text_node("one", "A", Some(first_visual)),
                    text_node("two", "A", Some(second_visual)),
                ],
            ),
            RenderTarget::Window(WindowId::PRIMARY),
            &images,
        )
        .unwrap();

    assert_eq!(packet.images().len(), 1);
    let image_indices = packet
        .primitives()
        .iter()
        .filter_map(|primitive| match primitive {
            UiPacketPrimitive::Rect {
                fill: Some(UiPacketFill::Image { image_index, .. }),
                ..
            } => Some(*image_index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(image_indices, vec![0, 0]);
}

#[test]
fn non_image_resource_identity_is_rejected_before_packet_commit() {
    let image_id = UiImageId::new("project://ui/not-an-image.png").unwrap();
    let mut images = UiPreparedImages::default();
    images.insert(
        image_id.clone(),
        captured_image_snapshot_with_kind(RenderResourceKind::new("not_image")),
    );
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let mut planner = UiPresentationPlanner::default();
    let first = planner
        .plan(
            frame(1, 1, vec![solid_node("first", 0.0)]),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();

    let error = planner
        .plan(
            frame(
                1,
                2,
                vec![
                    UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 32.0))
                        .with_key(UiLocalKey::runtime("packet-test", "invalid-kind").unwrap())
                        .with_visual(UiVisual::image(image_id, UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
                ],
            ),
            target,
            &images,
        )
        .unwrap_err();

    assert!(matches!(
        error,
        UiPresentationContentError::InvalidImageResourceKind { .. }
    ));
    assert!(Arc::ptr_eq(&planner.last_good(target).unwrap(), &first));
}

#[test]
fn missing_image_preflight_does_not_stage_glyphs_or_replace_last_good() {
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let mut planner = UiPresentationPlanner::default();
    let first = planner
        .plan(
            frame(1, 1, vec![text_node("label", "A", None)]),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    let missing = UiImageId::new("project://ui/missing.png").unwrap();
    let error = planner
        .plan(
            frame(
                1,
                2,
                vec![text_node(
                    "label",
                    "B",
                    Some(UiVisual::image(missing, UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
                )],
            ),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        UiPresentationContentError::MissingImage { .. }
    ));
    assert!(Arc::ptr_eq(&planner.last_good(target).unwrap(), &first));

    let replay = planner
        .plan(
            frame(1, 3, vec![text_node("label", "A", None)]),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    assert_eq!(first.glyph_pages()[0].key(), replay.glyph_pages()[0].key());
    assert!(first.glyph_pages()[0].shares_storage_with(&replay.glyph_pages()[0]));
}

#[test]
fn target_private_state_is_isolated_and_released_only_by_explicit_retirement() {
    let first_target = RenderTarget::Window(WindowId::PRIMARY);
    let second_target = RenderTarget::Window(WindowId::new(2));
    let mut planner = UiPresentationPlanner::default();
    let first = planner
        .plan(
            frame(1, 1, vec![text_node("first", "A", None)]),
            first_target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    let second = planner
        .plan(
            frame(2, 1, vec![text_node("second", "B", None)]),
            second_target,
            &UiPreparedImages::default(),
        )
        .unwrap();

    assert!(Arc::ptr_eq(
        &planner.last_good(first_target).unwrap(),
        &first
    ));
    assert!(Arc::ptr_eq(
        &planner.last_good(second_target).unwrap(),
        &second
    ));
    assert!(Arc::ptr_eq(
        &planner.retire_target(first_target).unwrap(),
        &first
    ));
    assert!(planner.last_good(first_target).is_none());
    assert!(Arc::ptr_eq(
        &planner.last_good(second_target).unwrap(),
        &second
    ));
}

#[test]
fn packet_snapshots_have_a_deterministic_surface_order() {
    let mut planner = UiPresentationPlanner::default();
    planner
        .plan(
            frame(2, 1, vec![solid_node("second", 0.0)]),
            RenderTarget::Window(WindowId::new(2)),
            &UiPreparedImages::default(),
        )
        .unwrap();
    planner
        .plan(
            frame(1, 1, vec![solid_node("first", 0.0)]),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();

    let surfaces = planner
        .snapshot_packets()
        .into_iter()
        .map(|packet| packet.identity().surface_id())
        .collect::<Vec<_>>();
    assert_eq!(
        surfaces,
        vec![UiSurfaceId::new(1).unwrap(), UiSurfaceId::new(2).unwrap()]
    );
}

#[test]
fn packet_retains_only_atlas_pages_referenced_by_its_glyphs() {
    let limits = UiPresentationLimits::default()
        .with_atlas_page_extent(64)
        .unwrap();
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let mut planner = UiPresentationPlanner::new(limits).unwrap();
    let populated = planner
        .plan(
            frame(
                1,
                1,
                vec![text_node(
                    "many",
                    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                    None,
                )],
            ),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    assert!(populated.glyph_pages().len() > 1);

    let focused = planner
        .plan(
            frame(1, 2, vec![text_node("one", "A", None)]),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    assert_eq!(focused.glyph_pages().len(), 1);
}
