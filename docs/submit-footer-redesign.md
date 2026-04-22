# Submit Footer Redesign — Implementation Tracking Plan

## Goal
Replace the current multi-line, verbose evaluation submit footer with a
single condensed line of bracketed per-backend status cells. Use two
distinct animated sprites (spinner = active, hourglass = waiting) plus a
small set of static glyphs (`✔`, `↻ F5`, `⊘`).

See `submit-footer-redesign.md` (on Desktop) for the full design spec, state
table, and example outputs. This file tracks the concrete work.

---

## Final spec (source of truth for implementation)

### Cell format
```
[<BACKEND> <ICON> [<DELAY>] [<REASON>]]
```

### Per-cell state mapping
| Backend State | Cell Output | Icon source |
|---|---|---|
| Submitting | `[GS ◐]` | `LoadingSpinner_10x3.png` (animated) |
| Submitted | `[GS ✔]` | static text |
| Waiting (auto retry) | `[GS ⧗ 8s Timeout]` | `Hourglass_10x3.png` (animated) |
| Waiting (manual cooldown) | `[GS ⧗ 4s Server 502]` | `Hourglass_10x3.png` (animated) |
| Manual retry ready | `[GS ↻ F5 Network]` | static text |
| Rejected (terminal) | `[GS ⊘ Invalid Score]` | static text |

### Reason short-forms
- `Timeout`
- `Network`
- `Server {code}`
- `<reject reason>` (passthrough)

### Backend label
- `GS` (or `BS` when BoogieStats active) — GrooveStats
- `AC` — ArrowCloud

### Layout
- One footer line, cells joined by single space.
- Empty when neither backend expected.
- Cells always bracketed with backend label, even single-backend case.

---

## Work breakdown

### Phase 1 — Assets (mostly done)
- [x] Generate `assets/graphics/submit/Hourglass_10x3.png` via
      `scripts/gen_hourglass_sprite.py` (white sprite, 30 frames @ 64×64,
      curved silhouette, supersampled AA, sand drains then flips to loop)
- [ ] Save `assets/graphics/submit/LoadingSpinner_10x3.png` from ITGmania
      Simply Love (MIT-licensed). Note source in commit message.
- [ ] Confirm both sprites are loaded into the texture atlas at startup
      (check `src/assets/textures.rs` for the load path; submit/ subdir may
      need to be added to whatever sprite-list discovery mechanism exists).

### Phase 2 — i18n strings (`assets/languages/en.ini`)
- [ ] `TimedOut=Timeout` (currently `Timed Out`)
- [ ] `NetworkError=Network` (currently `Network Error`)
- [ ] `ServerError=Server {code}` (currently `Server Error ({code})` —
      uncommitted edit already adds parens; this further shortens)
- [ ] Drop now-unused keys: `SubmittedCombined`, `Submitting`, `Retrying`,
      `RetryingIn`, `RetryableIn`, `F5Retry`, `TimedOutRetry`
- [ ] Keep: `Rejected`, `BSLabel`, `GSLabel`, `ACLabel`, `Submitted` (may
      reuse for `✔` cell tooltip later, otherwise drop)
- [ ] Add: `F5Hint=F5`  (or just hard-code "F5" — tiny win)
- [ ] Audit non-English `*.ini` files for the same keys; remove or update
      to keep parity (deferred if non-English support is not maintained).

### Phase 3 — `src/screens/evaluation.rs` core helpers
- [ ] Define `SubmitFooterCell` enum or struct returned by a new
      `submit_footer_cell(backend_label: &str, status: SubmitFooterStatus)`
      helper. It needs to express text fragments + an animated-sprite
      slot (Spinner or Hourglass) so the render path can emit a sprite
      actor between text actors.
      Suggested shape:
      ```rust
      enum CellIcon { Spinner, Hourglass, Static(&'static str) }
      struct CellSpec {
          backend_label: Arc<str>,
          icon: CellIcon,
          countdown_secs: Option<u32>,
          reason: Option<Arc<str>>,
      }
      ```
- [ ] Rewrite `submit_footer_lines(...)` → return `Vec<CellSpec>` of length
      0, 1, or 2 (one cell per expected backend). The "single line"
      assumption now lives in the renderer.
- [ ] Delete dead helpers:
      - `submit_footer_status_text` (line 197)
      - `submit_footer_retry_suffix` (line 188)
      - `submit_footer_status_glyph` (line 318)
      - `combined_submit_footer_text` (line 338)
      - `submit_footer_service_line` (line 355)
      - `SUBMIT_STATUS_CHECK_GLYPH` / `SUBMIT_STATUS_CROSS_GLYPH` constants
        (lines 67–68) if unused after rewrite

### Phase 4 — Render path (`evaluation.rs:3832-3863`)
- [ ] Replace the `for (idx, status_text) in lines.iter().enumerate()` loop
      with a per-cell composer that:
      1. Computes total line width by summing text-fragment widths and
         sprite slot width (sprite = same height as text caps).
      2. Emits actors left-to-right, x-advancing per fragment, centered
         around the side's anchor x.
      3. For Spinner/Hourglass cells, emits a `sprite()` actor with
         `setstate((screen.elapsed_secs * 30.0) as u32 % 30)` and matching
         dimensions.
- [ ] Use the existing `font("miso")` zoom 0.8 for text. Pick sprite
      pixel size to roughly match cap-height (likely ~12–14px tall).
- [ ] Confirm `screen.elapsed_secs` (or equivalent driver-time field) is
      reachable in this scope; otherwise plumb it from `state`.

