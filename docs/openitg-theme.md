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
archive ships source and a legacy Windows project. An isolated Win32 build
now runs the home overlay in a 640×480 OpenGL window on Windows. It uses
Visual Studio 2022 and compatible Ogg and Vorbis libraries built from the
local ITGmania source archive. The build uses the archived theme files and
game rules. Build logs and individual compatibility adjustments are under
`target/openitg-native-reference`. A focused-window keyboard compatibility
change enabled the automated observation. The native capture record covers
the title, style selection, song wheel, gameplay, and Evaluation.

## Title screen

- The theme renders original DeadSync graphics with the reference column and
  animation timings. It uses shared text rendering, the bundled Miso font,
  and original OpenITG theme movement and confirmation cues.
- Home mode offers Play, Options, Find Content, Exit, and Shutdown when enabled.
  Free and Pay mode expose Play and use the same host credit rules as Simply
  Love. Both title implementations use the shared menu availability contract.
- The reference's Dance, Battle, Marathon, Fitness, Edit, and Records rows
  differ from the current DeadSync actions. The reference logo, background,
  typography, and audio also differ. The two 640×480 captures confirm the
  shared first-row anchor and pitch. The native menu runs directly from Dance
  Mode through style to the song wheel. DeadSync keeps its separate mode step.

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

Graphics Options opens a basic page with display mode, aspect ratio, resolution,
and the Advanced Graphic Options link. The advanced page contains renderer,
refresh rate, presentation, and performance settings. Both pages edit a single
set of pending display settings. Back or Done from Advanced returns focus to
its launcher, and leaving the basic page applies the pending settings together.
Settings search can jump between the pages during an edit. Overscan returns to
its row in Advanced and preserves pending display changes. Simply Love presents
the combined Graphics Options page.

The route follows `ScreenGraphicOptionsBasic` in the home theme's
`metrics.ini`. The Graphic Quality preset in `Scripts/SimpleGraphics.lua`
controls legacy color depths, texture limits, caches, and filtering. That
preset and the reference's explicit row spacing remain open.

The effective `ScreenOptions` reference places the heading at `(36, 38)`,
eight rows at y=127 with 28 px spacing, the scrollbar near `(34, 242)`, and
the Done row at y=353. The OpenITG panel uses these anchors at 640×480 and
centers the layout at 854×480. The panel slides between submenus
on DeadSync's existing transition clock. The menu presents supported
DeadSync actions in their current order, with shared persistence semantics.
Reference cursor tweens, header motion, and modal styling remain open.

Tests cover the viewport and Done row, prepared values, disabled and
multiselect rows, an edited volume appearing in the alternate renderer, and
the reload modal taking over that renderer. Graphics tests cover both return
paths, a single deferred apply, settings search, and pending display changes
surviving an Overscan round trip. Vulkan captures at both supported fixture
sizes exercise the route through the shared controller and show the launcher
focus, the advanced title, and the centered row layout.

At 854×480, nine warm actor-construction runs of 12,000 frames per layout
measured medians of 32.6 μs for Simply Love and 19.1 μs for OpenITG on the
service menu, and 59.8 μs and 30.2 μs on Graphics Options. The fixture used
the bundled fonts and dev-profile theme libraries.

Two repetitions of the same warm actor-construction benchmark measured
19.4–19.5 μs for Basic and 27.1–27.5 μs for Advanced. The previous combined
OpenITG page took 28.9–29.1 μs. Simply Love's combined page measured
57.7–57.9 μs before the split and 57.9–59.8 μs with the new controller.

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
The reference's 16-row grouping, scrollbar, and artwork remain open. Timing
scaling and workout overrides remain governed by DeadSync's rules.

Tests cover P2-only perspective edits, profile requests, stage recovery and
resubmission, and noteskin preview requests through both layouts. Actor checks
cover separate versus focus and enabled choices, the arcade next-row affordance,
and row anchors at 640×480 and 854×480.

At 854×480, nine warm runs of 8,000 actor builds with dev-profile theme libraries
measured Simply Love at 60.6 µs before the change and 60.2 µs afterward in solo,
and 78.7 µs and 79.3 µs in versus. The OpenITG layout measured 38.8 µs in solo
and 48.9 µs in versus. These timings cover settled main-pane actor construction
with bundled fonts.

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

