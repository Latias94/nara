//! Sprite authoring data for 2D scenes.

use nara_app::{App, Plugin, PluginError};
use nara_asset::{AssetRef, AssetRefError, AssetServer, AssetSourceKind, Handle};
use nara_core::{Color, Vec2};
use nara_ecs::{Component, World};
use nara_image::ImageAsset;
use nara_material::{AddressMode, AlphaMode2d, FilterMode, SamplerDescriptor};
use nara_reflect::{
    ComponentCodecError, ComponentDecodeContext, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentRegistryError, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, PreparedComponent,
};

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextureRegion {
    pub min: Vec2,
    pub size: Vec2,
}

impl TextureRegion {
    pub const FULL: Self = Self {
        min: Vec2::ZERO,
        size: Vec2::ONE,
    };

    #[must_use]
    pub const fn new(min: Vec2, size: Vec2) -> Self {
        Self { min, size }
    }

    #[must_use]
    pub fn from_pixels(min: Vec2, size: Vec2, image_size: Vec2) -> Option<Self> {
        if image_size.x <= 0.0
            || image_size.y <= 0.0
            || !image_size.is_finite()
            || !min.is_finite()
            || !size.is_finite()
            || size.x <= 0.0
            || size.y <= 0.0
        {
            return None;
        }

        let region = Self::new(min / image_size, size / image_size);
        region.is_valid_uv().then_some(region)
    }

    #[must_use]
    pub fn max(self) -> Vec2 {
        self.min + self.size
    }

    #[must_use]
    pub fn is_valid_uv(self) -> bool {
        self.min.is_finite()
            && self.size.is_finite()
            && self.size.x > 0.0
            && self.size.y > 0.0
            && self.min.x >= 0.0
            && self.min.y >= 0.0
            && self.max().x <= 1.0
            && self.max().y <= 1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SpriteAnchor {
    pub normalized: Vec2,
}

impl SpriteAnchor {
    pub const CENTER: Self = Self {
        normalized: Vec2::ZERO,
    };
}

impl Default for SpriteAnchor {
    fn default() -> Self {
        Self::CENTER
    }
}

#[derive(Debug, Clone, PartialEq, Component)]
pub struct Sprite {
    pub material: SpriteMaterial,
    pub texture_region: Option<TextureRegion>,
    pub size: Vec2,
    pub anchor: SpriteAnchor,
    pub layer: i32,
    pub sort_key: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteMaterial {
    pub image: Option<Handle<ImageAsset>>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: Color,
}

impl SpriteMaterial {
    #[must_use]
    pub fn from_color(tint: Color) -> Self {
        Self {
            image: None,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint,
        }
    }

    #[must_use]
    pub fn from_image(image: Handle<ImageAsset>) -> Self {
        Self {
            image: Some(image),
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint: Color::WHITE,
        }
    }

    #[must_use]
    pub const fn with_sampler(mut self, sampler: SamplerDescriptor) -> Self {
        self.sampler = sampler;
        self
    }

    #[must_use]
    pub const fn with_alpha_mode(mut self, alpha_mode: AlphaMode2d) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }

    #[must_use]
    pub const fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }
}

impl Default for SpriteMaterial {
    fn default() -> Self {
        Self::from_color(Color::WHITE)
    }
}

impl Sprite {
    #[must_use]
    pub fn from_color(size: Vec2, color: Color) -> Self {
        Self {
            material: SpriteMaterial::from_color(color),
            texture_region: None,
            size,
            anchor: SpriteAnchor::CENTER,
            layer: 0,
            sort_key: 0,
        }
    }

    #[must_use]
    pub fn from_texture(texture: Handle<ImageAsset>, size: Vec2) -> Self {
        Self {
            material: SpriteMaterial::from_image(texture),
            texture_region: None,
            size,
            anchor: SpriteAnchor::CENTER,
            layer: 0,
            sort_key: 0,
        }
    }

    #[must_use]
    pub fn with_texture_region(mut self, region: TextureRegion) -> Self {
        self.texture_region = Some(region);
        self
    }

    #[must_use]
    pub const fn with_sampler(mut self, sampler: SamplerDescriptor) -> Self {
        self.material.sampler = sampler;
        self
    }

