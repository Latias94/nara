use std::collections::BTreeMap;

use nara_asset::{AssetRef, ImportedProductId};
use nara_reflect::{
    ComponentCodecError, ComponentMapRef, ComponentValue, ComponentValueMaterializationLimits,
    ComponentValueRef, decode_asset_reference_value, encode_asset_reference_value,
};

use crate::{
    Material2dDeclaration, MaterialParameterValue, MaterialParameterValues, asset::Material2dAsset,
    declaration::MAX_MATERIAL_TEXTURES, parameters::MAX_MATERIAL_PARAMETERS,
};

use super::{
    validate_asset_values, validate_authored_values, validate_declaration, validate_field_id,
};

const PARAMETER_TAGS: [&str; 6] = ["f32", "i32", "u32", "vec2", "vec3", "vec4"];

/// A complete durable material assignment containing only semantic values and asset references.
///
/// `from_component_value` admits wire shape and bounded data. The content owner calls
/// `validate_asset` after loading the actual source or processed material and before
/// staging gameplay or Scene state. No current asset or World is consulted here.
#[derive(Clone, Debug, PartialEq)]
pub struct Material2dRecord {
    pub(super) material: AssetRef,
    pub(super) declaration_id: String,
    pub(super) declaration_version: u32,
    pub(super) parameters: MaterialParameterValues,
    pub(super) texture_overrides: BTreeMap<String, AssetRef>,
}

impl Material2dRecord {
    /// Joins full semantic values and optional texture replacements to a declaration.
    pub fn new(
        material: AssetRef,
        declaration: &Material2dDeclaration,
        parameters: MaterialParameterValues,
        texture_overrides: BTreeMap<String, AssetRef>,
    ) -> Result<Self, ComponentCodecError> {
        validate_declaration(declaration, &parameters, &texture_overrides)?;
        let record = Self {
            material,
            declaration_id: declaration.id().to_owned(),
            declaration_version: declaration.version(),
            parameters,
            texture_overrides,
        };
        record.to_component_value()?;
        Ok(record)
    }

    #[must_use]
    pub const fn material(&self) -> &AssetRef {
        &self.material
    }

    #[must_use]
    pub fn declaration_id(&self) -> &str {
        &self.declaration_id
    }

    #[must_use]
    pub const fn declaration_version(&self) -> u32 {
        self.declaration_version
    }

    #[must_use]
    pub fn parameters(&self) -> &MaterialParameterValues {
        &self.parameters
    }

    #[must_use]
    pub fn texture_overrides(&self) -> &BTreeMap<String, AssetRef> {
        &self.texture_overrides
    }

    /// Validates the entire assignment against the caller's actual loaded material.
    pub fn validate_asset(&self, asset: &Material2dAsset) -> Result<(), ComponentCodecError> {
        let product_matches = match &self.material {
            AssetRef::Path(_) => true,
            AssetRef::StableId(id) => {
                *id == asset.product().source()
                    && asset.product().product() == ImportedProductId::PRIMARY
            }
            AssetRef::Product(product) => *product == asset.product(),
        };
        if !product_matches {
            return Err(ComponentCodecError::invalid_asset_ref(
                "material",
                self.material.to_string(),
                "the record and the supplied material asset select different Products",
            ));
        }
        validate_asset_values(
            &self.declaration_id,
            self.declaration_version,
            &self.parameters,
            &self.texture_overrides,
            asset,
        )
    }

    /// Encodes the canonical record with the ordinary native materialization limits.
    pub fn to_component_value(&self) -> Result<ComponentValue, ComponentCodecError> {
        self.to_component_value_with_limits(ComponentValueMaterializationLimits::default())
    }

