use super::{CompositionError, ProductRecipe, ProductRecipeError, RuntimePlanError, TestManifest};
#[cfg(feature = "presentation-ui")]
use nara::app::{PluginGroup, PluginGroupBuilder, PluginGroupId};
use nara::{
    app::{App, PluginPlan, PluginPlanError},
    project_host::{ingest_project_manifest, resolve_product_recipe},
    ui::{FontFace, UiSurfaceId},
};

fn custom_face() -> FontFace {
    FontFace::from_bytes(
        include_bytes!("../../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap()
}

fn configured_recipe() -> ProductRecipe {
    ProductRecipe::new().with_ui_font(custom_face()).unwrap()
}

fn manifest(presentation: bool) -> TestManifest {
    let capabilities = if presentation {
        r#"["runtime-ui", "presentation-ui"]"#
    } else {
        r#"["runtime-ui"]"#
    };
    TestManifest::new(&format!(
        "schema_version = 1\n[project]\nname = 'Configured UI'\n[runtime]\npreset = 'minimal'\n[capabilities]\nrequested = {capabilities}\n"
    ))
}

#[test]
fn recipe_default_font_rejects_duplicates_and_profiles_without_ui() {
    let recipe = configured_recipe();
    assert_eq!(recipe.ui_font().unwrap().id(), custom_face().id());
    assert!(matches!(
        recipe.clone().with_ui_font(custom_face()),
        Err(ProductRecipeError::DuplicateDefaultConfiguration { plugin })
            if plugin == nara::ui::UI_PLUGIN_ID
    ));

    let project = TestManifest::new(super::MINIMAL_MANIFEST);
    let candidate = ingest_project_manifest(&project.capability(), None).unwrap();
    assert_eq!(
        resolve_product_recipe(&candidate, recipe).unwrap_err(),
        RuntimePlanError::Composition(CompositionError::UnsupportedDefaultConfiguration {
            plugin: nara::ui::UI_PLUGIN_ID,
        })
    );
}

#[test]
fn recipe_default_font_configures_runtime_ui_without_requiring_presentation() {
    let project = manifest(false);
    let candidate = ingest_project_manifest(&project.capability(), None).unwrap();
    let first = resolve_product_recipe(&candidate, configured_recipe()).unwrap();
    let repeated = resolve_product_recipe(&candidate, configured_recipe()).unwrap();
    let plain = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    assert_eq!(
        first.plugin_plan().fingerprint(),
        repeated.plugin_plan().fingerprint()
    );
    assert_ne!(
        first.plugin_plan().fingerprint(),
        plain.plugin_plan().fingerprint()
    );
    assert_eq!(
        first.schema_validation().composition_fingerprint(),
        plain.schema_validation().composition_fingerprint()
    );
    let ui = first
        .plugin_plan()
        .entries()
        .iter()
        .find(|entry| entry.plugin_id() == nara::ui::UI_PLUGIN_ID)
        .unwrap();
    assert_eq!(ui.definition_key(), nara::ui::plugin(custom_face()).key());
    assert!(
        first
            .plugin_plan()
            .entries()
            .iter()
            .all(|entry| { entry.plugin_id().as_str() != "nara.ui-render" })
    );

    // Fresh runtime state must not inherit a surface installed in another reconstruction.
    let recipe = configured_recipe();
    let mut first_app = App::new();
    first_app
        .add_plugins(recipe.clone().with_defaults(nara::RuntimeUiPlugins))
        .unwrap();
    install_surface(first_app.world_mut().unwrap());
    let mut second_app = App::new();
    second_app
        .add_plugins(recipe.with_defaults(nara::RuntimeUiPlugins))
        .unwrap();
    assert_eq!(
        nara::ui::active_ui_surface_slots(first_app.world())
            .unwrap()
            .len(),
        1
    );
    assert!(
        nara::ui::active_ui_surface_slots(second_app.world())
            .unwrap()
            .is_empty()
    );

    let direct =
        PluginPlan::resolve(configured_recipe().with_defaults(nara::RuntimeUiPlugins)).unwrap();
    let direct_ui = direct
        .entries()
        .iter()
        .find(|entry| entry.plugin_id() == nara::ui::UI_PLUGIN_ID)
        .unwrap();
    assert_eq!(ui.definition_key(), direct_ui.definition_key());
}

#[test]
fn configured_recipe_requires_the_owning_defaults_group() {
    let error =
        PluginPlan::resolve(configured_recipe().with_defaults(nara::MinimalPlugins)).unwrap_err();
    assert!(
        matches!(error, PluginPlanError::MissingSlotEditTarget { slot }
        if slot.as_str() == "nara.plugins.slot.ui")
    );
    let mut app = App::new();
    assert!(
        app.add_plugins((nara::RuntimeUiPlugins, configured_recipe()))
            .is_err()
    );
}

fn install_surface(world: &mut nara::ecs::World) {
    use nara::{
        input::{InputPointerId, InputSeatId, InputWindowId},
        ui::{UiSize, UiSurfaceContentTransform, UiSurfaceEndpoint, UiSurfaceSlot},
    };
    nara::ui::install_ui_surface(
        world,
        UiSurfaceSlot::new(
            UiSurfaceId::new(71).unwrap(),
            UiSurfaceEndpoint::new(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            UiSurfaceContentTransform::direct(UiSize::new(240.0, 100.0).unwrap(), 1.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
}

#[cfg(feature = "presentation-ui")]
struct UiDefaults;

#[cfg(feature = "presentation-ui")]
impl PluginGroup for UiDefaults {
    const ID: PluginGroupId = PluginGroupId::new("test.configured-ui.defaults");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(nara::RuntimeUiPlugins)
            .add_group(nara::PresentationUiPlugins)
    }
}

#[cfg(feature = "presentation-ui")]
#[test]
fn recipe_default_font_selects_the_same_compile_and_render_face() {
    use nara::app::RuntimeInstance;

    let project = manifest(true);
    let candidate = ingest_project_manifest(&project.capability(), None).unwrap();
    let plan = resolve_product_recipe(&candidate, configured_recipe()).unwrap();
    let direct = PluginPlan::resolve(configured_recipe().with_defaults(UiDefaults)).unwrap();
    for (plugin, expected) in [
        (nara::ui::UI_PLUGIN_ID, nara::ui::plugin(custom_face())),
        (
            nara::ui_render::UI_RENDER_PLUGIN_ID,
            nara::ui_render::plugin(custom_face()),
        ),
    ] {
        let file_entry = plan
            .plugin_plan()
            .entries()
            .iter()
            .find(|entry| entry.plugin_id() == plugin)
            .unwrap();
        let direct_entry = direct
            .entries()
            .iter()
            .find(|entry| entry.plugin_id() == plugin)
            .unwrap();
        assert_eq!(file_entry.definition_key(), expected.key());
        assert_eq!(file_entry.definition_key(), direct_entry.definition_key());
    }

    // Install the real consumer before the App is sealed. The file-backed execution path is
    // covered separately by the same-face Headless/Check and managed Editor Play fixtures.
    let recipe = configured_recipe()
        .add_plugin::<FontFixturePlugin>()
        .unwrap();
    let mut app = App::new();
    app.add_plugins(recipe.with_defaults(UiDefaults)).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let proof = runtime.world().resource::<FontPaintProof>();
    assert_eq!(proof.packets, 1);
    assert!(proof.rasterized_text);
    assert!(proof.selected_compile_face);
    runtime.finish().unwrap();
}

#[cfg(feature = "presentation-ui")]
#[derive(Default)]
struct FontFixturePlugin;

#[cfg(feature = "presentation-ui")]
impl nara::app::Plugin for FontFixturePlugin {
    fn declaration() -> &'static nara::app::PluginDeclaration {
        static DECLARATION: nara::app::PluginDeclaration = nara::app::PluginDeclaration::new(
            nara::app::PluginId::new("test.product-default.font-fixture"),
            nara::app::PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::ui::UI_PLUGIN_ID, nara::ui_render::UI_RENDER_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), nara::app::PluginError> {
        app.init_resource::<FontPaintProof>()?
            .add_systems(nara::app::CoreStage::Sort, capture_font_paint)?;
        let world = app.world_mut()?;
        world.spawn(nara::window::Window::new(
            "Configured font",
            nara::window::WindowResolution::new(240, 100),
        ));
        install_surface(world);
        install_text(world);
        Ok(())
    }
}

#[cfg(feature = "presentation-ui")]
#[test]
fn recipe_default_configuration_does_not_override_a_duplicate_recipe_plugin() {
    let project = manifest(true);
    let candidate = ingest_project_manifest(&project.capability(), None).unwrap();
    let recipe = configured_recipe()
        .add_plugin::<nara::ui_render::UiRenderPlugin>()
        .unwrap();
    assert!(matches!(
        resolve_product_recipe(&candidate, recipe),
        Err(RuntimePlanError::PluginPlan(PluginPlanError::DuplicatePlugin { plugin }))
            if plugin == nara::ui_render::UI_RENDER_PLUGIN_ID
    ));
}

#[cfg(feature = "presentation-ui")]
#[derive(Default, nara::ecs::Resource)]
struct FontPaintProof {
    packets: usize,
    rasterized_text: bool,
    selected_compile_face: bool,
}

#[cfg(feature = "presentation-ui")]
fn capture_font_paint(world: &mut nara::ecs::World) {
    use nara::ui_render::advanced::{
        UiPreparedImages, UiPresentationContentError, UiPresentationPlanner,
    };
    let frame = nara::ui::latest_ui_frame(world, UiSurfaceId::new(71).unwrap())
        .unwrap()
        .unwrap();
    let mut bundled = UiPresentationPlanner::new(Default::default()).unwrap();
    let selected_compile_face = matches!(
        bundled.plan(frame, nara::render::RenderTarget::Window(nara::window::WindowId::PRIMARY), &UiPreparedImages::new()),
        Err(UiPresentationContentError::FontFaceMismatch { actual, .. }) if actual == custom_face().id()
    );
    let (packets, _) = nara_ui_render::__private::snapshot_ui_paint_packets(world)
        .unwrap()
        .into_parts();
    *world.resource_mut::<FontPaintProof>() = FontPaintProof {
        packets: packets.len(),
        rasterized_text: packets.iter().any(|packet| {
            packet
                .glyph_pages()
                .iter()
                .any(|page| page.pixels().iter().any(|pixel| *pixel != 0))
        }),
        selected_compile_face,
    };
}

#[cfg(feature = "presentation-ui")]
fn install_text(world: &mut nara::ecs::World) {
    use nara::hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
    };
    use nara::ui::{UiLayer, UiNode, UiRoot, UiRuntimeKey, UiStyle, UiText};
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new("test.product-default", "root").unwrap(),
        ))
        .id();
    let text = world
        .spawn((
            UiNode::new(UiStyle::absolute(4.0, 4.0, 210.0, 40.0)),
            UiText::new("Product recipe font", 18.0).unwrap(),
            UiRuntimeKey::new("test.product-default", "text").unwrap(),
        ))
        .id();
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
        .unwrap();
}