    #[must_use]
    pub const fn with_alpha_mode(mut self, alpha_mode: AlphaMode2d) -> Self {
        self.material.alpha_mode = alpha_mode;
        self
    }

    #[must_use]
    pub const fn with_tint(mut self, tint: Color) -> Self {
        self.material.tint = tint;
        self
    }

    #[must_use]
    pub fn with_layer(mut self, layer: i32) -> Self {
        self.layer = layer;
        self
    }

    #[must_use]
    pub fn with_sort_key(mut self, sort_key: i32) -> Self {
        self.sort_key = sort_key;
        self
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.sprite"),
            nara_app::PluginCategory::Runtime,
        )
    }

    fn preflight(&self, app: &App) -> Result<(), PluginError> {
        let Some(registry) = app.world().get_resource::<ComponentRegistry>() else {
            return Ok(());
        };
        let component_id = ComponentTypeId::new("nara.sprite.Sprite");
        registry
            .validate_component_registration::<Sprite>(&component_id)
            .map_err(|error| {
                PluginError::component_registration(self.plugin_id(), component_id.as_str(), error)
            })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<ComponentRegistry>()?;
        let component_id = ComponentTypeId::new("nara.sprite.Sprite");
        register_sprite_components(&mut app.world_mut()?.resource_mut::<ComponentRegistry>())
            .map_err(|error| {
                PluginError::component_registration(self.plugin_id(), component_id.as_str(), error)
            })?;
        Ok(())
    }
}

pub fn register_sprite_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    let component_id = ComponentTypeId::new("nara.sprite.Sprite");
    registry.register_component_codec_with_context_and_fields::<Sprite, _, _>(
        component_id.clone(),
        ComponentSchemaVersion(1),
        sprite_fields(),
        |value, context| {
            let size = read_vec2(value.field("size")?, "size")?;
            let material = read_sprite_material(value.field("material")?, context)?;
            let texture_region =
                read_optional_texture_region(value.get("texture_region"), "texture_region")?;
            let layer = optional_i32(value, "layer")?.unwrap_or(0);
            let sort_key = optional_i32(value, "sort_key")?.unwrap_or(0);

            Ok(PreparedComponent::new(move |world, entity| {
                let material = resolve_prepared_material(world, material)?;
                let sprite = Sprite {
                    material,
                    texture_region,
                    size,
                    anchor: SpriteAnchor::CENTER,
                    layer,
                    sort_key,
                };
                let mut entity_mut = world
                    .get_entity_mut(entity)
                    .map_err(|_| ComponentCodecError::EntityMissing)?;
                entity_mut.insert(sprite);
                Ok(())
            }))
        },
        |world, entity, context| {
            let Some(sprite) = world.get::<Sprite>(entity) else {
                return Ok(None);
            };
            let image = match sprite.material.image {
                Some(handle) => Some(asset_ref_value(
                    &AssetRef::from_handle_with_policy(
                        world.get_resource::<AssetServer>().ok_or_else(|| {
                            ComponentCodecError::Message(
                                "AssetServer resource is missing".to_string(),
                            )
                        })?,
                        handle,
                        context.asset_ref_export_policy(),
                    )
                    .map_err(|error| ComponentCodecError::Message(error.to_string()))?,
                )?),
                None => None,
            };
            let material = material_value(
                image.unwrap_or(ComponentValue::Null),
                sprite.material.sampler,
                sprite.material.alpha_mode,
                sprite.material.tint,
            )?;

            let mut fields = vec![
                ("size", vec2_value(sprite.size)?),
                ("material", material),
                ("layer", ComponentValue::I64(i64::from(sprite.layer))),
                ("sort_key", ComponentValue::I64(i64::from(sprite.sort_key))),
            ];
            fields.push((
                "texture_region",
                sprite
                    .texture_region
                    .map(texture_region_value)
                    .transpose()?
                    .unwrap_or(ComponentValue::Null),
            ));
            Ok(Some(ComponentValue::map(fields)))
        },
    )?;
    Ok(())
}

