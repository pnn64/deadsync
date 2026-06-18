# ITGmania / Simply Love → DeadSync import mapping

This is the authoritative field-by-field reference for what DeadSync's profile
importer (**Options → Manage Local Profiles → Import from ITGmania**) reads from
an ITGmania + Simply Love local profile and where each value lands in DeadSync.

For the end-user walkthrough see
[migrating-from-itgmania.md](./migrating-from-itgmania.md); this document is for
contributors maintaining the importer.

## Source files in an ITGmania profile

An ITGmania `LocalProfiles/<id>/` directory contains:

| File | What the importer reads from it |
| --- | --- |
| `Editable.ini` | Display name, weight, birth year, player initials |
| `GrooveStats.ini` | GrooveStats API key, username, pad-player flag |
| `ArrowCloud.ini` | ArrowCloud API key |
| `Simply Love UserPrefs.ini` | `[Simply Love]` section → player options |
| `Avatar.png` (or `.jpg`/`.jpeg`) | Profile avatar image |
| `favorites.txt` | Favorited songs (resolved to chart hashes) |
| `ITL2026.json` | ITL event progress (scores, points, unlocks) |
| `Stats.xml` (or `Stats.xml.gz`) | Per-chart high scores |

Reader code: `src/game/import/itg.rs`. Each reader degrades gracefully — a
missing file or key yields an empty/default value rather than failing the whole
import.

## Pipeline overview

```
Editable.ini ─┐
GrooveStats ──┤
ArrowCloud  ──┼─► itg.rs (readers) ─► ItgSource ─► run.rs (orchestration)
UserPrefs.ini ┤                                        │
Avatar.png  ──┤                          options.rs ◄──┤ player options
Stats.xml ────┘                          resolver.rs ◄─┤ chart → GS hash
                                         import.rs   ◄──┘ score → LocalScoreEntry
```

- **`options.rs`** translates Simply Love mods → `PlayerOptionsData`.
- **`resolver.rs`** matches a `Stats.xml` chart (song dir + steps type +
  difficulty + description) to a chart in DeadSync's scanned library, recovering
  the GrooveStats `short_hash`.
- **`deadsync-score/src/import.rs`** maps one `<HighScore>` → `LocalScoreEntry`.
- **`run.rs`** writes the new profile and imported scores.

## 1. Profile metadata

Source: `Editable.ini` `[Editable]`. Writer: `src/game/profile.rs`
(`ImportProfileData`).

| ITGmania key | DeadSync field | Notes |
| --- | --- | --- |
| `DisplayName` | display name | |
| `WeightPounds` | weight (lbs) | parsed `u32`, `0` if absent |
| `BirthYear` | birth year | parsed `u32`, `0` if absent |
| `LastUsedHighScoreName` | player initials | sanitised; falls back to initials derived from the display name |
| `IgnoreStepCountCalories` | `ignore_step_count_calories` | disables step-count calorie estimation; parsed as bool |

The `Stats.xml` `GeneralData/CurrentCombo` is imported into the profile's
`current_combo` (the streak carried between songs) via `ProfileStats`. Known
packs are **not** imported — DeadSync marks all currently-scanned packs as known
on a new profile's first load, which is the desired behavior (importing only the
ITGmania-played subset would wrongly flag the rest as "new").

## 2. Online service keys

Sources: `GrooveStats.ini` `[GrooveStats]`, `ArrowCloud.ini` `[ArrowCloud]`.
Written into the new profile's lowercase `groovestats.ini` / `arrowcloud.ini`.

| ITGmania source | DeadSync target |
| --- | --- |
| `GrooveStats.ApiKey` | `groovestats.ini` ApiKey |
| `GrooveStats.Username` | `groovestats.ini` Username |
| `GrooveStats.IsPadPlayer` | `groovestats.ini` IsPadPlayer |
| `ArrowCloud.ApiKey` | `arrowcloud.ini` ApiKey |