## Select Music

The base layout draws the live wheel, selected song, prepared banner, player
and difficulty information, and the shared launch prompt. Left and right still
browse the same catalog. Song confirmation, previews, chart shortcuts, and
the second Start into Player Options use the existing controller. Search,
sort, lobby, pad tools, and other auxiliary screens keep their existing
presentation over the wheel.

The source comparison uses the home release's effective `ScreenSelectMusic`
and `MusicWheel` metrics and `WheelBase.cpp`. The layout uses the source
wheel center `(212, 222)`, a radius-200 arc, a 20° parent yaw, and a 58°
horizontal perspective centered at `(320, 222)`. The cards and their labels
tilt together. The shared view retains 17 rows so the edge cards can enter as
the source's 15-row window moves. A fixed selection border pulses on the
preview beat. The wheel uses a curved detail divider, an inset banner, an
artist and BPM rail, and the 20.5 px difficulty-row pitch. The option offer
appears in the footer so the confirmed song stays visible.

The native 640×480 Quartz capture confirms the tilted rows and the fixed
selection area. Vulkan captures of the DeadSync Quartz view checked the same
title, artist, BPM, and Hard 5 chart at 640×480 and 854×480. Additional
two-player captures check catalog labels, records, and the highlight during
movement. The artwork and proportions use DeadSync's presentation.

The OpenITG song wheel includes Chance and Random in that order whenever
the active sort contains a playable chart for the current style. The
effective English metrics call these native wheel actions `TYPE_RANDOM`
and `TYPE_PORTAL`. Chance reveals a song from the active sort and locks
the wheel around it. A subsequent Start confirms the song, with the usual
option to enter Player Options. Random previews a resolved song while the
Random row stays focused. Its chart panes, banner, and confirmation use the
same choice until focus leaves the row.

The game prepares one candidate pool for the active sort and play style.
It prefers the joined players' standard difficulties and gives Random the
open section when possible. The pool is refreshed on sort or style changes.
The input route verifies Chance through Player Options and Evaluation and
Random through direct launch. Vulkan Quartz fixtures at 640×480 and 854×480
check both labels, the resolved chart pane, and the dimmed neighbors on the
Chance lock. The native animated labels, idle music, and locked-wheel cue
remain open.

In the dev profile, choosing from 20,000 songs dropped from a median of
37.7 ms with chart inspection on each choice to 1.14 ms with a prepared
pool. Preparing a different sort at that size took 37.7 ms. The 854×480
two-player fixture measured 43.5–43.7 µs for settled actor construction
and 53.0–53.7 µs while moving across the two builds. The Quartz fixture
with the two actions built in 32.6 µs when settled and 41.2 µs while moving.

OpenITG's song and course banners slide in with a 750 ms deceleration and
slide out with a 750 ms acceleration. The first accepted confirmation starts
the exit. The stage-launch controller still handles the second Start and
decides when to enter Player Options or Gameplay. The effective patch's
`FadingBanner` commands drive a 500 ms corner-by-corner fade of the outgoing
image over the replacement. DeadSync keeps up to four outgoing textures
through that animation and reuses them when revisiting a banner. Video
banners retain the texture they last displayed.

Vulkan captures at 640×480 and 854×480 verify corner order at 0, 125, 250, and
375 ms with two contrasting fixture textures. The wide entry and exit
captures keep the side margins clear. A Vulkan media check verifies repeated
selection, the four-texture bound, expiration, and clearing the outgoing
layers on screen change. Motion timing is verified against the effective
source metrics. A timestamp-matched native animation capture and the native
low-to-full-resolution cache upgrade remain open. DeadSync's text fallback
is shown under an outgoing image and gives way directly to the first loaded
image.

In the 854×480 two-player fixture, median actor build time went from 44.0
to 45.6 µs when settled and from 53.6 to 57.4 µs while scrolling with one
outgoing image. Median CPU composition stayed within 143–146 µs, with one
additional draw in the fade. These measurements use warm actor and
composition buffers and dev-profile libraries.

