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

## Gameplay policy and presentation

`deadsync-gameplay` owns target-score reachability and each player's one-shot
fail/restart decision. The HUD uses the same current-score facts and reachability
predicate, retaining its conditional calculations and single EX snapshot. Profile
adapters select the persisted ITG, EX, or Hard EX policy; themes map the resulting
`GameplayAction` to screen effects.

Frame ordering is part of compatibility. Target policy runs after song-Lua
updates and before frame input and Draw. Existing completion/cancellation takes
priority. Lua outro suppression still tests the original core action, so a
target-triggered failure retains its immediate exit. Autoplay invalidation and
natural-completion mine bookkeeping also live in the game and run at their
existing stage-exit boundaries.

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

## Rust API migration

Workspace callers are updated for these source-level changes:

- Import `REFLEX_PAD_ID_BASE` and `PAD_ID_COUNT_CAP` from `deadsync-input`.
  Persisted device IDs are unchanged.
- Exhaustive `GameplayAction` matches must handle the new `Restart` action.
- Initialize gameplay through `init_gameplay_runtime`.
  `GameplayControlRuntimeState` now keeps one-shot target handling private and
  no longer supports struct-literal construction outside `deadsync-gameplay`.
- `State::song_lua` is now a `SongLuaGameplaySession`. Its scene accessors retain
  `as_ref`, `as_mut`, `is_some`, `is_none`, and `take`; direct `Option` pattern
  matches or assignments must use the session API. Existing theme update/input
  wrappers delegate to the canonical controller, and `SongLuaHeldInputs` remains
  an alias for the game-owned reload token.

## Enforcement

`tests/layer_dependencies.rs` checks Cargo metadata for every workspace member,
including newly added crates. It checks transitive workspace paths and uses
package names, so renamed dependencies do not hide upward edges. Optional,
target-specific, and build dependencies are included. Development dependencies
are excluded because tests may exercise multiple layers together. Third-party
dependency internals are not traversed.

The song-Lua crate additionally has a test-build guard: its direct development
dependencies and their production closures must remain independent of themes and
the application shell. This keeps controller behavior tests usable by any theme.

`tests/architecture_boundaries.rs` additionally checks source ownership and the
specific APIs that protect canonical gameplay and notefield composition.

```sh
cargo test -p deadsync --test layer_dependencies --test architecture_boundaries
cargo test -p deadsync-gameplay --lib
cargo test -p deadsync-input --test input_pipeline_allocations
cargo test -p deadsync-song-lua --features test-support --test gameplay_session
```

## Remaining separation

The production crate graph follows these directions, and normal song-Lua
lifecycle and callback input sequencing now have game owners. Simply Love still
wraps gameplay state, coordinates lobby readiness and practice playback, and
exposes concrete screen types used by the shell. Move those remaining runtime
responsibilities into game/application owners before replacing the concrete
screen integration with a theme-neutral presentation interface. Practice has a
separate playback loop and must retain its editing and looping semantics.