The importer copies **credentials only**; it does not enable the services. The
machine-level toggles (`enable_groovestats`, `enable_arrowcloud`,
`enable_boogiestats`) live in global config and must still be turned on under
**Options → Online Scoring**. BoogieStats has no separate credential — it reuses
the GrooveStats key (`is_boogiestats_active()` is just
`enable_groovestats && enable_boogiestats`), so importing the GrooveStats key is
all that's needed for it.

## 3. Avatar

`Avatar.png` / `avatar.png` / `Avatar.jpg` / `Avatar.jpeg` (first match,
case-insensitive) is copied into the new profile directory as `profile.png`.

## 3a. Favorites

Source: `favorites.txt` (Simply Love). Reader: `read_favorites` /
`parse_favorites_text` in `src/game/import/itg.rs`; resolution + write in
`src/game/import/run.rs` and `src/game/profile.rs`.

Simply Love stores favorites **per song** as `Pack/SongFolder` lines, optionally
grouped under `---Section` headers (custom playlists). DeadSync stores favorites
**per chart** (`short_hash`). The importer therefore:

1. Reads each favorited song key, skipping `---` section headers and blanks.
2. Resolves the song against the scanned library (same `Pack/SongFolder`
   matching as scores).
3. Favorites **all of that song's charts'** hashes, so the song shows as
   favorited regardless of which difficulty is viewed.

Section/playlist grouping is **not** preserved (DeadSync has no favorites
sections); songs not in the library are reported as skipped in the summary.

## 3b. ITL event data

Source: `ITL2026.json` (Simply Love ITL event file). Reader: `read_itl_json` in
`src/game/import/itg.rs`; import in `src/game/scores/itl.rs`
(`import_itl_json`).

DeadSync's ITL support uses the **same `ITL2026.json` schema** Simply Love
writes — `pathMap` (song dir → hash), `hashMap` (hash → per-song event metadata:
EX, points, clear type, judgments, ranks…), and `unlockFolders`. The importer
parses the Simply Love file through DeadSync's own `ItlFileData` and writes it
into the new profile, so your ITL event progress (scores, points, unlocks)
carries over directly. Song ranks are recomputed when the profile's ITL cache
next loads. A missing, empty, or unparseable file imports nothing.

## 4. Player options (Simply Love)

Source: `Simply Love UserPrefs.ini` `[Simply Love]`. Translator:
`src/game/import/options.rs` (`translate_player_options`).

**Design rule:** only settings that map with high confidence are translated.
Anything DeadSync doesn't recognise — including custom theme graphics/fonts it
doesn't ship — is left at the DeadSync default rather than guessed. Simply Love
serialises with Lua `tostring`, so booleans are `true`/`false`.

### 4.1 Scalar / parsed values

| Simply Love key | Value format | DeadSync field | Translation |
| --- | --- | --- | --- |
| `SpeedModType` + `SpeedMod` | `X`/`C`/`M` + number | `scroll_speed` | → `ScrollSpeedSetting::{XMod,CMod,MMod}`; ignored if rate ≤ 0 |
| `Mini` | `"50%"` | `mini_percent` | leading signed int |
| `Spacing` | `"-25%"` | `spacing_percent` | leading signed int |
| `NoteFieldOffsetX` | int | `note_field_offset_x` | leading signed int |
| `NoteFieldOffsetY` | int | `note_field_offset_y` | leading signed int |
| `VisualDelay` | `"12ms"` | `visual_delay_ms` | leading signed int |
| `TiltMultiplier` | float | `tilt_multiplier` | parsed `f32` |
| `MeasureCounterLookahead` | int | `measure_counter_lookahead` | clamped `0..=255` |
| `NoteSkin` | name | `noteskin` | `NoteSkin::new` (passthrough) |

### 4.2 Theme graphics & font

Simply Love stores the **full sprite filename** for the graphics and the **font
directory name** for the combo font. These resolve through DeadSync's stock
alias tables via a *stock-only* parse (`from_stock_name`): a name DeadSync
doesn't ship resolves to `None` and the base default is kept (so the profile
never points at a missing texture).

