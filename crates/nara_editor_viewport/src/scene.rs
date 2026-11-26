use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use nara_app::{App, RealTime, RuntimeInstance};
use nara_asset::{
    AssetEvents, AssetPlugin, AssetSourceKind, AssetSourcePath, AssetStates, Assets, Handle,
    ProjectAssetDatabase, allocate_anonymous_handle,
};
use nara_core::{
    Affine2, AssetId, Color, EntityUuid, Vec2,
    vfs::{AssetPath, DirAssetSource},
};
use nara_ecs::{Component, Entity, EntityWorldMut, Mut, World};
use nara_editor_core::viewport::{LocalBounds2D, PickCandidate2D, ViewportCamera2D};
use nara_editor_gizmo::{OverlayFrame, OverlayPrimitive, OverlayStroke};
use nara_reflect::Schema;
use nara_render::{
    Camera2d, Extent2d, ExtractedViews, RenderImage2d, RenderImagePacket, RenderPlugin,
    RenderTarget, ViewMapping2d, ViewportRect, build_image_render_frame_packet,
    gpu::RenderInput,
    image::{ImageAsset, ImageColorSpace, ImagePreparePlugin},
    sprite::Sprite,
    sprite_render::{SpriteRenderPlugin, SpriteRenderStats, batching::QueuedSpriteItems},
};
use nara_scene::{
    authoring::{ColliderSource, SceneDocument, SceneEntity, SpriteSource},
    spatial::{
        ChildOf, GlobalTransform2d, InheritedVisibility, SpatialPlugin, Transform2d, Visibility,
    },
};

use crate::ViewportError;

#[cfg(test)]
mod tests;

const MAX_PNG_READ: u64 = 16 * 1024 * 1024 + 1;
const CIRCLE_SEGMENTS: usize = 48;

#[derive(Component)]
struct PreviewEntity(EntityUuid);

pub(crate) struct PreviewScene {
    runtime: RuntimeInstance,
    camera: Entity,
    target: Handle<RenderImage2d>,
    projection: Option<(ViewportCamera2D, Extent2d)>,
    document: Option<SceneDocument>,
    asset_root: Option<PathBuf>,
    entities: HashMap<EntityUuid, Entity>,
    images: HashMap<AssetId, PreviewImage>,
}

struct PreviewImage {
    path: AssetSourcePath,
    handle: Handle<ImageAsset>,
}

impl PreviewScene {
    pub(crate) fn new() -> Result<Self, ViewportError> {
        let mut app = App::new();
        app.add_plugins((
            AssetPlugin,
            SpatialPlugin,
            RenderPlugin::default(),
            ImagePreparePlugin,
            SpriteRenderPlugin,
        ))?;
        let target = RenderImage2d::allocate_handle()?;
        let camera = app
            .world_mut()?
            .spawn((
                Camera2d {
                    target: RenderTarget::Image(target),
                    clear_color: Some(Color::rgb(0.018, 0.024, 0.035)),
                    ..Camera2d::default()
                },
                Transform2d::IDENTITY,
            ))
            .id();
        Ok(Self {
            runtime: RuntimeInstance::start(app)?,
            camera,
            target,
            projection: None,
            document: None,
            asset_root: None,
            entities: HashMap::new(),
            images: HashMap::new(),
        })
    }

