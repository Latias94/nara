use std::collections::BTreeMap;

use nara_reflect::{
    AssetReferenceDemand, ComponentCapability, ComponentCodecError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentNumericDomain, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape,
};

use crate::{Material2dDeclaration, MaterialParameterKind, MaterialParameterValues};

use super::{record::parameter_value, validate_declaration};

/// Returns the only registered native material carrier, independent of material families.
#[must_use]
pub fn material2d_schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.material2d"),
        "Material2d",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        required(
            "material",
            ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
        )
        .with_capability(ComponentCapability::AssetRef),
        required("declaration_id", ComponentValueKind::String),
        required("declaration_version", ComponentValueKind::U64)
            .with_numeric_domain(ComponentNumericDomain::U32),
        required(
            "parameters",
            ComponentValueShape::record(
                [
                    MaterialParameterKind::F32,
                    MaterialParameterKind::I32,
                    MaterialParameterKind::U32,
                    MaterialParameterKind::Vec2,
                    MaterialParameterKind::Vec3,
                    MaterialParameterKind::Vec4,
                ]
                .map(|kind| {
                    let (tag, shape) = parameter_shape(kind);
                    (tag.to_owned(), ComponentValueShape::map(shape))
                }),
            ),
        ),
        required(
            "texture_overrides",
            ComponentValueShape::map(ComponentValueShape::asset_ref(
                AssetReferenceDemand::Immediate,
            )),
        )
        .with_capability(ComponentCapability::AssetRef),
    ])
}

impl Material2dDeclaration {
    /// Projects actual semantic fields into their durable record paths and defaults.
    ///
    /// Field IDs use separate parameter/texture namespaces and the BLAKE3 of the complete
    /// semantic ID, so every admitted ID fits without truncation. Parameter paths contain the
    /// three literal segments `parameters`, the scalar/vector tag and the complete semantic ID.
    /// Texture overrides remain optional Immediate AssetRef leaves at their complete key.
    /// No shader symbol, offset or Rust TypeId is stored.
    /// The result is metadata; it does not register another native component.
    pub fn binding_fields(
        &self,
        default_parameters: &MaterialParameterValues,
    ) -> Result<Vec<ComponentFieldSchema>, ComponentCodecError> {
        validate_declaration(self, default_parameters, &BTreeMap::<String, ()>::new())?;
        let mut fields =
            Vec::with_capacity(self.parameters().fields().len() + self.textures().len() + 1);
        for field in self.parameters().fields() {
            let (tag, shape) = parameter_shape(field.kind());
            let id = projected_id("parameter", field.id());
            let value = default_parameters.get(field.id()).ok_or_else(|| {
                ComponentCodecError::missing_field(format!("parameters.{}", field.id()))
            })?;
            let (_, payload) = parameter_value(*value)?;
            let mut projected = ComponentFieldSchema::required(
                id,
                field.id(),
                ComponentFieldPath::from_fields(["parameters", tag, field.id()]),
                shape,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING);
            projected.default_value = Some(payload);
            fields.push(projected);
        }
        if self.textures().is_empty() {
            fields.push(required(
                "texture_overrides",
                ComponentValueShape::record_members([]),
            ));
        } else {
            for texture in self.textures() {
                let mut projected = ComponentFieldSchema::required(
                    projected_id("texture", texture.id()),
                    texture.id(),
                    ComponentFieldPath::from_fields(["texture_overrides", texture.id()]),
                    ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_capability(ComponentCapability::AssetRef);
                projected.required = false;
                fields.push(projected);
            }
        }
        Ok(fields)
    }

    /// Projects one declaration over the carrier for inspection and authoring metadata.
    ///
    /// This schema retains the carrier's identity. It is never registered beside
    /// `MATERIAL_SCHEMA_PROVIDER`; the provider owns the sole native carrier schema.
    /// Record decoding still requires the complete authored parameter map.
    pub fn binding_schema(
        &self,
        default_parameters: &MaterialParameterValues,
    ) -> Result<ComponentSchema, ComponentCodecError> {
        let fields = self.binding_fields(default_parameters)?;
        let mut schema = material2d_schema();
        schema
            .fields
            .retain(|field| !matches!(field.id.as_str(), "parameters" | "texture_overrides"));
        for field in &mut schema.fields {
            match field.id.as_str() {
                "declaration_id" => {
                    field.default_value = Some(ComponentValue::String(self.id().to_owned()));
                }
                "declaration_version" => {
                    field.default_value = Some(ComponentValue::U64(u64::from(self.version())));
                }
                _ => {}
            }
        }
        for kind in [
            MaterialParameterKind::F32,
            MaterialParameterKind::I32,
            MaterialParameterKind::U32,
            MaterialParameterKind::Vec2,
            MaterialParameterKind::Vec3,
            MaterialParameterKind::Vec4,
        ] {
            if !self
                .parameters()
                .fields()
                .iter()
                .any(|field| field.kind() == kind)
            {
                let (tag, _) = parameter_shape(kind);
                schema.fields.push(
                    ComponentFieldSchema::required(
                        ComponentFieldId::new(format!("material.empty.{tag}")),
                        tag,
                        ComponentFieldPath::from_fields(["parameters", tag]),
                        ComponentValueShape::record_members([]),
                    )
                    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect]),
                );
            }
        }
        schema.fields.extend(fields);
        Ok(schema)
    }
}

fn projected_id(kind: &str, semantic_id: &str) -> ComponentFieldId {
    ComponentFieldId::try_new(format!(
        "material.{kind}.{}",
        blake3::hash(semantic_id.as_bytes()).to_hex()
    ))
    .expect("fixed namespaces and a complete digest fit the persistent field identity")
}

fn required(id: &str, shape: impl Into<ComponentValueShape>) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::try_new(id).expect("fixed material carrier field IDs are valid"),
        id,
        ComponentFieldPath::from_fields([id]),
        shape,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
}

fn parameter_shape(kind: MaterialParameterKind) -> (&'static str, ComponentValueShape) {
    let float = ComponentValueShape::scalar(ComponentValueKind::F64)
        .with_numeric_domain(ComponentNumericDomain::F32);
    match kind {
        MaterialParameterKind::F32 => ("f32", float),
        MaterialParameterKind::I32 => (
            "i32",
            ComponentValueShape::scalar(ComponentValueKind::I64)
                .with_numeric_domain(ComponentNumericDomain::I32),
        ),
        MaterialParameterKind::U32 => (
            "u32",
            ComponentValueShape::scalar(ComponentValueKind::U64)
                .with_numeric_domain(ComponentNumericDomain::U32),
        ),
        MaterialParameterKind::Vec2 => ("vec2", ComponentValueShape::list(float)),
        MaterialParameterKind::Vec3 => ("vec3", ComponentValueShape::list(float)),
        MaterialParameterKind::Vec4 => ("vec4", ComponentValueShape::list(float)),
    }
}
