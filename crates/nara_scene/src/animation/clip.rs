use std::{borrow::Cow, f64::consts::FRAC_PI_2, mem, sync::Arc, time::Duration};

use nara_core::{Color, EntityUuid, UuidIdError, Vec2};
use nara_reflect::{
    ComponentFieldPath, ComponentFieldPathParseError, ComponentFieldPathSegment, ComponentTypeId,
    ComponentTypeIdError,
};
use thiserror::Error;

#[cfg(feature = "cooked")]
mod cooked;
#[cfg(feature = "runtime")]
mod loader;
#[cfg(feature = "authoring")]
mod source;

#[cfg(feature = "runtime")]
pub use loader::{PropertyAnimationLoader, PropertyAnimationLoaderError};

/// Maximum size in bytes of a `.anim.ron` source document.
pub const MAX_PROPERTY_ANIMATION_SOURCE_BYTES: usize = 8 * 1024 * 1024;

const MAX_TRACKS: usize = 1_024;
const MAX_KEYFRAMES: usize = 65_536;
const MAX_MARKERS: usize = 4_096;
const MAX_PATH_SEGMENTS: usize = 32;
const MAX_PATH_BYTES: usize = 1_024;
const MAX_MARKER_BYTES: usize = 256;
const MAX_STRING_BYTES: usize = 16 * 1024;
const MAX_TOTAL_STRING_BYTES: usize = 8 * 1024 * 1024;
const MAX_TIME: Duration = Duration::from_nanos(u64::MAX);

/// Maps normalized segment progress to an interpolation factor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "authoring", derive(serde::Deserialize))]
#[repr(u8)]
pub enum Easing {
    Step = 0,
    #[default]
    Linear = 1,
    QuadIn = 2,
    QuadOut = 3,
    QuadInOut = 4,
    CubicIn = 5,
    CubicOut = 6,
    CubicInOut = 7,
    SineIn = 8,
    SineOut = 9,
    SineInOut = 10,
}

impl Easing {
    /// Samples a curve over `0..=1`. Progress outside the interval is clamped and NaN maps to zero.
    #[must_use]
    pub fn sample(self, progress: f32) -> f32 {
        self.factor(f64::from(progress)) as f32
    }

    fn factor(self, progress: f64) -> f64 {
        if progress <= 0.0 || progress.is_nan() {
            return 0.0;
        }
        if progress >= 1.0 {
            return 1.0;
        }
        let factor = match self {
            Self::Step => 0.0,
            Self::Linear => progress,
            Self::QuadIn => progress * progress,
            Self::QuadOut => progress * (2.0 - progress),
            Self::QuadInOut if progress < 0.5 => 2.0 * progress * progress,
            Self::QuadInOut => 1.0 - 2.0 * (1.0 - progress).powi(2),
            Self::CubicIn => progress.powi(3),
            Self::CubicOut => progress * (progress * (progress - 3.0) + 3.0),
            Self::CubicInOut if progress < 0.5 => 4.0 * progress.powi(3),
            Self::CubicInOut => 1.0 - 4.0 * (1.0 - progress).powi(3),
            Self::SineIn => 2.0 * (0.5 * FRAC_PI_2 * progress).sin().powi(2),
            Self::SineOut => (FRAC_PI_2 * progress).sin(),
            Self::SineInOut => (FRAC_PI_2 * progress).sin().powi(2),
        };
        factor.min(1.0)
    }
}

/// A scalar or aggregate value that can be assigned to a component field.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "authoring", derive(serde::Deserialize))]
pub enum KeyframeValue {
    F32(f32),
    Vec2(Vec2),
    Color(#[cfg_attr(feature = "authoring", serde(with = "source::ColorSource"))] Color),
    Bool(bool),
    I64(i64),
    String(String),
}

impl KeyframeValue {
    /// Interpolates scalar, vector, and linear RGBA channels at normalized progress.
    /// Discrete or unmatched variants hold the start value until progress reaches one.
    /// NaN progress selects the start value.
    #[must_use]
    pub fn interpolate<'a>(&'a self, end: &'a Self, progress: f32) -> Cow<'a, Self> {
        if progress <= 0.0 || progress.is_nan() {
            return Cow::Borrowed(self);
        }
        if progress >= 1.0 {
            return Cow::Borrowed(end);
        }
        self.interpolate_between(end, f64::from(progress))
    }

