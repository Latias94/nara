use nara_asset::AssetRef;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};
use nara_scene::PrefabDocument;
use nara_ui_kernel::{
    UiActionId, UiAlign, UiBorder, UiBrush, UiColor, UiColorRole, UiCornerRadii, UiDimension,
    UiEdges, UiFlexDirection, UiImageId, UiJustify, UiLayer, UiOverflow, UiPositionMode,
    UiSemanticRole, UiStyle, UiTextAlignment, UiTextOverflow, UiThemePalette,
};

use crate::{UiButton, UiButtonVisuals, UiNode, UiRoot, UiSemantics, UiText, UiTheme, UiVisual};

const UI_ROOT_ID: &str = "nara.ui.UiRoot";
const UI_NODE_ID: &str = "nara.ui.UiNode";
const UI_VISUAL_ID: &str = "nara.ui.UiVisual";
const UI_TEXT_ID: &str = "nara.ui.UiText";
const UI_BUTTON_ID: &str = "nara.ui.UiButton";
const UI_BUTTON_VISUALS_ID: &str = "nara.ui.UiButtonVisuals";
const UI_SEMANTICS_ID: &str = "nara.ui.UiSemantics";
const UI_THEME_ID: &str = "nara.ui.UiTheme";

const DIMENSION_FIELDS: &[&str] = &["kind", "value"];
const LEGACY_BRUSH_FIELDS: &[&str] = &["kind", "red", "green", "blue", "alpha", "reference"];
const BRUSH_FIELDS: &[&str] = &["kind", "red", "green", "blue", "alpha", "role", "image"];
const COLOR_FIELDS: &[&str] = &["red", "green", "blue", "alpha"];

const UI_SCHEMA_VERSION_TWO: ComponentSchemaVersion = ComponentSchemaVersion(2);

pub fn register_ui_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    validate_ui_components(registry)?;
    registry.register_persistent_component_with_codec::<UiRoot, _, _>(
        ui_root_schema(),
        decode_root,
        encode_root,
    )?;
    registry.register_persistent_component_with_codec::<UiNode, _, _>(
        ui_node_schema(),
        decode_node,
        encode_node,
    )?;
    registry.register_persistent_component_with_codec::<UiVisual, _, _>(
        ui_visual_schema(),
        decode_visual,
        encode_visual,
    )?;
    registry.register_persistent_component_with_codec::<UiText, _, _>(
        ui_text_schema(),
        decode_text,
        encode_text,
    )?;
    registry.register_persistent_component_with_codec::<UiButton, _, _>(
        ui_button_schema(),
        decode_button,
        encode_button,
    )?;
    registry.register_persistent_component_with_codec::<UiButtonVisuals, _, _>(
        ui_button_visuals_schema(),
        decode_button_visuals,
        encode_button_visuals,
    )?;
    registry.register_persistent_component_with_codec::<UiSemantics, _, _>(
        ui_semantics_schema(),
        decode_semantics,
        encode_semantics,
    )?;
    registry.register_persistent_component_with_codec::<UiTheme, _, _>(
        ui_theme_schema(),
        decode_theme,
        encode_theme,
    )?;
    registry.register_component_migration(
        &ComponentTypeId::new(UI_VISUAL_ID),
        ComponentSchemaVersion::ONE,
        UI_SCHEMA_VERSION_TWO,
        migrate_legacy_visual,
    )?;
    registry.register_component_migration(
        &ComponentTypeId::new(UI_TEXT_ID),
        ComponentSchemaVersion::ONE,
        UI_SCHEMA_VERSION_TWO,
        migrate_legacy_text,
    )?;
    Ok(())
}

pub(crate) fn validate_ui_components(
    registry: &ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry.validate_component_registration::<UiRoot>(&ComponentTypeId::new(UI_ROOT_ID))?;
    registry.validate_component_registration::<UiNode>(&ComponentTypeId::new(UI_NODE_ID))?;
    registry.validate_component_registration::<UiVisual>(&ComponentTypeId::new(UI_VISUAL_ID))?;
    registry.validate_component_registration::<UiText>(&ComponentTypeId::new(UI_TEXT_ID))?;
    registry.validate_component_registration::<UiButton>(&ComponentTypeId::new(UI_BUTTON_ID))?;
    registry.validate_component_registration::<UiButtonVisuals>(&ComponentTypeId::new(
        UI_BUTTON_VISUALS_ID,
    ))?;
    registry
        .validate_component_registration::<UiSemantics>(&ComponentTypeId::new(UI_SEMANTICS_ID))?;
    registry.validate_component_registration::<UiTheme>(&ComponentTypeId::new(UI_THEME_ID))?;
    Ok(())
}

pub fn prefab_contains_ui_root(document: &PrefabDocument) -> bool {
    let root = ComponentTypeId::new(UI_ROOT_ID);
    document
        .entities
        .iter()
        .any(|entity| entity.components.contains_key(&root))
}

pub(crate) fn ui_schema_predecessor_catalog() -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: vec![
            ui_root_schema(),
            ui_node_schema(),
            legacy_ui_visual_schema(),
            legacy_ui_text_schema(),
            ui_button_schema(),
            ui_button_visuals_schema(),
            ui_semantics_schema(),
            ui_theme_schema(),
        ],
        ..ComponentSchemaCatalog::default()
    }
}

pub(crate) fn ui_schema_catalog() -> ComponentSchemaCatalog {
    let predecessor = ui_schema_predecessor_catalog();
    let mut current = ComponentSchemaCatalog::successor_of(&predecessor)
        .expect("the fixed generation-one UI catalog always has a successor");
    current.components = vec![
        ui_root_schema(),
        ui_node_schema(),
        ui_visual_schema(),
        ui_text_schema(),
        ui_button_schema(),
        ui_button_visuals_schema(),
        ui_semantics_schema(),
        ui_theme_schema(),
    ];
    current
}

pub(crate) fn validate_ui_component_value(
    id: &ComponentTypeId,
    value: &ComponentValue,
) -> Result<(), ComponentCodecError> {
    match id.as_str() {
        UI_ROOT_ID => decode_root(value).map(drop),
        UI_NODE_ID => decode_node(value).map(drop),
        UI_VISUAL_ID => decode_visual(value).map(drop),
        UI_TEXT_ID => decode_text(value).map(drop),
        UI_BUTTON_ID => decode_button(value).map(drop),
        UI_BUTTON_VISUALS_ID => decode_button_visuals(value).map(drop),
        UI_SEMANTICS_ID => decode_semantics(value).map(drop),
        UI_THEME_ID => decode_theme(value).map(drop),
        _ => Err(ComponentCodecError::invalid_field(
            "component",
            "a first-party persistent UI component",
        )),
    }
}