fn sprite_fields() -> [ComponentFieldSchema; 16] {
    [
        ComponentFieldSchema::required(
            ComponentFieldPath::from_fields(["size", "x"]),
            ComponentValueKind::F64,
        ),
        ComponentFieldSchema::required(
            ComponentFieldPath::from_fields(["size", "y"]),
            ComponentValueKind::F64,
        ),
        ComponentFieldSchema::required(
            ComponentFieldPath::from_fields(["material", "tint", "r"]),
            ComponentValueKind::F64,
        ),
        ComponentFieldSchema::required(
            ComponentFieldPath::from_fields(["material", "tint", "g"]),
            ComponentValueKind::F64,
        ),
        ComponentFieldSchema::required(
            ComponentFieldPath::from_fields(["material", "tint", "b"]),
            ComponentValueKind::F64,
        ),
        ComponentFieldSchema::required(
            ComponentFieldPath::from_fields(["material", "tint", "a"]),
            ComponentValueKind::F64,
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["material", "image"]),
            ComponentValueKind::AssetRef,
            ComponentValue::Null,
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["material", "sampler", "min_filter"]),
            ComponentValueKind::String,
            ComponentValue::String("linear".to_string()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["material", "sampler", "mag_filter"]),
            ComponentValueKind::String,
            ComponentValue::String("linear".to_string()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["material", "sampler", "mipmap_filter"]),
            ComponentValueKind::String,
            ComponentValue::String("linear".to_string()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["material", "sampler", "address_mode_u"]),
            ComponentValueKind::String,
            ComponentValue::String("clamp_to_edge".to_string()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["material", "sampler", "address_mode_v"]),
            ComponentValueKind::String,
            ComponentValue::String("clamp_to_edge".to_string()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["material", "alpha_mode"]),
            ComponentValueKind::String,
            ComponentValue::String("blend".to_string()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["layer"]),
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["sort_key"]),
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["texture_region"]),
            ComponentValueKind::Map,
            ComponentValue::Null,
        ),
    ]
}

#[derive(Debug, Clone)]
enum PreparedTexture {
    Resolved(Handle<ImageAsset>),
    Deferred(AssetRef),
}

#[derive(Debug, Clone)]
struct PreparedSpriteMaterial {
    image: Option<PreparedTexture>,
    sampler: SamplerDescriptor,
    alpha_mode: AlphaMode2d,
    tint: Color,
}

fn read_sprite_material(
    value: &ComponentValue,
    context: &mut ComponentDecodeContext<'_>,
) -> Result<PreparedSpriteMaterial, ComponentCodecError> {
    let tint = read_color(value.field("tint")?, "material.tint")?;
    let image_ref = read_optional_asset_ref(value.get("image"), "material.image")?;
    let image = prepare_optional_texture(context, image_ref, "material.image.value")?;
    let sampler = read_sampler(value.get("sampler"), "material.sampler")?;
    let alpha_mode = read_alpha_mode(value.get("alpha_mode"), "material.alpha_mode")?;

    Ok(PreparedSpriteMaterial {
        image,
        sampler,
        alpha_mode,
        tint,
    })
}

fn prepare_optional_texture(
    context: &mut ComponentDecodeContext<'_>,
    texture_ref: Option<AssetRef>,
    field: &str,
) -> Result<Option<PreparedTexture>, ComponentCodecError> {
    let Some(texture_ref) = texture_ref else {
        return Ok(None);
    };
    prepare_texture_handle(context, field, texture_ref).map(Some)
}

fn prepare_texture_handle(
    context: &mut ComponentDecodeContext<'_>,
    field: &str,
    asset_ref: AssetRef,
) -> Result<PreparedTexture, ComponentCodecError> {
    let expected_source_kind = AssetSourceKind::Image;
    if let Some(result) =
        context.resolve_asset_ref_with_kind::<ImageAsset>(&asset_ref, &expected_source_kind)
    {
        return result
            .map(PreparedTexture::Resolved)
            .map_err(|error| invalid_asset_ref(field, &asset_ref, error));
    }

    if let Some(result) = context.validate_asset_ref_with_kind(&asset_ref, &expected_source_kind) {
        return match result {
            Ok(()) => Ok(PreparedTexture::Deferred(asset_ref)),
            Err(error) => Err(invalid_asset_ref(field, &asset_ref, error)),
        };
    }

    if let Some(stable_id) = asset_ref.as_stable_id() {
        let Some(database) = context.project_asset_database() else {
            return Err(invalid_asset_ref(
                field,
                &asset_ref,
                AssetRefError::MissingProjectDatabase(stable_id),
            ));
        };
        database.resolve_ref(&asset_ref).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(field, asset_ref.to_string(), error.to_string())
        })?;
    }

    Ok(PreparedTexture::Deferred(asset_ref))
}