    fn interpolate_between(&self, end: &Self, progress: f64) -> Cow<'_, Self> {
        let lerp = |start: f32, end: f32| {
            ((1.0 - progress) * f64::from(start) + progress * f64::from(end)) as f32
        };
        let value = match (self, end) {
            (Self::F32(start), Self::F32(end)) => Self::F32(lerp(*start, *end)),
            (Self::Vec2(start), Self::Vec2(end)) => {
                Self::Vec2(Vec2::new(lerp(start.x, end.x), lerp(start.y, end.y)))
            }
            (Self::Color(start), Self::Color(end)) => Self::Color(Color::rgba(
                lerp(start.r, end.r),
                lerp(start.g, end.g),
                lerp(start.b, end.b),
                lerp(start.a, end.a),
            )),
            _ => return Cow::Borrowed(self),
        };
        Cow::Owned(value)
    }

    pub(super) fn is_finite(&self) -> bool {
        match self {
            Self::F32(value) => value.is_finite(),
            Self::Vec2(value) => value.is_finite(),
            Self::Color(Color { r, g, b, a }) => [*r, *g, *b, *a].into_iter().all(f32::is_finite),
            Self::Bool(_) | Self::I64(_) | Self::String(_) => true,
        }
    }

    fn string_len(&self) -> usize {
        match self {
            Self::String(value) => value.len(),
            _ => 0,
        }
    }
}

/// A value at a clip-relative time. Easing applies to the segment starting at this keyframe.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyKeyframe {
    pub time: Duration,
    pub value: KeyframeValue,
    pub easing: Easing,
}

/// A homogeneous, nonempty sequence of keyframes for one component field.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyTrack {
    /// `None` targets the entity playing the clip.
    pub target_child: Option<EntityUuid>,
    pub component: ComponentTypeId,
    pub path: ComponentFieldPath,
    keyframes: Box<[PropertyKeyframe]>,
}

impl PropertyTrack {
    /// Validates the target and takes keyframes in strictly increasing time order.
    pub fn new(
        target_child: Option<EntityUuid>,
        component: ComponentTypeId,
        path: ComponentFieldPath,
        keyframes: Vec<PropertyKeyframe>,
    ) -> Result<Self, PropertyAnimationClipError> {
        check_limit("track keyframes", keyframes.len(), MAX_KEYFRAMES)?;
        let first = keyframes
            .first()
            .ok_or(PropertyAnimationClipError::EmptyTrack)?;
        for (index, keyframe) in keyframes.iter().enumerate() {
            if index > 0 && keyframes[index - 1].time >= keyframe.time {
                return Err(PropertyAnimationClipError::KeyframeOrder(index));
            }
            if keyframe.time > MAX_TIME {
                return Err(PropertyAnimationClipError::KeyframeOutsideDuration(index));
            }
            if mem::discriminant(&first.value) != mem::discriminant(&keyframe.value) {
                return Err(PropertyAnimationClipError::MixedValues(index));
            }
            if !keyframe.value.is_finite() {
                return Err(PropertyAnimationClipError::NonFiniteValue(index));
            }
            check_limit(
                "keyframe string bytes",
                keyframe.value.string_len(),
                MAX_STRING_BYTES,
            )?;
        }
        let track = Self {
            target_child,
            component,
            path,
            keyframes: keyframes.into_boxed_slice(),
        };
        track.validate_target()?;
        Ok(track)
    }

    #[must_use]
    pub fn keyframes(&self) -> &[PropertyKeyframe] {
        &self.keyframes
    }

