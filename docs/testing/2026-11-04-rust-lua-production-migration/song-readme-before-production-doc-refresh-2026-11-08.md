# Chart Lua ownership

`deadsync-song-lua` owns DeadSync's chart-Lua compatibility semantics. A theme
change must not change compilation, modifier windows, actor transforms, proxy
targets, capture ordering, or scheduled chart media.

- The compiler and `playback/prepare.rs` select song layers, preserve shared-session
  compilation and independent-layer fallback, and prepare runtime commands.
- `gameplay.rs` translates compiled commands into the deterministic windows
  consumed by `deadsync-gameplay`. The gameplay crate never depends on this
  presentation-aware crate and never builds actors.
- `playback.rs` owns actor execution, proxies, offscreen captures, media schedules,
  frame ordering, and song-lifetime scratch storage. It uses the existing
  `NoteskinSlot` contract, not the concrete asset loader or profile types.
- `deadsync-notefield` continues to own note rendering and player transform math.
- `deadsync-assets` supplies file/resource compilation. `deadsync-profile-gameplay`
  supplies profile/session compilation context. Neither supplies Lua semantics.
- `deadsync-shell` initiates preparation, coordinates media and warmup, and runs
  composition for gameplay and practice. Simply Love supplies presentation
  fragments through `frame_layers` and its existing field/HUD adapter. Shared
  playback decides which fragments a chart captures and how it transforms them.

The renderer reads gameplay state without advancing judgment or chart timing.
Buffers and caches retain their song lifetime and are prepared before live play.

Playback fixtures live in `tests/playback.rs`. They include the production source
so they can exercise private capture/cache paths with real asset slots without a
library-unit-test type-identity cycle through the asset crate. Full-frame gameplay
and practice regressions live with the shell orchestration. Native ITGmania actor
and Lua-semantic fixtures remain in the workspace tests.

The separately authorized [production migration](../../docs/testing/2026-11-04-rust-lua-production-migration/objective.txt)
is in progress. The standalone interpreter and C API removal milestones remain
closed. The opt-in `rust-engine` feature exposes `rust_engine::RustSongRuntime`
through the shared `deadlib-lua` Rust host API. It owns clock/SongPosition and RNG
bindings, file/environment helpers, actor declarations, and the current
Actor/ActorFrame/ActorFrameTexture/Quad/Sprite/Banner/Sound/BitmapText/GraphDisplay/SongMeterDisplay/CourseContentsList/Model/ActorMultiVertex/ActorProxy
and wrapper instance bindings.
`Def` creates declarations; `instantiate_actor` creates a scene whose command,
tween and message lifecycle is driven through the runtime. Resource bindings
include retained texture handles, Sound and Sprite preparation events, and mesh
uploads with a renderer-facing read API. Proxy bindings retain target identity
and expose the existing player/screen/actor target classification for downstream
overlay resolution. ActorFrameTexture bindings own allocation requests and
retained texture metadata. Custom draw callbacks and explicit `Draw` submissions
produce immutable draw frames with nested render targets, caller transforms,
mesh payloads and the existing capture limits. The host resolves captured actor
identities through the compiler/playback source map.
BitmapText bindings include text conversion, attributes, stroke and layout
settings, font resolution, and the existing theme helpers. `LoadFont` creates
reusable text templates; concatenation preserves command composition and
independent mutable attributes. Text resource reads return owned payloads.
Complete compiler text-change capture/publication remains part of the pending
song integration.
The existing RollingNumbers, BPMDisplay, DeviceList and InputList classes now
instantiate with Actor inheritance and private text/numeric bindings. Their
formatting, bound receiver behavior and fixed input-status resource labels are
covered by production comparisons. Additional own methods on unlinked classes
remain part of the binding migration.

Banner inherits Sprite and has Rust bindings for its existing media loaders,
scrolling controls and size methods. The shared Sprite media helpers retain
path-query ordering, the per-actor current-background Lua helper, and the existing
texture reset/publication distinctions. The bundled blank bitmap is prepared by
one Lua-independent routine shared with the production control.

GraphDisplay owns its two body children and line child, with the existing named
lookup and command traversal rules. `Load` and `Set` preserve metric/stat fields,
life-record callbacks and fallback samples. `actor_graph_display` returns owned
sample and child-style data; compiler publication remains pending.

