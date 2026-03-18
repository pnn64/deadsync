# Feature: Lift Notes

## Summary

Lift notes are a note type where the player must **release** a held button at the correct time, rather than pressing it. They are the inverse of tap notes. In .sm/.ssc files they are represented by the character `L`. DeadSync already has partial infrastructure for lifts (receptor glow fields, `NoLifts` modifier, `ArrowStats.lifts` counting in rssp) but does not parse, render, judge, or score them.

---

## How Lift Notes Work in ITGMania

### Data Model
- Lift is a distinct `TapNoteType_Lift` in the `TapNoteType` enum.
- Lifts are instantaneous (duration = 0), like taps — they are NOT hold-style notes.
- Stored as a `TapNote` struct with `type = TapNoteType_Lift`.

### Parsing
- Character `'L'` in .sm/.ssc chart data maps to a lift note, exactly like `'1'` maps to a tap.

### Rendering
- Lifts have their own `NotePart_Lift` enum value.
- Noteskins load a separate "Tap Lift" actor/sprite for lift notes, giving them a distinct visual appearance.
- Drawn with the same arrow-placement logic as taps, just using lift-specific graphics.

### Judgment / Input
- **Trigger: button RELEASE, not button press.** This is the defining difference.
- ITGMania's `Player::Step()` function uses the expression `(pTN->type == TapNoteType_Lift) == bRelease` — so lifts are judged when `bRelease == true`, taps when `bRelease == false`.
- Lifts use the **same timing windows** as tap notes (W1–W5 / Fantastic–Way Off).
- Lifts can miss if the player doesn't release within the timing window.
- Lifts contribute to combo and score identically to taps.

### Autoplay
- Autoplay must schedule a button release at the lift note's time (the column must already be held down beforehand).

---

## Current DeadSync Infrastructure (Already Exists)

| Item | Location | Status |
|------|----------|--------|
| `ArrowStats.lifts` field | `src/extern/rssp/src/stats.rs:27` | ✅ Exists, counts `'L'` chars |
| `REMOVE_MASK_BIT_NO_LIFTS` | `src/game/gameplay.rs:59` | ✅ Exists |
| NoLifts modifier option | `src/screens/player_options.rs:1434` | ✅ Exists |
| NoLifts attack parsing | `src/game/gameplay.rs:1903` | ✅ Exists |
| NoLifts debug message (notes not parsed) | `src/game/gameplay.rs:1626–1629` | ✅ Placeholder |
| `receptor_glow_lift_start_alpha/zoom` | `src/game/gameplay.rs:3436–3437` | ✅ Exists |
| Lift-specific receptor glow logic | `src/game/gameplay.rs:298–346` | ✅ Exists |
| CSV/JSON report fields for lifts | `src/extern/rssp/src/report.rs` | ✅ Exists |

---

## Implementation Plan

### Phase 1: Data Model

**1.1 Add `Lift` variant to `NoteType` enum**
- File: `src/game/note.rs`
- Add `Lift` to the `NoteType` enum after `Fake` (or between `Mine` and `Fake`).
- Lift notes have no `HoldData` — they're instantaneous like taps.

