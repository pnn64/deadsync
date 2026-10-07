# Full-song archive parity passes

This batch starts from DeadSync `0.5.1822` and commit `675b17142`.
The fixture runner lists 492 archives: 483 primary captures and 9 retained
historical versions. Archives are scanned in index order. Each repair pass
closes the next failing archive with a complete native comparison, one
DeadSync patch-version increment, and a separate commit. A harness change
receives its own patch-version increment and commit when required.

Archive bytes, song inputs, comparison tolerances, and comparator coverage
remain unchanged. Results describe the complete fixture runner's exercised
semantic, composition, modifier, and transform checks, not pixel output from
an interactive ITGmania process. Local ITGmania sources are the reference.

| Pass | Archive / simfile | Initial failure | Verified result | Version |
|---|---|---|---|---|
| 1 | `fc4f528d0fe0b68e691c222b2b2d58d8deb0f10d938f6eac3f57533cb06bb7ff.tar.zst` / `(R10) Warp Zone/warp zone.ssc` | 10 player-transform frames | 212,220/212,220 | `0.5.1823` |

## Pass 1: preserve sampled transform time

The archive's own compiled Y sample at frame 5521 equals the native value
`514.8974609375`. Playback returned `514.90106` after converting the rounded
beat `196.30222` back to time and interpolating towards the next frame.
The other nine differences have the same cause. Split chart timing kept
these samples on the beat path even though the reference callback clock
was continuous BPM.

ITGmania `src/Actor.cpp`, `Actor::UpdateTweening` subtracts float seconds
from each queued tween. `Actor::CalcPercentThroughTween` evaluates its
position from that remaining time. Replaying the native float arithmetic
for Warp Zone's chained 1.875-second tweens reproduces `514.8974609375`.
DeadSync now stores sampled player and column transforms on the original
seconds clock, using the existing BeatClock translation for continuous-BPM
contexts. The old beat conversion was removed from this capture path.

Verification command:

```powershell
cargo test --test full_song_lua fc4f528d0fe0b68e691c222b2b2d58d8deb0f10d938f6eac3f57533cb06bb7ff.tar.zst
```

Native duration and every recorded frame are exercised. The final report
contains 206,691/206,691 player-transform checks and 212,220/212,220 total
checks, with zero failures. No fixture recapture or harness change was needed.
The complete output is kept locally in
`target/song-lua-archive-passes/pass-01-verify.log`.

Verification log SHA-256: `b2a16647072aa000a5a65189eb3a470157bc600fd934a20ba590bc09230e5392`.
The song-Lua package unit suite also passed all 465 tests.