SongMeterDisplay materializes its Stream declaration, preserves its width methods
and exposes owned stream render data. Its stream initializes after the meter,
runs startup commands and joins the cached frame traversal. Named children share
one helper with GraphDisplay. Generated groups remain rooted while a callback
invalidates the update plan and collects; plan invalidation and scene retirement
release those roots. Full meter publication remains part of compiler integration.

CourseContentsList retains its raw or existing Display table, scroller fields,
transform callbacks and per-player SetSong parameters. Shared displays retain
identity across lists; scene ownership keeps their callbacks and later wrappers
alive until the last owning scene retires. Plain display tables keep their authored
methods and metatables. Production manager and compiler integration remain pending.

Model construction preserves its default depth, culling and texture settings,
Actor inheritance and private sprite/child-query methods. Resource reads resolve
Meshes/Materials/Bones and return owned layers through the existing asset loader.
Preparation uses the existing Rust material clocks, preserving state selection,
hibernation, parent update rates and callback ordering. Material samples survive
capture retirement; scene ownership retains frame data until release. Declared
camera FOV and its command override now initialize the Rust render state. Full
Model publication and live rendering remain part of compiler/playback integration.

PREFSMAN and DISPLAY now use Rust host bindings. Preferences retain their existing
defaults, case-sensitive stored overrides, receiver behavior and side-effect
ordering. Display specs and modes preserve shared identity across calls. Logical
screen dimensions stay independent of physical display preferences; the aspect
helper reads the current public preference store. Traced method captures keep
saved queries and values alive without retaining discarded manager cycles. The
unchanged display-size fixture checks these bindings with framebuffer allocation
and actor rendering state in preparation and live contexts.

THEME exposes the existing paths, localized strings, metric definitions and
dynamic SL metric lookups through Rust bindings. Numeric metric getters keep
their definition-only behavior; lead-in and scoring queries read the current SL
tables with normal metamethods and traced intermediate values. Virtual theme
paths load Actor, Sprite or Sound declarations without filesystem resolution,
preserving the production host's initial resource state. Theme font paths use
the existing owned bitmap-font resources.

SL state, ThemePrefs and SL_CustomPrefs are initialized in the Rust song host.
Per-player modifiers, stage statistics, streams and mode tables preserve the
production defaults and separate table identities. ThemePrefs retains a private
store; InitAll preserves existing values and evaluates default getters with the
existing side-effect and error rules. ParseChartInfo retains its target streams
across callback reentry and collection. ParseColumnCues uses the existing Lua
helper unchanged. Theme preference option-row UI bindings remain pending.

The Rust song host now installs SONGMAN and SongUtil using retained song, chart,
course and trail objects. Chart selection uses immutable authored metadata and
traced chart identities even when scripts change the exposed arrays or getters.
Timing queries retain their owned music-rate state; density stays cached at
construction while column cues invoke the current Steps timing callbacks.
Collecting and reentrant callbacks preserve their intermediate objects. The
catalog retains selection slots and option groups for compiler/live integration.

SongOptions now has distinct Preferred, Stage, Song and Current objects. Setters,
text parsing and assignment, stage restart, attack clearing and the song phase of
current-option approach preserve the existing values, identities and write order.
Timing queries retain the same Current object as those updates. Group operations
remain explicit host phases; GAMESTATE methods use the same retained option levels.

The catalog also owns eight PlayerOptions objects, one per player and level.
Rust bindings preserve lazy methods, mutable state/speed tables, perspective
aliases, speed modes, string parsing, timing-window controls and noteskin callback
dispatch. Saved methods retain the appropriate owner across GC and observe
replaced state on later calls. PlayerState now retains these levels and distinct
SongPosition wrappers, provides option strings/arrays, and assigns options with
the native propagation and partial-failure order.

Host option phases preserve the chronological clock, stage restart, attack
rebuilding/clearing and SongOptions/PREFSMAN/player update order. Position queries
and sampling do not advance options. Controller and health queries read values
published by the host; the complete live health-update and message-dispatch phase
still needs migration. GAMESTATE exposes PlayerState and option methods.

The song-side NOTESKIN manager uses the existing resolver through
`RustSongRuntime::new_with_noteskin_resolver`. It snapshots song-local skin names,
provides typed metrics and paths, compiles metric commands, and loads Lua templates
with independent skin/button/element environments. Image and missing-resource
templates have native actor methods and independent concatenation state. Saved
commands retain their environments across collection and scene phases. The Var
fallback and its GameCommand queries use the current public GAMESTATE methods.

