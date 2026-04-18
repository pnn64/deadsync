# Localization

DeadSync supports multiple languages through INI-based translation files and a runtime string lookup system.

## How It Works

All user-facing strings are stored in language files under `assets/languages/` using ITGmania-compatible INI format. At startup, the `i18n` module loads the English baseline (`en.ini`) plus the user's selected language file (if different). Screens retrieve strings at render time via `tr("Section", "Key")`, which returns an `Arc<str>`.

### Fallback chain

1. Look up the key in the **active language** file
2. Fall back to the **English** value (`en.ini`)
3. If missing from English too, return `"Section.Key"` — this makes untranslated strings visible during development

### Language selection

The `language` config field in `deadsync.ini` controls which language is used:

- `auto` (default) — detects the OS language and picks the best available match
- Any locale code (e.g. `en`, `sv`, `ja`) — uses that language directly

Users can change the language at runtime from the System Options screen. The change takes effect immediately — no restart required.

## File Format

Language files live in `assets/languages/{locale}.ini`. Each file uses INI sections that map to screens or logical groupings, with `PascalCase` keys:

```ini
[Meta]
NativeName=English

[Menu]
Gameplay=GAMEPLAY
Options=OPTIONS
Exit=EXIT

[Common]
Yes=Yes
No=No
Back=Back
```

The `[Meta]` section is required and must contain a `NativeName` key — this is the language name shown in the language picker, written in the language itself (e.g. `日本語`, `Svenska`, `Español`).

### Format strings

Some strings contain `{placeholder}` tokens for dynamic values:

```ini
SongSummary={songs} songs in {packs} groups, {courses} courses
VersionLine=DeadSync {version}
```

These are filled at runtime via `tr_fmt("Section", "Key", &[("placeholder", value)])`. Translators can reorder placeholders freely to match the target language's word order.

## Adding a New Language

1. Copy `assets/languages/en.ini` to `assets/languages/{locale}.ini` (e.g. `es.ini`, `ja.ini`, `zh-Hant.ini`)
2. Set `NativeName` in the `[Meta]` section to the language's own name (e.g. `Español`, `日本語`)
3. Translate the values (right side of `=`). Do not change the keys (left side) or section names
4. DeadSync will automatically detect the new file and show it in the language picker

### Locale codes

