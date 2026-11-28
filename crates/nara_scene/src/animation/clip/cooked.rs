use std::{str, time::Duration};

use nara_core::{Color, EntityUuid, Vec2};
use nara_reflect::{ComponentFieldPath, ComponentTypeId};
use uuid::Uuid;

use super::{
    Easing, KeyframeValue, MAX_KEYFRAMES, MAX_MARKER_BYTES, MAX_MARKERS, MAX_PATH_BYTES,
    MAX_STRING_BYTES, MAX_TRACKS, PropertyAnimationClip, PropertyAnimationClipError,
    PropertyKeyframe, PropertyMarker, PropertyPlaybackMode, PropertyTrack, check_limit,
};

const MAGIC: [u8; 4] = *b"NPAN";
const VERSION: u16 = 1;
const MAX_COOKED_BYTES: usize = 16 * 1024 * 1024;
const KEYFRAME_HEADER_BYTES: usize = 9;
const MIN_TRACK_BYTES: usize = 10 + KEYFRAME_HEADER_BYTES + 1;
const MIN_MARKER_BYTES: usize = 11;

use PropertyAnimationClipError::InvalidCooked;

impl PropertyAnimationClip {
    /// Encodes deterministic NPAN v1. All numbers are little-endian and all times are `u64`
    /// nanoseconds. Text is a `u16` byte length followed by UTF-8.
    ///
    /// The header stores `NPAN`, a `u16` version, a mode tag (`Once`, `Loop`, `PingPong`), a
    /// reserved zero byte, the duration, and `u16` track and marker counts. Each track stores a
    /// target tag (0 for self or 1 followed by 16 UUID bytes), the component ID and canonical
    /// relative path as text, one value tag (`F32`, `Vec2`, `Color`, `Bool`, `I64`, `String`),
    /// and a `u32` keyframe count. A keyframe contains a time, an easing tag in `Easing`
    /// declaration order, and a typed value. Vectors store XY and colors store linear RGBA.
    /// Booleans store a single 0 or 1 byte. Markers store a time and a name as text. Enum tags
    /// are zero-based `u8` values in the listed order.
    pub fn encode_cooked(&self) -> Result<Vec<u8>, PropertyAnimationClipError> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&[self.mode() as u8, 0]);
        write_time(&mut bytes, self.duration());
        bytes.extend_from_slice(&(self.tracks().len() as u16).to_le_bytes());
        bytes.extend_from_slice(&(self.markers().len() as u16).to_le_bytes());
        for track in self.tracks() {
            bytes.push(u8::from(track.target_child.is_some()));
            if let Some(child) = track.target_child {
                bytes.extend_from_slice(child.as_uuid().as_bytes());
            }
            write_text(&mut bytes, track.component.as_str());
            write_text(&mut bytes, &track.path.to_string());
            let keyframes = track.keyframes();
            bytes.push(ValueKind::of(&keyframes[0].value) as u8);
            bytes.extend_from_slice(&(keyframes.len() as u32).to_le_bytes());
            for keyframe in keyframes {
                write_time(&mut bytes, keyframe.time);
                bytes.push(keyframe.easing as u8);
                write_value(&mut bytes, &keyframe.value);
            }
        }
        for marker in self.markers() {
            write_time(&mut bytes, marker.time);
            write_text(&mut bytes, &marker.name);
        }
        check_limit("cooked bytes", bytes.len(), MAX_COOKED_BYTES)?;
        Ok(bytes)
    }

    /// Reads exactly one validated NPAN v1 clip in canonical track and marker order.
    pub fn from_cooked(bytes: &[u8]) -> Result<Self, PropertyAnimationClipError> {
        check_limit("cooked bytes", bytes.len(), MAX_COOKED_BYTES)?;
        let mut reader = Reader(bytes);
        if reader.array::<4>()? != MAGIC {
            return Err(InvalidCooked("incorrect magic"));
        }
        let version = u16::from_le_bytes(reader.array()?);
        if version != VERSION {
            return Err(PropertyAnimationClipError::UnsupportedVersion(version));
        }
        let mode = read_tag(&mut reader, &MODES, "unknown playback mode")?;
        if reader.byte()? != 0 {
            return Err(InvalidCooked("nonzero reserved byte"));
        }
        let duration = reader.time()?;
        let track_count = u16::from_le_bytes(reader.array()?) as usize;
        let marker_count = u16::from_le_bytes(reader.array()?) as usize;
        check_limit("tracks", track_count, MAX_TRACKS)?;
        check_limit("markers", marker_count, MAX_MARKERS)?;
        reader.expect_bytes(track_count * MIN_TRACK_BYTES + marker_count * MIN_MARKER_BYTES)?;
        let mut tracks: Vec<PropertyTrack> = Vec::with_capacity(track_count);
        let mut total_keyframes = 0;
        for _ in 0..track_count {
            let track = read_track(&mut reader, &mut total_keyframes)?;
            if tracks
                .last()
                .is_some_and(|previous| previous.target() >= track.target())
            {
                return Err(InvalidCooked("tracks are out of canonical order"));
            }
            tracks.push(track);
        }
        let mut markers: Vec<PropertyMarker> = Vec::with_capacity(marker_count);
        for _ in 0..marker_count {
            let marker = PropertyMarker {
                time: reader.time()?,
                name: reader
                    .text("marker name bytes", MAX_MARKER_BYTES)?
                    .to_owned(),
            };
            if markers.last().is_some_and(|previous| previous >= &marker) {
                return Err(InvalidCooked("markers are out of canonical order"));
            }
            markers.push(marker);
        }
        if !reader.0.is_empty() {
            return Err(InvalidCooked("unexpected trailing bytes"));
        }
        Self::new(duration, mode, tracks, markers)
    }
}