fn resolve_prepared_texture(
    world: &mut World,
    texture: Option<PreparedTexture>,
) -> Result<Option<Handle<ImageAsset>>, ComponentCodecError> {
    match texture {
        None => Ok(None),
        Some(PreparedTexture::Resolved(handle)) => Ok(Some(handle)),
        Some(PreparedTexture::Deferred(texture_ref)) => {
            resolve_optional_texture(world, Some(&texture_ref), "material.image.value")
        }
    }
}

fn resolve_prepared_material(
    world: &mut World,
    material: PreparedSpriteMaterial,
) -> Result<SpriteMaterial, ComponentCodecError> {
    Ok(SpriteMaterial {
        image: resolve_prepared_texture(world, material.image)?,
        sampler: material.sampler,
        alpha_mode: material.alpha_mode,
        tint: material.tint,
    })
}

fn resolve_optional_texture(
    world: &mut World,
    texture_ref: Option<&AssetRef>,
    field: &str,
) -> Result<Option<Handle<ImageAsset>>, ComponentCodecError> {
    let Some(texture_ref) = texture_ref else {
        return Ok(None);
    };
    if world.get_resource::<AssetServer>().is_none() {
        world.insert_resource(AssetServer::new());
    }
    texture_ref
        .resolve::<ImageAsset>(&mut world.resource_mut::<AssetServer>())
        .map(Some)
        .map_err(|error| invalid_asset_ref(field, texture_ref, error))
}

fn invalid_asset_ref(
    field: &str,
    asset_ref: &AssetRef,
    error: AssetRefError,
) -> ComponentCodecError {
    ComponentCodecError::invalid_asset_ref(field, asset_ref.to_string(), error.to_string())
}

fn optional_i32(value: &ComponentValue, field: &str) -> Result<Option<i32>, ComponentCodecError> {
    value
        .get(field)
        .map(|value| {
            let value = value
                .as_i64()
                .ok_or_else(|| ComponentCodecError::invalid_field(field, "i32"))?;
            i32::try_from(value).map_err(|_| ComponentCodecError::invalid_field(field, "i32"))
        })
        .transpose()
}

fn read_vec2(value: &ComponentValue, field: &str) -> Result<Vec2, ComponentCodecError> {
    Ok(Vec2::new(
        read_f32(value.field("x")?, &format!("{field}.x"))?,
        read_f32(value.field("y")?, &format!("{field}.y"))?,
    ))
}

fn read_f32(value: &ComponentValue, field: &str) -> Result<f32, ComponentCodecError> {
    let value = value
        .as_f64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite f32"))?;
    if value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(ComponentCodecError::invalid_field(field, "finite f32"));
    }
    Ok(value as f32)
}

fn vec2_value(value: Vec2) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("x", ComponentValue::f64(f64::from(value.x))?),
        ("y", ComponentValue::f64(f64::from(value.y))?),
    ]))
}

fn read_optional_texture_region(
    value: Option<&ComponentValue>,
    field: &str,
) -> Result<Option<TextureRegion>, ComponentCodecError> {
    match value {
        None | Some(ComponentValue::Null) => Ok(None),
        Some(value) => {
            let region = TextureRegion::new(
                read_vec2(value.field("min")?, &format!("{field}.min"))?,
                read_vec2(value.field("size")?, &format!("{field}.size"))?,
            );
            if !region.is_valid_uv() {
                return Err(ComponentCodecError::invalid_field(
                    field,
                    "valid normalized uv region",
                ));
            }
            Ok(Some(region))
        }
    }
}

fn texture_region_value(region: TextureRegion) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("min", vec2_value(region.min)?),
        ("size", vec2_value(region.size)?),
    ]))
}

