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

## Pass 16: repair hibernating callbacks and native update rates

The linked `Actor::Update` and `ActorFrame::UpdateInternal` method bodies
match the latest local ITGmania reference byte-for-byte. The first checks
hibernation before wrappers, preserves a native float wake-up remainder,
and then enters the actor. The second reads its update rate after those
wrappers, multiplies a native float delta, advances children, and finally
runs its callback. Simply Love's ScreenGameplay metrics use `math.huge`
to hibernate engine HUD actors through this same mechanism.

DeadSync now applies that entry gate in both its recursive and compiled
callback paths. Sleeping owners block their wrappers, descendants, recurring
commands, and chronological motion updates. A waking owner passes its
remaining delta to wrappers before applying its current rate to its own
work and children. The compiler reads that rate after wrapper callbacks;
it no longer retains a precomputed product of ancestor rates. Regression
checks cover callback order, nested rates, a rate changed by a wrapper in
that frame, own visibility, indefinite sleep, explicit wake-up, and replacing
an existing sleep. Song cache version 29 invalidates earlier baked results.

The source crosscheck also found that the harness ignored `SetUpdateRate`.
Harness `0.1.7`, commit `aca10de`, retains the native float rate and applies
it after hibernation and wrappers. Its scaled fixture checks the resulting
parent and child alpha as well as callback deltas. The same fixture fails
two delta assertions on `0.1.6`, then captures the complete four-second
probe with no errors or dropped events on `0.1.7`. A further probe changes
the owner's rate from its wrapper during the wake-up frame and passes.
All 163 harness tests pass, with five existing tests ignored. DeadSync's
reference gate now rejects earlier captures with non-default update rates;
its positive and negative regression checks pass.

DeadSync `0.5.1837` passes all 470 song Lua unit tests, two overlay-state
checks, and 182 playback checks, with three existing playback tests ignored.
The stale-cache rejection test and 19 profile/gameplay checks pass.
The final sweep from the top passes the first 13 archives and all 2,757,351
comparisons on executable SHA-256
`5ecfab7cbd235a6387c5bcc44cbb442732b68d411b0d7ffee412f230cdf7e186`.
The next indexed archive is Nishi-Shinjuku's obsolete clock reference, which
the runner correctly rejects before compilation.

This repairs the callback scheduling phase, not the complete hibernation
model. Queued tween progress still needs an actor clock that pauses, and
the visibility capture workaround still needs replacement. The corrected
Nishi-Shinjuku archive still fails the whole-song composition assertion
on this executable and remains unindexed. No assertion or tolerance was
relaxed, and no failed archive was published.

The updated audit covers all 492 indexed and retained variants, including
serialized infinite hibernation. It finds 375 obsolete clocks, 22 pre-`0.1.6`
hibernation references, and 53 missing frame histories. These sets overlap;
at least 386 variants need regeneration. No stored variant records a
non-default update rate. A full corpus pass remains outstanding.

Audit SHA-256:
`74caaca889dc0070859b02ba1bb7cf15502718d44f3bafc7779edc11f8719d87`.
Song Lua verification log SHA-256:
`7f66f49f7fbc5ab5d52daa26fb4d821b452f0d632b0b8848a537825608078dfd`.
Nishi-Shinjuku failure log SHA-256:
`dbe27cfc067c2cba0f4c80e67943e202d97fee0f053230f8b06a72a244ae5db9`.
Top-of-corpus verification receipt SHA-256:
`a572568f61126544db8b227baec940df682ef56e60db84d02e1cfabbb2a29315`.

## Pass 17: pause startup tweens and preserve own visibility

DeadSync 0.5.1838 separates an actor's hibernation draw gate from its own
visible flag. The old visibility/sleep shortcut is deleted. Startup tween
samples now account for static ancestor hibernation and update rates, with
wrappers receiving the wake remainder before their owner's rate. Replay
restores every initial actor pose after speculative capture. Cache version
30 invalidates the previous baked results. Projected drawing checks use
this independent gate; GetVisible assertions and tolerances are unchanged.

The corrected harness 0.1.6 Nishi-Shinjuku archive
`75ab9a044b221eae9f6d2abf2c8fd119072f737c9ff47d8cef44edf3bc3f3582.tar.zst`
passes all 143,004 comparisons and replaces its obsolete primary capture.
It has complete native frames, zero runtime errors or dropped events, and
verified source/member hashes. It records no SetUpdateRate writes, so its
0.1.6 provenance remains valid under the 0.1.7 update-rate gate.

All 471 song Lua unit tests, two overlay tests and 182 playback tests pass
(three existing playback tests ignored). The stale-cache rejection and
19 profile/gameplay checks also pass. The source-order sweep passes the
first 14 archives and all 2,900,355 comparisons on executable SHA-256
`8c52364d861bb106080433d19c33ab6e54f5e9dccc91ea5f29367f948a607301`.
The next source is CO5M1C R4ILR0AD's obsolete continuous-clock archive.

A separate linked native 0.1.7 runtime probe hibernates an already active
aux/alpha tween. Its clean native archive has no runtime errors or dropped
events, but DeadSync passes 5,363 of 5,366 comparisons: the Lua freeze
assertion fails and rendered alpha finishes early. This proves a remaining
chronological tween-queue gap, beyond the repaired startup schedule.
Captured pure-message hibernation also still needs its actual timer request;
a draw-state boolean alone cannot replace that operation. Neither diagnostic
is published as a corpus reference.

The updated 492-variant audit finds 374 obsolete clocks, 21 pre-0.1.6
hibernation captures and 53 missing frame histories; the overlapping union
is 385 variants requiring regeneration. All 18 uncaptured simfiles and the
unresolved chart/theme contexts remain in scope. A full final sweep remains
outstanding.

Nishi verification log SHA-256:
`ad3fb99dc5cadc99d16116dc695690db2e8b55b9d2c0c6c1307df7557f64d68d`.
Top-of-corpus receipt SHA-256:
`6d3d22553cbe6742aed46def09d3df663210b6a1c1edf094183e959203c762b6`.
Reference audit SHA-256:
`88798465efbf2f72e4e4597b3ebf535bf55fe0dbaa8d3c2c751468a13f69abfd`.
Runtime probe failure log SHA-256:
`8c3956dec71ba9210d548b55160d29b1015ae4ea370666daad9636c4705e7a84`.

## Pass 18: replay live native queues and preserve Update(0)

DeadSync's startup discovery formerly ran future queued bodies, then restored
actor fields without restoring their Lua counters or callback installation
state. The linked Actor.cpp queue and ActorFrame.cpp update bodies establish
the required order: hibernation, wrappers, owner rate, queue advancement,
children, then the owner callback. ActorFrame::HandleMessage also propagates
ordinary queued commands to children even if the frame has no such command.

Chronological replay now owns each actor's actual float queue, including
callback-only frames. Commands execute after the head pose is advanced and
popped, and their replacement queue consumes the remaining delta. GetTweenLeft
uses the remaining queue; current-position getters and destination-state
getters preserve their distinct native meanings. Stop/finish clear the live
queue. Startup discovery's Lua locals, globals, sound probes and future
SetUpdateFunction installs are restored before replay. The real zero-delta
update runs before this discovery and its observed state survives restoration.
The old visibility/cursor shortcut, late zero-update pass, generic bounce
approximation and unused opt2 queue field are removed.

Native probes demonstrate these repairs. The new zero-callback archive failed
480 comparisons before the ordering fix and passes all 10,854 afterward.
Active hibernation and destination getters each pass 5,369 comparisons.
Native recurring-stop and fallback-curve/queued-child regressions pass. The
modern immediate-shadow reference uses native float song timing: its beat-3
sample is 2.999999761581421, where the old continuous clock was exactly 3.
The new shadow archive passes 3,738 comparisons and replaces the old micro
reference. Near-camera likewise passes all 9,497 whole-archive comparisons;
its refreshed micro regression passes 4,194 comparisons and all 816 separate
native actor projection checks, including exact depth bits. The latter oracle
uses a 64px Sprite, while the Lua fixture's zoom(16/15) replaces its unit Quad's
zoomto scale. Synthetic spin and multitap adapters now declare their recorded
size rather than relying on the default Quad size. No tolerance is widened.

Harness 0.1.8 also repairs serialization rather than rendering: raw native
render alpha retains NaN and both infinities through compression, then uses
typed JSON number tags. Old null alpha is rejected as a lossy reference.
CO5M1C R4ILR0AD's zero-duration division produces both signed infinities;
Actor::SetDiffuseAlpha stores the raw float. Its complete 0.1.8 replacement
passes 180,339 comparisons with zero native errors or dropped events and
replaces the old primary archive.

A further source check includes the Lua binding and C++ setter, not just the
update method. ActorFrame:SetUpdateRate rejects nonpositive floats; its C++
setter ignores NaN and accepts positive infinity. DeadSync now raises for
invalid arguments, preserves the previous rate for NaN, and retains positive
infinity. Harness 0.1.8 incorrectly stored NaN; 0.1.9 preserves the native
setter behavior. A source-backed probe failed in the earlier harness, then
captures cleanly and passes all 5,309 DeadSync comparisons on 0.5.1844. NaN
hibernation also bypasses the gate, as native Actor::Update tests strictly
positive remaining time. The archive gate rejects pre-0.1.9 NaN-rate captures.
Cache version 35 invalidates earlier baked results.

On 0.5.1844, all 473 song Lua unit tests, two overlay-state checks and 182
playback checks pass (three existing playback tests ignored). Harness 0.1.9
passes 164 tests with five existing tests ignored. The selected native queue,
shadow, projection, modifier-boundary, nonfinite-alpha and reference-rejection
regressions pass. The modifier-boundary test now captures enough song seconds
for 60 real-time seconds at its accelerated rates. BroGamer's idle-window
regression uses the parsed 1.167-second offset instead of rebuilding timing
with offset zero; it passes without changing its thresholds.

The source-order sweep on 0.5.1843 passes the first 18 archives and 3,729,375
comparisons, then stops compiling KABOOOOOM at its missing Center1Player
utility. That failure is not a song-clock rejection. One earlier Nishi
run failed five filesystem binding checks despite identical printed paths.
Three subsequent runs, including the original executable and the restarted
sweep, pass all 143,004 checks. This filesystem failure is not explained or
claimed fixed; failure messages now retain canonicalization results.

The broader semantic suite on an earlier 0.5.1843 executable reported 142
passed, 18 failed and 74 existing ignored tests. It used older standalone
references as well as the corpus. Some failures are repaired by the later
adapter, duration and BroGamer offset fixes; others remain pending. Publishing
Sharkmode during that run also invalidated its cached reference mapping; this
is a verification scheduling error, not a reason to weaken the mapping check.
A current full-suite result remains outstanding. Hundreds of obsolete corpus
captures, all 18 uncaptured simfiles, authored missing files/errors, pure-message
hibernation, and unresolved theme/chart contexts remain in scope. This pass
does not establish full song Lua parity.

Unit/playback log SHA-256:
`45727023d6358344fa1a2dd3846e708df29a4cc4174e78d6d6ba7f832658c82e`.
Harness test log SHA-256:
`032bdf27a9ab1ec13d3e10ac238e8c19c4582d445d27af9e86dac7483d7395f9`.
Rate-validation verification log SHA-256:
`e83715226ab9d4394f10ca67c8fc438ec1d6be80cc6b5e0914434208371d01cc`.
18-archive sweep receipt SHA-256:
`d18345107574d4d200e5ff22b82165fc1048153b8b37552d8d26f5db316a8a3a`.
Broad semantic failure log SHA-256:
`832e1ff278640fa157c1c1cf343c6aba6af84cac2b677649432094a527c28974`.

## Pass 19: refresh And Drugs with native song timing

The complete 0.1.8 capture reaches beat 265 at 127.20000457763672 seconds,
with zero native errors or dropped events. Its six members, source bytes,
SHA-256 digests and lossless level-22 recompression are verified. Archive
`04eb6389c65a08fc4711ea7e02466871c65681de0683aa26d1f4b81559bdd156.tar.zst`
passes all 182,791 comparisons on 0.5.1843 and replaces the obsolete primary
capture. The older source-order aliases remain mapped to this replacement.
Verification log SHA-256:
`29f81f9c038b1250a0348360a939978e7e44cd5f4c989ea89583e3ff5b6c282f`.

## Pass 20: refresh Karachi with native song timing

The complete 0.1.8 capture reaches beat 324 at 126.23377227783205 seconds,
with zero native errors or dropped events. Its seven members, source bytes,
digests and lossless recompression are verified. Archive
`0044b7fb565b42508bfa6b3df15a1c7759386e378f1bb579ccfdd955678ba9ad.tar.zst`
passes all 206,323 comparisons on 0.5.1843 before publication. Standalone
Karachi references from older captures remain separate pending work.
Verification log SHA-256:
`aab3fd606be438f9ecf34d93c3b8cb258e497ce0a654a6c53fc68ac49ad94d6d`.

## Pass 21: refresh Sharkmode with native song timing

The complete 0.1.8 capture reaches beat 357 at 138.1935577392578 seconds,
with zero native errors or dropped events. All 19 members and their original
source bytes are verified, as is lossless level-22 recompression. Archive
`e657c3259372c4b2925fcdc49177e99fcd0ce6db2ec78b30d6532eabfa10d119.tar.zst`
passes all 259,567 comparisons on 0.5.1843, including custom draws and manual
mesh bindings, poses and colors, before replacing the obsolete primary.
Verification log SHA-256:
`34baa0411478dc5990e8a78271d7e14c52319555d1113cd4b60d93b65fa3efae`.

## Pass 22: native noteskins, recurring text and option prefixes

The restarted 0.5.1845 sweep passes the first 18 archives and 3,729,375
comparisons. The fallback theme utility now implements Center1Player using
the checked-out style and preference rules. KABOOOOOM then compiles, but its
old indexed capture fails 921 comparisons. Its eight noteskin explosions
were placeholder Sprites rather than the actual Cyber actor trees. Returning
zero for native noteskin metrics also hid its real countdown colors.

Harness 0.1.10 loads native noteskin resources by default and rejects missing
resources instead of creating substitutes. Native NoteSkinManager is
initialized with a scoped dance Game, and both players select the requested
skin through the actual PlayerOptions binding. The temporary GameState
pointer uses its source-defined SetWithoutBroadcast API. The linked native
suite passes 164 tests with five existing ignored tests.