| Simply Love key | Example value | DeadSync field |
| --- | --- | --- |
| `JudgmentGraphic` | `Love 2x7 (doubleres).png` | `judgment_graphic` |
| `HeldGraphic` | `Love (doubleres).png` | `held_miss_graphic` |
| `HoldJudgment` | `ITG2 1x2 (doubleres).png` | `hold_judgment_graphic` |
| `ComboFont` | `Wendy`, `Bebas Neue` | `combo_font` (`FromStr`; unknown → default) |

### 4.3 Enum-valued settings

Translated via DeadSync's `FromStr`, which normalises case/punctuation and
rejects unknown vocabularies (unknown → default preserved).

| Simply Love key | DeadSync field | Accepted values |
| --- | --- | --- |
| `BackgroundFilter` | `background_filter` | `0`–`100` (or `Off`/`Dark`/`Darker`/`Darkest`) |
| `ComboColors` | `combo_colors` | `Glow`, `Solid`, `Rainbow`, `RainbowScroll`, `None` |
| `ComboMode` | `combo_mode` | `FullCombo`, `CurrentCombo` |
| `LifeMeterType` | `lifemeter_type` | `Standard`, `Surround`, `Vertical` |
| `MeasureCounter` | `measure_counter` | `None`, `8th`, `12th`, `16th`, `24th`, `32nd` |
| `MeasureLines` | `measure_lines` | `Off`, `Measure`, `Quarter`, `Eighth` |
| `ErrorBarTrim` | `error_bar_trim` | `Off`, `Fantastic`, `Excellent`, `Great` |
| `MiniIndicator` | `mini_indicator` | `None`, `SubtractiveScoring`, `PredictiveScoring`, `PaceScoring`, `RivalScoring`, `Pacemaker`, `StreamProg` |
| `DataVisualizations` | `step_statistics` | `None`/`Target Score Graph` → empty; `Step Statistics` → all widgets |
| `StepStatsExtra` | `step_stats_extra` | `None`, `ErrorStats`, and the GIF widgets (`AmongUs`, `CatJAM`, `Nyan Cat`, `Sonic`, …) |
| `TargetScore` | `target_score` | only `Machine best` / `Personal best` (SL's `SpecifiedValue`+number and `Ghost Data` have no equivalent → default) |

### 4.4 SelectMultiple flag groups → bitmasks

Applied only when at least one flag of the group is present (otherwise the
DeadSync default is kept).

**Error-bar style** → `error_bar_active_mask` (then `error_bar` and
`error_bar_text` are derived from the mask):

| Simply Love flag | `ErrorBarMask` bit |
| --- | --- |
| `Colorful` | `COLORFUL` |
| `Monochrome` | `MONOCHROME` |
| `Text` | `TEXT` |
| `Highlight` | `HIGHLIGHT` |
| `Average` | `AVERAGE` |

**Judgment flash** → `column_flash_mask`:

| Simply Love flag | `ColumnFlashMask` bit |
| --- | --- |
| `FlashMiss` | `MISS` |
| `FlashWayOff` | `WAY_OFF` |
| `FlashDecent` | `DECENT` |
| `FlashGreat` | `GREAT` |
| `FlashExcellent` | `EXCELLENT` |
| `FlashFantastic` | `BLUE_FANTASTIC` **and** `WHITE_FANTASTIC` |

> Simply Love has a single "Fantastic" flash; DeadSync splits fantastic into
> blue (W0/FA+) and white (W1) columns, so a single `FlashFantastic` enables
> both bits.

### 4.5 Engine modifier string

`PlayerOptionsString` (comma-separated engine mods, e.g.
`"1.5x, Reverse, Mirror"`) contributes:

| Token(s) | DeadSync field |
| --- | --- |
| `reverse` | `scroll_option |= Reverse`, `reverse_scroll = true` |
| `split` / `alternate` / `cross` / `centered` | corresponding `ScrollOption` bit |
| a parseable `TurnOption` (e.g. `mirror`, `left`, `right`, `shuffle`) | `turn_option` |

### 4.6 Boolean toggles (1:1)

These Simply Love booleans map directly to the same-meaning DeadSync field:

| Simply Love key | DeadSync field |
| --- | --- |
| `HideTargets` | `hide_targets` |
| `HideSongBG` | `hide_song_bg` |
| `HideCombo` | `hide_combo` |
| `HideLifebar` | `hide_lifebar` |
| `HideScore` | `hide_score` |
| `HideDanger` | `hide_danger` |
| `HideComboExplosions` | `hide_combo_explosions` |
| `MeasureCounterLeft` | `measure_counter_left` |
| `MeasureCounterUp` | `measure_counter_up` |
| `MeasureCounterVert` | `measure_counter_vert` |
| `BrokenRun` | `broken_run` |
| `RunTimer` | `run_timer` |
| `RainbowMax` | `rainbow_max` |
| `ResponsiveColors` | `responsive_colors` |
| `ShowLifePercent` | `show_life_percent` |
| `ColumnFlashOnMiss` | `column_flash_on_miss` |
| `SubtractiveScoring` | `subtractive_scoring` |
| `Pacemaker` | `pacemaker` |
| `TrackEarlyJudgments` | `track_early_judgments` |
| `ScaleGraph` | `scale_scatterplot` |
| `NPSGraphAtTop` | `nps_graph_at_top` |
| `JudgmentTilt` | `judgment_tilt` |
| `ColumnCues` | `column_cues` |
| `ColumnCountdown` | `column_countdown` |
| `ErrorBarUp` | `error_bar_up` |
| `ErrorBarMultiTick` | `error_bar_multi_tick` |
| `ShowFaPlusWindow` | `show_fa_plus_window` |
| `ShowExScore` | `show_ex_score` |
| `ShowFaPlusPane` | `show_fa_plus_pane` |
| `SmallerWhite` | `fa_plus_10ms_blue_window` |
| `SplitWhites` | `split_15_10ms` |
| `HideEarlyDecentWayOffJudgments` | `hide_early_dw_judgments` |
| `HideEarlyDecentWayOffFlash` | `hide_early_dw_flash` |
| `DisplayScorebox` | `display_scorebox` |
| `JudgmentBack` | `judgment_back` |
| `ErrorMSDisplay` | `error_ms_display` |

## 5. Offline scores

Source: `Stats.xml`
`SongScores/Song[@Dir]/Steps[@StepsType,@Difficulty,@Description]/HighScoreList/HighScore`.
Mapper: `deadsync-score/src/import.rs` (`local_score_from_itg`).

### Chart resolution (`resolver.rs`)

`Stats.xml` keys a chart by song folder + steps type + difficulty (+ optional
edit description), **not** by hash. The resolver normalises the song directory
and looks it up in DeadSync's scanned song library to recover the GrooveStats
`short_hash`. Outcomes:

- **Found** → the score is mapped and attached to that chart.
- **SongNotFound** / **ChartNotFound** → counted in the import summary and
  skipped. Scan your packs (and add your ITGmania `Songs` folder) before
  importing so more scores match.

### `<HighScore>` → `LocalScoreEntry`

| ITGmania element | DeadSync field | Notes |
| --- | --- | --- |
| `Grade` (`Tier01`…`Failed`, with or without a `Grade_` prefix) | `grade_code` | via `grade_from_itg`; unrecognised grade → score skipped |
| `PercentDP` (0.0–1.0) | `score_percent` | clamped `0.0..=1.0` |
| `DateTime` (`YYYY-MM-DD HH:MM:SS`, local) | `played_at_ms` | epoch ms; `0` if unparseable |
| `Modifiers` (`"1.5xMusic, …"`) | `music_rate` | parsed `xMusic` token; default `1.0` |
| `TapNoteScores/{W1,W2,W3,W4,W5,Miss}` | `judgment_counts` | `[W1, W2, W3, W4, W5, Miss]` |
| `TapNoteScores/AvoidMine` | `mines_avoided` | |
| `TapNoteScores/HitMine` + `AvoidMine` | `mines_total` | |
| `HoldNoteScores/Held` | `holds_held` | |
| `HoldNoteScores/{Held,LetGo,MissedHold}` | `holds_total` | summed |
| `SurviveSeconds` | `fail_time` | only for `Grade_Failed` |
| (recomputed from judgments) | `lamp_index`, `lamp_judge_count` | `compute_local_lamp` |

**Always defaulted (not recoverable from `Stats.xml`):**

| DeadSync field | Value | Why |
| --- | --- | --- |
| `ex_score_percent` | `0.0` | ITGmania stores no FA+ (W0) split |
| `hard_ex_score_percent` | `0.0` | same |
| `rolls_held`, `rolls_total` | `0` | `Stats.xml` folds holds and rolls together |
| `hands_achieved` | `0` | not stored per high score |
| `beat0_time_ns` | `0` | no replay data |
| `replay` | empty | no per-tap offset data in `Stats.xml` |

## Known limitations

- **Scores only attach to charts present in DeadSync's library** (hash recovered
  via the resolver); unmatched charts are reported and skipped.