pub(crate) fn ui_component_schema_version(id: &ComponentTypeId) -> Option<ComponentSchemaVersion> {
    match id.as_str() {
        UI_VISUAL_ID | UI_TEXT_ID => Some(UI_SCHEMA_VERSION_TWO),
        UI_ROOT_ID | UI_NODE_ID | UI_BUTTON_ID | UI_BUTTON_VISUALS_ID | UI_SEMANTICS_ID
        | UI_THEME_ID => Some(ComponentSchemaVersion::ONE),
        _ => None,
    }
}

pub(crate) fn decode_root(value: &ComponentValue) -> Result<UiRoot, ComponentCodecError> {
    expect_fields(value, "UiRoot", &["layer"])?;
    Ok(UiRoot::new(read_layer(value.field("layer")?, "layer")?))
}

pub(crate) fn encode_root(root: &UiRoot) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([(
        "layer",
        ComponentValue::String(layer_name(root.layer()).to_string()),
    )]))
}

pub(crate) fn decode_node(value: &ComponentValue) -> Result<UiNode, ComponentCodecError> {
    expect_fields(value, "UiNode", &["style"])?;
    Ok(UiNode::new(read_style(value.field("style")?)?))
}

pub(crate) fn encode_node(node: &UiNode) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([("style", style_value(node.style())?)]))
}

pub(crate) fn decode_visual(value: &ComponentValue) -> Result<UiVisual, ComponentCodecError> {
    expect_fields(
        value,
        "UiVisual",
        &["background", "border_width", "border_brush", "radii"],
    )?;
    let background = read_brush(value.field("background")?, "background")?;
    let border_width = read_non_negative(value.field("border_width")?, "border_width")?;
    let border_brush = read_brush(value.field("border_brush")?, "border_brush")?;
    let mut visual = match background {
        UiBrush::Solid(color) => UiVisual::solid(color),
        UiBrush::Role(role) => UiVisual::role(role),
        UiBrush::Image { image, tint } => UiVisual::image(image, tint),
    }
    .with_radii(read_radii(value.field("radii")?)?);
    if border_width > 0.0 {
        visual = visual.with_border(
            UiBorder::new(border_width, border_brush)
                .map_err(|error| codec_message(error.to_string()))?,
        );
    }
    Ok(visual)
}

pub(crate) fn encode_visual(visual: &UiVisual) -> Result<ComponentValue, ComponentCodecError> {
    let visual = visual.kernel();
    let background = visual
        .background()
        .ok_or_else(|| ComponentCodecError::invalid_field("background", "an explicit UI brush"))?;
    let (border_width, border_brush) = visual.border().map_or_else(
        || (0.0, UiBrush::Solid(UiColor::rgba(0.0, 0.0, 0.0, 0.0))),
        |border| (border.width(), border.brush().clone()),
    );
    Ok(ComponentValue::map([
        ("background", brush_value(background)?),
        ("border_width", float_value(border_width)?),
        ("border_brush", brush_value(&border_brush)?),
        ("radii", radii_value(visual.radii())?),
    ]))
}

pub(crate) fn decode_text(value: &ComponentValue) -> Result<UiText, ComponentCodecError> {
    expect_fields(
        value,
        "UiText",
        &[
            "content",
            "logical_size",
            "brush",
            "alignment",
            "overflow",
            "has_semantic_text",
            "semantic_text",
        ],
    )?;
    let mut text = UiText::new(
        read_string(value.field("content")?, "content")?,
        read_positive(value.field("logical_size")?, "logical_size")?,
    )
    .map_err(|error| codec_message(error.to_string()))?
    .with_brush(read_brush(value.field("brush")?, "brush")?)
    .with_alignment(read_text_alignment(value.field("alignment")?, "alignment")?)
    .with_overflow(read_text_overflow(value.field("overflow")?, "overflow")?);
    let has_semantic_text = read_bool(value.field("has_semantic_text")?, "has_semantic_text")?;
    let semantic_text = read_string(value.field("semantic_text")?, "semantic_text")?;
    if has_semantic_text {
        text = text.with_semantic_text(semantic_text);
    } else if !semantic_text.is_empty() {
        return Err(ComponentCodecError::invalid_field(
            "semantic_text",
            "an empty string when has_semantic_text is false",
        ));
    }
    Ok(text)
}

pub(crate) fn encode_text(text: &UiText) -> Result<ComponentValue, ComponentCodecError> {
    let text = text.kernel();
    Ok(ComponentValue::map([
        (
            "content",
            ComponentValue::String(text.content().to_string()),
        ),
        ("logical_size", float_value(text.logical_size())?),
        ("brush", brush_value(text.brush())?),
        (
            "alignment",
            ComponentValue::String(text_alignment_name(text.alignment()).to_string()),
        ),
        (
            "overflow",
            ComponentValue::String(text_overflow_name(text.overflow()).to_string()),
        ),
        (
            "has_semantic_text",
            ComponentValue::Bool(text.semantic_text().is_some()),
        ),
        (
            "semantic_text",
            ComponentValue::String(text.semantic_text().unwrap_or_default().to_string()),
        ),
    ]))
}

pub(crate) fn decode_button(value: &ComponentValue) -> Result<UiButton, ComponentCodecError> {
    expect_fields(
        value,
        "UiButton",
        &["label", "action", "enabled", "logical_text_size"],
    )?;
    let action = UiActionId::new(read_string(value.field("action")?, "action")?)
        .map_err(|error| codec_message(error.to_string()))?;
    let button = UiButton::new(
        read_string(value.field("label")?, "label")?,
        action,
        read_positive(value.field("logical_text_size")?, "logical_text_size")?,
    )
    .map_err(|error| codec_message(error.to_string()))?;
    Ok(button.with_enabled(read_bool(value.field("enabled")?, "enabled")?))
}

pub(crate) fn encode_button(button: &UiButton) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("label", ComponentValue::String(button.label().to_string())),
        (
            "action",
            ComponentValue::String(button.action().as_str().to_string()),
        ),
        ("enabled", ComponentValue::Bool(button.enabled())),
        (
            "logical_text_size",
            float_value(button.logical_text_size())?,
        ),
    ]))
}

