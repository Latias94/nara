use nara_identity::EntityReference;
use nara_reflect::{ComponentValue, ComponentValueKind};
use nara_tooling::{SceneInspectorComponentView, SceneInspectorFieldView};

pub(crate) fn component_title(component: &SceneInspectorComponentView) -> String {
    component.component.as_str().to_owned()
}

pub(crate) fn field_value_label(field: &SceneInspectorFieldView) -> String {
    match field.value.as_ref().or(field.default_value.as_ref()) {
        Some(value) => component_value_label(value),
        None => "<missing>".to_owned(),
    }
}

pub(crate) fn field_edit_source_text(field: &SceneInspectorFieldView) -> String {
    field
        .value
        .as_ref()
        .or(field.default_value.as_ref())
        .and_then(|value| editable_value_text(field.value_kind, value))
        .unwrap_or_else(|| empty_editable_text(field.value_kind).to_owned())
}

pub(crate) fn value_kind_label(kind: ComponentValueKind) -> &'static str {
    match kind {
        ComponentValueKind::Null => "null",
        ComponentValueKind::Bool => "bool",
        ComponentValueKind::I64 => "i64",
        ComponentValueKind::U64 => "u64",
        ComponentValueKind::F64 => "f64",
        ComponentValueKind::String => "string",
        ComponentValueKind::List => "list",
        ComponentValueKind::Map => "map",
        ComponentValueKind::AssetRef => "asset_ref",
        ComponentValueKind::EntityRef => "entity_ref",
    }
}

pub(crate) fn parse_editable_value(
    kind: ComponentValueKind,
    text: &str,
) -> Result<ComponentValue, String> {
    match kind {
        ComponentValueKind::Bool => text
            .parse::<bool>()
            .map(ComponentValue::Bool)
            .map_err(|_| "expected true or false".to_owned()),
        ComponentValueKind::I64 => text
            .parse::<i64>()
            .map(ComponentValue::I64)
            .map_err(|_| "expected a signed integer".to_owned()),
        ComponentValueKind::U64 => text
            .parse::<u64>()
            .map(ComponentValue::U64)
            .map_err(|_| "expected an unsigned integer".to_owned()),
        ComponentValueKind::F64 => {
            let value = text
                .parse::<f64>()
                .map_err(|_| "expected a finite float".to_owned())?;
            ComponentValue::f64(value).map_err(|_| "expected a finite float".to_owned())
        }
        ComponentValueKind::String => Ok(ComponentValue::String(text.to_owned())),
        ComponentValueKind::Null
        | ComponentValueKind::List
        | ComponentValueKind::Map
        | ComponentValueKind::AssetRef
        | ComponentValueKind::EntityRef => Err(format!(
            "{} fields are read-only in this panel",
            value_kind_label(kind)
        )),
    }
}

fn component_value_label(value: &ComponentValue) -> String {
    match value {
        ComponentValue::Null => "null".to_owned(),
        ComponentValue::Bool(value) => value.to_string(),
        ComponentValue::I64(value) => value.to_string(),
        ComponentValue::U64(value) => value.to_string(),
        ComponentValue::F64(value) => value.get().to_string(),
        ComponentValue::String(value) => value.clone(),
        ComponentValue::List(values) => format!("[{} items]", values.len()),
        ComponentValue::Map(fields) => format!("{{{} fields}}", fields.len()),
        ComponentValue::EntityReference(reference) => entity_reference_label(reference),
    }
}

fn entity_reference_label(reference: &EntityReference) -> String {
    match reference {
        EntityReference::SceneLocal { entity } => format!("scene-local:{}", entity.as_str()),
        EntityReference::Persistent { entity } => {
            format!("{}:{}", entity.namespace.as_str(), entity.entity)
        }
    }
}

fn editable_value_text(kind: ComponentValueKind, value: &ComponentValue) -> Option<String> {
    match kind {
        ComponentValueKind::Bool => value.as_bool().map(|value| value.to_string()),
        ComponentValueKind::I64 => value.as_i64().map(|value| value.to_string()),
        ComponentValueKind::U64 => value.as_u64().map(|value| value.to_string()),
        ComponentValueKind::F64 => value.as_f64().map(|value| value.to_string()),
        ComponentValueKind::String => value.as_str().map(ToOwned::to_owned),
        ComponentValueKind::Null
        | ComponentValueKind::List
        | ComponentValueKind::Map
        | ComponentValueKind::AssetRef
        | ComponentValueKind::EntityRef => None,
    }
}

fn empty_editable_text(kind: ComponentValueKind) -> &'static str {
    match kind {
        ComponentValueKind::Bool => "false",
        ComponentValueKind::I64 | ComponentValueKind::U64 => "0",
        ComponentValueKind::F64 => "0.0",
        ComponentValueKind::String
        | ComponentValueKind::Null
        | ComponentValueKind::List
        | ComponentValueKind::Map
        | ComponentValueKind::AssetRef
        | ComponentValueKind::EntityRef => "",
    }
}