PROFILEMAN now provides the existing machine, player, fallback and local profile
objects through Rust bindings. Saved methods retain their original owner, mutable
user tables preserve identity, and setters keep conversion and side-effect order.
High-score lists expose the existing defaults and resolve current score-name
callbacks while preserving list mutation and collection during lookup. Profile
cycles are traced by the VM.

STATSMAN provides the existing stage and player statistics through Rust bindings.
Host reset and frame-feed phases preserve selected-step and table identities,
partial writes on errors, and recorded life versus current meter life. Live-player
publication is a separate phase; FailPlayer updates published state and queues
ordered writes when live mode is active. Saved queries retain only their required
objects. Full live orchestration and its query tracing still need migration.

Option-row factories reuse the existing Rust specifications for custom, theme,
configuration and operator rows. Selection loads use raw lists; saves resolve the
current row fields and preference/SL callbacks while retaining values across GC
and preserving partial writes. Saved save callbacks retain their row, while load
and enabled-player callbacks do not. Background fit, Center1Player, TableToString
and SM execute the unchanged production helper scripts. ScreenString, credit text
and Branch routing preserve the existing policies. Screen actor construction and
transition orchestration remain pending.

Screen input registration and dispatch are available through
`bind_screen_input` and `dispatch_input`. The registry retains callback identity in
a traced VM map, orders callbacks by identity, defers removal during dispatch,
and repairs its cursor after additions, nested dispatch or screen replacement.
Callbacks share one mutable event per dispatch. The existing finite callback
compiler now has a song-owned bytecode interface in `baked_program`; input
predicates retain its unchanged field schema. A compiled prefix avoids event
allocation, while hooks and stateful callbacks use the shared Rust VM. Binding a
replacement screen clears callbacks but preserves the sticky offline rejection.
Full screen construction and live input orchestration still need integration.

`publish_screen` publishes an already constructed screen and its players through
the existing SCREENMAN contract. The manager table survives screen changes;
saved GetPlayerAF functions retain their original pair. Top-screen queries use
raw loading/current-screen fields, while AddNewScreenToTop uses ordinary lookup
and assignment. SetNewScreen queues the latest target without replacing the
current screen. Optional source observations preserve every authored request and
its f32-derived timestamp. Request draining preserves the native priority between
new-screen and transition queues. Numeric reads and publication use the rooted
Rust stack API without internal calls visible to hooks. Generated screen/player
construction and transition command execution remain pending.

The private gameplay-player constructor now creates the existing PlayerActor,
its unnamed AttackDisplay frame and its LifeMeter. It preserves current player
positions, child aliases, OITG zoom truthiness, and the life query methods.
SetLife updates retained live players and statistics before the meter and its
synchronous LifeChanged message; error paths retain prior writes and clear the
recursion guard. Saved player setters use their supplied receiver without
retaining the old player. Saved meter queries retain their original meter.
The player Init phase now attaches Combo, Judgment, unnamed per-column
HoldJudgment frames and NoteField in the same owned scene. Named lookups alias
the ordered child tree; duplicate unnamed groups retain the AttackDisplay first.
Combo remains an invisible Actor when hidden or when no combo fonts exist.
Judgment owns its replaceable sprite, and hold sprites use the existing theme
path lookup and initial alpha. The screen's later theme commands, combo milestone
actors and full ScreenGameplay construction still need migration.

