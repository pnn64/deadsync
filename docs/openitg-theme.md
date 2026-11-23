# OpenITG theme progress

Choose **OpenITG** in **Options → System → Theme**, then restart DeadSync.
The equivalent setting is `Theme=OpenITG` under `[Options]` in `deadsync.ini`.
Simply Love is the default. The selection mounts that theme's title and Player
Options. Both themes continue into the same DeadSync session flow.

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

System Options, Select Music, gameplay presentation, and Evaluation still use
Simply Love. End-to-end parity across two complete presentations, visual
comparison against a running reference, and distribution validation remain
open.