const MODES: [PropertyPlaybackMode; 3] = [
    PropertyPlaybackMode::Once,
    PropertyPlaybackMode::Loop,
    PropertyPlaybackMode::PingPong,
];

const EASINGS: [Easing; 11] = [
    Easing::Step,
    Easing::Linear,
    Easing::QuadIn,
    Easing::QuadOut,
    Easing::QuadInOut,
    Easing::CubicIn,
    Easing::CubicOut,
    Easing::CubicInOut,
    Easing::SineIn,
    Easing::SineOut,
    Easing::SineInOut,
];

#[derive(Clone, Copy)]
#[repr(u8)]
enum ValueKind {
    F32 = 0,
    Vec2 = 1,
    Color = 2,
    Bool = 3,
    I64 = 4,
    String = 5,
}

impl ValueKind {
    const TAGS: [Self; 6] = [
        Self::F32,
        Self::Vec2,
        Self::Color,
        Self::Bool,
        Self::I64,
        Self::String,
    ];

    fn of(value: &KeyframeValue) -> Self {
        match value {
            KeyframeValue::F32(_) => Self::F32,
            KeyframeValue::Vec2(_) => Self::Vec2,
            KeyframeValue::Color(_) => Self::Color,
            KeyframeValue::Bool(_) => Self::Bool,
            KeyframeValue::I64(_) => Self::I64,
            KeyframeValue::String(_) => Self::String,
        }
    }

    fn minimum_bytes(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::Vec2 | Self::I64 => 8,
            Self::Color => 16,
            Self::Bool => 1,
            Self::String => 2,
        }
    }

    fn read(self, reader: &mut Reader<'_>) -> Result<KeyframeValue, PropertyAnimationClipError> {
        Ok(match self {
            Self::F32 => KeyframeValue::F32(reader.float()?),
            Self::Vec2 => KeyframeValue::Vec2(Vec2::new(reader.float()?, reader.float()?)),
            Self::Color => KeyframeValue::Color(Color::rgba(
                reader.float()?,
                reader.float()?,
                reader.float()?,
                reader.float()?,
            )),
            Self::Bool => KeyframeValue::Bool(match reader.byte()? {
                0 => false,
                1 => true,
                _ => return Err(InvalidCooked("invalid boolean")),
            }),
            Self::I64 => KeyframeValue::I64(i64::from_le_bytes(reader.array()?)),
            Self::String => KeyframeValue::String(
                reader
                    .text("keyframe string bytes", MAX_STRING_BYTES)?
                    .to_owned(),
            ),
        })
    }
}