Private screen display construction also covers the timer, percentage-score
composites, lyric layers, background borders/brightness overlays, Debug and the
existing simple engine labels/frames. Score children retain their named-only
registries; backgrounds retain ordered owned lookups and duplicate child groups.
Lyric positioning reads current player options, and both text layers retain the
existing Changed callbacks and inherited LyricCommand animation helper.
Internal kinds retain the native owner-bound texture query and class overrides.
StepsDisplay now owns its Frame/Meter/Description/Autogen children and preserves
steps/trail/current-game selection, parameter identity, ordered Set callbacks and
immediate visibility. Its separate gameplay initialization installs the existing
metrics commands, player-dependent frame flip, text layout and MeterSet broadcasts.
Player constants are installed by the song host. In, Out and Cancel now own
actual transition children, animation queues, textures and command handlers.
Their base queues advance while dormant; child updates and frame callbacks are
gated, and completion is tested before the final child update. The virtual tween
time follows the retained child and excludes the transition owner's own queue.
The existing Center/CenterX/CenterY/FullScreen bindings read current screen globals.
The transition commands also use the installed Color, ivalues and enum-name helpers.
Generated screen constructors now enter a separate preparation-capture phase.
Queued commands dispatch under their captured time and retain shared Lua writes;
explicit property writes produce the existing typed command blocks. In/Out/Cancel
constructor output is compared block-for-block with the legacy control, including
queued visibility, easing and empty timed steps. Playback promotion clears the
captured queue without running those callbacks again and preserves preparation
position/aux getter behavior. The original failing preparation comparison remains
enabled and now passes. Constructor capture also preserves Stop/Finish/Hurry:
command cancellation, retained direct/immediate blocks, scaled capture clocks,
overflow finishing and current-position getters. Spin queue initialization is
kept separate from raw rotation fields. Full startup/frame capture scheduling,
recurring callbacks, the remaining capture channels and compiler/baking integration
are still pending.

Authored actors can now enter an explicit preparation phase before declarations
load. The first Init holds command queues across later layers and On; the compiler
can copy typed blocks, tween-step indices and pending commands before releasing
the barrier after Update(0). Zero-time discovery retains delayed commands and
queued messages for chronological replay. Stop/Finish clear deferred discovery,
and released scenes leave no pending startup owner. Complete frame-replay setup
and publication into the production compiler remain unfinished.

Construction and Init are separate host phases: `construct_actor` owns the graph,
`set_actor_scene_parent` assigns the foreground parent, and the compiler can read
initial render state before `initialize_actor_scene` runs child-first Init.
`append_actor_scene` adds the initialized root once and shares subtree ownership,
including wrappers, with the parent scene. Ordinary declarations subscribe during
Init; generated native actors retain their constructor registration. The existing
`instantiate_actor` convenience method still constructs and initializes together.
`begin_actor_preparation` installs the existing offline Def behavior: constructors
initialize the supplied table immediately, while Init remains a separate phase.
Repeated owned children and roots clone mutable state and command capture;
concatenation initially shares child identities and rebinds instance methods.
Cloning preserves resource state without repeating resource constructors.
Preparation `LoadActor` follows the existing production wrapper: it returns the
first Lua-file result and creates registered dummy instances for media and
non-string arguments. `LoadFont` uses the same ownership path; each NullActor
occurrence clones its reusable template. Initial media state, captured font
commands and scene-release collection are compared against production.

Saved callbacks resolve relative actor paths from their source file, including
tail calls. Redirects, wildcard ambiguity, default.lua/default.xml selection and
file errors preserve the existing policies and caller scope. The host entry keeps
its loader across authored global replacement and roots arguments across collection;
Lua errors and instruction limits restore file context. Noteskin source callbacks
retain their original source compiler. These phases do not yet replace the
production compiler's full startup, baking and live orchestration.

The screen-control bindings now provide the existing TopScreen methods, option
rows and MusicWheel backing slots. Screen publication, input binding and child
assembly remain separate phases. Saved wheel/row-constructor methods retain only
the owners they need; option rows have constructor ownership independent of the
screen. Transition requests preserve descendant Off ordering, Out activation and
partial writes when callbacks fail. Complete generated players use the existing
GetComboFonts scanner, backed by FILEMAN and StripSpriteHints. File queries reuse
the production path/filter/sorting rules; mutation methods remain sandbox stubs
with the same side-effect counts. Font discovery searches only the current theme
and preserves an empty result when no matching assets exist.

The private ScreenGameplay base factory now constructs both players and life
meters in native order, attaches enabled screen children, dispatches the initial
LifeChanged messages, and preserves stable draw order and the five unnamed
transition owners. Its underlay runs the unchanged Header, background filter,
difficulty meter, standard life meter and BPM scripts with the production theme
helpers. Constructor failure releases partial actors; the owned screen and retained
update closures survive GC and release cleanly at teardown. LifeMeter retains its
native child queries despite using the public Actor class.

Separate gameplay theme phases now load the original bundled combo milestones
and the five unnamed transitions. Fresh chunks preserve their source names and
environments; bitmap preparation uses the existing theme assets. Loaded actors
join the screen owner before Init, so failed loads and retained callbacks have
the same release boundary. Combo Init precedes parenting, and these phases leave
the startup queue barrier to the original underlay phase. Comparisons exercise
preparation/live commands, visual styles, GC, and the fontless Combo leaf.

