# Architectural layers

DeadSync separates reusable engine functionality, game behavior, and replaceable
presentation. A theme may change how game information is displayed; judgments,
scoring, failure, input semantics, and chart behavior belong to the game.

| Layer | Crates | Responsibilities |
| --- | --- | --- |
| Engine | `deadlib-*` | Generic rendering, audio, physical input, assets, Lua, and platform services. No dependency on DeadSync or its themes. |
| Game | `deadsync-*`, excluding themes and the application shell | Timing, judgments, scoring, player/session state, logical input, receptors, arrows, notefields, noteskins, song Lua, profiles, and game services. No dependency on theme crates. |
| Themes | `deadsync-theme`, `deadsync-theme-*` | Screen presentation, layout, fonts, colors, menus, evaluation, and HUD appearance. Consume game data and translate user actions into requests. |

Production dependencies point downward. Themes can use game and engine crates.
Game crates can use engine crates. The `deadsync` executable selects the
installed theme in `src/theme_registry.rs` and passes its presentation to
`deadsync-shell`. Neither crate is a dependency of engine, game, or theme crates.

The mount supplies title, stage, selection, System Options, Player Options,
attract, and chat presentations and their supplemental assets. The shell
passes game state, elapsed time, input intent, and ready media through
contracts in `deadsync-theme`. Themes own the layout, animation, resource
choices, and presentation state. OpenITG is a production dependency of
the executable. Shell integration tests exercise the same
registry as the executable.

The transition mount also supplies screen opacity and optional fixed entry/exit
effects. Effects are prepared with the theme's resource manifest and played once
at the transition boundary. Theme actors own their fades and overlays; the shell
does not choose transition artwork or timing for an installed native theme.

The title mount adapts each theme once at startup. OpenITG uses the shared
focus and input handling for the list of menu actions.
Each theme owns its presentation clock and receives the same prepared
credit and session facts. The shell admits Start and records the physical
side before the title requests profile selection.
The shell supplies the installed version to the title menu and updater views.
The version watermark receives the compiled version and build hash.

Attract presentations own their scene sequence and media choices. A demo
request specifies its chart filter, player count, HUD visibility, and whether
to play music. The shell prepares an isolated gameplay session; silent demos
use a frame-driven clock and discard audio commands. Cabinet ranking views
carry real chart identities and leaderboard rows prepared by the profile/game
adapter. No theme scripts or external theme directories are loaded by this flow.

The shell uses shared `Screen` roles for stage launch, restart, Practice,
profile selection, and Evaluation return. `deadsync-core::session_flow`
resolves optional session steps using machine settings and those roles.
The title presentation emits a destination after the session records the
side that pressed Start. Theme screen IDs remain available to presentation.
Song Lua's legacy screen names resolve to the same roles, and normal stage
completion and reload still go through the game-owned lifecycle.

Evaluation input emits a departure intent. The game selects the song or
course return from the recorded origin. The mounted overview can supply
its departure cue, readiness time, duration, and overlay through
`StagePresentation`. The shell advances its existing transition clock.
The result view borrows that elapsed time for animation while the visit
clock holds its original value.

The shell prepares one immutable score snapshot for each song or course
result. The mounted Evaluation overview and the Simply Love panes borrow
those records. Course paging shares the same records, including the timing
and life histories. The score crate resolves the displayed Routine grade
from the combined percentage and the participants' failure states.
`deadsync-profile-gameplay` resolves the shared scorebox pane settings for
Select Music, Evaluation, and Simply Love gameplay. Simply Love applies them
when mounting its gameplay HUD.
`deadsync-score` supplies the replay request payload. The profile gameplay
adapter prepares the recorded edges and beat-zero offset for stage playback.
The shell prepares the visible song or course banner through the same cache
when a result page opens. The mounted stage chooses its results music and
which cues the fallback panes may play during the visit.

All Player Options layouts consume the same editor and submit
`deadsync-profile-gameplay::stage_options::StageOptions`. The mounted layout
supplies its viewport and transitions. The game resolves stage scroll speed,
tournament restrictions, and Golf settings on owned profile copies.
Content reload and bulk ReplayGain workers publish progress through
`deadsync-assets::progress`. Simply Love resolves its visual style, screen
bars, background movie, wheel fallback banners, and density mesh in its theme
crate.

The chart catalog also retains duration-independent radar numerators in its
metadata cache. The simfile loader computes them from chart rows and hold
endpoints; `ChartData::radar_values` normalizes them by the song's music length.
Themes receive that duration with the selected song and own the polygon drawing.

Wheel views also forward the resolved chart/song display-BPM range and plain
session layout facts, including the primary side, without exposing runtime
profile objects. Presentations choose their own numeric font, BPM animation,
and style artwork even when no chart is focused.

Setup, retained evaluation pages and session summaries carry those same plain
style facts. The shared result view uses its layout flag for double-width
presentation; each theme owns any style badge and header animation.

The score layer computes chart achievement ratios from judged notes and recorded
combo runs. Song results retain these optional ratios; course results retain the
number of passed stages. Evaluation views forward these game facts, while each
theme decides whether to draw bars, survived counters, or other result artwork.

Gameplay HUDs also declare whether life warnings follow individual playfields or
require every joined player to be low. The common composer applies that scope to
the live player facts and warning preferences, then invokes the mounted artwork.
The scope affects presentation only, with no changes to life or failure rules.

HUD presentations may replace the shared player-identity footer using resolved
profile labels indexed by physical side. Joined, guest and hidden-name rules
remain in the game. Artwork depths remain theme-owned through the existing
actor controls; notefield geometry, noteskin selection and scoring remain
game-owned.

The title-menu runtime view includes whole credits, remaining coins toward the
next credit, and coins per credit. The shell prepares these from its coin
balance and cabinet settings. Themes choose their credit text, fonts and
positions without reading the coin ledger or configuration globals.

Session-ending presentations receive a prepared session summary, banner keys,
record facts and a game-owned timed name editor. The score layer aggregates
recent passed stages and resolves timed character selections; the shell maps
physical input, forwards profile updates and handles navigation. The mounted
presentation supplies its clock policy and draws summary, name entry and Game
Over without loading an external theme directory.

Simply Love resolves its Step Statistics and upper NPS graph dimensions,
scroll window, and scale from the prepared stage. The stage allocates the
optional graph's life sample buffers before play and samples the same game
life and timing data. The shell forwards the saved step artist preference
to Select Music for the layout to interpret. Twitch connection state stays
with the shell service. The mounted overlay reads a borrowed chat snapshot
and owns placement, text, and its actor cache.

