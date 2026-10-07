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
| 5 | `257b7bb4bff4c6f447a19f3e4b179d73d033e328e7652a7bca9050c9b6d73fb5.tar.zst` / `100 Bad Days/100 Bad Days.ssc` | Missing movie prevented compilation | 174,897/174,897 | `0.5.1827` |

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

## Pass 5: complete the Episode 16 SSC archive

The next indexed archive,
`87faee68a8915840ee0c94f1281cdf25eb814d53843a7823e8d3577aec1ed3c0.tar.zst`,
is the separate SSC simfile for Episode 16. It failed compilation because it
also omitted `lua/100BadDays.mp4`. The native movie dependency repair from
pass 4 applies to this archive too. Its replacement was captured with harness
`0.1.4`, preserving this simfile's original bytes and all 7,920 native update
frames over 131.98 seconds. Asset and archive hashes were verified; the old
archive is retained in Git history and local diagnostic storage, with its
selectors mapped to the replacement.

The complete replacement passes all 174,897 comparisons on DeadSync
`0.5.1827`. No additional source or harness change was needed. Its separate
commit records the corrected archive and this pass's patch-version increment.

Verification log SHA-256: `59b95d29493eb670a3581474179397f28ef1a3d2e27c38eaa754c966fb41daba`.

## Pass 6: preserve queued effects during tween replay

The original 10:35 archive,
`86d614310d0cfd0f681d6c7c2c52032448765c1d4ce31f95a2e0ed079c19742c.tar.zst`,
failed 920 projected-geometry comparisons. At beat 34.2667 its queued
InitPulse command applies a native scale near 1.01115, while DeadSync reverted
to the unpulsed size on the next tween frame.

ITGmania `src/Actor.h::TweenState` owns pose, crop, fade, and colors; the
actor's effect selector, clock, timing, and magnitude are separate fields.
`src/Actor.cpp::PreDraw` applies pulse to the temporary draw scale.
DeadSync's chronological message replay incorrectly restored the entire
old state every frame. It now preserves effect properties until another
command block writes them. The replacement removes that repeated reset;
it does not change pulse math or fixture comparison tolerances.

The capture path was also checked: the harness's `_ITG_PULSE_ZOOM` calls
native `Actor::SetEffectPulse` and `Actor::PreDraw`. Its independent
`late_pulse_matches_native` check passes. Simply Love's gameplay overlay
contains no pulse override for this custom song foreground. No harness or
archive change is required. A new compiler regression verifies pulse mode,
clock, magnitude changes, and explicit stop across concurrent startup
tweens; all 466 song-Lua unit tests pass.

The complete original archive passes all 248,156 comparisons on DeadSync
`0.5.1828`.

Verification log SHA-256: `f41303e1ce2fd3ff22006f223349c40743896989dbca119043dc871758a86a4f`.

## Pass 7: complete ChikuTaku's movie archive

The old archive `a3792891b461050fa08a726beb23cab5ad5fc508002adf27a482df1927ad3bb2.tar.zst`
omitted the movie loaded twice by `lua/default.lua`. The source movie is
68,632,900 bytes. ITGmania's `ActorUtil.cpp::InitFileTypeLists` classifies MP4
as a Sprite texture; the pass-4 harness fix therefore collects this required
dependency without any new harness code. Harness `0.1.4` recaptured all
13,308 native frames through the 221.778-second endpoint with no errors or
dropped events. Member hashes and the original source bytes were verified.

The replacement `4d492047030008443585021530eb446c0ef3e87a6eb74f24467777927dd342d1.tar.zst`
passes all 328,107 comparisons on DeadSync `0.5.1829`. Existing parser and
playback behavior passes without further changes. Old archive selectors
still resolve; superseded bytes remain in Git history and local diagnostics.

Verification log SHA-256: `37364c9494ad0e6f9c8765aaa62ce77aed5d7b1015decc9c6acffabbd62b45f3`.

## Archive size policy (after pass 7)