pub(crate) fn decode_button_visuals(
    value: &ComponentValue,
) -> Result<UiButtonVisuals, ComponentCodecError> {
    expect_fields(
        value,
        "UiButtonVisuals",
        &[
            "normal_text",
            "hovered_background",
            "hovered_text",
            "pressed_background",
            "pressed_text",
            "focused_border",
            "focused_text",
            "focus_border_width",
            "disabled_background",
            "disabled_text",
        ],
    )?;
    let disabled_background =
        read_color_role(value.field("disabled_background")?, "disabled_background")?;
    let disabled_text = read_color_role(value.field("disabled_text")?, "disabled_text")?;
    let visuals = UiButtonVisuals::new()
        .with_normal_text(read_color_role(value.field("normal_text")?, "normal_text")?)
        .with_hovered(
            read_color_role(value.field("hovered_background")?, "hovered_background")?,
            read_color_role(value.field("hovered_text")?, "hovered_text")?,
        )
        .with_pressed(
            read_color_role(value.field("pressed_background")?, "pressed_background")?,
            read_color_role(value.field("pressed_text")?, "pressed_text")?,
        )
        .with_focused(
            read_color_role(value.field("focused_border")?, "focused_border")?,
            read_color_role(value.field("focused_text")?, "focused_text")?,
            read_positive(value.field("focus_border_width")?, "focus_border_width")?,
        )
        .map_err(|error| codec_message(error.to_string()))?;
    Ok(visuals.with_disabled(disabled_background, disabled_text))
}

pub(crate) fn encode_button_visuals(
    visuals: &UiButtonVisuals,
) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        (
            "normal_text",
            ComponentValue::String(color_role_name(visuals.normal_text()).to_string()),
        ),
        (
            "hovered_background",
            ComponentValue::String(color_role_name(visuals.hovered_background()).to_string()),
        ),
        (
            "hovered_text",
            ComponentValue::String(color_role_name(visuals.hovered_text()).to_string()),
        ),
        (
            "pressed_background",
            ComponentValue::String(color_role_name(visuals.pressed_background()).to_string()),
        ),
        (
            "pressed_text",
            ComponentValue::String(color_role_name(visuals.pressed_text()).to_string()),
        ),
        (
            "focused_border",
            ComponentValue::String(color_role_name(visuals.focused_border()).to_string()),
        ),
        (
            "focused_text",
            ComponentValue::String(color_role_name(visuals.focused_text()).to_string()),
        ),
        (
            "focus_border_width",
            float_value(visuals.focus_border_width())?,
        ),
        (
            "disabled_background",
            ComponentValue::String(color_role_name(visuals.disabled_background()).to_string()),
        ),
        (
            "disabled_text",
            ComponentValue::String(color_role_name(visuals.disabled_text()).to_string()),
        ),
    ]))
}

pub(crate) fn decode_semantics(value: &ComponentValue) -> Result<UiSemantics, ComponentCodecError> {
    expect_fields(
        value,
        "UiSemantics",
        &[
            "role",
            "name",
            "has_value",
            "value",
            "has_selected",
            "selected",
            "has_expanded",
            "expanded",
            "actions",
        ],
    )?;
    let mut semantics = UiSemantics::new(
        read_semantic_role(value.field("role")?, "role")?,
        read_string(value.field("name")?, "name")?,
    )
    .map_err(|error| codec_message(error.to_string()))?;
    let has_value = read_bool(value.field("has_value")?, "has_value")?;
    let semantic_value = read_string(value.field("value")?, "value")?;
    if has_value {
        semantics = semantics
            .with_value(semantic_value)
            .map_err(|error| codec_message(error.to_string()))?;
    } else if !semantic_value.is_empty() {
        return Err(ComponentCodecError::invalid_field(
            "value",
            "an empty string when has_value is false",
        ));
    }
    if read_bool(value.field("has_selected")?, "has_selected")? {
        semantics = semantics.with_selected(read_bool(value.field("selected")?, "selected")?);
    }
    if read_bool(value.field("has_expanded")?, "has_expanded")? {
        semantics = semantics.with_expanded(read_bool(value.field("expanded")?, "expanded")?);
    }
    let ComponentValue::List(actions) = value.field("actions")? else {
        return Err(ComponentCodecError::invalid_field(
            "actions",
            "a string list",
        ));
    };
    for action in actions {
        let action = UiActionId::new(read_string(action, "actions[]")?)
            .map_err(|error| codec_message(error.to_string()))?;
        semantics = semantics
            .with_action(action)
            .map_err(|error| codec_message(error.to_string()))?;
    }
    Ok(semantics)
}

fn encode_semantics(semantics: &UiSemantics) -> Result<ComponentValue, ComponentCodecError> {
    let semantics = semantics.kernel();
    Ok(ComponentValue::map([
        (
            "role",
            ComponentValue::String(semantic_role_name(semantics.role()).to_string()),
        ),
        ("name", ComponentValue::String(semantics.name().to_string())),
        (
            "has_value",
            ComponentValue::Bool(semantics.value().is_some()),
        ),
        (
            "value",
            ComponentValue::String(semantics.value().unwrap_or_default().to_string()),
        ),
        (
            "has_selected",
            ComponentValue::Bool(semantics.selected().is_some()),
        ),
        (
            "selected",
            ComponentValue::Bool(semantics.selected().unwrap_or(false)),
        ),
        (
            "has_expanded",
            ComponentValue::Bool(semantics.expanded().is_some()),
        ),
        (
            "expanded",
            ComponentValue::Bool(semantics.expanded().unwrap_or(false)),
        ),
        (
            "actions",
            ComponentValue::List(
                semantics
                    .actions()
                    .iter()
                    .map(|action| ComponentValue::String(action.as_str().to_string()))
                    .collect(),
            ),
        ),
    ]))
}

pub(crate) fn decode_theme(value: &ComponentValue) -> Result<UiTheme, ComponentCodecError> {
    expect_fields(value, "UiTheme", &["revision", "palette"])?;
    UiTheme::new(
        read_u64(value.field("revision")?, "revision")?,
        read_palette(value.field("palette")?)?,
    )
    .map_err(|error| codec_message(error.to_string()))
}

fn encode_theme(theme: &UiTheme) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("revision", ComponentValue::U64(theme.revision().get())),
        ("palette", palette_value(theme.palette())?),
    ]))
}

