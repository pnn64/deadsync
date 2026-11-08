# Queued Lua messages

`Actor:queuemessage` previously recorded a broadcast immediately and never
dispatched its receivers. The Lua reference host also skipped this method,
so previous whole-song comparisons could miss the same behavior on both
sides.

DeadSync now appends messages to the existing actor tween queue using
ITGmania's `!` command marker. When the queue item begins, it calls the
installed message manager. Sleeps delay dispatch, queue controls can cancel
pending messages, and broadcasts use the actor's replay frame. The immediate
recording stub is deleted. Commands and messages share the enqueue logic.
Even zero-duration startup messages wait for the first positive frame.

The native oracle now links the local, unmodified `MessageManager.cpp`.
The old no-op dispatcher is deleted. Its adapter exposes subscriptions,
queued messages, immediate command state, cancellation, and broadcast history.
The Lua host dispatches messages at the same queued tween boundary.

## Independent native evidence

The small `queued-broadcasts` fixture uses a sender, receivers before and
after that sender, and an observer of a startup message's Lua state. At 60
FPS, native ITGmania dispatches Zero on frame 1, Hit on frame 13, Again on
frame 22, and Cancel on frame 37. Never does not dispatch. On the Hit frame,
the early receiver is still at x=160 while the later receiver has advanced
to x=453.3333435058594. The startup observer is x=0 at frame zero and x=50
on frame one.

The unchanged DeadSync runtime failed the new test with native x=50 versus
DeadSync x=0 on frame one. The corrected implementation passes 4,344 native
position and color comparisons across all 181 frames, three exact broadcast
frame checks, cancellation, and 259 semantic observations. Position tolerance
is 0.0001 and color tolerance is 0.000001; existing semantic tolerances are
unchanged. Compressed captures, input, and source hashes are retained under
`tests/fixtures/itgmania-song-lua-micro/queued-broadcasts*`.

## Whole-song audit

The reference host also indexed unnamed children by their definition IDs.
`GetChild("")` therefore returned a synthetic external actor. Native
`ActorFrame::GetChild("")` finds the real child. Fixing the host index makes
the Ryuko character darkening and stopped animation target the correct
sprite. The `unnamed-broadcast` fixture compares 488 exact native RGBA
values over 61 frames and retains all 51 semantic observations.

Both small fixtures use the linked native C++ implementation, independently
of the Lua host. The local ITGmania source tree remains unchanged. The
harness passes 139 selected tests.

Against the same corrected full Mawaru8 trace, the old MAIN 0.5.1750 binary
reports **328,358/332,010**, with **3,652 failing checks** and 376 detailed
gaps. The corrected runtime reports **332,029/332,029**, with no skipped
message handlers or unsupported function actions, eases, or perframes.
It compiles 464 messages rather than the old binary's 432.

| Corrected full-song section | Passing observations |
| --- | ---: |
| Compile information | 8/8 |
| Layer order | 3/3 |
| Final render | 1,088/1,088 |
| Render persistence | 3,150/3,150 |
| Update values | 6,854/6,854 |
| Player ranges | 10/10 |
| Projected geometry | 100,720/100,720 |
| Draw colors | 189,824/189,824 |
| Projected vibration | 29,748/29,748 |
| Timeline | 149/149 |
| Message commands | 221/221 |
| Runtime modifiers | 254/254 |

The complete compressed trace and loaded Lua source hashes are retained as
`mawaru8-queued-messages*` fixtures. The regular full-song test requires all
332,029 observations. The capture has 143,193 recorded events, no runtime
errors, and no dropped events. Existing comparison tolerances are unchanged.
Comparison totals also include compiler diagnostics and checks that become
available when receivers and visible geometry are restored. There are 18
additional geometry checks and two additional timeline checks; the compiler
diagnostic total drops by one. Existing checks and tolerances are unchanged.
The earlier 306,497/306,499 audit
used a host that omitted queued broadcasts and cannot establish this result.

Both repositories pass **1,091 selected regression tests**: 783 unit,
174 playback, two AMV, 115 regular semantic, and 17 profile/gameplay tests.
The full Mawaru8 regression and unchanged Mawaru5 regression run in the
regular semantic suite. The direct native micro-fixtures retain 4,344 queue
position/color checks and 488 unnamed-child color checks in addition to
their semantic observations.

Publication copies 17 curated source, fixture, and documentation files to
MAIN, plus Cargo.toml and Cargo.lock. MAIN advances exactly once from
`0.5.1750` to `0.5.1751`. Rework remains uncommitted. All Cargo commands run
offline. Both repositories use `RUST_MIN_STACK=16777216`; MAIN resolves
local song resources through `ITGMANIA_SONG_LUA_WORKSPACE=C:\GitHub\rework`.
The large semantic executable builds with one job and
`-C debuginfo=0 -C codegen-units=16`. These settings do not change selection
or comparison tolerances.

## Remaining scope

Zero-duration startup commands still perform static compiler discovery,
including proxy binding and action-table construction. Deferring every
startup command broke that discovery; this pass preserves it and corrects
queued message timing. General startup-command Lua-global timing requires
a separate compiler change. No-input comparisons also do not establish
interactive minigame input or audible sound parity.

The frozen 63-chart inventory is unchanged: 58 exact local identities,
three hash mismatches, and two missing charts. No files were downloaded.

| Song | Frozen hash | Local hash |
| --- | --- | --- |
| Mawaru5 | `74765c1936186d20` | `8e0b6274cb33af5e` |
| Mawaru8 | `cadefe09888e9ab8` | `8224fb7e0b05040f` |
| Brain Power | `a73ec5f2f3015620` | `f40ebaf45ea6e26d` |

Get Into It (`9c208360a9b25133`) and Rhythm Hell
(`be38aa9e3c88c32b`) remain absent from the audited local resources.
