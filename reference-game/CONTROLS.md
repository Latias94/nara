# Controls

## Desktop

- `W`, `A`, `S`, `D`: move the player
- `Enter`: start a new wave after victory or defeat
- Window close button: stop and retire the runtime

The HUD shows the current wave state, player health, score, and remaining enemies.

## Headless

The headless executable requires no physical input. It runs the bundled semantic command stream and
prints one terminal `nara-reference-game.wave-summary-v1` JSON object.

```text
headless[.exe] --max-ticks 96
```

The maximum tick value must be between 1 and 256.
