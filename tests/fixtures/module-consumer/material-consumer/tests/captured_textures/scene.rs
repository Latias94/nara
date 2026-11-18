//! Real managed frames with an adversarial slot write between Extract and Image Prepare.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use nara::{
    ProductConfiguration,
    advanced_prelude::CurrentScene,
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{AssetEvents, AssetServer, AssetStates, AssetVersion, Assets, Handle},
    core::{Color, Vec2},
    ecs::{Mut, Resource, World, schedule::IntoScheduleConfigs},
    hierarchy::{HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    identity::EntityLookup,
    image::{
        ImageAsset, ImageColorSpace, ImageContentSnapshot, ImageExtent, ImageFormat,
        PreparedImages, image_resource_key, prepare_images,
    },
    material::{Material2d, Material2dBinding, SamplerDescriptor},
    render::{Camera2d, RenderTarget},
    render_wgpu::{
        WgpuPolicyCaptureSet, WgpuRenderInput, WgpuStockPolicy, take_captured_policy_batch,
    },
    sprite::Sprite,
    transform::Transform2d,
    window::{Window, WindowResolution},
};
use nara_material_module_consumer::{palette_sprite, startup_materials};

use crate::{expected, project};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Authored,
    Direct,
}

impl Mode {
    pub(super) fn frames(self) -> u32 {
        match self {
            Self::Authored => 3,
            Self::Direct => 2,
        }
    }

    pub(super) fn palette(self, frame: u32) -> Option<[u8; 4]> {
        match (self, frame) {
            (_, 1) => Some(expected::A),
            (Self::Authored, 2) => None,
            _ => Some(expected::B),
        }
    }
}

#[derive(Clone, Resource)]
pub(super) struct Proof {
    mode: Mode,
    world_dropped: Arc<AtomicBool>,
    pixels_dropped: Arc<AtomicBool>,
}

impl Proof {
    pub(super) fn new(mode: Mode) -> Self {
        Self {
            mode,
            world_dropped: Arc::default(),
            pixels_dropped: Arc::default(),
        }
    }

    pub(super) fn assert_retained(&self) {
        assert!(
            !self.pixels_dropped.load(Ordering::SeqCst),
            "original A pixel custody"
        );
    }

    pub(super) fn assert_world_dropped(&self) {
        assert!(
            self.world_dropped.load(Ordering::SeqCst),
            "original source World retired"
        );
    }

    pub(super) fn assert_pixels_dropped(&self) {
        assert!(
            self.pixels_dropped.load(Ordering::SeqCst),
            "original A pixel custody released"
        );
    }
}

impl ProductConfiguration for Proof {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"consumer.palette.captured-textures.v1\0");
        output.push(match self.mode {
            Mode::Authored => 0,
            Mode::Direct => 1,
        });
    }
}

#[derive(Resource)]
struct WorldLifetime {
    armed: bool,
    dropped: Arc<AtomicBool>,
}

impl Drop for WorldLifetime {
    fn drop(&mut self) {
        if self.armed {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }
}

struct PixelLifetime(Arc<AtomicBool>);

impl Drop for PixelLifetime {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[derive(Default, Resource)]
struct State {
    frame: u32,
    palette: Option<Handle<ImageAsset>>,
    version: Option<AssetVersion>,
    latest: Option<WgpuRenderInput>,
}

pub(super) struct Capture(pub(super) Proof);

impl Plugin for Capture {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.captured-textures"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.0.clone())?
            .insert_resource(WorldLifetime {
                armed: false,
                dropped: Arc::clone(&self.0.world_dropped),
            })?
            .init_resource::<State>()?
            .add_systems(CoreStage::Update, update)?
            .add_systems(
                CoreStage::Prepare,
                replace_after_extract.before(prepare_images),
            )?
            .add_systems(
                CoreStage::Render,
                capture.in_set(WgpuPolicyCaptureSet::Publish),
            )?;
        Ok(())
    }
}

fn palette_image(mode: Mode, pixels: [u8; 4]) -> ImageAsset {
    match mode {
        Mode::Authored => ImageAsset::from_product(
            project::palette_product(),
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Linear,
            pixels.to_vec(),
        ),
        Mode::Direct => ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Linear,
            pixels.to_vec(),
        ),
    }
    .unwrap()
}

fn publish(world: &mut World, handle: Handle<ImageAsset>, image: ImageAsset) -> AssetVersion {
    world.resource_scope(|world, mut images: Mut<Assets<ImageAsset>>| {
        world.resource_scope(|world, mut states: Mut<AssetStates>| {
            images
                .commit_loaded(
                    handle,
                    image,
                    &mut states,
                    &mut world.resource_mut::<AssetEvents>(),
                    None,
                    None,
                )
                .unwrap()
        })
    })
}