Shell services accept runtime requests and publish events through
`deadsync-theme::app_requests`. The song search worker uses the game catalog
and ranking in `deadsync-simfile`. Each theme owns any search overlay and
passes the current chart type in its request.

The shell still depends on the default theme for the Player Options editor,
parts of Select Music, System Options, and Evaluation. The executable mounts
gameplay presenters through the shell. `deadsync-assets` shares the asset-backed
background drawing, profile-selected field feedback, and text prewarming.
OpenITG and StepMania 3.95 each mount the shared gameplay presenter with their
own HUD artwork, labels, and layouts. Practice uses the Simply Love gameplay
adapter with the existing editor. All presenters use the same prepared stage.
For song intros, the shell resolves the optional pack event name with the
simfile catalog. StepMania 3.95 and Simply Love choose their labels from that
stage fact. OpenITG supplies its own label.

## Shared contracts

Shared data belongs below its consumers, so game code does not need to import a
theme to describe an input or store a setting:

- `deadsync-core::hud_style` defines plain HUD appearance inputs. Themes supply
  their values; canonical receptor geometry and note ordering stay in the game.
- `deadsync-core::color` defines saved color-setting names, judgment roles, and
  palette data. Concrete palette values remain supplied by themes.
  The shell uses the installed theme's judgment preset for Player Options,
  gameplay, Evaluation, post-song screens, and palette editing.
- `deadlib-render-core::color` defines the shared color value and hex format.
  Config persists overscan settings, and the shell applies them to
  `deadlib-present` on startup and after a confirmed adjustment.
- `deadlib-assets::AssetManifest` describes fonts, textures, and texture sampling
  requirements. The game resolves its catalog and paths before engine loading.
- `deadlib-assets` owns the font catalog, texture identities, and upload queue.
  On the render thread, `deadlib-render` supplies the texture operations defined
  in `deadlib-render-core`. Game asset helpers use the same interface. The
  shell owns the concrete texture store. Presentation borrows fonts and texture
  bindings through `AssetView`.
- Video decoding and texture uploads share the packed YUV420 image and color
  conversion defined by `deadlib-render-core`. The shell hands each image and
  its buffer recycler to the asset queue. The game supplies artwork and video
  poster decoding to the image cache.
- `deadsync-theme` reexports the original public style/color names and
  `ThemeAssetManifest` alias for caller compatibility. These are the same types,
  with no wrapper allocation or conversion.
- `deadsync-input` owns the logical device namespace, including persisted RE:Flex
  IDs. `deadlib-platform::input` owns physical events and native transport limits.
- `deadlib-platform::input` also owns the Windows backend preference, stable
  device UUID generation, and the bounded UUID-to-slot table. Config reads and
  writes the pad-order INI keys, and the shell provides slot lookup to native
  drivers through their host callback.
- `deadsync-input::devices` owns the StepManiaX and RE:Flex provider identities.
  The shell tags OS and StepManiaX connection events with the game's provider
  type and moves their existing payload. Native backends use the shell's live
  StepManiaX ownership check when selecting devices.
- `deadsync-core::input` owns the saved StepManiaX sensitivity preset
  used by config, profiles, and presentation. The shell passes the selected
  preset and enabled underglow colors to `deadsync-smx`, which owns threshold
  values, LED color conversion, wire order, and device commands.
- `deadsync-core::lights` owns the saved cabinet driver, gameplay pad-light
  mode, PacDrive ordering, and serial port types. Config reads and writes
  these preferences. The shell supplies them to `deadsync-lights` for driver
  startup, device I/O, and gameplay light updates. It translates virtual actions
  into cabinet pad and menu light events and routes the operator button.
- The shell selects and loads cabinet charts and prepares their timestamped
  lighting events. The cabinet driver schedules those events and receives
  note timestamps and eligible columns from the shell for timed pad blinks.
  The shell supplies StepManiaX lane input, translates judgments into panel
  effects, and resolves result grades to GIF suffixes. The SMX driver tracks
  lane events and drives panel animations.
- Config and profiles read INI files with `deadlib-platform::ini`.
  `deadsync-core::session_flow` defines the game, preferred play style, and
  preferred play mode shared by both crates. The shell supplies machine defaults
  and the callback for saving default profile IDs. Profiles resolve session
  styles and modes, manage profile files, and apply confirmed selections.
- Config and chart metadata share `deadsync-core::timing::SyncPref`. Config
  owns the saved machine default. Charts resolve inherited preferences and
  the corresponding timing offset.
- The shell installs and migrates the Cel and Metal Workshop. It uses
  `deadsync-updater` for checksum-verified downloads and `deadsync-noteskin`
  for pack compilation and validation.
- The shell supplies DeadSync's Vulkan instance names through
  `deadlib-render::BackendOptions` at startup and renderer recreation.
  The engine provides generic defaults and labels its own GPU resources.
  The executable supplies the log path from `AppDirs` to
  `deadlib-platform::logging`, which handles rotation at that path.
  The executable also supplies the logger's file context to audio codec probes.
- `deadsync-config` validates and saves audio preferences. The executable
  applies the loaded mixer levels, ReplayGain, and pitch preservation before
  starting `deadsync-audio-stream`. The shell applies live requests, including
  forwarding pitch preservation to the current stream through `AudioControl`.
  The executable initializes the ReplayGain cache and supplies the gain lookup.
  Playback associates the initial and late gains with the active track.
  Config and playback share the mixer volume percentages defined in
  `deadsync-core`.
- `deadlib-audio-core` defines the saved Linux backend and output mode choices.
  `deadlib-audio` owns backend discovery, startup, and fallback.
- Song and course scans receive additional song roots from the shell. Scan,
  reload, and stage callers supply the cache preferences, pack exclusions,
  and timing offsets needed for each operation. The shell supplies the music
  duration probe using the same audio timeline as playback. `deadsync-simfile`
  owns the catalog, parsing, cache refresh, and sync analysis load policy.
- `deadsync-online` owns DeadSync's HTTP User-Agent and shared connection
  pools. `deadsync-net` builds clients with caller-supplied settings and
  handles HTTP errors, body size limits, and response decoding.
- Song Lua owns media path lookup and actor metadata caching. The asset host
  supplies sound duration and movie dimension probes to each song VM.
  `deadsync-input` translates physical events into legacy device and button
  names for accepted Song Lua callbacks.

## Gameplay policy and presentation

