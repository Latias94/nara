//! Stable typed material values and uniform encoding backed by encase.

use std::collections::{BTreeMap, BTreeSet};

use encase::ShaderType;
use encase::internal::{AlignmentValue, WriteInto};
use glam::{Vec2, Vec3, Vec4};

use crate::declaration::{MaterialDeclarationError, validate_id, validate_name};

/// The maximum number of named fields in a material uniform.
pub const MAX_MATERIAL_PARAMETERS: usize = 32;
/// The maximum encoded material uniform size in bytes.
pub const MAX_MATERIAL_UNIFORM_BYTES: u32 = 65_536;

/// The supported WGSL scalar and floating-point vector types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MaterialParameterKind {
    F32,
    I32,
    U32,
    Vec2,
    Vec3,
    Vec4,
}

impl MaterialParameterKind {
    fn metadata(self) -> (AlignmentValue, u64) {
        macro_rules! metadata {
            ($ty:ty) => {
                (
                    <$ty as ShaderType>::METADATA.alignment(),
                    <$ty as ShaderType>::min_size().get(),
                )
            };
        }
        match self {
            Self::F32 => metadata!(f32),
            Self::I32 => metadata!(i32),
            Self::U32 => metadata!(u32),
            Self::Vec2 => metadata!(Vec2),
            Self::Vec3 => metadata!(Vec3),
            Self::Vec4 => metadata!(Vec4),
        }
    }
}

/// An owned semantic value, independent of Rust struct layout and shader names.
///
/// Floating-point bits, including signed zero, are preserved when encoded. NaN
/// and infinities are rejected by value validation and every encoding path.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MaterialParameterValue {
    F32(f32),
    I32(i32),
    U32(u32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
}

impl MaterialParameterValue {
    /// Returns the semantic scalar/vector type.
    #[must_use]
    pub const fn kind(&self) -> MaterialParameterKind {
        match self {
            Self::F32(_) => MaterialParameterKind::F32,
            Self::I32(_) => MaterialParameterKind::I32,
            Self::U32(_) => MaterialParameterKind::U32,
            Self::Vec2(_) => MaterialParameterKind::Vec2,
            Self::Vec3(_) => MaterialParameterKind::Vec3,
            Self::Vec4(_) => MaterialParameterKind::Vec4,
        }
    }

    /// Rejects non-finite components and identifies their semantic field.
    pub fn validate(&self, id: &str) -> Result<(), MaterialDeclarationError> {
        let finite = match self {
            Self::F32(value) => value.is_finite(),
            Self::I32(_) | Self::U32(_) => true,
            Self::Vec2(value) => value.iter().all(|value| value.is_finite()),
            Self::Vec3(value) => value.iter().all(|value| value.is_finite()),
            Self::Vec4(value) => value.iter().all(|value| value.is_finite()),
        };
        if !finite {
            return Err(MaterialDeclarationError::NonFiniteParameter(id.to_owned()));
        }
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, MaterialDeclarationError> {
        match self {
            Self::F32(value) => encode_field(value),
            Self::I32(value) => encode_field(value),
            Self::U32(value) => encode_field(value),
            Self::Vec2(value) => encode_field(&Vec2::from_array(*value)),
            Self::Vec3(value) => encode_field(&Vec3::from_array(*value)),
            Self::Vec4(value) => encode_field(&Vec4::from_array(*value)),
        }
    }
}

/// Material parameters keyed exclusively by their stable semantic field IDs.
pub type MaterialParameterValues = BTreeMap<String, MaterialParameterValue>;

/// One semantic field mapped to an actual shader member and byte offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialUniformField {
    id: String,
    name: String,
    kind: MaterialParameterKind,
    offset: u32,
}

