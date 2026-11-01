use nara_asset::{AssetPathScope, AssetServer, AssetStates, Assets, ProjectAssetDatabase};
use nara_ecs::World;
use nara_image::ImageAsset;
use nara_project::{EffectivePrimaryUiSurface, EffectiveUiSafeArea};
use nara_reflect::ComponentTypeId;
use nara_render_wgpu::{
    WgpuPolicyDefinition, WgpuRenderPolicy, WgpuRenderProgramError, WgpuStockPolicy,
};
use nara_scene::{PrefabDocument, SceneComponentRecord, SceneEntityId, SceneEntityRecord};
use nara_ui::{__private::project_prefab_authoring, UiNode, UiRoot};

use super::*;
use crate::project_host::runtime::editor::authoring_images::EditorAuthoringImageCatalogCandidate;

struct CanvasSource {
    server: AssetServer,
    database: ProjectAssetDatabase,
    source: SourceDocumentKey,
    revision: SceneAuthoringRevision,
    projection: UiAuthoringProjection,
    images: EditorAuthoringImageCatalogReceipt,
    profile: AcceptedPrimaryUiSurfaceProfile,
}

impl CanvasSource {
    fn new() -> Self {
        let server = AssetServer::new();
        let images = EditorAuthoringImageCatalogCandidate::capture(
            &server,
            &Assets::<ImageAsset>::default(),
            &AssetStates::default(),
            &AssetPathScope::ROOT,
        )
        .unwrap()
        .into_receipt(UiImageChoiceGeneration::new(1))
        .unwrap();
        let registry = crate::project_host::authoring::first_party_authoring_preview_registry(
            crate::project_host::composition::built_in_schema_providers(),
        )
        .unwrap();
        let mut world = World::new();
        let entity = world.spawn((UiRoot::default(), UiNode::default())).id();
        let mut root = SceneEntityRecord::new(SceneEntityId::new("canvas-root").unwrap());
        for name in ["nara.ui.UiRoot", "nara.ui.UiNode"] {
            let component = ComponentTypeId::new(name);
            let record = SceneComponentRecord::new(
                registry.schema(&component).unwrap().version,
                registry
                    .encode_component(&component, &world, entity)
                    .unwrap()
                    .unwrap(),
            );
            root = root.with_component(component, record);
        }
        Self {
            server,
            database: ProjectAssetDatabase::default(),
            source: SourceDocumentKey::named(
                "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
                "typed-policy-canvas",
                nara_scene::SourceDocumentKind::Prefab,
            )
            .unwrap(),
            revision: SceneAuthoringRevision::initial(),
            projection: project_prefab_authoring(&PrefabDocument::from_ordered_roots([root]))
                .unwrap(),
            images,
            profile: AcceptedPrimaryUiSurfaceProfile::from_effective(EffectivePrimaryUiSurface {
                logical_width: 320,
                logical_height: 180,
                safe_area: EffectiveUiSafeArea::default(),
            }),
        }
    }

    fn prepare(&self, session: &mut EditorUiPreviewSession) -> UiAuthoringCanvasProjection {
        session.prepare(
            EditorDocumentId::from_raw(1),
            &self.source,
            self.revision,
            None,
            &self.projection,
            &self.images,
            EditorCanvasImageSources::new(
                &self.server,
                self.database.resolver(&AssetPathScope::ROOT),
            ),
            Some(self.profile),
        )
    }
}

#[test]
fn canvas_moves_original_input_and_rebuilds_under_its_retained_installation() {
    let source = CanvasSource::new();
    let mut session = EditorUiPreviewSession::new().unwrap();
    assert!(matches!(
        source.prepare(&mut session),
        UiAuthoringCanvasProjection::Ready(_)
    ));
    let candidate = session.take_candidate().unwrap();
    let original_input = candidate.render_input.clone();
    let original_paint = Arc::clone(&candidate.paint_packet);
    let original_canvas = candidate.canvas.clone();
    let installation = original_input.installation();

    let (document, revision, gesture, generation, input, paint, canvas) = candidate.into_parts();
    assert_eq!(document, EditorDocumentId::from_raw(1));
    assert_eq!(revision, source.revision);
    assert_eq!(gesture, None);
    assert_eq!(Some(generation), session.geometry_generation());
    assert_eq!(canvas, original_canvas);
    assert_eq!(input.installation(), installation);
    assert!(std::ptr::eq(input.packet(), original_input.packet()));
    assert!(std::ptr::eq(
        input.captured::<WgpuStockPolicy>().unwrap(),
        original_input.captured::<WgpuStockPolicy>().unwrap(),
    ));
    assert!(Arc::ptr_eq(&paint, &original_paint));
    assert_eq!(paint.target(), RenderTarget::Image(session.target));

    assert!(session.take_candidate().is_none());
    assert!(matches!(
        source.prepare(&mut session),
        UiAuthoringCanvasProjection::Ready(_)
    ));
    let (_, _, _, rebuilt_generation, rebuilt_input, rebuilt_paint, rebuilt_canvas) =
        session.take_candidate().unwrap().into_parts();
    assert_eq!(rebuilt_generation, generation);
    assert_eq!(rebuilt_canvas, canvas);
    assert_eq!(rebuilt_input.installation(), installation);
    assert_eq!(rebuilt_input.packet().target(), input.packet().target());
    assert_eq!(
        rebuilt_input.packet().source_generation(),
        input.packet().source_generation(),
    );
    assert!(rebuilt_input.packet().frame_index() > input.packet().frame_index());
    assert!(Arc::ptr_eq(&rebuilt_paint, &paint));

    session.retire();
    drop(session);
    let mut replacement = EditorUiPreviewSession::new().unwrap();
    assert_eq!(input.installation(), installation);
    assert_eq!(rebuilt_input.installation(), installation);
    assert!(std::ptr::eq(
        input.captured::<WgpuStockPolicy>().unwrap(),
        original_input.captured::<WgpuStockPolicy>().unwrap(),
    ));
    assert!(matches!(
        source.prepare(&mut replacement),
        UiAuthoringCanvasProjection::Ready(_)
    ));
    let foreign = replacement.take_candidate().unwrap();
    assert_ne!(foreign.render_input.installation(), installation);
    assert_ne!(
        foreign.render_input.packet().target(),
        input.packet().target()
    );
    assert!(
        WgpuStockPolicy::new()
            .validate_capture(
                foreign.render_input.packet(),
                input.captured::<WgpuStockPolicy>().unwrap(),
            )
            .is_err(),
        "the retained UI drawing must still belong to its original Canvas target"
    );
}

