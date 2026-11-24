# StepMania 3.95 native theme

Choose **StepMania 3.95** in System Options, or set `Theme=StepMania 3.95`
under `[Options]` in `deadsync.ini`, then restart the game. Launch the regular
`deadsync.exe`. The theme uses bundled assets and native Rust actors; it does
not read the reference theme folder at runtime.

The local reference is `stepmania-395/Themes/default`. Original graphics,
bitmap fonts, music and effects were copied from that installation. Asset
provenance and adaptations are recorded in the theme's
[asset notes](../crates/deadsync-theme-stepmania-395/assets/README.md).

## Presentation coverage

| Area | Native presentation |
| --- | --- |
| Startup and title | Loading progress, Company and Logo intros, title animation and menu |
| Session setup | Profile picker adaptation, five style cards, eight difficulty/mode cards |
| Options | System Options, nested settings, Player Options and entry/exit transitions |
| Selection | Song and course wheels, original banners and frames, chart radar, course text banners, options prompt and stage introductions |
| Gameplay | HUD, life and score frames, judgments, combo, Ready/Go, fallback background, danger animation, cleared and failed sequences |
| Results | Song and Nonstop evaluation, P2 solo placement, radar achievements, survived count, session summary, name entry and Game Over |
| Idle attract | Company, Logo, animated tutorial pad, demonstration, cabinet ranking and Warning |

The theme owns its layout, animation, sounds and supported session pages. It
has no color-selection page in the reference, so it disables that optional
step through the shared theme contract. Cabinet preferences are retained for
other themes. Song/course preparation, judged results, profile updates and
navigation remain shared game behavior.

## Verification

The configured-theme route test launches each installed theme in an isolated
process using its INI setting. The native route covers P2 session setup,
Player Options, a judged song, evaluation, session summary, name entry and
Game Over. It then selects Nonstop through the normal menu, plays a parsed
two-stage course, checks its real song/artist metadata and accumulated P2
results, and returns to the course wheel. No reference scripts participate.

The hidden-window GPU capture fixture in `tests/attract_render.rs` renders 51
native scenes at 640 by 480, including transition frames, gameplay sequences,
P2-only results and Nonstop evaluation. Captures are written to
`target/native-395-attract-captures`. Focused presentation, navigation and
architecture tests supplement the visual checks.

The shell's ignored `native_gameplay_composition_captures` test also renders
five complete gameplay scenes with prepared chart data, resident stock noteskin
textures and the native HUD: P1, P2, Reverse, Versus, and a Versus song-script
foreground. These captures are written to `target/native-395-gameplay-captures`.
They check composition around the fields, including the absence of the default
profile footer and the score/life frames' position above hold bodies. The
selected noteskin and game-owned field geometry are retained.

## Fidelity boundaries

- Challenge, Endless and Magic Dance cards remain disabled because their
  source outcomes are not implemented by the shared mode controller.
- The reference has no local-profile browser; the shared picker uses the
  original options artwork and blank player-selection header.
- Cabinet rankings use real per-chart records. Legacy category-wide numeric
  histories and unlock points are unavailable, so those totals and the
  unlock-points attract page are omitted.
- Canonical receptors and notefield geometry remain game-owned. The source
  uses slightly different vertical receptor positions; the theme does not
  override shared gameplay geometry.
- The supplied default character has no dancer model. The tutorial animates
  the original training pad without inventing a dancer.

The shell still uses legacy default-theme controller and composer types, as
documented in [the architecture record](architecture.md). This port mounts
its native presentations through the shared contracts; it does not finish
that pre-existing controller migration.