Each joined side's chart pane shows steps, jumps, hands, holds, rolls, and
judgable mines. The machine record and holder and the personal best use the
same local ITG snapshots as Simply Love. When the selected chart changes, the
previous record disappears until the matching snapshot is ready. The panes
appear for joined sides and display personal bests when the profile has one.
The effective patched `ScreenSelectMusic` enables panes while hiding the older
integer score fields. `PaneDisplay.cpp` supplies the six counts and two local
records. The source's pane entry and exit animation and separate song
popularity indicator remain open.

Vulkan captures at 640×480 and 854×480 verified the title and two-player
music wheel with the bundled Miso font, with a fixture supplying both local
records. The title, wheel rows, and chart counts render with letter and number
glyphs. The configured play route also checks the P2 pane against the chart's
actual tap count and checks that its font contains the displayed characters.
At 854×480, two sets of seven 8,000-frame batches measured median actor
construction at 22.5–23.3 µs before and 35.7–36.6 µs with both panes and
their records. The layout builds 90 actors for this fixture, up from 60.

Circular indexing now gives short wheels the same wrap order in navigation,
media requests, score requests, and drawing. An index-only benchmark on lists
of 3, 128, 701, and 10,000 entries measured medians of 4.78 ns before and
5.60 ns after the fix.

The perspective milestone used an 854×480 two-player wheel fixture with
bundled fonts and warm actor and composition buffers. Median actor build time
went from 40.27 to 42.73 µs when settled, and from 39.54 to 52.17 µs while
scrolling. CPU composition went from 120.72 to 142.33 µs when settled, and from
119.35 to 143.21 µs while scrolling. The projected cards increase the wheel
from 78 to 93 draw operations and use a separate camera pose for each visible
card. GPU wheel timing remains open.

In matching portable Windows gameplay runs over 600 measured Quartz frames
per theme, p95 frame time went from 8.69 to 8.64 ms for OpenITG and from 8.66
to 8.62 ms for Simply Love after the camera extraction. All four runs had
zero measured buffer growth and zero audio underruns.

## Theme resources and menu cues

The effective home and patch overlays inherit the default theme's core cue
routing. `ScreenSelectMaster.cpp` plays a title movement cue and the common
confirmation. `ScreenOptions.cpp` plays different row and value cues. The
music wheel has its own movement and section expansion sounds. Difficulty
changes use their own feedback, and Player Options retains the song preview.

The OpenITG theme bundles original WAVs for those interactions. Each cue is
prepared once at startup for the output device and played by its retained
handle. Both themes use the same wheel, option, and chart controllers. The
OpenITG tap atlas also loads from the selected theme's manifest. Menus and
overlays still use Simply Love's fallback assets for effects outside the
installed cue set. The title entry stinger, sort and lock feedback, Evaluation
entry audio, and comparisons with the reference audio remain open.

All nine cues match the file-backed decoder sample for sample at 24 kHz mono,
44.1 kHz stereo, and 48 kHz stereo. The Windows packager's development-build
smoke archive contains the nine WAVs and the tap atlas inside the executable.
Separate portable starts prepare the OpenITG cues only for the OpenITG setting.

Before and after the resource change, both themes rendered the same 600-frame
fixture at 854×480 using Vulkan through wgpu and WASAPI shared at 48 kHz.
OpenITG averaged 8.56 ms in both runs. Simply Love averaged 8.57 ms before
and 8.56 ms after. All four runs reported zero audio underruns. Quartz's
foreground Lua draws a second judgment through an ActorProxy at its authored
offset. Both themes show one built-in tap judgment when that Lua is disabled.

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
wording and the full-combo color cycle remain open.

The stock OpenITG hold atlas supplies two original 78×44 badges for completed
and released holds and rolls. The shared compositor positions each badge at
its lane and interpolates from −90 to +90 with reverse. Its scale settles from
1.25× to 1.0× over 0.3 seconds and the badge disappears at 0.8 seconds.
Release badges alternate −20° and +20° in each lane. Completed holds advance
that lane's phase. The lookup preserves custom and disabled hold graphics. The
stock OpenITG badge is reserved for outcomes after a hit head. Missed heads
use the tap feedback and the player's separate held-miss setting.

These metrics are from the effective patched `[HoldJudgment]` section,
inherited Player metrics, `HoldJudgment.cpp`, and `Player.cpp`. The tap Judgment
proxy and hold badges keep their existing composition ranges.