    pub(crate) fn set_document(
        &mut self,
        document: &SceneDocument,
        assets: Option<(&DirAssetSource, &ProjectAssetDatabase)>,
    ) -> Result<(), ViewportError> {
        let root = assets.map(|(source, _)| source.root());
        let same_root = self.asset_root.as_deref() == root;
        let same_document = self.document.as_ref() == Some(document);
        if !same_document {
            document.validate()?;
        }
        let rows = document
            .entities
            .iter()
            .map(SceneRow::read)
            .collect::<Result<Vec<_>, _>>()?;

        let mut images = HashMap::new();
        let mut pending = Vec::new();
        if let Some((source, database)) = assets {
            let installed = self.runtime.world().resource::<Assets<ImageAsset>>();
            for id in rows.iter().filter_map(SceneRow::image) {
                let Entry::Vacant(entry) = images.entry(id) else {
                    continue;
                };
                let path = image_path(database, id)?;
                let cached = self.images.get(&id).filter(|_| same_root);
                let handle = if let Some(cached) = cached.filter(|image| &image.path == path) {
                    cached.handle
                } else {
                    let image = read_png(source, path)?;
                    let handle = match cached {
                        Some(cached) => cached.handle,
                        None => allocate_anonymous_handle()?,
                    };
                    if installed
                        .get(handle)
                        .is_none_or(|previous| previous.content_hash() != image.content_hash())
                    {
                        pending.push((handle, image));
                    }
                    handle
                };
                entry.insert(PreviewImage {
                    path: path.clone(),
                    handle,
                });
            }
        }
        let retired = self
            .images
            .iter()
            .filter_map(|(id, previous)| {
                images
                    .get(id)
                    .is_none_or(|image| image.handle != previous.handle)
                    .then_some(previous.handle)
            })
            .collect::<Vec<_>>();

        if !same_document || !pending.is_empty() || !retired.is_empty() {
            self.runtime
                .with_driver_scope(|scope| {
                    scope.with_world_mut(|world| {
                        publish_images(world, pending, retired)?;
                        apply_rows(world, &mut self.entities, rows, &images);
                        Ok::<(), ViewportError>(())
                    })
                })?
                .output?;
            self.runtime.drive(Duration::ZERO)?;
        }
        self.images = images;
        if !same_root {
            self.asset_root = root.map(Path::to_path_buf);
        }
        if !same_document {
            self.document = Some(document.clone());
        }
        Ok(())
    }

    pub(crate) fn render_input(
        &mut self,
        camera: ViewportCamera2D,
        extent: Extent2d,
    ) -> Result<RenderInput, ViewportError> {
        if self.projection != Some((camera, extent)) {
            let (transform, viewport, viewport_height) = camera_geometry(camera, extent)?;
            self.runtime
                .with_driver_scope(|scope| {
                    scope.with_world_mut(|world| {
                        let mut entity = world
                            .get_entity_mut(self.camera)
                            .map_err(|_| ViewportError::MissingCamera)?;
                        let mut camera = entity
                            .get_mut::<Camera2d>()
                            .ok_or(ViewportError::MissingCamera)?;
                        camera.viewport = Some(viewport);
                        camera.viewport_height = viewport_height;
                        entity.insert(transform);
                        Ok::<(), ViewportError>(())
                    })
                })?
                .output?;
            self.runtime.drive(Duration::ZERO)?;
            self.projection = Some((camera, extent));
        }
        let world = self.runtime.world();
        let packet = build_image_render_frame_packet(
            self.runtime.generation().into(),
            world.resource::<RealTime>().frame,
            world.resource::<ExtractedViews>(),
            RenderImagePacket::new(self.target, extent)?,
        )?
        .ok_or(ViewportError::MissingCamera)?;
        Ok(RenderInput::capture(world, packet))
    }

    pub(crate) fn extraction(&self) -> SpriteRenderStats {
        *self.runtime.world().resource::<SpriteRenderStats>()
    }

    pub(crate) fn collect_picks(&self, output: &mut Vec<PickCandidate2D>) {
        let world = self.runtime.world();
        output.clear();
        output.extend(
            world
                .resource::<QueuedSpriteItems>()
                .as_slice()
                .iter()
                .filter_map(|item| sprite_quad(world, item.entity)),
        );
    }

    pub(crate) fn selection_quad(&self, entity: EntityUuid) -> Option<PickCandidate2D> {
        sprite_quad(self.runtime.world(), *self.entities.get(&entity)?)
    }

    pub(crate) fn scene_bounds(&self, selection: &[EntityUuid]) -> Option<[Vec2; 2]> {
        let world = self.runtime.world();
        let mut corners = self
            .entities
            .iter()
            .filter(|(id, _)| selection.is_empty() || selection.contains(id))
            .filter_map(|(_, &entity)| sprite_quad(world, entity))
            .flat_map(|quad| quad.world_corners().map(|corner| corner.0));
        let first = corners.next()?;
        Some(corners.fold([first, first], |[min, max], corner| {
            [min.min(corner), max.max(corner)]
        }))
    }

    pub(crate) fn world_affine(&self, entity: EntityUuid) -> Option<Affine2> {
        self.runtime
            .world()
            .get::<GlobalTransform2d>(*self.entities.get(&entity)?)
            .map(GlobalTransform2d::affine)
    }

    pub(crate) fn parent_affine(&self, entity: EntityUuid) -> Option<Affine2> {
        let entity = *self.entities.get(&entity)?;
        let world = self.runtime.world();
        match world.get::<ChildOf>(entity) {
            Some(link) => world
                .get::<GlobalTransform2d>(link.parent())
                .map(GlobalTransform2d::affine),
            None => Some(Affine2::IDENTITY),
        }
    }

