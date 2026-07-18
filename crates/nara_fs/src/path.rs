use std::{
    ffi::{OsStr, OsString},
    fmt::{self, Debug, Formatter},
    path::{Component, Path},
};

use crate::PathValidationError;

const MAX_COMPONENTS: usize = 256;
const MAX_COMPONENT_UNITS: usize = 255;
const MAX_PATH_UNITS: usize = 32_767;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RelativeComponent(OsString);

impl RelativeComponent {
    pub fn new(value: impl AsRef<OsStr>) -> Result<Self, PathValidationError> {
        let value = value.as_ref();
        validate_component(value)?;
        Ok(Self(value.to_os_string()))
    }

    pub(crate) fn as_os_str(&self) -> &OsStr {
        &self.0
    }
}

impl Debug for RelativeComponent {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("RelativeComponent(<validated>)")
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RelativePath {
    components: Vec<RelativeComponent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelativePathPreflight {
    components: usize,
    path_units: usize,
}

impl RelativePathPreflight {
    #[must_use]
    pub const fn components(self) -> usize {
        self.components
    }

    #[must_use]
    pub const fn path_units(self) -> usize {
        self.path_units
    }
}

impl RelativePath {
    pub fn preflight(path: impl AsRef<Path>) -> Result<RelativePathPreflight, PathValidationError> {
        preflight_relative_path(path.as_ref())
    }

    pub fn new(path: impl AsRef<Path>) -> Result<Self, PathValidationError> {
        let path = path.as_ref();
        let preflight = Self::preflight(path)?;

        let mut components = Vec::with_capacity(preflight.components());
        for component in path.components() {
            match component {
                Component::Normal(value) => components.push(RelativeComponent::new(value)?),
                Component::CurDir => return Err(PathValidationError::CurrentDirectory),
                Component::ParentDir => return Err(PathValidationError::ParentTraversal),
                Component::RootDir | Component::Prefix(_) => {
                    return Err(PathValidationError::AbsoluteOrPrefixed);
                }
            }
        }

        Ok(Self { components })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.components.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub(crate) fn components(&self) -> &[RelativeComponent] {
        &self.components
    }
}

impl Debug for RelativePath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelativePath")
            .field("components", &self.components.len())
            .finish()
    }
}

fn validate_component(value: &OsStr) -> Result<(), PathValidationError> {
    let mut unit_count = 0_usize;
    let mut last = None;
    let mut stem = [0_u32; 4];
    let mut stem_len = 0_usize;
    let mut before_extension = true;

    for unit in os_units(value) {
        unit_count = unit_count
            .checked_add(1)
            .ok_or(PathValidationError::ComponentTooLong)?;
        if unit_count > MAX_COMPONENT_UNITS {
            return Err(PathValidationError::ComponentTooLong);
        }
        if is_forbidden_unit(unit) {
            return Err(PathValidationError::ForbiddenCharacter);
        }
        if before_extension {
            if unit == u32::from(b'.') {
                before_extension = false;
            } else {
                if stem_len < stem.len() {
                    stem[stem_len] = unit;
                }
                stem_len = stem_len
                    .checked_add(1)
                    .ok_or(PathValidationError::ComponentTooLong)?;
            }
        }
        last = Some(unit);
    }

    if unit_count == 0 {
        return Err(PathValidationError::EmptyComponent);
    }
    if last.is_some_and(|unit| unit == u32::from(b'.') || unit == u32::from(b' ')) {
        return Err(PathValidationError::ForbiddenCharacter);
    }
    if stem_len <= stem.len() && is_reserved_device_stem(&stem[..stem_len]) {
        return Err(PathValidationError::ReservedDeviceName);
    }
    Ok(())
}

fn validate_separator_shape(value: &OsStr) -> Result<(), PathValidationError> {
    let is_separator = |unit: u32| unit == u32::from(b'/') || unit == u32::from(b'\\');
    let mut unit_count = 0_usize;
    let mut previous_was_separator = false;
    for unit in os_units(value) {
        if unit == u32::from(b'\\') {
            return Err(PathValidationError::ForbiddenCharacter);
        }
        let separator = is_separator(unit);
        if (unit_count == 0 && separator) || (previous_was_separator && separator) {
            return Err(PathValidationError::EmptyComponent);
        }
        previous_was_separator = separator;
        unit_count = unit_count
            .checked_add(1)
            .ok_or(PathValidationError::PathTooLong)?;
    }
    if unit_count == 0 {
        return Err(PathValidationError::Empty);
    }
    if previous_was_separator {
        return Err(PathValidationError::EmptyComponent);
    }
    Ok(())
}

fn preflight_relative_path(path: &Path) -> Result<RelativePathPreflight, PathValidationError> {
    validate_separator_shape(path.as_os_str())?;

    let mut components = 0_usize;
    let mut path_units = 0_usize;
    for component in path.components() {
        let Component::Normal(value) = component else {
            return Err(match component {
                Component::CurDir => PathValidationError::CurrentDirectory,
                Component::ParentDir => PathValidationError::ParentTraversal,
                Component::RootDir | Component::Prefix(_) => {
                    PathValidationError::AbsoluteOrPrefixed
                }
                Component::Normal(_) => unreachable!(),
            });
        };
        validate_component(value)?;
        components = components
            .checked_add(1)
            .ok_or(PathValidationError::PathTooLong)?;
        if components > MAX_COMPONENTS {
            return Err(PathValidationError::PathTooLong);
        }
        path_units = path_units
            .checked_add(usize::from(components > 1))
            .and_then(|total| total.checked_add(unit_len(value)))
            .ok_or(PathValidationError::PathTooLong)?;
        if path_units > MAX_PATH_UNITS {
            return Err(PathValidationError::PathTooLong);
        }
    }
    if components == 0 {
        return Err(PathValidationError::Empty);
    }

    Ok(RelativePathPreflight {
        components,
        path_units,
    })
}

fn is_forbidden_unit(unit: u32) -> bool {
    unit <= 31
        || matches!(
            unit,
            x if x == u32::from(b'<')
                || x == u32::from(b'>')
                || x == u32::from(b':')
                || x == u32::from(b'"')
                || x == u32::from(b'/')
                || x == u32::from(b'\\')
                || x == u32::from(b'|')
                || x == u32::from(b'?')
                || x == u32::from(b'*')
        )
}

fn is_reserved_device_stem(stem: &[u32]) -> bool {
    let fold = |unit| {
        if (u32::from(b'a')..=u32::from(b'z')).contains(&unit) {
            unit - 32
        } else {
            unit
        }
    };
    let ascii = |value: &[u8]| {
        stem.len() == value.len()
            && stem
                .iter()
                .zip(value)
                .all(|(left, right)| fold(*left) == u32::from(*right))
    };

    ascii(b"CON")
        || ascii(b"PRN")
        || ascii(b"AUX")
        || ascii(b"NUL")
        || (stem.len() == 4
            && ((fold(stem[0]) == u32::from(b'C')
                && fold(stem[1]) == u32::from(b'O')
                && fold(stem[2]) == u32::from(b'M'))
                || (fold(stem[0]) == u32::from(b'L')
                    && fold(stem[1]) == u32::from(b'P')
                    && fold(stem[2]) == u32::from(b'T')))
            && (u32::from(b'1')..=u32::from(b'9')).contains(&stem[3]))
}

fn unit_len(value: &OsStr) -> usize {
    os_units(value).count()
}

#[cfg(windows)]
fn os_units(value: &OsStr) -> impl Iterator<Item = u32> + '_ {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().map(u32::from)
}

#[cfg(unix)]
fn os_units(value: &OsStr) -> impl Iterator<Item = u32> + '_ {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().iter().copied().map(u32::from)
}

#[cfg(not(any(unix, windows)))]
fn os_units(value: &OsStr) -> impl Iterator<Item = u32> + '_ {
    value.as_encoded_bytes().iter().copied().map(u32::from)
}