A complete replacement uses the same noteskin resource copy consumed by
DeadSync. An earlier attempt correctly failed dependency validation because
the bundled native PNGs and DeadSync's optimized PNGs have different hashes.
No resource check was relaxed. The shared-assets capture records 11,969
frames, 322 definitions, no placeholders, no runtime errors and no dropped
events; all 65 noteskin dependency hashes match. Its 26-member archive is
losslessly recompressed to 34,858,675 bytes:
`708efaf6d0a74ebfbbcad5e3ffe643400ccb8699c09e5058f43250a1a45bbcb5`.

DeadSync now bakes recurring BitmapText writes after the native-order actor
updates, including Update(0). The initial text is retained before speculative
queue discovery. Text changes use absolute song seconds when timing is
present, so changes during stops survive replay and seeking. Existing prewarm
includes every baked string; playback performs no formatting or buffer growth.
The native text regression checks all 24 recorded writes, including different
texts at the same stopped beat, and passes on 0.5.1847.

PlayerOptions.cpp reads every space-separated prefix and uses the final word
as the modifier. DeadSync now follows that order, including KABOOOOOM's
`0.8 0% Tipsy`, repeated approach speeds, `no`, millisecond values and invalid
trailing stars. A linked native probe verifies those cases. Float modifier
methods also use the existing native protocol for previous-value returns,
approach speeds, explicit chaining and error order. The old first-word helper
and duplicate partial float-method list are removed. One capability test's
implicit setter chain is replaced with separate calls, preserving its value
assertions. Song cache version 38 invalidates the earlier baked representation.

On 0.5.1847, 476 unit tests, two overlay tests and 182 playback tests pass
(660 total, three existing playback tests ignored). Seven existing focused
native controls and the new recurring-text control pass. Both native song-clock
cache round-trip and old-cache rejection checks pass.
A parallel profile run exits abnormally with STATUS_STACK_BUFFER_OVERRUN.
The serial rerun passes all 228 profile unit tests and three integration
tests; the crash's cause is not established or claimed fixed.

The full shared-assets KABOOOOOM comparison passes 1,605,529 of 1,663,506
checks and fails 57,977. All 79,044 multitap writes now match, including text
and colors; drawable membership and all 223 sprite textures match. Remaining
failures include shared message-state movement, effect geometry, eight spline
endpoints, four message checks and noteskin option audit coverage. The complete
text/prefix diagnostic separately passes 5,446 of 5,449 checks; its whole-song
modifier audit still rejects `.5 Drunk` and lacks Passmark coverage. Neither
diagnostic archive replaces an indexed corpus reference. The restarted
0.5.1847 source-order sweep passes the first 18 archives and 3,729,375
comparisons, then rejects KABOOOOOM's indexed placeholder noteskin actors.
The remaining corpus and final full-corpus verification remain outstanding;
the full 501-simfile/492-variant objective is still active.

The message-order investigation also finds a reference limitation. The song
semantic host broadcasts in tree order; MessageManager.cpp instead visits a
pointer-ordered subscriber set, and ActorFrame.cpp does not propagate broadcasts
to children. An independent actor-conformance input makes two native handlers
write the same child. Eight separate native runs produce both exact legal
outcomes (x=1 and x=2), while an unsubscribed actor never receives the message.
The harness preserves this input without a single golden result. KABOOOOOM's
shared `godir` needs native dispatch evidence; its current movement comparison
is diagnostic and must not drive a forced tree-order change in DeadSync.

Unit/playback log SHA-256:
`45cc68bad125d86a08a09f4776b8db8ddcf2ec4e002d7e3c9c62d96fc09cc90e`.
Harness native noteskin test log SHA-256:
`7b7e0096e5fdbee6ac9c0076c1ddc5815ca97a41dd0c17cc26c86fa24534edba`.
Native recurring-text verification log SHA-256:
`f00dae0c45be7de34814986d610182bee286b436f9799dfd9a19cbde74467f99`.
KABOOOOOM verification log SHA-256:
`a3996b24936175c42a90b6b80c02a1ab978422d3448a82201cd6234a711f26e5`.
Current source-order sweep receipt SHA-256:
`6f022e98aa103815905c66eb84cccfe277e5d16710669f0a97728b01b2982e76`.
Native subscriber-order receipt SHA-256:
`2b156684089deb653f2bcf8ebe5758facfe8bdc75946731ee2d022f743b9aea2`.

## Pass 23: actual native message dispatch and queued parameters

Harness 0.1.11 replaces the semantic host's tree traversal with subscriptions
to the linked native MessageManager. A bridge retains each standalone Lua
actor and callback, delegates Broadcast to the actual subscriber set, and
preserves the shared parameter table through nested broadcasts. Callback
errors are raised after the native dispatch unwinds. Every session unsubscribes
its actors before closing its Lua state. Captures record the native subscriber
pointer ranks and the actual recipient order for each broadcast. These ranks
describe that capture; they are not a universal order for other allocations.

Harness 0.1.12 additionally validates the native table-or-nil parameter contract
before recording an event. Rejected calls no longer appear as delivered
broadcasts. Its regression checks shared table identity, nested broadcasts,
dynamic and external actor subscriptions, queued messages, invalid arguments
and four consecutive session lifetimes. It does not assert a fixed tree order.
The complete harness suite passes 165 tests with five existing tests ignored.

Whole-song reference validation rejects older tree-order captures before any
semantic comparisons. Mutation checks reject missing dispatch evidence,
reversed recipient ranks, unknown recipients and duplicate ranks; an observed
child-before-parent native order is accepted. Native ActorFrame::HandleMessage
does not forward broadcasts to children. DeadSync's registry order is not
changed to fit a particular captured pointer allocation.

Native Actor::UpdateTweening only consumes queued messages with positive delta.
MessageManager's string overload constructs a Message with a new empty Lua
table. An independent linked actor input records x=[0,7,7,13,13] at times
[0,1/60,2/60,0.05,4/60]. The sender queues Startup, sleeps 0.05 seconds and
queues Later; a separate receiver changes x. Using separate actors matters:
setters on an actor with pending tweens change its queued destination instead
of necessarily changing its current state.

DeadSync now sends that empty table through both queued-message dispatch
paths and rejects invalid Broadcast parameter types before side effects.
Parameterized listeners execute while function actions are captured, since
their values cannot be retained in a named event. This also fixes a regression
where suppressing the probe's broadcasts discarded the scheduled queued
action entirely. The regression checks an empty parameter table and a retained
beat-2 trigger whose 0.1-second tween moves through x=0,6,12. Song cache version
39 invalidates the earlier baked behavior. Numeric/invalid broadcast names
and runtime actor userdata equivalence remain separate API audit work.

On 0.5.1848, 478 unit tests, two overlay tests and 182 playback tests pass
(662 total, three existing playback tests ignored). Both native song-clock
cache round-trip and old-cache rejection checks pass. The first larger parity
build fails with disk-full error 112. Cargo's package-scoped cleanup of
deadsync-shell and deadsync-theme-simply-love frees approximately 44 GB;
fixtures, captures and verification receipts are retained. The fresh parity
executable passes all seven selected native controls: reference rejection,
consolidated references, queued messages, queued child cycles, deferred effects,
recurring queue/camera behavior and recurring text. The affected whole-song
captures are verified below; the broader corpus is still outstanding.

Warp Zone's complete 0.1.11 capture has 9,395 frames, 15 actors and no errors
or dropped events. Its eight-member lossless archive is 489,572 bytes:
`06f189434a38499da35adc5ae226bb68f1c220b840792ff7045c7559c679c3b2`.
It passes all 212,220 comparisons on 0.5.1847 before replacing its obsolete
primary reference, then passes all 212,220 again on 0.5.1848.

KABOOOOOM's complete native-dispatch 0.1.11 capture has 322 actors, 11,969
frames, 151 broadcasts and zero errors or dropped events. Its 26-member
archive is 34,862,617 bytes:
`6ff3192ef021f890465cfdc5e3c404e354021f76ed3ca225cadaca416c472946`.
On 0.5.1847 it passes 1,605,586 and fails 58,035 of 1,663,621 comparisons.
It also passes 1,605,586 and fails 58,035 on 0.5.1848; the queued parameter
fix does not reduce this chart's gaps. It remains outside the published
fixture folder. Remaining failures include eight zoom spline endpoints,
shared message-state movement, effect geometry, five draw colors, four message
checks and 44 Cyber modifier audit writes. Native recipient ranks are evidence
of that capture's allocation, so the movement differences still require a
source-backed interpretation rather than forcing its particular order.
The full 501-simfile/492-variant objective remains outstanding.

Harness 0.1.12 test log SHA-256:
`d7a743d6cbc987477917c3aa1409872904722afe133ad204cc3884858c4fb5f3`.
Native queued actor input SHA-256:
`d84afebed136a287ea856e05fb1c67009cc9abe8d9d1d288458d0c62ec3904f0`.
Native queued actor output SHA-256:
`3fa1740fbfbf655d6195e60ff23877b7c3821b290f1b507f5b7247419e573251`.
Unit/playback log SHA-256:
`b1e2c4f69090ccd5d6eee3e1f621c48e003f8e4bc4cc89d26ce344d3e860af52`.
Queued action regression log SHA-256:
`c6137ff590e818ad0102e646e95ced23b6fe1b29fa204fab2e1a397b080e3417`.
Initial KABOOOOOM native-dispatch comparison log SHA-256:
`b5249eff184dc78c3ae93c84ae5f2befc55545a99d605862405c33ce776c59d3`.
Current Warp Zone verification log SHA-256:
`00d0ce7263ef129b49443ab7cecb964b9d59a49c2699b1239c2eaad2b6611a6f`.
Current full-song executable SHA-256:
`478769d4001d1b59ce384bc9b77398c630d8c45ba8e700d2c841bafc1b7b2275`.
Current KABOOOOOM comparison log SHA-256:
`c3f9b71bf118bc95960b0224937708e5baa69c72c1d9b669599da5c106b96295`.

## Pass 24: refresh Let Me Hear That with native message dispatch

The complete 0.1.11 capture reaches beat 266 at 145.09091186523438 seconds,
with zero native errors or dropped events. All 18 members, original simfile
bytes, digests and lossless level-22 recompression are verified. Archive
`b9b9ea7d862a66acc7e0c65f1f4cad273e8220d3d71f462d50602c5a9668f5fe.tar.zst`
is 768,132 bytes and passes all 205,071 comparisons on both 0.5.1847 and
0.5.1848 before replacing the obsolete 0.1.5 primary. Existing aliases remain
mapped to the replacement. Verification log SHA-256:
`6ad32ab71c625a54eee9eb71ca4921c2743249cc32cbe80a3b22303de183861b`.

## Pass 25: refresh Waltz Capriccio with native message dispatch

The obsolete 0.1.5 reference is rejected for tree-order broadcast replay
before comparison. Its complete replacement uses the pinned 0.1.12 harness
and actual native subscriber dispatch. It reaches beat 230 at
141.01080322265625 seconds with 8,462 update frames, 26 actors, no runtime
errors and no dropped events. All 13 members and original source bytes are
verified. Lossless level-22 recompression produces the 8,585,196-byte archive
`a649a5534ca4d8c61c0af03e817d4dae8b20b586c91d37c5d82b2edfa6813828.tar.zst`.
It passes all 260,300 comparisons on 0.5.1848 before replacing the old primary.
The published reference-resolution check also passes. The restarted sweep
passes these first three archives and all 677,591 comparisons, then rejects
100 Bad Days.sm's obsolete tree-order broadcast capture before comparison.
These three replacements do not establish parity for the remaining corpus
or missing charts.

Verification log SHA-256:
`852fb68095782a35685c0341d63f2d246a68b869d662a718a33c4e15dcbe81a0`.
Harness executable SHA-256:
`dffb70f638c5306f443fdade3e6b5141c661f2cde1dd88f0666563a213276359`.

## Pass 26: native-dispatch Episode 16 diagnostic

100 Bad Days.sm captures completely with the pinned 0.1.12 harness: 7,920
update frames, beat 360.75 at 131.9791717529297 seconds, no runtime errors
and no dropped events. All four archive members, source bytes and digests
are verified. Lossless recompression produces the 129,865,668-byte archive
`c9313b93ccbadd0dadb10f3ad955141a7e9e84fb01e99d356ed4c4bed99de308.tar.zst`.
It is marked local-only and its explicit ignore rule is installed before
staging, so it cannot recreate the oversized Git blob problem.

On 0.5.1848 it passes 174,897 and fails 94 of 174,991 comparisons. All
174,241 player-transform checks, message checks, timeline checks, geometry,
colors and resource checks pass. The remaining 94 checks report Cyber as a
numeric modifier without a DeadSync runtime value. The native string actually
comes from GetPlayerOptionsString, which serializes the selected NoteSkin;
PlayerOptions.cpp recognizes existing skin names through NoteSkinManager.
This category requires an independent string-option audit, not an unconditional
exception for Cyber or removal of failed coverage. The diagnostic is retained
outside the published fixture folder and does not replace the old primary.

Verification log SHA-256:
`7deabf5ffa8ac40d5579f8ef4d5e17bbde53f30eaf177480b5b2e371fa331295`.
Uncompressed archive SHA-256:
`d234a8a2a473cb0731647b04db52bf30b3d83fc582a1627a8817913c3d2b4a41`.

## Pass 27: source-backed noteskin strings and capture evidence

The linked PlayerOptions.cpp, OptionsBinding.h, NoteSkinManager.cpp,
ThemeMetric.h and RageUtil.cpp are byte-identical to the latest reference
files under rework/itgmania/src. Simply Love's Common.DefaultNoteSkinName
is cel. The harness previously supplied nil for the typed native metric and
an empty string through its string getter; DeadSync exposed default instead.
Harness 0.1.13 and DeadSync 0.5.1849 now use the checked-out theme's cel.

Native FromString recognizes existing noteskin names as strings, ignoring
percentage/approach prefixes. Branch order matters: early numeric families
win over colliding skin names, while skin lookup precedes Skew/Tilt. DeadSync
now applies that ordering, preserves the skin in GetPlayerOptionsString,
lowercases parsed names, keeps direct-setter casing, and clears prior raw
skin on fresh PlayerState.SetPlayerOptions assignments. Clearall and no
noteskin use native/theme default behavior. Cache version 40 invalidates
baked results made with the earlier string handling.

The native optional-chaining macro checks original_top even when it is zero,
an invalid Lua argument index. A fresh regression observed a stale slot
producing PlayerOptions userdata instead of a getter string after a setter.
The harness now uses the equivalent explicit-nil native NoteSkin getter.
Its regression passes nine native update frames, startup string calls,
invalid setter return values, string serialization, and copy-query isolation.
All 166 harness tests pass; five existing tests remain ignored.

