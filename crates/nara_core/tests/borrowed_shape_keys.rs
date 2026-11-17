#![cfg(feature = "serde")]

use nara_core::{
    ByteLimit, DepthLimit, ItemLimit, SerdeShapeError, SerdeShapeLimits, SerdeShapePreflightError,
    preflight_serde_shape,
};
use serde::de::{
    DeserializeSeed, MapAccess,
    value::{
        BorrowedStrDeserializer, Error, MapAccessDeserializer, StrDeserializer, StringDeserializer,
        UnitDeserializer,
    },
};

enum Key<'de> {
    Borrowed(&'de str),
    Transient(&'de str),
    Owned(String),
}

struct KeyMap<'de> {
    keys: std::vec::IntoIter<Key<'de>>,
    scratch: [u8; 16],
}

impl<'de> MapAccess<'de> for KeyMap<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        let key = match self.keys.next() {
            Some(Key::Borrowed(key)) => seed.deserialize(BorrowedStrDeserializer::new(key)),
            Some(Key::Owned(key)) => seed.deserialize(StringDeserializer::new(key)),
            Some(Key::Transient(key)) => {
                self.scratch[..key.len()].copy_from_slice(key.as_bytes());
                let key = std::str::from_utf8(&self.scratch[..key.len()]).unwrap();
                seed.deserialize(StrDeserializer::new(key))
            }
            None => return Ok(None),
        };
        key.map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        seed.deserialize(UnitDeserializer::new())
    }
}

fn key_map(keys: Vec<Key<'_>>) -> MapAccessDeserializer<KeyMap<'_>> {
    MapAccessDeserializer::new(KeyMap {
        keys: keys.into_iter(),
        scratch: [0; 16],
    })
}

fn limits(items: usize, per_string: usize, total: usize) -> SerdeShapeLimits {
    SerdeShapeLimits::new(
        DepthLimit::new(8).unwrap(),
        ItemLimit::new(64).unwrap(),
        ItemLimit::new(items).unwrap(),
        ByteLimit::new(per_string).unwrap(),
        ByteLimit::new(total).unwrap(),
    )
}

fn preflight_json(
    input: &str,
    limits: SerdeShapeLimits,
) -> Result<(), SerdeShapePreflightError<serde_json::Error>> {
    preflight_serde_shape(&mut serde_json::Deserializer::from_str(input), limits)
}

#[test]
fn mixed_key_callbacks_preserve_keys_after_transient_scratch_reuse() {
    let borrowed = "café".to_owned();
    let transient = "alpha".to_owned();
    let keys = || {
        vec![
            Key::Transient(&transient),
            Key::Borrowed(&borrowed),
            Key::Owned("雪".to_owned()),
            Key::Transient("bravo"),
        ]
    };
    preflight_serde_shape(key_map(keys()), limits(4, 5, 18)).unwrap();

    for duplicate in [
        Key::Borrowed(&transient),
        Key::Owned(borrowed.clone()),
        Key::Transient("雪"),
    ] {
        let mut entries = keys();
        entries.push(duplicate);
        assert!(matches!(
            preflight_serde_shape(key_map(entries), limits(5, 5, 23)),
            Err(SerdeShapePreflightError::Shape(
                SerdeShapeError::DuplicateMapKey
            ))
        ));
    }

    let mut entries = keys();
    entries.push(Key::Borrowed(&transient));
    assert!(matches!(
        preflight_serde_shape(key_map(entries), limits(4, 5, 23)),
        Err(SerdeShapePreflightError::Shape(
            SerdeShapeError::ContainerItemLimitExceeded { maximum: 4 }
        ))
    ));
}

#[test]
fn decoded_unicode_keys_compare_equally_across_borrowed_and_escaped_strings() {
    for input in [
        r#"{"café":0,"caf\u00e9":1}"#,
        r#"{"caf\u00e9":0,"café":1}"#,
        r#"{"\u96ea":0,"雪":1}"#,
        r#"{"\ud83c\udf32":0,"🌲":1}"#,
    ] {
        assert!(matches!(
            preflight_json(input, limits(2, 5, 10)),
            Err(SerdeShapePreflightError::Shape(
                SerdeShapeError::DuplicateMapKey
            ))
        ));
    }
    preflight_json(r#"{"é":0,"e\u0301":1}"#, limits(2, 3, 5)).unwrap();
}

#[test]
fn nested_key_scopes_share_string_budget_before_duplicate_rejection() {
    let input = r#"{"é":{"é":"雪"},"b":[{"é":0},{"\u00e9":1}]}"#;
    preflight_json(input, limits(2, 3, 12)).unwrap();
    assert!(matches!(
        preflight_json(input, limits(2, 3, 11)),
        Err(SerdeShapePreflightError::Shape(
            SerdeShapeError::TotalStringLimitExceeded { maximum: 11 }
        ))
    ));

    let duplicate = r#"{"é":{"é":"雪"},"b":[{"é":0},{"\u00e9":1,"é":2}]}"#;
    assert!(matches!(
        preflight_json(duplicate, limits(2, 3, 13)),
        Err(SerdeShapePreflightError::Shape(
            SerdeShapeError::TotalStringLimitExceeded { maximum: 13 }
        ))
    ));
    assert!(matches!(
        preflight_json(duplicate, limits(2, 3, 14)),
        Err(SerdeShapePreflightError::Shape(
            SerdeShapeError::DuplicateMapKey
        ))
    ));
}