Vulkan captures show the completed hold in OpenITG at 640×480 and 854×480
and in Simply Love at 854×480. The 640×480 left-lane badge can touch the
outer life rail. Notefield tests cover both release angles, the atlas cells,
zoom, missed-head behavior, and expiry. Both themes ran the same 600-frame
fixture before and after with WASAPI shared audio. Mean frame times across
the six runs ranged from 8.50 to 8.57 ms, with zero audio underruns. A fresh
Windows portable smoke archive embeds the hold and tap atlases alongside the
nine audio cues. Both theme configurations reached the event loop from that
archive.

The shared song-Lua host now loads the sprite frames for generated tap and hold
judgment actors when it binds their textures. The 16 live actor regression cases
pass, including the released-hold frame, tap grades, early and late offsets,
and authored callbacks that run between feedback phases. The wider live-scene
suite passes 141 of 152 cases. Both judgment-tree cases now pass. The 11
remaining failures also occur on the pre-change build.

The shared notefield now limits both forms of a Judgment capture to tap
feedback. The hold sprite and the held-miss indicator stay with the lane and
remain available to whole-Player captures.

In an optimized two-player combo draw benchmark, nine runs of one million
frames gave medians of 65.1 ns for Simply Love before the change, 71.4 ns for
Simply Love with the shared style input, and 247.2 ns for OpenITG with the
additional label and pulse. These timings cover draw command construction.

## Evaluation

The OpenITG overview uses DeadSync's completed-stage score, grade, judgment
counts, jump totals, hold and roll results, avoided mines, hand counts, and
life history.
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

Jump totals use completed judgments and the playable tap and hold-head rows.
Finite course summaries add the planned jumps on unplayed songs to the
possible total. The counts use the reference's three-digit padding, and the
maximum combo is padded to five digits. The strip shows uninterrupted runs
of at least five below the life line. It labels every longest run and excludes
the combo brought into the stage. Course summaries join runs carried between songs
and mark song boundaries on the life graph. Both graphs use the shared
result timeline.

DeadSync still owns saves, submissions, the course page sequence, replay and
practice handoffs, and onward routing. Timing, records, online detail panels,
and other auxiliary panes retain their existing presentation and controls.
Evaluation uses original artwork and DeadSync's existing entry and record
cues. Life-graph coloring by judgment quality and reference audio comparisons
remain open.

The native 640×480 Quartz capture was taken after the panel entrance, with
P1 Single, Hard 5, and AutoPlay. It confirms the Jumps row above Holds and
the graph area above the counts. The reference shows the round label below
the header and modifiers above the percentage. DeadSync's overview places
modifiers at the bottom of the panel and still needs the round label. Native
AutoPlay produced an empty combo graph. A DeadSync Vulkan fixture supplied
the same Quartz result to compare the counts, rank-free layout, and empty
graph at 640×480. Separate solo and versus captures at 640×480 and 854×480
showed filled runs, maximum-run labels, and record ranks.

At 854×480, nine warm runs of 8,000 settled-frame actor builds measured
34.9 μs for Simply Love and 22.3 μs for OpenITG in solo, and 33.2 μs and
39.3 μs in versus. The OpenITG versus overview builds 96 top-level actors
for its two panels. The benchmark covers actor construction with bundled
fonts and dev-profile theme libraries. Evaluation GPU timing remains open.

The jump and combo display comparison used nine 8,000-frame batches per
layout at 854×480 with three drawn combo runs per player. Median OpenITG
actor construction was 22.1 µs before and 28.0 µs afterward in solo, and
39.4 µs and 52.0 µs in versus. The layouts added six actors per player.
The padded counts use inline text formatting. Simply Love measured 34.4 µs
and 33.1 µs in solo, and 32.9 µs and 33.2 µs in versus.

In paired 600-frame Windows gameplay runs with Vulkan through wgpu and WASAPI,
95th-percentile frame time was 8.61 ms before and 8.69 ms after in OpenITG,
and 8.62 ms and 8.66 ms in Simply Love. All runs recorded zero audio underruns
and zero buffer growth events. The full shell, gameplay, and score suites
passed. The final overview layout also passed the OpenITG unit tests and the
configured play route and gameplay parity regressions.

