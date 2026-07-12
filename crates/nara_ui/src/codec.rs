use nara_asset::{AssetRef, AssetRefError, AssetServer, AssetSourceKind, Handle};
use nara_core::Color;
use nara_image::ImageAsset;
use nara_material::{AddressMode, AlphaMode2d, FilterMode, SamplerDescriptor};
use nara_reflect::{
    ComponentApplyContext, ComponentCapability, ComponentCodecError, ComponentDecodeContext,
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentRegistry,
    ComponentRegistryError, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, PreparedComponent,
};
use nara_render::RenderTarget;

use crate::{UiNode, UiPanel, UiPanelMaterial, UiRoot, UiStyle, UiVal};

pub fn register_ui_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry.validate_component_registration::<UiRoot>(&ComponentTypeId::new("nara.ui.UiRoot"))?;
    registry.validate_component_registration::<UiNode>(&ComponentTypeId::new("nara.ui.UiNode"))?;
    registry
        .validate_component_registration::<UiPanel>(&ComponentTypeId::new("nara.ui.UiPanel"))?;
    register_ui_root_component(registry)?;
    register_ui_node_component(registry)?;
    register_ui_panel_component(registry)
}

pub(crate) fn register_ui_root_component(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    let root_id = ComponentTypeId::new("nara.ui.UiRoot");
    let schema = ComponentSchema::new(root_id, "UI root", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(ui_root_fields());
    registry.register_persistent_component_with_codec::<UiRoot, _, _>(
        schema,
        |value| {
            Ok(UiRoot {
                target: read_render_target(value.get("target"))?,
                order: optional_i32(value, "order")?.unwrap_or(0),
            })
        },
        |root| {
            Ok(ComponentValue::map([
                ("target", render_target_value(root.target)?),
                ("order", ComponentValue::I64(i64::from(root.order))),
            ]))
        },
    )?;
    Ok(())
}

pub(crate) fn register_ui_node_component(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    let node_id = ComponentTypeId::new("nara.ui.UiNode");
    let schema = ComponentSchema::new(node_id, "UI node", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(ui_node_fields());
    registry.register_persistent_component_with_codec::<UiNode, _, _>(
        schema,
        |value| {
            Ok(UiNode {
                style: read_style(value.get("style"))?,
                z_index: optional_i32(value, "z_index")?.unwrap_or(0),
                visible: optional_bool(value, "visible")?.unwrap_or(true),
                focusable: optional_bool(value, "focusable")?.unwrap_or(false),
                clip: optional_bool(value, "clip")?.unwrap_or(false),
            })
        },
        |node| {
            Ok(ComponentValue::map([
                ("style", style_value(node.style)?),
                ("z_index", ComponentValue::I64(i64::from(node.z_index))),
                ("visible", ComponentValue::Bool(node.visible)),
                ("focusable", ComponentValue::Bool(node.focusable)),
                ("clip", ComponentValue::Bool(node.clip)),
            ]))
        },
    )?;
    Ok(())
}

pub(crate) fn register_ui_panel_component(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    let panel_id = ComponentTypeId::new("nara.ui.UiPanel");
    let schema = ComponentSchema::new(panel_id, "UI panel", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(ui_panel_fields());
    registry.register_persistent_component_codec_with_context::<UiPanel, _, _>(
        schema,
        |value, context| {
            let material = read_panel_material(value.field("material")?, context)?;
            Ok(PreparedComponent::new(move |context| {
                let material = resolve_prepared_material(context, material)?;
                Ok(UiPanel { material })
            }))
        },
        |world, entity, context| {
            let Some(panel) = world.get::<UiPanel>(entity) else {
                return Ok(None);
            };
            let image = match panel.material.image {
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
            Ok(Some(ComponentValue::map([(
                "material",
                material_value(
                    image.unwrap_or(ComponentValue::Null),
                    panel.material.sampler,
                    panel.material.alpha_mode,
                    panel.material.tint,
                )?,
            )])))
        },
    )?;
    Ok(())
}

fn ui_root_fields() -> [ComponentFieldSchema; 2] {
    [
        ui_optional(
            "target",
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::String,
            ComponentValue::String("primary_window".to_string()),
        ),
        ui_optional(
            "order",
            "Order",
            ComponentFieldPath::from_fields(["order"]),
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        ),
    ]
}

fn ui_node_fields() -> [ComponentFieldSchema; 12] {
    [
        ui_optional(
            "clip",
            "Clip",
            ComponentFieldPath::from_fields(["clip"]),
            ComponentValueKind::Bool,
            ComponentValue::Bool(false),
        ),
        ui_optional(
            "focusable",
            "Focusable",
            ComponentFieldPath::from_fields(["focusable"]),
            ComponentValueKind::Bool,
            ComponentValue::Bool(false),
        ),
        ui_optional(
            "style.height.kind",
            "Height mode",
            ComponentFieldPath::from_fields(["style", "height", "kind"]),
            ComponentValueKind::String,
            ComponentValue::String("auto".to_string()),
        ),
        ui_optional(
            "style.height.value",
            "Height value",
            ComponentFieldPath::from_fields(["style", "height", "value"]),
            ComponentValueKind::F64,
            ComponentValue::f64(0.0).expect("0.0 is a valid ComponentValue f64"),
        ),
        ui_optional(
            "style.left.kind",
            "Left mode",
            ComponentFieldPath::from_fields(["style", "left", "kind"]),
            ComponentValueKind::String,
            ComponentValue::String("px".to_string()),
        ),
        ui_optional(
            "style.left.value",
            "Left value",
            ComponentFieldPath::from_fields(["style", "left", "value"]),
            ComponentValueKind::F64,
            ComponentValue::f64(0.0).expect("0.0 is a valid ComponentValue f64"),
        ),
        ui_optional(
            "style.top.kind",
            "Top mode",
            ComponentFieldPath::from_fields(["style", "top", "kind"]),
            ComponentValueKind::String,
            ComponentValue::String("px".to_string()),
        ),
        ui_optional(
            "style.top.value",
            "Top value",
            ComponentFieldPath::from_fields(["style", "top", "value"]),
            ComponentValueKind::F64,
            ComponentValue::f64(0.0).expect("0.0 is a valid ComponentValue f64"),
        ),
        ui_optional(
            "style.width.kind",
            "Width mode",
            ComponentFieldPath::from_fields(["style", "width", "kind"]),
            ComponentValueKind::String,
            ComponentValue::String("auto".to_string()),
        ),
        ui_optional(
            "style.width.value",
            "Width value",
            ComponentFieldPath::from_fields(["style", "width", "value"]),
            ComponentValueKind::F64,
            ComponentValue::f64(0.0).expect("0.0 is a valid ComponentValue f64"),
        ),
        ui_optional(
            "visible",
            "Visible",
            ComponentFieldPath::from_fields(["visible"]),
            ComponentValueKind::Bool,
            ComponentValue::Bool(true),
        ),
        ui_optional(
            "z_index",
            "Z index",
            ComponentFieldPath::from_fields(["z_index"]),
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        ),
    ]
}

fn ui_panel_fields() -> [ComponentFieldSchema; 11] {
    [
        ui_optional(
            "material.alpha_mode",
            "Alpha mode",
            ComponentFieldPath::from_fields(["material", "alpha_mode"]),
            ComponentValueKind::String,
            ComponentValue::String("blend".to_string()),
        ),
        ui_optional_asset_ref(
            "material.image",
            "Image",
            ComponentFieldPath::from_fields(["material", "image"]),
            ComponentValue::Null,
        ),
        ui_optional(
            "material.sampler.address_mode_u",
            "Horizontal address mode",
            ComponentFieldPath::from_fields(["material", "sampler", "address_mode_u"]),
            ComponentValueKind::String,
            ComponentValue::String("clamp_to_edge".to_string()),
        ),
        ui_optional(
            "material.sampler.address_mode_v",
            "Vertical address mode",
            ComponentFieldPath::from_fields(["material", "sampler", "address_mode_v"]),
            ComponentValueKind::String,
            ComponentValue::String("clamp_to_edge".to_string()),
        ),
        ui_optional(
            "material.sampler.mag_filter",
            "Magnification filter",
            ComponentFieldPath::from_fields(["material", "sampler", "mag_filter"]),
            ComponentValueKind::String,
            ComponentValue::String("linear".to_string()),
        ),
        ui_optional(
            "material.sampler.min_filter",
            "Minimum filter",
            ComponentFieldPath::from_fields(["material", "sampler", "min_filter"]),
            ComponentValueKind::String,
            ComponentValue::String("linear".to_string()),
        ),
        ui_optional(
            "material.sampler.mipmap_filter",
            "Mipmap filter",
            ComponentFieldPath::from_fields(["material", "sampler", "mipmap_filter"]),
            ComponentValueKind::String,
            ComponentValue::String("linear".to_string()),
        ),
        ui_required(
            "material.tint.a",
            "Tint alpha",
            ComponentFieldPath::from_fields(["material", "tint", "a"]),
            ComponentValueKind::F64,
        ),
        ui_required(
            "material.tint.b",
            "Tint blue",
            ComponentFieldPath::from_fields(["material", "tint", "b"]),
            ComponentValueKind::F64,
        ),
        ui_required(
            "material.tint.g",
            "Tint green",
            ComponentFieldPath::from_fields(["material", "tint", "g"]),
            ComponentValueKind::F64,
        ),
        ui_required(
            "material.tint.r",
            "Tint red",
            ComponentFieldPath::from_fields(["material", "tint", "r"]),
            ComponentValueKind::F64,
        ),
    ]
}

fn ui_required(
    id: &str,
    alias: &str,
    path: ComponentFieldPath,
    kind: ComponentValueKind,
) -> ComponentFieldSchema {
    ComponentFieldSchema::required(ComponentFieldId::new(id), alias, path, kind)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
}

fn ui_optional(
    id: &str,
    alias: &str,
    path: ComponentFieldPath,
    kind: ComponentValueKind,
    default_value: ComponentValue,
) -> ComponentFieldSchema {
    ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new(id),
        alias,
        path,
        kind,
        default_value,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
}

fn ui_optional_asset_ref(
    id: &str,
    alias: &str,
    path: ComponentFieldPath,
    default_value: ComponentValue,
) -> ComponentFieldSchema {
    ui_optional(id, alias, path, ComponentValueKind::AssetRef, default_value)
        .with_capability(ComponentCapability::AssetRef)
}

fn read_render_target(value: Option<&ComponentValue>) -> Result<RenderTarget, ComponentCodecError> {
    match value.and_then(ComponentValue::as_str) {
        None | Some("primary_window") => Ok(RenderTarget::PrimaryWindow),
        Some(_) => Err(ComponentCodecError::invalid_field(
            "target",
            "'primary_window'",
        )),
    }
}

fn render_target_value(target: RenderTarget) -> Result<ComponentValue, ComponentCodecError> {
    match target {
        RenderTarget::PrimaryWindow => Ok(ComponentValue::String("primary_window".to_string())),
        RenderTarget::Window(_) | RenderTarget::Image(_) => Err(ComponentCodecError::Message(
            "only primary window UI roots are scene-capable in this slice".to_string(),
        )),
    }
}

fn read_style(value: Option<&ComponentValue>) -> Result<UiStyle, ComponentCodecError> {
    let Some(value) = value else {
        return Ok(UiStyle::default());
    };
    Ok(UiStyle {
        left: read_ui_val(value.get("left"), "style.left", UiVal::Px(0.0))?,
        top: read_ui_val(value.get("top"), "style.top", UiVal::Px(0.0))?,
        width: read_ui_val(value.get("width"), "style.width", UiVal::Auto)?,
        height: read_ui_val(value.get("height"), "style.height", UiVal::Auto)?,
    })
}

fn read_ui_val(
    value: Option<&ComponentValue>,
    field: &str,
    default: UiVal,
) -> Result<UiVal, ComponentCodecError> {
    let Some(value) = value else {
        return Ok(default);
    };
    match value
        .get("kind")
        .and_then(ComponentValue::as_str)
        .unwrap_or(match default {
            UiVal::Px(_) => "px",
            UiVal::Percent(_) => "percent",
            UiVal::Auto => "auto",
        }) {
        "px" => Ok(UiVal::Px(
            optional_f32(value, "value", &format!("{field}.value"))?.unwrap_or(0.0),
        )),
        "percent" => Ok(UiVal::Percent(
            optional_f32(value, "value", &format!("{field}.value"))?.unwrap_or(0.0),
        )),
        "auto" => Ok(UiVal::Auto),
        _ => Err(ComponentCodecError::invalid_field(
            format!("{field}.kind"),
            "'px', 'percent', or 'auto'",
        )),
    }
}

fn style_value(style: UiStyle) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("left", ui_val_value(style.left)?),
        ("top", ui_val_value(style.top)?),
        ("width", ui_val_value(style.width)?),
        ("height", ui_val_value(style.height)?),
    ]))
}