The harness records unsampled noteskin_option previous/current getters and
exact parts classified by the actual native parser on a copy. The verifier
has a separate string API audit: counts, clock, order, previous/current
values, and classifier/getter consistency must agree. Corrupt or missing
runtime writes and contradictory native metadata must fail. Only exact
native-classified parts leave the numeric audit; old Cyber captures without
this evidence remain uncovered. Clearall still undergoes numeric reset checks.
DeadSync's bounded test-only records include authentic Init/On and Update(0)
calls, excluding queued discovery probes; later replay starts after frame zero.
The focused production round-trip test passes after correcting that cutoff.

The fresh Episode 16 SM capture reaches beat 360.75 at 131.9791717529297
seconds with 7,920 frames, 94 native string calls, no errors and no dropped
events. Its four payload files plus manifest and original source bytes are verified. Lossless
level-22 recompression produces 129,865,756 bytes:
2b913d2a02c46c97bfc48f342520741a8ca622e3567bb57810a07129f913963b.
Its explicit ignore rule is installed before staging and it stays local-only.
The strict comparison passes all 175,181 checks with zero failures on 0.5.1849,
including all 94 string calls. It replaces the obsolete SM primary locally;
existing reference aliases resolve to the replacement. The final DeadSync
song-Lua suite passes 479 unit, two overlay and 183 playback tests (664 total),
with three existing playback tests ignored. Both cache checks and all nine
selected parity controls pass. The negative audit also rejects contradictory
native classification metadata, so a string cannot silently leave numeric
coverage merely because the capture labels it a skin.
The full 501-simfile/492-variant objective, KABOOOOOM and uncaptured charts
remain outstanding.

Native source evidence: target/song-lua-archive-passes/pass27-noteskin-source-proof.json.
Harness test log SHA-256:
b241984f72945c51cd2b46438c18ac9c9b3915352d81948e4568db3e8c067c7c.
Harness executable SHA-256:
e804de24c8b6b53d77d808e1056d594d5ed45e7ebace7eb10fd68eb4ec9d48f8.
Focused production test log SHA-256:
73181b79feb5e59b69b63579a8edb92f3ab4113f422de19b657160a6a61b6304.

Final unit/playback test log SHA-256:
f498c4fe738378cfb32bea2e7b174787c70add4b54312e22a161e24b603ded0a.
Negative audit test log SHA-256:
ff973686949c5a5b7e084daad34b1f575d86e8236f91ce9727a09906cecaa564.
Episode 16 SM comparison log SHA-256:
873f0e436ade66b2c18294a6d2c9aeaf462785c173714099b22f4dd5d6149612.
Current full-song parity executable SHA-256:
6d59140d23487f229fbc488b83e7b7fe46a3ce39067a88338cb9105bb106f96f.

After publication, a new sweep from the top passes all four primaries and
852,772 comparisons on the current executable. It then rejects the SSC
variant's obsolete replay before comparison; that variant is the next pass.
Published reference resolution also passes. Log SHA-256:
650666809ac64d082f9f032f3fe20e936e55902c0703d3279087c00b0219fe67.

## Pass 28: refresh the Episode 16 SSC variant

The restarted sweep rejects this variant's obsolete replay before comparison.
The complete 0.1.13 replacement reaches beat 360.75 at
131.9791717529297 seconds with zero errors or dropped events.
All four payload files plus manifest, source bytes and lossless recompression are verified.
Archive b33a57b8e7aeb4c73488a54036ab93b7c9f1c043ac5ccfb1726b668a9a171079
is 129,865,822 bytes. Its ignore rule is installed before staging,
and the verified primary stays local-only. It passes all 175,181 comparisons
on 0.5.1849 with zero failures before replacing the obsolete SSC primary.
Existing aliases are preserved. No further production changes are needed
for this variant; the full 501-simfile/492-variant objective remains active.

Verification log SHA-256:
508b4c5efdb83adcb42220939700fb9810512d922ccda6f206ad05f6589b07cb.
Uncompressed archive SHA-256:
0f14a3fe09fbc98db3b7d7422f823211b4a6ce52313459eef6cf7d669bbcf2bf.

The next sweep passes the first eight primaries and all 1,797,722 checks,
then rejects Spooky's obsolete broadcast replay before comparison.

## Pass 29: refresh Spooky with native dispatch and option evidence

The complete 0.1.13 capture reaches beat 275.3541564941406 at
118.00892639160156 seconds, with no errors or dropped events.
All 8 payload files plus manifest, source bytes and recompression are
verified. Archive de6f91a3ec6861dbc1591ab3b8de1f3678f77fbf316648e92d7102f43ae91750
is 157,887 bytes. It passes all 157,678 checks with zero
failures on 0.5.1849 before replacing the obsolete primary. Its aliases remain
valid. Earlier entries have already passed with this unchanged executable;
continuation starts at the replaced entry instead of repeating them per chart.
The full corpus objective remains active.

Verification log SHA-256:
cd453ad960c683ac6f6efa10ce33764a42c1e9cce5ac8ed39ff232e238f01ace.

## Pass 30: refresh CRYSTAL_ACCESS with native dispatch and option evidence

Riddle's existing reference passes all 201,471 checks on the unchanged
0.5.1849 executable. The next entry's obsolete replay is rejected before
comparison. The complete 0.1.13 CRYSTAL_ACCESS capture reaches beat 384 at
144 seconds, with no errors or dropped events. All 6 payload files
plus manifest, source bytes and lossless recompression are verified. Archive
4738c72fb84ef4ca46e9e90d6c13728b7912987570964d56e1f4d2a333bd6d9f is
137,728 bytes and passes all 194,374 comparisons with zero
failures before replacing the obsolete primary. Existing aliases are preserved.
The sweep has verified the first eleven entries on this unchanged executable;
its continuation starts at this replaced entry. The full corpus remains pending.

Verification log SHA-256:
4a1f2df895149d5108a1bfd6e240a1736e5ef3cac2f5bd5b460434ca1459dc6d.

The workspace Simply-Love-SM5 metrics also declare cel, independently of the
peer Simply Love theme. Ready receipts count payload files; manifest.json is
an additional archive member. The source-proof receipt records both themes.

## Pass 31: refresh SAIKYOU STRONGER with native dispatch and option evidence

The complete 0.1.13 capture reaches beat 481 at
140.78048706054688 seconds, with no errors or dropped events.
All 18 payload files plus manifest, source bytes and lossless
recompression are verified. Archive
75ee08d05fae82adeab94b4b611eda8764963f47ae2f410a728213526b0fd2dc is
7,867,053 bytes and passes all 208,584 comparisons with zero
failures on 0.5.1849 before replacing its obsolete primary. Its aliases are
preserved. The unchanged executable has verified the first twelve entries;
continuation starts at the replaced entry. The full corpus remains pending.

Verification log SHA-256:
fb152f7f2ce36af064b4bef28dc1c55c069b65aefce290d69c1122433bdd9f64.

## Pass 32: refresh BroGamer with native dispatch and option evidence

The complete 0.1.13 capture reaches beat 440 at
127.53623199462892 seconds, with no errors or dropped events.
All 11 payload files plus manifest, source bytes and lossless
recompression are verified. Archive
7014cbf42845f417b626f9a1507dba786e059658df8d5ea7c1d42a87f9e43d0a is
370,245 bytes and passes all 198,090 comparisons with zero
failures on 0.5.1849 before replacing its obsolete primary. Its aliases are
preserved. This completes the first thirteen entries in the sweep. The
comparison is already current for this exact archive and unchanged executable,
so continuation starts at the following entry. The full corpus remains pending.

Verification log SHA-256:
8cdf6f9a783401b2452215e05b37f72fa90412290cbdc9c45581e26ae981a351.

The continued sweep also passes Nishi-Shinjuku (143,004), CO5M1C R4ILR0AD
(180,339), And Drugs (182,791), Karachi (206,323) and Sharkmode (259,567).
The first eighteen entries pass all 3,729,943 checks on the same immutable
0.5.1849 executable. Every receipt matches the current primary hash and that
executable's digest; pass32-verified-prefix.json records this exact scope.
Published reference resolution passes again. The next entry is KABOOOOOM;
its obsolete primary is rejected before comparison. The prior native-dispatch
diagnostic still has genuine geometry/spline gaps, so the full objective
remains incomplete. A new 0.1.13 diagnostic is being captured for it.
Reference-resolution log SHA-256:
650666809ac64d082f9f032f3fe20e936e55902c0703d3279087c00b0219fe67.


## Pass 33: retain the complete KABOOOOOM noteskin diagnostic

The 0.1.13 capture completes at beat 645.75 / 199.4571533203125 seconds
with no runtime errors or dropped events. All 26 payload files plus manifest,
original source bytes and lossless recompression are verified. Archive
4f642d3600c819500acddca0e6efb83518d91b302b8dae200b9578ca6b2f2e00 is
34,958,568 bytes. The immutable 0.5.1849 checker passes 1,637,230 and fails
26,585 comparisons out of 1,663,815. All 140 noteskin API checks pass.
This diagnostic is retained outside the flat fixture directory at
target/song-lua-archive-passes/pass33-native-diagnostics; it is unpublished.
Verification log SHA-256:
3c4334ee4d90faff21bef4b9dfe8c8a3c4f81f3f0177b5b1389a12a634518429.

Eight endpoint failures are checker errors: a recorded SetSize(58946) has
one authored SetPoint, but the checker compared its size against the count
of authored knots. Native CubicSpline::resize allocates the full extent and
zero-initializes new knots. Pointer-ordered native broadcasts can change the
other mismatch counts between captures; a lower count alone proves no fix.

## Pass 34: use native spline storage and audit unwritten knots

Harness 0.1.14 now delegates column spline storage and the spline Lua API to
linked CubicSplineN, rather than generic actor state setters. Session-owned
native objects use NCSplineHandler's three dimensions and actor ownership.
Bounds, fractional integer arguments, copied points, solve/evaluate results,
shrink/regrowth defaults and protected destruction have native regressions.
All 167 harness tests pass (five existing tests remain ignored).
Harness test-log SHA-256:
2aedfa960d49f518872005e3816a2da77cebcf6069a2b4f36e83c2f20fd51ec5.
Executable SHA-256:
0205a710ca237b1d620b1086a0f7b25ca3bf878c1832daf69b2f54f3d23f20cb.

DeadSync 0.5.1850 truncates spline sizes/indexes as native IArg does, rejects
out-of-range points, copies and pads/truncates coordinate vectors, and clears
removed knots so regrowth starts at zero. The existing 65,536-point geometry
limit also bounds allocation at SetSize. Cache version advances to 41.
All 665 song-Lua tests pass (three existing tests remain ignored).
DeadSync test-log SHA-256:
e4a32fd4cc094247869229bbc9e282d10589a0911ed15526d5ac3da3513da663.

The independent zoom comparator replays ordered SetSize/SetPoint operations
and checks every allocated knot, including unwritten zeros. Missing size
evidence or invalid writes fail explicitly. Its regression tests native resize
ordering, altered extents and hiding at an unwritten knot. CubicSpline.cpp,
CubicSpline.h, LuaBinding.h, NoteDisplay.h and fallback alias source are
byte-identical between the workspace reference and the harness's linked tree;
pass34-native-spline-source-proof.json records their hashes. Reference trees
remain unmodified. The first eighteen archives pass all 3,729,943 checks on
immutable 0.5.1850. A fresh complete 0.1.14 KABOOOOOM diagnostic passes
2,216,359 and fails 58,009 of 2,274,368 comparisons. Spline extent/default
checks now pass; projected geometry, colors and message commands still fail.
Archive 4f09569ca812f61e07b5ad1264443301f58570869aca2e9347dd193f114c55aa
(34,869,412 bytes) remains outside the flat fixtures and is unpublished.
Verification log SHA-256:
296527d6503257f8cac5443c077bf49d6193edc38d115f29fec159d96a1849bb.

Twelve of thirteen focused native controls pass. The remaining Position
spline clock/modifier control exposes a preexisting startup Reverse write
loss, also reproduced with immutable 0.5.1849. KABOOOOOM runtime compilation
also becomes much slower with dense zero-knot Lua tables. Both are addressed
in pass 35; neither failure is hidden by dropping its check.

## Pass 35: preserve option approach defaults and implicit zero knots

DeadSync 0.5.1851 retains native default approach speed 1 when a scalar
PlayerOptions setter changes only its amount. Previously the Lua getter
returned that default, but runtime compilation could suppress a constant
startup Reverse target as unchanged baseline state. The existing native
Position spline clock/modifier control now passes. All thirteen focused
native controls pass; their exact test names and actual single-test results
are recorded in pass35-native-spline-option-final.json.

Unwritten spline knots use implicit zero storage. Production position and
uniform-component readers reconstruct zeros; shrink/regrowth still clears
removed authored knots. The native storage regression also checks the
production geometry reader's extent, zero coordinates and numeric-string
point padding. This avoids one Lua table per untouched knot in startup
snapshots. Cache version advances to 42. All 665 song-Lua tests pass (three
existing tests remain ignored), as do song-clock cache roundtrip and old
header rejection. Test-log SHA-256:
be1197454202c4eec167915bc3c84ce15a48700ada5ab434ff296d24155aae91.
PlayerOptions.cpp and CubicSpline.cpp/h are byte-identical between workspace
and linked reference trees; pass35-native-storage-source-proof.json records
the source hashes. The same complete 0.1.14 KABOOOOOM diagnostic produces
exactly the same 2,216,359 passes and 58,009 failures of 2,274,368 comparisons.
Measured replay time falls from 582.22 to 224.05 seconds; compilation reaches
frame composition at 137.0 seconds. The diagnostic remains unpublished at
target/song-lua-archive-passes/pass34-native-diagnostics. Replay-log SHA-256:
cbc732b9dfc3bd6bab9ed052dd87c976c46ca2c111b26e4d10cdb64a67a33231.

A new sweep from index zero passes the first eighteen archives and all
3,729,943 checks on immutable 0.5.1851, executable SHA-256
8ee9a1ca5cb93a2a446bf8770149b484e4118b50432084a312da9e225c5f2aa2.
pass35-verified-prefix.json checks every current primary hash and receipt.
The nineteenth obsolete KABOOOOOM primary is rejected before comparison;
next-failure.json also links its current complete failing diagnostic. The
501-simfile / 492-variant objective remains incomplete.

## Pass 36: wrappers created after startup