## Configured play route

The Windows shell test `configured_themes_complete_song_route` loads each
theme through a fresh `deadsync.ini` in its own process. It starts at the title
and sends P2's Start through the normal input router for profile selection,
style, play mode, and Select Music. Two Start presses enter Player Options
with the confirmed chart. The test accepts the options, plays six timestamped
taps, checks that the resulting stage records six hits on P2 alone, and
checks that Evaluation renders the song title and returns to Select Music.
The test drives the application's idle-screen update and advances gameplay
with a deterministic audio clock in a windowless event loop.

## Gameplay parity verification

The shell regression test `simply_love_and_openitg_produce_identical_gameplay_results`
feeds the same timestamped keyboard edges and audio clock into two copies of a
dance-single stage. It renders both HUDs and checks the judgments, life history,
combo, and failure state each frame. The passing and failing runs include an
early Fantastic, a late Excellent, and a wrong-lane press followed by a Miss.
At completion the test compares the recorded maximum combo, grade points,
ITG and EX scores, and fail time.

The native and scripted frame paths now resolve the notefield perspective from
the same Player-position rules. The render-frame comparison passes for Dance
and Pump in solo, double, and versus, with both physical sides and changes to
song-visual and HUD visibility. The late-foreground fixture checks the
projected depth-tested quad after its authored screen movement. The score-proxy
fixture checks that the gameplay exit prompt fades with the song's Overlay
while the system footer and Stage label retain their own alpha. The whole-Player
proxy regression verifies the separate Combo, field, and Judgment segments.

Before and after the camera change, both themes rendered 600 frames of the
same dance-single fixture at 854×480 with Vulkan through wgpu and WASAPI
shared audio. Mean frame time was 8.56 ms in all four runs. The 95th percentile
ranged from 8.61 to 8.66 ms, with zero audio underruns.

The Song Lua frame repeatability and top-screen AFT tests pass with each
Judgment proxy using its intended capture path. A dedicated regression checks
that both themes can draw two root Judgment proxies from a retained HUD
range. The live Lua fixture checks marker positions and counts using the
renderer’s packed RageColor values.
The full shell suite passed with 450 tests passing and 11 ignored.

Separate Windows captures at 854×480 show the chart's root Judgment proxy
draw at its authored offset in both themes. In the 600-frame baked-Lua case,
OpenITG averaged 8.56 ms before and after. Simply Love measured 8.72 ms before
and 8.56 ms after. All four Vulkan and WASAPI runs recorded zero audio underruns.

With song Lua set to Runtime, the 600-frame Quartz comparison averaged 8.57 ms
in Simply Love on both builds. An OpenITG repeat after the test build finished
averaged 8.74 ms before and 8.60 ms after. Its 95th percentile was 8.78 ms before
and 8.67 ms after. These captures recorded zero audio underruns and zero buffer
growth events. That Quartz script queued zero tween time on its foreground.
Runtime expired the foreground at its start beat, following the lifetime
rule in OpenITG's and ITGmania's `Foreground.cpp`. Baked playback now applies
that rule to visual foregrounds using the full actor tree's tween time after
`On`. The foreground draws at the stop beat. Once the song clock passes that
beat, actor draws and proxy requests stop.

A Quartz foreground with three seconds of authored tween time drew the
offset tap judgment in both Lua modes and both themes at 854×480. The
shell regressions check expiry in both modes and exercise a later foreground's
independent stop beat and reappearance after seeking backward. In the early-hold
Quartz capture, the lane badge and both tap
judgments are visible after the capture fix. The copied hold badge is gone
from both themes. The paired 600-frame Baked runs at 854×480 measured
8.56 ms before and after in OpenITG and 8.56 ms before and 8.57 ms after
in Simply Love. All four runs reported zero audio underruns or buffer
growth events.

Windows captures at 854×480 show both themes drawing the Quartz proxy in Baked
mode with three seconds of tween time. With zero tween time, both captures show only
the original tap judgment. In the paired 600-frame timed runs, 95th-percentile
frame time moved from 8.64 to 8.61 ms in OpenITG and from 8.67 to 8.62 ms in
Simply Love. All four Vulkan and WASAPI runs recorded zero audio underruns and
zero buffer growth events.

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
