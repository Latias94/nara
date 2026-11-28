use std::{fmt, marker::PhantomData, time::Duration};

use nara_core::Color;
use nara_reflect::ComponentTypeId;
use serde::{
    Deserialize, Deserializer,
    de::{Error, IgnoredAny, SeqAccess, Visitor},
};

use super::{
    Easing, KeyframeValue, MAX_KEYFRAMES, MAX_MARKERS, MAX_PATH_BYTES,
    MAX_PROPERTY_ANIMATION_SOURCE_BYTES, MAX_TRACKS, PropertyAnimationClip,
    PropertyAnimationClipError, PropertyKeyframe, PropertyMarker, PropertyPlaybackMode,
    PropertyTrack, check_limit,
};

#[derive(Deserialize)]
#[serde(rename = "PropertyAnimationClip", deny_unknown_fields)]
struct ClipSource {
    duration_ns: u64,
    #[serde(default)]
    mode: PropertyPlaybackMode,
    #[serde(deserialize_with = "bounded_list::<_, _, MAX_TRACKS>")]
    tracks: Vec<TrackSource>,
    #[serde(default, deserialize_with = "bounded_list::<_, _, MAX_MARKERS>")]
    markers: Vec<MarkerSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackSource {
    #[serde(default)]
    target_child: Option<String>,
    component: ComponentTypeId,
    path: String,
    #[serde(deserialize_with = "bounded_list::<_, _, MAX_KEYFRAMES>")]
    keyframes: Vec<KeyframeSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyframeSource {
    time_ns: u64,
    value: KeyframeValue,
    #[serde(default)]
    easing: Easing,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerSource {
    time_ns: u64,
    name: String,
}

#[derive(Deserialize)]
#[serde(remote = "Color", deny_unknown_fields)]
pub(super) struct ColorSource {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl PropertyAnimationClip {
    /// Parses a bounded `.anim.ron` document. Times are integer nanoseconds relative to the clip.
    /// Tracks use quoted component IDs and relative field paths. An optional `target_child`
    /// selects a child by its quoted UUID. Playback defaults to `Once` and easing to `Linear`.
    ///
    /// ```ron
    /// PropertyAnimationClip(
    ///     duration_ns: 1_000_000_000,
    ///     tracks: [(
    ///         component: "nara.Transform2d",
    ///         path: "z",
    ///         keyframes: [(time_ns: 0, value: F32(0.0))],
    ///     )],
    ///     markers: [(time_ns: 1_000_000_000, name: "finished")],
    /// )
    /// ```
    pub fn from_ron(source: &str) -> Result<Self, PropertyAnimationClipError> {
        validate_source_len(source.len())?;
        let source: ClipSource = ron::from_str(source)?;
        let tracks = source
            .tracks
            .into_iter()
            .map(|track| {
                check_limit("field path bytes", track.path.len(), MAX_PATH_BYTES)?;
                PropertyTrack::new(
                    track.target_child.as_deref().map(str::parse).transpose()?,
                    track.component,
                    track.path.parse()?,
                    track
                        .keyframes
                        .into_iter()
                        .map(|keyframe| PropertyKeyframe {
                            time: Duration::from_nanos(keyframe.time_ns),
                            value: keyframe.value,
                            easing: keyframe.easing,
                        })
                        .collect(),
                )
            })
            .collect::<Result<_, _>>()?;
        let markers = source
            .markers
            .into_iter()
            .map(|marker| PropertyMarker {
                time: Duration::from_nanos(marker.time_ns),
                name: marker.name,
            })
            .collect();
        Self::new(
            Duration::from_nanos(source.duration_ns),
            source.mode,
            tracks,
            markers,
        )
    }
}

pub(super) fn validate_source_len(len: usize) -> Result<(), PropertyAnimationClipError> {
    if len > MAX_PROPERTY_ANIMATION_SOURCE_BYTES {
        return Err(PropertyAnimationClipError::SourceTooLarge);
    }
    Ok(())
}

fn bounded_list<'de, D, T, const MAX: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct BoundedList<T, const MAX: usize>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>, const MAX: usize> Visitor<'de> for BoundedList<T, MAX> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {MAX} animation records")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Vec<T>, A::Error> {
            let mut values = Vec::new();
            while values.len() < MAX {
                let Some(value) = sequence.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if sequence.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::invalid_length(MAX + 1, &self));
            }
            Ok(values)
        }
    }

    deserializer.deserialize_seq(BoundedList::<T, MAX>(PhantomData))
}
