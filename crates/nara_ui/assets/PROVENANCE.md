# Bundled Fira Sans Provenance

`FiraSans-Bold.ttf` is copied byte-for-byte from Bevy commit
`1f2feb69f85f691874be54fa2f56b66c08a70ff6`:

`https://raw.githubusercontent.com/bevyengine/bevy/1f2feb69f85f691874be54fa2f56b66c08a70ff6/assets/fonts/FiraSans-Bold.ttf`

- Size: 438,028 bytes
- SHA-256: `12b632bf96ea4e760691f345ef7743c549ca387a018681e11094fcf6146244f1`
- Face index: 0
- Upstream family/version: Fira Sans Bold 4.106

`FiraSans-LICENSE.txt` is copied byte-for-byte from Mozilla Fira commit
`fd8c8c0a3d353cd99e8ca1662942d165e6961407`:

`https://raw.githubusercontent.com/mozilla/Fira/fd8c8c0a3d353cd99e8ca1662942d165e6961407/LICENSE`

- Size: 4,479 bytes
- SHA-256: `99bfb7c1e845b79d1031fcf37e678178eb4a8c7cc74a3f218ddf7223083a6fcd`
- License: SIL Open Font License 1.1

Bevy's adjacent `assets/fonts/OFL.txt` belongs to Mona Sans and is intentionally not used as Fira
Sans provenance.

## Regular interface face

`FiraSans-Regular.ttf` is copied byte-for-byte from Google Fonts commit
`ed7143b8f0c9587f9dcfbcdf5b34ec1a7bc07fca`:

`https://raw.githubusercontent.com/google/fonts/ed7143b8f0c9587f9dcfbcdf5b34ec1a7bc07fca/ofl/firasans/FiraSans-Regular.ttf`

- Size: 456,996 bytes
- SHA-256: `c29556a2719bf613ef3d5e070e40d903a8965d9c081beca1375dc1e6e0f93c23`
- Face index: 0
- License: SIL Open Font License 1.1, the adjacent `OFL.txt` at the same commit, copied
  unchanged as `FiraSans-Regular-LICENSE.txt`.

`bundled_regular_font_face` is an explicit caller choice. The original bundled Bold face
and its default consumers retain their existing identity; no synthetic weight or family
discovery is introduced.

## Typography test fallback

`tests/fixtures/NotoSansSC-subset.otf` retains U+4E2D, U+6587, U+65E5, U+672C and U+8A9E
from the real `NotoSansSC-Regular.otf` at [Noto CJK Sans2.004](https://github.com/notofonts/noto-cjk/tree/Sans2.004/Sans/SubsetOTF/SC).
The original SHA-256 is `faa6c9df652116dde789d351359f3d7e5d2285a2b2a1f04a2d7244df706d5ea9`.
FontTools Subsetter used its default options with `recalc_timestamp=False`; original metrics,
retained outlines and copyright metadata are preserved. The original license is adjacent as
`NotoSansSC-LICENSE`. This finite fixture qualifies real mixed-face metrics without an OS font
dependency; it is not the product's CJK font policy.