fn read_style(value: &ComponentValue) -> Result<UiStyle, ComponentCodecError> {
    expect_fields(
        value,
        "style",
        &[
            "position_mode",
            "width",
            "height",
            "left",
            "top",
            "flex_direction",
            "align_items",
            "justify_content",
            "gap",
            "padding",
            "flex_grow",
            "flex_shrink",
            "z_index",
            "visible",
            "overflow",
            "stacking_context",
        ],
    )?;
    let padding = value.field("padding")?;
    expect_fields(
        padding,
        "style.padding",
        &["left", "right", "top", "bottom"],
    )?;
    Ok(UiStyle::flex()
        .with_position_mode(read_position_mode(
            value.field("position_mode")?,
            "style.position_mode",
        )?)
        .with_width(read_dimension(value.field("width")?, "style.width")?)
        .with_height(read_dimension(value.field("height")?, "style.height")?)
        .with_left(read_dimension(value.field("left")?, "style.left")?)
        .with_top(read_dimension(value.field("top")?, "style.top")?)
        .with_flex_direction(read_flex_direction(
            value.field("flex_direction")?,
            "style.flex_direction",
        )?)
        .with_align_items(read_align(
            value.field("align_items")?,
            "style.align_items",
        )?)
        .with_justify_content(read_justify(
            value.field("justify_content")?,
            "style.justify_content",
        )?)
        .with_gap(read_non_negative(value.field("gap")?, "style.gap")?)
        .with_padding(
            UiEdges::new(
                read_non_negative(padding.field("left")?, "style.padding.left")?,
                read_non_negative(padding.field("right")?, "style.padding.right")?,
                read_non_negative(padding.field("top")?, "style.padding.top")?,
                read_non_negative(padding.field("bottom")?, "style.padding.bottom")?,
            )
            .map_err(|error| codec_message(error.to_string()))?,
        )
        .with_flex_grow(read_non_negative(
            value.field("flex_grow")?,
            "style.flex_grow",
        )?)
        .with_flex_shrink(read_non_negative(
            value.field("flex_shrink")?,
            "style.flex_shrink",
        )?)
        .with_z_index(read_i32(value.field("z_index")?, "style.z_index")?)
        .with_visible(read_bool(value.field("visible")?, "style.visible")?)
        .with_overflow(read_overflow(value.field("overflow")?, "style.overflow")?)
        .with_stacking_context(read_bool(
            value.field("stacking_context")?,
            "style.stacking_context",
        )?))
}

fn style_value(style: UiStyle) -> Result<ComponentValue, ComponentCodecError> {
    let padding = style.padding();
    Ok(ComponentValue::map([
        (
            "position_mode",
            ComponentValue::String(position_mode_name(style.position_mode()).to_string()),
        ),
        ("width", dimension_value(style.width())?),
        ("height", dimension_value(style.height())?),
        ("left", dimension_value(style.left())?),
        ("top", dimension_value(style.top())?),
        (
            "flex_direction",
            ComponentValue::String(flex_direction_name(style.flex_direction()).to_string()),
        ),
        (
            "align_items",
            ComponentValue::String(align_name(style.align_items()).to_string()),
        ),
        (
            "justify_content",
            ComponentValue::String(justify_name(style.justify_content()).to_string()),
        ),
        ("gap", float_value(style.gap())?),
        (
            "padding",
            ComponentValue::map([
                ("left", float_value(padding.left())?),
                ("right", float_value(padding.right())?),
                ("top", float_value(padding.top())?),
                ("bottom", float_value(padding.bottom())?),
            ]),
        ),
        ("flex_grow", float_value(style.flex_grow())?),
        ("flex_shrink", float_value(style.flex_shrink())?),
        ("z_index", ComponentValue::I64(i64::from(style.z_index()))),
        ("visible", ComponentValue::Bool(style.visible())),
        (
            "overflow",
            ComponentValue::String(overflow_name(style.overflow()).to_string()),
        ),
        (
            "stacking_context",
            ComponentValue::Bool(style.stacking_context()),
        ),
    ]))
}

fn read_dimension(value: &ComponentValue, field: &str) -> Result<UiDimension, ComponentCodecError> {
    expect_fields(value, field, DIMENSION_FIELDS)?;
    let kind = read_string(value.field("kind")?, &format!("{field}.kind"))?;
    let number = read_non_negative(value.field("value")?, &format!("{field}.value"))?;
    match kind.as_str() {
        "auto" if number == 0.0 => Ok(UiDimension::auto()),
        "px" => UiDimension::px(number).map_err(|error| codec_message(error.to_string())),
        "percent" => UiDimension::percent(number).map_err(|error| codec_message(error.to_string())),
        "auto" => Err(ComponentCodecError::invalid_field(
            format!("{field}.value"),
            "0.0 for an auto dimension",
        )),
        _ => Err(ComponentCodecError::invalid_field(
            format!("{field}.kind"),
            "auto, px, or percent",
        )),
    }
}

fn dimension_value(value: UiDimension) -> Result<ComponentValue, ComponentCodecError> {
    let (kind, value) = match value {
        UiDimension::Auto => ("auto", 0.0),
        UiDimension::Px(value) => ("px", value),
        UiDimension::Percent(value) => ("percent", value),
    };
    Ok(ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_string())),
        ("value", float_value(value)?),
    ]))
}

fn read_brush(value: &ComponentValue, field: &str) -> Result<UiBrush, ComponentCodecError> {
    expect_fields(value, field, BRUSH_FIELDS)?;
    let kind = read_string(value.field("kind")?, &format!("{field}.kind"))?;
    let color = read_flat_color(value, field)?;
    let role = read_string(value.field("role")?, &format!("{field}.role"))?;
    let image = read_optional_asset_ref(value.field("image").ok(), &format!("{field}.image"))?;
    match kind.as_str() {
        "solid" if role.is_empty() && image.is_none() => Ok(UiBrush::Solid(color)),
        "role" if image.is_none() => Ok(UiBrush::Role(read_color_role_name(
            &role,
            &format!("{field}.role"),
        )?)),
        "image" if role.is_empty() => {
            let asset_ref = image.ok_or_else(|| {
                ComponentCodecError::invalid_field(
                    format!("{field}.image"),
                    "a project-relative path asset reference",
                )
            })?;
            let AssetRef::Path(path) = asset_ref else {
                return Err(ComponentCodecError::invalid_field(
                    format!("{field}.image.kind"),
                    "path",
                ));
            };
            Ok(UiBrush::Image {
                image: UiImageId::new(path.as_str())
                    .map_err(|error| codec_message(error.to_string()))?,
                tint: color,
            })
        }
        "solid" => Err(ComponentCodecError::invalid_field(
            format!("{field}.role"),
            "an empty role and no image for a solid brush",
        )),
        "role" => Err(ComponentCodecError::invalid_field(
            format!("{field}.image"),
            "no image for a role brush",
        )),
        "image" => Err(ComponentCodecError::invalid_field(
            format!("{field}.role"),
            "an empty role for an image brush",
        )),
        _ => Err(ComponentCodecError::invalid_field(
            format!("{field}.kind"),
            "solid, role, or image",
        )),
    }
}

