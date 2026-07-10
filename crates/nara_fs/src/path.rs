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

impl RelativePath {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, PathValidationError> {
        let path = path.as_ref();
        validate_separator_shape(path.as_os_str())?;

        let mut components = Vec::new();
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

        if components.is_empty() {
            return Err(PathValidationError::Empty);
        }
        if components.len() > MAX_COMPONENTS
            || components
                .iter()
                .map(|component| unit_len(component.as_os_str()))
                .sum::<usize>()
                .saturating_add(components.len().saturating_sub(1))
                > MAX_PATH_UNITS
        {
            return Err(PathValidationError::PathTooLong);
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
    let units = os_units(value);
    if units.is_empty() {
        return Err(PathValidationError::EmptyComponent);
    }
    if units.len() > MAX_COMPONENT_UNITS {
        return Err(PathValidationError::ComponentTooLong);
    }
    if units.iter().copied().any(is_forbidden_unit)
        || units
            .last()
            .is_some_and(|unit| *unit == u32::from(b'.') || *unit == u32::from(b' '))
    {
        return Err(PathValidationError::ForbiddenCharacter);
    }

    let stem_end = units
        .iter()
        .position(|unit| *unit == u32::from(b'.'))
        .unwrap_or(units.len());
    let stem = &units[..stem_end];
    if is_reserved_device_stem(stem) {
        return Err(PathValidationError::ReservedDeviceName);
    }
    Ok(())
}

fn validate_separator_shape(value: &OsStr) -> Result<(), PathValidationError> {
    let units = os_units(value);
    if units.is_empty() {
        return Err(PathValidationError::Empty);
    }

    let is_separator = |unit: u32| unit == u32::from(b'/') || unit == u32::from(b'\\');
    if units.iter().any(|unit| *unit == u32::from(b'\\')) {
        return Err(PathValidationError::ForbiddenCharacter);
    }
    if units.first().copied().is_some_and(is_separator)
        || units.last().copied().is_some_and(is_separator)
        || units
            .windows(2)
            .any(|pair| is_separator(pair[0]) && is_separator(pair[1]))
    {
        return Err(PathValidationError::EmptyComponent);
    }
    Ok(())
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
    let folded = stem
        .iter()
        .copied()
        .map(|unit| {
            if (u32::from(b'a')..=u32::from(b'z')).contains(&unit) {
                unit - 32
            } else {
                unit
            }
        })
        .collect::<Vec<_>>();
    let ascii = |value: &[u8]| {
        folded.len() == value.len()
            && folded
                .iter()
                .zip(value)
                .all(|(left, right)| *left == u32::from(*right))
    };

    ascii(b"CON")
        || ascii(b"PRN")
        || ascii(b"AUX")
        || ascii(b"NUL")
        || (folded.len() == 4
            && (folded[..3] == [u32::from(b'C'), u32::from(b'O'), u32::from(b'M')]
                || folded[..3] == [u32::from(b'L'), u32::from(b'P'), u32::from(b'T')])
            && (u32::from(b'1')..=u32::from(b'9')).contains(&folded[3]))
}

fn unit_len(value: &OsStr) -> usize {
    os_units(value).len()
}

#[cfg(windows)]
fn os_units(value: &OsStr) -> Vec<u32> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().map(u32::from).collect()
}

#[cfg(unix)]
fn os_units(value: &OsStr) -> Vec<u32> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().iter().copied().map(u32::from).collect()
}

#[cfg(not(any(unix, windows)))]
fn os_units(value: &OsStr) -> Vec<u32> {
    value.to_string_lossy().chars().map(u32::from).collect()
}