`deadsync-gameplay` owns target-score reachability and each player's one-shot
fail/restart decision. The HUD uses the same current-score facts and reachability
predicate, retaining its conditional calculations and single EX snapshot. Profile
adapters select the persisted ITG, EX, or Hard EX policy; themes forward the
resulting `GameplayAction` as a semantic effect for game-owned routing.

Frame ordering is part of compatibility. Target policy runs after song-Lua
updates and before frame input and Draw. Existing completion/cancellation takes
priority. Lua outro suppression still tests the original core action, so a
target-triggered failure retains its immediate exit. Autoplay invalidation and
natural-completion mine bookkeeping also live in the game and run at their
existing stage-exit boundaries.

`deadsync-gameplay::exit_route` owns course continuation, completion destinations,
course cancellation, and consumption of a pending restart. It accepts the
existing `GameplayAction` plus live session facts and returns a semantic
destination with the original normal or immediate transition mode. Only normal
completion can advance a course; immediate completion always requests results.
An immediate cancel from gameplay consumes a pending restart before considering
course cancellation. A direct Restart action still begins the existing exit
protocol before its eventual cancel returns to the stage.

Normal gameplay effects carry the original game action to the shell instead of
first becoming a concrete screen request. The shell calls the game policy for
each effect when it executes, after preceding effects have changed session state.
It reads failure state only for normal completion from gameplay, and consumes a
pending restart before offset-prompt or transition-busy early returns. The shared
screen contract maps the destination to a screen role. Explicit song-Lua navigation to
Evaluation or Select Music uses the same policy through a compatibility adapter;
other named destinations and validation before End retain their existing paths.
No-op actions still become empty effects, and existing effect buffers are reused.

`deadsync-gameplay::course_progress::CourseProgress` owns the course cursor,
interruption state, availability and completion rules, entry/reload advancement,
and Endless renewal decisions. Queries receive the current prepared stage count
and Endless flag, so appended cycles and changes to the course mode are observed
immediately. The value contains no stage metadata, result history, or theme types.

The canonical cursor advances only after successful gameplay entry. A song-Lua
replacement borrows the existing stages with a saturating cursor rewind. The
view preserves canonical progress and leaves the outgoing stage unrecorded. The
game also selects the next-stage lead-in. Endless lookup retains modulo
availability, while entry requires a new cycle at the prepared boundary and
rejects skipped, unprepared history. Loading interruption recovers recorded
course results without marking players failed.

`deadsync-rules::course::CourseType` is the shared course mode. The simfile parser
classifies its file data into that game type, and theme views carry the same value.
The parser's `CourseType` and theme's `CourseTypeView` public paths remain
reexports, with the same variants and Nonstop default. Course selection passes
the value directly instead of converting between duplicate enums.

`deadsync-simfile::course_selection` owns course catalog lookup, play/grade facts,
difficulty availability and defaults, ordered stage preparation, numeric totals,
score identity, and seeded Endless rerolls. `deadsync-chart::course::CourseStagePlan`
is the canonical resolved stage payload. The theme supplies one seed per course,
shared across its ratings, and formats each selected chart through an inline
immutable callback.

Preparation preserves authored entry indices for random selection, skips missing
entries, and records only accepted songs in repeat history. History starts fresh
for each rating and Endless cycle. Explicit nonnegative meters retain empty
ratings; implicit Medium remains available. Catalog duplicate precedence,
saturating play counts and totals, duration rounding, raw authored gains, and
path-based score hashes retain their existing behavior. Endless rerolls prepare
only the new stage sequence and reuse the initial rating's presentation metadata.
Themes keep localized track-list text, wheel grouping, labels, and layout.

Course startup accepts `deadsync-chart::course::CourseSessionPlan`: path, score
identity, canonical difficulty and meter, mode, life settings, and resolved stages.
The shell retains the separate `CoursePresentationView` for the course name,
banner, options label, and summary song template. The original song template is
preserved because evaluation also uses its artist, BPM, and other metadata.
Presentation values do not participate in runtime stage loading or life policy.

On confirmation, an Endless run retains its resolver, authored course, rating,
and original stage order. The game rerolls from that source and falls back to
the confirmed order if it resolves an empty cycle. Renewal supplies
`CourseCyclePlan` to the active run, which checks the exact path and append
authorization. The course keeps its rules and score identity when the theme
rebuilds the wheel. Readiness checks borrow the selected rating, and the legacy
selection adapters still support their original callers.

`deadsync-gameplay::course_run::CourseRun` resolves the selected stage plans into
playable stages and computes their per-player totals. Startup and Endless
renewal share the same preparation path. The shell supplies the chart loader
and reports errors. Partner styles load each distinct chart payload once per
stage, and a failed renewal rolls back appended stages and preserves totals.

`deadsync-profile-gameplay::results` assembles the course summary and selects
which sides save a score. Finite courses use the prepared trail totals and
Endless uses the recorded stage totals. Routine saves the two participants'
shared result for the master side. The shell retains the recorded stages,
supplies display metadata for the summary, and adapts the output for evaluation.

`deadsync-gameplay::course_life_config` owns course life-model selection and Oni
rewards alongside life initialization, damage, and carry. Its `CourseLifeStage`
borrows song metadata and copies the selected chart indices and authored gains.
Missing stages use Bar; explicit nonnegative Oni rewards, including zero, skip
chart lookup. Automatic rewards retain each player's difficulty/edit-slot lookup
and the meter-eight boundary. Survival gains pass through unchanged so existing
initialization applies its minimum and nonfinite handling. The shell retains its
life-config wrapper and calls it with the existing reload-adjusted entry view.

`deadsync-assets::gameplay_noteskin` owns gameplay noteskin preparation: chart
overrides, profile-part migration, bundled-default fallback, per-part composition,
and receptor startup beats. Its `GameplayNoteskinAssets` keeps the existing shared
asset references and prepares the scalar `GameplayNoteskinData` consumed by the
simulation. Receptor glow/step behavior and tap/mine explosion lifetimes therefore
come from the same game-owned adapter for every theme. Hidden tap explosions,
active-player limits, and copy-on-write changes to cached skins retain their
existing behavior. Preparation still runs once during song loading; themes retain
their render caches and HUD appearance.