impl MaterialUniformField {
    /// Validates nonempty identity/name and the field type's real encase alignment.
    ///
    /// `name` is the actual Rust/WGSL field identifier and is independent of `id`.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: MaterialParameterKind,
        offset: u32,
    ) -> Result<Self, MaterialDeclarationError> {
        let id = id.into();
        let name = name.into();
        validate_id(&id, "parameter")?;
        validate_name(&name, "parameter")?;
        let (alignment, _) = kind.metadata();
        if !alignment.is_aligned(u64::from(offset)) {
            return Err(MaterialDeclarationError::MisalignedParameter {
                id,
                offset,
                alignment: alignment.get(),
            });
        }
        Ok(Self {
            id,
            name,
            kind,
            offset,
        })
    }

    /// Returns the stable semantic field ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the actual shader member name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the WGSL scalar/vector type.
    #[must_use]
    pub const fn kind(&self) -> MaterialParameterKind {
        self.kind
    }

    /// Returns the byte offset within the material uniform.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }
}

/// A bounded uniform layout whose field sizes and alignments come from encase.
///
/// Dynamic construction validates nonoverlapping ranges and bounded size. The
/// shader admission owner must still compare the complete layout to Naga's actual
/// uniform type, member names/types/offsets and minimum binding size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialParameterLayout {
    fields: Vec<MaterialUniformField>,
    byte_size: u32,
}

impl MaterialParameterLayout {
    /// Validates unique IDs/names, field bounds, nonoverlap and uniform size.
    pub fn new(
        fields: Vec<MaterialUniformField>,
        byte_size: u32,
    ) -> Result<Self, MaterialDeclarationError> {
        if fields.is_empty() || fields.len() > MAX_MATERIAL_PARAMETERS {
            return Err(MaterialDeclarationError::InvalidParameterCount {
                count: fields.len(),
                max: MAX_MATERIAL_PARAMETERS,
            });
        }
        checked_uniform_size(u64::from(byte_size))?;
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut max_alignment = fields[0].kind.metadata().0;
        for (index, field) in fields.iter().enumerate() {
            if !ids.insert(field.id()) {
                return Err(MaterialDeclarationError::DuplicateParameterId(
                    field.id.clone(),
                ));
            }
            if !names.insert(field.name()) {
                return Err(MaterialDeclarationError::DuplicateParameterName(
                    field.name.clone(),
                ));
            }
            let (alignment, size) = field.kind.metadata();
            if alignment.get() > max_alignment.get() {
                max_alignment = alignment;
            }
            let end = u64::from(field.offset) + size;
            if end > u64::from(byte_size) {
                return Err(MaterialDeclarationError::ParameterOutOfBounds {
                    id: field.id.clone(),
                    offset: field.offset,
                    size,
                    byte_size,
                });
            }
            for other in &fields[..index] {
                let other_end = u64::from(other.offset) + other.kind.metadata().1;
                if u64::from(field.offset) < other_end && u64::from(other.offset) < end {
                    return Err(MaterialDeclarationError::OverlappingParameters {
                        first: other.id.clone(),
                        second: field.id.clone(),
                    });
                }
            }
        }
        if !max_alignment.is_aligned(u64::from(byte_size)) {
            return Err(MaterialDeclarationError::MisalignedUniformSize {
                byte_size,
                alignment: max_alignment.get(),
            });
        }
        Ok(Self { fields, byte_size })
    }

    /// Returns the declared members in author-supplied order.
    #[must_use]
    pub fn fields(&self) -> &[MaterialUniformField] {
        &self.fields
    }

    /// Returns the complete minimum uniform binding size, including padding.
    #[must_use]
    pub const fn byte_size(&self) -> u32 {
        self.byte_size
    }

    /// Validates an exact semantic value set, including type and finite components.
    pub fn validate_values(
        &self,
        values: &MaterialParameterValues,
    ) -> Result<(), MaterialDeclarationError> {
        for field in &self.fields {
            let value = values
                .get(field.id())
                .ok_or_else(|| MaterialDeclarationError::MissingParameter(field.id.clone()))?;
            validate_kind(field.id(), field.kind, value)?;
        }
        for id in values.keys() {
            if !self.fields.iter().any(|field| field.id() == id) {
                return Err(MaterialDeclarationError::UnknownParameter(id.clone()));
            }
        }
        Ok(())
    }

