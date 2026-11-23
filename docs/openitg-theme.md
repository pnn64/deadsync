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

- The title uses the native outlined Eurostile bitmap font and metrics from
  `default/Fonts/_eurostile outline`. The font and controller glyphs are
  bundled in `assets/fonts/openitg` and loaded through the standard font loader.
  The title's chrome frame, red streaks, and rotating flares come from the
  effective `ScreenTitleMenu underlay` and `ScreenWithMenuElements underlay`.
- The title and intro share the effective home theme's `ScreenLogo background`
  film, logo artwork, and timed image layers. The film stretches to the
  viewport at 70% opacity and loops at double speed. The title uses the
  embedded poster while the first frame loads or when video backgrounds are
  disabled. The chrome reveals from the left over one second, and focused
  text uses the reference's red pulse and 110% scale. The inherited title
  music redirect is silent.
- Home mode offers Play, Options, Find Content, Exit, and Shutdown when enabled.
  Free and Pay mode expose Play and use the same host credit rules as Simply
  Love. Both title implementations use the shared menu availability contract.
- DeadSync's Play route includes the existing mode step. The native direct
  mode choices, Records, and editor entries remain fidelity differences.

The shared menu frame carries the decoded texture. The shell uses the same
bounded background-decoder preparation and upload path for the OpenITG
title and Simply Love's video-enabled visual styles. The title keeps drawing
its poster while that work runs. A settled request reuses its texture key
without resolving the asset path or reopening the decoder each frame.

Vulkan captures at 640×480 and 854×480 cover the moving film, mid-entry
sweep, settled Play row, and focus on Options after a menu input. Compared
with the native 640×480 title capture, the arrow, wordmark, chrome, and
row pitch align. The fallback captures show the original controller glyphs
with movies disabled. Warmed title actor construction and composition
took 55 μs before this change and 53 μs afterward in the 854×480 harness.
The warmed frame used 25 draw operations with the extra logo layers, up
from 20. These timings exclude video decoding and GPU work.
The offscreen Windows runtime mounted the movie in 0.20–0.34 ms on the
render thread and uploaded the first frame after 0.19–0.23 seconds. It
rendered the movie across its 15-second loop and retired the decoder on
exit, including when a title reentry was canceled during preparation.

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

The service menu prioritizes the current equivalents of the home theme's
arcade, system, graphics, pack, update, input-test, key-mapping, and reload
actions. Other DeadSync settings follow in their existing order. Test Input
and Configure Keyboard/Pad Mappings open directly from the service menu and
return to the selected launcher. Settings search lists each action once.
Simply Love reaches the same tools through its Input submenu. The native
credit, diagnostics, and reset rows, pack workflows, and tool styling remain
open.

Graphics Options opens a basic page with display mode, aspect ratio, resolution,
and the Advanced Graphic Options link. The advanced page contains renderer,
refresh rate, presentation, and performance settings. Both pages edit a single
set of pending display settings. Back or Done from Advanced returns focus to
its launcher, and leaving the basic page applies the pending settings together.
Settings search can jump between the pages during an edit. Overscan returns to
its row in Advanced and preserves pending display changes. Simply Love presents
the combined Graphics Options page.

Both layouts use the host's monitor capabilities and the display settings
rules in `deadsync-config`. The resolution list retains the current physical
size when it falls outside the configured aspect. Fullscreen refresh rate
choices come from the active monitor at the selected resolution. The FPS cap
editor seeds its value from the selected refresh rate or the monitor's modes
when the saved cap is off.
Before and after the extraction, both layouts produced identical graphics row
snapshots for 1280×720, 1920×1080, and 3840×780 with the same monitor fixture.
The six OpenITG Basic Graphics captures at 640×480 and 854×480 matched byte for
byte through the aspect-ratio choice animation.

