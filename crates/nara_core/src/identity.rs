//! Non-nil UUIDs for assets, entities, and documents.

use std::{error::Error, fmt, str::FromStr};

use uuid::Uuid;

macro_rules! uuid_identity {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub const fn try_from_uuid(uuid: Uuid) -> Result<Self, UuidIdError> {
                if uuid.is_nil() {
                    Err(UuidIdError::Nil)
                } else {
                    Ok(Self(uuid))
                }
            }

            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl FromStr for $name {
            type Err = UuidIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::try_from_uuid(Uuid::parse_str(value).map_err(UuidIdError::Invalid)?)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, formatter)
            }
        }
    };
}

uuid_identity!(
    AssetId,
    "Stable UUID of a source asset. Typed runtime handles have their own identity."
);
uuid_identity!(
    EntityUuid,
    "UUID of an entity, independent of its runtime incarnation."
);
uuid_identity!(
    DocumentId,
    "Stable UUID of a document across open and close cycles."
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UuidIdError {
    Invalid(uuid::Error),
    Nil,
}

impl fmt::Display for UuidIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(error) => error.fmt(formatter),
            Self::Nil => formatter.write_str("identity UUID must not be nil"),
        }
    }
}

impl Error for UuidIdError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Invalid(error) => Some(error),
            Self::Nil => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_retains_existing_uuid_values_and_rejects_nil() {
        let text = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
        let uuid = Uuid::parse_str(text).unwrap();
        assert_eq!(text.parse::<AssetId>().unwrap().as_uuid(), uuid);
        assert_eq!(EntityUuid::try_from_uuid(uuid).unwrap().to_string(), text);
        assert_eq!(DocumentId::try_from_uuid(uuid).unwrap().as_uuid(), uuid);
        assert_eq!(AssetId::try_from_uuid(Uuid::nil()), Err(UuidIdError::Nil));
        assert_eq!(
            EntityUuid::try_from_uuid(Uuid::nil()),
            Err(UuidIdError::Nil)
        );
        assert_eq!(
            DocumentId::try_from_uuid(Uuid::nil()),
            Err(UuidIdError::Nil)
        );
    }
}