The user requested excluding files over 100 MB from Git. Both complete
Episode 16 archives are 129.9 MB and now remain local, with explicit ignore
entries and `local_only` metadata. Their source-backed recaptures and full
comparison results remain valid. Only this session's unpublished commits
were rewritten to remove these blobs from pushable history; the published
parent and each repair's version and message were preserved. Both local
archives remain available for all comparisons in this workspace.

## Pass 8: give runtime readers one schedule and use native song clocks

The original Spooky archive `bc2abb30e2bafa458fec4c897ed1d1b38659b7a6566b747ca890d888b28bfd15.tar.zst`
failed eight comparisons. Four player-transform samples exposed speculative
prefix callback probes leaving a bogus value of one. Four alpha samples
exposed a duplicate fade: an analytical message at beat 273.5 preceded the
reader's actual dispatch at 273.50555. ITGmania's ActorFrame update order and
message dispatch advance a later child once on the dispatch frame. DeadSync
now lets the recurring prefix reader own its callbacks, perframes, modifiers,
and actions. The replaced partial filtering and duplicate static schedule
are removed. A regression verifies that callback probing does not invent
player values and that a reader action produces only its actual message.
All 467 song-Lua unit tests pass.

The capture clock was independently audited against ITGmania
`src/SongPosition.cpp::UpdateSongPosition` and
`src/TimingData.cpp::GetBeatInternal`. Both use native float arithmetic even
for BPM-only maps. The harness had bypassed native timing for those maps.
Harness `0.1.5`, commit `0a7b4cf`, removes that bypass, reports
`native-song-timing`, and passes 162 tests with five ignored, including a
new no-pause float-clock regression. Synthetic actor inputs without a
simfile keep their existing fallback. Simply Love reads the engine song
position; it does not supply a replacement double-precision clock.

Recapturing Spooky with that clock exposed two further player comparisons:
frame 6435 is at native beat 250.24998474121094, below the 250.25 callback
boundary. DeadSync had the same BPM-only bypass. Every Lua song now retains
its global native timing, independently of chart timing. Song cache version
28 invalidates omitted clocks; a simfile regression checks both sides of
this exact boundary. All 198 simfile unit tests pass.

The complete replacement `9930bef5335a264f7182e991bb690ca96dc6b5ed63a17e942166ce82c1d08d7f.tar.zst`
passes all 157,678 comparisons on DeadSync `0.5.1830`. No comparison tolerance
was changed. Four earlier captures in this batch also used the legacy
clock and were source-preservingly recaptured with harness `0.1.5`:

| Song | Current archive | Comparisons |
|---|---|---:|
| Warp Zone | `e57c20353ce651300e3e8edf2ac6c7a762a1a998315c4152fb8c56cc8e211b34.tar.zst` | 212,220 |
| Let Me Hear That | `d3f9acbe26ef5ca7324dd648c807263ad082b3965a73da8f9fc40c395a1a8bd3.tar.zst` | 205,071 |
| Waltz Capriccio | `5b699a3ba33f296be051db2b9f54e2b3c1ecd0326519fdf5caaa15332d2d6837.tar.zst` | 260,300 |
| 10:35 | `150693f162462dd89173d5e3b8b49db858d7956a2665eff50cc740db24409aef.tar.zst` | 248,156 |

Each complete capture passes, with original source and member hashes
verified. Both Episode 16 archives and ChikuTaku already used native timing
and remain on their recorded harness `0.1.4`. Old selectors resolve to
replacements; superseded bytes remain in Git history and local diagnostics.
Earlier pass logs document their historical references, while this table
records the current native-clock fixtures. Legacy clocks in other, unverified
archives remain a reason for future source-backed regeneration.

Verification log SHA-256: `23e4e97c4c7886af55787769d8dff0b990e4d23bfcc72f62f1bcd76e97672b28`.

## Pass 9: refresh Riddle with native song timing

The original Riddle archive
`dd2642a3338bc3f53179bbbe4a55ba44db0369dc1ab8f5b4c3f30471477b3132.tar.zst`
failed eight player-modifier comparisons, including mini at beat 152,
stealth at beat 40, and tornado at beat 56. It used the harness's legacy
continuous double clock. The pass-8 native timing correction applies:
ITGmania `SongPosition.cpp::UpdateSongPosition` takes its float song position
from `TimingData`, and Simply Love reads that engine position.