Two paired runs alternated fullscreen resolutions through the normal Options
sync path. Rebuilding the resolution and refresh lists took 4.4–4.7 μs per
edit across layouts, compared with 4.5–4.8 μs before extraction. At 854×480,
steady OpenITG Basic Graphics actor construction took 19.7 μs, compared with
19.6–19.9 μs. The Simply Love service menu took 30.8–31.3 μs, compared with
30.2–30.4 μs.

The route follows `ScreenGraphicOptionsBasic` in the home theme's
`metrics.ini`. Its display mode, aspect ratio, resolution, and Advanced link
use the reference's y positions of 127, 183, 211, and 267 at 640×480. The
Graphic Quality preset in `Scripts/SimpleGraphics.lua` controls legacy color
depths, texture limits, caches, and filtering. Its slot between display mode
and aspect ratio remains open.

The effective `ScreenOptions` reference places the heading at `(36, 38)`,
eight rows at y=127 with 28 px spacing, the scrollbar near `(34, 242)`, and
the Done row at y=353. Service Options and its settings pages share the
original frame, separator lines, Eurostile fonts, and red menu backdrop with
Player Options. The panel uses the native anchors at 640×480 and centers
the layout at 854×480. The inherited Frame commands slide it in with
0.3 s deceleration and out with 0.3 s acceleration. The service heading
bounces in over 0.3 s and widens, flattens, and fades over 0.2 s at exit,
following the `ScreenOptionsMenu header.xml` commands. The heading keeps its
own anchor while the rows move with the panel.

The OpenITG service page uses the 0.5 s navigation interval from
`_options to options`. Settings pages use the 0.3 s panel interval. The same
phase drives Options entry, exit, and submenu switches. Both layouts use the
same settings validation and persistence.

The OpenITG layout moves rows and the choice cursor over 150 ms and moves
the scrollbar thumb over 200 ms. A new input retargets each movement from
its displayed position. The selection band moves to the destination row at
once, and the scrollbar follows the selected row through the Done item.
The durations and retargeting follow `ScreenOptions.cpp`, `OptionRow.cpp`,
and `DualScrollBar.cpp`. The cursor follows the width of the focused label,
including Done. Inline choices start at x=160 and use the reference's 14 px
gap between measured text widths. A row falls back to its selected value
when the choices exceed the available width. The width of the native
three-piece gold cursor also tweens when focus moves. Enabled inline values
use the matching underline. The scrollbar uses a solid track and thumb.
Auxiliary modals and tools retain their current presentation.

The selected explanation starts at `(54, 381)` and reveals its wrapped text
in glyph order over 500 ms, following the default theme's `ScreenOptions`
metrics and `BitmapText.cpp`. Rows with identical explanation text on the
same page continue the current reveal. The panel extends downward for longer
localized DeadSync explanations and scales text uniformly when needed. A
secondary detail such as a path stays visible below the explanation.

Tests cover the viewport and Done row, prepared values, disabled and
multiselect rows, an edited volume appearing in the alternate renderer, and
the reload modal taking over that renderer. Graphics tests cover both return
paths, a single deferred apply, settings search, and pending display changes
surviving an Overscan round trip. Input tests cover direct and nested launch,
search results, and the shell's return selection. Vulkan captures at 640×480
and 854×480 show the service order, focused Test Input, remaining nested Input
rows, and Basic Graphics positions through the shared controller. The
reference positions and service order were checked against the home metrics.
Timed Vulkan captures at both widths show service entry at 75, 150, 225, and
300 ms, the local exit at 100 ms, and the following Basic Graphics entry at
150 ms at 640×480. Tests check that the service title keeps its anchor while
the panel moves and that the controller keeps the source page active for
the mounted duration. The Eurostile and frame pass checks a 150 ms entry
and the settled service and Graphics pages at both sizes. Placement follows
the effective theme sources. A comparison against a running native Service
Options screen remains open.

