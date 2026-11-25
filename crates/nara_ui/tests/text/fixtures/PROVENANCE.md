# Custom Font Test Fixture

`FiraMono-Medium.ttf` is copied byte-for-byte from the local Bevy reference checkout at commit
`85503941e488152711a4695bbe248e9ed9f453fc`. Its adjacent `FiraMono-LICENSE` is copied with line
endings normalized to LF and one trailing space removed:

- `assets/fonts/FiraMono-Medium.ttf`
- `assets/fonts/FiraMono-LICENSE`

The original repository is `https://github.com/bevyengine/bevy`.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `FiraMono-Medium.ttf` | 173,516 | `5f9173ce3d05fadef74c7eed06570d54e4f75bd0cd9860726fb2987a7f848292` |
| `FiraMono-LICENSE` | 4,385 | `dd6c2808925cc92548634cb06c25b933f280eff90b181d381eb8b8879222458a` |

The fixture uses face index zero and is distributed under the SIL Open Font License 1.1.
It exercises a real admitted font whose content identity differs from the bundled Fira Sans face.
Tests embed the owned fixture and do not require the reference checkout or system fonts.

`MonaSans-VariableFont.ttf` is copied byte-for-byte from `assets/fonts/MonaSans-VariableFont.ttf`;
`MonaSans-LICENSE` comes from `assets/fonts/OFL.txt` in the same pinned Bevy checkout, with one
trailing space removed. This SIL Open Font License 1.1 fixture detects implicit style synthesis away from
the admitted face's default variation instance.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `MonaSans-VariableFont.ttf` | 348,692 | `ae12d1b4e32a29fd115da324e525469ce760157803ab963e22967741e72671e0` |
| `MonaSans-LICENSE` | 4,511 | `d7fdb7f636f8dab1b6ebd9152bdfd265682587d9610c478776732ddefe8238e8` |
