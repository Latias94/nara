use std::{collections::BTreeMap, sync::Arc, time::Duration};

use nara::{
    app::{App, CoreStage, RuntimeInstance},
    asset::{AssetEvents, AssetPath, AssetStates, Assets, Handle, allocate_anonymous_handle},
    core::{Color, Vec2},
    ecs::{World, schedule::IntoScheduleConfigs},
    hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
        HierarchyWriter,
    },
    image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImagePreparePlugin},
    material::{
        AlphaMode2d, Material2dBinding, Material2dDeclaration, Material2dGeneration,
        Material2dLibrary, MaterialParameterKind, MaterialParameterLayout, MaterialParameterValue,
        MaterialTextureDeclaration, MaterialUniformField, SamplerDescriptor,
    },
    reflect::ComponentRegistryPlugin,
    render::{Camera2d, RenderComponentsPlugin, RenderPlugin, RenderTarget},
    sprite::{Sprite, SpritePlugin},
    transform::{Transform2d, TransformPlugin},
    window::{Window, WindowPlugin, WindowResolution},
};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuPolicyCaptureSet, WgpuRenderInput, WgpuStockCapture,
    WgpuStockCapturePlugin, WgpuStockSpritePlugin, take_captured_policy_batch,
};
use nara_stock_composition_consumer::{
    CompositionConfiguration, CompositionPlugin, CompositionPolicy, RetainedInputs,
};

use crate::scene;

pub const MATERIAL_ID: &str = "consumer.graph-material.palette";
pub const PALETTE_ID: &str = "image.palette";
pub const ACCENT_ID: &str = "image.accent";
pub const SCALE_ID: &str = "palette.scale";
pub const SCALE: [f32; 4] = [0.75, 0.5, 0.75, 0.0];
pub const TINT: [f32; 4] = [0.75, 0.75, 1.0, 0.75];
pub const DEFAULT_PALETTE: [u8; 4] = [96, 0, 64, 128];
pub const ACCENT: [u8; 4] = [224, 224, 224, 255];

pub struct Fixture {
    pub inputs: [WgpuRenderInput; 3],
    pub material: Material2dBinding,
    pub foreign_generation: Material2dGeneration,
    pub other_image: Handle<ImageAsset>,
}

