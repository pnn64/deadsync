# OpenITG theme progress

Choose **OpenITG** in **Options → System → Theme**, then restart DeadSync.
The equivalent setting is `Theme=OpenITG` under `[Options]` in `deadsync.ini`.
Simply Love is the default. The selection mounts that theme's title, System
Options, Profile Picker, Select Style, Select Play Mode, Profile Load, Player
Options, the song and course wheels, supported gameplay HUD widgets, and
Evaluation overview. Both themes use the same DeadSync session flow.

## Reference

The local `openitg-master` source archive has no Git metadata. Relevant files
match upstream snapshot `f2c129fe65c65e4a9b3a691ff35e7717b4e8de51`. The
home release overlays `assets/d4`, `assets/game-data`, and `assets/patch-data`
in that order, as defined in `generate-home-release.sh`. `GamePrefs.ini`
selects `home` for dance. Theme lookup follows `home`, `default`, then
`fallback`. Patch data replaces the base `default/metrics.ini`.

The title layout and behavior were read from the home metrics, effective
default and fallback metrics, and `ScreenTitleMenu` actors and redirects.
The reference uses a 640×480 logical area, a first row at `(120, 112)`, a
36 px pitch, immediate wrapping selection, a 110% focus scale with a 0.5 s
pulse, and 0.3 s entry and exit fades. These are source observations. The
archive did not include a runnable Windows reference executable.

## Title screen

- The theme renders original DeadSync graphics with the reference column and
  animation timings. It uses shared text rendering, the bundled game font,
  and DeadSync's common interaction cues.
- Home mode offers Play, Options, Find Content, Exit, and Shutdown when enabled.
  Free and Pay mode expose Play and use the same host credit rules as Simply
  Love. Both title implementations use the shared menu availability contract.
- The reference's Dance, Battle, Marathon, Fitness, Edit, and Records rows
  differ from the current DeadSync actions. The reference logo, background,
  typography, and audio also differ. Pixel comparison against a running
  OpenITG reference is pending.

The focused tests exercise configuration round-tripping, both titles' P2
session entry, arcade final-credit Start, home navigation, and actor placement
at 640×480 and 854×480. Architecture checks cover both title mounts. Cargo's
locked dependency graph confirms the concrete themes have no dependencies on
each other.

## System Options

The OpenITG presentation covers the service menu and its nested settings
pages. The existing controller selects the rows, applies edits, validates
graphics and input changes, and sends requests to the shell. Both layouts
read the same formatted values and multiselect bits. Search, reload, pack
and updater workflows, and confirmation dialogs use their existing overlays.

The effective `ScreenOptions` reference places the heading at `(36, 38)`,
eight rows at y=127 with 28 px spacing, the scrollbar near `(34, 242)`, and
the Done row at y=353. The original panel graphics use these anchors at
640×480 and center the layout at 854×480. The panel slides between submenus
on DeadSync's existing transition clock. The menu presents supported
DeadSync actions in their current order, with DeadSync's cues and persistence
semantics. Reference cursor tweens, header motion, cues, and modal styling
remain open.

Tests cover the viewport and Done row, prepared values, disabled and
multiselect rows, an edited volume appearing in the alternate renderer, and
the reload modal taking over that renderer.

At 854×480, nine warm actor-construction runs of 12,000 frames per layout
measured medians of 32.6 μs for Simply Love and 19.1 μs for OpenITG on the
service menu, and 59.8 μs and 30.2 μs on Graphics Options. The fixture used
the bundled fonts and dev-profile theme libraries.

## Player Options

Choose Player Options through the usual second Start on a confirmed song. The
OpenITG layout presents the same four editor panes and row choices as Simply
Love. Both layouts support chart changes where allowed, X/C/M speed and music
rate, perspective, noteskin and judgment previews, modifier toggles, and
setting search. Each joined side has its own focus. Both layouts use the same
profile updates, stage submission, course validation, and failed-load recovery.

The effective reference uses the `ScreenPlayerOptions` section in patch
`default/metrics.ini` and the parent `ScreenOptions` metrics. The row labels
start at x=53, rows begin at y=127 with 28 px spacing, and Exit sits at y=353
on the 640×480 layout. The layout uses those anchors and fits DeadSync's
ten-slot scrolling window between the first row and Exit. The shared editor
supplies the row order and navigation, including the optional arcade bindings.
The reference's 16-row grouping, scrollbar, artwork, and cue comparisons remain
open. Timing scaling and workout overrides remain governed by DeadSync's rules.

