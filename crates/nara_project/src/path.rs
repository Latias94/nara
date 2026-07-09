use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPath(String);

impl ProjectPath {
    pub fn new(path: impl Into<String>) -> Result<Self, ProjectPathError> {
        let path = path.into();
        validate_project_path(&path)?;
        Ok(Self(path))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ProjectPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPathError {
    Empty,
    Absolute,
    ContainsBackslash,
    ContainsDrivePrefix,
    ContainsNull,
    ContainsEmptySegment,
    ContainsCurrentDirectory,
    ContainsParentDirectory,
}

impl Display for ProjectPathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("project path cannot be empty"),
            Self::Absolute => formatter.write_str("project path must be relative"),
            Self::ContainsBackslash => formatter.write_str("project path must use '/' separators"),
            Self::ContainsDrivePrefix => {
                formatter.write_str("project path must not contain a drive prefix")
            }
            Self::ContainsNull => formatter.write_str("project path must not contain null bytes"),
            Self::ContainsEmptySegment => {
                formatter.write_str("project path must not contain empty segments")
            }
            Self::ContainsCurrentDirectory => {
                formatter.write_str("project path must not contain '.' segments")
            }
            Self::ContainsParentDirectory => {
                formatter.write_str("project path must not contain '..' segments")
            }
        }
    }
}

impl Error for ProjectPathError {}

fn validate_project_path(path: &str) -> Result<(), ProjectPathError> {
    if path.is_empty() {
        return Err(ProjectPathError::Empty);
    }
    if path.starts_with('/') {
        return Err(ProjectPathError::Absolute);
    }
    if path.contains('\\') {
        return Err(ProjectPathError::ContainsBackslash);
    }
    if path.as_bytes().get(1) == Some(&b':') {
        return Err(ProjectPathError::ContainsDrivePrefix);
    }
    if path.contains('\0') {
        return Err(ProjectPathError::ContainsNull);
    }

    for segment in path.split('/') {
        match segment {
            "" => return Err(ProjectPathError::ContainsEmptySegment),
            "." => return Err(ProjectPathError::ContainsCurrentDirectory),
            ".." => return Err(ProjectPathError::ContainsParentDirectory),
            _ => {}
        }
    }

    Ok(())
}