The loaded underlay phase now runs the original Danger, StepStatistics, NoteField
and SongInfoBar files. It preserves the shared StepStatistics environment,
relative NoteField loaders, hidden option text, adoption order, startup queues,
Begin/On ordering and initial CurrentSongChanged delivery. Required enum,
numeric, time, color and alignment helpers match the existing native behavior.
Implicit Def/LoadFont owners created during a theme phase join the screen's
release boundary, including construction left behind when a callback fails.
The twelve focused underlay comparisons pass in preparation and live modes.

The loaded overlay/decorations phases now execute the original bundled files.
Overlay replacement preserves its engine-layer identity and happens before Init;
Header/Footer merge the metric On command and sort after their real Init commands.
Host uptime keeps the native preparation/live precision and observed clock-input
rules. Draw order and hibernation publish the fields consumed by these phases;
declaration provenance retains the original debug primitive. Nine comparisons
cover callback order, shared environments, judgment data, GC and teardown. The
existing preparation-mode diagnostic-probe error and its partial tree are also
preserved; live capture-probe loading succeeds.
The pre-activation full checkpoint passed 566 tests across all 55 Rust adapter targets. Source
and formatting audits pass; Clippy retains the same recorded diagnostics and
existing cmd.rs:349 failure.

Selection-aware screen construction now projects retained chart identities and
immutable metadata, seeds nonhuman players without authored selection messages,
and initializes statistics after the base screen from that selected context.
Reconstruction preserves process globals and RNG state while restarting options,
retiring old message subscriptions, replacing the input map, and resetting stage
statistics and clocks in native order. Theme loading is separate so its partial
actors remain explicitly owned when a callback fails. Eight comparisons cover
these boundaries, style reentry, timing fallback, retained old controls and
collection after repeated End/reconstruction/release cycles.

`retire_actor_scene` now transfers an outgoing scene from process roots to a
Lua-owned graph. Its host handle becomes inactive while saved Lua actors, wrappers
and sound handles remain usable. The graph keeps native references traced through
private host table attachments; once unreachable, GC queues Rust metadata cleanup
for the next actor-host operation. Shared scene owners remain respected. Wrappers
created after retirement also enter the current message registry, which roots
them until the next reconstruction even after Lua drops its external references.
Explicit hard release still invalidates actors when that is the caller's
intended operation. Scene handles now use a VM-scoped Root identity, preventing
one runtime from retiring another's scene.

`SongLayerPlan` validates the existing compiler inputs and preserves separate
physical-file and authored-source paths. `initialize_song_layers` executes those
entries in the retained VM, parents foreground actors before Init, then attaches
them in order and applies the existing delayed-name/visibility rules. Background
owners remain outside the screen's command children. UpperLeft wrapping, live
load-error/scalar fallback and Xero file environments follow production behavior.
All created actors, including implicit constructors from failed loads, join the
screen owner; its private traced state also owns the loader result table.

`begin_song_compilation` installs the production public debug table once, before
theme and authored loading. Internal loaders retain their original VM primitives;
reloads preserve author replacements of the public table. The authored entry
loader retains the raw chunk name for constructor provenance while public
`debug.getinfo` reports the current script context. Seven new cases cover input
validation and construction; the differential cases run through the actual
production compiler's Init boundary, including retained-state cleanup.

The initialization phase also captures players and the four screen layers before
loading entries, and each authored actor tree before Init. `song_initial_states`
returns those owned render values while the active screen traces their actor
identities. Later commands and GC cannot change the saved values; retiring or
releasing the screen makes the accessor unavailable. The reader preserves raw
property coercions/defaults, vectors and disabled FOV declarations. Three new
production-compiler comparisons cover preparation/live timing, authored render
properties and errors before Init. Generated player dummy sprites retain the
existing compiler's disabled animation default until a command changes it.

`activate_song_layers` performs the real startup through Update(0): typed Init
globals remain traced, Init screen requests are discarded at their original
boundary, Begin and In activation precede ScreenChanged and On, and startup
options are read before the initial update. Dynamic child/wrapper traversal,
deferred live updates, sleeping layers, partial callback errors and new
constructor cleanup are preserved. It returns the retained globals, options and
NewScreen request, owned Update(0) captures, wrapper defaults, preparation Init/On
tween captures, and runtime broadcast events. Startup text is retained after the
real initial update. Integration into complete compiled/live output remains required. Nine activation
cases and three capture comparisons use the actual production compiler through
this boundary.

