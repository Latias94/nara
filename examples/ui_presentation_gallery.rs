use std::{env, error::Error, sync::Arc};

use nara::input::{
    ButtonTransitionPhase, InputPointerId, InputSeatId, InputTimeline, InputWindowId,
};
use nara::ui_render::advanced::{UiPreparedImages, UiPresentationPlanner};
use nara::{advanced_prelude::*, backend_prelude::*};

#[path = "ui_presentation_gallery/fixture.rs"]
mod fixture;
#[path = "ui_presentation_gallery/presenter.rs"]
mod presenter;
#[path = "ui_presentation_gallery/probes.rs"]
mod probes;

#[path = "support/runtime_retirement.rs"]
mod runtime_retirement;

use fixture::{GalleryImage, capture_prepared_image, setup_gallery, surface_id};
use presenter::{HeadlessTestPresenter, commit_headless_test_outcome};
use probes::assert_owned_packet;
use runtime_retirement::finish_runtime_after_winit;

const DOMAIN: &str = "nara.example.ui-presentation-gallery";
const UI_TEXTURE_STABLE_ID: &str = "f7d2d9c7-2b13-49fe-8b89-83d0f98f0c3f";
const ACTIVATE_ACTION: &str = "gallery.activate";

fn main() -> Result<(), Box<dyn Error>> {
    if env::args().any(|argument| argument == "--windowed") {
        run_windowed()
    } else {
        run_headless_conformance()
    }
}

fn run_headless_conformance() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins((
        RuntimeUiPlugins,
        nara::render::RenderComponentsPlugin,
        RenderPlugin,
        ImagePlugin::default(),
        ImagePreparePlugin,
    ))?;
    app.add_systems(StartupStage::Scene, setup_gallery)?;
    app.world_mut()?.insert_resource(AssetServer::new());

    let endpoint = UiSurfaceEndpoint::new(
        InputWindowId::new(WindowId::PRIMARY.get()),
        InputSeatId::PRIMARY,
        InputPointerId::PRIMARY,
    );
    let base_slot = UiSurfaceSlot::new(
        surface_id(1),
        endpoint,
        content_transform(480.0, 270.0, 480.0, 270.0, 1.0)?,
    )?;
    install_ui_surface(app.world_mut()?, base_slot)?;
    let render_mapped = update_ui_surface_slot(
        app.world_mut()?,
        base_slot.surface_id(),
        endpoint,
        content_transform(480.0, 270.0, 720.0, 405.0, 1.0)?,
    )?;
    assert_eq!(render_mapped.slot_generation(), 2);
    assert_eq!(render_mapped.route_generation(), 2);
    assert_eq!(render_mapped.geometry_generation(), 2);
    assert_eq!(render_mapped.presentation_generation(), 2);
    let mapped_slot = update_ui_surface_slot(
        app.world_mut()?,
        base_slot.surface_id(),
        endpoint,
        content_transform(480.0, 270.0, 720.0, 405.0, 1.2)?,
    )?;
    assert_eq!(mapped_slot.slot_generation(), 3);
    assert_eq!(mapped_slot.route_generation(), 3);
    assert_eq!(mapped_slot.geometry_generation(), 3);
    assert_eq!(mapped_slot.presentation_generation(), 3);

    let cursor = app
        .world_mut()?
        .resource_mut::<UiActionJournal>()
        .register_consumer(UiActionConsumerId::new("presentation-gallery")?)?;
    app.update()?;

    let frame = latest_ui_frame(app.world(), mapped_slot.surface_id())?
        .expect("the presentation gallery should compile a candidate frame");
    let handle = app.world().resource::<GalleryImage>().0;
    let image_snapshot = capture_prepared_image(app.world(), handle)?;
    let image_id = UiImageId::new(UI_TEXTURE_STABLE_ID)?;
    let mut prepared_images = UiPreparedImages::new();
    prepared_images.insert(image_id, image_snapshot.clone());
    let mut presentation_planner = UiPresentationPlanner::default();
    let packet = presentation_planner.plan(
        Arc::clone(&frame),
        RenderTarget::Window(WindowId::PRIMARY),
        &prepared_images,
    )?;
    drop(prepared_images);

    assert_owned_packet(&packet, &frame, &image_snapshot, mapped_slot);
    let mut rejecting_presenter =
        HeadlessTestPresenter::new(RenderTarget::Window(WindowId::new(2)));
    let rejected =
        rejecting_presenter.accept_and_submit(app.world_mut()?, Arc::clone(&packet), 6)?;
    assert!(rejecting_presenter.submitted().is_none());
    assert_eq!(
        rejected.attempt(),
        nara_ui_render::__private::presentation_attempt(&packet)
    );
    assert!(commit_headless_test_outcome(app.world_mut()?, rejected)?.is_none());
    assert!(latest_ui_frame(app.world(), mapped_slot.surface_id())?.is_some());

    let mut presenter = HeadlessTestPresenter::new(RenderTarget::Window(WindowId::PRIMARY));
    let presented = presenter.accept_and_submit(app.world_mut()?, Arc::clone(&packet), 7)?;
    assert!(Arc::ptr_eq(
        presenter
            .submitted()
            .expect("the presenter should retain the exact submitted packet"),
        &packet,
    ));
    let receipt = commit_headless_test_outcome(app.world_mut()?, presented)?
        .expect("the exact submitted packet should advance presentation authority");
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY)?;
    let button = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "Activate packet")
        .expect("the presented Button should publish formal semantics");
    let action = UiActionId::new(ACTIVATE_ACTION)?;
    assert_eq!(button.actions(), std::slice::from_ref(&action));
    let timeline = &mut *app.world_mut()?.resource_mut::<InputTimeline>();
    assert_eq!(
        timeline
            .admit_gamepad_button(
                mapped_slot.endpoint().window(),
                mapped_slot.endpoint().seat(),
                0,
                13,
                ButtonTransitionPhase::Pressed,
            )?
            .admitted(),
        1
    );
    assert!(matches!(
        dispatch_ui_accessibility_input(
            app.world_mut()?,
            UiAccessibilityInput::semantic_action(
                receipt,
                UiSeatId::PRIMARY,
                button.key().clone(),
                action.clone(),
            ),
        ),
        UiRouteDisposition::Consumed(_)
    ));
    app.update()?;
    let journal = app.world().resource::<UiActionJournal>();
    let pending = journal.pending(&cursor)?;
    assert_eq!(pending.len(), 1);
    let event = &pending[0];
    assert_eq!(event.action(), &action);
    assert_eq!(event.target(), button.key());
    assert_eq!(event.source(), UiActivationSource::Accessibility);
    assert_eq!(
        event.frame_revision(),
        packet.identity().frame_revision().get()
    );
    assert_eq!(event.presentation_epoch(), 7);

    let resized_slot = update_ui_surface_slot(
        app.world_mut()?,
        mapped_slot.surface_id(),
        mapped_slot.endpoint(),
        content_transform(512.0, 288.0, 720.0, 405.0, 1.2)?,
    )?;
    app.update()?;
    let resized_frame = latest_ui_frame(app.world(), resized_slot.surface_id())?
        .expect("the resized surface should compile a replacement candidate");
    let resized_logical_size = resized_frame.environment().logical_size();
    assert_eq!(resized_logical_size.width(), 512.0);
    assert_eq!(resized_logical_size.height(), 288.0);
    assert_ne!(resized_frame.revision(), frame.revision());
    assert!(packet.images()[0].snapshot().resource().pixel_len() > 0);

    let retirement = retire_ui_surface(app.world_mut()?, mapped_slot.surface_id())?
        .expect("the installed Surface Slot should retire exactly once");
    assert_eq!(retirement.slot_generation(), resized_slot.slot_generation());
    assert_eq!(
        retirement.retirement_generation(),
        resized_slot.slot_generation() + 1
    );
    assert_eq!(retirement.token().surface_id(), mapped_slot.surface_id());
    assert!(matches!(
        install_ui_surface(app.world_mut()?, mapped_slot),
        Err(UiAdapterError::RetiredSurface { surface: 1 })
    ));
    let recovered = UiSurfaceSlot::new(
        surface_id(2),
        endpoint,
        content_transform(480.0, 270.0, 720.0, 405.0, 1.2)?,
    )?;
    install_ui_surface(app.world_mut()?, recovered)?;
    app.update()?;
    assert!(latest_ui_frame(app.world(), recovered.surface_id())?.is_some());

    assert!(packet.images()[0].snapshot().resource().pixel_len() > 0);
    assert!(
        packet
            .glyph_pages()
            .iter()
            .flat_map(|page| page.pixels())
            .any(|byte| *byte != 0)
    );
    println!(
        "presentation gallery: ECS candidate -> owned solid/image/glyph packet -> matching receipt -> accessibility action; retired packet remains readable"
    );
    Ok(())
}

