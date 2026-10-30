use std::{collections::BTreeSet, sync::Arc};

use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetEvents, AssetPathScope, AssetProductRef,
    AssetRecord, AssetServer, AssetSourceKind, AssetSourcePath, AssetStates, Assets,
    ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest, ImportProfile,
    ImportedAssetType, ImportedProductId, ImporterId, ImporterVersion, SourceHash, StableAssetId,
};
use nara_image::{
    ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageImportSettings,
    ImageSourceMetadata, PreparedImageSnapshot, PreparedImages,
};
use nara_render::RenderTarget;
use nara_text::{FontFace, TextEngine, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiBorder, UiBrush, UiColor, UiCompiler, UiCornerRadii, UiEdges,
    UiEnvironment, UiImageId, UiLayer, UiLocalKey, UiNode, UiOverflow, UiPaintPrimitive, UiPoint,
    UiRoot, UiRootKey, UiSafeAreaInsets, UiSize, UiStyle, UiSurfaceContentTransform,
    UiSurfaceGenerations, UiSurfaceId, UiText, UiTextOverflow, UiTheme, UiThemePalette, UiVisual,
};
use nara_window::WindowId;

use crate::{
    UiPacketFill, UiPacketPrimitive, UiPacketUv, UiPreparedImages, UiPresentationContentError,
    UiPresentationLimits, UiPresentationPlanner,
};

fn frame(surface: u64, revision: u64, nodes: Vec<UiNode>) -> Arc<nara_ui_kernel::UiFrame> {
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
    frame_with_environment(revision, nodes, environment)
}

fn frame_with_environment(
    revision: u64,
    nodes: Vec<UiNode>,
    environment: UiEnvironment,
) -> Arc<nara_ui_kernel::UiFrame> {
    frame_with_font(revision, nodes, environment, bundled_font_face().unwrap())
}

