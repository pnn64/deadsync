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

## Next investigation: wrappers created after startup

An independent four-second native probe creates a bob wrapper from a queued
self callback at t=1. It uses no broadcasts or random calls, so recipient
order cannot explain its failure. Matching noteskin assets, 241 native update
frames, no errors/dropped events and the complete endpoint are verified.
Archive 37e8ced29ca9dec62328bae8488822ac45a95ea7c7e969097d730bef6191fdb3
passes 5,586 and fails 24 of 5,610 comparisons on immutable 0.5.1851. Every
failure is projected geometry. At beat 1.25 ITGmania's quad center is
[410.39557, 244.15823], while DeadSync keeps [400, 240]. The suspected cause
is the overlay capture list built before chronological callbacks create new
wrappers; its source fix is pending. Native Actor.cpp is byte-identical in the
workspace and linked tree. Actor::AddWrapperState, wrapper-before-owner
Update, Draw's wrapper stack and the bob formula are recorded with hashes
and anchors in pass36-late-wrapper-native-investigation.json. The probe is
retained only under harness target/song-lua-repair/pass36-late-wrapper-probe;
it is unindexed and absent from the flat corpus fixtures. An initial capture
using the other noteskin tree was rejected before any comparisons, and is
not treated as evidence of the behavior gap.