Focus-motion tests cover a second input during a viewport scroll, a horizontal
choice change, and reset on a new page. Vulkan captures at 640×480 and 854×480
show the same row, cursor, and thumb positions immediately before and after
the mid-scroll retarget. The highlight and help switch to the new selection
at that frame. Basic Graphics captures show the cursor moving between aspect
ratio values while the new value is active.

Paired Basic Graphics captures at 640×480 and 854×480 show the change
from 16:9 to 16:10. The gold cursor travels between the two labels and
widens with the measured text while the new value is active.
An actor test with the bundled font checks the gap between equal-length
labels with different glyph widths at both resolutions.

At 854×480, the original artwork pass increased median process CPU time
for actor construction from 20 to 23 µs for Service and from 20 to 27 µs
for Basic Graphics. The Simply Love service fixture measured 31 µs in both
builds. Settled Service CPU composition went from 31 to 39 µs and from
28 to 37 draw operations. These measures used dev-profile theme libraries
and reused composition buffers.

Vulkan captures at 640×480 and 854×480 show Test Input at 125, 250, and
500 ms after focus changes. Each additional glyph appears at its final
position. Configure Pads fits its full five-line explanation at both widths.

At 854×480, nine warm actor-construction runs of 12,000 frames per layout
measured medians of 32.6 μs for Simply Love and 19.1 μs for OpenITG on the
service menu, and 59.8 μs and 30.2 μs on Graphics Options. The fixture used
the bundled fonts and dev-profile theme libraries.

Two repetitions of the same warm actor-construction benchmark measured
19.4–19.5 μs for Basic and 27.1–27.5 μs for Advanced. The previous combined
OpenITG page took 28.9–29.1 μs. Simply Love's combined page measured
57.7–57.9 μs before the split and 57.9–59.8 μs with the new controller.

The service order and spacing changes measured 19.0 μs on the OpenITG
service menu and 18.6–18.9 μs on Basic Graphics. The same fixture previously
measured 18.5–18.9 μs and 18.5–18.6 μs, respectively. Simply Love's service menu
measured 31.2–31.8 μs, compared with 30.6–31.4 μs in the previous build.

With the heading motion mounted, warm steady service actor construction
measured 19.4–19.6 μs against 19.1–19.2 μs before the change, with 30 actors
in both cases. Warm composition at 854×480 measured 34.4 μs and 29 draw ops,
against 31.1 μs and 27 draw ops before the heading shadow. The sampled
animated frames measured 34.0–34.3 μs with the same 29 draw ops.

The focus tweens measured 19.7 μs for steady service actor construction
at 854×480, compared with 19.3–19.4 μs before this change. A stress case
retargeting the viewport every 75 ms measured 23.3 μs for actor construction.
Putting the cursor on the existing marker layer reduced steady composition
from 30 to 28 draw ops in the layer experiment. The final build measured
33.4 μs and 28 draw ops, compared with 33.8 μs and 29 draw ops before this change.

With explanation reveals, two 854×480 runs measured 19.5–19.6 μs for steady
service actor construction, compared with 19.0–19.8 μs before the change.
Retargeting every 75 ms measured 27.3–27.5 μs, compared with 23.1–23.4 μs,
including paragraph measurement when the selected help changes. Steady CPU
composition measured 32.8–33.2 μs, compared with 33.6–34.2 μs. At the halfway
point of a reveal, composition measured 93.9–97.1 μs while building the
visible glyphs in reusable scratch storage. All composition samples had
28 draw ops.

With measured choice widths, two paired 854×480 runs with aspect ratio
focused measured 19.4 μs for actor construction, compared with 19.1–19.4 μs
before the change. CPU composition measured 31.7–32.2 μs, compared with
30.6–31.4 μs, with 29 draw ops in every sample. Service actor construction
measured 20.4–20.6 μs, compared with 19.7–20.0 μs. Steady service composition
measured 34.1–36.3 μs, compared with 33.0–33.2 μs, with 28 draw ops.

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
on the 640×480 layout. OpenITG uses an eight-slot viewport and tweens row
movement over 150 ms. When players focus distant rows, each gets a window
around their selection. Exit appears at its separate anchor when the window
reaches the final row. The shared editor supplies the row order and navigation,
including the optional arcade bindings and full-height wrapped rows.