fn initialize(world: &mut World, proof: &Proof, state: &mut State) {
    assert!(!proof.world_dropped.load(Ordering::SeqCst));
    world.resource_mut::<WorldLifetime>().armed = true;
    let entity = match CurrentScene::get(world)
        .unwrap()
        .receipt()
        .resolve(world, &project::actor())
    {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("original authored Sprite must be resident: {other:?}"),
    };
    assert!(world.get::<Material2d>(entity).is_some());
    assert!(world.get::<Material2dBinding>(entity).is_none());
    let indices = world.get::<Sprite>(entity).unwrap().material.image.unwrap();
    let palette = match proof.mode {
        Mode::Authored => world
            .resource::<AssetServer>()
            .get_product_handle::<ImageAsset>(project::palette_product())
            .unwrap(),
        Mode::Direct => {
            let handle = world
                .resource_mut::<AssetServer>()
                .reserve_anonymous::<ImageAsset>()
                .unwrap();
            let (_, binding) =
                palette_sprite(&startup_materials().unwrap(), indices, handle, 0.0).unwrap();
            world
                .entity_mut(entity)
                .remove::<Material2d>()
                .insert(binding);
            handle
        }
    };
    let mut image = palette_image(proof.mode, expected::A);
    let owner = Arc::new(PixelLifetime(Arc::clone(&proof.pixels_dropped)));
    assert!(image.try_attach_retention_owner(owner));
    state.version = Some(publish(world, palette, image));
    state.palette = Some(palette);

    {
        let mut sprite = world.get_mut::<Sprite>(entity).unwrap();
        sprite.size = Vec2::new(10.0, 6.0);
        sprite.material.tint = Color::WHITE;
        sprite.material.sampler = SamplerDescriptor::NEAREST_CLAMP;
    }
    world
        .entity_mut(entity)
        .insert(Transform2d::from_translation(Vec2::new(-3.0, 2.0)));
    assert_eq!(world.query::<&Window>().iter(world).count(), 0);
    let window = Window::new(
        "Captured palette F4",
        WindowResolution::new(expected::WIDTH, expected::HEIGHT),
    );
    let target = RenderTarget::Window(window.id);
    world.spawn(window);
    let camera = world
        .spawn((
            Camera2d {
                target,
                viewport_height: expected::HEIGHT as f32,
                clear_color: Some(Color::rgba(0.0, 0.0, 0.0, 0.0)),
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    HierarchyWriter::new(world)
        .apply_batch(&[HierarchyOperation::Place {
            entity: camera,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        }])
        .unwrap();
}

fn update(world: &mut World) {
    let proof = world.resource::<Proof>().clone();
    world.resource_scope(|world, mut state: Mut<State>| {
        state.frame += 1;
        assert!(
            state.frame <= proof.mode.frames(),
            "finite consumer frame count"
        );
        if state.frame == 1 {
            initialize(world, &proof, &mut state);
        } else if state.frame == proof.mode.frames() {
            let version = publish(
                world,
                state.palette.unwrap(),
                palette_image(proof.mode, expected::B),
            );
            assert_ne!(
                Some(version),
                state.version,
                "B requires an actual publication"
            );
            state.version = Some(version);
        }
    });
}

/// The public stage order completes Extract before Prepare, and the explicit edge places this
/// adversarial write before the stock Image preparation system. No private Extract set is needed.
fn replace_after_extract(world: &mut World) {
    let state = world.resource::<State>();
    if state.frame != 1 {
        return;
    }
    let handle = state.palette.unwrap();
    let version = state.version.unwrap();
    let mode = world.resource::<Proof>().mode;
    let previous = world
        .resource_mut::<Assets<ImageAsset>>()
        .insert(handle, palette_image(mode, expected::B))
        .unwrap();
    assert_eq!(previous.pixels(), expected::A);
    assert_eq!(
        world
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .usable_version(),
        Some(version),
    );
    assert_ne!(
        world
            .resource::<AssetStates>()
            .published_slot_revision(handle.id()),
        Some(&world.resource::<Assets<ImageAsset>>().slot_revision(handle)),
        "the replacement must remain unmanaged until commit_loaded",
    );
}

fn capture(world: &mut World) {
    let batch = take_captured_policy_batch(world).expect("one original stock Render capture");
    assert!(take_captured_policy_batch(world).is_none());
    let input = batch
        .into_render_input()
        .unwrap()
        .expect("the real logical View");
    assert_eq!(input.packet().views().len(), 1);
    let extent = input.packet().target().extent();
    assert_eq!(
        (extent.width, extent.height),
        (expected::WIDTH, expected::HEIGHT)
    );

    let state = world.resource::<State>();
    let proof = world.resource::<Proof>();
    let handle = state.palette.unwrap();
    let images = world.resource::<Assets<ImageAsset>>();
    assert_eq!(images.get(handle).unwrap().pixels(), expected::B);
    let prepared = world
        .resource::<PreparedImages>()
        .get(image_resource_key(handle))
        .unwrap();
    assert_eq!(
        prepared.resource().pixels(),
        expected::B,
        "stock Prepare must observe the newer slot"
    );
    assert_eq!(
        prepared.identity().slot_revision(),
        &images.slot_revision(handle)
    );
    if proof.mode == Mode::Authored {
        let authorized = ImageContentSnapshot::capture(
            handle,
            world.resource::<AssetServer>(),
            world.resource::<AssetStates>(),
            images,
        );
        assert_eq!(
            authorized.is_ok(),
            state.frame == proof.mode.frames(),
            "only committed B is a fresh managed publication"
        );
    }

    let stock = input.captured::<WgpuStockPolicy>().unwrap();
    let batches = stock.sprite_batches().unwrap().as_slice();
    if let Some(pixels) = proof.mode.palette(state.frame) {
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].instances.len(), 1);
        let material = batches[0].material_extension.as_ref().unwrap();
        assert_eq!(material.binding.textures()["palette"], handle);
        assert_eq!(material.textures.len(), 1);
        let texture = &material.textures[0].1;
        assert_eq!(texture.resource().pixels(), pixels);
        assert_eq!(texture.identity().asset_version(), state.version.unwrap());
        if state.frame == 1 {
            assert_ne!(
                texture.identity().slot_revision(),
                prepared.identity().slot_revision()
            );
            assert!(texture.resource().has_retention_owner());
        } else {
            assert_eq!(texture.identity(), prepared.identity());
        }
    } else {
        assert!(
            batches.is_empty(),
            "invalid managed publication must omit the whole item"
        );
    }
    world.resource_mut::<State>().latest = Some(input);
}

pub(super) fn latest(world: &World) -> (u32, WgpuRenderInput) {
    let state = world.resource::<State>();
    (state.frame, state.latest.as_ref().unwrap().clone())
}
