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
| 3 | `e37dbafcf2ebfb7b60b054ff2f640ede4df9a4832dfda309d56fd3d6967b2908.tar.zst` / `(R6) Waltz Capriccio/waltz_capriccio.ssc` | Missing hinted overlay asset prevented compilation | 260,292/260,292 | `0.5.1825` |
| 4 | `54d5c782bdbdf43c6fd23825be53039fd343ad7db5ca37464d24f1bd1e99bf4e.tar.zst` / `100 Bad Days/100 Bad Days.sm` | Missing movie; rounded song offset delayed eight modifier checks; numeric prefix uncovered | 174,897/174,897 | `0.5.1826` |

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

## Pass 3: preserve hinted assets and harness provenance

The old archive `c05dc759b9709606a61148a0ea1b6e59a69eb17bc706e2105f02ab5283d2424d.tar.zst`
omitted `lua/overlay2 3x4.png`, although the unmodified source calls
`LoadActor("overlay2")`. ITGmania `src/ActorUtil.cpp::ResolvePath` appends `*`
after an exact miss. DeadSync already implements this lookup; the harness
archive collector required an exact stem and dropped the hinted filename.
The collector now uses the native prefix rule and refuses ambiguous matches.
Its regression test verifies the dependency list, hinted file, exact path,
and ambiguity behavior.

The replacement capture includes that asset, zero runtime errors and dropped
events, and 8,462 update frames across 141.01 seconds. The original capture
stopped at 107.81 seconds and contained no recorded update frames. The native
timing capture therefore restores coverage as well as the missing dependency.
All 260,292 comparisons pass, including 67,306 modifier and 186,165 player
transform checks. Source bytes, member hashes, and the full endpoint were
verified before publishing. The obsolete archive is preserved in Git history
and local diagnostic storage; its selectors resolve to the replacement.

Harness `0.1.3`, commit `c494528`, also writes `harness_version` into each
archive index entry from the generating semantic manifest. All existing
483 primary entries were backfilled from their own archive manifests:
482 initially reported `0.1.0`, and Let Me Hear That reported `0.1.2`.
Waltz Capriccio's replacement reports `0.1.3`. Retained archives expose their
own manifest version. The runner prints and validates the provenance without
altering existing archive payloads solely for this metadata addition.

The harness suite passes 160 tests with five ignored tests. Targeted archive
and CLI checks also verify provenance after adding the index field. DeadSync
version is `0.5.1825`; the complete fixture log is
`target/song-lua-archive-passes/pass-03-verify.log`.

Verification log SHA-256: `ab8c4dca4770e9a2001fd29389c069f0a870c909426c97c4d2073762df510a3f`.

## Pass 4: retain movie assets and authored song offsets

The old archive `cd076a0f887e93f2ec4e6e54d4065943c984a99c750218ab1b2abe4e7dc4d7f4.tar.zst`
omitted `lua/100BadDays.mp4`. ITGmania `src/ActorUtil.cpp::InitFileTypeLists`
classifies eleven movie formats as Sprite textures. The harness collector
now retains these required texture assets, including exact and extensionless
references, and reuses captured dimensions before probing disk. Harness
`0.1.4`, commit `275a0ff`, passes 161 tests with five ignored tests, plus
checks covering every supported movie extension. The new archive preserves
the original movie bytes and all 7,920 native frames over 131.98 seconds.

That complete archive exposed ten modifier gaps. Eight came from runtime
song metadata using RSSP's report-rounded `0.066` instead of the authored
`0.065760` offset. ITGmania `NotesLoaderSM.cpp::SMSetOffset` stores the
original float; its `TimingData.cpp::GetBeatInternal` evaluates float song
position. With the original offset, frame 25 crosses beat one at
`1.0000001192092896`. Report rounding made DeadSync wait until frame 26.
Runtime metadata now reads the original global offset through RSSP's parser;
report formatting remains RSSP's responsibility. Song cache version 27
invalidates offsets stored by the previous loader. A split-timing regression
checks the original song offset, separate chart offset, and exact native
beat-one frame. All 197 simfile unit tests pass.

The remaining two gaps involved `*50 30+0% beat`.
`PlayerOptions.cpp::FromOneModString` calls `StringToFloat`, whose `strtof`
accepts the numeric prefix `30`. DeadSync's amount parser and the independent
reference audit now accept that prefix. Numeric and Lua-level regressions
verify the returned 0.3 target; all 465 song-Lua unit tests passed during
this repair. The two unsupported-write sentinels become normal option probes,
merged with same-frame writes under the runner's existing last-write rule.
No tolerance or required comparison was relaxed.

The complete archive passes all 174,897 comparisons. Warp Zone, Let Me Hear
That, and Waltz Capriccio also pass on this parser change. Obsolete archive
bytes remain in Git history and local diagnostic storage, with old selectors
preserved. DeadSync version is `0.5.1826`.

Verification log SHA-256: `f436bf2aa9dc0c73692ec6609902fac31f99e6f709189a65dcb898bb8a561caa`.