    /// Samples at clip-relative time and holds the endpoint values outside the track's range.
    /// Exact keys, step segments, and discrete values return a borrowed value.
    #[must_use]
    pub fn sample(&self, time: Duration) -> Cow<'_, KeyframeValue> {
        let upper = self
            .keyframes
            .partition_point(|keyframe| keyframe.time <= time);
        let start = &self.keyframes[upper.saturating_sub(1)];
        let Some(end) = self.keyframes.get(upper) else {
            return Cow::Borrowed(&start.value);
        };
        if time <= start.time
            || start.easing == Easing::Step
            || matches!(
                start.value,
                KeyframeValue::Bool(_) | KeyframeValue::I64(_) | KeyframeValue::String(_)
            )
        {
            return Cow::Borrowed(&start.value);
        }
        let progress = (time - start.time).as_secs_f64() / (end.time - start.time).as_secs_f64();
        start
            .value
            .interpolate_between(&end.value, start.easing.factor(progress))
    }

    fn target(&self) -> (Option<EntityUuid>, &ComponentTypeId, &ComponentFieldPath) {
        (self.target_child, &self.component, &self.path)
    }

    fn validate_target(&self) -> Result<(), PropertyAnimationClipError> {
        check_limit(
            "component ID bytes",
            self.component.as_str().len(),
            ComponentTypeId::MAX_BYTES,
        )?;
        ComponentTypeId::try_new(self.component.as_str())?;
        let segments = self.path.segments();
        if !matches!(segments.first(), Some(ComponentFieldPathSegment::Field(_))) {
            return Err(PropertyAnimationClipError::InvalidPath);
        }
        check_limit("field path segments", segments.len(), MAX_PATH_SEGMENTS)?;
        for segment in segments {
            if let ComponentFieldPathSegment::Field(name) = segment {
                check_limit("field name bytes", name.len(), MAX_PATH_BYTES)?;
            }
        }
        check_limit(
            "field path bytes",
            self.path.to_string().len(),
            MAX_PATH_BYTES,
        )
    }
}

/// An event label at a clip-relative time. Simultaneous markers are ordered by name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PropertyMarker {
    pub time: Duration,
    pub name: String,
}

/// How a player advances a clip's local clock.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "authoring", derive(serde::Deserialize))]
#[repr(u8)]
pub enum PropertyPlaybackMode {
    #[default]
    Once = 0,
    Loop = 1,
    PingPong = 2,
}

/// Immutable property animation data. Clones share the validated tracks and marker timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyAnimationClip(Arc<ClipData>);

#[derive(Debug, PartialEq)]
struct ClipData {
    duration: Duration,
    mode: PropertyPlaybackMode,
    tracks: Box<[PropertyTrack]>,
    markers: Box<[PropertyMarker]>,
}