`discover_song_startup_commands` retains shared script state before probing
startup queues. `restore_song_startup_locals` consumes that snapshot at the
compiler's chronological replay boundary. Globals and command environments,
referenced tables and closure cells keep their identities, aliases and cycles;
saved handles are traced through private scene state. Live and replaced screens
skip probing. Failed probes preserve their partial mutations without installing
a rewind. `song_startup_queued_states` returns the pre-probe actor properties,
queued blocks and ancestry clocks, including wrappers and removed siblings.
Discovery clears only earlier capture blocks while preserving tween timing and
pending commands. Sprite bindings are deep copied before probes; queued loads
record size, texture-bound and reset-frame writes. Full replay setup and complete
output integration remain required.

The earlier [activation checkpoint](../../docs/testing/2026-11-04-rust-lua-production-migration/song-activation-review.json) passed
140 tests across ten relevant targets on the final source, including nine new
activation cases. Its 1,195 source hashes match both test runs and Clippy;
formatting and source isolation pass. Existing tests and fixtures are unchanged.
Clippy retains the recorded 166 warnings and existing cmd.rs:349 error. This
focused run does not replace the historical full-suite checkpoint above.

The earlier [startup-capture checkpoint](../../docs/testing/2026-11-04-rust-lua-production-migration/song-startup-capture-review.json) passed
90 relevant regression tests across seven targets. After a sole `or_default`
style cleanup, all 12 activation tests pass again and Clippy matches its recorded
baseline. The 1,196-source audit verifies that exact difference, formatting,
isolated oracle changes and preservation of existing tests and fixtures. Captured
actor identities stay traced by the screen even if Update(0) removes a child;
owned state/block/queue values remain independent of later Lua mutations.

The earlier [shared-state checkpoint](../../docs/testing/2026-11-04-rust-lua-production-migration/song-shared-state-review.json) passed
60 tests across activation, preparation, scene retirement and song layers on the
final source. Six new comparisons cover cycles/aliases and replaced cells, custom
environment targets, forced GC, skipped live/replaced screens, errors, retirement
and an 8,201-table graph. All 1,198 watched source hashes match the final test and
lint runs; formatting and source isolation pass. Clippy retains exactly its
recorded 166 warnings and existing cmd.rs:349 error. The normal song compiler
still uses the legacy backend.

The earlier [queued-capture checkpoint](../../docs/testing/2026-11-04-rust-lua-production-migration/song-queued-capture-review.json) recorded
114 tests across eight relevant targets on its recorded source. Four new comparisons
cover queued actor/wrapper states, removed siblings across GC, preserved sprite
bindings and text, live/replaced screen skips, and malformed binding capture.
The media comparison exposed missing Sprite load capture writes; those are fixed.
All 1,198 watched hashes match the final regression and lint runs. Formatting,
source isolation and preservation of earlier coverage pass. Clippy retains the
recorded 166 warnings and existing cmd.rs:349 error. Production songs still use
the legacy backend; replay/output integration and cutover remain required.

The earlier [replay-setup checkpoint](../../docs/testing/2026-11-04-rust-lua-production-migration/song-replay-setup-review.json) recorded
127 passing tests on its recorded source across nine relevant targets. `begin_song_replay` restores saved script
locals, selected sprite bindings and compiler-provided frame-zero getter state,
then reconstructs Init/On queues, including callback-only actors. Five new
comparisons cover chronological queue order, hibernation/rates, GC, mode/owner
rejection, sprite restoration, easing options and getter publication around
relative/color writes and queue cancellation. All 1,199 watched source hashes
match the final regression and lint runs; formatting and preservation audits
pass. Clippy retains the same 166 warnings and existing cmd.rs:349 error.
The caller still needs complete output capture, including tween progress and
write provenance, plus tracked actor/options/media orchestration. Normal
production songs continue to use the legacy engine; cutover and final application
validation remain required.