### Phase 5 — Tests (`evaluation.rs` test module)
- [ ] Inventory the existing 23 footer tests (`submit_footer_lines` calls
      at lines 791, 813, 831, 849, 875, 893, 911, 929, 949, 965, 983,
      1001, 1023, 1041, plus assertions in earlier blocks).
- [ ] Rewrite each to assert against the new `Vec<CellSpec>` shape rather
      than `Vec<Arc<str>>`. Specifically test:
      - Empty when neither backend expected
      - Single-backend Submitting → one cell, icon=Spinner
      - Single-backend Submitted → one cell, icon=Static("✔")
      - Single-backend TimedOut auto pending → one cell, icon=Hourglass,
        countdown=Some(n), reason=Some("Timeout")
      - Single-backend TimedOut budget exhausted → icon=Static("↻ F5"),
        countdown=None, reason=Some("Timeout")
      - Single-backend NetworkError waiting → icon=Hourglass, reason=Network
      - Single-backend ServerError waiting → reason=Server {code}
      - Single-backend Rejected → icon=Static("⊘"), reason=label
      - Both expected, both Submitting → two cells
      - Both expected, mixed states → two cells, independent icons
      - Both expected, both Submitted → two cells, both ✔
- [ ] Verify the `combined_submit_footer_text` test at line 775 is
      removed/replaced.

### Phase 6 — Validation
- [ ] `cargo test -p deadsync screens::evaluation` (or whatever the crate
      filter is) — all evaluation tests pass
- [ ] `cargo test -p deadsync game::scores` — submit/retry tests still pass
      (they're untouched but worth a regression check)
- [ ] `cargo build --release` — no warnings introduced
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] Manual smoke test: launch deadsync, complete a song, confirm footer
      renders for: success path, simulated timeout (deferred — needs the
      removed-then-restored `DEADSYNC_FAKE_SUBMIT_FAIL` env hook OR live
      against real backends).

### Phase 7 — Commits & PR

Break the work into small, reviewable commits. Each commit must compile,
pass tests, and represent one logical change. Suggested sequence:

1. **`docs(evaluation): add submit footer redesign plan`**
   Adds `docs/submit-footer-redesign.md`. No code changes. Lands first so
   the PR has design context for reviewers.

2. **`chore(scripts): add hourglass sprite generator`**
   Adds `scripts/gen_hourglass_sprite.py`. No runtime impact.

3. **`feat(assets): add submit status sprites`**
   Adds `assets/graphics/submit/Hourglass_10x3.png` and
   `assets/graphics/submit/LoadingSpinner_10x3.png`. Note ITGmania Simply
   Love (MIT) attribution for spinner in commit message. No code wiring
   yet — but if atlas registration is required, include the minimal change
   here.

4. **`i18n(evaluation): shorten submit status reason labels`**
   `assets/languages/en.ini` only: `TimedOut=Timeout`,
   `NetworkError=Network`, `ServerError=Server {code}`. Tests still pass
   because the existing tests assert on the localized strings as composed,
   so update the relevant assertions in the same commit.

5. **`refactor(evaluation): introduce CellSpec/CellIcon footer model`**
   Adds the new `CellSpec` struct + `CellIcon` enum and the
   `submit_footer_cell` builder. New `submit_footer_lines` returns
   `Vec<CellSpec>`. Render path adapted to consume cells but still emits
   only text actors (sprites come next). All tests rewritten to assert
   against cells. Old helpers deleted in this commit.

6. **`feat(evaluation): animated icons in submit footer`**
   Render path emits sprite actors for `Spinner`/`Hourglass` cells with
   `setstate(frame)` driven by `screen.elapsed_secs`. Vertical alignment
   tuned. No test changes (logic already validated in commit 5).

7. **`chore(i18n): drop unused submit status keys`**
   Removes now-orphan keys (`SubmittedCombined`, `Submitting`,
   `Retrying*`, `RetryableIn`, `F5Retry`, `TimedOutRetry`).

8. **`docs(evaluation): update PR description for new footer`**
   Updates `pr-description.md`.

After all commits land locally:
- `cargo test` clean at every commit (use `git rebase -i --exec 'cargo test'` to verify)
- `cargo clippy --all-targets -- -D warnings` clean at HEAD
- `git push --force-with-lease origin adstep/main/classify-submit-failures`

### Commit hygiene rules
- Each commit must build and test cleanly in isolation.
- No mixed concerns: assets, i18n, refactor, and feature work go in
  separate commits.
- Commit messages follow Conventional Commits + scope (`feat(evaluation):`,
  `i18n(evaluation):`, etc.).
- No `Co-authored-by: Copilot` trailer per project convention.

---

## Deferred / out of scope
- **Color**: White-only this pass. Future pass can tint icons by status
  (success=green, waiting=yellow, F5=cyan, rejected=red) per Option A in
  the color discussion.
- **Non-English translations**: Update only if those locales are actively
  maintained.
- **Spinner reuse elsewhere**: The animated sprite infrastructure may be
  useful in other screens (e.g., music download), but that's a separate
  refactor.

---

## Open items / risks
- **Sprite vertical alignment**: text uses zoom 0.8 of the miso font;
  sprites need dimensions chosen so they sit on the same baseline.
  Will likely need a small visual tweak loop after first wire-up.
- **Texture atlas registration**: deadsync may auto-discover sprites from
  certain directories or require explicit registration. Need to verify
  during Phase 1.
- **Cell-width measurement for centering**: text width measurement helper
  must exist (it does for other multi-actor lines like the records
  banner); confirm it's reachable from this render path.