Player Options continues the red menu backdrop and streaks from Select Music.
It uses the source *Pick Modifiers* heading, Eurostile bitmap fonts, panel
and frame, seat emblems, amber row highlight, and each player's three-piece
choice cursor and underline. The nameplates identify the physical seats.
Inline choices fit the available width from the font's glyph advances.
Wider translated lists use the compact player columns.

The scrollbar centers at `(34, 242)` and travels within 240 px. The thumb
for each joined seat follows its position in the selectable row order over
200 ms, with Exit at the bottom. The two thumbs share the width when they
overlap. The marker artwork uses DeadSync's player colors. The reference's
16-row grouping and artwork remain open. Timing scaling and workout overrides
remain governed by DeadSync's rules.

Each explanation uses the reference anchor at y=381, with P1 left-aligned at
x=68 and P2 right-aligned at x=568. Each column has the equivalent of the
reference's 250 px displayed wrap width. The renderer keeps translated line
breaks and scales the full paragraph uniformly when it needs to fit.
The two 500 ms glyph reveals run independently and restart when the associated
explanation changes or the editor switches panes. Longer help text extends
the frame below the divider, keeping the main rows at their authored positions.

Tests cover P2-only perspective edits, profile requests, stage recovery and
resubmission, and noteskin preview requests through both layouts. Actor checks
cover separate versus focus and enabled choices, the arcade next-row affordance,
and row anchors at 640×480 and 854×480.

Viewport tests cover wrapped rows under both window sizes, reveal the separate
Exit when P2 reaches it, and check retargeting an in-flight thumb. Vulkan
captures at 640×480 and 854×480 show the overlapping thumbs on entry, the
thumbs and rows in motion at 75 ms, and the two settled row windows. At Exit,
P2's thumb reaches y=340 while P1 stays at y=122. A P2-only capture shows one
full-width green thumb.

The Visual Delay captures at 640×480 and 854×480 show the first 125, 250, and
500 ms after P2 changes focus. P1's explanation stays fully visible. The
four-line P2 paragraph also fits in the P2-only layout. The tests check both
anchors, paragraph bounds, and the reveal when another row has identical help.
System Options actor traces for Test Input and Configure Pads match at both
widths across the change to the shared explanation renderer.

This visual pass uses the supplied artwork and the effective metrics. Vulkan
captures at 640×480 and 854×480 check the new panel and cursor art for versus
and P2-only, at the settled opening and while scrolling to Visual Delay.
A settled native Player Options capture remains open.

With the source artwork and fonts at 854×480, median OpenITG actor build time
went from 27 to 45 µs in solo and from 35 to 59 µs in versus. The settled
versus fixture went from 39 to 70 draw operations and from 70 to 109 µs for
CPU composition. The measured actor and composition medians add up to
about 0.06 ms more CPU time per frame for the settled versus fixture.

At 854×480, nine warm runs of 8,000 actor builds with dev-profile theme libraries
measured Simply Love at 60.6 µs before the change and 60.2 µs afterward in solo,
and 78.7 µs and 79.3 µs in versus. The OpenITG layout measured 38.8 µs in solo
and 48.9 µs in versus. These timings cover settled main-pane actor construction
with bundled fonts.

Two paired runs of the eight-slot viewport at 854×480 measured 26.3–26.4 μs
for OpenITG solo actor construction and 33.6–33.9 μs in versus. The previous
viewport measured 39.0–39.1 μs and 47.9–48.9 μs, respectively. The Simply Love
control runs measured 61.0–61.4 μs in solo and 77.7–79.3 μs in versus across
the two builds. At the start of the versus fixture, CPU composition went from
80.9 μs and 53 draw ops to 70.8 μs and 39 draw ops. The 75 ms scroll sample
went from 108.3 μs and 60 draw ops to 92.6 μs and 53 draw ops.