fn ui_val_value(value: UiVal) -> Result<ComponentValue, ComponentCodecError> {
    let (kind, numeric) = match value {
        UiVal::Px(value) => ("px", Some(value)),
        UiVal::Percent(value) => ("percent", Some(value)),
        UiVal::Auto => ("auto", None),
    };
    Ok(ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_string())),
        (
            "value",
            numeric
                .map(|value| ComponentValue::f64(f64::from(value)))
                .transpose()?
                .unwrap_or(ComponentValue::Null),
        ),
    ]))
}

#[derive(Debug, Clone)]
enum PreparedImage {
    Resolved(Handle<ImageAsset>),
    Deferred(AssetRef),
}

#[derive(Debug, Clone)]
struct PreparedPanelMaterial {
    image: Option<PreparedImage>,
    sampler: SamplerDescriptor,
    alpha_mode: AlphaMode2d,
    tint: Color,
}

fn read_panel_material(
    value: &ComponentValue,
    context: &mut ComponentDecodeContext<'_>,
) -> Result<PreparedPanelMaterial, ComponentCodecError> {
    let tint = read_color(value.field("tint")?, "material.tint")?;
    let image_ref = read_optional_asset_ref(value.get("image"), "material.image")?;
    let image = prepare_optional_image(context, image_ref, "material.image.value")?;
    let sampler = read_sampler(value.get("sampler"), "material.sampler")?;
    let alpha_mode = read_alpha_mode(value.get("alpha_mode"), "material.alpha_mode")?;

    Ok(PreparedPanelMaterial {
        image,
        sampler,
        alpha_mode,
        tint,
    })
}

