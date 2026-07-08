use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

#[cfg(feature = "serde")]
use crate::{PrefabDocument, SceneDocument};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneFormatError {
    Json(String),
    Ron(String),
}

impl Display for SceneFormatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "JSON scene format error: {error}"),
            Self::Ron(error) => write!(formatter, "RON scene format error: {error}"),
        }
    }
}

impl Error for SceneFormatError {}

#[cfg(feature = "serde")]
impl SceneDocument {
    pub fn to_json_string(&self) -> Result<String, SceneFormatError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| SceneFormatError::Json(error.to_string()))
    }

    pub fn from_json_str(input: &str) -> Result<Self, SceneFormatError> {
        let mut document = serde_json::from_str::<Self>(input)
            .map_err(|error| SceneFormatError::Json(error.to_string()))?;
        document.canonicalize();
        Ok(document)
    }

    pub fn to_ron_string(&self) -> Result<String, SceneFormatError> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|error| SceneFormatError::Ron(error.to_string()))
    }

    pub fn from_ron_str(input: &str) -> Result<Self, SceneFormatError> {
        let mut document = ron::from_str::<Self>(input)
            .map_err(|error| SceneFormatError::Ron(error.to_string()))?;
        document.canonicalize();
        Ok(document)
    }
}

#[cfg(feature = "serde")]
impl PrefabDocument {
    pub fn to_json_string(&self) -> Result<String, SceneFormatError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| SceneFormatError::Json(error.to_string()))
    }

    pub fn from_json_str(input: &str) -> Result<Self, SceneFormatError> {
        let mut document = serde_json::from_str::<Self>(input)
            .map_err(|error| SceneFormatError::Json(error.to_string()))?;
        document.canonicalize();
        Ok(document)
    }

    pub fn to_ron_string(&self) -> Result<String, SceneFormatError> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|error| SceneFormatError::Ron(error.to_string()))
    }

    pub fn from_ron_str(input: &str) -> Result<Self, SceneFormatError> {
        let mut document = ron::from_str::<Self>(input)
            .map_err(|error| SceneFormatError::Ron(error.to_string()))?;
        document.canonicalize();
        Ok(document)
    }
}