`deadsync-assets::gameplay_stage` prepares the simulation for both Gameplay and
Practice. It resolves the game profile's noteskin feedback, pack sync preference,
canonical crossover annotations, Lua lead-in and lifecycle, and the existing
score targets before the shell constructs the screen. Course submissions use the
title from the active run. The theme formats the course title and displays its
banner. The shell installs live Lua callbacks
after the theme builds its initial caches, then restores held inputs for a
replacement stage. Render scratch warms at the same callback-install boundary.
Simply Love's public combined constructor delegates to the shared preparation
and presentation paths for existing callers.

`GameplayStage` owns the song media schedules and the reusable compositor
storage for Gameplay and Practice. Game-stage construction prepares those
buffers before presentation. The shell samples foreground ownership and video
clocks at their existing media boundaries, and plays scheduled Lua sounds
after draining gameplay audio. Themes supply HUD and field presentation
separately.

`deadsync-theme::views::GameplayHud` receives read-only stage facts for the
standard life meter, percentage score, song meter, difficulty, and stage
badge. The shared layer order keeps those actors reachable through the
existing song Lua hooks. The presenter may also supply fallback background
art for songs without media.

`deadsync-song-lua::gameplay_presentation` drives frame composition through
`GameplayFramePresenter`. The presenter supplies screen layers, notefield
actors, and any final overlays. The game builds frame options from the live
session and saved player settings. Gameplay and the exporter include chart
attacks and song-authored visuals. Practice includes them during playback
and suppresses them during editing, using the live Practice session. The
presenter receives that decision with each notefield draw request and supplies
layout and HUD options. Both modes share capture order and restore the stage and
theme buffers before final overlays.

`deadsync-notefield` resolves per-player motion from the live stage and borrows
the chart's timing and lane caches. It selects the cached reverse directions
and live attack effects for gameplay. Practice editing uses base options and
its explicit scroll override. The compositor sends these resolved inputs,
the borrowed Lua columns and note visibility, and the current authored judgment
texture to the field presenter. Both native and Lua composition use the same
preparation and resolve the notefield perspective from the Player position.
Native playback takes the stage's prepared position, and scripted playback
supplies the evaluated Player state. The presenter supplies HUD assets and
layout. The system overlay draws outside song cameras and captures, while
the gameplay exit prompt participates in the song's Overlay.
Eligible Judgment proxies borrow the retained HUD draw range. Judgment proxies
inside render-target captures use the prepared actor source. Both paths capture
the tap sprite and its optional overlay. Lane hold feedback stays in the
whole-Player draw. The song Lua host loads the frame metadata when it binds
a judgment texture to a generated actor.
A tap judgment actor created without a texture exposes logical frame indices
until a chart loads a sprite sheet.

Game results count jumps from finalized rows on the playable chart and retain
combo intervals at the existing row and checkpoint scoring boundaries. Course
assembly adds planned jumps for unplayed finite-course songs and joins combo
runs carried across songs. Themes receive these results through the evaluation
view, alongside the existing song times and life graph.

Runtime Song Lua measures a foreground actor tree's remaining tween time after
`On` and converts it to an inclusive final song beat. Baked Song Lua records
the same window during stage preparation. Composition uses the window for the
foreground's actors, proxy requests, and render targets. Shared screen and
player tracks are evaluated on their own timelines.

Stage preparation prewarms the notefield model cache against the selected
noteskins and sizes the hold mesh and field capture buffers. The stage retains
these caches and the hold emitters across frames. `deadsync-notefield` samples
the stage for receptor feedback, cues, flashes, and explosions in draw order.

`deadsync-gameplay::stage_charts` resolves chart fallback and assigns charts to
solo, Versus, and partner players. Preload, Player Options entry, and direct
stage entry read the wheel through one adapter. `deadsync-assets::song_lua`
builds the Lua context and translates the prepared scene's selections into
chart indices, cabinet-side joins, and a gameplay style. The chart adapter in
`deadsync-profile-gameplay` validates the selection and commits chart metadata,
payloads, and session changes together after loading succeeds.

The game synchronizes the selected charts when both players share one chart or
choose Versus charts with different music. The wheel sends its current and prior
chart slots to this policy, and Player Options sends the edited side and slot.
The theme refreshes both chart cursors and any BPM text affected by the change.
A Versus launch with conflicting picks and no prior slots uses P2's choice,
matching the precedence when both wheel slots change at once. Preload, direct
entry, and entry from Player Options compare the actual charts after applying
the same fallback as stage entry. On late join, the game keeps the
incumbent's selection and starts the new player on the first available chart
while keeping their saved difficulty preference.

`deadsync-profile-gameplay::stage_options` supplies the settings for stage
startup, including legacy noteskin migration and effective scroll speed.
Select Music preload reads the session and player profiles directly. Gameplay
and Practice accept the same settings when Simply Love submits Player Options.
Direct song entry, course stages, and restarts queue those settings in the
shell. Chart selection and difficulty fallback use the same game helpers for
both entry paths. If chart loading fails, the shell reopens Player Options
with the requested stage and settings.

Machine replay confirmation sends a `StageReplayRequest` to the shell. The
queued stage keeps the selected song, chart, and recording together during
the fade. `deadsync-profile-gameplay::stage_replay` converts the owned input
buffer and recorded beat-zero offset for the existing playback controller.
Simply Love draws the picker and formats the playback label.

`deadsync-gameplay::song_backgrounds` owns the authored background timeline.
Stage preparation expands random backgrounds using the final master chart,
including solo P2 and Lua chart replacements. Cue starts are converted once
with the stage timing map. The primary cursor keeps its forward-only behavior,
and layer-two lookup follows the display clock in either direction. The shell
loads textures and drives video timing. The theme applies tint, transitions,
and flash easing to the selected cues.

`deadsync-song-lua::gameplay_session` owns the live song scene, callback setup,
Update/input/Draw ordering, output buffering, deferred reloads, and exit clocks.
The shell calls its controller directly. The statically bound host adapter
supplies life-meter animation and converts semantic actions and screen names to
presentation effects. Game actions decide when End and Draw run, independently
of those effect values. The reserved `ScreenGameplay` destination reloads through
game code; other names are validated by the presentation adapter before End.
Normal gameplay setup also selects music fade/end timing in the game-owned
session constructor.

`deadsync-input::CallbackInputState` owns physical callback deduplication, held
input ages, repeats, synthetic releases, and held-input transfer during reload.
Repeat callbacks do not replay scoring. Their complete batch is collected before
dispatch, and its storage is reused across frames. Callback routing reads the
current game session, including Single/Double changes made by Lua reloads and
the active P2 side, rather than a possibly stale theme snapshot.

