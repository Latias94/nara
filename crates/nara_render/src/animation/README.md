# Sprite animation

Build immutable clips in Rust or load a `SpriteAnimationSet` asset. The sprite owns its image
and sampler, and animation selects a normalized region within that image.

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
includes its start and excludes its end. `Loop` wraps at the exact clip end. `PingPong` traverses
the interior frames in reverse after the forward pass, giving each endpoint its authored duration
once per cycle. `Once` holds the last frame and reports `finished` at and after the clip end.
Sampling allocates no storage.

## Authoring and playback

The `animation-authoring` feature enables `.sanim.ron` loading and parsing. Clip names are
case-sensitive. Omit `event` on frames that have no marker.

```ron
(
    clips: [
        (name: "walk", mode: loop, frames: [
            (min: (0.25, 0.0), size: (0.25, 1.0), duration_nanos: 40000000),
            (min: (0.5, 0.0), size: (0.25, 1.0), duration_nanos: 60000000,
             event: Some("footstep")),
        ]),
    ],
)
```

Cook packages animation sets as NSAN v1. Install `SpriteAnimationPlugin` with `AssetPlugin`, then
attach `SpriteAnimator::new(handle, "walk")` alongside a `Sprite`. The animator advances in
`Update` using virtual time by default. Use `.with_clock(SpriteAnimationClock::Fixed)` to advance
in `FixedUpdate`. Game systems can order their writes or message reads against
`SpriteAnimationUpdateSet` in the corresponding schedule.

`SpriteAnimationEvent` reports markers on entering frames, including the starting frame and cycle
wraps, and completion after the last frame of a `Once` clip. A single step reports the first 1,024
crossed markers per animator and samples the frame at the full elapsed time.

With `animation-scene`, the plugin also resolves `SpriteAnimatorSource` scene components into
runtime animators. The scene selects the initial clip, and changes to the source apply when the
requested clip is available. Runtime systems can switch clips between source changes.