fn frame_with_font(
    revision: u64,
    nodes: Vec<UiNode>,
    environment: UiEnvironment,
    font: FontFace,
) -> Arc<nara_ui_kernel::UiFrame> {
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
    let theme = UiTheme::new(
        environment.theme_revision().get(),
        UiThemePalette::default(),
    )
    .unwrap();
    UiCompiler::new(font, Default::default())
        .unwrap()
        .compile(&snapshot, environment, theme)
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

fn image_asset(pixels: [u8; 4]) -> ImageAsset {
    let stable_id = StableAssetId::parse_str("d7961f69-0e25-41d5-8893-0ef85ab19615").unwrap();
    let source_hash = SourceHash::from_bytes(pixels);
    let artifact = ImportArtifactRecord::new(ImportArtifactKey::new(
        stable_id,
        source_hash,
        ImportDependencyDigest::empty(),
        ImporterId::new("nara-ui-planner-test").unwrap(),
        ImporterVersion::new(1),
        ImageImportSettings::default().settings_hash(),
        ImportProfile::default(),
        ImportedAssetType::new("image").unwrap(),
        ArtifactLabel::default(),
        ArtifactFormatVersion::new(1),
    ))
    .unwrap();
    ImageAsset::from_import(
        ImageSourceMetadata::new(
            AssetSourcePath::new("ui/planner.png").unwrap(),
            ImageImportSettings::default(),
            artifact,
        )
        .unwrap(),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        pixels.to_vec(),
    )
    .unwrap()
}

fn captured_image_snapshot() -> PreparedImageSnapshot {
    let handle = nara_asset::allocate_anonymous_handle().unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    images
        .commit_loaded(
            handle,
            image_asset([240, 16, 120, 255]),
            &mut states,
            &mut events,
            None,
            None,
        )
        .unwrap();
    PreparedImageSnapshot::capture(handle, &images, &states).unwrap()
}

#[test]
fn custom_font_shapes_and_plans_nonblank_glyphs() {
    let font = FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    assert_ne!(font.id(), bundled_font_face().unwrap().id());
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 320.0, 180.0, 1.0, 1, 1, 1, 1).unwrap();
    let candidate = frame_with_font(
        1,
        vec![text_node("custom-label", "Nara mono", None)],
        environment,
        font.clone(),
    );
    let packet = UiPresentationPlanner::with_font(font, Default::default())
        .unwrap()
        .plan(
            candidate,
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .expect("an admitted custom face must support ordinary glyph presentation");

    assert!(
        packet
            .primitives()
            .iter()
            .any(|primitive| matches!(primitive, UiPacketPrimitive::Glyph { .. }))
    );
    assert!(
        packet
            .glyph_pages()
            .iter()
            .any(|page| page.pixels().iter().any(|coverage| *coverage != 0))
    );
}

#[test]
fn foreign_font_is_rejected_before_atlas_mutation_and_last_good_replay_survives() {
    let custom = FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let mut planner = UiPresentationPlanner::with_font(custom.clone(), Default::default()).unwrap();
    let candidate = frame_with_font(
        1,
        vec![text_node("label", "Nara mono", None)],
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 320.0, 180.0, 1.0, 1, 1, 1, 1).unwrap(),
        custom.clone(),
    );
    let first = planner
        .plan(Arc::clone(&candidate), target, &UiPreparedImages::default())
        .unwrap();
    let submission = crate::packet::UiPreparedBackendSubmission::new(
        crate::UiPresentationAttemptId::new(1).unwrap(),
        Arc::clone(&first),
        crate::UiPresentationEpoch::new(1).unwrap(),
        crate::UiPresentationRuntimeGeneration::new(1).unwrap(),
        crate::UiPresentationCapabilityGeneration::new(1).unwrap(),
    );
    planner
        .replace_backend_submissions(std::slice::from_ref(&submission), &[])
        .unwrap();
    planner.commit_presented(submission.identity(), Arc::clone(&first));
    let error = planner
        .plan(
            frame(1, 2, vec![text_node("label", "Nara", None)]),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        UiPresentationContentError::FontFaceMismatch {
            expected: custom.id(),
            actual: bundled_font_face().unwrap().id(),
        }
    );
    assert!(Arc::ptr_eq(&planner.planned(target).unwrap(), &first));
    assert!(Arc::ptr_eq(
        &planner.last_presented(target).unwrap(),
        &first
    ));

    let replay = planner
        .plan(candidate, target, &UiPreparedImages::default())
        .unwrap();
    assert_eq!(first.glyph_pages()[0].key(), replay.glyph_pages()[0].key());
    assert!(first.glyph_pages()[0].shares_storage_with(&replay.glyph_pages()[0]));
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
fn padded_text_packet_matches_an_equivalent_unpadded_content_box() {
    for scale in [1.0, 1.25] {
        let environment = UiEnvironment::new(
            UiSurfaceId::new(1).unwrap(),
            320.0,
            180.0,
            scale,
            1,
            1,
            1,
            1,
        )
        .unwrap();
        let text = UiText::new("One two three four five six seven eight", 16.0).unwrap();
        let padded = UiNode::new(
            UiStyle::absolute(10.0, 10.0, 100.0, 150.0)
                .with_padding(UiEdges::symmetric(20.0, 7.0).unwrap()),
        )
        .with_key(UiLocalKey::runtime("packet-test", "padded").unwrap())
        .with_text(text.clone());
        let content = UiNode::new(UiStyle::absolute(30.0, 17.0, 60.0, 136.0))
            .with_key(UiLocalKey::runtime("packet-test", "content").unwrap())
            .with_text(text);
        let padded_frame = frame_with_environment(1, vec![padded], environment);
        let content_frame = frame_with_environment(1, vec![content], environment);
        assert!(padded_frame.diagnostics().is_empty());
        assert!(content_frame.diagnostics().is_empty());
        let UiPaintPrimitive::Text {
            layout: padded_layout,
            ..
        } = &padded_frame.paint()[0]
        else {
            panic!("the padded node must publish text")
        };
        let UiPaintPrimitive::Text {
            layout: content_layout,
            ..
        } = &content_frame.paint()[0]
        else {
            panic!("the content node must publish text")
        };
        assert!(padded_layout.lines().len() > 1);
        assert_eq!(padded_layout, content_layout);

        let mut planner = UiPresentationPlanner::default();
        let padded_packet = planner
            .plan(
                padded_frame,
                RenderTarget::Window(WindowId::PRIMARY),
                &UiPreparedImages::default(),
            )
            .unwrap();
        let content_packet = planner
            .plan(
                content_frame,
                RenderTarget::Window(WindowId::PRIMARY),
                &UiPreparedImages::default(),
            )
            .unwrap();
        let glyph_boxes = |packet: &crate::UiPaintPacket| {
            packet
                .primitives()
                .iter()
                .filter_map(|primitive| match primitive {
                    UiPacketPrimitive::Glyph { rect, clip, .. } => Some((*rect, *clip)),
                    UiPacketPrimitive::Rect { .. } => None,
                })
                .collect::<Vec<_>>()
        };
        let padded_glyphs = glyph_boxes(&padded_packet);
        assert!(!padded_glyphs.is_empty());
        assert_eq!(padded_glyphs, glyph_boxes(&content_packet));
    }
}

#[test]
fn padded_text_packet_preserves_nested_clip_at_fractional_scale() {
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 320.0, 180.0, 1.25, 1, 1, 1, 1).unwrap();
    let child = UiNode::new(
        UiStyle::absolute(40.0, 0.0, 100.0, 55.0)
            .with_padding(UiEdges::symmetric(20.0, 5.0).unwrap()),
    )
    .with_key(UiLocalKey::runtime("packet-test", "nested-padded").unwrap())
    .with_visual(UiVisual::solid(UiColor::rgba(0.1, 0.2, 0.3, 1.0)))
    .with_text(
        UiText::new("Mmmmmmmmmmm", 16.0)
            .unwrap()
            .with_overflow(UiTextOverflow::Clip),
    );
    let parent =
        UiNode::new(UiStyle::absolute(10.0, 10.0, 80.0, 55.0).with_overflow(UiOverflow::Clip))
            .with_child(child);
    let candidate = frame_with_environment(1, vec![parent], environment);
    assert!(candidate.diagnostics().is_empty());
    let content_clip = nara_ui_kernel::UiRect::from_origin_size(70.0, 15.0, 20.0, 45.0).unwrap();
    let expected_clip = environment.physical_rect(content_clip).unwrap();
    let packet = UiPresentationPlanner::default()
        .plan(
            candidate,
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();
    let mut glyphs = 0;
    for primitive in packet.primitives() {
        let UiPacketPrimitive::Glyph { clip, .. } = primitive else {
            continue;
        };
        let clip = clip.expect("nested text must retain its clip");
        assert!(clip.min_x() >= expected_clip.min_x());
        assert!(clip.min_y() >= expected_clip.min_y());
        assert!(clip.max_x() <= expected_clip.max_x());
        assert!(clip.max_y() <= expected_clip.max_y());
        glyphs += 1;
    }
    assert!(glyphs > 0);
    let UiPacketPrimitive::Rect { rect, .. } = packet.primitives()[0] else {
        panic!("the background must precede glyphs")
    };
    assert!(rect.min_x() < expected_clip.min_x());
}

#[test]
fn packet_lowering_uses_the_exact_letterboxed_surface_transform() {
    let surface = UiSurfaceId::new(9).unwrap();
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(100.0, 100.0).unwrap(),
        UiSize::new(300.0, 200.0).unwrap(),
        UiSafeAreaInsets::new(10.0, 0.0, 10.0, 0.0).unwrap(),
        1.25,
    )
    .unwrap();
    let generations = UiSurfaceGenerations::new(7, 8, 9, 10).unwrap();
    let environment = UiEnvironment::for_surface(surface, transform, generations, 11, 12).unwrap();
    let clipped_visual = UiNode::new(UiStyle::absolute(80.0, 10.0, 60.0, 30.0))
        .with_key(UiLocalKey::runtime("packet-test", "letterboxed").unwrap())
        .with_visual(UiVisual::solid(UiColor::rgba(0.2, 0.3, 0.4, 1.0)));
    let text = UiNode::new(UiStyle::absolute(10.0, 50.0, 60.0, 20.0))
        .with_key(UiLocalKey::runtime("packet-test", "letterboxed-text").unwrap())
        .with_text(UiText::new("Nara", 12.0).unwrap());

    let candidate = frame_with_environment(1, vec![clipped_visual, text], environment);
    let packet = UiPresentationPlanner::default()
        .plan(
            candidate,
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();

    assert_eq!(packet.identity().slot_generation().get(), 7);
    assert_eq!(packet.identity().geometry_generation().get(), 9);
    let UiPacketPrimitive::Rect { rect, clip, .. } = packet.primitives().first().unwrap() else {
        panic!("the authored visual must lower to a rect")
    };
    // 100 x 100 authored content fits a 300 x 200 surface at scale 2 with x = 50 letterboxing;
    // the authored visual extends past the right edge and is clipped before physical snapping.
    assert_eq!(
        (rect.min_x(), rect.min_y(), rect.max_x(), rect.max_y()),
        (210, 20, 250, 80)
    );
    assert_eq!(*clip, Some(*rect));
    assert!(packet.primitives().iter().any(|primitive| match primitive {
        UiPacketPrimitive::Glyph { rect, clip, .. } => {
            rect.min_x() >= 50
                && rect.max_x() <= 250
                && clip.is_some_and(|clip| clip.min_x() >= 50 && clip.max_x() <= 250)
        }
        UiPacketPrimitive::Rect { .. } => false,
    }));
}

#[test]
fn visible_glyph_ink_survives_a_baseline_outside_the_surface() {
    let surface = UiSurfaceId::new(1).unwrap();
    let direct = UiEnvironment::new(surface, 100.0, 100.0, 1.0, 1, 1, 1, 1).unwrap();
    let letterboxed = UiEnvironment::for_surface(
        surface,
        UiSurfaceContentTransform::new(
            UiSize::new(100.0, 100.0).unwrap(),
            UiSize::new(340.0, 225.0).unwrap(),
            UiSafeAreaInsets::new(10.0, 0.0, 10.0, 0.0).unwrap(),
            1.25,
        )
        .unwrap(),
        UiSurfaceGenerations::new(1, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap();

    for environment in [direct, letterboxed] {
        let node_at = |top| {
            UiNode::new(UiStyle::absolute(10.0, top, 60.0, 32.0))
                .with_key(UiLocalKey::runtime("packet-test", "bottom-edge-text").unwrap())
                .with_text(UiText::new("M", 16.0).unwrap())
        };
        let probe = frame_with_environment(1, vec![node_at(0.0)], environment);
        let UiPaintPrimitive::Text { layout, .. } = &probe.paint()[0] else {
            panic!("the probe must compile display text")
        };
        assert_eq!(layout.glyphs().len(), 1);
        let transform = environment.content_transform();
        let scale = transform.content_scale_factor();
        let authored_height = transform.authored_logical_size().height();
        let top = authored_height - layout.glyphs()[0].y() + 0.5 / scale;
        let candidate = frame_with_environment(2, vec![node_at(top)], environment);
        assert!(candidate.diagnostics().is_empty());
        let UiPaintPrimitive::Text { rect, layout, .. } = &candidate.paint()[0] else {
            panic!("partially visible text must remain in the compiled frame")
        };
        let glyph = &layout.glyphs()[0];
        let glyph_y = glyph.y();
        let baseline = UiPoint::new(rect.origin().x() + glyph.x(), rect.origin().y() + glyph.y());
        assert!(baseline.y() >= authored_height);
        assert!(transform.authored_to_physical(baseline).is_none());
        let mut text = TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
        let raster = text.rasterize(glyph.key()).unwrap();
        assert!(raster.width() > 0 && raster.height() > 0);
        let physical_baseline_x =
            (transform.physical_content_origin().x() + baseline.x() * scale).round() as i32;
        let physical_baseline_y =
            (transform.physical_content_origin().y() + baseline.y() * scale).round() as i32;
        let visible = environment.physical_rect(*rect).unwrap();
        let has_visible_ink = raster.mask().iter().enumerate().any(|(index, alpha)| {
            let x = physical_baseline_x + raster.left() + (index % raster.width() as usize) as i32;
            let y = physical_baseline_y - raster.top() + (index / raster.width() as usize) as i32;
            *alpha != 0
                && x >= visible.min_x()
                && x < visible.max_x()
                && y >= visible.min_y()
                && y < visible.max_y()
        });
        assert!(
            has_visible_ink,
            "the regression must contain real visible raster ink"
        );
        let physical_bottom = (transform.physical_content_origin().y()
            + transform.physical_content_size().height())
        .round() as i32;
        assert!(physical_baseline_y >= physical_bottom);
        let packet = UiPresentationPlanner::default()
            .plan(
                candidate,
                RenderTarget::Window(WindowId::PRIMARY),
                &UiPreparedImages::default(),
            )
            .unwrap();
        assert!(
            packet.primitives().iter().any(|primitive| match primitive {
                UiPacketPrimitive::Glyph {
                    rect,
                    clip: Some(clip),
                    ..
                } => {
                    rect.min_y() < physical_bottom
                        && rect.max_y() > clip.min_y()
                        && clip.min_y() < physical_bottom
                        && clip.max_y() == physical_bottom
                }
                _ => false,
            }),
            "visible ink must produce a glyph clipped at the surface edge"
        );

        let outside_top = authored_height - glyph_y + (raster.top() + 2) as f32 / scale;
        assert!(
            outside_top < authored_height,
            "the outside glyph's node still intersects the surface"
        );
        let outside = frame_with_environment(3, vec![node_at(outside_top)], environment);
        let UiPaintPrimitive::Text { rect, layout, .. } = &outside.paint()[0] else {
            panic!("the outside glyph must still reach packet clipping")
        };
        assert!(environment.physical_rect(*rect).is_some());
        let outside_baseline = transform.physical_content_origin().y()
            + (rect.origin().y() + layout.glyphs()[0].y()) * scale;
        assert!(outside_baseline.round() as i32 - raster.top() >= physical_bottom);
        let outside_packet = UiPresentationPlanner::default()
            .plan(
                outside,
                RenderTarget::Window(WindowId::PRIMARY),
                &UiPreparedImages::default(),
            )
            .unwrap();
        assert!(
            outside_packet
                .primitives()
                .iter()
                .all(|primitive| !matches!(primitive, UiPacketPrimitive::Glyph { .. }))
        );
    }
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
fn automatic_image_capture_never_interprets_named_ids_or_other_provider_tokens() {
    struct OtherImageProvider;
    let snapshot = captured_image_snapshot();
    let token = snapshot.identity().key().asset_id().raw();
    let external = UiImageId::external::<ImageAsset>(token);
    let names = [
        UiImageId::new("ui/planner.png").unwrap(),
        UiImageId::new("product:d7961f69-0e25-41d5-8893-0ef85ab19615/primary").unwrap(),
        UiImageId::external::<OtherImageProvider>(token),
    ];
    let mut prepared = PreparedImages::default();
    prepared.insert(snapshot.clone());
    let referenced = names.iter().cloned().chain([external.clone()]).collect();
    let automatic = UiPreparedImages::capture(&referenced, Some(&prepared));
    for (index, opaque) in names.into_iter().enumerate() {
        let frame = frame(
            1,
            index as u64 + 1,
            vec![text_node(
                "opaque",
                "",
                Some(UiVisual::image(
                    opaque.clone(),
                    UiColor::rgba(1.0, 1.0, 1.0, 1.0),
                )),
            )],
        );
        assert!(matches!(
            UiPresentationPlanner::default().plan(
                Arc::clone(&frame),
                RenderTarget::Window(WindowId::PRIMARY),
                &automatic
            ),
            Err(UiPresentationContentError::MissingImage { .. })
        ));
        let mut explicit = UiPreparedImages::new();
        explicit.insert(opaque, snapshot.clone());
        let packet = UiPresentationPlanner::default()
            .plan(frame, RenderTarget::Window(WindowId::PRIMARY), &explicit)
            .unwrap();
        assert_eq!(packet.images()[0].snapshot(), &snapshot);
    }
    let packet = UiPresentationPlanner::default()
        .plan(
            frame(
                1,
                1,
                vec![text_node(
                    "external",
                    "",
                    Some(UiVisual::image(external, UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
                )],
            ),
            RenderTarget::Window(WindowId::PRIMARY),
            &automatic,
        )
        .unwrap();
    assert_eq!(packet.images()[0].snapshot(), &snapshot);
}

#[test]
fn planned_packets_retain_image_snapshots_across_successor_publication() {
    let mut asset_server = AssetServer::new();
    let handle = asset_server
        .reserve::<ImageAsset>("ui/planner.png")
        .unwrap();
    let image_id = UiImageId::external::<ImageAsset>(handle.id().raw());
    let referenced = BTreeSet::from([image_id.clone()]);
    let mut assets = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let mut prepared = PreparedImages::default();
    let first_pixels = [240, 16, 120, 255];
    let first_version = assets
        .commit_loaded(
            handle,
            image_asset(first_pixels),
            &mut states,
            &mut events,
            None,
            None,
        )
        .unwrap();
    let first_snapshot = PreparedImageSnapshot::capture(handle, &assets, &states).unwrap();
    prepared.insert(first_snapshot.clone());
    let first_images = UiPreparedImages::capture(&referenced, Some(&prepared));
    let visual = UiVisual::image(image_id, UiColor::rgba(1.0, 1.0, 1.0, 1.0));
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let mut planner = UiPresentationPlanner::default();
    let first = planner
        .plan(
            frame(1, 1, vec![text_node("label", "A", Some(visual.clone()))]),
            target,
            &first_images,
        )
        .unwrap();

    let successor_pixels = [16, 240, 120, 255];
    let successor_version = assets
        .commit_loaded(
            handle,
            image_asset(successor_pixels),
            &mut states,
            &mut events,
            None,
            None,
        )
        .unwrap();
    let successor_snapshot = PreparedImageSnapshot::capture(handle, &assets, &states).unwrap();
    prepared.insert(successor_snapshot.clone());
    let successor_images = UiPreparedImages::capture(&referenced, Some(&prepared));
    let successor = planner
        .plan(
            frame(1, 2, vec![text_node("label", "A", Some(visual))]),
            target,
            &successor_images,
        )
        .unwrap();

    drop(assets);
    drop(states);
    drop(asset_server);
    drop(prepared);
    drop(first_images);
    drop(successor_images);
    assert_eq!(first.images()[0].snapshot(), &first_snapshot);
    assert_eq!(successor.images()[0].snapshot(), &successor_snapshot);
    assert_eq!(
        first_snapshot.identity().key(),
        successor_snapshot.identity().key()
    );
    assert_eq!(first_snapshot.identity().asset_version(), first_version);
    assert_eq!(
        successor_snapshot.identity().asset_version(),
        successor_version
    );
    assert_ne!(first_version, successor_version);
    assert_ne!(
        first_snapshot.identity().slot_revision(),
        successor_snapshot.identity().slot_revision()
    );
    assert_ne!(
        first_snapshot.identity().content_hash(),
        successor_snapshot.identity().content_hash()
    );
    assert_eq!(
        first.images()[0].snapshot().resource().pixels(),
        &first_pixels
    );
    assert_eq!(
        successor.images()[0].snapshot().resource().pixels(),
        &successor_pixels
    );
    assert!(Arc::ptr_eq(&planner.planned(target).unwrap(), &successor));
}

#[test]
fn captured_canonical_image_paths_select_exact_root_and_retain_paint_resource() {
    let local = nara_asset::AssetPath::new("ui/panel.png").unwrap();
    let scope = AssetPathScope::under(AssetSourcePath::new("game/art").unwrap());
    let foreign_scope = AssetPathScope::under(AssetSourcePath::new("package/art").unwrap());
    let source = StableAssetId::parse_str("d7961f69-0e25-41d5-8893-0ef85ab19615").unwrap();
    let foreign = StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap();
    let mut server = AssetServer::new();
    let mut assets = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let mut prepared = PreparedImages::default();
    let mut expected = None;
    for (scope, source, pixels) in [
        (&scope, source, [240, 16, 120, 255]),
        (&foreign_scope, foreign, [16, 240, 120, 255]),
    ] {
        let record = AssetRecord::new(source, scope.resolve(&local), AssetSourceKind::Image);
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let product = AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap();
        let image = ImageAsset::from_product(
            product,
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            pixels.to_vec(),
        )
        .unwrap();
        assets
            .commit_loaded(handle, image, &mut states, &mut events, None, None)
            .unwrap();
        let snapshot = PreparedImageSnapshot::capture(handle, &assets, &states).unwrap();
        if expected.is_none() {
            expected = Some(snapshot.clone());
        }
        prepared.insert(snapshot);
    }
    let id = UiImageId::external::<ImageAsset>(
        expected.as_ref().unwrap().identity().key().asset_id().raw(),
    );
    let unresolved_local = UiImageId::new(local.as_str()).unwrap();
    let referenced = BTreeSet::from([id.clone(), unresolved_local.clone()]);
    let images = UiPreparedImages::capture(&referenced, Some(&prepared));
    let missing = UiPresentationPlanner::default().plan(
        frame(
            1,
            1,
            vec![text_node(
                "unbound",
                "",
                Some(UiVisual::image(
                    unresolved_local,
                    UiColor::rgba(1.0, 1.0, 1.0, 1.0),
                )),
            )],
        ),
        RenderTarget::Window(WindowId::PRIMARY),
        &images,
    );
    assert!(matches!(
        missing,
        Err(UiPresentationContentError::MissingImage { .. })
    ));
    let packet = UiPresentationPlanner::default()
        .plan(
            frame(
                1,
                1,
                vec![text_node(
                    "selected",
                    "",
                    Some(UiVisual::image(id, UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
                )],
            ),
            RenderTarget::Window(WindowId::PRIMARY),
            &images,
        )
        .unwrap();
    drop(server);
    drop(assets);
    drop(prepared);
    drop(images);
    assert_eq!(packet.images().len(), 1);
    assert_eq!(packet.images()[0].snapshot(), &expected.unwrap());
    assert_eq!(
        packet.images()[0].snapshot().resource().pixels(),
        &[240, 16, 120, 255]
    );
}

#[test]
fn captured_pathless_products_keep_primary_auxiliary_and_source_separate() {
    let source = StableAssetId::parse_str("d7961f69-0e25-41d5-8893-0ef85ab19615").unwrap();
    let foreign = StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap();
    let auxiliary =
        ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap();
    // Runtime-only image bindings and pixels have no source catalogue or source paths.
    let mut server = AssetServer::new();
    let mut assets = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let mut prepared = PreparedImages::default();
    let mut expected = Vec::new();
    let mut ids = Vec::new();
    for (source, product, pixels) in [
        (source, ImportedProductId::PRIMARY, [240, 0, 0, 255]),
        (source, auxiliary, [0, 240, 0, 255]),
        (foreign, auxiliary, [0, 0, 240, 255]),
    ] {
        let reference = AssetProductRef::new(source, product).unwrap();
        let handle = server.reserve_product::<ImageAsset>(reference).unwrap();
        assert!(server.path(handle.id()).is_none());
        let image = ImageAsset::from_product(
            reference,
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            pixels.to_vec(),
        )
        .unwrap();
        assets
            .commit_loaded(handle, image, &mut states, &mut events, None, None)
            .unwrap();
        let snapshot = PreparedImageSnapshot::capture(handle, &assets, &states).unwrap();
        prepared.insert(snapshot.clone());
        expected.push(snapshot);
        ids.push(UiImageId::external::<ImageAsset>(handle.id().raw()));
    }
    let referenced = ids.iter().cloned().collect();
    let images = UiPreparedImages::capture(&referenced, Some(&prepared));
    let nodes = ids
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            text_node(
                &format!("product-{index}"),
                "",
                Some(UiVisual::image(id, UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
            )
        })
        .collect();
    let packet = UiPresentationPlanner::default()
        .plan(
            frame(1, 1, nodes),
            RenderTarget::Window(WindowId::PRIMARY),
            &images,
        )
        .unwrap();
    drop(server);
    drop(assets);
    drop(prepared);
    drop(images);
    assert_eq!(packet.images().len(), expected.len());
    for snapshot in expected {
        assert!(
            packet
                .images()
                .iter()
                .any(|image| image.snapshot() == &snapshot)
        );
    }
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
    assert!(Arc::ptr_eq(&planner.planned(target).unwrap(), &first));

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

    assert_ne!(
        first.glyph_pages()[0].pixels(),
        second.glyph_pages()[0].pixels()
    );
    assert_ne!(first.glyph_pages()[0].key(), second.glyph_pages()[0].key());
    assert!(Arc::ptr_eq(&planner.planned(first_target).unwrap(), &first));
    assert!(Arc::ptr_eq(
        &planner.planned(second_target).unwrap(),
        &second
    ));
    assert!(Arc::ptr_eq(
        &planner.retire_target(first_target).unwrap(),
        &first
    ));
    assert!(planner.planned(first_target).is_none());
    assert!(Arc::ptr_eq(
        &planner.planned(second_target).unwrap(),
        &second
    ));

    let replacement = planner
        .plan(
            frame(1, 1, vec![text_node("replacement", "B", None)]),
            first_target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    assert_ne!(
        first.glyph_pages()[0].pixels(),
        replacement.glyph_pages()[0].pixels()
    );
    assert_ne!(
        first.glyph_pages()[0].key(),
        replacement.glyph_pages()[0].key()
    );
}

#[test]
fn independent_planners_do_not_alias_glyph_pages_for_the_same_target() {
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let mut first_planner = UiPresentationPlanner::default();
    let mut second_planner = UiPresentationPlanner::default();
    let first = first_planner
        .plan(
            frame(1, 1, vec![text_node("label", "A", None)]),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    let second = second_planner
        .plan(
            frame(1, 1, vec![text_node("label", "B", None)]),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();

    assert_ne!(
        first.glyph_pages()[0].pixels(),
        second.glyph_pages()[0].pixels()
    );
    assert_ne!(first.glyph_pages()[0].key(), second.glyph_pages()[0].key());
}

#[test]
fn font_revision_replacement_does_not_alias_retained_glyph_pages() {
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let mut planner = UiPresentationPlanner::default();
    let first = planner
        .plan(
            frame(1, 1, vec![text_node("label", "A", None)]),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    let changed_environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 320.0, 180.0, 1.0, 2, 1, 1, 2).unwrap();
    let changed = planner
        .plan(
            frame_with_environment(2, vec![text_node("label", "B", None)], changed_environment),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    assert_eq!(changed.glyph_pages()[0].key().font_revision(), 2);

    let replacement = planner
        .plan(
            frame(1, 3, vec![text_node("label", "B", None)]),
            target,
            &UiPreparedImages::default(),
        )
        .unwrap();
    assert_ne!(
        first.glyph_pages()[0].pixels(),
        replacement.glyph_pages()[0].pixels()
    );
    assert_ne!(
        first.glyph_pages()[0].key(),
        replacement.glyph_pages()[0].key()
    );
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
        .snapshot_planned_packets()
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