`deadsync-gameplay::practice::PracticePlayback` owns practice playback mode,
loop ranges, score/replay disabling, entry audio stop, audio-start snapping,
judgment reset, core advancement, and playing-input routing. Practice retains
its separate core loop: it does not run the normal song-Lua controller. Audio
starts one second before the requested beat and may be snapped by the audio
owner, while judgment reset uses the unsnapped requested beat. The game restarts
the range at the one-second tail boundary or on a core exit action. Static
preparation hooks clear editor navigation holds before restart/stop, preserving
input cleanup order; the theme applies cursor/display and flash updates afterward.

`deadsync-gameplay::practice::editor::PracticeEditor` owns the edit cursor, snap
resolution, selection, note seeking, playback-range selection, modifier and held
input state, navigation repeats, music-rate controls, and gameplay function-key
dispatch. Configuration is copied into the controller at entry; its canonical
state is private and exposed through read-only accessors and a restorable
snapshot. Both logical and raw input use the same controller, including the
existing precedence of modifiers, global hotkeys, playback, and menus.
`deadsync-input::practice` translates physical keys and their aliases into game
roles while preserving edge, repeat, and timestamp data. The editor consumes
that typed input without importing platform event types or services.

The theme applies semantic menu/zoom/playback requests and maps typed feedback to
localized flashes, sounds, and profile requests. It retains menu rows, display
easing, edit-field zoom, and render/text caches. Feedback runs inline with an
immutable view of the core, reusing the existing presentation queues without an
intermediate event buffer or chart/state clone. Each rate change preserves the
core mutation, profile request, audio command, flash, and sound ordering. The
theme refreshes its derived rate-text cache after that controller call, before
any rendering observes it. Rate repeats still run during playback and while a
menu is open; Control and Tab retain their existing navigation semantics.

`deadsync-online::gameplay_lobby` owns gameplay readiness, disconnect holds,
input routing, machine-stat publication, and one-time music start. The shell
calls its entry, update, input, and snapshot-refresh operations directly.
An entered lobby always waits until the next update, and the server snapshot
decides when playback begins. The update that starts playback publishes pre-start stats,
starts music, and publishes post-start stats in the original order. A completed
disconnect hold clears the local snapshot and starts offline in the same update.
The shell drains the queued audio command before sampling the stream clock.
Readiness and reconnect changes never re-pause a stage that has already started.
The entry-time refresh flag and active local sides remain fixed for the stage.
Themes retain lobby HUD visibility, translated status text, and countdown display.
The lobby session recognizes the dedicated menu-left/menu-right chord and tells
the shell which edges reach gameplay. A presenter receives the toggle separately.

`deadsync-online::lobby_wait` owns Evaluation and Select Music input barriers,
held-Start disconnects, and Evaluation machine-stat publication. Each screen has
one game-owned session with a retained lobby view and private hold state. Calls
supply the current joined sides and selected-side fallback, so a context refresh
changes policy immediately. Typed wait reasons preserve each screen's barrier
precedence without constructing presentation strings during input or frame checks.

Evaluation lazily requests scalar score facts when publication is allowed. The
game converts judgments and score percentages, publishes stats, and then checks
for a completed disconnect hold. It emits Disconnect before clearing its local
snapshot and reconnect state. Select Music retains its snapshot after requesting
disconnect, preserving the same-frame remote-selection and presence updates.
Themes adapt effects and score facts without moving theme view types into the
game or adding an intermediate event buffer or snapshot clone.

The themes retain status wording, countdown formatting, notices, overlays, and
navigation cleanup. Evaluation favorite-code handling still precedes the lobby
barrier. Select Music clears its existing navigation holds before sampling the
disconnect deadline. The game owns whether input is blocked; the displayed
status label does not participate in that decision.

`deadsync-online::lobby_selection` owns remote song resolution, confirmation
publication, Select Music presence, and synchronization retries. Its private
selection cache lives inside the existing `SelectMusicLobbySession`, which
retains the single lobby view. Successful and failed identities are independent:
changes to the wheel generation retry a failed lookup without reapplying a
successful one. Room changes clear both identities; disconnection in the same
room or a missing remote selection clears only failure. Reconstructing the screen
starts a new session.

Song resolution preserves grouped-wheel precedence, the catalog's ordered
full-path or pack/song matching, and its first song-directory fallback. Cached
identity uses case-sensitive normalized paths and exact chart/rate values.
Confirmation separately accepts optional remote metadata and its existing rate
tolerance, and builds a local payload only when publication is eligible. Presence
is emitted on each connected, joined synchronization; the online runtime still
deduplicates machine-state requests.

The theme focuses the resolved song before passing live local selection facts
back to the game. The game chooses the local-style chart, participant row,
preferred difficulty, accepted rate, and whether preview must stop. The theme
applies those decisions and resets wheel animations and image caches. A resolved
song remains successful even without a chart for the local style. Synchronization
keeps its existing frame position and profile/audio/online request buffers, with
no candidate list or additional retained snapshot. Stable cache hits reuse their
retained keys without allocating.

## Rust API migration

Workspace callers are updated for these source-level changes:

- Construct controller-only stages with `GameplayStage::new`. Use
  `GameplayStage::with_song_playback` to prepare the compositor and media
  schedules. Sound paths for prewarming are available in
  `stage.song_media.sound_paths`.

- Read authored background changes and active timing through
  `gameplay.song_backgrounds()`. The `GameplayInitView.background_changes`
  field still accepts an explicit timeline for compatibility constructors.

- Replay confirmation emits `SimplyLoveRuntimeRequest::StartReplay`, which
  replaces `select_music::take_pending_replay`. The `ReplayStartPayload`
  reexport retains its fields.

- Import `REFLEX_PAD_ID_BASE` and `PAD_ID_COUNT_CAP` from `deadsync-input`.
  Persisted device IDs are unchanged.
- Import `GamepadBackend` and `GamepadSystemEvent` from
  `deadsync_input::devices` for DeadSync's dedicated controllers.
  `deadlib_input_native::{PadBackend, GpSystemEvent}` still expose the native
  event contract. `BackendHost::device_claimed` exposes the caller's device
  claim predicate to native enumeration and hot-plug handlers.
- Exhaustive `GameplayAction` matches must handle the new `Restart` action.
- Normal gameplay effects now carry `SimplyLoveRuntimeRequest::GameplayAction`;
  exhaustive runtime-request matches must handle that variant. The exit-routing
  extraction adds no further `GameplayAction` variants. The public
  `resolve_effect_route` context, result, and function remain compatible adapters
  around the game policy. Raw semantic runtime effects have no `ThemeFlowEvent`
  until routing maps them to navigation.