fn run_windowed() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins((
        RuntimeUiPlugins,
        PresentationUiPlugins,
        WindowPlugin {
            primary_window: Some(Window::new(
                "nara UI presentation gallery",
                WindowResolution::new(960, 540),
            )),
        },
        WgpuBackendPlugins,
        WgpuUiSubmitterPlugin,
        WinitUiAccessibilityContributionPlugin,
    ))?;
    app.add_systems(StartupStage::Scene, setup_gallery)?;
    app.world_mut()?.insert_resource(AssetServer::new());
    install_ui_surface(
        app.world_mut()?,
        UiSurfaceSlot::new(
            surface_id(1),
            UiSurfaceEndpoint::new(
                InputWindowId::new(WindowId::PRIMARY.get()),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            content_transform(480.0, 270.0, 960.0, 540.0, 1.0)?,
        )?,
    )?;
    let candidate = nara::app::RuntimeAdmissionReservation::try_acquire()?.admit(
        app.seal()?,
        nara::app::RuntimeObligationLedger::new(),
        nara::app::RuntimeClosePolicy::default(),
    )?;
    let mut runtime = candidate.complete_startup()?.promote();
    let run_result = WinitRunner::default().run(&mut runtime);
    finish_runtime_after_winit(run_result, runtime)
}

fn content_transform(
    authored_width: f32,
    authored_height: f32,
    physical_width: f32,
    physical_height: f32,
    platform_scale_factor: f32,
) -> Result<UiSurfaceContentTransform, Box<dyn Error>> {
    let authored = UiSize::new(authored_width, authored_height)
        .ok_or("the authored gallery UI size must be finite and non-negative")?;
    let physical = UiSize::new(physical_width, physical_height)
        .ok_or("the gallery physical surface size must be finite and non-negative")?;
    Ok(UiSurfaceContentTransform::new(
        authored,
        physical,
        UiSafeAreaInsets::default(),
        platform_scale_factor,
    )?)
}