An independent four-second native probe creates a bob wrapper from a queued
self callback at t=1. It uses no broadcasts or random calls, so recipient
order cannot explain its failure. Matching noteskin assets, 241 native update
frames, no errors/dropped events and the complete endpoint are verified.
Archive 37e8ced29ca9dec62328bae8488822ac45a95ea7c7e969097d730bef6191fdb3
passes 5,586 and fails 24 of 5,610 comparisons on immutable 0.5.1851. Every
failure is projected geometry. At beat 1.25 ITGmania's quad center is
[410.39557, 244.15823], while DeadSync keeps [400, 240]. The suspected cause
is the overlay capture list built before chronological callbacks create new
wrappers. The runtime capture fix is described below. Native Actor.cpp is
byte-identical in the workspace and linked tree. Actor::AddWrapperState, wrapper-before-owner
Update, Draw's wrapper stack and the bob formula are recorded with hashes
and anchors in pass36-late-wrapper-native-investigation.json. The probe is
retained only under harness target/song-lua-repair/pass36-late-wrapper-probe;
it is unindexed and absent from the flat corpus fixtures. An initial capture
using the other noteskin tree was rejected before any comparisons, and is
not treated as evidence of the behavior gap.


DeadSync 0.5.1852 registers wrappers created by chronological callbacks in
the capture arrays before their setters run. It extends the reusable state
buffers, retains their effects and tweens, inserts the new wrapper outside
the owner's earlier wrappers, and normalizes the final graph to parent-first
order while preserving sibling draw order. The existing layer split remaps
all overlay references consistently. New wrappers inherit layer ownership.
Cache version advances to 43.

The independent bob probe now passes all 5,610 full-archive comparisons;
its micro regression checks all 307 semantic observations. No comparisons
are removed. All 665 song-Lua tests and both cache tests pass. The focused
native checks, including existing wrapper drawing controls, also pass.

## Pass 37: audit native wrapper effects independently

The initial 0.1.14 KABOOOOOM replay reported 189 vibration mismatches after
runtime wrappers became visible to the comparator. This alone did not prove
a production regression. Actor::Draw calls PreDraw/BeginDraw on each direct
wrapper; Actor::PreDraw applies vibration at lines 606-612. The headless
effect-chain recorder omitted those wrappers. A new regression first proves
the real linked C++ drawing applies both wrapper vibrations, then fails
because the old semantic trace records only the ordinary parent.

Harness 0.1.15 includes direct wrapper effects in that leaf-to-root chain
and reports wrapper_effects = native-draw-stack. All 168 harness tests pass
(five existing tests remain ignored). Harness commit:
90b606f58353bcd4ae1a3f86a0f1131dd50b4ead.
Test-log SHA-256:
e660c9e329700133deeacf77faff8d67477fa414d1a226bc67ec515bfe936751.
The workspace and linked Actor.cpp hashes are identical; the source proof
records the draw/update/vibration anchors. References remain unmodified.

A complete four-second 0.1.15 probe creates vibration from a queued callback
after startup, with 241 native update frames and no errors or dropped events.
Archive c66b74116032f90638c1547a4bf3349da9a37ba4513d6cedd8c9ba172a86b881
fails one of 5,357 checks on immutable 0.5.1851 and passes all 5,357 on
0.5.1852. Its micro regression checks all 54 observations and verifies that
removing the wrapper's vibration capture causes the audit to fail. Whole-song
validation rejects obsolete wrapper-vibration traces, including calls inside
recorded tween operations. Older captures with only bob wrappers remain
usable. All eighteen focused native controls pass.

The fresh complete 0.1.15 KABOOOOOM capture reaches beat 645.75 at
199.4571533203125 seconds, without native errors or dropped events. All 26
payload files plus manifest, original source bytes and lossless recompression
are verified. Archive
015de9f0dfb4d41dae9f90b9df7a0ebabedc8d10e01051de741c157d5c894346
is 34,867,575 bytes and remains unpublished in
target/song-lua-archive-passes/pass37-native-diagnostics.

Both builds replay this exact archive: 0.5.1851 passes 2,216,403 and fails
58,310 of 2,274,713 checks; 0.5.1852 passes 2,222,087 and fails 52,626.
Projected vibration failures fall from 300 to 96. Geometry, color, message
and vibration gaps remain; a lower count does not establish full parity.
The chart's airhorn handlers share hornIndex, so native pointer-ordered
delivery can select different actors and random draws. Recipient ordering
and remaining queued effect timing require source-backed investigation.

A new sweep from index zero again passes the first eighteen archives and all
3,729,943 checks on immutable 0.5.1852, executable SHA-256
1833dbf0f605475581db4c227505d7b87cf7686ed8853cef326d4de01ba849b8.
pass37-verified-prefix.json verifies the current hashes and receipts. The
nineteenth obsolete primary is rejected before comparison; next-failure.json
links the complete failing 0.1.15 diagnostic separately. The full
501-simfile / 492-variant parity objective remains incomplete.

## Pass 38: retain vibration diagnostics after geometry failures

The whole-song reporter's fifty-detail budget was already consumed before
vibration comparisons ran. Give this comparator its own nested report,
merging both section tallies and failure descriptions afterward. The native
micro regression removes the runtime vibration capture and fills the prior
geometry budget; it fails before this reporting fix and passes afterward.
Both comparisons and their shared progress counters remain intact. All five
focused wrapper/vibration/reference controls pass.

The exact 0.1.15 KABOOOOOM archive is replayed again with the new reporter.
All 2,274,713 comparisons remain: 2,222,087 pass and 52,626 fail. Fifty
vibration failure details are now visible; the section still fails 96 checks.
The receipt and log are pass38-kaboom-vibration-report.json/log, log SHA-256
c60b748ab8861a1bd7605b754c294d7190911d9886be9535ab0a85ec8f5cf5ef.
No failing capture is published.

Source-backed recipient-order analysis now proves a concrete difference.
MessageManager.cpp:96 uses std::set<IMessageSubscriber*> and line 206
iterates that pointer order. The linked and workspace files are identical.
The chart's paired NextAirHorn handlers at lua/default.lua:268 and :285
share hornIndex from line 261. Simulating those conditions with the captured
native recipient order reproduces every selected wrapper and creation beat.
Only one horn is selected on the first native broadcast. Registration-order
delivery selects all twenty-two first-side horns on that broadcast; DeadSync
currently iterates its registry at lua_util.rs:2984. The resulting actor and
random-consumption differences require a message-delivery fix. No actor
choice or expected output has been hardcoded into production.

An independent four-second queued-wag probe uses the chart's accelerate,
queuecommand, linear and wag sequence without broadcasts or random calls.
The pinned 0.1.15 capture has 241 frames, no errors or dropped events, and
passes all 5,610 archive comparisons on the current DeadSync pin. Archive
6332c6210e0b8b705cc2044a50391170cd2c9a4dc60e972fac194c80c4a0e4f1
is retained only in the harness target diagnostics. Its passing control
narrows the investigation; it does not establish whole-song parity.

The order proof, five-control receipt, unchanged comparison counts and
queued-wag control are linked by pass38-vibration-report-verified.json.
The full 501-simfile / 492-variant objective remains incomplete.

## Pass 39: replay native subscriber identities

MessageManager.cpp delivers broadcasts through its pointer-ordered subscriber
set. DeadSync now orders live recipients by actor identity, independently of
definition construction and registry order. A replay host with test-support
can supply the recorded allocation ranks, keyed by session/child paths or
external screen paths. Only ranks are imported: the Lua handlers still
select actors, change state and consume the original random stream. The
native adapter and ITGmania reference sources remain unchanged.

The semantic driver resolves ranks from the original runtime tree, including
repeated definition instances and external wrapper paths. Missing or duplicate
identities reject replay. Cache version advances to 44; DeadSync advances
to 0.5.1853. This preserves the native allocation context rather than forcing
native recipients into DeadSync's previous registration order.

An independent four-second native probe contains eight pairs of subscribers,
a shared cursor and randomized zoom. It has 241 frames, seventeen runtime
actors, no errors or dropped events, and reaches beat 4 at second 4. The
immutable 0.5.1852 build fails nine of 5,902 full-archive comparisons; the
new build passes all 5,902 on the exact same archive. The micro regression
retains all 599 observations and fails when only subscriber ranks are
reversed. Original capture and source bytes are preserved with provenance.

The unchanged complete KABOOOOOM 0.1.15 archive is replayed with the recorded
subscriber identities. Of 2,274,713 comparisons, 2,274,594 pass and 119 fail.
This closes 52,507 mismatches from the preceding 0.5.1852 replay without
removing checks. All 57,326 vibration checks now pass. The remaining failures
are 110 projected-geometry, five draw-color and four message-command checks.
The first bounds failures involve queued wag effects on PeepingCow/HORNN;
the color/visibility failure involves the third big horn. These require
separate source-backed repairs. The capture remains unpublished.

The whole-song receipt is pass39-kaboom-native-message-order.json/log;
log SHA-256 acead28472465ef6cdaf0c4a2df0913e7fd9a8bf79ee0ac2efc9fe9203ffd80a.
The same-archive comparison proof is pass39-message-order-comparison.json.
The current production build without test-support and both cache tests pass.
The initial unit run exposed one load-order assumption; its replacement
checks both valid identity orders and preserves the local-state/probe
invariants. A separate live-subscriber regression verifies that permuting
the registry does not change pointer-ordered delivery.

All 666 song-Lua tests, both cache tests, the production check without
test-support, and all twelve final focused native controls pass. The first
eighteen archives again pass all 3,729,943 comparisons on the recorded
0.5.1853 pins. An extraction disk-space failure at index thirteen succeeds
on retry; the original zero-comparison failure receipt is preserved. The
prefix proof links all hashes and receipts in pass39-verified-prefix.json.
Only test coverage and an observation-count assertion changed after the
whole-song pin was built. The obsolete nineteenth primary is rejected before
comparison, and its complete 0.1.15 diagnostic remains unpublished with 119
remaining checks. The full 501-simfile / 492-variant objective is incomplete.


## Pass 40: retain the queue preceding a message tween

The command audit assumed every message starts a new tween chain at zero.
ITGmania Actor::BeginTweening appends to the existing queue; StopEffect does
not clear that queue. A separate two-message wag/stop probe reproduced the
false audit failure while all 482 drawable frame checks passed. DeadSync's
final Y write had the correct 2.51-second delay, but the audit expected 1.75.
Production playback is unchanged in this pass (0.5.1853, cache version 44).

Harness 0.1.16 records queue_start_seconds before each new segment, using the
actor's own native float queue sum and hibernation rather than ActorFrame's
child maximum. Sleep's implicit tail starts after the sleep. These are
source-derived headless offsets, not native queue-memory snapshots. The
independent message_queue_offsets_match_native test runs linked C++ Actor
queues and verifies all three appended offsets and durations. The harness
commit is bc1604f292658277dd449a33e48687dc959c1969; all 169 harness tests pass,
with five existing tests ignored.

DeadSync consumes these offsets without changing observation tolerances or
removing comparisons. The new regression retains 792 command, geometry and
frame checks. Its negative control removes the prior queue delay and still
fails one of the two command targets. All twelve existing native controls
also pass. The versioned micro trace and provenance are checked in; older
traces retain their earlier audit behavior until recaptured.

A fresh, complete KABOOOOOM capture from the unchanged source reaches beat
645.75 / 199.4571533203125 seconds, with zero runtime errors or dropped events.
The exact same archive is checked with the old and corrected comparators:
2,274,594 pass / 119 fail before; 2,274,598 pass / 115 fail after, out of the
same 2,274,713 checks. All 169 message-command checks now pass. The remaining
110 geometry and five color checks still fail and are not waived. The full
archive remains an unpublished target diagnostic; its temporary flat copy
was removed. Receipts and executable/log hashes are linked in
pass40-queue-offset-comparison.json under target/song-lua-archive-passes.

A separate callback-driven repeat-wag probe now reproduces the early-wag
geometry failure (8,436 pass / one fail). Its queued-command counterpart
passes all 8,436 checks. These focused diagnostics remain under target and
provide the next playback investigation. The full corpus goal is unfinished.


## Pass 41: complete queued effects independently of tween poses

The separate callback-driven repeat-wag chart failed one geometry check,
while the queued-command counterpart passed. Its old wag sample remained
pending behind a later Y tween; after a new stopeffect it was restored at
3.7666667 seconds, before the new queued Waggy command actually dispatched.
ITGmania Actor.h stores effect fields outside TweenState. ActorFrame.cpp
updates children before its callback; Actor.cpp dispatches queued commands
from UpdateTweening. An effect written by an already reached command must
complete independently of later pose tweens.

Scheduled completion now blocks only the existing TWEEN_POSE_TARGETS shared
with ActorTweenReplay. Immediate effects and other non-tweened fields keep
their dispatch ordering without waiting behind pose/color samples. The
existing zero-time pose ordering guard remains active. DeadSync is 0.5.1854;
song-cache version 45 invalidates the earlier compiled tracks.

The focused whole archive changes from 8,436 pass / one fail to all 8,437
checks passing. The checked-in regression preserves 1,216 observations,
including 722 drawable frame checks; replacing the second stop-effect sample
with wag fails native geometry without dropping comparisons. All fourteen
native controls, all 666 song-Lua tests, the production check without test
support, and both cache checks pass. Three existing core tests remain ignored.

The identical complete harness-0.1.16 KABOOOOOM archive now has 2,274,702 pass /
11 fail out of the same 2,274,713 checks (previously 115 failures). All 104
early-wag geometry failures are removed. The remaining six geometry and five
color failures concern the late big-horn wrapper fade. The archive is still
an unpublished target diagnostic; the temporary flat copy was removed.
The receipts, hashes, production gates and same-archive comparison are linked
in pass41-effect-queue-comparison.json under target/song-lua-archive-passes.

A separate late-wrapper/fade probe reproduces alpha, visibility and draw-color
failures (5,393 pass / three fail out of 5,396 checks). It remains under target
for the next investigation. The full corpus goal remains unfinished.


## Pass 42: restore wrapper membership after command probes

The independent late-wrapper/fade chart reproduced three alpha, visibility
and draw-color failures. A second native probe asserts one wrapper after
AddWrapperState and before GetWrapperState(1). ITGmania has no runtime errors;
DeadSync fails the first assertion at the real Begin callback. Speculative
message-command capture had retained a wrapper on the live owner, so later
GetWrapperState(1) addressed that orphan instead of the runtime wrapper.
Actor.cpp:972 creates one wrapper per call; its Lua binding at 2392 returns
that new object, and the GetWrapperState binding at 2415 is one-indexed.

