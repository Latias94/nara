use std::sync::{Arc, Mutex};

use nara::{
    ProductConfiguration, ProductRecipe, ProductRecipeError,
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        StartupStage,
    },
    ecs::{Resource, World},
    ui::{FontFace, UiLayer, UiNode, UiRoot, UiRuntimeKey, UiStyle, UiSurfaceId, UiText},
};

pub const PROFILE_FONT_PROOF: &str =
    "profile_font: shaped_selected_face=true rasterized_selected_face=true";
pub const PROFILE_FONT_NATIVE_PROOF: &str =
    "profile_font_native: presented_selected_face=true submitted_glyphs=true";
pub const WORLD_CANVAS_NATIVE_PROOF: &str =
    "world_canvas_native: fixed_world=true native_ui=true presented_picking=true";

#[derive(Default, Resource, Clone, Debug, PartialEq, Eq)]
pub struct ProfileFontOutcome {
    pub compiled_selected_face: bool,
    pub rendered_selected_face: bool,
    pub presented_selected_face: bool,
}

pub type ProfileFontObservations = Arc<Mutex<Vec<ProfileFontOutcome>>>;

struct ProfileFontConfiguration {
    face: FontFace,
    observations: Option<ProfileFontObservations>,
}

impl ProductConfiguration for ProfileFontConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.face.id().as_bytes());
    }
}

/// One ordinary product recipe shared by all fixture entry points.
pub fn profile_font_recipe(
    face: FontFace,
    observations: Option<ProfileFontObservations>,
) -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new()
        .with_ui_font(face.clone())?
        .add_configured_plugin(
            ProfileFontConfiguration { face, observations },
            |configuration| ProfileFontPlugin {
                face: configuration.face.clone(),
                observations: configuration.observations.clone(),
            },
        )
}

struct ProfileFontPlugin {
    face: FontFace,
    observations: Option<ProfileFontObservations>,
}

#[derive(Resource)]
struct ProfileFontRuntime {
    face: FontFace,
    observations: Option<ProfileFontObservations>,
}

impl Plugin for ProfileFontPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("test.product-profile-font"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::ui::UI_PLUGIN_ID, nara::ui_render::UI_RENDER_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ProfileFontRuntime {
            face: self.face.clone(),
            observations: self.observations.clone(),
        })?
        .init_resource::<ProfileFontOutcome>()?
        .add_systems(StartupStage::Runtime, install_font_fixture)?
        .add_systems(CoreStage::Sort, capture_font)?
        .add_systems(CoreStage::Last, capture_presented_font)?;
        Ok(())
    }
}

fn install_font_fixture(world: &mut World) {
    let mut windows = world.query::<&nara::window::Window>();
    if windows.iter(world).next().is_none() {
        world.spawn(nara::window::Window::new(
            "Font boot",
            nara::window::WindowResolution::new(320, 180),
        ));
    }
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new("test.font-boot", "root").unwrap(),
        ))
        .id();
    let text = world
        .spawn((
            UiNode::new(UiStyle::absolute(4.0, 4.0, 280.0, 44.0)),
            UiRuntimeKey::new("test.font-boot", "text").unwrap(),
            UiText::new("Configured font boot", 18.0).unwrap(),
        ))
        .id();
    use nara::hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
    };
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: text,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: root,
                }),
            },
        ])
        .expect("the fresh font fixture hierarchy is valid");
}

