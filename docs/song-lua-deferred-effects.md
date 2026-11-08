# Deferred message effects

Mawaru8's message handlers initialize their song tables after startup.
Retrying them at song end exposed two compiler gaps: the probe drained
queues on every actor, including unrelated looping actors, and it rejected
represented `SOUND:PlayOnce` calls as unsupported side effects.

Message probing now uses ordinary dispatch for the receiver and actors it
queues. The global queue sweep and its copied actor list are deleted.
Each capture retains a side-effect count. Deferred capture accepts stable
blocks when every side effect corresponds to one recorded sound play;
repeated plays remain separate even when they share a resource. Music
changes, missing resources, outgoing broadcasts and failing Lua remain
reported. Existing function-action handling still treats side effects as
handled, preserving its previous behavior.

## Regression evidence

The local `deferred-effects` fixture reproduces a late player binding, a
looping unrelated actor, receiver and cross-actor queued commands, and two
plays of one local sound. Before the fix, its unit test fails with retained
`UnsentMessageCommand` and `UnsentSoundMessageCommand` diagnostics.
The fixed test requires both queued effects, no unrelated message target,
two sound actors, one resource path, and no messages fired by probing.
A second test retains four diagnostics for music stop, partial music,
a missing sound, and genuinely broken Lua.

The local Lua semantic capture records the observed message and unchanged
oscillator schedule. An independent C++ actor oracle records all 421 frames
of the same oscillator. Playback must match each native rotation at the
existing `f32::EPSILON` threshold. Semantic and direct native captures,
input, generated local PCM sound, and source hashes are retained.
The sound host records API calls; these captures do not validate audible
output. No file was downloaded and frozen song identities were not edited.

## Runtime cost

The change is confined to the single-threaded song-load compiler. It
removes a per-probe actor-list allocation and unrelated queue execution.
Captures reuse existing song-lifetime message blocks and sound resources.
Gameplay has no new allocation, cache, file read, Lua probe or maintenance
work. Compiled resources are destroyed with the song at transition.

## Whole-song result and publication

The retained full Mawaru8 audit reports **306,497/306,499**, with two
remaining failures in `RyukoCheckP1/P2`. The previous audit was
306,497/306,502 with five gaps. Compile-info checks include one failure
per reported diagnostic, so removing three gaps reduces the denominator;
it does not add three positive native observations. The independent
regression supplies 365 semantic observations and 421 native queue states.

Every other full-song observation is retained: 92,870 projected geometry,
174,128 draw colors, 27,784 vibration, 6,854 update values, 3,150 render
persistence, 1,088 final render, 254 modifiers, 214 messages, 135 timeline,
10 player-range and three layer-order checks all pass. The audit continues
to fail honestly on the two unresolved broadcast-chain handlers.

Both repositories pass **1,088 selected regression tests**: 783 unit,
174 playback, two AMV, 112 regular semantic and 17 profile/gameplay tests.
Mawaru5 retains all 77,718 passing local full-song observations. The new
semantic fixture requires all 365 observations, and the direct C++ queue
comparison requires all 421 frames at the original tolerance.

Publication copies 12 curated source, fixture and documentation files to
MAIN, plus Cargo.toml and Cargo.lock. MAIN advances exactly once from
`0.5.1749` to `0.5.1750`. Its independent full Mawaru8 audit also reports
306,497/306,499 with the same two unresolved handlers and every previously
passing section retained. Rework remains uncommitted.

All Cargo commands run offline. Package tests use two jobs; the large
semantic build uses one job and `-C debuginfo=0 -C codegen-units=16` for
that test executable. Both environments set `RUST_MIN_STACK=16777216`;
MAIN sets `ITGMANIA_SONG_LUA_WORKSPACE=C:\GitHub\rework`. These build
settings do not change test selection or comparison tolerances.

## Remaining scope

Whole-song parity remains unfinished while `RyukoCheckP1/P2` require an
outgoing broadcast chain. The compile diagnostic remains visible.
Interactive minigame input callbacks and scheduled queued broadcasts also
need direct engine/reference validation; a clean no-input semantic trace
cannot establish interactive or audible parity.

## Frozen chart identities

The 63-chart inventory is unchanged: 58 exact local matches, three local
hash mismatches and two missing local charts. The user will edit these
identities later.

| Song | Frozen hash | Local hash |
| --- | --- | --- |
| Mawaru5 | `74765c1936186d20` | `8e0b6274cb33af5e` |
| Mawaru8 | `cadefe09888e9ab8` | `8224fb7e0b05040f` |
| Brain Power | `a73ec5f2f3015620` | `f40ebaf45ea6e26d` |

Get Into It (`9c208360a9b25133`) and Rhythm Hell
(`be38aa9e3c88c32b`) remain absent from the audited local songs and archives.
