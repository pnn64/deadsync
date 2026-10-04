# Jumper Lua parity (0.5.1736)

The frozen project's Jumper chart is dance-single Challenge, hash
`7c085505e95af69a`, description `Mild Gallopy Chart`, meter 10. The full
comparison passes **1,102 / 1,102** checks against a fresh native capture.
The corpus test verifies the chart hash and the capture's random seed.

## Runtime corrections

A startup `sleep(180):queuecommand("Answer")` previously executed its Lua
callback while probing startup commands. That put `ShowAnswer` on the first
frame instead of the frame after the timeout. Startup queues now retain
the delayed callback and enter the normal chronological queue clock.
Clearing a queue also clears its deferred-startup state. The optimized
multitap path yields to chronological capture when such work is pending.

Stateful cross-actor message commands also use deferred capture when a
second static probe fails or produces different effects. Their real
broadcast records the writes and clears the provisional diagnostic. This
replaces duplicate deferred-message registration with one shared path;
unobserved, unsupported commands still retain their diagnostic. A small
regression consumes a table entry once and verifies its real target write.

Jumper's answer handler sleeps another seven seconds before hiding
`SongForeground` and restoring the players and screen layers. Those writes
target actors outside the returned Lua tree. The chronological reader now
captures foreground and screen-layer writes as timed commands on their
existing captured actors. No new runtime track representation is needed.

The layer renderer now skips a hidden foreground and composes its parent
state into drawable actors, including transforms and diffuse tint. Hidden
layers continue advancing their commands; drawing them does not advance
the simulation. The semantic geometry comparison samples the production
message replay and parent composition functions too.

## Native harness corrections

`LoadActor` now creates a Sound for ITGmania's supported audio extensions,
including extensionless sound names. The previous harness emitted four
spurious Sprite drawables for Jumper's keyboard sounds. Relative actor
loads made by queued callbacks now resolve from the callback's Lua source
directory, so Jumper's input handler is loaded and executed.

The harness accepts an explicit `--random-seed`. Seed 1 exposes a bug in
the original song: its fifth `math.random(0,5757)` draw is zero, indexing
outside its one-based dictionary. Native ITGmania consequently fails in
`SaveData` while concatenating the second guest's missing word. Seed 2
draws `2510, 1065, 149, 5363, 3164`, allowing the original song to run.
This pass explicitly uses seed 2; the song and dictionary are unmodified.
The seed-1 authoring bug remains, and this capture covers the valid seed-2
branch rather than every possible random draw or interactive word guess.

Actual gameplay previously forced seed 1 on every song load. The shell now
chooses the current Unix time, matching native ITGmania's default seeding,
and records it in the compile context once before probing or retrying.
The reproducible compile-context default stays at 1; native comparisons
specify their seeds explicitly. This does not change the random algorithm
or repair invalid dictionary indexing in the original song.

## Complete native capture

ITGmania revision: `5b205125ad53b9867bb4a494ff858f8d38ad4406`.
Context: seed 2, cel, both players enabled, 854 by 480, 60 Hz updates,
samples every 0.125 beat. The capture reaches beat 608 and
204.9438202247191 seconds with 12,298 update frames, zero runtime errors,
and zero dropped events.

| Comparison | Passed / total |
| --- | ---: |
| Compile info | 4 / 4 |
| Layer order | 2 / 2 |
| Final render | 344 / 344 |
| Projected geometry | 536 / 536 |
| Projected vibration | 215 / 215 |
| Timeline | 1 / 1 |
| Total | **1,102 / 1,102** |

After correcting the sound classification, the old runtime failed 108
checks. Correcting delayed startup fixed the timeline check; retaining
and rendering foreground state fixed the remaining 107 visibility checks.
Comparison tolerances are unchanged.

The fixture is
`tests/fixtures/itgmania-song-lua-selected/Jumper/Jumper.ssc.semantic.json.zst`.
The selected manifest records exact source, noteskin, harness and capture
hashes. Raw capture SHA-256:
`db441fd6a8db15e37013e0e27f9c8c7a20432f1fb06c45acadce9e28be0217a6`.
Compressed capture SHA-256:
`f9672a8752a0f51bbb83904a8cb6fedeac388e385b3624a8a5fffa141834b7eb`.
Simfile SHA-256:
`5c0fd85553d9b7148d297d805e6c4370e511375d92ca8997681baf969ccbec38`.
Capture host SHA-256:
`9c6791a539865d234af68d77fb4e08911de524662dda5e9001ebf7963762a606`.

Regenerate in the rework workspace with the cel noteskin environment used
by the other selected captures:

```powershell
itgmania-harness-rs/target/debug/itgmania-harness-rs.exe song-lua-semantic-baseline `
  "lua-songs/Jumper" --out .tmp/jumper-native --difficulty Challenge `
  --steps-type dance-single --random-seed 2 --beat-step 0.125 --max-events 2000000
```

## Regression coverage

Three small, unmodified native captures cover the individual gaps. Their
scripts and simfiles live in `crates/deadsync-song-lua/tests/fixtures/`:

- `startup-delay`: ITGmania broadcasts at 2.0166667 seconds, and the old
  runtime fails that timeline check.
- `foreground-delay`: a queued message hides the external foreground.
  It also applies foreground translation, zoom and alpha before the hide.
  Native projected geometry verifies inheritance and visibility. The
  production layer-builder test checks emitted actors before and after
  the hide, then seeks backward to verify drawing resumes.
- `consumed-message`: a receiver consumes one table entry and writes
  another actor. The real broadcast is captured despite the failed second
  static probe. Removing that runtime broadcast fails the native timeline
  comparator even though there is no static receiver command block.

Timeline comparisons include runtime message captures and screen-layer
listeners. This preserves checks when the compiler represents a callback
with sampled writes rather than a static block. Bank Account and Delightful
Day gain seven and four timeline checks respectively; their render,
geometry and modifier checks are unchanged.

The main pass passes 973 Lua, profile, performance and playback tests and
100 regular semantic tests. Complete regressions pass 398,506 double
feelyourtouch checks, 435,962 Botanic Panic checks and 352,855 mawaru9
checks. The latter two gain timeline coverage for runtime-only message
listeners. Harness regressions pass 129 tests covering seed selection,
Sound loading and relative actor loading from queued callbacks. The shell
test suite passes 360 tests and covers the gameplay loader with explicit
seeds; the application supplies wall time and tests supply fixed values.

The HUD-fade gameplay fixture now excludes `SongBackground` and
`SongForeground`, as Delightful Day's actual script does. Its old blanket
fade made the parent of its own proxy transparent, so its expectation of
visible notes depended on the missing foreground-alpha inheritance.