    pub(crate) fn append_colliders(&self, frame: &mut OverlayFrame, stroke: OverlayStroke) {
        let world = self.runtime.world();
        for &entity in self.entities.values() {
            if let (Some(collider), Some(affine)) = (
                world.get::<ColliderSource>(entity),
                visible_affine(world, entity),
            ) {
                append_collider(&mut frame.primitives, *collider, affine, stroke);
            }
        }
    }
}

struct SceneRow {
    id: EntityUuid,
    parent: Option<EntityUuid>,
    transform: Transform2d,
    visibility: Visibility,
    sprite: Option<SpriteSource>,
    collider: Option<ColliderSource>,
}

impl SceneRow {
    fn read(entity: &SceneEntity) -> Result<Self, ViewportError> {
        Ok(Self {
            id: entity.id,
            parent: entity.parent,
            transform: typed::<Transform2d>(entity)?.unwrap_or_default(),
            visibility: typed::<Visibility>(entity)?.unwrap_or_default(),
            sprite: typed::<SpriteSource>(entity)?,
            collider: typed::<ColliderSource>(entity)?,
        })
    }

    fn image(&self) -> Option<AssetId> {
        self.sprite.as_ref()?.image
    }

    fn renderer_sprite(&self, images: &HashMap<AssetId, PreviewImage>) -> Option<Sprite> {
        let source = self.sprite.as_ref()?;
        let image = source.image.and_then(|id| images.get(&id));
        Some(
            image
                .map_or_else(
                    || Sprite::from_color(source.size, source.tint()),
                    |image| {
                        Sprite::from_texture(image.handle, source.size).with_tint(source.tint())
                    },
                )
                .with_layer(source.layer),
        )
    }
}

fn typed<T: Schema>(entity: &SceneEntity) -> Result<Option<T>, ViewportError> {
    entity
        .typed::<T>()
        .map_err(|source| ViewportError::Component {
            entity: entity.id,
            component: T::COMPONENT_ID,
            source,
        })
}

fn apply_rows(
    world: &mut World,
    entities: &mut HashMap<EntityUuid, Entity>,
    rows: Vec<SceneRow>,
    images: &HashMap<AssetId, PreviewImage>,
) {
    let mut live = HashSet::with_capacity(rows.len());
    for row in &rows {
        live.insert(row.id);
        entities.entry(row.id).or_insert_with(|| {
            world
                .spawn((PreviewEntity(row.id), row.transform, row.visibility))
                .id()
        });
    }

    for row in &rows {
        let mut entity = world.entity_mut(entities[&row.id]);
        let parent = row.parent.and_then(|id| entities.get(&id).copied());
        if entity.get::<ChildOf>().map(ChildOf::parent) != parent {
            entity.remove::<ChildOf>();
        }
    }
    entities.retain(|id, entity| {
        if live.contains(id) {
            true
        } else {
            if let Ok(entity) = world.get_entity_mut(*entity) {
                entity.despawn();
            }
            false
        }
    });

    for row in rows {
        let mut entity = world.entity_mut(entities[&row.id]);
        if entity.get::<Transform2d>() != Some(&row.transform) {
            entity.insert(row.transform);
        }
        if entity.get::<Visibility>() != Some(&row.visibility) {
            entity.insert(row.visibility);
        }
        set_optional(&mut entity, row.renderer_sprite(images));
        set_optional(&mut entity, row.collider);
        if entity.get::<ChildOf>().is_none()
            && let Some(parent) = row.parent.and_then(|id| entities.get(&id).copied())
        {
            entity.insert(ChildOf(parent));
        }
    }
}

fn set_optional<T: Component + PartialEq>(entity: &mut EntityWorldMut<'_>, value: Option<T>) {
    if entity.get::<T>() != value.as_ref() {
        if let Some(value) = value {
            entity.insert(value);
        } else {
            entity.remove::<T>();
        }
    }
}

fn image_path(
    database: &ProjectAssetDatabase,
    id: AssetId,
) -> Result<&AssetSourcePath, ViewportError> {
    let record = database
        .record_for_stable_id(id.into())
        .ok_or(ViewportError::MissingImageAsset { id })?;
    if record.source_kind() != &AssetSourceKind::Image {
        return Err(ViewportError::InvalidImageAssetKind {
            id,
            path: record.path().to_string(),
            kind: record.source_kind().clone(),
        });
    }
    Ok(record.path())
}

