# Full-song archive parity passes

This batch starts from DeadSync `0.5.1822` and commit `675b17142`.
The fixture runner lists 492 archives: 483 primary captures and 9 retained
historical versions. Archives are scanned in index order. Each repair pass
closes the next failing archive with a complete native comparison, one
DeadSync patch-version increment, and a separate commit. A harness change
receives its own patch-version increment and commit when required.

Song inputs and comparison tolerances remain unchanged. Source-backed
recaptures are recorded explicitly below, with their replacement hashes.
Comparator coverage is preserved or expanded. Results describe the complete fixture runner's exercised
semantic, composition, modifier, and transform checks, not pixel output from
an interactive ITGmania process. Local ITGmania sources are the reference.

| Pass | Archive / simfile | Initial failure | Verified result | Version |
|---|---|---|---|---|
| 1 | `fc4f528d0fe0b68e691c222b2b2d58d8deb0f10d938f6eac3f57533cb06bb7ff.tar.zst` / `(R10) Warp Zone/warp zone.ssc` | 10 player-transform frames | 212,220/212,220 | `0.5.1823` |
| 2 | `b92716e36a8b1e460e7d52ce3530bf7a4dbd92ff2843b04bb2672bce56b37bbc.tar.zst` / `(R5) Let Me Hear That/let me hear that.sm` | 382 modifier checks in obsolete capture; AFT output omitted by composition adapter | 205,075/205,075 | `0.5.1824` |

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

## Pass 2: recapture native timing and count AFT output

The original archive `ded3626a5b627270035ff26730920fbc17f4bf8187666176e747adb54b178b80.tar.zst`
recorded no native update frames. Its recorded source revision,
`54e62e5ffedf406a1713ec55a1da3dcecf50817c`, resolves in the rework history to
`itgmania-harness-rs/semantic_host.lua`. That host advanced by quarter beats
and drained queued callbacks by absolute expiry, without native tween
remaining-time arithmetic. Its 382 modifier differences therefore cannot
serve as evidence for changing DeadSync's frame-based evaluator.

The current standalone harness captures 8,707 update frames at 60 Hz over
145.09 seconds, with no errors or dropped events. The replacement archive
above preserves the source simfile and dependencies. Archive member hashes,
original source bytes, complete endpoint, and compressed hash were checked.
The obsolete archive is preserved in Git history and local diagnostic
storage, outside the runnable archive directory. Its selector aliases point
to the corrected capture. The primary archive count remains 483, with nine
additional retained historical captures.

This stronger reference revealed a composition adapter omission: gameplay's
AFT sprite path uses capture topology, whereas the headless whole-song actor
counter only called the ordinary image builder. ITGmania's
`src/ActorFrameTexture.cpp` and `src/Sprite.cpp` bind the created target as a
sprite texture. The counter now invokes the existing gameplay AFT builder;
it still requires drawable output and keeps every geometric comparator.
The complete capture passes all 205,075 comparisons, including 3,698 runtime
modifier and 191,555 player-transform checks.

Harness version `0.1.2` is committed as `57bfba1d73856c8c6480d4623d85df6cb63ca2f6`.
That version checkpoint follows verification of its existing native binding
repairs: 159 harness tests passed, with five explicitly ignored tests. No
new harness source changes were needed for this pass.

Final validation logs are kept in
`target/song-lua-archive-passes/pass-02-verify.log` and
`target/song-lua-archive-passes/pass-02-warp-regression.log`.

Final verification log SHA-256: `bc0de2ce3f89720c666ecd5dda3310786b9a8192d450e8cbe0f69bddcfc9a0f2`.
Warp Zone also passes all 212,220 checks on this version. The song-Lua
unit suite passes all 465 tests.
