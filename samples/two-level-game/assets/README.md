`textures/meadow.png` and `textures/cavern.png` reuse the original geometric token authored for
the sibling Collect Game. Their different stable asset identities deliberately exercise
per-level Image residency. No external stock-image or font source is required by this sample.

`textures/explorer.png` is an original procedural geometric game fixture authored for this sample,
with no external artwork or attribution requirement. It is a 64-by-16 RGBA8 sRGB PNG containing
four adjacent 16-by-16 frames in one row, ordered from left to right:

| Index | Frame | Background RGBA |
| --- | --- | --- |
| 0 | Idle | `[64, 160, 240, 255]` |
| 1 | WalkOne | `[32, 216, 232, 255]` |
| 2 | WalkTwo | `[240, 152, 48, 255]` |
| 3 | WalkThree | `[112, 224, 80, 255]` |

The exact pixel recipe uses zero-based local coordinates within each frame, with `(0, 0)` at
the top left. Pixels where `x` or `y` is `0` or `15` form a one-pixel border colored
`[16, 32, 48, 255]`. The two single-pixel eyes at `(5, 5)` and `(10, 5)` are
`[255, 255, 255, 255]`. Every other pixel uses that frame's background color. There are no feet
or other details; all pixels are opaque. The fixture was generated directly with Python Pillow,
saved as RGBA PNG with an sRGB chunk, without modifying any existing image.

The sidecar uses the same Image metadata format as the level textures, with sRGB import settings
and source UUID `2eece9cd-6332-4868-b0d2-289879df9de8`. Its complete Product is that source's
`primary` Product. The game-owned `animation` module maps each frame to an exact normalized
quarter-width texture region and selects walking frames 1 through 3 every 100 ms of logical game
time whenever held directions do not cancel on both axes.