Harness `0.1.5` recaptured the complete song through native beat
300.0000305175781 at 140.625 seconds, with no errors or dropped events.
Original source and member hashes were verified. The replacement
`45be98118e8a0ab0db2cfb017f52de2475650620cda25a3639686f424f9e900c.tar.zst`
passes all 201,471 comparisons on DeadSync `0.5.1831`. The changed sample
count reflects actual native callback writes; no playback code or
comparison tolerance was changed in this pass.

Verification log SHA-256:
`4f6c2ee19fd4f5c978ad205d5eaa885002ee8920968b5331ba13eab3d0780cf2`.

## Pass 10: refresh Crystal Access's easing boundary

The original Crystal Access archive
`5c254c50d281cc55f60385b7a991560390c2f26ca13cd740395a2042ada60fe2.tar.zst`
failed two drunk-modifier comparisons at beat 84: the reference was -1,
while DeadSync produced -0.9944757. The authored Lua uses an outCirc easing
function starting at beat 84, with a 0.5-beat duration. Its BPM is 160 and
its offset is -0.588 seconds.

The old harness recorded exactly beat 84 at 31.5 seconds using its legacy
double clock. ITGmania `SongPosition.cpp::UpdateSongPosition` stores the
float beat computed by `TimingData.cpp::GetBeatInternal`; the fallback Lua
`GameState:GetSongBeat` alias reads that position. The native harness now
records beat 84.00000762939453 on the same frame. The pass-8 corrections in
both implementations therefore close this gap without changing easing or
comparison tolerance. Simply Love consumes the same engine song position.

Harness `0.1.5` recaptured all 8,641 frames through beat 384 at 144 seconds,
with no errors or dropped events. Source bytes, archive members, and
lossless level-22 normalization were verified. The complete replacement
`a03a6bec25440238c8d3fa0206e4ab165058e50a4640f252fadbd57d30e5f48d.tar.zst`
passes all 194,374 comparisons on DeadSync `0.5.1832`.

Verification log SHA-256:
`359dab369c354fc8dd287c71e63f2ebe2f01bb20822183410161678470ed7ea1`.

## Final batch verification

All ten repaired archives were replayed and composed again on DeadSync `0.5.1832`. Every archive and member was validated before comparison. The batch passes all 2,157,171 comparisons, with zero failures. This includes both complete local-only movie archives.

| Song | Comparisons |
|---|---:|
| Warp Zone | 212,220 |
| Let Me Hear That | 205,071 |
| Waltz Capriccio | 260,300 |
| 100 Bad Days (SM) | 174,897 |
| 100 Bad Days (SSC) | 174,897 |
| 10:35 | 248,156 |
| ChikuTaku | 328,107 |
| Spooky | 157,678 |
| Riddle | 201,471 |
| Crystal Access | 194,374 |

Final test executable SHA-256: `67323b080ad11958cc0fbcb595ed0bf8839642372ccad01287b8088fc1e91e68`.
Local verification receipt SHA-256: `2b633fe1655a70026f2683fb396771091f383ff6cf7e147d7e4194e4db926fc6`.

The consolidated reference-resolution test and the selected Crystal Access archive integrity test also pass. Earlier source checks passed all 467 song-Lua, 198 simfile, and 162 harness tests (five harness tests ignored); source code did not change in passes 9 and 10.

An unfiltered archive-integrity check still stops on its existing nonempty Lua-closure assertion. The corpus includes recovered charts without Lua, documented in the capture audit. This separate validation gap and the 18 uncaptured charts remain outside the ten repaired archives; this batch does not establish parity for the entire corpus.

The final outgoing-history audit finds no Git blobs at or above 100,000,000 bytes. Both 129.9 MB Episode 16 archives remain present with matching content hashes, ignored, and untracked. The upstream is an ancestor of this branch, so these commits support an ordinary push. No push was performed by this repair.

## Full corpus sweep (restarted 2026-10-07)

