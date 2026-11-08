# Late pulse effect clocks

ITGmania's `Actor::UpdateInternal` advances the per-actor timer even when
the actor has no active effect. `Actor::SetEffectPulse` preserves that timer,
including when pulse is reapplied with a different period. DeadSync previously
evaluated these pulses against song time, shifting the character animation in
Mawaru8's cooperation game.

Chronological compilation now retains the timer for every actor it advances.
The timer is created once per actor during compilation and retained for that
compile; playback consumes the compiled phase tracks without running Lua or
allocating clocks. Only bob, bounce, wag, and pulse emit these phase tracks.
Pulse applies its captured phase when producing effect state, so parent
transforms and leaf rendering use the same clock. The existing global-time pulse
assumption is replaced; motion reset behavior is preserved.

`late-pulse.lua` starts pulse after 3.25 seconds, then reapplies it after another
1.4 seconds with a different period. It covers a bottom-aligned leaf and a child
under a pulsing parent. The local harness compares 6,736 screen coordinate
values with real native ITGmania actors over 421 frames. The DeadSync regression
checks all 2,210 semantic fields and 3,368 rendered native corners. The prior
MAIN build passed only 1,630 of those semantic checks.

The native semantic trace and actor draw data were captured locally from
ITGmania revision `5b205125ad53b9867bb4a494ff858f8d38ad4406` with
`semantic_host.lua` SHA-256
`8254e16e41b79a833adda87b5ed9de07920ae04556800073ca72155b6b4fdf10`.
Both use 854 by 480, 120 BPM, 60 Hz, seed 1, and both players enabled.
Raw semantic SHA-256:
`eb7964ba9dc3e307b85d2e1e750a8b09c40398b0c1860c322d04e73f91f73bc0`.
Raw actor draw SHA-256:
`72e1a9666e12f5af8b807b0f6393101b154770a0cc3c607bf79214abb4b916af`.
Zstandard compression was verified by exact round trip.

Validation: all 781 song-Lua unit tests, 103 regular semantic tests, and 132
local harness tests pass. Rustfmt checks pass for the changed DeadSync source
and test files. Tolerances and expected native values were not widened.

The complete local Mawaru8 audit improves from **98,165/98,602** to
**98,581/98,602**. Projected geometry improves from 68,441/68,871 to
68,857/68,871. The cooperation character pulse differences are resolved.
Full-song parity remains unfinished: five speculative input-message probes,
two player rotation ranges, and fourteen projected geometry checks still fail.
The fresh complete native trace has zero runtime errors and dropped events.

No files were downloaded, and original song resources and frozen project
metadata were left unchanged. Hash and missing-resource findings remain in
`song-lua-native-queue-clock.md` for the user's later correction.