fn brush_value(brush: &UiBrush) -> Result<ComponentValue, ComponentCodecError> {
    let (kind, color, role, image) = match brush {
        UiBrush::Solid(color) => ("solid", *color, String::new(), ComponentValue::Null),
        UiBrush::Role(role) => (
            "role",
            UiColor::rgba(0.0, 0.0, 0.0, 0.0),
            color_role_name(*role).to_string(),
            ComponentValue::Null,
        ),
        UiBrush::Image { image, tint } => {
            let asset_ref = AssetRef::path(image.as_str()).map_err(|error| {
                ComponentCodecError::invalid_asset_ref(
                    "image.value",
                    image.as_str(),
                    error.to_string(),
                )
            })?;
            ("image", *tint, String::new(), asset_ref_value(&asset_ref))
        }
    };
    let [red, green, blue, alpha] = color.components();
    Ok(ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_string())),
        ("red", float_value(red)?),
        ("green", float_value(green)?),
        ("blue", float_value(blue)?),
        ("alpha", float_value(alpha)?),
        ("role", ComponentValue::String(role)),
        ("image", image),
    ]))
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
    expect_fields(value, field, &["kind", "value"])?;
    let kind = read_string(value.field("kind")?, &format!("{field}.kind"))?;
    let reference = read_string(value.field("value")?, &format!("{field}.value"))?;
    match kind.as_str() {
        "path" => AssetRef::path(reference.clone()).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(
                format!("{field}.value"),
                reference,
                error.to_string(),
            )
        }),
        "stable_id" => AssetRef::stable_id(&reference).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(
                format!("{field}.value"),
                reference,
                error.to_string(),
            )
        }),
        _ => Err(ComponentCodecError::invalid_field(
            format!("{field}.kind"),
            "path or stable_id",
        )),
    }
}

pub(crate) fn asset_ref_value(asset_ref: &AssetRef) -> ComponentValue {
    match asset_ref {
        AssetRef::Path(path) => ComponentValue::map([
            ("kind", ComponentValue::String("path".to_owned())),
            ("value", ComponentValue::String(path.as_str().to_owned())),
        ]),
        AssetRef::StableId(id) => ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_owned())),
            ("value", ComponentValue::String(id.to_string())),
        ]),
    }
}

fn migrate_legacy_visual(mut value: ComponentValue) -> Result<ComponentValue, ComponentCodecError> {
    expect_fields(
        &value,
        "UiVisual",
        &["background", "border_width", "border_brush", "radii"],
    )?;
    migrate_legacy_brush_at(&mut value, "background")?;
    migrate_legacy_brush_at(&mut value, "border_brush")?;
    decode_visual(&value)?;
    Ok(value)
}

fn migrate_legacy_text(mut value: ComponentValue) -> Result<ComponentValue, ComponentCodecError> {
    expect_fields(
        &value,
        "UiText",
        &[
            "content",
            "logical_size",
            "brush",
            "alignment",
            "overflow",
            "has_semantic_text",
            "semantic_text",
        ],
    )?;
    migrate_legacy_brush_at(&mut value, "brush")?;
    decode_text(&value)?;
    Ok(value)
}

fn migrate_legacy_brush_at(
    value: &mut ComponentValue,
    field: &str,
) -> Result<(), ComponentCodecError> {
    let ComponentValue::Map(fields) = value else {
        return Err(ComponentCodecError::invalid_field(
            "component",
            "a component map",
        ));
    };
    let legacy = fields.get(field).ok_or_else(|| {
        ComponentCodecError::invalid_field(field, "a required legacy brush field")
    })?;
    let migrated = migrate_legacy_brush(legacy, field)?;
    fields.insert(field.to_owned(), migrated);
    Ok(())
}

fn migrate_legacy_brush(
    value: &ComponentValue,
    field: &str,
) -> Result<ComponentValue, ComponentCodecError> {
    expect_fields(value, field, LEGACY_BRUSH_FIELDS)?;
    let kind = read_string(value.field("kind")?, &format!("{field}.kind"))?;
    let color = read_flat_color(value, field)?;
    let reference = read_string(value.field("reference")?, &format!("{field}.reference"))?;
    let brush = match kind.as_str() {
        "solid" if reference.is_empty() => UiBrush::Solid(color),
        "role" => UiBrush::Role(read_color_role_name(
            &reference,
            &format!("{field}.reference"),
        )?),
        "image" => {
            let AssetRef::Path(path) = AssetRef::path(reference.clone()).map_err(|error| {
                ComponentCodecError::invalid_asset_ref(
                    format!("{field}.reference"),
                    reference.clone(),
                    error.to_string(),
                )
            })?
            else {
                return Err(ComponentCodecError::invalid_field(
                    format!("{field}.reference"),
                    "a project-relative path",
                ));
            };
            UiBrush::Image {
                image: UiImageId::new(path.as_str())
                    .map_err(|error| codec_message(error.to_string()))?,
                tint: color,
            }
        }
        "solid" => {
            return Err(ComponentCodecError::invalid_field(
                format!("{field}.reference"),
                "an empty string for a solid brush",
            ));
        }
        _ => {
            return Err(ComponentCodecError::invalid_field(
                format!("{field}.kind"),
                "solid, role, or image",
            ));
        }
    };
    brush_value(&brush)
}

fn read_radii(value: &ComponentValue) -> Result<UiCornerRadii, ComponentCodecError> {
    expect_fields(
        value,
        "radii",
        &["top_left", "top_right", "bottom_right", "bottom_left"],
    )?;
    let values = [
        read_non_negative(value.field("top_left")?, "radii.top_left")?,
        read_non_negative(value.field("top_right")?, "radii.top_right")?,
        read_non_negative(value.field("bottom_right")?, "radii.bottom_right")?,
        read_non_negative(value.field("bottom_left")?, "radii.bottom_left")?,
    ];
    if values.iter().all(|value| *value == values[0]) {
        return UiCornerRadii::all(values[0]).map_err(|error| codec_message(error.to_string()));
    }
    Err(ComponentCodecError::invalid_field(
        "radii",
        "equal first-pass corner radii",
    ))
}

fn radii_value(radii: UiCornerRadii) -> Result<ComponentValue, ComponentCodecError> {
    let [top_left, top_right, bottom_right, bottom_left] = radii.values();
    Ok(ComponentValue::map([
        ("top_left", float_value(top_left)?),
        ("top_right", float_value(top_right)?),
        ("bottom_right", float_value(bottom_right)?),
        ("bottom_left", float_value(bottom_left)?),
    ]))
}