The next goal restarts at the first archive and continues beyond ten repairs.
All 492 stored archive variants, including local-only movies, are in scope.
Completion requires a full final sweep, source-backed reference clocks and
dependencies, and verification of the uncaptured source charts. Missing or
invalid inputs remain outstanding until native behavior can be established. Passing comparisons against a legacy double-clock capture do not
establish correct native timing; those references also need regeneration.
The earlier empty-Lua validation limitation remains pending in this sweep.

## Pass 11: tween auxiliary state in Stronger

The original Stronger archive
`c537253c06b17440ec88674bc99aced4c251d1dc08beacac7cf5e864087b46a8.tar.zst`
failed 74 comparisons. Recapturing with harness `0.1.5` removes 66 player
transform differences caused by the legacy clock, leaving eight projected
geometry failures. At beat 445.306 the black-hole Sprite collapses to a point
in DeadSync while ITGmania still draws its accelerated fade-out in scale.

ITGmania `Actor.h::SetAux` writes `DestTweenState().aux`; `GetAux` reads
`m_current.aux`. `Actor.cpp::TweenState::MakeWeightedAverage` interpolates
that float alongside position. The source's BlackHoleLeave command queues
an accelerated aux transition on another Actor, and its update callback
reads that current value to set the Sprite zoom. DeadSync had treated aux
as an immediate scalar outside its tween state.

Aux now uses the existing captured delta, tween queue, getter, update,
and playback paths. The replaced per-message scalar, auxiliary snapshot
index, and separate capture merging are deleted. A regression reads an
intermediate value during a queued aux tween. All 468 song-Lua unit tests
and 182 playback tests pass (three existing playback tests ignored). The
17 profile-gameplay song-Lua tests also pass. The playback projection test
also resolves its previously moved Step Your Game
Up trace through the compressed fixture index; the reference bytes and
comparison remain unchanged. Simply Love does not override Actor aux.

The complete native replacement
`73da04c58a35f1e4a8246b4e9d7e9deddb2e4f3a29e2fc38b83fa4f63dd9c48f.tar.zst`
passes all 208,584 comparisons on DeadSync `0.5.1833`. The 8,448 frames reach
beat 481 at 140.78048706054688 seconds, with no runtime errors or dropped
events. Original source and member hashes and lossless recompression were
verified. No comparison tolerance changed.

Verification log SHA-256:
`3422aa64da3dab38d31904963e15bdfad855cb6084db273e9590e9d2648c6276`.

## Pass 12: refresh BroGamer with native song timing

The original BroGamer capture failed ten comparisons: confusion offset,
tiny, and player rotation at tween boundaries. Its continuous double clock
bypassed ITGmania's float `TimingData` song position. The native timing
correction from pass 8 applies here; `SongPosition::UpdateSongPosition`
and Simply Love consume the engine position without replacing its clock.

Harness `0.1.5` recaptured the complete song through beat 440 at
127.53623199462892 seconds, with zero errors or dropped events. Original
simfile bytes, member hashes, and lossless level-22 recompression were
verified. The replacement
`fb753c9cf62a0d6daf502468ec752d5a22809b14a5c0f835e2af3dfd699bedfe.tar.zst`
passes all 198,090 comparisons on DeadSync `0.5.1834`. Playback code and
comparison tolerances did not change in this pass.

Verification log SHA-256:
`826e78dc5ecca7a2aa7e74a9cafc56142f588ffe6f73ffafcfbcf7555bb42d35`.

## Pass 13: repair native hibernation (DeadSync repair pending)

The initial native-timing recapture of Nishi-Shinjuku removed missing movie
assets and four player-rotation differences, leaving five final visibility
differences. Source inspection found a further reference defect: the harness
only hid hibernating actors from drawing while continuing their tweens,
effects, children, and update callbacks.

ITGmania `Actor.cpp::Update` subtracts the hibernation timer as native floats,
returns while it remains positive, and supplies the leftover delta on the
wake-up frame. `Actor.h::GetVisible` is independent of that timer, while
`ActorFrame.cpp::GetTweenTimeLeft` includes its sleep and child queues.
Simply Love hibernates its engine HUD through these same Actor methods.
Harness commit `74cf6c3` removes its draw-only deadline and calls linked
`Actor::Update` for that phase. The paused tree, current aux, child alpha,
callback delta, visibility, and queue-time regression passes. All 163
harness tests pass, with five existing tests ignored; release version is
`0.1.6`.