Actor snapshots now include wrapper membership. Both Rust and Lua snapshot
forms copy the list while retaining the live Actor tables and their method
closures; deep cloning those objects would recurse into their owner and lose
identity. AddWrapperState registers its owner with an active capture scope
before mutation, so probes touching another owner restore that list too.
The replaced snapshot path omitted membership. DeadSync is 0.5.1855, with
song-cache version 46 invalidating earlier compiled wrapper tracks.

The unchanged fade archive now passes all 5,396 checks (previously three
failures), and the count variant passes all 5,396 (previously six failures).
The checked-in regression retains 575 observations, including 482 drawable
frames. Removing its wrapper diffuse track fails native colors without
removing comparisons. A core regression also preserves existing wrapper
identity and state on both the direct owner and another touched owner.
All 15 native controls and all 667 core song-Lua tests pass; three existing
core tests remain ignored. The production build without test support and both
cache checks pass.

The identical complete harness-0.1.16 KABOOOOOM archive now passes all
2,274,713 comparisons (previously 11 failures). Its complete 35,928,363-byte
capture, with zero native errors or drops, replaces the obsolete indexed
archive. The superseded bytes remain under target; aliases and fixture
references follow the validated replacement. No observation, tolerance or
comparison was removed.

The first 18 previously passing archives also pass all 3,729,943 comparisons
with the new executable. Together with KABOOOOOM this verifies 19 archives and
6,004,656 comparisons. Hashes, production gates, count control and same-archive
results are linked in pass42-wrapper-fade-comparison.json under
target/song-lua-archive-passes. Full-corpus verification remains unfinished.


## Pass 43: replace the second obsolete KABOOOOOM capture

The next Tech Spectrum Super variant's indexed capture was rejected before
comparisons because it lacked the required native wrapper draw evidence.
An unchanged harness-0.1.16 executable captured its original source through
beat 645.75 / 199.4571533203125 seconds, with 26 payload members and zero native
errors or dropped events. The new complete archive passes all 2,274,713
comparisons on the same DeadSync-0.5.1855 executable used in pass 42.

The 35,928,262-byte archive replaces the obsolete fixture and preserves its
aliases; the superseded bytes remain under target. No production code,
tolerance or observation changed in this pass. Together with the 18 rechecked
archives and the first KABOOOOOM variant, 20 ordered archives now pass all
8,279,369 comparisons. Receipts, hashes and publication metadata are linked in
pass43-kaboom-main-comparison.json. Full-corpus verification remains unfinished.


## Pass 44: compare manual draws on the song music clock

The Boys Are Back in Town failed 1,055 custom draw-plan comparisons with both
its old archive and a fresh complete harness-0.1.16 capture. The compiled
frames contained the expected player draws, starting at music time 1.341;
the audit looked them up using zero-based native trace seconds. The native
bridge explicitly restores TimingData's beat-zero music origin before song
position lookup. DrawFrame.second retains that music timestamp, and gameplay
selects those frames using the music effect clock. The overlay audit already
performed this conversion.

The manual plan and manual mesh audits now use the existing Second clock
conversion, preserving the authored origin and music rate. Production playback,
DeadSync version 0.5.1855 and song-cache version 46 are unchanged. Independent
positive- and negative-offset draw controls failed 105 and 30 plan checks
before this fix; both now pass all 247 observations each. Shifting retained
frame timestamps by 0.25 seconds still fails, retaining all 241 native frame
checks in each negative control. The local and linked native SongPosition,
TimingData, Actor and ActorFrame sources are byte-identical.

The exact same full archive now passes all 179,179 comparisons, with no
observation or tolerance removed. All 16 native controls pass. Sharkmode, the
only earlier archive with a manual draw audit, passes all 259,567 checks on
the corrected comparator. The other 19 prior archives have no manual audit;
their previously verified comparison paths are unchanged.

The complete 443,121-byte capture replaces the obsolete published fixture,
retaining its aliases and preserving superseded bytes under target. Native
capture reaches beat 343.66668701171875 / 129.78482055664062 seconds with 13
payload members and zero runtime errors or dropped events. Receipts and
hashes are linked in pass44-manual-clock-comparison.json. The ordered verified
prefix contains 21 archives / 8,458,548 comparisons; the full corpus is not
complete. A separate audit of public music-seconds getter semantics is also
pending; this pass establishes the conversion for the captured clock contract.


## Pass 45: replace the obsolete Epidermis timing capture

The next indexed Venetian Snares - Epidermis archive was rejected before any
comparisons because it used the obsolete continuous-BPM song clock. The
authored foreground reads GetSongBeat to fade a quad between beats 126 and
163.5. An unchanged harness-0.1.16 executable recaptured the original source
using native TimingData through beat 910.75 / 273.2250061035156 seconds, with
three payload members and zero native runtime errors or dropped events.

The complete 100,673-byte replacement passes all 363,873 comparisons on the
unchanged DeadSync-0.5.1855 executable. It replaces the obsolete published
archive while retaining aliases and preserving its superseded bytes under
target. No production code, tolerance or observation changed in this pass.

The second Boys Are Back in Town variant also passes all 179,179 checks.
Together with the prior verified prefix, 23 ordered archives now pass
9,001,600 comparisons. Receipts and hashes are linked in
pass45-epidermis-comparison.json under target/song-lua-archive-passes. The
full corpus and separate public music-seconds getter audit remain unfinished.


## Pass 46: replace the second obsolete Epidermis capture

The Tech Spectrum Super variant also failed before comparisons because its
archive retained the obsolete continuous-BPM clock. The unchanged native
harness recaptured the original source through the same complete endpoint,
beat 910.75 / 273.2250061035156 seconds, with three payload members and no
native runtime errors or dropped events. All 363,873 comparisons pass on the
same pinned DeadSync-0.5.1855 executable.

The verified 100,620-byte archive replaces the obsolete published fixture,
retaining aliases and preserving superseded bytes under target. No code,
tolerance or observation changed. The ordered prefix now contains 24 archives
and 9,365,473 comparisons; full-corpus parity and the public music-seconds
getter audit remain unfinished. Receipts and hashes are linked in
pass46-epidermis-main-comparison.json.


## Pass 47: expose native public music seconds in both implementations

SongPosition.cpp assigns the raw music timestamp to m_fMusicSeconds and its
Lua binding returns that native float. GameState.GetCurMusicSeconds returns
the same field. Simply Love's StepStatistics/Time.lua handles negative music
time and divides it by rate itself. Both the harness and DeadSync instead
exposed elapsed trace time, silently agreeing on the wrong API when offsets
or music rate mattered.

Harness 0.1.17 now compiles SongPosition.cpp and invokes its UpdateSongPosition
and actual Lua getters. Its independent positive-offset assertion failed
with public 0 versus native 1.25 before the fix. Both offset probes now pass
on every frame. All 170 harness tests pass, with five existing tests ignored.
The lighting preference uses the checked-out LightsManager default of 0.05.

DeadSync 0.5.1856 restores the music origin, music rate and native float
precision for public song/player position getters and GetCurMusicSeconds.
The elapsed timer remains unchanged. Lua 5.1 whole-number formatting is
preserved without rounding tiny fractional times away. Song-cache version 47
invalidates previously compiled getter-dependent tracks. The replaced path
returned elapsed seconds directly.

Each of five getters drives a drawable quad; a sixth quad retains elapsed
timer time. The positive control failed 170 of 5,256 checks before the fix.
Both controls now pass all 10,512 observations. Zeroing only Clock1's retained
X samples still fails, without removing observations. The two existing manual
draw controls were recaptured from the native getter and pass all 494 checks.
All 16 native controls, all 668 core song-Lua tests, the production build
without test support and both cache checks pass; three existing core tests
remain ignored. Source and capture hashes are pinned in the micro provenance.

Full archives now require the native-music-seconds capture marker. A fresh
Bank Account reference passes 116,603 checks, and a fresh Boys Are Back in Town
reference passes 179,179. Both complete archives replace their obsolete
fixtures, retaining aliases and preserving superseded bytes under target.
The obsolete-reference guard regression also passes. Earlier prefix results
are historical: those references require recapture and revalidation under
the corrected contract, so this pass does not claim a new verified prefix.

The complete fresh 321STARS capture remains unpublished with eight multitap
write differences out of 372,891 comparisons: visibility at beat 52, and
color/zoom at beat 56. Receipts are linked in pass47-public-music-comparison.json.
Full-corpus parity remains unfinished. A separate source-backed audit of the
music effect clock remains pending; this pass establishes the public getters.

## Pass 48: verify music effect clocks against compiled Actor behavior

ITGmania Actor.cpp advances music effect clocks from Actor::SetBGMTime;
GameState.cpp supplies the visible/raw music timestamp independently of the
nonnegative elapsed Actor::Update delta. The harness still used trace elapsed
seconds for music effects after pass 47 fixed the public SongPosition getters.
Two offset controls independently run the compiled Actor implementation with
music origins 1.25 and -0.5. Each checks 1,629 exact effect-time, delta and spin
rotation fields, plus 4,344 pulse vertex axes. Both previously failed at the
first music-clock getter. They now pass, including timer controls. The negative
case also exposed a harness helper that incorrectly passed a negative effect
delta to Actor::Update; it now calls the compiled internal spin update instead.
Harness 0.1.18 is committed as 9b7284ee6a1c83ab917c97ce837437b591b0bbb8.

DeadSync 0.5.1857 uses native music timestamps for non-timer time clocks,
shares the existing motion-clock delta with spin, and records effect-time
anchors in the raw music coordinate used by production rendering. Selected
effects and music clocks initialize during the existing zero-delta startup
update, before child getters and parent callbacks. No additional callback is
added; the native text and callback-count regressions remain unchanged.
The geometry comparator also needed raw music time for composition and leaf
effect rendering. Both offset traces retain all 27,168 observations per case,
including every pulse, spin, color, crop, shadow and drawable-frame check.
Incorrect getter tracks are rejected without removing observations.
Song-cache version 48 invalidates previously compiled effect clock anchors.

Full archives now require the native-music-seconds music_effect_clock marker.
The guard independently rejects missing and elapsed-seconds effect clocks.
Only the newly captured Bank Account and extra-chart Boys archives have been
verified under this contract; older prefix counts remain historical. The full
501-simfile / 492-archive audit remains unfinished, including the eight known
321STARS multitap differences. No failing chart has been published as verified.
Detailed native source, fixture and executable hashes are committed in
music-effect-clock-provenance.json; local gate and publication receipts are
recorded in pass48-music-effect-comparison.json.

## Pass 49: retain strict native multitap boundaries

A complete harness 0.1.18 recapture of 321STARS retained eight differences
at beats 52 and 56. Both clocks report those exact beats. The original
multitap/Default.lua keeps a multitap visible at its final tap and advances
its bounce/color only for beat > tap. DeadSync's analytic multitap path
encoded those edges with tap.next_up(), which can round back to the same
float seconds when converted for playback.

DeadSync 0.5.1858 checks whether those strict edges survive conversion before
replacing the authored callback with analytic curves. When they collapse,
the existing chronological compiler replays the original Lua and bakes its
native frame writes. Gameplay continues to consume compiled tracks. Cache
version 49 invalidates the obsolete analytic windows. No comparison or
observation was removed or relaxed.

The same complete archive now passes all 372,891 comparisons, including all
77,904 multitap writes. A strict-boundary regression covers the exact first
tap and adjacent native frames; existing multitap, public music getter and
music effect clock controls pass. All 668 core song-Lua tests, the production
check and both cache checks pass. The clean archive replaces the obsolete
fixture, with its aliases retained and original bytes saved under target.
Hashes and before/after evidence are in multitap-boundary-provenance.json.

Three complete archives are verified under the current native clock contract.
The full 501-simfile / 492-archive audit remains unfinished. The next pass
recaptures Warp Zone and Let Me Hear That from the start of the corpus;
earlier prefix counts remain historical until independently revalidated.

## Pass 50: restart the corpus audit with the corrected native clocks

Fresh harness 0.1.18 references for the first two ordered archives, Warp Zone
and Let Me Hear That, are complete with no runtime errors or dropped events.
DeadSync 0.5.1858 passes all 212,220 and 205,071 comparisons respectively.
Both references replace their obsolete fixtures; aliases and superseded
bytes are preserved. Capture-report.json now records their executable and
log hashes alongside the exact results.

The current verified prefix is two archives / 417,291 comparisons, with five
complete archives verified under the corrected native clock contract. The
full 501-simfile / 492-archive audit remains unfinished. The next archive in
order is Waltz Capriccio; the audit stops at the next actual capture or parity
failure for source-backed investigation.

## Pass 51: continue the current native clock corpus audit

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

Archive 6: 1035/1035.sm passes all 248156 comparisons. The verified prefix is now 6/492 archives; full-corpus parity remains unfinished.

Archive 7: 188-HS-Holdswitch[lv.08] the shadow/theshadow.ssc passes all 345742 comparisons. The verified prefix is now 7/492 archives; full-corpus parity remains unfinished.

Archive 8: 242-MISC.[lv.02] ChikuTaku/ChikuTaku.ssc passes all 328107 comparisons. The verified prefix is now 8/492 archives; full-corpus parity remains unfinished.

Archive 9: 271-MODS-[lv.01] Spooky/1.09 - Spooky.ssc passes all 157678 comparisons. The verified prefix is now 9/492 archives; full-corpus parity remains unfinished.

Archive 10: 272-MODS-[lv.02] Riddle/Riddle.ssc passes all 201471 comparisons. The verified prefix is now 10/492 archives; full-corpus parity remains unfinished.

Archive 11: 273-MODS-[lv.03] [CRYSTAL_ACCESS]/[CRYSTAL_ACCESS].ssc passes all 194374 comparisons. The verified prefix is now 11/492 archives; full-corpus parity remains unfinished.

Archive 12: 274-MODS-[lv.04] SAIKYOU STRONGER/REDALiCE_vs_USAO_-_STRONGER.ssc passes all 208584 comparisons. The verified prefix is now 12/492 archives; full-corpus parity remains unfinished.

Archive 13: 275-MODS-[lv.05] BroGamer/BroGamer.ssc passes all 198090 comparisons. The verified prefix is now 13/492 archives; full-corpus parity remains unfinished.

Archive 14: 276-MODS-[lv.06] Nishi-Shinjuku seisou kyoku/nssk-chart.ssc passes all 167442 comparisons. The verified prefix is now 14/492 archives; full-corpus parity remains unfinished.

