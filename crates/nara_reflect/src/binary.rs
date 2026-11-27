//! Typed Postcard adapters for the field types supported by Schema.

mod dynamic;

use std::collections::BTreeMap;

use nara_asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId};
use nara_core::{AssetId, Color, EntityUuid, Vec2, Vec3, Vec4};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use uuid::Uuid;

use crate::{Bytes, ComponentCodecError, PackedF32, PackedI32, PackedU16, PackedU32};

/// An identity reference stored in a typed Postcard component payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostcardReference {
    Asset(AssetId),
    Entity(EntityUuid),
}

#[doc(hidden)]
pub trait PostcardField: Sized {
    const HAS_REFERENCES: bool = false;

    fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error>;
    fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;

    fn visit_references(
        &self,
        _visit: &mut dyn FnMut(PostcardReference),
    ) -> Result<(), ComponentCodecError> {
        Ok(())
    }
}

struct WriteField<'a, T>(&'a T);

impl<T: PostcardField> Serialize for WriteField<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.write(serializer)
    }
}

#[repr(transparent)]
struct ReadField<T>(T);

impl<'de, T: PostcardField> Deserialize<'de> for ReadField<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::read(deserializer).map(Self)
    }
}

pub fn write_postcard_field<T: PostcardField>(
    bytes: &mut Vec<u8>,
    value: &T,
    field: &str,
) -> Result<(), ComponentCodecError> {
    postcard::to_io(&WriteField(value), bytes)
        .map(|_| ())
        .map_err(|error| binary_error(field, error))
}

pub fn read_postcard_field<T: PostcardField>(
    bytes: &mut &[u8],
    field: &str,
) -> Result<T, ComponentCodecError> {
    let (ReadField(value), remaining) =
        postcard::take_from_bytes(bytes).map_err(|error| binary_error(field, error))?;
    *bytes = remaining;
    Ok(value)
}

pub fn finish_postcard(remaining: &[u8]) -> Result<(), ComponentCodecError> {
    if remaining.is_empty() {
        Ok(())
    } else {
        Err(ComponentCodecError::Message(
            "unexpected trailing bytes in component Postcard payload".into(),
        ))
    }
}

pub fn visit_postcard_field_references<T: PostcardField>(
    value: &T,
    visit: &mut dyn FnMut(PostcardReference),
) -> Result<(), ComponentCodecError> {
    if T::HAS_REFERENCES {
        value.visit_references(visit)?;
    }
    Ok(())
}

fn binary_error(field: &str, error: postcard::Error) -> ComponentCodecError {
    ComponentCodecError::invalid_field(field, format!("a typed Postcard value ({error})"))
}

#[doc(hidden)]
pub trait PostcardChoice {
    fn choice(&self) -> Option<&str>;
}

impl PostcardChoice for String {
    fn choice(&self) -> Option<&str> {
        Some(self)
    }
}

impl PostcardChoice for Option<String> {
    fn choice(&self) -> Option<&str> {
        self.as_deref()
    }
}

pub fn check_postcard_choice(
    value: &impl PostcardChoice,
    field: &str,
    choices: &[&str],
) -> Result<(), ComponentCodecError> {
    if let Some(value) = value.choice()
        && !choices.contains(&value)
    {
        return Err(ComponentCodecError::invalid_field(
            field,
            "a declared string choice",
        ));
    }
    Ok(())
}

macro_rules! native_field {
    ($($native:ty),+) => {$(
        impl PostcardField for $native {
            fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.serialize(serializer)
            }

            fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::deserialize(deserializer)
            }
        }
    )+};
}

native_field!(bool, String, i8, i16, i32, i64, u8, u16, u32, u64);