fn read_color(value: &ComponentValue, field: &str) -> Result<Color, ComponentCodecError> {
    Ok(Color::rgba(
        read_f32(value.field("r")?, &format!("{field}.r"))?,
        read_f32(value.field("g")?, &format!("{field}.g"))?,
        read_f32(value.field("b")?, &format!("{field}.b"))?,
        read_f32(value.field("a")?, &format!("{field}.a"))?,
    ))
}

fn color_value(value: Color) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("r", ComponentValue::f64(f64::from(value.r))?),
        ("g", ComponentValue::f64(f64::from(value.g))?),
        ("b", ComponentValue::f64(f64::from(value.b))?),
        ("a", ComponentValue::f64(f64::from(value.a))?),
    ]))
}

fn read_sampler(
    value: Option<&ComponentValue>,
    field: &str,
) -> Result<SamplerDescriptor, ComponentCodecError> {
    let Some(value) = value else {
        return Ok(SamplerDescriptor::default());
    };
    if matches!(value, ComponentValue::Null) {
        return Ok(SamplerDescriptor::default());
    }
    Ok(SamplerDescriptor {
        min_filter: read_filter_mode(
            value.get("min_filter"),
            &format!("{field}.min_filter"),
            FilterMode::Linear,
        )?,
        mag_filter: read_filter_mode(
            value.get("mag_filter"),
            &format!("{field}.mag_filter"),
            FilterMode::Linear,
        )?,
        mipmap_filter: read_filter_mode(
            value.get("mipmap_filter"),
            &format!("{field}.mipmap_filter"),
            FilterMode::Linear,
        )?,
        address_mode_u: read_address_mode(
            value.get("address_mode_u"),
            &format!("{field}.address_mode_u"),
            AddressMode::ClampToEdge,
        )?,
        address_mode_v: read_address_mode(
            value.get("address_mode_v"),
            &format!("{field}.address_mode_v"),
            AddressMode::ClampToEdge,
        )?,
    })
}

fn read_filter_mode(
    value: Option<&ComponentValue>,
    field: &str,
    default: FilterMode,
) -> Result<FilterMode, ComponentCodecError> {
    let Some(value) = value else {
        return Ok(default);
    };
    match value
        .as_str()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "filter mode string"))?
    {
        "nearest" => Ok(FilterMode::Nearest),
        "linear" => Ok(FilterMode::Linear),
        _ => Err(ComponentCodecError::invalid_field(
            field,
            "'nearest' or 'linear'",
        )),
    }
}

fn read_address_mode(
    value: Option<&ComponentValue>,
    field: &str,
    default: AddressMode,
) -> Result<AddressMode, ComponentCodecError> {
    let Some(value) = value else {
        return Ok(default);
    };
    match value
        .as_str()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "address mode string"))?
    {
        "clamp_to_edge" => Ok(AddressMode::ClampToEdge),
        "repeat" => Ok(AddressMode::Repeat),
        "mirror_repeat" => Ok(AddressMode::MirrorRepeat),
        _ => Err(ComponentCodecError::invalid_field(
            field,
            "'clamp_to_edge', 'repeat', or 'mirror_repeat'",
        )),
    }
}

fn read_alpha_mode(
    value: Option<&ComponentValue>,
    field: &str,
) -> Result<AlphaMode2d, ComponentCodecError> {
    let Some(value) = value else {
        return Ok(AlphaMode2d::Blend);
    };
    match value
        .as_str()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "alpha mode string"))?
    {
        "opaque" => Ok(AlphaMode2d::Opaque),
        "blend" => Ok(AlphaMode2d::Blend),
        _ => Err(ComponentCodecError::invalid_field(
            field,
            "'opaque' or 'blend'",
        )),
    }
}

fn material_value(
    image: ComponentValue,
    sampler: SamplerDescriptor,
    alpha_mode: AlphaMode2d,
    tint: Color,
) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("image", image),
        ("sampler", sampler_value(sampler)),
        (
            "alpha_mode",
            ComponentValue::String(alpha_mode_str(alpha_mode).to_string()),
        ),
        ("tint", color_value(tint)?),
    ]))
}