fn read_palette(value: &ComponentValue) -> Result<UiThemePalette, ComponentCodecError> {
    const NAMES: &[&str] = &[
        "surface",
        "panel",
        "primary_text",
        "secondary_text",
        "accent",
        "positive",
        "warning",
        "danger",
        "disabled",
        "focus",
    ];
    expect_fields(value, "palette", NAMES)?;
    Ok(UiThemePalette {
        surface: read_color(value.field("surface")?, "palette.surface")?,
        panel: read_color(value.field("panel")?, "palette.panel")?,
        primary_text: read_color(value.field("primary_text")?, "palette.primary_text")?,
        secondary_text: read_color(value.field("secondary_text")?, "palette.secondary_text")?,
        accent: read_color(value.field("accent")?, "palette.accent")?,
        positive: read_color(value.field("positive")?, "palette.positive")?,
        warning: read_color(value.field("warning")?, "palette.warning")?,
        danger: read_color(value.field("danger")?, "palette.danger")?,
        disabled: read_color(value.field("disabled")?, "palette.disabled")?,
        focus: read_color(value.field("focus")?, "palette.focus")?,
    })
}

fn palette_value(palette: UiThemePalette) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("surface", color_value(palette.surface)?),
        ("panel", color_value(palette.panel)?),
        ("primary_text", color_value(palette.primary_text)?),
        ("secondary_text", color_value(palette.secondary_text)?),
        ("accent", color_value(palette.accent)?),
        ("positive", color_value(palette.positive)?),
        ("warning", color_value(palette.warning)?),
        ("danger", color_value(palette.danger)?),
        ("disabled", color_value(palette.disabled)?),
        ("focus", color_value(palette.focus)?),
    ]))
}

fn read_color(value: &ComponentValue, field: &str) -> Result<UiColor, ComponentCodecError> {
    expect_fields(value, field, COLOR_FIELDS)?;
    read_flat_color(value, field)
}

fn read_flat_color(value: &ComponentValue, field: &str) -> Result<UiColor, ComponentCodecError> {
    Ok(UiColor::rgba(
        read_unit(value.field("red")?, &format!("{field}.red"))?,
        read_unit(value.field("green")?, &format!("{field}.green"))?,
        read_unit(value.field("blue")?, &format!("{field}.blue"))?,
        read_unit(value.field("alpha")?, &format!("{field}.alpha"))?,
    ))
}

fn color_value(color: UiColor) -> Result<ComponentValue, ComponentCodecError> {
    let [red, green, blue, alpha] = color.components();
    Ok(ComponentValue::map([
        ("red", float_value(red)?),
        ("green", float_value(green)?),
        ("blue", float_value(blue)?),
        ("alpha", float_value(alpha)?),
    ]))
}

fn expect_fields(
    value: &ComponentValue,
    field: &str,
    expected: &[&str],
) -> Result<(), ComponentCodecError> {
    let ComponentValue::Map(fields) = value else {
        return Err(ComponentCodecError::invalid_field(field, "an exact map"));
    };
    if fields.len() != expected.len() || expected.iter().any(|name| !fields.contains_key(*name)) {
        return Err(ComponentCodecError::invalid_field(
            field,
            format!("exact fields: {}", expected.join(", ")),
        ));
    }
    Ok(())
}

fn read_string(value: &ComponentValue, field: &str) -> Result<String, ComponentCodecError> {
    match value {
        ComponentValue::String(value) => Ok(value.clone()),
        _ => Err(ComponentCodecError::invalid_field(field, "a string")),
    }
}

fn read_color_role(
    value: &ComponentValue,
    field: &str,
) -> Result<UiColorRole, ComponentCodecError> {
    let role = read_string(value, field)?;
    read_color_role_name(&role, field)
}

fn read_bool(value: &ComponentValue, field: &str) -> Result<bool, ComponentCodecError> {
    value
        .as_bool()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "a bool"))
}

fn read_u64(value: &ComponentValue, field: &str) -> Result<u64, ComponentCodecError> {
    value
        .as_u64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "a u64"))
}

fn read_i32(value: &ComponentValue, field: &str) -> Result<i32, ComponentCodecError> {
    let value = value
        .as_i64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "an i32"))?;
    i32::try_from(value).map_err(|_| ComponentCodecError::invalid_field(field, "an i32"))
}

fn read_f32(value: &ComponentValue, field: &str) -> Result<f32, ComponentCodecError> {
    let value = value
        .as_f64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "a finite f32"))?;
    if !value.is_finite() || value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(ComponentCodecError::invalid_field(field, "a finite f32"));
    }
    Ok(value as f32)
}

fn read_non_negative(value: &ComponentValue, field: &str) -> Result<f32, ComponentCodecError> {
    let value = read_f32(value, field)?;
    if value < 0.0 {
        return Err(ComponentCodecError::invalid_field(
            field,
            "a non-negative f32",
        ));
    }
    Ok(value)
}

fn read_positive(value: &ComponentValue, field: &str) -> Result<f32, ComponentCodecError> {
    let value = read_f32(value, field)?;
    if value <= 0.0 {
        return Err(ComponentCodecError::invalid_field(field, "a positive f32"));
    }
    Ok(value)
}

fn read_unit(value: &ComponentValue, field: &str) -> Result<f32, ComponentCodecError> {
    let value = read_f32(value, field)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(ComponentCodecError::invalid_field(
            field,
            "a value in 0.0..=1.0",
        ));
    }
    Ok(value)
}

fn float_value(value: f32) -> Result<ComponentValue, ComponentCodecError> {
    ComponentValue::f64(f64::from(value)).map_err(|error| codec_message(error.to_string()))
}

fn codec_message(message: String) -> ComponentCodecError {
    ComponentCodecError::Message(message)
}

macro_rules! string_enum_reader {
    ($name:ident, $type:ty, $expected:literal, {$($text:literal => $value:expr),+ $(,)?}) => {
        fn $name(value: &ComponentValue, field: &str) -> Result<$type, ComponentCodecError> {
            match read_string(value, field)?.as_str() {
                $($text => Ok($value),)+
                _ => Err(ComponentCodecError::invalid_field(field, $expected)),
            }
        }
    };
}