Tests cover P2-only perspective edits, profile requests, stage recovery and
resubmission, and noteskin preview requests through both layouts. Actor checks
cover separate versus focus and enabled choices, the arcade next-row affordance,
and row anchors at 640×480 and 854×480.

At 854×480, nine warm runs of 8,000 actor builds with dev-profile theme libraries
measured Simply Love at 60.6 µs before the change and 60.2 µs afterward in solo,
and 78.7 µs and 79.3 µs in versus. The OpenITG layout measured 38.8 µs in solo
and 48.9 µs in versus. These timings cover settled main-pane actor construction
with bundled fonts.

The broader shell run still hits `late_foreground_keeps_shared_gameplay_tracks`,
which reproduces at the same assertion on `703cb0d25`. A follow-up run confirmed
that the failure poisons the shared session lock for `live_lua_screen_frames`.

## Profile Picker

The standalone profile picker and the music wheel's fast-switch and late-join
overlay use the same OpenITG panels. Each physical side shows its own local
profiles, guest choice, avatar, played-song count, and confirmation status.
Both themes use `deadsync-profile` for joining, undoing readiness, and rejecting
a local profile already confirmed on the other side. The existing shell still
applies the selected profiles and resumes the appropriate route.

OpenITG's `GameState::PlayersFinalized` loads the first available profile from
the cabinet memory card. DeadSync presents its own local profiles and guest
entries with original two-player panels. A live cabinet comparison of profile
feedback remains open.

At 854×480 with both players joined, the median of seven 8,000-frame batches
was 43–44 µs before and 42–43 µs after for Simply Love, and 8.9 µs for OpenITG.
The benchmark measures steady-state actor construction with bundled fonts
and dev-profile libraries.

## Select Style and Play Mode

The style and mode screens present the shared controller's current choices,
confirmation progress, and joined players on a rotating selector. The effective
`ScreenSelectStyle2` and `ScreenSelectPlayMode` metrics supply the radius, depth
spacing, dimming, and 0.3 s per item. The layout centers the 640×480 canvas in
wider viewports. It uses original pad and meter illustrations, a flat projection
of the reference ring, and the existing DeadSync selection and confirmation
cues. Labels stay readable on the far side of the ring.

DeadSync's flow shows Style first and retains its Regular and Marathon modes.
Premium Free appears when the configured session permits it. Mode descriptions
use the configured language and wrap to the panel. The three short style
explanations are in English. Color selection uses the existing screen. The
reference's perspective, artwork,
explanation panels, and transitions still need comparison against a live build.

Actor checks cover the native and wide anchors, available modes, joined-player
status, wraparound, and changing selection mid-animation. At 854×480, two
runs of seven 8,000-frame batches measured median actor construction at
26–29 µs before and 26–27 µs after for Simply Love Style, and 57–62 µs before
and 58–59 µs after for Simply Love Mode. The OpenITG layouts measured about
21 µs for Style and 15 µs for Mode. The benchmark covers steady-state actor
construction with dev-profile libraries and bundled fonts.

The ordinary-song compositor comparison still reports a camera mismatch for
Single/P1 with song visuals visible. Three shell test binaries built before
this change reproduce that assertion. The cross-theme result and live Lua
score checks pass.

## Select Music

The base layout draws the live wheel, selected song, prepared banner, player
and difficulty information, and the shared launch prompt. Left and right still
browse the same catalog. Song confirmation, previews, chart shortcuts, and
the second Start into Player Options use the existing controller. Search,
sort, lobby, pad tools, and other auxiliary screens keep their existing
presentation over the wheel.

The source comparison uses the home release's effective `ScreenSelectMusic`
and `MusicWheel` metrics and `WheelBase.cpp`. The layout uses the source
wheel center `(212, 222)`, 15 rows on a radius-200 arc, the banner at
`(480, 112)`, and a 20.5 px difficulty-row pitch. Actor tests check the 640×480
and 854×480 layouts. The renderer uses original graphics. Exact 3D perspective,
banner framing, score panes, and special-item presentation are pending.