fn capture_font(world: &mut World) {
    use nara::ui_render::advanced::{
        UiPreparedImages, UiPresentationContentError, UiPresentationPlanner,
    };
    let frame = nara::ui::latest_ui_frame(world, UiSurfaceId::new(1).unwrap())
        .unwrap()
        .unwrap();
    let expected_face = world.resource::<ProfileFontRuntime>().face.id();
    let compiled_selected_face = matches!(
        UiPresentationPlanner::new(Default::default()).unwrap().plan(
            frame, nara::render::RenderTarget::Window(nara::window::WindowId::PRIMARY), &UiPreparedImages::new(),
        ),
        Err(UiPresentationContentError::FontFaceMismatch { actual, .. }) if actual == expected_face
    );
    let face = world.resource::<ProfileFontRuntime>().face.clone();
    let selected_frame = nara::ui::latest_ui_frame(world, UiSurfaceId::new(1).unwrap())
        .unwrap()
        .unwrap();
    let packet = UiPresentationPlanner::with_font(face, Default::default())
        .unwrap()
        .plan(
            selected_frame,
            nara::render::RenderTarget::Window(nara::window::WindowId::PRIMARY),
            &UiPreparedImages::new(),
        )
        .unwrap();
    let rendered_selected_face = packet
        .glyph_pages()
        .iter()
        .any(|page| page.pixels().iter().any(|pixel| *pixel != 0));
    assert!(
        compiled_selected_face,
        "the profile must compile text with the selected face"
    );
    assert!(
        rendered_selected_face,
        "the profile's configured renderer must rasterize that text"
    );
    let outcome = ProfileFontOutcome {
        compiled_selected_face,
        rendered_selected_face,
        presented_selected_face: world
            .resource::<ProfileFontOutcome>()
            .presented_selected_face,
    };
    let first = !world
        .resource::<ProfileFontOutcome>()
        .compiled_selected_face;
    if first {
        println!("{PROFILE_FONT_PROOF} face={expected_face:?}");
        if let Some(observations) = &world.resource::<ProfileFontRuntime>().observations {
            observations.lock().unwrap().push(outcome.clone());
        }
    }
    *world.resource_mut::<ProfileFontOutcome>() = outcome;
}

#[allow(
    unexpected_cfgs,
    reason = "this fixture source is also compiled in a generated product with a desktop feature"
)]
fn capture_presented_font(world: &mut World) {
    use nara::ui_render::advanced::{
        UiPreparedImages, UiPresentationContentError, UiPresentationPlanner,
        latest_presented_ui_observation,
    };
    if world
        .resource::<ProfileFontOutcome>()
        .presented_selected_face
    {
        return;
    }
    let Some(presented) = latest_presented_ui_observation(world) else {
        return;
    };
    let expected_face = world.resource::<ProfileFontRuntime>().face.id();
    let selected = matches!(
        UiPresentationPlanner::new(Default::default()).unwrap().plan(
            presented.frame(), presented.packet().target(), &UiPreparedImages::new(),
        ),
        Err(UiPresentationContentError::FontFaceMismatch { actual, .. }) if actual == expected_face
    );
    assert!(
        selected,
        "the owner-produced presentation must retain the configured shaped face"
    );
    assert!(
        presented
            .packet()
            .glyph_pages()
            .iter()
            .any(|page| page.pixels().iter().any(|pixel| *pixel != 0))
    );
    #[cfg(feature = "desktop")]
    assert!(
        presented
            .wgpu()
            .is_some_and(|gpu| gpu.submitted_glyph_quad_count() > 0 && gpu.glyph_page_count() > 0)
    );
    if let Some(canvas) = world.get_resource::<nara::render::WorldCanvas>().copied() {
        use nara::{core::Vec2, ecs::bevy_ecs::system::SystemState, render::WorldPointer};
        let view = world.resource::<nara::render::ExtractedViews>().as_slice()[0];
        assert_eq!(view.viewport.extent(), canvas.extent());
        let window = {
            let mut query = world.query::<&nara::window::Window>();
            query.iter(world).next().unwrap().clone()
        };
        assert_ne!(
            window.resolution.physical_width,
            canvas.extent().width,
            "the ordinary project must keep the actual window extent"
        );
        let physical_ui = presented
            .frame()
            .environment()
            .content_transform()
            .physical_surface_size();
        assert_eq!(physical_ui.width(), window.resolution.physical_width as f32);
        assert_eq!(
            physical_ui.height(),
            window.resolution.physical_height as f32
        );
        let window_center = Vec2::new(
            window.resolution.physical_width as f32,
            window.resolution.physical_height as f32,
        ) / (2.0 * window.resolution.scale_factor as f32);
        let expected = view
            .viewport_to_world(Vec2::new(
                canvas.extent().width as f32 * 0.5,
                canvas.extent().height as f32 * 0.5,
            ))
            .unwrap();
        let mut state = SystemState::<WorldPointer>::new(world);
        let actual = state
            .get(world)
            .unwrap()
            .window_to_world(view.camera_entity, window_center)
            .expect("world input must use the actual successful presentation");
        assert!((actual - expected).length() < 0.001);
        println!("{WORLD_CANVAS_NATIVE_PROOF}");
    }
    world
        .resource_mut::<ProfileFontOutcome>()
        .presented_selected_face = true;
    println!("{PROFILE_FONT_NATIVE_PROOF} face={expected_face:?}");
}