macro_rules! finite_float {
    ($($native:ty),+) => {$(
        impl PostcardField for $native {
            fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                if !self.is_finite() {
                    return Err(serde::ser::Error::custom("expected a finite float"));
                }
                self.serialize(serializer)
            }

            fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = Self::deserialize(deserializer)?;
                if value.is_finite() {
                    Ok(value)
                } else {
                    Err(serde::de::Error::custom("expected a finite float"))
                }
            }
        }
    )+};
}

finite_float!(f32, f64);

macro_rules! math_field {
    ($native:ty, $new:ident, $($channel:ident),+) => {
        impl PostcardField for $native {
            fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                [$(WriteField(&self.$channel)),+].serialize(serializer)
            }

            fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let [$(ReadField($channel)),+] = Deserialize::deserialize(deserializer)?;
                Ok(Self::$new($($channel),+))
            }
        }
    };
}

math_field!(Vec2, new, x, y);
math_field!(Vec3, new, x, y, z);
math_field!(Vec4, new, x, y, z, w);
math_field!(Color, rgba, r, g, b, a);

macro_rules! uuid_field {
    ($($native:ty => $reference:ident),+) => {$(
        impl PostcardField for $native {
            const HAS_REFERENCES: bool = true;

            fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.as_uuid().as_bytes().serialize(serializer)
            }

            fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let bytes = <[u8; 16]>::deserialize(deserializer)?;
                Self::try_from_uuid(Uuid::from_bytes(bytes)).map_err(serde::de::Error::custom)
            }

            fn visit_references(
                &self,
                visit: &mut dyn FnMut(PostcardReference),
            ) -> Result<(), ComponentCodecError> {
                visit(PostcardReference::$reference(*self));
                Ok(())
            }
        }
    )+};
}

uuid_field!(AssetId => Asset, EntityUuid => Entity);

#[derive(Deserialize, Serialize)]
enum AssetLocator<'a> {
    Path(&'a str),
    StableId([u8; 16]),
    Product([u8; 16], Option<[u8; 16]>),
}

impl PostcardField for AssetRef {
    const HAS_REFERENCES: bool = true;

    fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let locator = match self {
            Self::Path(path) => AssetLocator::Path(path.as_str()),
            Self::StableId(id) => AssetLocator::StableId(*id.as_uuid().as_bytes()),
            Self::Product(reference) => AssetLocator::Product(
                *reference.source().as_uuid().as_bytes(),
                reference
                    .product()
                    .as_subproduct()
                    .map(|uuid| *uuid.as_bytes()),
            ),
        };
        locator.serialize(serializer)
    }

    fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match AssetLocator::deserialize(deserializer)? {
            AssetLocator::Path(path) => Self::path(path).map_err(serde::de::Error::custom),
            AssetLocator::StableId(bytes) => Ok(Self::StableId(StableAssetId::from_uuid(
                Uuid::from_bytes(bytes),
            ))),
            AssetLocator::Product(source, product) => {
                let product = match product {
                    Some(bytes) => ImportedProductId::subproduct(Uuid::from_bytes(bytes))
                        .map_err(serde::de::Error::custom)?,
                    None => ImportedProductId::PRIMARY,
                };
                AssetProductRef::new(StableAssetId::from_uuid(Uuid::from_bytes(source)), product)
                    .map(Self::product)
                    .map_err(serde::de::Error::custom)
            }
        }
    }

    fn visit_references(
        &self,
        visit: &mut dyn FnMut(PostcardReference),
    ) -> Result<(), ComponentCodecError> {
        if let Self::StableId(id) = self
            && let Ok(id) = AssetId::try_from(*id)
        {
            visit(PostcardReference::Asset(id));
            return Ok(());
        }
        Err(ComponentCodecError::invalid_field(
            "<asset-ref>",
            "a non-nil stable asset UUID",
        ))
    }
}

impl<T: PostcardField> PostcardField for Option<T> {
    const HAS_REFERENCES: bool = T::HAS_REFERENCES;

    fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_ref().map(WriteField).serialize(serializer)
    }

    fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Option::<ReadField<T>>::deserialize(deserializer)?.map(|value| value.0))
    }

    fn visit_references(
        &self,
        visit: &mut dyn FnMut(PostcardReference),
    ) -> Result<(), ComponentCodecError> {
        if let Some(value) = self {
            value.visit_references(visit)?;
        }
        Ok(())
    }
}

impl<T: PostcardField> PostcardField for Vec<T> {
    const HAS_REFERENCES: bool = T::HAS_REFERENCES;

    fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.len()))?;
        for value in self {
            sequence.serialize_element(&WriteField(value))?;
        }
        sequence.end()
    }

    fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<ReadField<T>>::deserialize(deserializer)?;
        Ok(values.into_iter().map(|value| value.0).collect())
    }

    fn visit_references(
        &self,
        visit: &mut dyn FnMut(PostcardReference),
    ) -> Result<(), ComponentCodecError> {
        for value in self {
            value.visit_references(visit)?;
        }
        Ok(())
    }
}

impl<T: PostcardField> PostcardField for BTreeMap<String, T> {
    const HAS_REFERENCES: bool = T::HAS_REFERENCES;

    fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (name, value) in self {
            map.serialize_entry(name, &WriteField(value))?;
        }
        map.end()
    }

    fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ReadMap<T>(std::marker::PhantomData<T>);

        impl<'de, T: PostcardField> serde::de::Visitor<'de> for ReadMap<T> {
            type Value = BTreeMap<String, T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a map with strictly ascending keys")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut source: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, ReadField(value))) =
                    source.next_entry::<String, ReadField<T>>()?
                {
                    if values
                        .last_key_value()
                        .is_some_and(|(previous, _)| previous >= &key)
                    {
                        return Err(serde::de::Error::custom("unordered or duplicate map key"));
                    }
                    values.insert(key, value);
                }
                Ok(values)
            }
        }

        deserializer.deserialize_map(ReadMap(std::marker::PhantomData))
    }

    fn visit_references(
        &self,
        visit: &mut dyn FnMut(PostcardReference),
    ) -> Result<(), ComponentCodecError> {
        for value in self.values() {
            value.visit_references(visit)?;
        }
        Ok(())
    }
}

macro_rules! packed_field {
    ($native:ty, $element:ty, $is_valid:expr) => {
        impl PostcardField for $native {
            fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                if !($is_valid)(self.0.as_slice()) {
                    return Err(serde::ser::Error::custom("expected finite packed floats"));
                }
                #[cfg(target_endian = "little")]
                return serializer.serialize_bytes(bytemuck::cast_slice(&self.0));
                #[cfg(target_endian = "big")]
                {
                    let bytes = self
                        .0
                        .iter()
                        .flat_map(|value| value.to_le_bytes())
                        .collect::<Vec<_>>();
                    serializer.serialize_bytes(&bytes)
                }
            }

            fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let bytes = <&[u8]>::deserialize(deserializer)?;
                const WIDTH: usize = std::mem::size_of::<$element>();
                let (chunks, remainder) = bytes.as_chunks::<WIDTH>();
                if !remainder.is_empty() {
                    return Err(serde::de::Error::custom("invalid packed block byte length"));
                }
                let values = chunks
                    .iter()
                    .copied()
                    .map(<$element>::from_le_bytes)
                    .collect::<Vec<_>>();
                if !($is_valid)(&values) {
                    return Err(serde::de::Error::custom("expected finite packed floats"));
                }
                Ok(Self(values))
            }
        }
    };
}

packed_field!(PackedU16, u16, |_| true);
packed_field!(PackedU32, u32, |_| true);
packed_field!(PackedI32, i32, |_| true);
packed_field!(PackedF32, f32, |values: &[f32]| values
    .iter()
    .all(|value| value.is_finite()));

impl PostcardField for Bytes {
    fn write<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }

    fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = <&[u8]>::deserialize(deserializer)?;
        Ok(Self(bytes.to_vec()))
    }
}