Circular indexing now gives short wheels the same wrap order in navigation,
media requests, score requests, and drawing. An index-only benchmark on lists
of 3, 128, 701, and 10,000 entries measured medians of 4.78 ns before and
5.60 ns after the fix. Full-frame timing remains to be measured.

## Profile Load and Select Course

The loading screen advances once the worker supplies a prepared song or course
wheel and the minimum display time has elapsed. The OpenITG loading animation
uses elapsed time. It remains visible if catalog preparation takes longer.

OpenITG's effective `ScreenSelectMusicCourse` inherits `ScreenSelectMusic`. Its
header actor reads “PICK A COURSE”. The course browser uses the song wheel's
arc and banner anchors with DeadSync's current course, rating, track list,
BPM, duration, authored description, and records. It shows when a rating can
be changed and highlights rating focus in three-button navigation. Both themes
use the same course launch and second Start into Player Options. The exit
confirmation uses the existing overlay. The track-list window steps through
long courses. The original header tween, artwork, and live reference captures
remain open.

The shell's course fixture checks that the installed OpenITG layout draws the
prepared rating and tracks. Course launch, recovery, and evaluation tests pass.
At 854×480, paired runs of seven 8,000-frame batches measured 19.6–19.8 µs
before and 19.5–20.0 µs after for song-wheel actor construction. The course
wheel measured 27.4–27.5 µs with six tracks and two records. These are
steady-state actor-build timings using dev-profile libraries.

## Gameplay HUD

The OpenITG theme draws a vertical Standard life bar and a two-decimal
percentage score from the same stage values used by Simply Love. The shell
selects these widgets once at stage entry. Draws occur inside the existing
HUD and Lua score proxy layers, with the same profile visibility, physical
player side, and gameplay state.

The effective `ScreenGameplay` metrics place the life frames 24 units in from
the outside edges at y=270, with a 272-unit bottom-up fill. The frame leaves a
16-unit inset above the painted area, corresponding to the transparent tip in
the rotated reference graphic. One-side scores use the screen quarters with
offsets of −20 for P1 and +18 for P2. Doubles uses offsets of ±140 from screen
center. Both forms place the score at y=56. Animated life glow, song and stage
banners, and specialized score and life modes remain open.

The effective Gameplay entry backdrop resolves to `_short`, an empty
animation. Once stage loading finishes, the OpenITG playfield is visible
during the shared two-second entry. The life bars hold 100 units outside their
settled x positions and the score holds 100 units above its settled y for 0.5
seconds. Both slide into place over 0.8 seconds with quadratic deceleration.
During the results transition they accelerate off by the same distance over 0.8
seconds. Loading and restart use the shared covers. The shell applies song Lua's
intro suppression and supplies the stage clock that also governs input and audio.

Paired 600-frame Windows development runs at 854×480 used the same chart,
configuration, and audio. OpenITG averaged 8.56 ms per frame before the change
and 8.57 ms afterward. Mean draw time was 2.09 ms and 2.05 ms respectively.
The settled life and score regions matched pixel for pixel.

Actor tests cover native and wide layouts, entry and exit positions, failed
life, and EX score tint. A song Lua fixture captures the animated OpenITG score
with Underlay hidden and still draws the canonical player proxy. The existing
independent footer fade test fails at 0.75 seconds on the known baseline.

An actor-construction benchmark drew two scores per frame at 854×480. Across
nine runs of 400,000 frames, the median was 1.05 μs for the Simply Love digits
and 0.75 μs for the OpenITG inline label. Both used optimized widget code with
shared dev-profile rendering dependencies and a reused actor buffer. Scene
composition, GPU timing, and live frame pacing remain to be measured.

### Judgment and combo

Gameplay uses the shared notefield to draw an original tap atlas and a combo
label. The OpenITG default graphic has two timing columns and seven grade
rows. The shell loads it from bytes embedded in the theme crate when it starts
or recreates the renderer. Profile-selected custom graphics and song Lua
textures use their existing asset and animation paths.

The default tap uses the reference’s 0.1-second stretch and settle, one-second
hold, and 0.2-second shrink. The feedback layout uses the existing field center
and Reverse and Centered inputs. At 640×480 and zero Centered, the tap marker
anchors at y=210 normally and y=270 in Reverse. The combo number anchors at
y=290 and y=240 respectively. The label is placed below the number to space
the shared fonts. The number grows with the displayed count and pulses for
0.05 seconds after a tap, using the stage’s existing feedback timestamp.