- The shell's internal `CourseRunState` contains the game-owned `CourseRun`,
  display metadata, and recorded results. Call the run for completion,
  availability and renewal, and borrow `CourseRun::entry` for the reload-adjusted
  stage, timing, and life configuration.
- Course modes have one definition in `deadsync_rules::course::CourseType`.
  `deadsync_simfile::course::CourseType` and
  `deadsync_theme::views::CourseTypeView` remain enum reexports, preserving
  existing qualified and glob variant imports.
- Use `deadsync_chart::course::CourseStagePlan` for resolved course stages.
  `deadsync_theme::views::CourseStageView` and Simply Love's `CourseStagePlan`
  retain their public names as aliases of that same type, with unchanged fields.
  `deadsync_simfile::course_selection` supplies the reusable preparation API;
  Select Course's `course_score_hash`, selection, and Endless reroll functions
  remain compatible adapters. `SelectedCourseView` retains its flat public
  fields and provides consuming `into_parts`/`from_parts` adapters. New startup
  callers use `CourseSessionPlan` with a separate `CoursePresentationView`;
  renewal uses `CourseCyclePlan` and readiness uses a borrowed query.
- Initialize gameplay through `init_gameplay_runtime`.
  `GameplayControlRuntimeState` now keeps one-shot target handling private and
  no longer supports struct-literal construction outside `deadsync-gameplay`.
- Use `deadsync_assets::gameplay_noteskin` for the asset bundle and preparation
  function. The Simply Love gameplay module reexports `GameplayNoteskinAssets`
  and `gameplay_noteskin_assets` under their existing public paths. The bundle's
  `gameplay_data` method is public so consumers can use the canonical effect
  conversion without depending on a theme.
- `State::song_lua` is now a `SongLuaGameplaySession`. Its scene accessors retain
  `as_ref`, `as_mut`, `is_some`, `is_none`, and `take`; direct `Option` pattern
  matches or assignments must use the session API. Existing theme update/input
  wrappers delegate to the canonical controller, and `SongLuaHeldInputs` remains
  an alias for the game-owned reload token.
- Lobby readiness, music-start, and hold fields now live inside the private
  `GameplayLobbySession` state. Use its read-only accessors and controller
  operations. Existing theme entry/update/refresh wrappers still delegate to
  those operations. `SimplyLoveLobbyRuntimeView` remains an alias for the
  game-owned `LobbyRuntimeView`, with the same fields and defaults.
- Evaluation and Select Music retain their screen entry/update/input signatures
  and runtime-view refresh wrappers. Their lobby view and disconnect holds now
  live in `EvaluationLobbySession` and `SelectMusicLobbySession`; direct state
  access uses the sessions' read-only runtime-view and countdown accessors.
- Select Music's room and remote-selection caches are private session state.
  `prepare_selection_sync` and `record_selection_attempt` coordinate presence
  and retries; `lobby_selection` supplies song resolution, publication, and typed
  chart/rate decisions. Existing playlist path and sync-label adapters retain
  their signatures and delegate to the game-owned formatters.
- Practice screen entry/update/input signatures and `MusicStartSnap` remain
  compatible. Its private playback mode is replaced by `PracticePlayback`, and
  its editor fields are owned by `PracticeEditor`. `PracticeRuntimeView` keeps
  its existing fields, and the opaque `EditSnapshot` still saves/restores the
  editor and the theme's scroll-speed selection through the existing functions.

## Enforcement

`tests/layer_dependencies.rs` checks Cargo metadata for every workspace member,
including newly added crates. It checks transitive workspace paths and uses
package names, so renamed dependencies do not hide upward edges. Optional,
target-specific, and build dependencies are included. Development dependencies
are excluded because tests may exercise multiple layers together. Third-party
dependency internals are not traversed.

The game asset and simfile crates, and the gameplay, online, and song-Lua
controller crates, additionally have a test-build guard: their direct development
dependencies and production closures must remain independent of themes and the
application shell. This keeps their behavior tests usable by any theme.

`tests/architecture_boundaries.rs` additionally checks source ownership and the
specific APIs that protect canonical gameplay and notefield composition. Noteskin
guards preserve the public compatibility reexports while rejecting theme-owned
preparation and effect conversion. Game asset tests cover override/fallback
precedence, component migration, cached-skin isolation, and feedback timing;
shell gameplay regressions use that same production preparation path. Practice
guards keep editor state, input/repeat decisions, and rate audio commands in the
game. Game-owned editor tests exercise the real gameplay core, including held
input ordering, selection restoration, seeking, playback ranges, rate feedback,
and gameplay function keys, without requiring a theme dependency.
Lobby guards keep wait decisions, disconnect holds, Evaluation stat conversion,
and remote-selection policy in `deadsync-online`. Its controller tests exercise
barrier precedence, live local sides, exact hold deadlines, publication ordering,
the screens' distinct snapshot behavior, song matching, selection retries,
chart/rate decisions, and allocation-free stable cache hits without importing a
theme.
Exit-routing guards keep course/restart decisions in `deadsync-gameplay` and
require the shell to resolve each atomic effect before executing it. Game tests
cover semantic destinations and restart consumption, while theme tests cover
legacy navigation and mapping to concrete screens.
Course-progress guards enforce private game-owned state, live course facts, and
entry/renewal ordering in the shell. Scalar game tests cover finite and Endless
boundaries, interruption, cursor saturation, and reload views. Shell regressions
retain real chart loading, append rollback, and accumulated-result coverage.
Course-mode and life-selection guards require the canonical game type, public
compatibility paths, borrowed course entry, lazy chart lookup, and reload entry
ordering. Game regressions cover life-model selection, authored and automatic
rewards, actual chart-slot lookup, raw Survival gains, and initialization/carry.
Course-preparation guards require game-owned resolver and stage types, theme
delegation, and the public compatibility aliases. Simfile tests cover catalog
precedence, score facts, difficulty policy, skipped entries, numeric aggregation,
empty ratings, authored random indices, and fresh-history Endless rerolls without
depending on a theme. Existing shell tests retain real selection-to-runtime and
Endless transition coverage.
Startup guards require the game session/cycle contracts and separate retained
presentation, while rejecting legacy combined views in shell loading paths.
Regression tests check lossless storage transfer, presentation-independent
runtime results, early cycle rejection without loading, and public selection and
renewal behavior. The game verifies that Lua replacements borrow their original
stage and preserve canonical progress. Shell integration tests use real parsed
partner charts and verify rollback and course score ownership.

