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

## Rust API migration

Workspace callers are updated for these source-level changes:

- Import `REFLEX_PAD_ID_BASE` and `PAD_ID_COUNT_CAP` from `deadsync-input`.
  Persisted device IDs are unchanged.
- Exhaustive `GameplayAction` matches must handle the new `Restart` action.
- Initialize gameplay through `init_gameplay_runtime`.
  `GameplayControlRuntimeState` now keeps one-shot target handling private and
  no longer supports struct-literal construction outside `deadsync-gameplay`.

## Enforcement

`tests/layer_dependencies.rs` checks Cargo metadata for every workspace member,
including newly added crates. It checks transitive workspace paths and uses
package names, so renamed dependencies do not hide upward edges. Optional,
target-specific, and build dependencies are included. Development dependencies
are excluded because tests may exercise multiple layers together. Third-party
dependency internals are not traversed.

`tests/architecture_boundaries.rs` additionally checks source ownership and the
specific APIs that protect canonical gameplay and notefield composition.

```sh
cargo test -p deadsync --test layer_dependencies --test architecture_boundaries
cargo test -p deadsync-gameplay --lib
cargo test -p deadsync-input --test input_pipeline_allocations
```

## Remaining separation

The production crate graph follows these directions, but screen/runtime
orchestration still needs further extraction. Simply Love currently wraps the
gameplay state and drives song-Lua lifecycle, input-repeat, and practice playback
sequencing; the shell names its concrete screen types. Move that sequencing into
game/application owners before replacing the concrete screen integration with a
theme-neutral presentation interface. Preserve native Update/input/Draw/Off
ordering while doing so.