string_enum_reader!(read_layer, UiLayer, "base, overlay, or modal", {
    "base" => UiLayer::Base, "overlay" => UiLayer::Overlay, "modal" => UiLayer::Modal,
});
string_enum_reader!(read_position_mode, UiPositionMode, "flow or absolute", {
    "flow" => UiPositionMode::Flow, "absolute" => UiPositionMode::Absolute,
});
string_enum_reader!(read_flex_direction, UiFlexDirection, "row or column", {
    "row" => UiFlexDirection::Row, "column" => UiFlexDirection::Column,
});
string_enum_reader!(read_align, UiAlign, "start, center, end, or stretch", {
    "start" => UiAlign::Start, "center" => UiAlign::Center,
    "end" => UiAlign::End, "stretch" => UiAlign::Stretch,
});
string_enum_reader!(read_justify, UiJustify, "start, center, end, or space_between", {
    "start" => UiJustify::Start, "center" => UiJustify::Center,
    "end" => UiJustify::End, "space_between" => UiJustify::SpaceBetween,
});
string_enum_reader!(read_overflow, UiOverflow, "visible or clip", {
    "visible" => UiOverflow::Visible, "clip" => UiOverflow::Clip,
});
string_enum_reader!(read_text_alignment, UiTextAlignment, "start, center, or end", {
    "start" => UiTextAlignment::Start, "center" => UiTextAlignment::Center,
    "end" => UiTextAlignment::End,
});
string_enum_reader!(read_text_overflow, UiTextOverflow, "wrap or clip", {
    "wrap" => UiTextOverflow::Wrap, "clip" => UiTextOverflow::Clip,
});
string_enum_reader!(read_semantic_role, UiSemanticRole, "group, text, button, or status", {
    "group" => UiSemanticRole::Group, "text" => UiSemanticRole::Text,
    "button" => UiSemanticRole::Button, "status" => UiSemanticRole::Status,
});

fn read_color_role_name(value: &str, field: &str) -> Result<UiColorRole, ComponentCodecError> {
    match value {
        "surface" => Ok(UiColorRole::Surface),
        "panel" => Ok(UiColorRole::Panel),
        "primary_text" => Ok(UiColorRole::PrimaryText),
        "secondary_text" => Ok(UiColorRole::SecondaryText),
        "accent" => Ok(UiColorRole::Accent),
        "positive" => Ok(UiColorRole::Positive),
        "warning" => Ok(UiColorRole::Warning),
        "danger" => Ok(UiColorRole::Danger),
        "disabled" => Ok(UiColorRole::Disabled),
        "focus" => Ok(UiColorRole::Focus),
        _ => Err(ComponentCodecError::invalid_field(
            field,
            "a UI theme color role",
        )),
    }
}

const fn layer_name(value: UiLayer) -> &'static str {
    match value {
        UiLayer::Base => "base",
        UiLayer::Overlay => "overlay",
        UiLayer::Modal => "modal",
    }
}
const fn position_mode_name(value: UiPositionMode) -> &'static str {
    match value {
        UiPositionMode::Flow => "flow",
        UiPositionMode::Absolute => "absolute",
    }
}
const fn flex_direction_name(value: UiFlexDirection) -> &'static str {
    match value {
        UiFlexDirection::Row => "row",
        UiFlexDirection::Column => "column",
    }
}
const fn align_name(value: UiAlign) -> &'static str {
    match value {
        UiAlign::Start => "start",
        UiAlign::Center => "center",
        UiAlign::End => "end",
        UiAlign::Stretch => "stretch",
    }
}
const fn justify_name(value: UiJustify) -> &'static str {
    match value {
        UiJustify::Start => "start",
        UiJustify::Center => "center",
        UiJustify::End => "end",
        UiJustify::SpaceBetween => "space_between",
    }
}
const fn overflow_name(value: UiOverflow) -> &'static str {
    match value {
        UiOverflow::Visible => "visible",
        UiOverflow::Clip => "clip",
    }
}
const fn text_alignment_name(value: UiTextAlignment) -> &'static str {
    match value {
        UiTextAlignment::Start => "start",
        UiTextAlignment::Center => "center",
        UiTextAlignment::End => "end",
    }
}
const fn text_overflow_name(value: UiTextOverflow) -> &'static str {
    match value {
        UiTextOverflow::Wrap => "wrap",
        UiTextOverflow::Clip => "clip",
    }
}
const fn semantic_role_name(value: UiSemanticRole) -> &'static str {
    match value {
        UiSemanticRole::Group => "group",
        UiSemanticRole::Text => "text",
        UiSemanticRole::Button => "button",
        UiSemanticRole::Status => "status",
    }
}
const fn color_role_name(value: UiColorRole) -> &'static str {
    match value {
        UiColorRole::Surface => "surface",
        UiColorRole::Panel => "panel",
        UiColorRole::PrimaryText => "primary_text",
        UiColorRole::SecondaryText => "secondary_text",
        UiColorRole::Accent => "accent",
        UiColorRole::Positive => "positive",
        UiColorRole::Warning => "warning",
        UiColorRole::Danger => "danger",
        UiColorRole::Disabled => "disabled",
        UiColorRole::Focus => "focus",
    }
}

fn ui_root_schema() -> ComponentSchema {
    schema(
        UI_ROOT_ID,
        "UI root",
        &[field("layer", ComponentValueKind::String)],
    )
}

fn ui_node_schema() -> ComponentSchema {
    let mut fields = Vec::new();
    for (name, kind) in [
        ("style.position_mode", ComponentValueKind::String),
        ("style.flex_direction", ComponentValueKind::String),
        ("style.align_items", ComponentValueKind::String),
        ("style.justify_content", ComponentValueKind::String),
        ("style.gap", ComponentValueKind::F64),
        ("style.padding.left", ComponentValueKind::F64),
        ("style.padding.right", ComponentValueKind::F64),
        ("style.padding.top", ComponentValueKind::F64),
        ("style.padding.bottom", ComponentValueKind::F64),
        ("style.flex_grow", ComponentValueKind::F64),
        ("style.flex_shrink", ComponentValueKind::F64),
        ("style.z_index", ComponentValueKind::I64),
        ("style.visible", ComponentValueKind::Bool),
        ("style.overflow", ComponentValueKind::String),
        ("style.stacking_context", ComponentValueKind::Bool),
    ] {
        fields.push(field(name, kind));
    }
    for dimension in ["width", "height", "left", "top"] {
        fields.push(field(
            &format!("style.{dimension}.kind"),
            ComponentValueKind::String,
        ));
        fields.push(field(
            &format!("style.{dimension}.value"),
            ComponentValueKind::F64,
        ));
    }
    schema(UI_NODE_ID, "UI node", &fields)
}