Process CPU timing at 854×480 measured steady OpenITG actor construction at
27.3 μs in solo and 33.2 μs in versus with the new explanations, compared
with 25.4 μs and 33.2 μs. Settled Visual Delay composition measured 62.5 μs
in both builds. At 250 ms the new renderer built a partial glyph mesh for
P2 and measured 148.4 μs. Both samples used 39 draw ops.

The renderer now reuses cached glyph geometry across the reveal. In two
paired dev-profile runs at 854×480, warmed composition of a changing P2
reveal used 148–164 μs of process CPU time per frame before the change and
63–78 μs afterward. The settled samples measured 62.5 μs. All 12 Vulkan
Player Options captures matched byte for byte, including the 125, 250, and
500 ms versus frames and the solo P2 frames at both widths. Eight System
Options captures covering the reveal and Configure Pads also matched at
640×480 and 854×480.

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

Style and Play Mode use the supplied default theme's backdrop textures,
characters, choice artwork, side arrows, explanation frame, and Eurostile
normal and blue glow bitmap fonts. They share the title screen's red streaks
and outlined font. The effective `ScreenSelectStyle2` and `ScreenSelectPlayMode`
metrics supply the field of view, radius, depth spacing, dimming, and 0.3 s per
item. The perspective ring passes in front of and behind the character, with
the 640×480 composition centered in wider viewports.

The dance pad preview uses 25 poses rasterized from the supplied `_platform
home single` model and materials at its authored tilt. The focused choice
rocks ten degrees in each direction every second. Focus changes reset the
motion and return the previous choice to its neutral pose. Pump choices
show a five-panel layout. The supplied **Dance** logo represents DeadSync's
Regular mode. DeadSync's controllers still supply the choices, joined players,
and confirmation timing, including Marathon and conditional Premium Free.
Mode descriptions and non-English choice labels come from the shared view.

The native 640×480 Style capture was compared with DeadSync Vulkan captures.
DeadSync captures at 640×480 and 854×480 check both screens at entry, through a
carousel step, and on each settled choice. Play Mode was compared with the
source theme assets and metrics. Actor checks cover the native and wide
anchors, available modes, joined-player status, wraparound, and changing
selection mid-animation.

At 854×480, two runs of seven 6,000-frame batches measured median CPU time
for actor construction and composition during movement. Style went from
57 µs to 36–39 µs per frame and Play Mode went from 44–47 µs to 36 µs. The
fixture used dev-profile libraries, warm composition buffers, and fonts
staged under the packaged asset paths. GPU time was outside this measurement.

The pad animation was checked at both yaw extremes and at rest in Vulkan
captures at 640×480 and 854×480. The paired widescreen benchmark measured
39 μs per Style frame before and after the change, with 27 actors and 22
draw operations in both builds.

## Startup, demonstrations, and idle scores

The OpenITG startup presentation begins when the shared song scan completes.
The company clip runs for six seconds, the In The Groove 2 intro for
30.5 seconds, and the OpenITG logo for six seconds. The first two play their
original audio. The logo uses the home theme layers and plays the background
film at double speed. The attract screen owns these videos for the duration
of each phase. Start or Back leaves attract for the title menu. When video
backgrounds are disabled or unavailable, embedded still frames carry the
intro and logo overlays.

After 30 seconds of inactivity on the title menu, the theme plays up to
30 seconds of autoplay and 41 seconds of Beginner training. Both use real
song audio, charts, and the shared stage and notefield in an isolated
single-player session. A demonstration ends early when its chart completes
or is skipped when the catalog has no eligible chart. The training phase
uses the native instructions, traffic lights, and perspective-tilted dance
pads. Playable note crossings light the matching panels and show a direction
or Jump! for simultaneous steps. The three preview lights forecast whole-beat
crossings and their dots travel across the actual beat interval. The Left and
Up instructions animate a second pad under the caption. Both demonstrations
are unattended and leave profiles, scores, and credits alone.