fn sampler_value(sampler: SamplerDescriptor) -> ComponentValue {
    ComponentValue::map([
        (
            "min_filter",
            ComponentValue::String(filter_mode_str(sampler.min_filter).to_string()),
        ),
        (
            "mag_filter",
            ComponentValue::String(filter_mode_str(sampler.mag_filter).to_string()),
        ),
        (
            "mipmap_filter",
            ComponentValue::String(filter_mode_str(sampler.mipmap_filter).to_string()),
        ),
        (
            "address_mode_u",
            ComponentValue::String(address_mode_str(sampler.address_mode_u).to_string()),
        ),
        (
            "address_mode_v",
            ComponentValue::String(address_mode_str(sampler.address_mode_v).to_string()),
        ),
    ])
}

fn filter_mode_str(filter_mode: FilterMode) -> &'static str {
    match filter_mode {
        FilterMode::Nearest => "nearest",
        FilterMode::Linear => "linear",
    }
}

fn address_mode_str(address_mode: AddressMode) -> &'static str {
    match address_mode {
        AddressMode::ClampToEdge => "clamp_to_edge",
        AddressMode::Repeat => "repeat",
        AddressMode::MirrorRepeat => "mirror_repeat",
    }
}

fn alpha_mode_str(alpha_mode: AlphaMode2d) -> &'static str {
    match alpha_mode {
        AlphaMode2d::Opaque => "opaque",
        AlphaMode2d::Blend => "blend",
    }
}

fn read_optional_asset_ref(
    value: Option<&ComponentValue>,
    field: &str,
) -> Result<Option<AssetRef>, ComponentCodecError> {
    match value {
        None | Some(ComponentValue::Null) => Ok(None),
        Some(value) => read_asset_ref(value, field).map(Some),
    }
}

fn read_asset_ref(value: &ComponentValue, field: &str) -> Result<AssetRef, ComponentCodecError> {
    match value.field_str("kind")? {
        "path" => AssetRef::path(value.field_str("value")?).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(
                format!("{field}.value"),
                value.field_str("value").unwrap_or_default(),
                error.to_string(),
            )
        }),
        "stable_id" => AssetRef::stable_id(value.field_str("value")?).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(
                format!("{field}.value"),
                value.field_str("value").unwrap_or_default(),
                error.to_string(),
            )
        }),
        _ => Err(ComponentCodecError::invalid_field(
            format!("{field}.kind"),
            "'path' or 'stable_id'",
        )),
    }
}

fn asset_ref_value(asset_ref: &AssetRef) -> Result<ComponentValue, ComponentCodecError> {
    match asset_ref {
        AssetRef::Path(path) => Ok(ComponentValue::map([
            ("kind", ComponentValue::String("path".to_string())),
            ("value", ComponentValue::String(path.as_str().to_string())),
        ])),
        AssetRef::StableId(id) => Ok(ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_string())),
            ("value", ComponentValue::String(id.to_string())),
        ])),
    }
}

pub mod prelude {
    pub use crate::{Sprite, SpriteAnchor, SpriteMaterial, SpritePlugin, TextureRegion};
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::{
        AssetId, AssetPath, AssetRecord, AssetSourceKind, ProjectAssetDatabase, StableAssetId,
    };
    use nara_reflect::ComponentDecodeContext;

    #[test]
    fn creates_color_sprite_with_default_authoring_state() {
        let sprite = Sprite::from_color(Vec2::new(16.0, 16.0), Color::WHITE);

        assert_eq!(sprite.material.image, None);
        assert_eq!(sprite.material.sampler, SamplerDescriptor::default());
        assert_eq!(sprite.material.alpha_mode, AlphaMode2d::Blend);
        assert_eq!(sprite.material.tint, Color::WHITE);
        assert_eq!(sprite.texture_region, None);
        assert_eq!(sprite.size, Vec2::new(16.0, 16.0));
        assert_eq!(sprite.anchor, SpriteAnchor::CENTER);
        assert_eq!(sprite.layer, 0);
        assert_eq!(sprite.sort_key, 0);
    }

