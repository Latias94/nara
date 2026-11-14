use std::{
    collections::BTreeMap,
    fmt,
    io::{self, Write},
    marker::PhantomData,
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape};
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeOwned, MapAccess, Visitor, value::MapAccessDeserializer},
};

use super::{MAX_SHADER_BYTES, MaterialAssetError, bounded_text};
use crate::{
    AlphaMode2d, Material2dDeclaration, MaterialParameterKind, MaterialParameterLayout,
    MaterialParameterValue, MaterialParameterValues, MaterialTextureDeclaration,
    MaterialUniformField,
};

#[derive(Serialize)]
#[serde(transparent)]
pub(super) struct Record<T>(pub T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Record<T> {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RecordVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for RecordVisitor<T> {
            type Value = Record<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a material JSON record")
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                T::deserialize(MapAccessDeserializer::new(map)).map(Record)
            }
        }

        deserializer.deserialize_map(RecordVisitor(PhantomData))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeclarationWire {
    id: String,
    version: u32,
    alpha: AlphaWire,
    uniform_size: u32,
    fields: Vec<Record<FieldWire>>,
    textures: Vec<Record<TextureWire>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldWire {
    id: String,
    name: String,
    kind: ParameterKindWire,
    offset: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextureWire {
    id: String,
    name: String,
    binding: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AlphaWire {
    Opaque,
    Blend,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ParameterKindWire {
    F32,
    I32,
    U32,
    Vec2,
    Vec3,
    Vec4,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ParameterValueWire {
    F32(f32),
    I32(i32),
    U32(u32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
}

pub(super) type ParametersWire = BTreeMap<String, ParameterValueWire>;

impl DeclarationWire {
    pub(super) fn from_declaration(declaration: &Material2dDeclaration) -> Self {
        Self {
            id: declaration.id().to_owned(),
            version: declaration.version(),
            alpha: match declaration.alpha_mode() {
                AlphaMode2d::Opaque => AlphaWire::Opaque,
                AlphaMode2d::Blend => AlphaWire::Blend,
            },
            uniform_size: declaration.parameters().byte_size(),
            fields: declaration
                .parameters()
                .fields()
                .iter()
                .map(|field| {
                    Record(FieldWire {
                        id: field.id().to_owned(),
                        name: field.name().to_owned(),
                        kind: match field.kind() {
                            MaterialParameterKind::F32 => ParameterKindWire::F32,
                            MaterialParameterKind::I32 => ParameterKindWire::I32,
                            MaterialParameterKind::U32 => ParameterKindWire::U32,
                            MaterialParameterKind::Vec2 => ParameterKindWire::Vec2,
                            MaterialParameterKind::Vec3 => ParameterKindWire::Vec3,
                            MaterialParameterKind::Vec4 => ParameterKindWire::Vec4,
                        },
                        offset: field.offset(),
                    })
                })
                .collect(),
            textures: declaration
                .textures()
                .iter()
                .map(|texture| {
                    Record(TextureWire {
                        id: texture.id().to_owned(),
                        name: texture.name().to_owned(),
                        binding: texture.binding(),
                    })
                })
                .collect(),
        }
    }

    pub(super) fn into_declaration(self) -> Result<Material2dDeclaration, MaterialAssetError> {
        bounded_text(&self.id)?;
        if self.fields.len() > crate::parameters::MAX_MATERIAL_PARAMETERS
            || self.textures.len() > crate::declaration::MAX_MATERIAL_TEXTURES
        {
            return Err(MaterialAssetError::Limit);
        }
        let fields = self
            .fields
            .into_iter()
            .map(|Record(field)| {
                bounded_text(&field.id)?;
                bounded_text(&field.name)?;
                let kind = match field.kind {
                    ParameterKindWire::F32 => MaterialParameterKind::F32,
                    ParameterKindWire::I32 => MaterialParameterKind::I32,
                    ParameterKindWire::U32 => MaterialParameterKind::U32,
                    ParameterKindWire::Vec2 => MaterialParameterKind::Vec2,
                    ParameterKindWire::Vec3 => MaterialParameterKind::Vec3,
                    ParameterKindWire::Vec4 => MaterialParameterKind::Vec4,
                };
                Ok(MaterialUniformField::new(
                    field.id,
                    field.name,
                    kind,
                    field.offset,
                )?)
            })
            .collect::<Result<Vec<_>, MaterialAssetError>>()?;
        let textures = self
            .textures
            .into_iter()
            .map(|Record(texture)| {
                bounded_text(&texture.id)?;
                bounded_text(&texture.name)?;
                Ok(MaterialTextureDeclaration::new(
                    texture.id,
                    texture.name,
                    texture.binding,
                )?)
            })
            .collect::<Result<Vec<_>, MaterialAssetError>>()?;
        Ok(Material2dDeclaration::new(
            self.id,
            self.version,
            match self.alpha {
                AlphaWire::Opaque => AlphaMode2d::Opaque,
                AlphaWire::Blend => AlphaMode2d::Blend,
            },
            MaterialParameterLayout::new(fields, self.uniform_size)?,
            textures,
        )?)
    }
}

pub(super) fn parameters_to_wire(parameters: &MaterialParameterValues) -> ParametersWire {
    parameters
        .iter()
        .map(|(id, value)| {
            let value = match *value {
                MaterialParameterValue::F32(value) => ParameterValueWire::F32(value),
                MaterialParameterValue::I32(value) => ParameterValueWire::I32(value),
                MaterialParameterValue::U32(value) => ParameterValueWire::U32(value),
                MaterialParameterValue::Vec2(value) => ParameterValueWire::Vec2(value),
                MaterialParameterValue::Vec3(value) => ParameterValueWire::Vec3(value),
                MaterialParameterValue::Vec4(value) => ParameterValueWire::Vec4(value),
            };
            (id.clone(), value)
        })
        .collect()
}

pub(super) fn parameters_from_wire(parameters: ParametersWire) -> MaterialParameterValues {
    parameters
        .into_iter()
        .map(|(id, value)| {
            let value = match value {
                ParameterValueWire::F32(value) => MaterialParameterValue::F32(value),
                ParameterValueWire::I32(value) => MaterialParameterValue::I32(value),
                ParameterValueWire::U32(value) => MaterialParameterValue::U32(value),
                ParameterValueWire::Vec2(value) => MaterialParameterValue::Vec2(value),
                ParameterValueWire::Vec3(value) => MaterialParameterValue::Vec3(value),
                ParameterValueWire::Vec4(value) => MaterialParameterValue::Vec4(value),
            };
            (id, value)
        })
        .collect()
}

pub(super) fn byte_limit(bytes: usize) -> ByteLimit {
    ByteLimit::new(bytes).expect("fixed nonzero material byte limit")
}

fn preflight(bytes: &[u8], runtime: bool) -> Result<(), MaterialAssetError> {
    let string_bytes = if runtime {
        MAX_SHADER_BYTES
    } else {
        super::MAX_PATH_BYTES
    };
    let total_strings = if runtime {
        MAX_SHADER_BYTES + super::MAX_SOURCE_FILE_BYTES
    } else {
        super::MAX_SOURCE_FILE_BYTES
    };
    let shape = SerdeShapeLimits::new(
        DepthLimit::new(12).expect("fixed nonzero material JSON depth"),
        ItemLimit::new(4096).expect("fixed nonzero material JSON node limit"),
        ItemLimit::new(64).expect("fixed nonzero material JSON container limit"),
        byte_limit(string_bytes),
        byte_limit(total_strings),
    );
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut parser, shape).map_err(|_| MaterialAssetError::Wire)?;
    parser.end().map_err(|_| MaterialAssetError::Wire)
}

pub(super) fn decode<T: DeserializeOwned>(
    bytes: &[u8],
    maximum: usize,
    runtime: bool,
) -> Result<T, MaterialAssetError> {
    if bytes.len() > maximum {
        return Err(MaterialAssetError::Limit);
    }
    preflight(bytes, runtime)?;
    serde_json::from_slice::<Record<T>>(bytes)
        .map(|Record(record)| record)
        .map_err(|_| MaterialAssetError::Wire)
}

pub(super) fn encode(
    value: &impl Serialize,
    maximum: usize,
    runtime: bool,
) -> Result<Box<[u8]>, MaterialAssetError> {
    struct Counter {
        bytes: usize,
        maximum: usize,
    }

    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(bytes.len())
                .filter(|bytes| *bytes <= self.maximum)
                .ok_or_else(|| io::Error::other("material JSON exceeds its encoded limit"))?;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let mut counter = Counter { bytes: 0, maximum };
    serde_json::to_writer(&mut counter, value).map_err(|_| MaterialAssetError::Limit)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(counter.bytes)
        .map_err(|_| MaterialAssetError::Limit)?;
    serde_json::to_writer(&mut bytes, value).map_err(|_| MaterialAssetError::Wire)?;
    preflight(&bytes, runtime)?;
    Ok(bytes.into_boxed_slice())
}