Recent song scores appear next with the native ranking music. The shared cabinet query
resolves up to 40 recently played charts against the loaded catalog and
supplies the machine record for each chart. The song pages show five records
for ten seconds each. Empty song score sets show the empty state.

The Marathon and Survival pages follow song scores. Each uses the original
ranking frame, banner font, and 65-unit row spacing. Up to 15 courses appear
per page in recent-play order, with unplayed courses after recorded ones.
Ranking excludes autogenerated courses, matching the reference catalog.
Rows scroll through the five-row opening at 0.4 seconds per row. The ranking
music continues across all pages before the intro resumes. Existing course
records are keyed by course path and supply one cabinet best per course.
Remaining differences include ranking difficulty columns and Survival times,
promotional lists, native demonstration style rotation, course attacks,
and authored movie and Lua background cues during demonstrations.

An isolated Windows portable run entered all three movie phases with live
video, decoded the original company and intro tracks, and played both
demonstrations on the Quartz fixture before the ranking track. Both demos
finished at the end of the 24-second chart. First and second stage preparation
took about 362 ms and 90 ms in the debug build. The game logged 66 autoplay
judgments and all 20 save files matched by hash before and after. Tests cover
the timeline, intro overlays, idle music, a prepared record, and training
cues across a BPM change. Offscreen Vulkan captures at 640×480 and 854×480
checked the opening fade, moving light dots, a two-panel jump, and both
teaching captions against the archived actor definitions. The pad, panel
highlight, and dot textures match the local OpenITG files byte for byte.
At 854×480, warmed training actor construction and composition took about
12 µs before adding the pads and dots and 30 µs with both pads visible.
The measurement excludes game simulation and GPU work.
Vulkan captures at 640×480 and 854×480 used real local score files written
by the score store and showed a completed Marathon, a failed Survival run,
and an unplayed Marathon. In the three-course fixture, catalog and score
resolution took 2.4–4.3 ms on first use and 0.7–1.0 ms after warming. The
screenshot comparison uses the effective theme assets and source metrics.

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
the source's 15-row window moves. The supplied song, section, and action
textures wrap around the wheel, with the native gold highlight pulsing on the
preview beat. The shared red backdrop and header and footer streaks continue
from Style and Play Mode. The supplied curved silver frame uses a 58° camera
and 20° yaw. The left rim sits at the authored center and the card edges
trim under it as the wheel turns. The right frame uses its own centered
vanishing point and the authored sprite dimensions. Difficulty names,
cursors, and meters use the difficulty list's 20.5-unit row spacing, yaw,
and vertical tilt. The artist and BPM rail uses Eurostile. The footer
reports the available DeadSync Start action.

The native 640×480 Quartz capture confirms the tilted rows and the fixed
selection area. Vulkan captures of the DeadSync Quartz view checked the same
title, artist, BPM, and Hard 5 chart at 640×480 and 854×480. Additional
two-player captures check catalog labels, records, the rim during movement,
and all five projected difficulty rows over the rules. The original
fallback banner fills the curved opening in the native Quartz capture and
in DeadSync's 640×480 capture. DeadSync centers the 640×480 wheel at wider
sizes. The popularity indicator remains a fidelity gap.

In the 854×480 two-player fixture, the sum of the median actor construction
and CPU composition times went from 187 to 210 µs per settled frame and
from 194 to 224 µs during scrolling. The segmented meters, chart text, and
additional artwork bring the fixture from 132 to 166 actors and from 94
to 106 draw operations. These timings use dev-profile libraries and warmed
composition buffers.