    #[test]
    fn creates_texture_sprite_without_backend_handles() {
        let texture = Handle::new(AssetId::from_raw(7));
        let sprite = Sprite::from_texture(texture, Vec2::new(32.0, 32.0))
            .with_texture_region(TextureRegion::new(Vec2::ZERO, Vec2::splat(0.5)));

        assert_eq!(sprite.material.image, Some(texture));
        assert_eq!(
            sprite.texture_region,
            Some(TextureRegion::new(Vec2::ZERO, Vec2::splat(0.5)))
        );
        assert_eq!(sprite.material.tint, Color::WHITE);
    }

    #[test]
    fn converts_pixel_regions_to_normalized_uv_regions() {
        let region = TextureRegion::from_pixels(
            Vec2::new(16.0, 32.0),
            Vec2::new(16.0, 16.0),
            Vec2::new(64.0, 128.0),
        )
        .unwrap();

        assert_eq!(
            region,
            TextureRegion::new(Vec2::new(0.25, 0.25), Vec2::new(0.25, 0.125))
        );
        assert!(region.is_valid_uv());
    }

    #[test]
    fn records_layer_and_sort_key() {
        let sprite = Sprite::from_color(Vec2::new(8.0, 8.0), Color::WHITE)
            .with_layer(3)
            .with_sort_key(-4);

        assert_eq!(sprite.layer, 3);
        assert_eq!(sprite.sort_key, -4);
    }

    #[test]
    fn sprite_codec_resolves_stable_texture_refs_during_preflight() {
        let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
        let database = test_database(stable_id, "textures/player.png");
        let mut asset_server = AssetServer::new();
        let prepared = {
            let mut context = ComponentDecodeContext::with_asset_server(&mut asset_server)
                .with_project_asset_database(&database);
            let mut registry = ComponentRegistry::new();
            register_sprite_components(&mut registry)
                .expect("component registration should succeed");

            let prepared = registry
                .preflight_component_with_context(
                    &sprite_type_id(),
                    &sprite_value(AssetRef::StableId(stable_id)),
                    &mut context,
                )
                .unwrap()
                .unwrap();
            assert!(context.asset_server_touched());
            prepared
        };
        let mut world = World::new();
        let entity = world.spawn_empty().id();

        prepared.apply(&mut world, entity).unwrap();

        let sprite = world.get::<Sprite>(entity).unwrap();
        let texture = sprite.material.image.unwrap();
        assert_eq!(asset_server.path(texture.id()), Some("textures/player.png"));
        assert_eq!(asset_server.stable_id(texture.id()), Some(stable_id));
    }

    #[test]
    fn sprite_codec_rejects_unknown_stable_texture_refs_before_apply() {
        let known_stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
        let unknown_stable_id = stable_id("b73f0f16-09e8-4265-b090-b689b41c197e");
        let database = test_database(known_stable_id, "textures/player.png");
        let mut asset_server = AssetServer::new();
        let mut context = ComponentDecodeContext::with_asset_server(&mut asset_server)
            .with_project_asset_database(&database);
        let mut registry = ComponentRegistry::new();
        register_sprite_components(&mut registry).expect("component registration should succeed");

        let result = registry
            .preflight_component_with_context(
                &sprite_type_id(),
                &sprite_value(AssetRef::StableId(unknown_stable_id)),
                &mut context,
            )
            .unwrap();

        assert!(matches!(
            result,
            Err(ComponentCodecError::InvalidAssetRef {
                field,
                asset_ref,
                ..
            }) if field == "material.image.value"
                && asset_ref == format!("stable_id:{unknown_stable_id}")
        ));
        assert_eq!(asset_server.path(AssetId::from_raw(1)), None);
    }

    #[test]
    fn sprite_codec_rejects_wrong_texture_source_kind() {
        let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(AssetRecord::new(
                stable_id,
                AssetPath::new("scenes/player.scene.ron").unwrap(),
                AssetSourceKind::Scene,
            ))
            .unwrap();
        let mut asset_server = AssetServer::new();
        let mut context = ComponentDecodeContext::with_asset_server(&mut asset_server)
            .with_project_asset_database(&database);
        let mut registry = ComponentRegistry::new();
        register_sprite_components(&mut registry).expect("component registration should succeed");

        let result = registry
            .preflight_component_with_context(
                &sprite_type_id(),
                &sprite_value(AssetRef::StableId(stable_id)),
                &mut context,
            )
            .unwrap();

        assert!(matches!(
            result,
            Err(ComponentCodecError::InvalidAssetRef { field, .. }) if field == "material.image.value"
        ));
        assert_eq!(asset_server.path(AssetId::from_raw(1)), None);
    }