fn prepare_optional_image(
    context: &mut ComponentDecodeContext<'_>,
    image_ref: Option<AssetRef>,
    field: &str,
) -> Result<Option<PreparedImage>, ComponentCodecError> {
    let Some(image_ref) = image_ref else {
        return Ok(None);
    };
    prepare_image_handle(context, field, image_ref).map(Some)
}

fn prepare_image_handle(
    context: &mut ComponentDecodeContext<'_>,
    field: &str,
    asset_ref: AssetRef,
) -> Result<PreparedImage, ComponentCodecError> {
    let expected_source_kind = AssetSourceKind::Image;
    if let Some(result) =
        context.resolve_asset_ref_with_kind::<ImageAsset>(&asset_ref, &expected_source_kind)
    {
        return result
            .map(PreparedImage::Resolved)
            .map_err(|error| invalid_asset_ref(field, &asset_ref, error));
    }

    if let Some(result) = context.validate_asset_ref_with_kind(&asset_ref, &expected_source_kind) {
        return match result {
            Ok(()) => Ok(PreparedImage::Deferred(asset_ref)),
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

    Ok(PreparedImage::Deferred(asset_ref))
}

fn resolve_prepared_material(
    context: &mut ComponentApplyContext,
    material: PreparedPanelMaterial,
) -> Result<UiPanelMaterial, ComponentCodecError> {
    Ok(UiPanelMaterial {
        image: resolve_prepared_image(context, material.image)?,
        sampler: material.sampler,
        alpha_mode: material.alpha_mode,
        tint: material.tint,
    })
}

fn resolve_prepared_image(
    context: &mut ComponentApplyContext,
    image: Option<PreparedImage>,
) -> Result<Option<Handle<ImageAsset>>, ComponentCodecError> {
    match image {
        None => Ok(None),
        Some(PreparedImage::Resolved(handle)) => Ok(Some(handle)),
        Some(PreparedImage::Deferred(image_ref)) => {
            resolve_optional_image(context, Some(&image_ref), "material.image.value")
        }
    }
}

fn resolve_optional_image(
    context: &mut ComponentApplyContext,
    image_ref: Option<&AssetRef>,
    field: &str,
) -> Result<Option<Handle<ImageAsset>>, ComponentCodecError> {
    let Some(image_ref) = image_ref else {
        return Ok(None);
    };
    context
        .resolve_asset_ref::<ImageAsset>(image_ref)
        .map(Some)
        .map_err(|error| invalid_asset_ref(field, image_ref, error))
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

fn optional_bool(value: &ComponentValue, field: &str) -> Result<Option<bool>, ComponentCodecError> {
    value
        .get(field)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| ComponentCodecError::invalid_field(field, "bool"))
        })
        .transpose()
}

fn optional_f32(
    value: &ComponentValue,
    field: &str,
    display_field: &str,
) -> Result<Option<f32>, ComponentCodecError> {
    value
        .get(field)
        .map(|value| read_f32(value, display_field))
        .transpose()
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
