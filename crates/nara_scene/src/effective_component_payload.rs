use std::{fmt, mem::size_of, sync::Arc};

use nara_reflect::{__private::FrozenComponentDefaults, ComponentValue, ComponentValueRef};

use crate::component_payload::ComponentPayload;

#[derive(Clone)]
pub(crate) enum EffectiveComponentPayload {
    Raw(ComponentPayload),
    Defaulted(Arc<DefaultedComponentPayload>),
}

pub(crate) struct DefaultedComponentPayload {
    source: ComponentPayload,
    defaults: FrozenComponentDefaults,
}

impl EffectiveComponentPayload {
    pub(crate) fn with_defaults(
        source: ComponentPayload,
        defaults: FrozenComponentDefaults,
    ) -> Self {
        Self::Defaulted(Arc::new(DefaultedComponentPayload { source, defaults }))
    }

    pub(crate) fn read(&self) -> ComponentValueRef<'_> {
        match self {
            Self::Raw(value) => ComponentValueRef::from(&**value),
            Self::Defaulted(value) => value.defaults.read(&value.source),
        }
    }

    pub(crate) fn raw(&self) -> Option<&ComponentPayload> {
        match self {
            Self::Raw(value) => Some(value),
            Self::Defaulted(_) => None,
        }
    }

    pub(crate) fn unique_raw_mut(&mut self) -> Option<&mut ComponentValue> {
        match self {
            Self::Raw(value) => value.get_mut(),
            Self::Defaulted(_) => None,
        }
    }

    pub(crate) fn materialize_mut(&mut self) -> &mut ComponentValue {
        if matches!(self, Self::Defaulted(_)) {
            *self = Self::from(self.read().to_owned());
        }
        match self {
            Self::Raw(value) => value.make_mut(),
            Self::Defaulted(_) => unreachable!(),
        }
    }

    pub(crate) fn into_owned(self) -> ComponentValue {
        match self {
            Self::Raw(value) => value.into_owned(),
            Self::Defaulted(value) => value.defaults.read(&value.source).to_owned(),
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        match self {
            Self::Raw(value) => value.retained_bytes(),
            Self::Defaulted(value) => {
                value
                    .source
                    .retained_bytes()
                    .saturating_add(Self::defaulted_storage_bytes(
                        value.defaults.retained_bytes(),
                    ))
            }
        }
    }

    pub(crate) fn defaulted_storage_bytes(defaults: usize) -> usize {
        size_of::<DefaultedComponentPayload>()
            .saturating_add(4 * size_of::<usize>())
            .saturating_add(defaults.saturating_sub(size_of::<FrozenComponentDefaults>()))
    }
}

impl From<ComponentValue> for EffectiveComponentPayload {
    fn from(value: ComponentValue) -> Self {
        Self::Raw(value.into())
    }
}

impl From<ComponentPayload> for EffectiveComponentPayload {
    fn from(value: ComponentPayload) -> Self {
        Self::Raw(value)
    }
}

impl fmt::Debug for EffectiveComponentPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.read(), formatter)
    }
}

impl PartialEq for EffectiveComponentPayload {
    fn eq(&self, other: &Self) -> bool {
        self.read() == other.read()
    }
}