```sh
cargo test -p deadsync --test layer_dependencies --test architecture_boundaries
cargo test -p deadsync-assets --lib gameplay_noteskin
cargo test -p deadsync-gameplay --lib
cargo test -p deadsync-gameplay --lib exit_route
cargo test -p deadsync-gameplay --lib course_progress
cargo test -p deadsync-gameplay --lib course_run
cargo test -p deadsync-gameplay --lib course_life_config_tests
cargo test -p deadsync-simfile --lib course::tests::course_type_follows_itg_precedence
cargo test -p deadsync-simfile --lib course_selection
cargo test -p deadsync-theme-simply-love --lib screens::select_course
cargo test -p deadsync-shell --lib -- course::partner_tests app::course_transition_tests --test-threads=1
cargo test -p deadsync-online --lib gameplay_lobby
cargo test -p deadsync-online --lib lobby_wait
cargo test -p deadsync-online --lib lobby_selection
cargo test -p deadsync-input --test input_pipeline_allocations
cargo test -p deadsync-song-lua --features test-support --test gameplay_session
```

## Screen transitions

The shell runs the transition clock with the shared `deadsync_theme::Screen`
roles. The installed theme supplies its title and modifier-entry actors, fade
timing, and local screen identity. External consumers of `current_screen.txt`
use the shared compatibility names for both themes.

The shared roles drive stage transitions, course continuation, and retention
of the temporary Practice rate across a visit to Player Options. Concrete
themes supply their own `ThemeScreenId` values for presentation.

`ThemeTransitionAudio` supplies presentation music, preview continuity, and
screen sound cleanup. Simply Love resolves tracks when a cue starts. The shell
applies the theme's music plan in order with profile updates and retains control
of audio and media on Gameplay and Practice routes.

Gameplay and Practice use the full stage lifecycle for transitions. Their faded
routes use game-owned entry, restart, and exit durations. The shell evaluates
the gameplay tick condition before advancing the transition. Pending
loading still holds the clock and fade-in completion allows the next idle frame
to step the screen. Themes receive the transition phase and its elapsed time
to draw their own covers and HUD effects.

Normal gameplay sends stage actions, Lua reload requests, and lobby work
directly to the shell in frame order. The shell keeps course continuation and
stage restart on the gameplay lifecycle. The game interprets the default and
`ScreenEvaluationStage` as completion, `ScreenSelectMusic` as cancellation, and
`ScreenGameplay` as reload. The shell validates other supported legacy Lua
destinations against shared roles before the live scene ends.
Gameplay frames under a screen transition still dispatch lobby work in order.

The game-owned Practice session coordinates editor input, the edit seek clock,
chart playback, and loop restarts. The shell drives the session directly and
applies its rate changes after draining game audio commands. The session chooses
the destination on menu activation, and the shell maps it to a shared role.
A reload restores the game editor state and the theme's saved view.

`deadsync-profile-gameplay` resolves stage-local scroll options for preloading,
Practice, and Gameplay. The resolver applies the No CMod alternative and any
tournament MMod override before live Routine shares the master's effective
speed and Mini. Scoring and note scroll use the same effective setting, and
replay entry retains each participant's recorded speed and Mini.

## Remaining separation

Continue the layer separation within the existing workspace crates.

The production crate graph follows these directions. Gameplay and Practice
startup, normal song-Lua lifecycle, callback input sequencing, practice
playback/editor behavior, and gameplay lobby coordination now have game owners,
as do Evaluation and Select Music lobby input barriers, disconnect handling, and
remote song-selection synchronization. The shell advances the game-owned stage
and lobby controller directly. The gameplay runtime accepts presenters for
startup, reload, live updates, and rendering. The app selects the Simply Love
presenters and assembles their initial HUD preferences.
Course continuation and restart destinations, course progress, interruption,
Endless renewal, course modes, life selection, difficulty availability, runtime
stage preparation, and course score aggregation now have game owners.
`CourseRunState` keeps application-owned result history and display metadata
alongside the game run. The app keeps Evaluation pages and their cursor with
screen state. Course completion uses the stage results in the session. Evaluation
restart, reload, and Practice use the song, charts, rate, and scroll speeds
retained with the game result for the selected page. Submission refresh and
retry, leaderboard queries, and favorite actions use the recorded player sides
and charts for that page.

The shell records whether Evaluation came from a song or a course when it
enters the results screen. Themes issue a leave request. The game selects the
corresponding return role, including after the course has ended or the player
has moved between result pages. Simply Love uses the origin to choose its
audio feedback and retains the standalone screen API.

`deadsync-profile` decides whether a second player can join a one-player session
and which Versus style applies. The shell handles the credit and session update.
Simply Love controls where to accept join input and how to update its screens.

`deadsync-profile` defines the profile catalog and applies confirmed selections,
including the joined sides, active side, and play style. The shell prepares the
catalog and the joined sides for the selected theme's profile picker. The game
routes profile confirmation through the theme's available login and startup
screens.

The game resolves session startup and post-play routes from the optional screen
roles supplied by each theme. Each mounted theme also declares a session-screen
support mask, intersected with cabinet preferences without changing the saved
configuration. Requests from profile-login screens pass through the same routing
policy. The shell records the side that pressed Start at
the menu and uses it for profile selection and machine defaults.

The title menu mounts through the selected theme's presentation interface. The
shell prepares credit and service status, and the theme owns menu state, input
feedback, animation, and actor composition.

The shell prepares UI cues from the selected theme at startup. The OpenITG
manifest supplies embedded WAVs. The shared audio loader resamples them for
the output device and retains the same handles used by file-backed cues.
Shared controllers submit wheel movements by cue. Their existing named menu
effects resolve to the matching installed cue through the fallback manifest.
Other effects used on those screens and their supporting assets still come
from the current Simply Love manifest. The graphics loader reads embedded PNGs
from the asset manifest on startup and renderer recreation.