fn ui_visual_schema() -> ComponentSchema {
    let mut fields = Vec::new();
    add_brush_fields(&mut fields, "background");
    fields.push(field("border_width", ComponentValueKind::F64));
    add_brush_fields(&mut fields, "border_brush");
    for name in ["top_left", "top_right", "bottom_right", "bottom_left"] {
        fields.push(field(&format!("radii.{name}"), ComponentValueKind::F64));
    }
    schema_with_version(UI_VISUAL_ID, "UI visual", UI_SCHEMA_VERSION_TWO, &fields)
        .with_field_tombstones([
            ComponentFieldId::new("background.reference"),
            ComponentFieldId::new("border_brush.reference"),
        ])
}

fn ui_text_schema() -> ComponentSchema {
    let mut fields = vec![
        field("content", ComponentValueKind::String),
        field("logical_size", ComponentValueKind::F64),
        field("alignment", ComponentValueKind::String),
        field("overflow", ComponentValueKind::String),
        field("has_semantic_text", ComponentValueKind::Bool),
        field("semantic_text", ComponentValueKind::String),
    ];
    add_brush_fields(&mut fields, "brush");
    schema_with_version(UI_TEXT_ID, "UI text", UI_SCHEMA_VERSION_TWO, &fields)
        .with_field_tombstones([ComponentFieldId::new("brush.reference")])
}

fn legacy_ui_visual_schema() -> ComponentSchema {
    let mut fields = Vec::new();
    add_legacy_brush_fields(&mut fields, "background");
    fields.push(field("border_width", ComponentValueKind::F64));
    add_legacy_brush_fields(&mut fields, "border_brush");
    for name in ["top_left", "top_right", "bottom_right", "bottom_left"] {
        fields.push(field(&format!("radii.{name}"), ComponentValueKind::F64));
    }
    schema(UI_VISUAL_ID, "UI visual", &fields)
}

fn legacy_ui_text_schema() -> ComponentSchema {
    let mut fields = vec![
        field("content", ComponentValueKind::String),
        field("logical_size", ComponentValueKind::F64),
        field("alignment", ComponentValueKind::String),
        field("overflow", ComponentValueKind::String),
        field("has_semantic_text", ComponentValueKind::Bool),
        field("semantic_text", ComponentValueKind::String),
    ];
    add_legacy_brush_fields(&mut fields, "brush");
    schema(UI_TEXT_ID, "UI text", &fields)
}

fn ui_button_schema() -> ComponentSchema {
    schema(
        UI_BUTTON_ID,
        "UI button",
        &[
            field("label", ComponentValueKind::String),
            field("action", ComponentValueKind::String),
            field("enabled", ComponentValueKind::Bool),
            field("logical_text_size", ComponentValueKind::F64),
        ],
    )
}

fn ui_button_visuals_schema() -> ComponentSchema {
    schema(
        UI_BUTTON_VISUALS_ID,
        "UI button interaction visuals",
        &[
            field("normal_text", ComponentValueKind::String),
            field("hovered_background", ComponentValueKind::String),
            field("hovered_text", ComponentValueKind::String),
            field("pressed_background", ComponentValueKind::String),
            field("pressed_text", ComponentValueKind::String),
            field("focused_border", ComponentValueKind::String),
            field("focused_text", ComponentValueKind::String),
            field("focus_border_width", ComponentValueKind::F64),
            field("disabled_background", ComponentValueKind::String),
            field("disabled_text", ComponentValueKind::String),
        ],
    )
}

fn ui_semantics_schema() -> ComponentSchema {
    schema(
        UI_SEMANTICS_ID,
        "UI semantics",
        &[
            field("role", ComponentValueKind::String),
            field("name", ComponentValueKind::String),
            field("has_value", ComponentValueKind::Bool),
            field("value", ComponentValueKind::String),
            field("has_selected", ComponentValueKind::Bool),
            field("selected", ComponentValueKind::Bool),
            field("has_expanded", ComponentValueKind::Bool),
            field("expanded", ComponentValueKind::Bool),
            field("actions", ComponentValueKind::List),
        ],
    )
}

fn ui_theme_schema() -> ComponentSchema {
    let mut fields = vec![field("revision", ComponentValueKind::U64)];
    for role in [
        "surface",
        "panel",
        "primary_text",
        "secondary_text",
        "accent",
        "positive",
        "warning",
        "danger",
        "disabled",
        "focus",
    ] {
        for channel in COLOR_FIELDS {
            fields.push(field(
                &format!("palette.{role}.{channel}"),
                ComponentValueKind::F64,
            ));
        }
    }
    schema(UI_THEME_ID, "UI theme", &fields)
}

fn schema(id: &str, alias: &str, fields: &[ComponentFieldSchema]) -> ComponentSchema {
    schema_with_version(id, alias, ComponentSchemaVersion::ONE, fields)
}

fn schema_with_version(
    id: &str,
    alias: &str,
    version: ComponentSchemaVersion,
    fields: &[ComponentFieldSchema],
) -> ComponentSchema {
    ComponentSchema::new(ComponentTypeId::new(id), alias, version)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(fields.iter().cloned())
}

fn field(path: &str, kind: ComponentValueKind) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new(path),
        path,
        ComponentFieldPath::from_fields(path.split('.')),
        kind,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
}

fn add_brush_fields(fields: &mut Vec<ComponentFieldSchema>, prefix: &str) {
    for name in ["kind", "red", "green", "blue", "alpha", "role"] {
        let kind = if matches!(name, "red" | "green" | "blue" | "alpha") {
            ComponentValueKind::F64
        } else {
            ComponentValueKind::String
        };
        fields.push(field(&format!("{prefix}.{name}"), kind));
    }
    fields.push(
        ComponentFieldSchema::optional(
            ComponentFieldId::new(format!("{prefix}.image")),
            format!("{prefix}.image"),
            ComponentFieldPath::from_fields([prefix, "image"]),
            ComponentValueKind::AssetRef,
        )
        .with_capabilities([
            ComponentCapability::Scene,
            ComponentCapability::Inspect,
            ComponentCapability::Edit,
            ComponentCapability::AssetRef,
        ]),
    );
}

fn add_legacy_brush_fields(fields: &mut Vec<ComponentFieldSchema>, prefix: &str) {
    for name in LEGACY_BRUSH_FIELDS {
        let kind = if matches!(*name, "red" | "green" | "blue" | "alpha") {
            ComponentValueKind::F64
        } else {
            ComponentValueKind::String
        };
        fields.push(field(&format!("{prefix}.{name}"), kind));
    }
}
