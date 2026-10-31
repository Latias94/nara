use nara_asset::{AssetPathScope, AssetServer, AssetStates, Assets, ProjectAssetDatabase};
use nara_ecs::World;
use nara_image::ImageAsset;
use nara_project::{EffectivePrimaryUiSurface, EffectiveUiSafeArea};
use nara_reflect::ComponentTypeId;
use nara_render_wgpu::{WgpuRenderPolicy, WgpuRenderProgramError};
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
    let installation = session.policy.id();

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
    assert_ne!(replacement.policy.id(), installation);
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
    assert_ne!(
        foreign.render_input.packet().target(),
        input.packet().target()
    );
    assert!(
        replacement
            .policy
            .definition()
            .policy()
            .validate_capture(
                foreign.render_input.packet(),
                input.captured::<WgpuStockPolicy>().unwrap(),
            )
            .is_err(),
        "the retained UI drawing must still belong to its original Canvas target"
    );
}

#[test]
fn closed_canvas_policy_rejects_capture_and_clears_the_previous_candidate() {
    let mut source = CanvasSource::new();
    let mut session = EditorUiPreviewSession::new().unwrap();
    assert!(matches!(
        source.prepare(&mut session),
        UiAuthoringCanvasProjection::Ready(_)
    ));
    assert!(session.candidate.is_some());
    let frame_index = session.next_frame_index;
    session.policy.close();
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
