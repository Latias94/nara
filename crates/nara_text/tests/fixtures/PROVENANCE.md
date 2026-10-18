# Custom Font Test Fixture

`FiraMono-Medium.ttf` is copied byte-for-byte from the local Bevy reference checkout at commit
`85503941e488152711a4695bbe248e9ed9f453fc`. Its adjacent `FiraMono-LICENSE` is copied with line
endings normalized to LF:

- `assets/fonts/FiraMono-Medium.ttf`
- `assets/fonts/FiraMono-LICENSE`

The original repository is `https://github.com/bevyengine/bevy`.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `FiraMono-Medium.ttf` | 173,516 | `5f9173ce3d05fadef74c7eed06570d54e4f75bd0cd9860726fb2987a7f848292` |
| `FiraMono-LICENSE` | 4,386 | `587dd9548304fd421cd254f7550bd101c1250ffc95417f2f7f92d55b44ecbe53` |

The fixture uses face index zero and is distributed under the SIL Open Font License 1.1.
It exercises a real admitted font whose content identity differs from the bundled Fira Sans face.
Tests embed the owned fixture and do not require the reference checkout or system fonts.