The `Theme` setting selects the installed presentation at startup. All title menus
use the same session start, cabinet permissions, and navigation commands.
OpenITG mounts its title, System Options, Profile Picker, Select Style, Select
Play Mode, Player Options, and base Select Music layouts. The existing Options
controller and shell still handle input, settings edits, and modal workflows.
An alternate page renderer lazily reads the controller's visible row order,
current values, and availability through `deadsync-theme`. OpenITG supplies its
service menu order through that contract. The controller uses the same tool
requests for direct and nested input launchers. The shell uses the originating
page to restore service-list focus when a direct tool closes. Options uses the
mounted theme's transition durations and passes elapsed time to its renderer.
The renderer owns the panel, heading, and focus motion. OpenITG's system and
player options share the native panel assets, title font, and three-piece
cursor drawing. Retained row positions are keyed by the controller's page
and row order. The system options renderer
also measures and caches displayed choice widths using the current font
chain. The shell passes the frame clock and resets presentation on Options
entry.
OpenITG retains the current explanation and its reveal timing. It uses the
engine bitmap text layout to measure wrapped paragraphs and reveal a glyph
prefix while keeping the full paragraph's placement. The engine reuses the
cached layout's geometry for atlas pages that are fully revealed and copies
the visible vertices of partially revealed pages into recycled frame buffers.
The Player Options mount supplies the viewport size, exit-row placement mode,
and row animation time to the shared editor. The editor exposes the joined
seats' positions in the selectable row order with the prepared rows. OpenITG
owns the panel, fonts, choice markers, and scrollbar artwork. It retargets each
thumb on the shell's frame clock.
The same mount lends the editor's prepared help lines to OpenITG. OpenITG
retains one paragraph and reveal clock per seat and uses the same measurement
and glyph reveal code as System Options.
OpenITG's basic and advanced Graphics pages share the same pending display
settings as the combined Simply Love editor. The shell adapts host monitor
capabilities to the theme contract, whose video mode records come from
`deadsync-core`. `deadsync-config::graphics`
builds resolution and refresh rate choices and seeds the FPS cap editor.
The controller keeps the row selections and emits one graphics request when
the user leaves Graphics. The shell applies it through the shared graphics
change plan.
The music wheel shares the game's catalog entries and circular index, and both
layouts use the same prepared difficulty window. The alternate base layout
borrows song, chart, player, banner, and preview beat data from the existing
controller. The OpenITG song and course wheels use the shared presentation
camera for projected cards and labels. The notefield uses the same perspective
calculation with its own camera parameters. The shell media manager retains
recently replaced banner textures for the transition lifetime requested by the
theme. OpenITG animates those layers and derives its exit motion from the
shared launch confirmation.

`deadsync-profile-gameplay::cabinet_records` resolves recency, catalog
identity, and machine bests for idle rankings. The shell requests a snapshot
when leaving the title screen and passes its score records to the theme.
Course ranking uses the same persisted course identities as course evaluation.
The theme controls the scroll geometry, page order, and music.

OpenITG requests two random-selection rows. The game-owned candidate pool
uses the active sort and style, and either action reaches the same chart
selection and stage launch path as a song row. The shell prepares one score
snapshot per physical side for both themes. The shared view checks the
current chart hash before exposing its local machine record or personal best.
The course browser uses the same wheel geometry with the prepared course
rating, track list, and records. On Profile Load, the shell installs the
worker's completed song or course state and hands its destination to the
loading screen. The loading view supplies the minimum presentation time for
both themes.
The shell continues to use that controller for input, previews, and launching
stages. At stage entry it can install alternate HUD drawing functions through
`deadsync-theme`. The gameplay composer supplies the already resolved life and
percentage score at the established HUD and Lua score capture sites. The shared
notefield accepts a theme-selected tap graphic, feedback metrics, and combo
label. It also accepts a stock hold atlas and its animation style. Gameplay
records the current hold result and alternating phase per lane. The canonical
renderer applies the style to the hold range. Judgment proxies capture the
tap range, and held-miss indicators use their own style. See
[the OpenITG progress record](openitg-theme.md) for the
reference, implemented behavior, and fidelity checks.

Select Style and Select Play Mode use their existing input and confirmation
controllers for both themes. The shell passes a borrowed `PlaySelectionView`
to the OpenITG carousel. The theme owns the carousel animation, projection,
and presentation assets. The controllers supply the available choices,
selected index, and joined players.

`deadsync-profile::picker` owns the per-side join and confirmation state,
including duplicate local-profile validation and late join. The standalone
picker and the Select Music overlay share that flow and the shell's profile
requests. OpenITG borrows a visible profile window and status through
`ProfilePickerScreenView` for both surfaces.

The evaluation visit receives completed stages before the shell chooses the
overview renderer. The overview view borrows prepared artwork and exposes the
canonical grade, scores, result counters, record ranks, and life graph. The
recorded maximum combo travels with each stage and aggregates by maximum on
course pages. Auxiliary panes and visit overlays remain with the existing
controller, and theme selection stays out of result routing and persistence.

The shell mounts one Player Options editor for song and course entry. Theme
selection chooses the layout and screen transitions. The alternate layout
borrows rows, focus, and help through `deadsync-theme` and draws the same live
samples at its own positions. Input, updates, search, profile requests, and
preview preparation use the existing editor in Simply Love. The shell supplies
device and asset views and manages preview uploads for both layouts.
`deadsync-profile-gameplay` defines the session and settings view, stage scroll
and rate rules, and the `StageOptions` submitted for gameplay, practice, and
recovery after a load failure.

The shell passes a shared `EvaluationStageView` and a separate presentation
context to the Evaluation mount. Stage and course entries use the same
per-player `EvaluationView`. A stage carries the completed song's round
number from the session or course cursor, captured before the result is
recorded. OpenITG reads it through the overview view. The shell tracks the
game result separately and uses it to resolve chart queries. The theme
reports which leaderboard panes need data and receives the available results,
heart-rate readings, and loaded result dialogs from the shell. The theme
selects entry audio and carries animation, audio, and noteskin previews
across course pages. The shell loads and plays the audio and tracks which
course result is active.

The mounted stage presentation supplies any Evaluation music loop. The
shared transition plan applies that loop on entry and keeps it across result
pages. The overview receives unweighted judgment counts, the dropped-hold
fact, and combo carry so themes can present awards from the completed result.

Video banners, screenshot labels, and StepManiaX result lighting use the
completed stage's song, chart, and grade. The shell captures the automatic
screenshot criteria before giving the result to the theme, and the theme
reports when it has finished showing animations and dialogs.

Several screens and runtime requests still use concrete Simply Love types.
The Evaluation mount still uses Simply Love's presentation context, runtime
views, and auxiliary pane controller.
Both title menus emit shared navigation effects and semantic UI cues directly.
Several screen controllers and their service requests still come from Simply
Love through `ActiveTheme`.