    /// Encodes semantic values and applies the caller's exact node, byte and depth ceilings.
    pub fn to_component_value_with_limits(
        &self,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<ComponentValue, ComponentCodecError> {
        validate_authored_values(
            &self.declaration_id,
            self.declaration_version,
            &self.parameters,
            &self.texture_overrides,
        )?;
        let mut parameters: BTreeMap<_, _> = PARAMETER_TAGS
            .map(|tag| (tag.to_owned(), BTreeMap::new()))
            .into_iter()
            .collect();
        for (id, value) in &self.parameters {
            let (tag, payload) = parameter_value(*value)?;
            parameters
                .get_mut(tag)
                .expect("every material parameter kind has a typed map")
                .insert(id.clone(), payload);
        }
        let parameters = parameters
            .into_iter()
            .map(|(tag, values)| (tag, ComponentValue::Map(values)))
            .collect();
        let overrides = self
            .texture_overrides
            .iter()
            .map(|(id, reference)| (id.clone(), encode_asset_reference_value(reference)))
            .collect();
        let value = ComponentValue::map([
            ("material", encode_asset_reference_value(&self.material)),
            (
                "declaration_id",
                ComponentValue::String(self.declaration_id.clone()),
            ),
            (
                "declaration_version",
                ComponentValue::U64(u64::from(self.declaration_version)),
            ),
            ("parameters", ComponentValue::Map(parameters)),
            ("texture_overrides", ComponentValue::Map(overrides)),
        ]);
        nara_reflect::__private::validate_value_materialization_budget(&value, limits)?;
        Ok(value)
    }

    /// Parses an exact record without looking up its material asset.
    pub fn from_component_value<'a>(
        value: impl Into<ComponentValueRef<'a>>,
    ) -> Result<Self, ComponentCodecError> {
        Self::from_component_value_with_limits(
            value,
            ComponentValueMaterializationLimits::default(),
        )
    }

    /// Admits the complete tree before allocating owned IDs, values or asset references.
    pub fn from_component_value_with_limits<'a>(
        value: impl Into<ComponentValueRef<'a>>,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<Self, ComponentCodecError> {
        let value = value.into();
        nara_reflect::__private::validate_value_materialization_budget(value, limits)?;
        let fields = read_map(value, "<material2d>")?;
        let expected = [
            "material",
            "declaration_id",
            "declaration_version",
            "parameters",
            "texture_overrides",
        ];
        if fields.len() != expected.len() || fields.keys().any(|key| !expected.contains(&key)) {
            return Err(ComponentCodecError::invalid_field(
                "<material2d>",
                "exactly material, declaration_id, declaration_version, parameters and texture_overrides",
            ));
        }
        let declaration_id = value.field("declaration_id")?.as_str().ok_or_else(|| {
            ComponentCodecError::invalid_field("declaration_id", "a stable declaration ID string")
        })?;
        validate_field_id(declaration_id, "declaration_id")?;
        let declaration_version =
            read_u32(value.field("declaration_version")?, "declaration_version")?;
        let parameter_fields = read_map(value.field("parameters")?, "parameters")?;
        if parameter_fields.len() != PARAMETER_TAGS.len()
            || parameter_fields
                .keys()
                .any(|tag| !PARAMETER_TAGS.contains(&tag))
        {
            return Err(ComponentCodecError::invalid_field(
                "parameters",
                "exactly the f32, i32, u32, vec2, vec3 and vec4 typed maps",
            ));
        }
        let mut count = 0_usize;
        for (tag, values) in parameter_fields {
            count = count.saturating_add(read_map(values, &format!("parameters.{tag}"))?.len());
        }
        if count == 0 || count > MAX_MATERIAL_PARAMETERS {
            return Err(ComponentCodecError::invalid_field(
                "parameters",
                format!("1 through {MAX_MATERIAL_PARAMETERS} semantic parameters"),
            ));
        }
        let mut parameters = BTreeMap::new();
        for (tag, values) in parameter_fields {
            for (id, value) in read_map(values, &format!("parameters.{tag}"))? {
                validate_field_id(id, "parameters")?;
                if parameters
                    .insert(
                        id.to_owned(),
                        read_parameter(tag, value, &format!("parameters.{tag}.{id}"))?,
                    )
                    .is_some()
                {
                    return Err(ComponentCodecError::invalid_field(
                        "parameters",
                        "each semantic ID in exactly one typed map",
                    ));
                }
            }
        }
        let texture_fields = read_map(value.field("texture_overrides")?, "texture_overrides")?;
        if texture_fields.len() > MAX_MATERIAL_TEXTURES {
            return Err(ComponentCodecError::invalid_field(
                "texture_overrides",
                format!("at most {MAX_MATERIAL_TEXTURES} texture overrides"),
            ));
        }
        let mut texture_overrides = BTreeMap::new();
        for (id, value) in texture_fields {
            validate_field_id(id, "texture_overrides")?;
            texture_overrides.insert(id.to_owned(), decode_asset_reference_value(value)?);
        }
        validate_authored_values(
            declaration_id,
            declaration_version,
            &parameters,
            &texture_overrides,
        )?;
        Ok(Self {
            material: decode_asset_reference_value(value.field("material")?)?,
            declaration_id: declaration_id.to_owned(),
            declaration_version,
            parameters,
            texture_overrides,
        })
    }
}