Archive 15: 277-MODS-[lv.07] CO5M1C R4ILR0AD/CO5M1C R4ILR0AD-chart.ssc passes all 199543 comparisons. The verified prefix is now 15/492 archives; full-corpus parity remains unfinished.

Archive 16: 278-MODS-[lv.08] And Drugs/and drugs.ssc passes all 182791 comparisons. The verified prefix is now 16/492 archives; full-corpus parity remains unfinished.

Archive 17: 279-MODS-[lv.09] Karachi/Jorts - Karachi.ssc passes all 206323 comparisons. The verified prefix is now 17/492 archives; full-corpus parity remains unfinished.

Archive 18: 280-MODS-[MASTER] Sharkmode/Sharkmode.ssc passes all 304425 comparisons. The verified prefix is now 18/492 archives; full-corpus parity remains unfinished.

Archive 19: 303-MODS-[lv.memes] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/KABOOOOOM!!!!.ssc passes all 2274713 comparisons. The verified prefix is now 19/492 archives; full-corpus parity remains unfinished.

Archive 20: 303-MODS-[lv.memes] [Tech Spectrum Super]/KABOOOOOM!!!!.ssc passes all 2274713 comparisons. The verified prefix is now 20/492 archives; full-corpus parity remains unfinished.

Archive 21: 307-MISC-[lv.Death] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/The Boys Are Back in Town (to kill you).ssc passes all 179179 comparisons. The verified prefix is now 21/492 archives; full-corpus parity remains unfinished.

Archive 22: 307-MISC-[lv.Death] [Tech Spectrum Super]/The Boys Are Back in Town (to kill you).ssc passes all 179179 comparisons. The verified prefix is now 22/492 archives; full-corpus parity remains unfinished.

Archive 23: 319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/Venetian Snares - Epidermis.ssc passes all 363873 comparisons. The verified prefix is now 23/492 archives; full-corpus parity remains unfinished.

Archive 24: 319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super]/Venetian Snares - Epidermis.ssc passes all 363873 comparisons. The verified prefix is now 24/492 archives; full-corpus parity remains unfinished.

Archive 25: 321STARS/321STARS.ssc passes all 372891 comparisons. The verified prefix is now 25/492 archives; full-corpus parity remains unfinished.


## Pass 52: verify actor lookup against native Lua bindings

A direct compiled ActorFrameTexture control calls ITGmania's real ActorFrame
Lua methods. All eight assertions pass: missing named and unnamed children
return nil without creating actors, and GetText and UnknownMethod are absent.
The old harness 0.1.18 fails the unchanged missing-child and GetText assertions.
Harness 0.1.20 fixes those two behaviors and preserves GetText on the nine
BitmapText subclasses identified in ITGmania's declarations. Generic unknown
method lookup and the external Simply Love screen context remain pending.

DeadSync 0.5.1859 exposes GetText only for BitmapText and its native subclasses,
adds the native BPMDisplay constructor, and corrects StepsDisplay's ActorFrame
inheritance. Its fabricated difficulty text, helper and tags are deleted.
Cache version 50 invalidates tracks compiled with the old feature probes.
Two existing theme tests now assert StepsDisplay.GetText is absent, following
StepsDisplay.cpp's Lua registration rather than the previous fabricated API.

The complete unchanged two-assertion probe fails compilation in DeadSync
0.5.1858 at the GetText assertion and passes all 2,669 comparisons in 0.5.1859.
The earlier probe with a mismatched noteskin hash is not lookup evidence.
All 669 core song-Lua tests, the production check, both cache checks and the
retained public-music, effect-clock and strict-multitap controls pass.
Warp Zone retains all 212,220 observations and 321STARS all 372,891, with no
differences. No observations or tolerances were removed or relaxed.

The synthetic archive, native control input/output, source hashes and complete
before/after receipts are in actor-lookup-provenance.json under the micro
fixtures. The probe is not counted as a corpus song. Full parity remains
unfinished across 501 simfiles, including 18 without indexed archives and
nine historical duplicate captures. The 25-archive prefix from pass 51 records
harness 0.1.18 and DeadSync 0.5.1858; these focused checks do not revalidate the
entire prefix on the new versions. ArrowQuest still needs a complete capture
after its 900-second timeout; no truncated or diagnostic archive was published.


## Pass 53: replace invented method probes with native inheritance

A direct compiled ActorFrameTexture control verifies inherited native methods,
unknown-method absence, GetChildAt absence, and callback ownership. Native
Actor and Sprite have no SetUpdateFunction; ActorFrame registers it. A caller's
Lua function added to ActorFrame is inherited by native ActorFrameTexture.
The native source files match the local ITGmania reference tree byte for byte.

Harness 0.1.21 now uses actual compiled prototypes for Actor, ActorFrame,
ActorFrameTexture, ActorMultiVertex and Sprite; Quad has Sprite's Lua type.
It deletes the invented GetChildAt and arbitrary ActorFrame method fallback,
keeps declared fallback helper names, and uses native external player/field
class labels. Source-invalid synthetic callbacks now belong to ActorFrames.
All existing numeric samples, counts and tolerances remain. The full native
suite passes 136 tests with two existing ignored tests.

DeadSync 0.5.1860 deletes GetChildAt from class and instance adapters and deletes
the unused numeric child-index helpers. Cache version 51 invalidates compiled
tracks with the previous feature decision. The unchanged complete two-assertion
probe fails in 0.5.1859 and passes all 2,669 comparisons in 0.5.1860. All 669 core
tests, the production and cache checks, and the retained native public-music,
effect-clock and strict-multitap controls pass. Warp Zone retains all 212,220
comparisons and 321STARS all 372,891, with no differences.

The tiny complete archive and independent native input/output are committed
under the micro fixtures with method-probes-provenance.json. It is not a new
corpus song. DeadSync's non-Frame callback registration, unlinked harness actor
class registrations, and Simply Love screen context still need source-backed
work. Full-corpus parity remains unfinished: 501 simfiles, 483 indexed source
simfiles, 18 unindexed, and nine historical duplicate captures. The old
25-archive prefix is not claimed as revalidated on these new versions.


## Pass 54: restart the corpus with native method feature probes

Warp Zone is recaptured completely with committed harness 0.1.21, with no
runtime errors or dropped events. DeadSync 0.5.1860 passes all 212,220
comparisons. The archive replaces the older reference; its aliases and
superseded bytes remain preserved. Capture-report.json records exact
executable and log hashes.

The current-version verified prefix is one archive. The old 25-archive
prefix remains historical. Full parity remains unfinished across 501
simfiles, including 18 unindexed sources and nine historical duplicate
captures. The next ordered source is Let Me Hear That.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

Archive 6: 1035/1035.sm passes all 248156 comparisons. The verified prefix is now 6/492 archives; full-corpus parity remains unfinished.

Archive 7: 188-HS-Holdswitch[lv.08] the shadow/theshadow.ssc passes all 345742 comparisons. The verified prefix is now 7/492 archives; full-corpus parity remains unfinished.

Archive 8: 242-MISC.[lv.02] ChikuTaku/ChikuTaku.ssc passes all 328107 comparisons. The verified prefix is now 8/492 archives; full-corpus parity remains unfinished.

Archive 9: 271-MODS-[lv.01] Spooky/1.09 - Spooky.ssc passes all 157678 comparisons. The verified prefix is now 9/492 archives; full-corpus parity remains unfinished.

Archive 10: 272-MODS-[lv.02] Riddle/Riddle.ssc passes all 201471 comparisons. The verified prefix is now 10/492 archives; full-corpus parity remains unfinished.

Archive 11: 273-MODS-[lv.03] [CRYSTAL_ACCESS]/[CRYSTAL_ACCESS].ssc passes all 194374 comparisons. The verified prefix is now 11/492 archives; full-corpus parity remains unfinished.


## Pass 55: establish the remaining native actor class contract

The renewed .21/.1860 audit reaches 11 complete archives and 2,503,481
comparisons with no differences, each committed as soon as verification
passes. The audit stops intentionally after CRYSTAL_ACCESS to investigate
an independently proven class-registration gap, rather than a song failure.

Actual compiled ITGmania bindings verify the complete own-function
inventory of Actor (171), ActorFrame (25), Sprite (27), ActorFrameTexture
(7) and ActorMultiVertex (39). The 269-method check works in both directions:
every declared entry exists and every actual own function is represented.
It includes the two methods generated by Actor.cpp
ADD_GET_SET_METHODS(tween_uses_effect_delta). Fallback Lua helper names
remain separate source evidence.

Two additional native userdata controls verify inheritance, Frame-only
callback ownership, absence of ActorFrame.fardistz, caller-added class Lua
functions and live base/derived method replacement. In particular, existing
ActorFrameTexture userdata immediately sees changes to Actor.GetX and
ActorFrame.GetX and inherits the base override when the derived override
is removed. A repair that leaves instance functions shadowing those class
tables would still be incorrect.

The unchanged complete class-inheritance archive captures cleanly under
harness .21 but fails compilation in DeadSync .1860 at the native Actor
class-exists assertion. The archive and actual native controls are preserved
under micro fixtures with actor-class-provenance.json. This is verified
failure evidence; the DeadSync implementation repair is still pending.
No production source or Lua song content is changed in this evidence commit,
and the probe is not counted as a corpus song or passing parity fixture.

## Pass 56: repair native public class inheritance

DeadSync .1861 exposes the compiled 269-method inventory through public Actor,
ActorFrame, Sprite, ActorFrameTexture and ActorMultiVertex class tables.
Existing instances see caller-added Lua methods and live base/derived overrides.
Frame callbacks remain unavailable on plain Actor and Sprite/Quad. Native
Actor::AddWrapperState allocates ActorFrame, so wrappers retain those callbacks
and all original hibernation timing assertions. Independent native controls
verify wrapper ownership, invalid callback arguments and multiple return values.

Banner is a distinct Sprite subclass using the 14 own method declarations in
local Banner.cpp. Those declarations are source-backed; Banner is not linked
into the compiled oracle. The obsolete standalone class forwarders and invented
ActorFrame.fardistz and AMV getters are removed from public lookup. The AMV shape
test uses GetDestDrawMode and verifies line width through a drawn three-pixel
line, retaining the original quad vertex and color assertions.

Synthetic callback owners are corrected to ActorFrame while drawable Quads,
numeric expectations, playback golden samples and tolerances remain intact.
Cache version 52 invalidates compiled tracks with the old method decisions.
All 670 core tests pass with three existing GPU tests ignored. Production,
clock-cache and old-cache rejection checks pass, as do the retained native
public-music, effect-clock and strict-multitap controls. The unchanged complete
class-inheritance archive passes all 2,694 comparisons after failing in
.1860. Exact receipts are preserved in actor-class-repair-provenance.json.

The unchanged archive also exposes a comparator mistake: a never-textured
Sprite was counted as drawable despite native Sprite::EarlyAbortDraw and
the trace's lack of Sprite geometry. Drawable membership now retains every
Sprite with a declared texture, recorded load/SetTexture or projected track;
the empty actor stays in the tree without inventing a primitive. All original
2,670 checks remain, and the image texture alias control passes unchanged.

This repair verifies class registration and lookup, not every native method
body or fallback helper. Full corpus parity remains unfinished: 501 simfiles,
483 indexed sources, 18 unindexed and nine historical extra captures. The prior
11-archive prefix is historical on .21/.1860 until revalidated with .1861.

## Pass 57: revalidate the corpus after native class inheritance

Archive 1: (R10) Warp Zone/warp zone.ssc passes all 212220 comparisons. The verified prefix is now 1/492 archives; full-corpus parity remains unfinished.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

Archive 6: 1035/1035.sm passes all 248156 comparisons. The verified prefix is now 6/492 archives; full-corpus parity remains unfinished.

Archive 7: 188-HS-Holdswitch[lv.08] the shadow/theshadow.ssc passes all 345742 comparisons. The verified prefix is now 7/492 archives; full-corpus parity remains unfinished.

Archive 8: 242-MISC.[lv.02] ChikuTaku/ChikuTaku.ssc passes all 328107 comparisons. The verified prefix is now 8/492 archives; full-corpus parity remains unfinished.

Archive 9: 271-MODS-[lv.01] Spooky/1.09 - Spooky.ssc passes all 157678 comparisons. The verified prefix is now 9/492 archives; full-corpus parity remains unfinished.

Archive 10: 272-MODS-[lv.02] Riddle/Riddle.ssc passes all 201471 comparisons. The verified prefix is now 10/492 archives; full-corpus parity remains unfinished.

Archive 11: 273-MODS-[lv.03] [CRYSTAL_ACCESS]/[CRYSTAL_ACCESS].ssc passes all 194374 comparisons. The verified prefix is now 11/492 archives; full-corpus parity remains unfinished.

Archive 12: 274-MODS-[lv.04] SAIKYOU STRONGER/REDALiCE_vs_USAO_-_STRONGER.ssc passes all 208584 comparisons. The verified prefix is now 12/492 archives; full-corpus parity remains unfinished.

Archive 13: 275-MODS-[lv.05] BroGamer/BroGamer.ssc passes all 198090 comparisons. The verified prefix is now 13/492 archives; full-corpus parity remains unfinished.

Archive 14: 276-MODS-[lv.06] Nishi-Shinjuku seisou kyoku/nssk-chart.ssc passes all 167442 comparisons. The verified prefix is now 14/492 archives; full-corpus parity remains unfinished.

Archive 15: 277-MODS-[lv.07] CO5M1C R4ILR0AD/CO5M1C R4ILR0AD-chart.ssc passes all 199543 comparisons. The verified prefix is now 15/492 archives; full-corpus parity remains unfinished.

Archive 16: 278-MODS-[lv.08] And Drugs/and drugs.ssc passes all 182791 comparisons. The verified prefix is now 16/492 archives; full-corpus parity remains unfinished.

Archive 17: 279-MODS-[lv.09] Karachi/Jorts - Karachi.ssc passes all 206323 comparisons. The verified prefix is now 17/492 archives; full-corpus parity remains unfinished.

## Pass 58: verify and repair ActorProxy against compiled ITGmania

The harness now links the unchanged native ActorProxy.cpp. Compiled userdata
assertions verify its two own methods, Actor inheritance, absent Frame callbacks
and child lookup, nil initial target, assigned target identity, rejection of
missing/scalar/plain-table targets, independent position, caller-added methods
and live base overrides. The local reference and pinned vendor Proxy sources
are byte-identical. Native control commit: 050a8d3; harness repair: 7abb249.