#[test]
fn rejected_canvas_policy_capture_clears_the_previous_candidate() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let mut source = CanvasSource::new();
    let mut session = EditorUiPreviewSession::new().unwrap();
    let rejected = Arc::new(AtomicBool::new(false));
    let reject_capture = Arc::clone(&rejected);
    session.policy = WgpuStockCaptureAdapter::new(
        WgpuPolicyDefinition::new(WgpuStockPolicy::new())
            .install()
            .unwrap(),
        move |stock| {
            if reject_capture.load(Ordering::SeqCst) {
                Err(WgpuRenderProgramError::PolicyClosed)
            } else {
                Ok(Arc::new(stock))
            }
        },
    );
    assert!(matches!(
        source.prepare(&mut session),
        UiAuthoringCanvasProjection::Ready(_)
    ));
    assert!(session.candidate.is_some());
    let frame_index = session.next_frame_index;
    rejected.store(true, Ordering::SeqCst);
    assert!(matches!(
        session.try_rebuild(session.key.clone().unwrap(), source.profile),
        Err(UiPreviewError::Policy(WgpuRenderProgramError::PolicyClosed))
    ));
    source.profile = AcceptedPrimaryUiSurfaceProfile::from_effective(EffectivePrimaryUiSurface {
        logical_width: 640,
        logical_height: 360,
        safe_area: EffectiveUiSafeArea::default(),
    });

    assert_eq!(
        source.prepare(&mut session),
        UiAuthoringCanvasProjection::Rejected(vec![
            UiAuthoringDiagnostic::canvas_compilation_rejected(),
        ]),
    );
    assert_eq!(session.geometry_generation(), None);
    assert!(session.take_candidate().is_none());
    assert_eq!(session.next_frame_index, frame_index);
}

#[test]
fn configured_canvas_keeps_original_capture_and_paint_across_owner_replacement() {
    use crate::product::wgpu_policy_tests::{TestPolicy, recipe};

    let source = CanvasSource::new();
    let selected = recipe(19);
    let mut session = EditorUiPreviewSession::with_recipe(&selected).unwrap();
    assert!(matches!(
        source.prepare(&mut session),
        UiAuthoringCanvasProjection::Ready(_)
    ));
    let candidate = session.take_candidate().unwrap();
    let original = candidate.render_input.clone();
    let (_, _, _, _, input, paint, _) = candidate.into_parts();
    let captured = input.captured::<TestPolicy>().unwrap();
    assert_eq!(captured.value, 19);
    assert!(input.captured::<WgpuStockPolicy>().is_none());
    assert!(Arc::ptr_eq(&captured.stock.ui_packets()[0], &paint));
    drop(session);

    // Content refresh reconstructs each Canvas from the Session's retained recipe.
    let mut refreshed = EditorUiPreviewSession::with_recipe(&selected).unwrap();
    assert!(matches!(
        source.prepare(&mut refreshed),
        UiAuthoringCanvasProjection::Ready(_)
    ));
    let refreshed = refreshed.take_candidate().unwrap();
    assert_ne!(refreshed.render_input.installation(), input.installation());
    assert_eq!(
        refreshed
            .render_input
            .captured::<TestPolicy>()
            .unwrap()
            .value,
        19,
    );

    let mut successor = EditorUiPreviewSession::with_recipe(&recipe(23)).unwrap();
    assert!(matches!(
        source.prepare(&mut successor),
        UiAuthoringCanvasProjection::Ready(_)
    ));
    let successor = successor.take_candidate().unwrap();
    assert_eq!(
        successor
            .render_input
            .captured::<TestPolicy>()
            .unwrap()
            .value,
        23,
    );
    assert_ne!(successor.render_input.installation(), input.installation());
    assert!(std::ptr::eq(input.packet(), original.packet()));
    assert!(std::ptr::eq(
        input.captured::<TestPolicy>().unwrap(),
        original.captured::<TestPolicy>().unwrap(),
    ));
    assert!(Arc::ptr_eq(
        &input.captured::<TestPolicy>().unwrap().stock.ui_packets()[0],
        &paint,
    ));
}