The complete corrected Nishi-Shinjuku candidate reaches beat 290 at
102.42857360839844 seconds with zero runtime errors or dropped events.
Its source bytes, members, and lossless normalization are verified, but
DeadSync `0.5.1834` emits no composed actors despite visible native geometry.
The candidate remains unindexed until the production hibernation gap is
closed and verified. No comparison was removed or relaxed.

Corrected candidate:
`75ab9a044b221eae9f6d2abf2c8fd119072f737c9ff47d8cef44edf3bc3f3582.tar.zst`.
Diagnostic log SHA-256:
`25f8d2e37fdde6c7a0de0c82e0dfebd3d2c2fb9a69700d11f6ad7f52a51ef745`.

## Pass 14: enforce native references and handle empty Lua layers

The audit of all 492 indexed and retained archive variants found 388 legacy
continuous clocks, at least 21 positive hibernation captures made before harness
`0.1.6`, 53 captures without update frames, and 18 genuine empty Lua closures.
These sets overlap; at least 398 archive variants require reference regeneration at
that audit snapshot, before the Bank Account repair below. Passing replay
comparisons against those references cannot establish engine parity.

The whole-song runner and integrity gate now reject obsolete clocks,
unpaused hibernation captures, native runtime errors, dropped events,
and missing or incomplete frame coverage. Trace and manifest capture
versions must agree. Synthetic Lua-only micro fixtures retain their own
clock because the gate applies to full-song archives.

Empty closure validation requires the parsed simfile to have no foreground
or background Lua layers. Every nonempty closure member must remain in the
hash-validated member list. The production compiler already returns an
empty layer list for an empty input; the runner now accepts that result,
keeps the native root-count check, rejects uncompiled native actors or
commands, and still compares player and modifier state. No placeholder
compiled layer is created. This matches `Foreground.cpp::LoadFromSong`,
which instantiates only the song's referenced foreground changes.

A complete harness `0.1.6` Bank Account capture replaces its obsolete clock
reference. The archive
`cc5b053ddbb5ff0166a3d531326bd07ff6bfb9f82c1f9cce4e7cf36a90fcc494.tar.zst`
passes all 116,603 comparisons on DeadSync `0.5.1835`. A missing-root
countercheck fails, and separate negative checks reject each obsolete
reference condition. Source bytes, members, and lossless recompression
are verified.

All 17 other empty-Lua charts were then recaptured, hash-validated, normalized,
and individually verified on the same executable before publication. They
pass 3,638,158 comparisons; together with Bank Account, all 18 pass 3,754,761.
The Warp Zone control still passes all 212,220 comparisons through the stricter
reference gate. The two new regression tests pass.

Empty-Lua batch receipt SHA-256:
`4e85179385a9e2ca1607339673dbf066a6158f3d3643a38fca50aaf9ed93980a`.

Final Bank Account verification log SHA-256:
`82af3da8a421bb53a8fbef07947d4cc96b9c5c9e15f7af0b1659f90d842c137e`.

## Pass 15: refresh The Shadow with native song timing

The Shadow's original continuous-clock reference passed replay comparisons,
but could not establish timing parity. Its complete harness `0.1.6` capture
uses the linked ITGmania `TimingData` position through beat 338 at 130 seconds,
with zero runtime errors or dropped events. Source bytes, member hashes, and
lossless level-22 recompression are verified.

The replacement
`db0f05f4ae045a58688bccfcdfcaba9bdf415081af2d8eb5d998a5a9fbbfe31f.tar.zst`
passes all 193,506 comparisons on DeadSync `0.5.1836`, including the native
reference validation gate. No playback code or comparison tolerance changed.
Nishi-Shinjuku's production hibernation failure remains pending; this result
does not establish parity for the rest of the corpus.

Verification log SHA-256:
`826d4d12656e072e43e358a014e9c24a6c24d9c9b179cfb1cc510533dd5bc92c`.