fn read_png(source: &DirAssetSource, path: &AssetSourcePath) -> Result<ImageAsset, ViewportError> {
    let mut bytes = Vec::new();
    AssetPath::new(path.as_str())
        .and_then(|path| source.open(&path))
        .and_then(|file| file.take(MAX_PNG_READ).read_to_end(&mut bytes))
        .map_err(|source| ViewportError::ReadImage {
            path: path.to_string(),
            source,
        })?;
    ImageAsset::from_png(&bytes, ImageColorSpace::Srgb).map_err(|source| {
        ViewportError::DecodeImage {
            path: path.to_string(),
            source,
        }
    })
}

fn publish_images(
    world: &mut World,
    pending: Vec<(Handle<ImageAsset>, ImageAsset)>,
    retired: Vec<Handle<ImageAsset>>,
) -> Result<(), ViewportError> {
    if pending.is_empty() && retired.is_empty() {
        return Ok(());
    }
    world.resource_scope(|world, mut assets: Mut<Assets<ImageAsset>>| {
        world.resource_scope(|world, mut states: Mut<AssetStates>| {
            let mut events = world.resource_mut::<AssetEvents>();
            for (handle, image) in pending {
                assets.commit_loaded(handle, image, &mut states, &mut events)?;
            }
            for handle in retired {
                assets.remove_with_state(handle, &mut states, &mut events)?;
            }
            Ok(())
        })
    })
}

fn camera_geometry(
    camera: ViewportCamera2D,
    extent: Extent2d,
) -> Result<(Transform2d, ViewportRect, f32), ViewportError> {
    let viewport = ViewportRect::from_extent(extent).ok_or(ViewportError::InvalidExtent)?;
    let logical = camera.panel_size().as_vec2().as_dvec2();
    let aspect_adjustment =
        (logical.x * f64::from(extent.height)) / (logical.y * f64::from(extent.width));
    let viewport_height = (logical.y / f64::from(camera.zoom())) as f32;
    let transform = Transform2d {
        translation: camera.center().0,
        scale: Vec2::new(aspect_adjustment as f32, 1.0),
        ..Transform2d::IDENTITY
    };
    ViewMapping2d::orthographic(transform.matrix(), viewport_height, viewport)
        .ok_or(ViewportError::InvalidCamera)?;
    Ok((transform, viewport, viewport_height))
}

fn visible_affine(world: &World, entity: Entity) -> Option<Affine2> {
    if !world.get::<InheritedVisibility>(entity)?.get() {
        return None;
    }
    let affine = world.get::<GlobalTransform2d>(entity)?.affine();
    affine.is_finite().then_some(affine)
}

fn sprite_quad(world: &World, entity: Entity) -> Option<PickCandidate2D> {
    let id = world.get::<PreviewEntity>(entity)?.0;
    let sprite = world.get::<Sprite>(entity)?;
    let center = -sprite.anchor.normalized * sprite.size;
    let half_size = sprite.size.abs() * 0.5;
    let bounds = LocalBounds2D::new(center - half_size, center + half_size)?;
    PickCandidate2D::new(id, bounds, visible_affine(world, entity)?)
}

fn append_collider(
    output: &mut Vec<OverlayPrimitive>,
    collider: ColliderSource,
    affine: Affine2,
    stroke: OverlayStroke,
) {
    if let Some(radius) = collider.radius {
        if !radius.is_finite() || radius <= 0.0 {
            return;
        }
        let start = affine.transform_point2(Vec2::new(radius, 0.0));
        let mut from = start;
        for index in 1..CIRCLE_SEGMENTS {
            let angle = std::f32::consts::TAU * index as f32 / CIRCLE_SEGMENTS as f32;
            let to = affine.transform_point2(Vec2::from_angle(angle) * radius);
            output.push(OverlayPrimitive::Line { from, to, stroke });
            from = to;
        }
        output.push(OverlayPrimitive::Line {
            from,
            to: start,
            stroke,
        });
    } else {
        let half = collider.half_extents;
        if half.is_finite() && half.min_element() > 0.0 {
            output.push(OverlayPrimitive::Rect {
                corners: [
                    -half,
                    Vec2::new(half.x, -half.y),
                    half,
                    Vec2::new(-half.x, half.y),
                ]
                .map(|point| affine.transform_point2(point)),
                stroke,
            });
        }
    }
}