impl Fixture {
    pub fn new(sampler: SamplerDescriptor) -> Self {
        let mut images = Assets::<ImageAsset>::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let [palette, accent, other_image] =
            [DEFAULT_PALETTE, ACCENT, [255, 255, 0, 255]].map(|pixels| {
                let handle = allocate_anonymous_handle().unwrap();
                images
                    .commit_loaded(
                        handle,
                        ImageAsset::new(
                            ImageExtent::new(1, 1),
                            ImageFormat::Rgba8,
                            ImageColorSpace::Linear,
                            pixels.to_vec(),
                        )
                        .unwrap(),
                        &mut states,
                        &mut events,
                        None,
                        None,
                    )
                    .unwrap();
                handle
            });
        let material = generation()
            .bind_values(
                BTreeMap::from([(SCALE_ID.to_owned(), MaterialParameterValue::Vec4(SCALE))]),
                BTreeMap::from([
                    (PALETTE_ID.to_owned(), palette),
                    (ACCENT_ID.to_owned(), accent),
                ]),
            )
            .unwrap();
        let window = Window::new(
            "Independent graph-shared Material",
            WindowResolution::new(scene::WIDTH, scene::HEIGHT),
        );
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            HierarchyPlugin,
            TransformPlugin,
            SpritePlugin,
            WindowPlugin {
                primary_window: Some(window.clone()),
            },
            RenderComponentsPlugin,
            RenderPlugin::default(),
            ImagePreparePlugin,
        ))
        .unwrap();
        app.add_plugins((
            nara::sprite_render::SpriteRenderPlugin,
            WgpuCapturePlugin,
            WgpuStockCapturePlugin,
            WgpuStockSpritePlugin,
            CompositionPlugin(CompositionConfiguration {
                enabled: false,
                ..Default::default()
            }),
        ))
        .unwrap();
        app.add_systems(
            CoreStage::Render,
            consume_original.in_set(WgpuPolicyCaptureSet::Publish),
        )
        .unwrap();
        app.insert_resource(images).unwrap();
        app.insert_resource(states).unwrap();
        let world = app.world_mut().unwrap();
        let sprite = world
            .spawn((
                Sprite::from_texture(accent, Vec2::new(4.0, 2.0))
                    .with_sampler(sampler)
                    .with_tint(Color::rgba(TINT[0], TINT[1], TINT[2], TINT[3])),
                Transform2d::default(),
                material.clone(),
            ))
            .id();
        let camera = world
            .spawn((
                Camera2d {
                    target: RenderTarget::Window(window.id),
                    viewport_height: 6.0,
                    clear_color: Some(Color::rgba(0.0, 0.0, 0.0, 0.0)),
                    ..Default::default()
                },
                Transform2d::default(),
            ))
            .id();
        HierarchyWriter::new(world)
            .apply_batch(&[sprite, camera].map(|entity| HierarchyOperation::Place {
                entity,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            }))
            .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        let mut drive_failure = None;
        for _ in 0..3 {
            if let Err(error) = runtime.drive(Duration::ZERO) {
                drive_failure = Some(error.to_string());
                break;
            }
        }
        let inputs = runtime.world().resource::<RetainedInputs>().0.clone();
        let runtime_generation = runtime.generation();
        runtime.finish().unwrap_or_else(|retirement| {
            panic!("material source Runtime retirement: {retirement:?}")
        });
        assert!(
            drive_failure.is_none(),
            "material source drive: {drive_failure:?}"
        );
        let inputs: [WgpuRenderInput; 3] = inputs.try_into().unwrap();
        let stock = &inputs[0].captured::<CompositionPolicy>().unwrap().stock;
        let batches = stock.sprite_batches().unwrap().as_slice();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].instances.len(), 1);
        assert_eq!(batches[0].material.sampler, sampler);
        assert_eq!(batches[0].material.alpha_mode, AlphaMode2d::Blend);
        let prepared = batches[0].material_extension.as_ref().unwrap();
        assert_eq!(prepared.binding.uniform_bytes(), material.uniform_bytes());
        assert_eq!(prepared.binding.generation(), material.generation());
        assert_eq!(prepared.binding.textures(), material.textures());
        assert_eq!(prepared.textures.len(), 2);
        assert_eq!(prepared.textures[0].0, 1);
        assert_eq!(prepared.textures[0].1.resource().pixels(), DEFAULT_PALETTE);
        assert_eq!(prepared.textures[1].0, 3);
        assert_eq!(prepared.textures[1].1.resource().pixels(), ACCENT);
        assert_eq!(batches[0].image.as_ref().unwrap(), &prepared.textures[1].1);
        for input in &inputs {
            assert_eq!(input.packet().views().len(), 1);
            assert!(input.packet().world_canvas().is_none());
            assert_eq!(input.packet().target().extent().width, scene::WIDTH);
            assert_eq!(input.packet().target().extent().height, scene::HEIGHT);
            assert_eq!(
                input.packet().source_generation().runtime_generation(),
                Some(runtime_generation)
            );
        }
        assert!(inputs[0].packet().frame_index() < inputs[1].packet().frame_index());
        assert!(inputs[1].packet().frame_index() < inputs[2].packet().frame_index());
        Self {
            inputs,
            material,
            foreign_generation: generation(),
            other_image,
        }
    }

    pub fn stock(&self, index: usize) -> Arc<WgpuStockCapture> {
        Arc::clone(
            &self.inputs[index]
                .captured::<CompositionPolicy>()
                .unwrap()
                .stock,
        )
    }
}

fn consume_original(world: &mut World) {
    let input = take_captured_policy_batch(world)
        .expect("the CPU consumer takes the original sealed batch once")
        .into_render_input()
        .expect("this source capture carries no UI presentation responsibility")
        .expect("the ordinary source collector sealed the original input");
    assert!(take_captured_policy_batch(world).is_none());
    let retained = world.resource::<RetainedInputs>().0.last().unwrap();
    assert!(std::ptr::eq(input.packet(), retained.packet()));
}

pub fn generation() -> Material2dGeneration {
    let mut library = Material2dLibrary::default();
    library
        .declare(
            Material2dDeclaration::new(
                MATERIAL_ID,
                1,
                AlphaMode2d::Blend,
                MaterialParameterLayout::new(
                    vec![
                        MaterialUniformField::new(
                            SCALE_ID,
                            "scale",
                            MaterialParameterKind::Vec4,
                            0,
                        )
                        .unwrap(),
                    ],
                    16,
                )
                .unwrap(),
                vec![
                    MaterialTextureDeclaration::new(PALETTE_ID, "palette_texture", 1).unwrap(),
                    MaterialTextureDeclaration::new(ACCENT_ID, "accent_texture", 3).unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let source = Arc::<str>::from(include_str!("material.wgsl"));
    let read = |_: &AssetPath| Ok(Arc::clone(&source));
    let update = library
        .prepare_update(
            MATERIAL_ID,
            &[AssetPath::new("materials/graph-palette.wgsl").unwrap()],
            read,
        )
        .unwrap();
    library.publish(&update, read).unwrap()
}