The historical [tween-progress checkpoint](../../docs/testing/2026-11-04-rust-lua-production-migration/song-tween-progress-review.json) passed
106 tests across six relevant targets. Explicit song-time advancement now records
native tween time-left samples, including parent hibernation, child update rates,
wrapper spans and paused holds. Owned captures include the current values of
explicitly written tween properties. `SongTweenReplay::apply_startup_progress`
attaches reached samples to the original startup blocks without confusing
cancelled steps with later queue entries. Three new comparisons match the actual
legacy scheduler and block-finalization output, including partial callback errors,
GC, duplicate sample reads and invalid scene handles. All 1,199 watched source
hashes match regression and lint runs; formatting and preservation audits pass.
Clippy retains exactly the same 166 warnings and existing cmd.rs:349 error.
The historical [message-provenance checkpoint](../../docs/testing/2026-11-04-rust-lua-production-migration/song-message-provenance-review.json)
passed **112 tests across six targets** on its recorded source. `song_replay_tween_messages` returns owned,
ordered writes for the 25 tween properties, using the replay overlay indices.
It preserves repeated writes, cross-actor targets, nested broadcasts, per-write
beats, actual queue delays and easing metadata. Hurry flushes the open command
block before scaling the retained queue; cancellation and dispatch reset timing.
Three new legacy comparisons cover those cases, partial errors, GC and teardown.
All 1,200 source hashes match both verification runs; formatting and preservation
checks pass. Clippy retains its exact baseline diagnostics (166 warnings and the
existing cmd.rs:349 error). This is tween-message output only. Non-tween scheduled
and immediate capture, tracked player/layer indices and late wrapper remapping,
full compiler/live orchestration, production cutover and final application
validation remain required. Normal production songs still use the legacy engine.

The earlier [capture queue scheduling check](../../docs/testing/2026-11-04-rust-lua-production-migration/song-static-queue-review.json)
recorded **144 tests across nine targets**, including the previously failing
immediate resource publication case. Actors without startup steps retain command
capture: queued callbacks can run synchronously with a relative captured start.
Frame-created queues use actor update order and discard stale captured durations.
Nested dispatch restores the prior scope after errors. Immediate buffers retain
31 property targets, ordered message writes, relative delays and precise frame
timestamps. The original resource comparison now also checks that its callback
runs before frame advancement and only once. Two added comparisons cover nested
errors and earlier/later actor dispatch against the native frame timeline.
Formatting passes. Clippy retains its recorded 166 warnings and cmd.rs:349 error.
The only source change after the regression run corrects the scheduling API's
documentation; the audit verifies that exact comment-only difference and matches
all final source hashes to Clippy. Full queue-clock/recurring capture integration,
delayed property and pose output, complete compiler/live orchestration, cutover
and final application verification remain required. Normal production songs
still use legacy Lua.

The earlier [recurring capture check](../../docs/testing/2026-11-04-rust-lua-production-migration/song-recurring-capture-review.json) recorded **148 tests across nine targets**.
Commands retained before frame updates now run through the Rust adapter with
native interval precision, zero-interval behavior, the 64-run limit and callback
ordering. Hibernation, update rates, conditional stopping and Hurry/Stop/Finish
preserve the existing timer rules. Runtime callback errors retain partial writes,
report once and restore dispatch scope. Four added comparisons use actual native
frame advancement and compare immediate/message output; all 41 earlier activation
cases remain unchanged. Formatting passes. Clippy retains exactly its recorded
166 warnings and existing cmd.rs:349 error. All 1,202 watched source hashes match
the regression and lint runs. This covers recurrence captured outside updates;
dynamic queue-clock interaction, delayed property/pose output, complete compiler
and live-session integration, cutover and final application validation remain
required. The production migration is incomplete; the interpreter and C API
removal milestones stay closed.

The earlier [scheduled render-property check](../../docs/testing/2026-11-04-rust-lua-production-migration/song-render-properties-review.json) recorded **151 tests across nine
targets**. `drain_song_property_updates` exports the 31 immediate targets plus ten
scheduled renderer targets: depth flags, FOV, vanishpoint, blending, draw order,
Z bias, draw-by-Z and texture filtering. It shares one ordered frame buffer with
the immediate-only projection. Pending render destinations supplied by the
compiler survive drains, provide later starting values, and participate in
Stop/Finish. Finish merges current writes before updating render getters and
preserves the disabled-FOV sentinel. Three comparisons match the actual native
buffers, message writes and selected getters, including errors, GC and teardown.
All 45 earlier activation cases are unchanged. Formatting passes; Clippy retains
its exact 166-warning/cmd.rs:349-error baseline. All 1,203 watched source hashes
match both runs. Future queue-progress and cross-actor frame-advance prediction,
remaining text/sprite channels, full compiler/live orchestration, production
cutover and the original application validation remain required. Production song
execution still uses legacy Lua; the migration is incomplete.