fn read_track(
    reader: &mut Reader<'_>,
    total_keyframes: &mut usize,
) -> Result<PropertyTrack, PropertyAnimationClipError> {
    let target_child = match reader.byte()? {
        0 => None,
        1 => Some(EntityUuid::try_from_uuid(Uuid::from_bytes(
            reader.array()?,
        ))?),
        _ => return Err(InvalidCooked("unknown track target")),
    };
    let component =
        ComponentTypeId::try_new(reader.text("component ID bytes", ComponentTypeId::MAX_BYTES)?)?;
    let path_text = reader.text("field path bytes", MAX_PATH_BYTES)?;
    let path: ComponentFieldPath = path_text.parse()?;
    if path.to_string() != path_text {
        return Err(InvalidCooked("noncanonical field path"));
    }
    let kind = read_tag(reader, &ValueKind::TAGS, "unknown value type")?;
    let count = u32::from_le_bytes(reader.array()?) as usize;
    check_limit("track keyframes", count, MAX_KEYFRAMES)?;
    *total_keyframes += count;
    check_limit("clip keyframes", *total_keyframes, MAX_KEYFRAMES)?;
    reader.expect_bytes(count * (KEYFRAME_HEADER_BYTES + kind.minimum_bytes()))?;
    let mut keyframes = Vec::with_capacity(count);
    for _ in 0..count {
        keyframes.push(PropertyKeyframe {
            time: reader.time()?,
            easing: read_tag(reader, &EASINGS, "unknown easing")?,
            value: kind.read(reader)?,
        });
    }
    PropertyTrack::new(target_child, component, path, keyframes)
}

fn write_time(bytes: &mut Vec<u8>, time: Duration) {
    bytes.extend_from_slice(&(time.as_nanos() as u64).to_le_bytes());
}

fn write_text(bytes: &mut Vec<u8>, text: &str) {
    bytes.extend_from_slice(&(text.len() as u16).to_le_bytes());
    bytes.extend_from_slice(text.as_bytes());
}

fn write_value(bytes: &mut Vec<u8>, value: &KeyframeValue) {
    match value {
        KeyframeValue::F32(value) => bytes.extend_from_slice(&value.to_le_bytes()),
        KeyframeValue::Vec2(value) => {
            for channel in value.to_array() {
                bytes.extend_from_slice(&channel.to_le_bytes());
            }
        }
        KeyframeValue::Color(Color { r, g, b, a }) => {
            for channel in [r, g, b, a] {
                bytes.extend_from_slice(&channel.to_le_bytes());
            }
        }
        KeyframeValue::Bool(value) => bytes.push(u8::from(*value)),
        KeyframeValue::I64(value) => bytes.extend_from_slice(&value.to_le_bytes()),
        KeyframeValue::String(value) => write_text(bytes, value),
    }
}

fn read_tag<T: Copy>(
    reader: &mut Reader<'_>,
    table: &[T],
    invalid: &'static str,
) -> Result<T, PropertyAnimationClipError> {
    table
        .get(reader.byte()? as usize)
        .copied()
        .ok_or(InvalidCooked(invalid))
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn array<const N: usize>(&mut self) -> Result<[u8; N], PropertyAnimationClipError> {
        let (head, tail) = self
            .0
            .split_first_chunk()
            .ok_or(InvalidCooked("truncated data"))?;
        self.0 = tail;
        Ok(*head)
    }

    fn byte(&mut self) -> Result<u8, PropertyAnimationClipError> {
        Ok(self.array::<1>()?[0])
    }

    fn time(&mut self) -> Result<Duration, PropertyAnimationClipError> {
        Ok(Duration::from_nanos(u64::from_le_bytes(self.array()?)))
    }

    fn float(&mut self) -> Result<f32, PropertyAnimationClipError> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    fn text(
        &mut self,
        what: &'static str,
        max: usize,
    ) -> Result<&'a str, PropertyAnimationClipError> {
        let len = u16::from_le_bytes(self.array()?) as usize;
        check_limit(what, len, max)?;
        let (head, tail) = self
            .0
            .split_at_checked(len)
            .ok_or(InvalidCooked("truncated text"))?;
        self.0 = tail;
        str::from_utf8(head).map_err(|_| InvalidCooked("non-UTF-8 text"))
    }

    fn expect_bytes(&self, minimum: usize) -> Result<(), PropertyAnimationClipError> {
        if self.0.len() < minimum {
            return Err(InvalidCooked("truncated records"));
        }
        Ok(())
    }
}
