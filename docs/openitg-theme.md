# OpenITG theme progress

Choose **OpenITG** in **Options → System → Theme**, then restart DeadSync.
The equivalent setting is `Theme=OpenITG` under `[Options]` in `deadsync.ini`.
Simply Love is the default. The selection mounts that theme's title, Player
Options, and base Select Music layout. Both themes use the same DeadSync
session flow.

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

## Player Options

Choose Player Options through the usual second Start on a confirmed song. The
OpenITG screen shows the selected stepchart and edits X/C/M mode, scroll speed,
and music rate. Each active side has its own row focus. The editor submits the
game's stage settings and uses the same profile and preview-rate services as
Simply Love. Chart selection, course validation, practice, and failed-load
recovery follow the shared application flow.

The effective reference uses the `ScreenPlayerOptions` section in patch
`default/metrics.ini` and the parent `ScreenOptions` metrics. The row labels
start at x=53, rows begin at y=127 with 28 px spacing, and Exit sits at y=353
on the 640×480 layout. The implemented rows use those anchors and original
graphics. Stepchart information is read-only here. Later rows, including
perspective, noteskins, and effects, are pending. The reference's timing
scaling and workout overrides belong to game rules. The editor retains
DeadSync's existing behavior for rate and profile changes.

Focused tests cover recovery and resubmission from both themes, P2 profile
saves, two-player confirmation, and matched session and audio rate changes.
The actor checks cover row anchors at 640×480 and 854×480.

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

## Gameplay HUD

The OpenITG theme draws a vertical Standard life bar and a two-decimal
percentage score from the same stage values used by Simply Love. The shell
selects these widgets once at stage entry. Draws occur inside the existing
HUD and Lua score proxy layers, with the same profile visibility, physical
player side, and gameplay state.

The effective `ScreenGameplay` metrics place the life frames at `(24, 270)`
and `(616, 270)`, with a 272 px bottom-up fill. Score text anchors are
`(160, 56)` and `(478, 56)` for one-side charts, and `(200, 56)` and
`(440, 56)` for doubles. The original widget art uses those positions on a
640×480 canvas centered at wider resolutions. Slide-in timing, animated life
glow, song and stage banners, judgment and combo styling, and specialized
score and life modes remain open.

Actor tests cover native and wide layouts, failed life, and EX score tint.
A song Lua fixture captures the OpenITG score with Underlay hidden and still
draws the canonical player proxy. The existing independent footer fade test
fails at 0.75 seconds in both the earlier Windows build and this build.

An actor-construction benchmark drew two scores per frame at 854×480. Across
nine runs of 400,000 frames, the median was 1.05 μs for the Simply Love digits
and 0.75 μs for the OpenITG inline label. Both used optimized widget code with
shared dev-profile rendering dependencies and a reused actor buffer. Scene
composition, GPU timing, and live frame pacing remain to be measured.

System Options and Evaluation still use Simply Love.
End-to-end parity across two complete presentations, visual comparison against
a running reference, and distribution validation remain open.