**1.2 Update all `match` arms on `NoteType`**
- The compiler will flag every incomplete match. Key locations:
  - `src/game/gameplay.rs` — `column_cue_is_mine()`: lift should return `Some(false)` (it's a judgable non-mine note, like a tap).
  - `src/game/gameplay.rs` — `recompute_player_totals()`: add `NoteType::Lift` to the count (either alongside `Tap` or as a separate `lifts` counter).
  - `src/game/gameplay.rs` — `is_note_judgable_for_capping()` (~line 830): `NoteType::Lift => !note.is_fake` (same as Tap).
  - `src/game/gameplay.rs` — any `NoteType::Tap | NoteType::Hold | NoteType::Roll` pattern that represents "judgable step notes" should include `NoteType::Lift` (e.g., lines 578, 989).
  - `src/game/gameplay.rs` — `NoteType::Tap | NoteType::Fake => {}` in totals: lifts go with Tap.
  - `src/game/gameplay.rs` — hold decay (`NoteType::Roll => TIMING_WINDOW_SECONDS_ROLL, _ => ...`): lifts aren't holds, so the existing wildcard is fine.
  - `src/screens/components/notefield.rs` — `tap_part_for_note_type()`: add a `NoteType::Lift` branch returning a lift-specific `NoteAnimPart`.
  - `src/screens/components/notefield.rs` — arrow rendering: if lift has its own sprite slot, branch like mines do; otherwise reuse the tap sprite with a tint/overlay.
  - Any `enforce_max_simultaneous_notes` logic: lifts count as steppable notes (same as tap).

### Phase 2: Parsing

**2.1 Parse `'L'` from chart data**
- File: `src/game/parsing/notes.rs` — `parse_chart_notes()`
- Add arm: `b'L' | b'l' => NoteType::Lift` (no `tail_row_index`, same as tap).

**2.2 Wire up NoLifts removal**
- File: `src/game/gameplay.rs` — `apply_uncommon_masks_with_masks()`
- Replace the debug log at line 1626–1629 with:
  ```rust
  if (remove_mask & REMOVE_MASK_BIT_NO_LIFTS) != 0 {
      notes.retain(|note| note.note_type != NoteType::Lift);
  }
  ```

### Phase 3: Judgment & Input

**3.1 Judge lift notes on button RELEASE**
- File: `src/game/gameplay.rs` — input edge processing loop (~line 8435)
- Currently, when `edge.pressed && !was_down && is_down` → calls `judge_a_tap()`.
- When `!edge.pressed && was_down && !is_down` → currently only calls `release_receptor_glow()`.
- **Add**: in the release branch, call a new `judge_a_lift(state, lane_idx, event_music_time)` function (or pass a `is_release` flag to `judge_a_tap`).

**3.2 Implement `judge_a_lift()` function**
- Mirrors `judge_a_tap()` but:
  - Only searches for `NoteType::Lift` notes (skips taps, holds, rolls, mines).
  - Uses the same timing windows as taps (`windows[0..4]`).
  - Applies the same scoring/judgment logic (grade assignment, combo, early hit rescoring, etc.).
  - Triggers tap explosion on hit.
  - Does NOT trigger `try_hit_mine_while_held` or `refresh_roll_life_on_step`.

**3.3 Auto-miss lifts that pass the timing window**
- File: `src/game/gameplay.rs` — `update_judged_rows()` or equivalent miss-detection code.
- Lift notes must be auto-missed when the music time passes beyond their timing window, just like taps. Verify the existing miss logic handles `NoteType::Lift` correctly (it likely will if lifts are treated as judgable non-mine notes).

### Phase 4: Rendering

**4.1 Add a `NoteAnimPart::Lift` variant** (or reuse `Tap` with a visual modifier)
- File: `src/screens/components/notefield.rs`
- Option A (preferred for ITGMania parity): Separate `NoteAnimPart::Lift` + load lift-specific sprite from noteskin.
- Option B (simpler MVP): Render lifts using the tap sprite but with a color tint or upside-down flip to distinguish visually.

**4.2 Add lift note graphics to default noteskin**
- Directory: `assets/noteskins/dance/default/`
- Add lift note sprites (e.g., `Down Tap Lift.png`, etc.) or redirect files.
- If going with Option B, no new assets needed initially.

**4.3 Update noteskin parsing**
- File: `src/game/parsing/noteskin.rs`
- Add a `lift_notes` field to the `Noteskin` struct (like `notes` for taps).
- Load lift sprites from the noteskin directory if present; fall back to tap sprites otherwise.

**4.4 Lift note tap explosion**
- Lift notes should trigger tap explosions on judgment, same as regular taps.
- Verify the explosion code in `notefield.rs` fires for lift judgments (it should if the judgment is stored in `note.result`).

### Phase 5: Autoplay

**5.1 Autoplay must handle lift notes**
- File: `src/game/gameplay.rs` — autoplay scheduling loop (~line 7650+)
- For lift notes, autoplay needs to:
  1. Ensure the column is already **pressed** before the lift note arrives.
  2. Schedule a **release** at the lift note's exact time.
- Add a `NoteType::Lift` branch in the autoplay note processing:
  ```
  NoteType::Lift => {
      // The column needs to be pressed before the lift.
      // If not already down, press it a bit early.
      // Schedule a release at the lift note's time.
  }
  ```
- This is the trickiest autoplay case. A simple approach: if the column is not held, press it at `row_event_time - AUTOPLAY_LIFT_PRE_HOLD_SECONDS`, then release at `row_event_time`.

### Phase 6: Scoring & Stats

**6.1 Lift note scoring**
- Lift notes should score identically to tap notes (same point values per timing window).
- Verify: `update_itg_grade_totals()`, FA+ scoring, DP scoring all treat lift judgments the same as taps.
- Lifts count toward `total_steps` (they are judgable row entries).
- Lifts increment/break combo like taps.

**6.2 `recompute_player_totals()` update**
- Add `NoteType::Lift` counting: either as a separate `lifts` total, or fold lifts into the step count (they already count via `count_total_steps_for_range` which counts all judgable non-mine notes).

**6.3 Jumps/hands counting**
- Lift notes should count as simultaneous notes for jump/hand detection, same as taps.

### Phase 7: Edge Cases & Polish

**7.1 Chart transforms**
- `enforce_max_simultaneous_notes`: lifts count toward simultaneous note limit (same as taps).
- `Little` modifier: lifts on non-4th/8th rows should be removed.
- `NoLifts` modifier: already has the mask bit — just wire up the `retain` (Phase 2.2).
- `Shuffle/Mirror/Blender`: lifts should be shuffled like taps (they're column-based).

**7.2 Replay recording & playback**
- Release edges are already recorded in the replay system (`edge.pressed` field). Verify that replaying releases triggers lift judgment correctly.

**7.3 Evaluation screen**
- Consider showing lift count on evaluation (e.g., "Lifts: X" alongside holds/rolls/mines).
- Use `chart.stats.lifts` from rssp (already available).

**7.4 Select music screen**
- Already shows `chart.stats.lifts` in the chart info panel if the rssp report includes it.

---

## File Change Summary

| File | Changes |
|------|---------|
| `src/game/note.rs` | Add `Lift` to `NoteType` enum |
| `src/game/parsing/notes.rs` | Parse `'L'`/`'l'` as `NoteType::Lift` |
| `src/game/gameplay.rs` | Judge lifts on release; autoplay lifts; NoLifts removal; update match arms |
| `src/screens/components/notefield.rs` | Render lift notes; lift tap explosions |
| `src/game/parsing/noteskin.rs` | Load lift note sprites (optional: fall back to tap) |
| `assets/noteskins/dance/default/` | Add lift note sprite assets (or use tap fallback) |

---

## Testing Strategy

1. **Parse a chart with `L` notes** — verify they appear as `NoteType::Lift` in the note vector.
2. **Render test** — verify lift notes appear on the notefield with distinct visuals.
3. **Judgment test** — press and hold a column, release on a lift note's timing window, verify correct grade.
4. **Miss test** — don't release on a lift note, verify it auto-misses.
5. **Autoplay test** — enable autoplay on a chart with lifts, verify perfect scores.
6. **NoLifts modifier** — enable NoLifts, verify lift notes are removed.
7. **Combo test** — verify lifts increment combo and break combo on miss.
8. **Replay test** — play a chart with lifts, save replay, replay it, verify scores match.