- **EX / Hard-EX can't be reconstructed** — ITGmania records only bucketed
  judgments (no W0 split, no per-tap timing), so EX starts at 0.
- **Holds vs rolls aren't distinguished**, and mine tallies are partial.
- **Auto-detection** covers the per-user save dirs
  (`%APPDATA%\ITGmania\Save\LocalProfiles`, `~/.itgmania/...`,
  `~/Library/Application Support/ITGmania/...`). **Portable installs** aren't
  auto-detected, but **"Browse for game directory…"** in the import picker opens
  a native folder dialog and resolves profiles from the chosen game folder using
  ITGmania's own `Portable.ini` rule (`detect.rs`).

## Deliberately not imported

Everything below is present in an ITGmania / Simply Love profile but **intentionally
left to DeadSync's defaults**. This is the definitive list; each row records *why*
it is excluded so the decision is auditable. Items that **are** imported (even
partially) are noted inline and are not repeated here.

**Reason codes**

| Code | Meaning |
| --- | --- |
| `NO-TARGET` | DeadSync has no field/feature for this — importing would require a new schema field (and usually UI) with nothing to read it yet. |
| `NO-MAP` | A target exists but the value vocabularies/semantics don't correspond, so any mapping would be a lossy guess. The translator deliberately never guesses. |
| `COUNTERPRODUCTIVE` | A faithful import would produce *worse* behavior than DeadSync's default. |
| `REDUNDANT` | Off by default and/or already covered by a source we do import. |
| `NOT-MODELED` | DeadSync doesn't implement the underlying feature (courses, characters, unlock system, …). |