    #[test]
    fn sprite_codec_validates_path_refs_when_database_is_present() {
        let stable_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
        let database = test_database(stable_id, "textures/player.png");
        let mut context = ComponentDecodeContext::new().with_project_asset_database(&database);
        let mut registry = ComponentRegistry::new();
        register_sprite_components(&mut registry).expect("component registration should succeed");

        let result = registry
            .preflight_component_with_context(
                &sprite_type_id(),
                &sprite_value(AssetRef::path("textures/missing.png").unwrap()),
                &mut context,
            )
            .unwrap();

        assert!(matches!(
            result,
            Err(ComponentCodecError::InvalidAssetRef { field, .. }) if field == "material.image.value"
        ));
        assert!(!context.asset_server_touched());
    }

    #[test]
    fn sprite_schema_exposes_authoring_fields() {
        let mut registry = ComponentRegistry::new();
        register_sprite_components(&mut registry).expect("component registration should succeed");

        let schema = registry
            .schema(&ComponentTypeId::new("nara.sprite.Sprite"))
            .unwrap();

        assert_eq!(
            schema
                .fields
                .iter()
                .map(|field| (field.path.to_string(), field.value_kind, field.required))
                .collect::<Vec<_>>(),
            vec![
                ("layer".to_string(), ComponentValueKind::I64, false),
                (
                    "material.alpha_mode".to_string(),
                    ComponentValueKind::String,
                    false
                ),
                (
                    "material.image".to_string(),
                    ComponentValueKind::AssetRef,
                    false
                ),
                (
                    "material.sampler.address_mode_u".to_string(),
                    ComponentValueKind::String,
                    false
                ),
                (
                    "material.sampler.address_mode_v".to_string(),
                    ComponentValueKind::String,
                    false
                ),
                (
                    "material.sampler.mag_filter".to_string(),
                    ComponentValueKind::String,
                    false
                ),
                (
                    "material.sampler.min_filter".to_string(),
                    ComponentValueKind::String,
                    false
                ),
                (
                    "material.sampler.mipmap_filter".to_string(),
                    ComponentValueKind::String,
                    false
                ),
                ("material.tint.a".to_string(), ComponentValueKind::F64, true),
                ("material.tint.b".to_string(), ComponentValueKind::F64, true),
                ("material.tint.g".to_string(), ComponentValueKind::F64, true),
                ("material.tint.r".to_string(), ComponentValueKind::F64, true),
                ("size.x".to_string(), ComponentValueKind::F64, true),
                ("size.y".to_string(), ComponentValueKind::F64, true),
                ("sort_key".to_string(), ComponentValueKind::I64, false),
                ("texture_region".to_string(), ComponentValueKind::Map, false),
            ]
        );
    }

    fn sprite_value(image: AssetRef) -> ComponentValue {
        ComponentValue::map([
            ("size", vec2_value(Vec2::new(32.0, 32.0)).unwrap()),
            (
                "material",
                material_value(
                    asset_ref_value(&image).unwrap(),
                    SamplerDescriptor::NEAREST_CLAMP,
                    AlphaMode2d::Blend,
                    Color::WHITE,
                )
                .unwrap(),
            ),
            ("layer", ComponentValue::I64(0)),
            ("sort_key", ComponentValue::I64(0)),
        ])
    }

    fn sprite_type_id() -> ComponentTypeId {
        ComponentTypeId::new("nara.sprite.Sprite")
    }

    fn test_database(stable_id: StableAssetId, path: &str) -> ProjectAssetDatabase {
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(AssetRecord::new(
                stable_id,
                AssetPath::new(path).unwrap(),
                AssetSourceKind::Image,
            ))
            .unwrap();
        database
    }

    fn stable_id(id: &str) -> StableAssetId {
        StableAssetId::parse_str(id).unwrap()
    }
}