The earlier [preparation queue-clock check](../../docs/testing/2026-11-04-rust-lua-production-migration/song-queue-clock-review.json) recorded **155 tests across nine
targets**. `set_song_replay_frames` installs the canonical timeline and optional
actor delta/span inputs; `advance_song_replay_at_frame` selects its exact song
time. The ten scheduled render targets now receive shared future progress arrays,
zero-state visibility, epoch-relative delays and cross-actor frame advancement.
Queued commands retain their dispatch remainder. Cancellation, Hurry, errors,
release and retirement preserve or clear clock state as appropriate; entering
live mode discards forecasts immediately. Four native comparisons cover those
paths and final-frame zero states. All 48 prior activation cases are preserved.
Formatting passes. Clippy retains the baseline 166 warnings and cmd.rs:349 error,
with no new diagnostics. All 1,204 watched source hashes match both final runs.
This verifies render-output prediction through the explicit timeline API; full
fallback/recurring execution, remaining capture channels, compiler/live-session
integration, production cutover and original application validation remain
required. Production song execution still uses legacy Lua.

The [fallback command-execution check](../../docs/testing/2026-11-04-rust-lua-production-migration/song-fallback-execution-review.json) passes **159 tests across nine
targets**. Full-timeline fallback commands now dispatch through the canonical
queue clock, preserving update-rate behavior, hibernation wakes at zero delta and
backdated publications. A new recurring command consumes only its saved dispatch
remainder once in that frame. Runtime queue errors report once, retain partial
writes and allow later actor updates. Inline child commands can clear their
caller's timestamp and restore it afterward. Four native comparisons cover these
paths; all 52 earlier activation cases and the original three failing cases are
unchanged. Formatting passes. Clippy retains its exact 166-warning/cmd.rs:349-error
baseline. All 1,204 watched source hashes match both final checks. Complete
fallback pose/getter reconciliation, remaining output channels, compiler/live
sessions, production cutover and original application validation remain required.
Normal production song execution still uses legacy Lua.

The session still owns outgoing End/retirement decisions. Complete compiler/session
startup, navigation, automatic retirement and output, preparation/fallback/cache
integration, and production cutover remain required.

The NoteField phase constructs its bundled blank Board sprite and every style
column eagerly, with distinct position, rotation and zoom spline handlers. The
spline methods use the existing Rust gameplay solver and preserve knot copying,
implicit zero points, resizing, looped evaluation and f32 arithmetic. Public
NoteField methods keep the native ActorFrame inheritance and forwarding rules;
unsupported native entries retain their existing errors. Tap callbacks preserve
argument conversion, synchronous reentry, partial writes and side-effect order.
Columns advance after the field callback with the incoming delta, outside the
field's update-rate and hibernation gates. Rebuilding columns invalidates the
cached update plan. Saved handler/spline getters retain only their selected
objects. These bindings still need attachment to the complete screen and the
compiler/live host orchestration.


GAMESTATE now exposes the production method surface through the writable GameState
class table and its protected instance metatable. Selection setters retain private
slots and immutable chart metadata, broadcast CurrentSteps changes synchronously,
and re-read the selected chart after callbacks before updating other players.
Preference reads, modifier queries/commands, environment values, style and clock
helpers retain their existing argument, side-effect and error behavior. Message
delivery and guarded ScriptError reporting share the actor host's implementation.
The frozen current-option controls and song-options-string fixture now exercise
the installed GAMESTATE methods. Compiler capture and screen-transition/live
orchestration still need migration.

GAMEMAN, CHARMAN, MEMCARDMAN, UNLOCKMAN and HOOKS now use Rust host bindings.
They retain the existing sandbox policies, including GAMEMAN's fresh single-style
list and the side-effect counters for manager operations. Unlock mutators chain
to their original owner; independent queries and style getters retain only the
data they need. These bindings do not introduce platform operations.

Generated gameplay actors, remaining managers and input orchestration, complete compiler capture
and live scenes still need migration. The production song compiler uses the legacy
engine until the complete replacement and its downstream behavior are verified.
This opt-in component is not the production cutover. See the
[migration ledger](../../docs/testing/2026-11-04-rust-lua-production-migration/README.md)
and [architecture contract](../../docs/lua-architecture.md).