Harness .22 passes 138 tests with two existing skips. DeadSync .1862 exposes
the same public class and own methods, validates targets before assignment,
and restricts the new GetTarget adapter to Proxy actors. Cache version is 53.
The unchanged native JSON assertions fail on the old implementation and pass
on the repaired source. Final source validation passes 486 library tests, two
integration tests, 183 playback tests and the production check. Three GPU
tests remain ignored. Both cache roundtrip and old-version rejection pass.

A parallel playback run hit a global texture-generation equality failure. The
isolated test and both complete serial runs pass without changing its numeric
assertions. This is recorded in proxy-methods-provenance.json.

Two complete two-second archives retain their Lua unchanged and capture against
DeadSync's noteskins. They contain zero runtime errors and dropped events. The
old .1861 executable fails on the expected class assertions. Both the intermediate
and final .1862 executables pass all 5,342 comparisons. The final executable's
source hashes match the committed repair, including the Proxy-only GetTarget
adapter. Explicitly enabling the existing Simply Love test-support feature
resolved the archive verifier's dependency linking error. No source or golden
assertion was changed to resolve this build error. These micro controls do not
establish full-corpus parity.

Other public classes, complete Proxy rendering/argument cases and Simply Love
screen context remain pending. The previous 17-archive prefix (3,666,254 passing
comparisons) was checked on harness .21 / DeadSync .1861 and needs refreshing.
Full scope remains 501 simfiles, including 18 unindexed sources, and 492 archive
variants. Full-corpus parity is unfinished.

## Pass 59: revalidate the corpus after native class inheritance

Archive 1: (R10) Warp Zone/warp zone.ssc passes all 212220 comparisons. The verified prefix is now 1/492 archives; full-corpus parity remains unfinished.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

Archive 6: 1035/1035.sm passes all 248156 comparisons. The verified prefix is now 6/492 archives; full-corpus parity remains unfinished.

Archive 7: 188-HS-Holdswitch[lv.08] the shadow/theshadow.ssc passes all 345742 comparisons. The verified prefix is now 7/492 archives; full-corpus parity remains unfinished.

Archive 8: 242-MISC.[lv.02] ChikuTaku/ChikuTaku.ssc passes all 328107 comparisons. The verified prefix is now 8/492 archives; full-corpus parity remains unfinished.


## Pass 60: verify BitmapText against compiled ITGmania

The unchanged native BitmapText.cpp and FontManager.cpp are now linked into
the actor oracle. Native control f576833 verifies 19 own methods, Actor
inheritance, absent Frame methods and unknown probes, empty text, live class
and base overrides, and the macro boolean setter. Numeric zero, strings and
tables are true; false, nil and missing values are false. The native label has
no loaded font, so this does not establish text layout or rendering parity.

Harness repair a9ee894 exposes the native class and corrects this setter.
All 140 harness tests pass with two existing skips. DeadSync .1863 installs
the same native inventory and strict lookup, preserves the five stock fallback
helper bodies, and invalidates old compiled behavior with cache version 54.
All 672 core tests pass with three existing GPU skips; production, clock-cache
and old-cache rejection checks pass. Native assertions are unchanged.

The style test now calls native wrapwidthpixels with its original numeric
expectations. The obsolete unconditional _wrapwidthpixels alias is removed: the
checked-out Simply Love helper has separate 8-bit and Unicode branches and is
not a native or fallback method. Full Simply Love wrapping remains pending.

Two complete two-second archives contain no runtime errors or dropped events.
Both fail on .1862 at the expected class assertions. Their .1863 archive
comparisons now pass all 5,342 checks using the pinned .1863 executable.
The source repair is committed as a93778308. The harness public raw-table
inventory and method-shadowing audit is pending and has an independent
compiled-native control that fails on the semantic adapter.
Full corpus parity remains unfinished, with 501 simfiles and 492 indexed
archive variants. The prior eight-archive prefix is historical on .22/.1862.


## Pass 62: repair public native method tables in the harness

Compiled native controls expose two more adapter gaps: native own methods exist
before lookup, and subclass own methods shadow later base overrides. A second
control verifies inherited false, zero, strings and tables. The .23 semantic
control fails for missing own BitmapText methods; an intermediate .24 control
fails for inherited false. Neither failed capture is published as an archive.

Harness commit 6a127f4 replaces lazy inherited-first lookup with eager own
native methods and fallback declarations, while base lookup preserves values.
All 142 harness tests pass with two existing skips. Exact Lua assertions match
compiled native method tables, with five BitmapText helper bodies taken from
the checked-out fallback script. No engine or theme reference code is changed.

Two complete two-second archives retain their Lua unchanged and have no runtime
errors or dropped events. DeadSync .1863 already has the correct public table
behavior and passes all 5,338 comparisons without further production changes.
The earlier raw-method archive is byte-identical after the inherited-value fix.
Exact native inputs, outputs, pins and failures are in
actor-class-tables-provenance.json. Method bodies, font layout, unlinked actor
subclasses and full Simply Love context remain pending.

The corpus must now be refreshed from the top on harness .24 / DeadSync .1863.
The .22/.1862 eight-archive prefix is historical; the full 501-simfile and
492-archive scope is unchanged. Full parity remains unfinished.

## Pass 63: revalidate the corpus after native class inheritance

Archive 1: (R10) Warp Zone/warp zone.ssc passes all 212220 comparisons. The verified prefix is now 1/492 archives; full-corpus parity remains unfinished.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

Archive 6: 1035/1035.sm passes all 248156 comparisons. The verified prefix is now 6/492 archives; full-corpus parity remains unfinished.

Archive 7: 188-HS-Holdswitch[lv.08] the shadow/theshadow.ssc passes all 345742 comparisons. The verified prefix is now 7/492 archives; full-corpus parity remains unfinished.

Archive 8: 242-MISC.[lv.02] ChikuTaku/ChikuTaku.ssc passes all 328107 comparisons. The verified prefix is now 8/492 archives; full-corpus parity remains unfinished.

Archive 9: 271-MODS-[lv.01] Spooky/1.09 - Spooky.ssc passes all 157678 comparisons. The verified prefix is now 9/492 archives; full-corpus parity remains unfinished.

Archive 10: 272-MODS-[lv.02] Riddle/Riddle.ssc passes all 201471 comparisons. The verified prefix is now 10/492 archives; full-corpus parity remains unfinished.

Archive 11: 273-MODS-[lv.03] [CRYSTAL_ACCESS]/[CRYSTAL_ACCESS].ssc passes all 194374 comparisons. The verified prefix is now 11/492 archives; full-corpus parity remains unfinished.

Archive 12: 274-MODS-[lv.04] SAIKYOU STRONGER/REDALiCE_vs_USAO_-_STRONGER.ssc passes all 208584 comparisons. The verified prefix is now 12/492 archives; full-corpus parity remains unfinished.

Archive 13: 275-MODS-[lv.05] BroGamer/BroGamer.ssc passes all 198090 comparisons. The verified prefix is now 13/492 archives; full-corpus parity remains unfinished.

Archive 14: 276-MODS-[lv.06] Nishi-Shinjuku seisou kyoku/nssk-chart.ssc passes all 167442 comparisons. The verified prefix is now 14/492 archives; full-corpus parity remains unfinished.

Archive 15: 277-MODS-[lv.07] CO5M1C R4ILR0AD/CO5M1C R4ILR0AD-chart.ssc passes all 199543 comparisons. The verified prefix is now 15/492 archives; full-corpus parity remains unfinished.

Archive 16: 278-MODS-[lv.08] And Drugs/and drugs.ssc passes all 182791 comparisons. The verified prefix is now 16/492 archives; full-corpus parity remains unfinished.

Archive 17: 279-MODS-[lv.09] Karachi/Jorts - Karachi.ssc passes all 206323 comparisons. The verified prefix is now 17/492 archives; full-corpus parity remains unfinished.


## Pass 64: correct native actor identity and BitmapText arguments

Compiled ITGmania userdata independently rejects nonboolean arguments to
rainbowscroll, jitter and uppercase. These bindings use BArg, unlike BIArg and
lua_toboolean. Luna::tostring_T returns the native class and an opaque pointer;
actor names and hierarchy paths do not appear in that string. Matching sources
are checked in the local reference tree and the compiled vendor revision.

The .24 harness leaked hierarchy paths into actor strings. Sharkmode's original
grabactors.lua classified Sprites under PlayerP1 as Players and attempted an
invalid GetNumChildren call. The unchanged runtime control also exposed the
internal name field shadowing actor:name(...). Harness 52385fe
fixes those semantics and passes 143 tests, with two existing skips.

DeadSync .1863 fails both complete control archives at the native assertions.
The replacement identity strings, native name setter and strict boolean
bindings pass both controls on .1864: 5,346 exact comparisons. The full song-Lua
suite, production check and cache checks pass. Cache version 55 invalidates
captures that could omit failing startup commands. The old name-based string
test is replaced by native class-and-pointer expectations.

Sharkmode now captures unchanged through beat 357 / 138.1935577392578 seconds
with no runtime errors or dropped events. DeadSync passes 240,340 comparisons
but fails 18,318 checks in drawable membership, custom draws and mesh matching.
Its newly captured archive remains unpublished while those gaps are diagnosed.
The two complete control archives are hash-verified and pass all comparisons.
Runtime controls, native source hashes, before/after failures and immutable
executable pins are in runtime-actors-provenance.json.

The previous 17-archive prefix is historical until refreshed on .25/.1864.
The full scope remains 501 simfiles and 492 archive variants; parity is unfinished.

## Pass 65: match native Sprite loading and retain runtime assets

Compiled ITGmania Sprite userdata confirms that LoadBackground and LoadBanner
require a string-compatible path, install the image and reset source dimensions.
Both bindings return the last stack argument, including a trailing nil, because
LunaSprite returns 1 without pushing self. The native fallback background helper
ignores that result and returns the actor itself. The runtime controls retain
the compiled native assertion body unchanged in both implementations.

Harness 5ebb034 handles those native methods and records runtime texture
requests. Its archive test loads the background through the actual fallback
helper, unloads it before any projected frame, and still verifies the original
PNG bytes, dimensions and SHA-256 in the complete archive. The obsolete
background helper branch is removed. All 146 harness tests pass; two existing
tests remain ignored.

DeadSync .1865 matches the native return values and argument validation.
All 674 song-Lua tests pass, along with the production build check and both
cache checks. Cache version 56 invalidates captures that could continue invalid
method chains. Native inputs, output, image and verification provenance are in
sprite-load-provenance.json. The native texture fixture verifies Lua contracts
and metadata; it does not establish pixel rendering or texture policy parity.

The Sharkmode archive omitted sharkmode-bg.png because the harness did not
handle LoadBackground. DeadSync already recognizes that filename. A fresh
.26 capture was required before publication; no comparator has been relaxed.
The historical 17-archive prefix requires refresh
on .26/.1865. The full 501-simfile and 492-archive scope remains unfinished.

The complete two-second Sprite control archive passes all 2,695 comparisons
on .1865, with no native runtime errors or dropped events. The previous .1864
verifier fails the same unchanged control at LoadBackground's return-value
assertion. The archive includes the original Lua, image and complete trace;
sprite-load-provenance.json records both immutable binaries and results.

Sharkmode's fresh .26 archive now passes all 304,425 comparisons on .1865,
including 8,293 custom draw plans, 14,940 mesh bindings, 14,940 mesh poses and
14,940 mesh color checks. It reaches beat 357 / 138.1935577392578 seconds with
no native runtime errors or dropped events. The archive includes the original
1,058,414-byte sharkmode-bg.png and unchanged song Lua. The corrected background
restores drawable membership and resolves the cascading mesh mapping failures.
The verified archive replaces the superseded canonical capture, preserving
its aliases. The full corpus refresh from the top remains pending.

## Pass 66: revalidate the corpus after native Sprite loading

Archive 1: (R10) Warp Zone/warp zone.ssc passes all 212220 comparisons. The verified prefix is now 1/492 archives; full-corpus parity remains unfinished.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

## Pass 67: preserve native texture filenames

A compiled native Sprite control confirms that RageTexture:GetPath returns the
RageTextureID filename, with dot components collapsed by RageUtil::CollapsePath.
The .26 harness replaced this value with a song:/ archive alias, causing the
unchanged runtime assertion to fail. The .1865 DeadSync verifier separately
fails the native dot-component assertion. The checked-out reference tree and
compiled vendor sources agree on both contracts.

Harness 68605b3 uses its compiled RageTextureID normalization helper for the
getter. The unchanged native assertion body passes actual Sprite userdata and
the headless runtime; all 146 harness tests pass with two existing skips.
DeadSync .1866 reuses its existing native filename-collapse implementation
when installing texture getters. Cache version 57 invalidates captures that
could branch on the old filename. Sprite runtime tests now cover both loading
and filename controls through the same test path.

The .26/.1865 refresh completed and committed five archives with 1,027,953
passing comparisons before its source guard stopped for this independently
proven gap. Both 100 Bad Days archives are oversized and remain local with
explicit ignore rules. The historical prefix must be refreshed on the corrected
binaries. The full 501-simfile and 492-archive scope remains unfinished.

The full archive reader additionally exposed absolute Sprite.Load references
from the capture machine. Harness .28 records these image arguments as portable
song:/ asset references while preserving the filenames used inside Lua.
The native assertion body remains unchanged; only its fixture-path setup removes
the Windows verbatim prefix introduced by canonicalized temporary directories.
The complete two-second control now passes all 2,696 comparisons on .1866.
The previous .1865 binary fails the same native dot-component assertion.
All 674 song-Lua tests, the production build check and both cache checks pass.
The earlier Sprite loading control also retains all 2,695 passing comparisons.
Native source hashes, unchanged controls, complete archive and before/after
receipts are in texture-path-provenance.json. The comparator remains unchanged.

## Pass 68: revalidate the corpus after native texture filenames

Archive 1: (R10) Warp Zone/warp zone.ssc passes all 212220 comparisons. The verified prefix is now 1/492 archives; full-corpus parity remains unfinished.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

Archive 6: 1035/1035.sm passes all 248156 comparisons. The verified prefix is now 6/492 archives; full-corpus parity remains unfinished.

Archive 7: 188-HS-Holdswitch[lv.08] the shadow/theshadow.ssc passes all 345742 comparisons. The verified prefix is now 7/492 archives; full-corpus parity remains unfinished.

Archive 8: 242-MISC.[lv.02] ChikuTaku/ChikuTaku.ssc passes all 328107 comparisons. The verified prefix is now 8/492 archives; full-corpus parity remains unfinished.