With five difficulty rows at 854×480, the rim and rail alignment pass went
from 78 to 79 µs for settled actor construction and from 86 to 95 µs while
scrolling. CPU composition stayed within 176–180 µs across the two builds,
with 112 draw operations in each.

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

OpenITG's song and course banners use the authored 430×130 dimensions,
perspective yaw, and 0.6° roll. The original depth mask clips stills and
their outgoing layers around the curved opening. Video banners render with
YUV sampling and use bands traced from the same mask, with opacity sampled
per band during the wipe. Banners slide in with a 750 ms deceleration and
slide out with a 750 ms acceleration. The first accepted confirmation starts
the exit. The stage-launch controller handles the second Start and decides
when to enter Player Options or Gameplay. The effective patch's
`FadingBanner` commands drive a 500 ms corner wipe over the replacement.
DeadSync keeps up to four outgoing textures through that animation and
reuses them when revisiting a banner. Video banners retain the texture they
last displayed.

Vulkan captures at 640×480 and 854×480 verify corner order at 0, 125, 250, and
375 ms with two contrasting stills. The wide entry and exit captures keep
the side margins clear. Separate Vulkan captures use YUV textures to verify
that the new projection preserves video color during entry and fade. A
Vulkan media check verifies repeated
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

The aperture and alignment pass, measured with five difficulty rows at
854×480, moved median actor construction from 81 to 85 µs when settled and
100 to 104 µs while scrolling and fading. CPU composition moved from 175 to
186 µs settled and 177 to 183 µs mid-wipe. Both paths add one draw operation.
Vulkan through wgpu measured 2.59 to 2.69 ms settled and 2.64 to 2.68 ms
mid-wipe for submitting and draining batches of precomposed frames.

Each joined side's chart pane uses the supplied red frame and shows the chart
level, steps, jumps, hands, holds, rolls, and judgable mines. The machine
record and holder and the personal best use the same local ITG snapshots as
Simply Love. When the selected chart changes, the previous record disappears
until the matching snapshot is ready.
The effective patched `ScreenSelectMusic` enables panes while hiding the older
integer score fields. `PaneDisplay.cpp` supplies the six counts and two local
records. The source's pane entry and exit animation and separate song
popularity indicator remain open.

Vulkan captures at 640×480 and 854×480 checked the two-player music wheel
with the bundled Eurostile font and fixtures supplying both local records.
The configured play route checks the P2 pane against the chart's actual tap
count and checks that its font contains the displayed characters.
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
installed cue set. Sort and lock feedback, Evaluation entry audio, and
comparisons with the reference audio remain open.

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

OpenITG uses the native song meter artwork, difficulty tabs, stage badges,
life meter textures, and Futurist Metallic score font. The shell selects these
widgets at stage entry. They draw in the existing HUD, stage, and Lua score
proxy layers using the shared song progress, physical player side, life, and
score. The native fallback background uses the stage's background brightness
when the song has no backdrop or active background change. The playfield
continues to apply the configured profile background filter. An altered
music rate is shown under the song meter.

The effective `ScreenGameplay` metrics place the life frames 24 units in from
the outside edges at y=270, with a 272-unit bottom-up fill. The frame leaves a
16-unit inset above the painted area from the actual transparent texture
border. The stream scrolls with song beats and uses the hot texture at full
life. Single and versus use the screen quarter anchors for the score. Doubles
uses anchors 140 units from center. The font draws at y=56 with the native
20-unit inward offsets. The stage badge uses the original Event, numbered,
and Final Stage artwork. It holds at full size in the center for 1.2 seconds,
then shrinks to 25% and moves to 40 units above the bottom over 0.3 seconds.
Its result-transition exit accelerates 150 units downward over 0.8 seconds.

The gameplay underlay uses the original low-health lettering and glow sweep.
A live player below 30% life shows the warning on their physical side. Its
red backdrop pulses between 30% and 80% opacity over one second. Doubles
spans both sides, and a failed player's underlay dims in black. The animation
clock advances only during the warning. The shared presenter reads life,
failed state, and profile visibility settings from the same stage as Simply
Love. The threshold and animation affect presentation only. The artwork
stays in the existing Underlay capture used by song Lua.