The canonical HUD keeps the combo digits, label, and judgment sprite in their
existing player and Lua capture ranges. Draw-range tests exercise both direct
and captured output. Reference-specific alternating tap tilt, beginner timing
wording, hold artwork, and the full-combo color cycle remain open.

In an optimized two-player combo draw benchmark, nine runs of one million
frames gave medians of 65.1 ns for Simply Love before the change, 71.4 ns for
Simply Love with the shared style input, and 247.2 ns for OpenITG with the
additional label and pulse. These timings cover draw command construction.

## Evaluation

The OpenITG overview uses DeadSync's completed-stage score, grade, judgment
counts, hold and roll results, avoided mines, hand counts, and life history.
It shows the saved personal and machine ranks, disqualification, and the
configured EX scores. Routine shows its shared result with the participant's
score beside it. Maximum combo comes from the recorded gameplay peak. Course
results retain the highest stage peak for each side, including recorded carry.

The layout follows the effective `ScreenEvaluation` and `ScreenEvaluationStage`
metrics. Single retains its physical player seat, versus draws both result
panels, and doubles uses the reference inward shift. The grade slides in over
0.5 seconds, settles by 3.0 seconds, and the banner and panels finish entering
at 3.3 seconds. Automated captures wait for that reveal. The life line uses
the existing sampler with a cached mesh at the requested dimensions.

DeadSync still owns saves, submissions, the course page sequence, replay and
practice handoffs, and onward routing. Timing, records, online detail panels,
and other auxiliary panes retain their existing presentation and controls.
Evaluation uses original artwork and DeadSync's existing entry and record
cues. Jump totals, the combo-quality strip, course graph separators, and
reference capture and audio comparisons remain open.

At 854×480, nine warm runs of 8,000 settled-frame actor builds measured
34.9 μs for Simply Love and 22.3 μs for OpenITG in solo, and 33.2 μs and
39.3 μs in versus. The OpenITG versus overview builds 96 top-level actors
for its two panels. The benchmark covers actor construction with bundled
fonts and dev-profile theme libraries. Evaluation GPU timing remains open.

## Gameplay parity verification

The shell regression test `simply_love_and_openitg_produce_identical_gameplay_results`
feeds the same timestamped keyboard edges and audio clock into two copies of a
dance-single stage. It renders both HUDs and checks the judgments, life history,
combo, and failure state each frame. The passing and failing runs include an
early Fantastic, a late Excellent, and a wrong-lane press followed by a Miss.
At completion the test compares the recorded maximum combo, grade points,
ITG and EX scores, and fail time.

## Windows release verification

The optimized Windows x86-64 build passed `cargo build --release --locked --offline
--bin deadsync`. The archive created by `scripts/package-windows-release.ps1`
passed its checksum check and contains the standard assets, songs, and courses.
The OpenITG tap judgment graphic is embedded in the archived executable.

Fresh extractions reached the event loop on ordinary portable startup. The
`--perf-case` harness then rendered a synthetic dance-single chart using Vulkan
through wgpu and WASAPI shared audio. Both widescreen cases used the same
executable, chart, audio, and configuration apart from the theme setting.
Each case recorded 600 frames at a 120 FPS cap and saved a gameplay capture.

| Theme | Window size | Mean frame | Mean draw |
| --- | --- | ---: | ---: |
| Simply Love | 854×480 | 8.35 ms | 1.13 ms |
| OpenITG | 854×480 | 8.35 ms | 1.10 ms |
| OpenITG | 640×480 | 8.38 ms | 1.13 ms |

The release captures exposed a life bar overlapping the left receptor. Follow-up
development captures verified the corrected frame and score positions at 640×480
with a 4:3 layout and at 854×480 in solo and versus. The frames sit at the outside
edges and clear the receptor sprite. In 4:3, the hit glow can still reach the
upper rail.

For two life bars and two scores, nine runs of 200,000 actor builds measured a
median of 1.34 µs before the correction and 1.33 µs afterward at 640×480. The
854×480 medians were 1.34 µs and 1.32 µs. The interactive menu-to-Evaluation
walkthrough, comparisons against a running reference, and distribution checks
on other platforms remain open.
