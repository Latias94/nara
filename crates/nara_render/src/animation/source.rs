use std::{fmt, marker::PhantomData, time::Duration};

use nara_core::Vec2;
use serde::{Deserialize, Deserializer, de};

use super::{
    Clip, Frame, MAX_CLIPS, MAX_FRAMES_PER_CLIP, MAX_SPRITE_ANIMATION_SOURCE_BYTES, PlaybackMode,
    SpriteAnimationAssetError, SpriteAnimationSet,
};
use crate::sprite::TextureRegion;

#[derive(Deserialize)]
#[serde(rename = "SpriteAnimationSet", deny_unknown_fields)]
struct AnimationSource {
    clips: BoundedList<ClipSource, MAX_CLIPS>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipSource {
    name: String,
    mode: PlaybackMode,
    frames: BoundedList<FrameSource, MAX_FRAMES_PER_CLIP>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameSource {
    min: [f32; 2],
    size: [f32; 2],
    duration_nanos: u64,
    #[serde(default)]
    event: Option<String>,
}

impl SpriteAnimationSet {
    /// Parses a bounded `.sanim.ron` document and admits its clips and frames.
    pub fn from_ron(source: &str) -> Result<Self, SpriteAnimationAssetError> {
        validate_source_len(source.len())?;
        let source: AnimationSource = ron::from_str(source)?;
        let mut clips = Vec::with_capacity(source.clips.0.len());
        for ClipSource { name, mode, frames } in source.clips.0 {
            let mut admitted = Vec::with_capacity(frames.0.len());
            for frame in frames.0 {
                let region =
                    TextureRegion::new(Vec2::from_array(frame.min), Vec2::from_array(frame.size));
                let mut value = Frame::new(region, Duration::from_nanos(frame.duration_nanos))?;
                if let Some(event) = frame.event {
                    value = value.with_event(event)?;
                }
                admitted.push(value);
            }
            clips.push((name, Clip::new(mode, admitted)?));
        }
        Ok(Self::new(clips)?)
    }
}

pub(super) fn validate_source_len(len: usize) -> Result<(), SpriteAnimationAssetError> {
    if len > MAX_SPRITE_ANIMATION_SOURCE_BYTES {
        return Err(SpriteAnimationAssetError::SourceTooLarge);
    }
    Ok(())
}

struct BoundedList<T, const LIMIT: usize>(Vec<T>);

impl<'de, T: Deserialize<'de>, const LIMIT: usize> Deserialize<'de> for BoundedList<T, LIMIT> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ListVisitor<T, const LIMIT: usize>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>, const LIMIT: usize> de::Visitor<'de> for ListVisitor<T, LIMIT> {
            type Value = BoundedList<T, LIMIT>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "a list of at most {LIMIT} entries")
            }

            fn visit_seq<A: de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element()? {
                    if values.len() == LIMIT {
                        return Err(de::Error::invalid_length(LIMIT + 1, &self));
                    }
                    values.push(value);
                }
                Ok(BoundedList(values))
            }
        }

        deserializer.deserialize_seq(ListVisitor(PhantomData))
    }
}