Specialized score and life displays and native course-stage artwork remain
open.

The effective Gameplay entry backdrop resolves to `_short`, an empty
animation. Once stage loading finishes, the OpenITG playfield is visible
during the shared two-second entry. The life bars hold 100 units outside their
settled x positions and the score holds 100 units above its settled y for 0.5
seconds. Both slide into place over 0.8 seconds with quadratic deceleration.
During the results transition they accelerate off by the same distance over 0.8
seconds. Loading and restart use the shared covers. The shell applies song Lua's
intro suppression and supplies the stage clock that also governs input and audio.

Vulkan and WASAPI fixture captures show the native HUD artwork at 640×480
in regular and Event Mode and at 854×480 in single and versus. The regular
launch displays Round 1 and Event Mode uses its own badge. The wide versus
capture shows the mirrored difficulty tab and independent life and score
positions. The native comparison uses the same Quartz chart. Note graphics
follow the installed noteskin and the fixture's profile filter is visible.

Actor tests cover native and wide layouts, entry and exit positions, failed
life, and EX score tint. Offscreen Vulkan captures at 640×480 and 854×480
show the badge at center, in motion, and in the footer, plus P1 and P2
warnings and a failed doubles field. The configured play route checks P2's
opening badge, the warning at 25% life, and the profile Hide Danger setting.
A song Lua fixture captures the animated OpenITG score
with Underlay hidden and still draws the canonical player proxy. The existing
independent footer fade test fails at 0.75 seconds on the known baseline.

The theme tests check meter fill and marker position, life geometry, score
placement, and the round badge. The paired stage test feeds both themes the
same inputs and compares score, life, and completion throughout play.

Two alternating 600-frame solo runs at 854×480 held the chart, configuration,
and audio fixed. Mean frame time was 8.56 ms before the artwork and
8.61–8.62 ms after. Mean draw time moved from 1.80–1.87 ms to 1.91–1.92 ms.
Actor-build p95 was 237–280 μs before and 277–301 μs after. Render storage
recorded zero growth events in all four runs.

With the badge and low-health artwork installed, two alternating 600-frame
pairs of the same healthy 854×480 stage averaged 8.33–8.35 ms before and
8.33 ms after. Draw time stayed between 1.79 and 1.83 ms. All four runs
recorded zero audio underruns and zero render-storage growth events.

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
the graph area above the counts. DeadSync now slides the round label in
below the header from the left after 3.0 seconds and centers the modifiers
above each player's percentage. The label uses DeadSync's text font.
Native AutoPlay produced an empty combo graph. A DeadSync Vulkan fixture supplied
the same Quartz result to compare the counts, rank-free layout, and empty
graph at 640×480. Separate solo and versus captures at 640×480 and 854×480
showed filled runs, maximum-run labels, and record ranks.

The round and modifier placements were checked in 18 paired Vulkan captures
at 640×480 and 854×480. The captures covered Quartz, solo, and versus at 3.0,
3.15, and 3.4 seconds. Pixel differences were confined to the round and
modifier rows. Both themes' configured play routes passed with the shared
completed-stage view. The OpenITG route displayed the recorded first round
after a P2-only stage.

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

With the round label and new modifier positions, two paired CPU measurements
of settled Evaluation actor construction at 854×480 measured 27.3 µs before
and 29.3 µs after in solo OpenITG. Versus measured 50.8 µs before and
50.8–52.7 µs after. Simply Love measured 31.3–35.2 µs across the four runs
and both layouts. Each measurement used the median of nine 8,000-frame
dev-profile batches.

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

The Windows debug build verified portable startup with the bundled OpenITG
fonts. Cargo's flat and nested build output layouts both stage assets beside
the profile executable, including custom profile output paths.

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
