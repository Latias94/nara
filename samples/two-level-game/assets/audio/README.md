# Two-Level Game one-shots

These are original procedural sound effects authored for Nara by Codex (OpenAI) on
2026-11-16. They contain no third-party recordings or samples and use the repository's
license. Both files are canonical RIFF/WAVE, 48,000 Hz, mono, little-endian signed PCM16.
Their attack and release envelope starts and ends at zero.

| File | Role | Frames | Duration | Bytes | SHA-256 of the complete WAV |
| --- | --- | ---: | ---: | ---: | --- |
| `pickup.wav` | Committed pickup: rising two-partial chirp | 5,760 | 0.12 s | 11,564 | `132109469d5b135114f78a7d208c04268a06643c84e00041c22e97cb3cefda24` |
| `travel.wav` | Successful travel entry: low three-partial chord | 11,520 | 0.24 s | 23,084 | `dcb899e67004520500594f1ffb657607fd498dc0fa5fce0894a54ea58573bfdf` |

## Deterministic source recipe

The following Python standard-library recipe is the complete source used for these assets.
It has no seed, randomness, external input, encoder dependency, or metadata timestamps.
Python `round` uses ties-to-even; samples are packed as signed 16-bit little-endian integers.
The checked-in WAV bytes and hashes above are authoritative if another platform's system
math library produces different last-bit floating-point results.

```python
import math
import struct
import wave
from pathlib import Path

root = Path("samples/two-level-game/assets/audio")
root.mkdir(parents=True, exist_ok=True)
rate = 48_000
for name, frames in [("pickup", 5_760), ("travel", 11_520)]:
    values = []
    duration = frames / rate
    for n in range(frames):
        t = n / rate
        u = n / (frames - 1)
        envelope = math.sin(math.pi * u) ** 2
        if name == "pickup":
            phase = math.tau * (880.0 * t + 880.0 * t * t / (2.0 * duration))
            tone = (math.sin(phase) + 0.25 * math.sin(2.0 * phase)) / 1.25
            value = round(32_767 * 0.28 * envelope * tone)
        else:
            tone = (math.sin(math.tau * 220.0 * t)
                    + 0.5 * math.sin(math.tau * 330.0 * t)
                    + 0.25 * math.sin(math.tau * 440.0 * t)) / 1.75
            value = round(32_767 * 0.22 * envelope * tone)
        values.append(value)
    with wave.open(str(root / f"{name}.wav"), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(rate)
        output.writeframes(struct.pack(f"<{frames}h", *values))
```

`game.soundbank.json` names these raw dependencies. Its single `.meta` file supplies the bank's
stable primary Product; the two raw WAVs have no separate identity sidecars. The game-owned
`src/sound_bank` importer validates the complete RIFF structure and Hound 3.5.1 format admission
before preparing either clip. It refuses a different rate, channel count, sample format, empty
input, or a clip longer than one second. Each PCM16 sample is normalized by exactly 32,768 into
immutable `Arc<[f32]>` storage. The source-free Runtime format retains complete Product identity,
fixed descriptors and canonical PCM16 with no source paths. The native callback keeps the acquired
bank's original content budget owner along with its sample allocations. The CPU mixer copies the mixed mono value to stereo, without
resampling, and clamps the final sum to `[-1, 1]`. One-shots follow the output device
clock even while game time is paused. CPU frame positions describe submitted callback
samples; they do not establish physical playback or human audibility.
