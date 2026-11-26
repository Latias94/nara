# Sprite animation

Build immutable clips in Rust, sample them with elapsed time, and apply the sampled region to a
sprite.

```rust
use std::time::Duration;
use nara_core::Vec2;
use nara_render::{
    animation::{Clip, ClipError, Frame, PlaybackMode},
    sprite::TextureRegion,
};

fn walking_clip() -> Result<Clip, ClipError> {
    Clip::new(PlaybackMode::Loop, [
        Frame::new(
            TextureRegion::new(Vec2::new(0.25, 0.0), Vec2::new(0.25, 1.0)),
            Duration::from_millis(40),
        )?,
        Frame::new(
            TextureRegion::new(Vec2::new(0.5, 0.0), Vec2::new(0.25, 1.0)),
            Duration::from_millis(60),
        )?,
    ])
}
```

Construct clips when their definitions change, then reuse `clip.sample(elapsed).region`. A frame
includes its start and excludes its end. `Loop` wraps at the exact clip end. `Once` holds the last
frame and reports `finished` at and after that end. Sampling allocates no storage.

`SpriteAnimationSet` groups case-sensitive named clips. The sprite owns its image and sampler.
The game keeps its elapsed time and matches the clip's UV regions to the chosen atlas.