    /// Encodes an exact semantic value set with deterministic zero padding.
    ///
    /// Each scalar/vector is encoded by encase and copied to its validated byte
    /// range. No Rust memory layout or hand-derived vector alignment is used.
    /// Encoding does not replace whole-layout Naga validation during admission.
    pub fn encode(
        &self,
        values: &MaterialParameterValues,
    ) -> Result<Vec<u8>, MaterialDeclarationError> {
        self.validate_values(values)?;
        let mut bytes = vec![0; self.byte_size as usize];
        for field in &self.fields {
            let value = values
                .get(field.id())
                .ok_or_else(|| MaterialDeclarationError::MissingParameter(field.id.clone()))?;
            let encoded = value.encode()?;
            let start = field.offset as usize;
            let end = start + encoded.len();
            let destination = bytes.get_mut(start..end).ok_or_else(|| {
                MaterialDeclarationError::ParameterOutOfBounds {
                    id: field.id.clone(),
                    offset: field.offset,
                    size: encoded.len() as u64,
                    byte_size: self.byte_size,
                }
            })?;
            destination.copy_from_slice(&encoded);
        }
        Ok(bytes)
    }
}

/// Typed semantic parameters and their actual encase uniform representation.
///
/// Derive this trait for a nonempty named struct with supported scalar fields or
/// `glam::Vec2`, `glam::Vec3`, `glam::Vec4`. Every field requires an explicit
/// `#[material(id = "stable-field-id")]`. The derive also implements encase's
/// `ShaderType`; a second `ShaderType` derive is unnecessary.
///
/// Renamed dependencies are discovered automatically. A container attribute
/// `#[material(crate = "my_nara::material")]` or
/// `#[material(crate = "my_material")]` selects the module exposing material
/// `__macro_support` explicitly.
pub trait MaterialParameters: Sized + ShaderType + WriteInto {
    /// Returns the complete layout from the actual encase struct metadata.
    fn parameter_layout() -> Result<MaterialParameterLayout, MaterialDeclarationError>;

    /// Projects all fields into validated semantic values.
    fn parameter_values(&self) -> Result<MaterialParameterValues, MaterialDeclarationError>;

    /// Reconstructs the struct from an exact, finite, correctly typed value set.
    fn from_parameter_values(
        values: &MaterialParameterValues,
    ) -> Result<Self, MaterialDeclarationError>;

    /// Encodes the whole struct through encase with deterministic zero padding.
    fn encode_uniform(&self) -> Result<Vec<u8>, MaterialDeclarationError> {
        let layout = Self::parameter_layout()?;
        layout.validate_values(&self.parameter_values()?)?;
        let mut buffer = encase::UniformBuffer::new(vec![0; layout.byte_size as usize]);
        buffer
            .write(self)
            .map_err(|error| MaterialDeclarationError::UniformEncoding(error.to_string()))?;
        Ok(buffer.into_inner())
    }
}

mod sealed {
    pub trait Sealed {}
}

/// The closed set of Rust scalar/vector field codecs used by the derive.
#[doc(hidden)]
pub trait MaterialParameterField: sealed::Sealed + ShaderType + WriteInto + Sized {
    const KIND: MaterialParameterKind;

    fn to_parameter_value(&self) -> MaterialParameterValue;

    fn from_parameter_value(
        id: &str,
        value: &MaterialParameterValue,
    ) -> Result<Self, MaterialDeclarationError>;
}

macro_rules! scalar_codec {
    ($ty:ty, $variant:ident) => {
        impl sealed::Sealed for $ty {}

        impl MaterialParameterField for $ty {
            const KIND: MaterialParameterKind = MaterialParameterKind::$variant;

            fn to_parameter_value(&self) -> MaterialParameterValue {
                MaterialParameterValue::$variant(*self)
            }

            fn from_parameter_value(
                id: &str,
                value: &MaterialParameterValue,
            ) -> Result<Self, MaterialDeclarationError> {
                validate_kind(id, Self::KIND, value)?;
                match value {
                    MaterialParameterValue::$variant(value) => Ok(*value),
                    _ => Err(kind_mismatch(id, Self::KIND, value.kind())),
                }
            }
        }
    };
}

