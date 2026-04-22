# Theme Font (Mega) — Step-by-Step Plan

Tracks the work for adding Simply Love's `ThemeFont` operator preference
to deadsync. Mirrors the pattern we used for `VisualStyle`
(`docs/visual-styles-plan.md`).

## Goal

Add a machine-wide **Theme Font** operator preference matching SL's
`ThemeFont` (`Themes/Simply Love/Scripts/99 SL-ThemePrefs.lua:370`).

- Choices: `Common` (default), `Mega`. Defer `Unprofessional` to a
  follow-up.
- Persists to `[Theme] ThemeFont` in `Save/<machine>.ini`.
- When `Mega` is active, the **Bold / Header / Footer / numbers /
  ScreenEval** font roles swap from Wendy to Mega. The **Normal**
  body-text role stays Miso (matches SL — see "Mega Normal.redir →
  Miso/_miso light").

## Out of scope

- `Unprofessional` theme font — needs a separate `_miso unprof` font
  variant; track as follow-up.
- Per-screen overrides where Wendy is used for *gameplay* (notefield
  combo, judgment numbers, hold judgments, etc.) — those stay on the
  player's `ComboFont` pref, not the theme font. Only static UI text
  is swapped.

## Reference: how SL maps roles → font files

From `Simply-Love-SM5/Fonts/*.redir`:

| Role           | Common (default) target              | Mega target                            |
| -------------- | ------------------------------------ | -------------------------------------- |
| `Normal`       | `Miso/_miso light`                   | **`Miso/_miso light`** (unchanged)     |
| `Bold`         | `Wendy/_wendy small`                 | `Mega/_mega font`                      |
| `Header`       | `Wendy/_wendy small`                 | `Mega/_mega font`                      |
| `Footer`       | `Wendy/_wendy small`                 | `Mega/_mega font`                      |
| `numbers`      | `Wendy/_wendy monospace numbers`     | `Mega/_mega monospace numbers`         |
| `ScreenEval`   | `Wendy/_ScreenEvaluation numbers`    | `Mega/_ScreenEvaluation numbers`       |

In SL, every static UI label calls
`LoadFont(ThemePrefs.Get("ThemeFont") .. " <Role>")` — the redir files
do the variant lookup. We'll do the same with a Rust resolver.

---

## Step 1 — Branch ✅

- [x] Created `adstep/main/theme-font` from `83363ddb`.

---

## Step 2 — Import Mega theme-font assets ✅

Imported into `assets/fonts/Mega/` (committed `7be1...` "feat(theme-font):
import Simply Love Mega font assets"):

- [x] `_mega font.ini` + `_mega font 13x7 (res 520x560).png`
- [x] `_mega monospace numbers.ini` + paired PNG
- [x] `_ScreenEvaluation numbers.ini` + paired PNG
- [x] `_game chars 36px 4x1.ini` + paired PNG (required by
      `_mega font.ini`'s `import=Mega/_game chars 36px[nameentry]`)
- [ ] No SL `attribution-Mega.txt` exists; nothing to copy.

---

## Step 3 — Add `ThemeFont` config ✅

Committed `1d7a4569` "feat(theme-font): add ThemeFont machine pref":

- [x] `ThemeFont { Common, Mega }` in `src/config/theme.rs` with
      `Default = Common`, `as_str()`, `FromStr` (case-insensitive,
      `"Wendy"` accepted as alias for `Common` to match SL's UI label).
- [x] `THEME_FONT_VARIANTS` const + re-export from `src/config/mod.rs`.
- [x] `theme_font: ThemeFont` field on `Config`, defaulted `Common`.
- [x] Persisted as `[Theme] ThemeFont = Common|Mega` in `Save/<machine>.ini`
      via `store/save.rs`, default written by `store/defaults.rs`,
      backfilled on upgrade via `load/backfill.rs`.
- [x] 5 unit tests (default, round-trip, case-insensitivity + Wendy
      alias, rejects unknowns, variants table is exhaustive).

---

## Step 4 — Add the font role resolver ✅

Committed `737eb254` "feat(theme-font): register Mega fonts and add role
resolver" together with Step 5:

- [x] `FontRole { Normal, Bold, Header, Footer, Numbers, ScreenEval }`
      + `theme_font_key(theme_font, role) -> &'static str` in
      `src/assets/mod.rs`.
- [x] Convenience `current_theme_font_key(role)` reads global config.
- [x] Doc comment explicitly tells gameplay code (combo / judgment /
      hold judgment) **not** to use this — those follow `ComboFont`.
- [x] 3-test matrix covering Normal-always-Miso + Common→Wendy +
      Mega→Mega for all 6 roles.

---

## Step 5 — Register the new Mega font keys ✅

(Same commit as Step 4.)

- [x] Registered four new keys in `load_initial_fonts`:
  - `mega_alpha` → `assets/fonts/Mega/_mega font.ini`
  - `mega_monospace_numbers` → `_mega monospace numbers.ini`
  - `mega_screenevaluation` → `_ScreenEvaluation numbers.ini`
  - `mega_game` → `_game chars 36px 4x1.ini` (referenced by
    `_mega font.ini`'s `import=Mega/_game chars 36px[nameentry]`)
- [x] `mega_alpha` fallback chain → `miso` (handles lowercase / non-ASCII
      gracefully since Mega is uppercase-only).

---

## Step 6 — Audit and route call sites

This is the biggest mechanical sweep. ~390 hardcoded font calls
across ~50 files; only the *static UI text* ones need rerouting.

Categorize each `font("wendy" | "_wendy small" | "_wendy monospace
numbers" | "_ScreenEvaluation")` call by SL role:

- [ ] **Header** — large screen titles, screen-bar headers
      (`screen_bar.rs`, top of `select_music`, `evaluation`, etc.).
- [ ] **Footer** — bottom-of-screen actions (`submit_footer`,
      `init.rs` continue prompts).
- [ ] **Bold** — emphasised labels in menus, options.
- [ ] **Numbers** — all numeric stats text where SL would use Wendy
      monospace numbers (percentages, score counters, BPM display).
- [ ] **ScreenEval** — evaluation panel numeric (`pane_percentage`,
      `pane_stats`, `pane_machine_records`, `pane_gs_records`).
- [ ] **Skip (stays hardcoded):** notefield combo numbers, judgment
      sprites, hold-judgment text, gameplay HUD digits — gameplay-side
      and governed by `ComboFont` already.

For each rerouted site, replace `font("wendy")` with
`font(theme_font_key(FontRole::Header))` (etc.). Prefer adding a
small helper macro/closure in each file rather than scattering raw
`theme_font_key()` calls.

Suggested ordering for incremental commits:

1. Step 6a — `screen_bar.rs` + screen titles (`Header`). ✅ `1f7e61b8`
2. Step 6b — `init.rs` splash title + Evaluation banners (`Header`). ✅ `91b534b5`
3. Step 6c — Evaluation panes (`Header` for `wendy_white`, `ScreenEval` for `wendy_screenevaluation`). ✅ `91b534b5`
4. Step 6d — `options.rs`, `manage_local_profiles.rs`, `mappings.rs`
   (`Bold`).
5. Step 6e — `select_music`, `select_course`, `select_mode`
   (`Header` + `Footer` + `Bold`).
6. Step 6f — Remaining stragglers found by grep.

After each sub-step: build clean, run focused tests (the affected
screens), commit.

---

## Step 7 — Operator UI entry  ✅ `dae6176c`

In `src/screens/options.rs`:

- [ ] Add a `ThemeFont` row under the `OptionsMachine` page,
      patterned on the `VisualStyle` row added in
      `docs/visual-styles-plan.md` Step 9.
- [ ] Cycle through `THEME_FONT_VARIANTS` on left/right.
- [ ] On change, write to config, save the ini, trigger font hot
      reload (see Step 8).
- [ ] Add help text via `OptionsMachineHelp.ThemeFont`.

---

## Step 8 — Hot reload on `ThemeFont` change

- [ ] On config change, re-run `load_initial_fonts` for the affected
      keys (or all). This is parallel to how the VisualStyle hot
      reload works for textures — confirm the existing font
      re-bind path supports this. If not, the operator can restart;
      worst case is unaffected gameplay until next screen
      transition.
- [ ] Verify already-rendered text retextures correctly on the next
      frame after the swap.

---

## Step 9 — i18n

Add to **every** language file under `[OptionsMachine]` and
`[OptionsMachineHelp]` that already defines `VisualStyle` keys
(en, sv, pseudo, plus the others if they're already up to date):

- [ ] `[OptionsMachine] ThemeFont = …`
- [ ] `[OptionsMachine] ThemeFontCommon = Common` (or "Wendy" — match
      SL's display label, which is "Wendy")
- [ ] `[OptionsMachine] ThemeFontMega = Mega`
- [ ] `[OptionsMachineHelp] ThemeFont = Choose the font used for screen
      titles, headers, footers, and numeric stats.`

Run `scripts/generate_pseudo.rs` to refresh `pseudo.ini`.

---

## Step 10 — Tests

- [ ] Unit tests in `src/config/theme.rs` mirroring the
      `VISUAL_STYLE_VARIANTS` tests:
  - `ThemeFont` round-trips through `FromStr`/`Display`.
  - Default is `Common`.
  - Reading missing key from ini → `Common`.
  - Reading explicit `[Theme] ThemeFont = Mega` → `Mega`.
- [ ] Resolver tests for `theme_font_key`:
  - `(Common, Normal)` and `(Mega, Normal)` both → `"miso"`.
  - `(Common, Bold)` → `"wendy"`, `(Mega, Bold)` → `"mega_alpha"`.
  - Each of the 6 roles tested for both variants.
- [ ] Compose-scenario coverage: extend
      `src/test_support/compose_scenarios.rs` to register the new
      Mega font dims so screens that go through it can be rendered
      headlessly without panics.

---

## Step 11 — Build / smoke / PR

- [ ] `cargo build --bin deadsync` clean.
- [ ] `cargo test --lib` — pre-existing flakes
      (`versus_exit_*`, `song_lua_quad_keeps_zoomed_size_in_scale`,
      `set_keymap_prepares_dense_debounce_slots`) ignored;
      everything else passes.
- [ ] Manual smoke each variant:
  - `Common` (default) — looks identical to current main.
  - `Mega` — screen titles, footers, numeric stats render in Mega's
    blocky monospace; body text (option labels, descriptions) still
    Miso.
- [ ] Push branch, open PR. Body should:
  - Link SL's `ThemeFont` script + the .redir table.
  - Note `Unprofessional` is deferred (asset + `_miso unprof`
    needed).
  - Note gameplay-side fonts (combo, judgment) are unaffected — those
    are still controlled by per-player `ComboFont`.

---

## Open questions / risks

- **Glyph coverage:** `_mega font` is uppercase-Latin + digits + some
  punctuation, no lowercase or non-ASCII. Confirm which call sites
  display lowercase or CJK — those should *probably* still go through
  Miso even when ThemeFont = Mega. SL gets away with this because
  most of its big-text actors uppercase first; deadsync may not.
  Mitigation: configure Mega's fallback chain → `miso` → `game` →
  `cjk` so missing glyphs degrade gracefully.
- **Hot reload scope:** changing fonts mid-screen may require
  re-laying out cached text geometry. Worst case, document
  "ThemeFont changes apply on screen change".
- **Audit accuracy:** the call-site categorization (Header vs Bold vs
  Footer) is judgment-heavy. Recommend a rubber-duck pass on
  Step 6's mapping before mass-editing.