impl PropertyAnimationClip {
    /// Creates a clip with a positive duration up to `u64::MAX` nanoseconds.
    /// Tracks sort by child UUID, component ID and path. Markers sort by time and name.
    /// Track targets and `(time, name)` marker pairs must be unique.
    /// A clip can hold up to 1,024 tracks, 65,536 keyframes total, and 4,096 markers.
    pub fn new(
        duration: Duration,
        mode: PropertyPlaybackMode,
        mut tracks: Vec<PropertyTrack>,
        mut markers: Vec<PropertyMarker>,
    ) -> Result<Self, PropertyAnimationClipError> {
        if duration.is_zero() || duration > MAX_TIME {
            return Err(PropertyAnimationClipError::InvalidDuration);
        }
        check_limit("tracks", tracks.len(), MAX_TRACKS)?;
        check_limit("markers", markers.len(), MAX_MARKERS)?;
        let keyframe_count = tracks.iter().map(|track| track.keyframes.len()).sum();
        check_limit("clip keyframes", keyframe_count, MAX_KEYFRAMES)?;
        let string_bytes = tracks
            .iter()
            .flat_map(PropertyTrack::keyframes)
            .map(|keyframe| keyframe.value.string_len())
            .sum();
        check_limit(
            "clip keyframe string bytes",
            string_bytes,
            MAX_TOTAL_STRING_BYTES,
        )?;
        for track in &tracks {
            track.validate_target()?;
            if track
                .keyframes
                .last()
                .is_some_and(|keyframe| keyframe.time > duration)
            {
                return Err(PropertyAnimationClipError::KeyframeOutsideDuration(
                    track.keyframes.len() - 1,
                ));
            }
        }
        for marker in &markers {
            if marker.time > duration || marker.name.is_empty() {
                return Err(PropertyAnimationClipError::InvalidMarker);
            }
            check_limit("marker name bytes", marker.name.len(), MAX_MARKER_BYTES)?;
        }
        tracks.sort_unstable_by(|left, right| left.target().cmp(&right.target()));
        if tracks
            .windows(2)
            .any(|pair| pair[0].target() == pair[1].target())
        {
            return Err(PropertyAnimationClipError::DuplicateTrack);
        }
        markers.sort_unstable();
        if markers.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(PropertyAnimationClipError::DuplicateMarker);
        }
        Ok(Self(Arc::new(ClipData {
            duration,
            mode,
            tracks: tracks.into_boxed_slice(),
            markers: markers.into_boxed_slice(),
        })))
    }

    #[must_use]
    pub fn duration(&self) -> Duration {
        self.0.duration
    }

    #[must_use]
    pub fn mode(&self) -> PropertyPlaybackMode {
        self.0.mode
    }

    #[must_use]
    pub fn tracks(&self) -> &[PropertyTrack] {
        &self.0.tracks
    }

    #[must_use]
    pub fn markers(&self) -> &[PropertyMarker] {
        &self.0.markers
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PropertyAnimationClipError {
    #[error("property animation duration must be positive and fit in u64 nanoseconds")]
    InvalidDuration,
    #[error("property animation {what} ({actual}) exceeds the limit of {maximum}")]
    Limit {
        what: &'static str,
        actual: usize,
        maximum: usize,
    },
    #[error("property animation track must have a keyframe")]
    EmptyTrack,
    #[error("property keyframe at index {0} is out of time order")]
    KeyframeOrder(usize),
    #[error("property keyframe at index {0} exceeds the animation duration")]
    KeyframeOutsideDuration(usize),
    #[error("property keyframe at index {0} changes the track value type")]
    MixedValues(usize),
    #[error("property keyframe at index {0} contains a nonfinite value")]
    NonFiniteValue(usize),
    #[error("property animation field path must start with a field")]
    InvalidPath,
    #[error("property animation contains duplicate track targets")]
    DuplicateTrack,
    #[error("property animation marker must have a name and be within the clip duration")]
    InvalidMarker,
    #[error("property animation contains a duplicate marker at the same time")]
    DuplicateMarker,
    #[error(transparent)]
    ComponentId(#[from] ComponentTypeIdError),
    #[error(transparent)]
    FieldPath(#[from] ComponentFieldPathParseError),
    #[error(transparent)]
    TargetUuid(#[from] UuidIdError),
    #[cfg(feature = "authoring")]
    #[error("property animation source exceeds {MAX_PROPERTY_ANIMATION_SOURCE_BYTES} bytes")]
    SourceTooLarge,
    #[cfg(feature = "authoring")]
    #[error(transparent)]
    Ron(#[from] ron::error::SpannedError),
    #[cfg(feature = "cooked")]
    #[error("invalid NPAN property animation: {0}")]
    InvalidCooked(&'static str),
    #[cfg(feature = "cooked")]
    #[error("unsupported NPAN property animation version {0}")]
    UnsupportedVersion(u16),
}

fn check_limit(
    what: &'static str,
    actual: usize,
    maximum: usize,
) -> Result<(), PropertyAnimationClipError> {
    if actual > maximum {
        return Err(PropertyAnimationClipError::Limit {
            what,
            actual,
            maximum,
        });
    }
    Ok(())
}