### Player options (`Simply Love UserPrefs.ini`)

| Setting(s) | Reason | Detail |
| --- | --- | --- |
| `MiniIndicatorColor` | `NO-MAP` | SL is a **fixed-color** picker (`Default`, `Red`, `Blue`, `Yellow`, `Green`, `Magenta`, `White`). DeadSync's enum is a **coloring strategy** (`Default`/`Detailed` = score gradient, `Combo` = match combo color). Only `Default`↔`Default` lines up — a no-op — and every actual color choice has no representation. |
| `TargetScore` = `SpecifiedValue` (+ `TargetScoreNumber`), `Ghost Data`; `ActionOnMissedTarget` | `NO-MAP` | DeadSync's `target_score` is a grade (`C-`…`S+`) or `Machine/Personal best`; it has no numeric-percent or ghost-data target. **`Machine best` / `Personal best` *are* imported.** |
| `PackBanner`, `StepInfo` | `NO-MAP` | These would set individual `step_statistics` bits (`PACK_BANNER` / `SONG_INFO`), but they collide with the all-or-nothing `DataVisualizations` → `step_statistics` mapping we already apply. |
| `SBITGScore`, `SBExScore`, `SBEvents` | `NO-TARGET` | Scorebox sub-toggles with no matching DeadSync field. |
| `TrackRecalc`, `TrackFoot` | `NO-TARGET` | No corresponding DeadSync option. |
| `TimerMode`, `JudgmentAnimation`, `RailBalance`, `GhostFault`, `BreakUI`, `GrowCombo`, `SpinCombo`, `WildCombo`, `RainbowComboOptions`, `TiltOptions`, `Waterfall`, `FadeFantastic`, `NoBar` | `NO-TARGET` | SL-only aesthetic / novelty effects DeadSync doesn't implement. |

