# Architectural layers

DeadSync separates reusable engine functionality, game behavior, and replaceable
presentation. A theme may change how game information is displayed; judgments,
scoring, failure, input semantics, and chart behavior belong to the game.

| Layer | Crates | Responsibilities |
| --- | --- | --- |
| Engine | `deadlib-*` | Generic rendering, audio, physical input, assets, Lua, and platform services. No dependency on DeadSync or its themes. |
| Game | `deadsync-*`, excluding themes and the application shell | Timing, judgments, scoring, player/session state, logical input, receptors, arrows, notefields, noteskins, song Lua, profiles, and game services. No dependency on theme crates. |
| Themes | `deadsync-theme`, `deadsync-theme-*` | Screen presentation, layout, fonts, colors, menus, evaluation, and HUD appearance. Consume game data and translate user actions into requests. |

Production dependencies point downward. Themes can use game and engine crates;
game crates can use engine crates. The `deadsync` executable and `deadsync-shell`
are the composition roots that select and connect the concrete implementation.
Neither is a dependency of engine, game, or theme crates.

## Shared contracts

Shared data belongs below its consumers, so game code does not need to import a
theme to describe an input or store a setting:

- `deadsync-core::hud_style` defines plain HUD appearance inputs. Themes supply
  their values; canonical receptor geometry and note ordering stay in the game.
- `deadsync-core::color` defines saved color-setting names, judgment roles, and
  palette data. Concrete palette values remain supplied by themes.
- `deadlib-assets::AssetManifest` describes fonts, textures, and texture sampling
  requirements. The game resolves its catalog and paths before engine loading.
- `deadsync-theme` reexports the original public style/color names and
  `ThemeAssetManifest` alias for caller compatibility. These are the same types,
  with no wrapper allocation or conversion.
- `deadsync-input` owns the logical device namespace, including persisted RE:Flex
  IDs. `deadlib-platform::input` owns physical events and native transport limits.
- `deadsync-input::devices` owns the StepManiaX and RE:Flex provider identities.
  The shell adapts OS connection events by moving their existing payload into the
  game's provider type. Native backends ask the host whether another provider
  claimed a device. DeadSync supplies its existing live StepManiaX ownership
  predicate, with no change to device slots or hot-plug ordering.

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
pending restart before offset-prompt or transition-busy early returns. The theme
maps the resulting destination to its screens. Explicit song-Lua navigation to
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
immutable callback. There is no additional retained stage buffer or chart search.

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

Endless renewal accepts `CourseCyclePlan`, containing only the path and stages.
It preserves the existing run's rules and score identity, checks the exact path
and game-owned append authorization, and retains partial resolution and rollback
on loader failures. Direct renewal and fallback paths no longer construct unused
labels or a summary song template. Readiness checks borrow the selected rating
without cloning its stage vector. Legacy theme selection functions remain
adapters, and their consuming split/recombine operations move existing storage.

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
score targets before the shell constructs the screen. Course banner and title
overrides are applied during presentation. The shell installs live Lua callbacks
after the theme builds its initial caches, then restores held inputs for a
replacement stage. Render scratch warms at the same callback-install boundary.
Simply Love's public combined constructor delegates to the shared preparation
and presentation paths for existing callers.

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
waiting-input consumption, machine-stat publication, and one-time music start.
The shell calls its entry, update, and snapshot-refresh operations directly.
An entered lobby always waits until the next update, and the server snapshot
decides when playback begins. The update that starts playback publishes pre-start stats,
starts music, and publishes post-start stats in the original order. A completed
disconnect hold clears the local snapshot and starts offline in the same update.
The shell drains the queued audio command before sampling the stream clock.
Readiness and reconnect changes never re-pause a stage that has already started.
The entry-time refresh flag and active local sides remain fixed for the stage.
Themes retain lobby HUD visibility, translated status text, and countdown display.

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

## Remaining separation

The production crate graph follows these directions. Gameplay and Practice
startup, normal song-Lua lifecycle,
callback input sequencing, practice playback/editor behavior, and gameplay lobby
coordination now have game owners, as do Evaluation and Select Music lobby input
barriers, disconnect handling, and remote song-selection synchronization. Simply
Love still wraps gameplay state, while gameplay noteskin preparation and effect
conversion now have a game owner. Course continuation and restart destinations,
course progress, interruption, Endless renewal, course modes, life selection,
difficulty availability, runtime stage preparation, and course score aggregation
now have game owners. `CourseRunState` keeps application-owned result history
and display metadata alongside the game run.

The shell still uses concrete theme screen types. Consolidate that integration
behind a theme-neutral presentation interface after the remaining runtime work;
concrete screen identifiers, menu layout, translations, and transition appearance
remain valid theme responsibilities.