Archive 9: 271-MODS-[lv.01] Spooky/1.09 - Spooky.ssc passes all 157678 comparisons. The verified prefix is now 9/492 archives; full-corpus parity remains unfinished.

Archive 10: 272-MODS-[lv.02] Riddle/Riddle.ssc passes all 201471 comparisons. The verified prefix is now 10/492 archives; full-corpus parity remains unfinished.

Archive 11: 273-MODS-[lv.03] [CRYSTAL_ACCESS]/[CRYSTAL_ACCESS].ssc passes all 194374 comparisons. The verified prefix is now 11/492 archives; full-corpus parity remains unfinished.

Archive 12: 274-MODS-[lv.04] SAIKYOU STRONGER/REDALiCE_vs_USAO_-_STRONGER.ssc passes all 208584 comparisons. The verified prefix is now 12/492 archives; full-corpus parity remains unfinished.

Archive 13: 275-MODS-[lv.05] BroGamer/BroGamer.ssc passes all 198090 comparisons. The verified prefix is now 13/492 archives; full-corpus parity remains unfinished.

Archive 14: 276-MODS-[lv.06] Nishi-Shinjuku seisou kyoku/nssk-chart.ssc passes all 167442 comparisons. The verified prefix is now 14/492 archives; full-corpus parity remains unfinished.

Archive 15: 277-MODS-[lv.07] CO5M1C R4ILR0AD/CO5M1C R4ILR0AD-chart.ssc passes all 199543 comparisons. The verified prefix is now 15/492 archives; full-corpus parity remains unfinished.

Archive 16: 278-MODS-[lv.08] And Drugs/and drugs.ssc passes all 182791 comparisons. The verified prefix is now 16/492 archives; full-corpus parity remains unfinished.

Archive 17: 279-MODS-[lv.09] Karachi/Jorts - Karachi.ssc passes all 206323 comparisons. The verified prefix is now 17/492 archives; full-corpus parity remains unfinished.

Archive 18: 280-MODS-[MASTER] Sharkmode/Sharkmode.ssc passes all 304425 comparisons. The verified prefix is now 18/492 archives; full-corpus parity remains unfinished.

Archive 19: 303-MODS-[lv.memes] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/KABOOOOOM!!!!.ssc passes all 2274690 comparisons. The verified prefix is now 19/492 archives; full-corpus parity remains unfinished.

Archive 20: 303-MODS-[lv.memes] [Tech Spectrum Super]/KABOOOOOM!!!!.ssc passes all 2274713 comparisons. The verified prefix is now 20/492 archives; full-corpus parity remains unfinished.

Archive 21: 307-MISC-[lv.Death] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/The Boys Are Back in Town (to kill you).ssc passes all 179179 comparisons. The verified prefix is now 21/492 archives; full-corpus parity remains unfinished.

Archive 22: 307-MISC-[lv.Death] [Tech Spectrum Super]/The Boys Are Back in Town (to kill you).ssc passes all 179179 comparisons. The verified prefix is now 22/492 archives; full-corpus parity remains unfinished.

Archive 23: 319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/Venetian Snares - Epidermis.ssc passes all 363873 comparisons. The verified prefix is now 23/492 archives; full-corpus parity remains unfinished.

Archive 24: 319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super]/Venetian Snares - Epidermis.ssc passes all 363873 comparisons. The verified prefix is now 24/492 archives; full-corpus parity remains unfinished.

Archive 25: 321STARS/321STARS.ssc passes all 372891 comparisons. The verified prefix is now 25/492 archives; full-corpus parity remains unfinished.

Archive 26: 666/666.ssc passes all 552285 comparisons. The verified prefix is now 26/492 archives; full-corpus parity remains unfinished.

## Pass 69: compare column splines on the native music clock

The complete `7th Gear/7th Gear.ssc` capture failed 300 of 881665 checks.
Its native trace timestamps are relative to beat zero, while the compiled
column windows and spline tracks use raw music time. Native `TimingData` and
`SongPosition` restore the simfile offset before timing lookup; the column
comparison omitted that conversion. Restore the same origin before querying
both types of track. Comparison counts and tolerances remain unchanged.

Compiled ITGmania controls with offsets +0.125 and -0.125 seconds, and a BPM
change from 120 to 150, verify the native music timestamp and spline writes.
The positive control fails before the fix; both controls and the existing
native spline curve and track tests pass afterwards. The unchanged complete
7th Gear capture now passes all 881665 checks, including all 2304 spline
checks, with no runtime errors or dropped events. Its obsolete archive is
replaced; aliases and superseded bytes are retained. Production code, version
0.5.1866, and song cache version 57 are unchanged in this pass.

An optimized build of the unchanged harness 0.1.28 passes 146 tests, with two
existing ignored tests. All 63 native actor controls match the debug build
byte for byte. This build is being used to retry the six historical timeouts.
The first ArrowQuest retry reached the original 2013-second endpoint with no
dropped events, but reported Lua runtime errors and remains pending. No
diagnostic archive is published as a complete fixture.

The older Igaku whole-song audit retains all 331752 observations: all 680
spline checks pass, while 16 confusion-offset modifier values still differ.
Its reference must be checked against a fresh capture with current native
song timing before changing the implementation. The full scope remains 501
simfiles, including 18 unindexed sources, and 492 existing archive cases.
Revalidation from the top with the corrected comparator remains unfinished.

Archive 1: (R10) Warp Zone/warp zone.ssc passes all 212220 comparisons. The verified prefix is now 1/492 archives; full-corpus parity remains unfinished.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

Archive 6: 1035/1035.sm passes all 248156 comparisons. The verified prefix is now 6/492 archives; full-corpus parity remains unfinished.

Archive 7: 188-HS-Holdswitch[lv.08] the shadow/theshadow.ssc passes all 345742 comparisons. The verified prefix is now 7/492 archives; full-corpus parity remains unfinished.

Archive 8: 242-MISC.[lv.02] ChikuTaku/ChikuTaku.ssc passes all 328107 comparisons. The verified prefix is now 8/492 archives; full-corpus parity remains unfinished.

Archive 9: 271-MODS-[lv.01] Spooky/1.09 - Spooky.ssc passes all 157678 comparisons. The verified prefix is now 9/492 archives; full-corpus parity remains unfinished.

Archive 10: 272-MODS-[lv.02] Riddle/Riddle.ssc passes all 201471 comparisons. The verified prefix is now 10/492 archives; full-corpus parity remains unfinished.

Archive 11: 273-MODS-[lv.03] [CRYSTAL_ACCESS]/[CRYSTAL_ACCESS].ssc passes all 194374 comparisons. The verified prefix is now 11/492 archives; full-corpus parity remains unfinished.

Archive 12: 274-MODS-[lv.04] SAIKYOU STRONGER/REDALiCE_vs_USAO_-_STRONGER.ssc passes all 208584 comparisons. The verified prefix is now 12/492 archives; full-corpus parity remains unfinished.

Archive 13: 275-MODS-[lv.05] BroGamer/BroGamer.ssc passes all 198090 comparisons. The verified prefix is now 13/492 archives; full-corpus parity remains unfinished.


## Pass 70: restore native actor definition concatenation

Both adapters discarded the base command when Lua concatenated definitions. ArrowQuest therefore lost map InitCommands and finished both full native captures with 51 runtime errors. The unmodified native fallback ActorDef.lua, executed in the compiled native oracle, proves that MergeTables creates a fresh definition, overrides numeric keys, combines all function collisions in left-to-right order, forwards arguments and trailing nil values, returns the second function result, and stops on a first-function error. The checked-in control fails on both old adapters and passes after the repairs. DeadSync rebinds methods to the new table and replaces the old mutation and numeric append paths; compiled song cache version is now 58.

Validation: 148 harness unit tests pass with two existing ignored; actor, chart and diff integrations pass. The unrelated outro integration input called ActorFrame-only SetUpdateFunction on Quad; the old .28 executable reproduces that failure. Its callback now lives on an ActorFrame and the unchanged endpoint checks pass. DeadSync passes 675 song-Lua tests with three existing GPU tests ignored. Pass 69 stopped at its source guard after 13 committed verified archives. Its prefix is historical after this production change. Original Mr. Sandman capture is ready in ignored staging and awaits full DeadSync verification; both ArrowQuest inputs require fresh full native captures. Scope remains 501 original simfiles and 492 archive variants until publication. Full-corpus parity remains unfinished.

Archive 1: (R10) Warp Zone/warp zone.ssc passes all 212220 comparisons. The verified prefix is now 1/492 archives; full-corpus parity remains unfinished.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

Archive 6: 1035/1035.sm passes all 248156 comparisons. The verified prefix is now 6/492 archives; full-corpus parity remains unfinished.

Archive 7: 188-HS-Holdswitch[lv.08] the shadow/theshadow.ssc passes all 345742 comparisons. The verified prefix is now 7/492 archives; full-corpus parity remains unfinished.

Archive 8: 242-MISC.[lv.02] ChikuTaku/ChikuTaku.ssc passes all 328107 comparisons. The verified prefix is now 8/492 archives; full-corpus parity remains unfinished.

Archive 9: 271-MODS-[lv.01] Spooky/1.09 - Spooky.ssc passes all 157678 comparisons. The verified prefix is now 9/492 archives; full-corpus parity remains unfinished.

Archive 10: 272-MODS-[lv.02] Riddle/Riddle.ssc passes all 201471 comparisons. The verified prefix is now 10/492 archives; full-corpus parity remains unfinished.

Archive 11: 273-MODS-[lv.03] [CRYSTAL_ACCESS]/[CRYSTAL_ACCESS].ssc passes all 194374 comparisons. The verified prefix is now 11/492 archives; full-corpus parity remains unfinished.

Archive 12: 274-MODS-[lv.04] SAIKYOU STRONGER/REDALiCE_vs_USAO_-_STRONGER.ssc passes all 208584 comparisons. The verified prefix is now 12/492 archives; full-corpus parity remains unfinished.

Archive 13: 275-MODS-[lv.05] BroGamer/BroGamer.ssc passes all 198090 comparisons. The verified prefix is now 13/492 archives; full-corpus parity remains unfinished.

Archive 14: 276-MODS-[lv.06] Nishi-Shinjuku seisou kyoku/nssk-chart.ssc passes all 167442 comparisons. The verified prefix is now 14/492 archives; full-corpus parity remains unfinished.

Archive 15: 277-MODS-[lv.07] CO5M1C R4ILR0AD/CO5M1C R4ILR0AD-chart.ssc passes all 199543 comparisons. The verified prefix is now 15/492 archives; full-corpus parity remains unfinished.

Archive 16: 278-MODS-[lv.08] And Drugs/and drugs.ssc passes all 182791 comparisons. The verified prefix is now 16/492 archives; full-corpus parity remains unfinished.

Archive 17: 279-MODS-[lv.09] Karachi/Jorts - Karachi.ssc passes all 206323 comparisons. The verified prefix is now 17/492 archives; full-corpus parity remains unfinished.

## Pass 71: revalidate the corpus after native value iterator lookups

Archive 1: (R10) Warp Zone/warp zone.ssc passes all 212220 comparisons. The verified prefix is now 1/492 archives; full-corpus parity remains unfinished.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.

Archive 6: 1035/1035.sm passes all 248156 comparisons. The verified prefix is now 6/492 archives; full-corpus parity remains unfinished.

Archive 7: 188-HS-Holdswitch[lv.08] the shadow/theshadow.ssc passes all 345742 comparisons. The verified prefix is now 7/492 archives; full-corpus parity remains unfinished.

Archive 8: 242-MISC.[lv.02] ChikuTaku/ChikuTaku.ssc passes all 328107 comparisons. The verified prefix is now 8/492 archives; full-corpus parity remains unfinished.

Archive 9: 271-MODS-[lv.01] Spooky/1.09 - Spooky.ssc passes all 157678 comparisons. The verified prefix is now 9/492 archives; full-corpus parity remains unfinished.

Archive 10: 272-MODS-[lv.02] Riddle/Riddle.ssc passes all 201471 comparisons. The verified prefix is now 10/492 archives; full-corpus parity remains unfinished.

Archive 11: 273-MODS-[lv.03] [CRYSTAL_ACCESS]/[CRYSTAL_ACCESS].ssc passes all 194374 comparisons. The verified prefix is now 11/492 archives; full-corpus parity remains unfinished.

Archive 12: 274-MODS-[lv.04] SAIKYOU STRONGER/REDALiCE_vs_USAO_-_STRONGER.ssc passes all 208584 comparisons. The verified prefix is now 12/492 archives; full-corpus parity remains unfinished.


## Pass 72: require the raw native song endpoint

ITGmania retains the raw Song::GetLastSecond value for its end timing. The harness beat/time roundtrip could stop below LASTSECONDHINT, and its selected chart could end before another chart in the song. Five compiled native controls cover fractional hints, both offset signs, a non-dyadic offset and a later chart. Four controls miss the native end callback on harness .30; all five reach it on .31.

DeadSync .1869 requires native endpoint metadata, the final native music timestamp and the last update frame to cover that endpoint. A native positive control and six mutations verify rejection of missing, short or nonfinite endpoints. The old clean Igaku capture, which passed 514,954 comparisons, is now rejected because that agreement did not prove complete coverage. Its .31 recapture reaches the original raw hint with zero runtime errors and no dropped events; full parity verification remains pending.

The corrected Bank Account archive passes all 116,603 comparisons. Its previous bytes and aliases are preserved. Scope remains 501 original simfiles, 483 canonical archives, nine historical variants and 18 unindexed sources. The old pass 71 stopped at 12 archives when the necessary harness source change activated its guard. The full corpus must still be revalidated under the corrected endpoint contract.

Archive 1: (R10) Warp Zone/warp zone.ssc passes all 212220 comparisons. The verified prefix is now 1/492 archives; full-corpus parity remains unfinished.

Archive 2: (R5) Let Me Hear That/let me hear that.sm passes all 205071 comparisons. The verified prefix is now 2/492 archives; full-corpus parity remains unfinished.

Archive 3: (R6) Waltz Capriccio/waltz_capriccio.ssc passes all 260300 comparisons. The verified prefix is now 3/492 archives; full-corpus parity remains unfinished.

Archive 4: 100 Bad Days/100 Bad Days.sm passes all 175181 comparisons. The verified prefix is now 4/492 archives; full-corpus parity remains unfinished.

Archive 5: 100 Bad Days/100 Bad Days.ssc passes all 175181 comparisons. The verified prefix is now 5/492 archives; full-corpus parity remains unfinished.