### Profile metadata & `GeneralData`

| Field(s) | Reason | Detail |
| --- | --- | --- |
| `known_pack_names` (derived) | `COUNTERPRODUCTIVE` | DeadSync marks **all** currently-scanned packs as known on a new profile's first load. Importing only the ITGmania-played subset would wrongly flag every other pack as "new". |
| `CharacterID` | `NOT-MODELED` | DeadSync has no character system. |
| `Voomax`, `IsMale` | `NO-TARGET` | Calorie-model inputs DeadSync doesn't model (weight + birth year **are** imported, as is `IgnoreStepCountCalories`). |
| Lifetime totals: `TotalDancePoints`, `TotalSessions`, `TotalSessionSeconds`, `TotalGameplaySeconds`, `TotalTapsAndHolds`, `TotalJumps`, `TotalHolds`, `TotalRolls`, `TotalMines`, `TotalHands`, `TotalLifts`, `NumToasties`, `NumExtraStagesPassed/Failed` | `NO-TARGET` | No lifetime-stats store or display in DeadSync. |
| Play-count histograms: `NumSongsPlayedBy{PlayMode,Style,Difficulty,Meter}`, `NumTotalSongsPlayed`, `NumStagesPassedBy{PlayMode,Grade}` | `NO-TARGET` | No aggregate play-count store. |
| `LastDifficulty`, `LastStepsType`, `Song`, `Course`, `LastPlayedDate` | `NO-TARGET` | DeadSync tracks last-played by chart hash internally; ITGmania's last-selection isn't portable. |
| Calorie history: `CalorieData/CaloriesBurned[@Date]`, `TotalCaloriesBurned`, `GoalType`/`GoalCalories`/`GoalSeconds` | `NO-TARGET` | Only *today's* calories are tracked; cross-machine daily history isn't meaningful. (`CurrentCombo` **is** imported.) |
| `Unlocks/UnlockEntry` | `NOT-MODELED` | Different unlock system. (ITL unlock folders **are** imported via `ITL2026.json`.) |

### Scores (`Stats.xml`)

| Field(s) | Reason | Detail |
| --- | --- | --- |
| Per-chart `HighScoreList/NumTimesPlayed`, `LastPlayed`, `HighGrade` | `NO-TARGET` | DeadSync stores the best score per chart, not play counts / last-played per chart. |
| Per-score `MaxCombo`, `Name`, `RadarValues`, `Disqualified`, `StageAward`, `PeakComboAward`, checkpoint counts | `NO-TARGET` | Not fields on `LocalScoreEntry`; would need a score-schema change. (Judgments, holds, mines, percent, grade, date, and music rate **are** imported.) |
| `CourseScores`, `CategoryScores` | `NOT-MODELED` | DeadSync has no course / ranking-category score store. |
| `ScreenshotData` | `NOT-MODELED` | No screenshot metadata store. |

### Simply Love extras

| Source | Reason | Detail |
| --- | --- | --- |
| `SL-Scores/*.json` | `REDUNDANT` | Per-play archive, **off by default** (`WriteCustomScores` theme pref). Richer than `Stats.xml` (rolls-vs-holds split, `MaxCombo`) but would duplicate the best-score data we already import and require play-level dedup. |
| `favorites.txt` section / playlist names (the `---Section` headers) | `NOT-MODELED` | DeadSync favorites have no section grouping. **The favorited songs themselves are imported.** |

## Source-of-truth code

| Concern | File |
| --- | --- |
| ITGmania file readers | `src/game/import/itg.rs` |
| `Stats.xml` parser | `src/game/import/xml.rs` |
| Player-options translation | `src/game/import/options.rs` |
| Chart resolver | `src/game/import/resolver.rs` |
| Score mapping | `crates/deadsync-score/src/import.rs` |
| Orchestration / profile writer | `src/game/import/run.rs`, `src/game/profile.rs` |
| Auto-detection | `src/game/import/detect.rs` |
| In-game UI | `src/screens/manage_local_profiles.rs` |
