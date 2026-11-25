# nara_sprite_animation

Immutable named Sprite clips, explicit source import and source-independent Runtime decoding.
The caller supplies elapsed time, selects a clip and applies the sampled region to its own Sprite.
The module creates no clock, playback system, ECS player, Image storage or GPU resources.

```rust
use std::time::Duration;
use nara_core::Vec2;
use nara_sprite::TextureRegion;
use nara_sprite_animation::{Clip, ClipError, Frame, PlaybackMode};

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

Construct clips when their definitions change, then reuse `clip.sample(elapsed).region`.
`Frame::new` admits finite,
positive-area normalized UV rectangles and positive durations representable in u64 nanoseconds.
`Clip::new` admits a nonempty frame sequence and its cumulative duration. A frame includes its
start and excludes its end. `Loop` wraps at the exact clip end; `Once` holds the last frame and
reports `finished` at and after that end. A large elapsed `Duration` retains its full u128
nanosecond precision before looping. Sampling allocates no storage and repeats no frame validation.

`SpriteAnimationSet` associates one complete Product with case-sensitive named clips. Names use
ASCII letters, digits, `_`, `.` and `-`, with at most 64 bytes. A set admits at most 32 clips,
256 frames per clip and 4096 frames in total. An empty set is valid data; the consuming game
determines which names it requires. Lookup and iteration borrow immutable clips. Iteration is
in name order; the frame order within each clip remains authored order.

The Sprite continues to own its Image and sampler. Normalized clip regions can describe an atlas
layout independently of Image loading, but they do not verify the atlas's visual content or pixel
dimensions. The concrete game or another binding layer owns that relationship.

## Source And Cooked Content

`SpriteAnimationDomain` implements the existing `SourceAssetImporter` and `SourceAssetBuild`.
Its source kind is `nara.sprite-animation`, with exactly empty import settings `{}`. The source's
actual metadata identity supplies its primary Product. The version-one descriptor is strict JSON:

```json
{
  "format_version": 1,
  "clips": [
    {
      "name": "walk",
      "mode": "loop",
      "frames": [
        { "min": [0.25, 0.0], "size": [0.25, 1.0], "duration_nanos": 100000000 },
        { "min": [0.5, 0.0], "size": [0.25, 1.0], "duration_nanos": 200000000 }
      ]
    }
  ]
}
```

`SpriteAnimationCodec` uses a versioned JSON Runtime representation carrying the complete Product
and the same clip records. Both decoders use the original core constructors. Runtime membership
compares the complete source and Product identity. Decoding loads no auxiliary files or Images.
The source and Runtime facets declare finite encoded, parser-work and retained-storage allowances
through the existing Asset owner. Those logical allowances are not process-memory measurements.

An application selects source import and Runtime decode separately through its `ProductRecipe`.
The [domain guide](../../docs/guides/project-asset-domains.md) describes those existing interfaces.
The [Two-Level Game](../../samples/two-level-game/README.md) uses an authored `idle`/`walk` resource,
its original game clock and the original source/cooked collection lifetime. Its owned clip selection
does not add a persistent playback cursor or change the resume wire format.

Independent playback state, events, blending, Sprite write arbitration, field animation and
skeletal targets remain responsibilities of their selected consuming modules.