pub(super) fn parameter_value(
    value: MaterialParameterValue,
) -> Result<(&'static str, ComponentValue), ComponentCodecError> {
    Ok(match value {
        MaterialParameterValue::F32(value) => ("f32", ComponentValue::f64(f64::from(value))?),
        MaterialParameterValue::I32(value) => ("i32", ComponentValue::I64(i64::from(value))),
        MaterialParameterValue::U32(value) => ("u32", ComponentValue::U64(u64::from(value))),
        MaterialParameterValue::Vec2(value) => ("vec2", vector_value(value)?),
        MaterialParameterValue::Vec3(value) => ("vec3", vector_value(value)?),
        MaterialParameterValue::Vec4(value) => ("vec4", vector_value(value)?),
    })
}

fn vector_value<const N: usize>(value: [f32; N]) -> Result<ComponentValue, ComponentCodecError> {
    value
        .into_iter()
        .map(|value| ComponentValue::f64(f64::from(value)))
        .collect::<Result<Vec<_>, _>>()
        .map(ComponentValue::List)
        .map_err(Into::into)
}

fn read_map<'a>(
    value: ComponentValueRef<'a>,
    field: &str,
) -> Result<ComponentMapRef<'a>, ComponentCodecError> {
    value
        .as_map()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "a map"))
}

fn read_parameter(
    kind: &str,
    value: ComponentValueRef<'_>,
    field: &str,
) -> Result<MaterialParameterValue, ComponentCodecError> {
    match kind {
        "f32" => read_f32(value, field).map(MaterialParameterValue::F32),
        "i32" => value
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .map(MaterialParameterValue::I32)
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "an i32")),
        "u32" => read_u32(value, field).map(MaterialParameterValue::U32),
        "vec2" => read_vector(value, field).map(MaterialParameterValue::Vec2),
        "vec3" => read_vector(value, field).map(MaterialParameterValue::Vec3),
        "vec4" => read_vector(value, field).map(MaterialParameterValue::Vec4),
        _ => Err(ComponentCodecError::invalid_field(
            field,
            "one of f32, i32, u32, vec2, vec3 or vec4",
        )),
    }
}

fn read_u32(value: ComponentValueRef<'_>, field: &str) -> Result<u32, ComponentCodecError> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "a u32"))
}

fn read_f32(value: ComponentValueRef<'_>, field: &str) -> Result<f32, ComponentCodecError> {
    let value = value
        .as_f64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "a finite f32"))?;
    if !value.is_finite() || value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(ComponentCodecError::invalid_field(field, "a finite f32"));
    }
    Ok(value as f32)
}

fn read_vector<const N: usize>(
    value: ComponentValueRef<'_>,
    field: &str,
) -> Result<[f32; N], ComponentCodecError> {
    let Some(values) = value.as_list() else {
        return Err(ComponentCodecError::invalid_field(
            field,
            format!("a list of {N} finite f32 values"),
        ));
    };
    if values.len() != N {
        return Err(ComponentCodecError::invalid_field(
            field,
            format!("a list of {N} finite f32 values"),
        ));
    }
    let mut result = [0.0; N];
    for (destination, value) in result.iter_mut().zip(values) {
        *destination = read_f32(value, field)?;
    }
    Ok(result)
}