macro_rules! vector_codec {
    ($ty:ty, $variant:ident) => {
        impl sealed::Sealed for $ty {}

        impl MaterialParameterField for $ty {
            const KIND: MaterialParameterKind = MaterialParameterKind::$variant;

            fn to_parameter_value(&self) -> MaterialParameterValue {
                MaterialParameterValue::$variant(self.to_array())
            }

            fn from_parameter_value(
                id: &str,
                value: &MaterialParameterValue,
            ) -> Result<Self, MaterialDeclarationError> {
                validate_kind(id, Self::KIND, value)?;
                match value {
                    MaterialParameterValue::$variant(value) => Ok(Self::from_array(*value)),
                    _ => Err(kind_mismatch(id, Self::KIND, value.kind())),
                }
            }
        }
    };
}

scalar_codec!(f32, F32);
scalar_codec!(i32, I32);
scalar_codec!(u32, U32);
vector_codec!(Vec2, Vec2);
vector_codec!(Vec3, Vec3);
vector_codec!(Vec4, Vec4);

/// Converts encase's actual fixed uniform size to the bounded public representation.
#[doc(hidden)]
pub fn material_uniform_size(size: u64) -> Result<u32, MaterialDeclarationError> {
    checked_uniform_size(size)
}

/// Converts encase's actual field offset without narrowing overflow.
#[doc(hidden)]
pub fn material_uniform_offset(offset: u64) -> Result<u32, MaterialDeclarationError> {
    u32::try_from(offset).map_err(|_| MaterialDeclarationError::UniformOffsetOverflow(offset))
}

/// Encodes a supported field as a finite semantic value.
#[doc(hidden)]
pub fn material_parameter_value<T: MaterialParameterField>(
    id: &str,
    value: &T,
) -> Result<MaterialParameterValue, MaterialDeclarationError> {
    let value = value.to_parameter_value();
    value.validate(id)?;
    Ok(value)
}

/// Looks up and decodes one field after complete value-set validation.
#[doc(hidden)]
pub fn material_parameter_from_values<T: MaterialParameterField>(
    id: &str,
    values: &MaterialParameterValues,
) -> Result<T, MaterialDeclarationError> {
    let value = values
        .get(id)
        .ok_or_else(|| MaterialDeclarationError::MissingParameter(id.to_owned()))?;
    T::from_parameter_value(id, value)
}

fn checked_uniform_size(size: u64) -> Result<u32, MaterialDeclarationError> {
    if size == 0 || size > u64::from(MAX_MATERIAL_UNIFORM_BYTES) {
        return Err(MaterialDeclarationError::InvalidUniformByteSize {
            size,
            max: MAX_MATERIAL_UNIFORM_BYTES,
        });
    }
    Ok(size as u32)
}

fn validate_kind(
    id: &str,
    expected: MaterialParameterKind,
    value: &MaterialParameterValue,
) -> Result<(), MaterialDeclarationError> {
    if value.kind() != expected {
        return Err(kind_mismatch(id, expected, value.kind()));
    }
    value.validate(id)
}

fn kind_mismatch(
    id: &str,
    expected: MaterialParameterKind,
    actual: MaterialParameterKind,
) -> MaterialDeclarationError {
    MaterialDeclarationError::ParameterKindMismatch {
        id: id.to_owned(),
        expected,
        actual,
    }
}

fn encode_field<T: MaterialParameterField>(value: &T) -> Result<Vec<u8>, MaterialDeclarationError> {
    let size = checked_uniform_size(T::min_size().get())?;
    let mut buffer = encase::UniformBuffer::new(vec![0; size as usize]);
    buffer
        .write(value)
        .map_err(|error| MaterialDeclarationError::UniformEncoding(error.to_string()))?;
    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests;