Use [IETF BCP 47](https://en.wikipedia.org/wiki/IETF_language_tag) base codes for file names:

| Code | Language |
|------|----------|
| `en` | English |
| `es` | Spanish |
| `fr` | French |
| `de` | German |
| `ja` | Japanese |
| `ko` | Korean |
| `sv` | Swedish |
| `zh-Hans` | Chinese (Simplified) |
| `zh-Hant` | Chinese (Traditional) |

### Partial translations

You don't have to translate every key. Any missing key falls back to the English value automatically. Start with the most visible sections (`Common`, `Menu`, `ScreenTitles`) and expand from there.

### Skipping keys (`@skip`)

Some keys don't need translating for a given language — brand names like "GrooveStats", technical terms, or values that are identical in English. Set these to `@skip`:

```ini
[Menu]
GrooveStatsName=@skip
BoogieStatsName=@skip
Gameplay=SPELA
```

At runtime, `@skip` values are treated as missing, so the English fallback is used. In coverage reports, `@skip` keys count as complete — a language that translates everything applicable and skips the rest can reach 100%.

This is per-language: Swedish might skip "GrooveStats" while Japanese might transliterate it to katakana.

### User overrides

Language files are resolved from the executable's `assets/languages/` directory. Users can place custom or modified `.ini` files there to override bundled translations.

## Contributing a Translation

### Getting started

1. Fork the repository and create a branch (e.g. `i18n-add-french`)
2. Copy `assets/languages/en.ini` to `assets/languages/{locale}.ini` (see [Locale codes](#locale-codes) for naming)
3. Set `NativeName` in the `[Meta]` section to the language's own name (e.g. `Español`, `日本語`)
4. Translate values (right side of `=`). Do not change keys (left side) or section names
5. For keys that should stay in English (brand names, technical terms), set the value to `@skip` — see [Skipping keys](#skipping-keys-skip)

### Testing locally

```sh
# Check coverage and see which keys are missing
cargo run --bin lang_coverage

# Run the CI validation tests
cargo test --test i18n_coverage

# Launch the game with your language to verify visually
# (set language = {locale} in deadsync.ini, or let auto-detection pick it up)
```

### Submitting

Open a PR. The CI will run the coverage tests to catch stale keys or duplicate entries. Partial translations are welcome — translate what you can and leave the rest. The app falls back to English for any missing key.

### Tips

- Start with the most visible sections: `[Common]`, `[Menu]`, `[ScreenTitles]`, `[SelectMusic]`
- Format strings like `{songs} songs in {packs} groups` — translate the words but keep the `{placeholder}` names unchanged. You can reorder placeholders to match your language's word order
- Test with longer strings — some languages (e.g. German) produce significantly longer text than English. Check that UI elements don't overflow
- If you're unsure about a game-specific term, check [ITGmania's translations](https://github.com/itgmania/itgmania/tree/release/Themes/_fallback/Languages) for reference — both projects use GPL-3.0

## For Developers

### Using `tr()` in code

```rust
use crate::assets::i18n::tr;

// Simple lookup — returns Arc<str>
let label = tr("Menu", "Gameplay");

// With placeholders
use crate::assets::i18n::tr_fmt;
let summary = tr_fmt("Menu", "SongSummary", &[
    ("songs", &song_count.to_string()),
    ("packs", &pack_count.to_string()),
    ("courses", &course_count.to_string()),
]);
```

### Key conventions

- **Section names** map to screens or logical groupings, using `PascalCase`
- **Key names** are `PascalCase` and should be descriptive (e.g. `MusicWheelSpeed`, not `Speed`)
- **Values** are the user-visible strings, including any emoji or special characters
- Strings that appear in multiple screens belong in `[Common]`
- Option-screen sections are prefixed with `Options` (e.g. `OptionsGraphics`, `OptionsSound`)

### Adding new strings

1. Add the key and English value to `assets/languages/en.ini` in the appropriate section
2. Use `tr("Section", "Key")` or `tr_fmt(...)` at the call site
3. Existing translations will fall back to the English value until translators add the new key

## Extraction Progress

String extraction is being done screen-by-screen. Each screen group gets its own commit that replaces hardcoded strings with `tr()` calls and adds the corresponding keys to `en.ini`.

| Screen | Source File(s) | Status |
|---|---|---|
| Main Menu | `menu.rs` | ✅ Done |
| Options | `options.rs`, `player_options.rs` | ✅ Done |
| Select Music | `select_music.rs` | ✅ Done |
| Select Mode | `select_mode.rs` | ✅ Done |
| Select Style | `select_style.rs` | ✅ Done |
| Select Course | `select_course.rs` | ✅ Done |
| Evaluation | `evaluation.rs` | ✅ Done |
| Evaluation Summary | `evaluation_summary.rs` | ✅ Done |
| Gameplay | `gameplay.rs` | ✅ Done |
| Game Over | `gameover.rs` | ✅ Done |
| Manage Local Profiles | `manage_local_profiles.rs` | ✅ Done |
| Select Profile | `select_profile.rs` | ➖ N/A (no user-facing strings) |
| Select Color | `select_color.rs` | ✅ Done |
| Initials | `initials.rs` | ✅ Done |
| Profile Load | `profile_load.rs` | ✅ Done |
| Pack Sync | `pack_sync.rs` | ✅ Done |
| Init / Splash | `init.rs` | ✅ Done |
| Credits | `credits.rs` | ✅ Done (proper names and license text intentionally untranslated) |
| Favorite Code | `favorite_code.rs` | ➖ N/A (no user-facing strings) |

**Remaining milestones:**

| Milestone | Status |
|---|---|
| Language selection in options screen | ✅ Done |
| Second language proof (Swedish) | ✅ Done |
| Translation coverage tooling | ✅ Done |

## Translation Coverage Tooling

An integration test suite validates language files and reports translation progress.

### Running

```sh
cargo test --test i18n_coverage -- --nocapture
```

### Tests

| Test | Behavior |
|------|----------|
| `en_ini_has_no_duplicate_keys` | **Fails** if `en.ini` contains duplicate `[Section] Key` pairs |
| `no_stale_keys_in_translations` | **Fails** if any translation has keys that don't exist in `en.ini` (renamed/removed keys that were never cleaned up) |
| `print_translation_coverage_report` | **Always passes** — prints a per-language coverage summary showing translated count, total, and percentage. `@skip` keys count as covered |

### Example output

```
Translation Coverage Report
============================
Language         Translated  Total  Coverage
--------         ----------  -----  --------
en (English)            590    590    100.0%
sv (Svenska)             42    590      7.1%
    548 missing keys (run with --nocapture to see full list)
```

### What the tests catch

- **Duplicate keys in `en.ini`** — a real bug since the second value silently overwrites the first
- **Stale keys in translations** — a key was renamed or removed in `en.ini` but the old key still exists in a translation file. The translated string would never be displayed
- **Coverage regressions** — the report makes it easy to spot if a language's coverage drops after a key restructuring

### What the tests don't enforce

- **Missing keys don't fail the build.** Partial translations are welcome — the app falls back to English for any missing key. Contributors can translate progressively and open PRs at any coverage level

## Language Support

The goal is to support all languages shipped by ITGmania, plus community contributions. Translation progress is measured as a percentage of keys translated relative to `en.ini`.

| Language | Code | Native Name | File | Progress |
|---|---|---|---|---|
| English | `en` | English | `en.ini` | ✅ 100%* |
| Spanish | `es` | Español | — | 0% |
| French | `fr` | Français | — | 0% |
| German | `de` | Deutsch | — | 0% |
| Japanese | `ja` | 日本語 | — | 0% |
| Korean | `ko` | 한국어 | — | 0% |
| Dutch | `nl` | Nederlands | — | 0% |
| Polish | `pl` | Polski | — | 0% |
| Slovak | `sk` | Slovenčina | — | 0% |
| Chinese (Traditional) | `zh-Hant` | 繁體中文 | — | 0% |

\* English is the baseline language. New keys are still being extracted from the codebase — see [Extraction Progress](#extraction-progress) for current status.

## Community Translations via Weblate

Once string extraction is complete, the plan is to set up [Weblate](https://weblate.org/) for community-driven translations. Weblate provides a web-based translation interface so contributors can translate without needing Git knowledge.

### Why Weblate

- Open-source (GPLv3+), free hosted tier for open-source projects on [hosted.weblate.org](https://hosted.weblate.org/)
- Native INI file format support — no conversion needed
- Direct GitHub integration — auto-commits translations or opens PRs
- Per-language completion dashboards, embeddable badges, and email notifications
- Translation memory and optional machine translation suggestions
- Used by LibreOffice, Fedora, openSUSE, and Godot Engine

### Planned configuration

| Setting | Value |
|---|---|
| File format | INI File |
| File mask | `assets/languages/*.ini` |
| Base language file | `assets/languages/en.ini` |
| Source language | English |
| Repository | `https://github.com/pnn64/deadsync` |
| Push branch | `weblate-translations` |

Weblate will push translations to a dedicated branch, which maintainers merge via PR.

### Translator workflow

1. Visit the DeadSync project on hosted.weblate.org
2. Pick a language (or request a new one)
3. Browse untranslated strings with English source text and context
4. Type translations — the progress bar updates live
5. A reviewer approves the translation
6. Weblate commits approved translations to the `weblate-translations` branch
7. A maintainer merges the branch into `main`

### Coexistence with direct PRs

Both contribution paths will coexist:

- **Weblate** for web-based contributors who prefer a translation UI
- **Direct PRs** for contributors who prefer editing `.ini` files in a text editor
- The CI coverage test validates both paths
- Weblate's lock feature prevents conflicts when both paths modify the same file

### Status (not yet set up)

Weblate integration is planned but not yet configured. It depends on completing string extraction and shipping at least one non-English translation first. See [Extraction Progress](#extraction-progress) for current status.
