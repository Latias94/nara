# Two-Level Exploration

A small independent Rust product using Nara's public project, command, Scene and filesystem APIs.
It owns ordinary Rust modules for gameplay and checkpoint data; the Reference Game and its wave
CLI remain separate consumers.

The blue explorer moves with WASD and automatically collects nearby gold gems. E travels between
Meadow and Cavern. Q switches between two independently persistent instances of the current source.
M places a pink dynamic marker. K saves the current checkpoint and L restores the slot. R restarts
the current logical level at its checkpoint, or retries a failed reconstruction. Closing the window
uses normal Runtime shutdown, including pending Scene preparation and temporary-file cleanup.
The green square marks the checkpoint; the purple square marks the destination portal.

## Build And Run

Run commands from this package directory. The optional desktop target needs the platform's normal
native window and GPU support. Reuse the engine target directory and one build job when developing
inside the repository.

```text
nara schema . --write
nara schema . --check
nara check .
cargo run --bin headless
cargo run --bin headless -- --verify-travel
cargo run --features desktop --bin desktop -- --save /absolute/existing-parent/checkpoint.json
cargo run --features desktop --bin desktop -- --verify-first-present
```

On Windows, pass a fully qualified path such as `D:\GameSaves\checkpoint.json`. The application
does not choose the process working directory or silently create an arbitrary save parent. Omit
`--save` for a session without persistence; K/L then report the absent application slot.

The authoring bundle is generated from this product's real native schema provider with the existing
schema command. It is not a hand-maintained second definition of component layouts.

## Checkpoint Acceptance

Create an application-owned directory, then run the same headless binary twice:

```text
cargo run --bin headless -- --verify-save /absolute/existing-parent/checkpoint.json
cargo run --bin headless -- --verify-restore /absolute/existing-parent/checkpoint.json
```

The save trajectory collects in Meadow 0, Cavern 0 and Cavern 1, places markers with IDs 1/2/3,
returns to Cavern 0, retries that current level and publishes the slot there. It requires score 30,
three collected facts and next marker ID 4 in the actual file readback. The second invocation starts
with a fresh process/World, restores Cavern 0, creates marker 4, visits Cavern 1 and returns to Meadow
0. It checks actual Sprite visibility, live dynamic marker IDs and changed `CurrentScene` instance
receipts, not just copied DTO values. Both invocations print `verified=true` only after completing
their finite command trajectory.

```text
cargo nextest run --manifest-path samples/two-level-game/Cargo.toml --all-targets
```

The command above is relative to the engine root. Tests include separate save/restore processes,
an actual destination source change after startup, corrupt save rejection, exact local conflict
checks, interruption before publication, and pure bounded format/migration cases. Desktop
`--verify-first-present` proves presentation only; it does not prove that a human exercised every
key or that a particular platform has passed the full interactive trajectory.

## Ownership And Limits

- `CurrentScene` is the only source/Scene-instance authority. Gameplay retains a logical `LevelKey`
  and bounded marker tokens for its own dynamic entities; it never maintains another Scene receipt.
  Normal R uses the retained committed source even if its disk file has changed; it is not Reload.
- `ProjectScenes` prepares only startup or declared content through the original Host loader and
  budget. The game inspects a read-only target, validates all checkpoint-referenced level sources
  one at a time, then explicitly commits the final target with the existing protected overlay.
- Collected authored pickups remain Scene members. Their game-owned collected facts make them
  inactive and set their actual Sprite alpha to zero. They are logically removed, not despawned.
- Dynamic marker IDs are game-owned serials. Live marker entities have ordinary engine identities
  and hierarchy placement. Travel supplies their exact retirement tokens and reconstructs target
  markers after commit. A reconstruction failure reports `Degraded`; it does not claim that the old
  World was rolled back. R retries reconstruction without reusing a stale Scene receipt.
  The automated trajectory verifies ordinary current-level Retry, including changed source files.
  Recovery from an injected
  post-commit native reconstruction failure has not been qualified by that trajectory.
- The versioned slot contains only explicit checkpoint/progress facts. Restore rebuilds movement,
  player position and dynamic entities at the authored checkpoint; it is not arbitrary-tick replay
  or serialization of World entities, tasks, handles, native resources or every component.
  In this game score has one source: each distinct collected pickup awards ten points. Content
  admission rejects otherwise well-formed checkpoints whose score disagrees with those facts.
- The single local writer uses an explicitly opened `TrustedLocal` parent capability, bounded
  decoding, advisory/platform locks, expected identity plus content digest, a synced temporary file,
  atomic name replacement and identity-bound readback. This is a cooperative local-file contract,
  not protection against hostile concurrent writers. A changed target is rejected. Failed temporary
  cleanup remains owned by the slot and its Runtime close participant. File-save success reports
  the parent-directory sync fact separately; unsupported directory sync is not crash-durability proof.
  The current Linux filesystem adapter cannot discard a failed candidate by exact opened-object
  identity. That failure remains `CleanupPending` and prevents clean Runtime shutdown; the sample
  retains the cleanup owner and does not substitute a pathname deletion or report false completion.
  Save is synchronous and bounded to 64 KiB of checkpoint data; this does not guarantee frame time
  or bound operating-system I/O latency. The headless binary's ten-second deadline bounds cleanup
  retries after gameplay returns, not the whole gameplay run. Child-process qualification adds its
  own external 90-second deadline and bounded captured output.

The headless entry uses complete managed frames through `HeadlessRunIntent::frames` so PreUpdate
collects semantic commands and TaskUpdate publishes prepared Scene results. Its supplied 20 ms
real delta is simulation input, not a sleep. App time settings remain authoritative. The separate
exact-tick API intentionally omits ordinary gameplay frame stages. This finite script has a
200,000-frame ceiling and reports 30 seconds without actual progress using a monotonic clock;
neither is a gameplay performance guarantee or an interrupt of a blocked synchronous operation.
Travel, fresh-process save/restore and retained-source Retry pass the sample's 24-test Windows
cohort. The release export separately passes both startup product observations and the same three
travel/save/restore operations from an unrelated directory. This does not qualify other platforms
or a complete interactive desktop control trajectory.

## Export

```text
nara export . --parent /absolute/existing-parent --destination two-level-candidate
```

The destination must be absent. Run `bin/headless --verify-travel` and the two checkpoint commands
from an unrelated directory; use `.exe` on Windows. `bin/desktop --verify-first-present` verifies
actual first presentation through the ordinary shutdown path. Both declared level sources and their
Images/metadata are delivered in the sibling `project/` tree. This is a source-carrying development
candidate, with checkpoint location still chosen by the application, rather than a full project backup.

Export uses the existing protocol-2 headless/desktop targets. `[content]` declares Cavern in addition
to implicit startup Meadow, so Check and export cover both Images and metadata through the delivery
union. Packaged executables load the sibling `project/` tree; they do not fall back to the source
checkout when that packaged tree is present but invalid. The five-file build-input observation is
not a complete hash of Rust `mod` dependencies; ordinary Cargo remains responsible for compiling
changes to these gameplay modules.
