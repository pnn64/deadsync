# Migrating from ITGmania to DeadSync

DeadSync can **automatically import** an ITGmania + Simply Love local profile —
including your player options and offline scores — from inside the game. This is
the recommended path; see [Automatic import](#automatic-import-recommended)
below.

If you'd rather move individual pieces by hand (or the automatic importer can't
find your ITGmania install), the rest of this guide explains what carries over
manually, what doesn't, and exactly which files to copy or edit.

Throughout this doc:

- `<data dir>` is DeadSync's per-user data directory. Its location depends on
  your OS — see [Data Directories](../README.md#data-directories) in the README
  for the exact path. You can also jump there in-game via
  **Options → Folders → Data Directory → Open**.
- `<itg profile>` is your ITGmania profile folder
  (`<itg save dir>/LocalProfiles/<id>/`).
- `<your folder>` is the name of your DeadSync profile folder. DeadSync names it
  after your display name (e.g. `Alice`), so it can change when you rename the
  profile. The folder name is *not* the profile's identity — see below.

> **Identity lives inside the profile.** Each DeadSync profile carries its own
> canonical id, a randomly generated GUID stored as `Guid=` under
> `[userprofile]` in `profile.ini`.
> DeadSync finds profiles by that embedded GUID, so you are free to rename a
> profile folder on disk without breaking scores, settings, or online logins.
> Existing profiles created before this scheme are assigned a GUID automatically
> the first time DeadSync loads them.

> **Heads up.** DeadSync's **Options → Online Scoring → Score Import** screen
> only downloads past scores from accounts that are *already* configured (i.e.
> you've put a GrooveStats / ArrowCloud API key in place). It is not a generic
> "import my old data" button, and it has no text field for entering keys.

## Where the files live

| Game     | Profile root                                |
| -------- | ------------------------------------------- |
| ITGmania | `<itg save dir>/LocalProfiles/<id>/`        |
| DeadSync | `<data dir>/save/profiles/<your folder>/`   |

You can jump straight to DeadSync's profile root from inside the game via
**Options → Folders → Profiles → Open**.

## Automatic import (recommended)

DeadSync can read an ITGmania `LocalProfiles/<id>/` folder and create a brand
new DeadSync profile from it in one step.

1. In DeadSync, go to **Options → Manage Local Profiles**.
2. Select **Import from ITGmania** and press **Start**.
3. DeadSync scans for ITGmania profiles and lists the ones it finds. Pick the
   profile you want and press **Start**. If yours isn't listed (e.g. a portable
   install), choose **Browse for game directory…** and select your ITGmania game
   folder — any profiles found there are added to the list.
4. The import runs in the background; when it finishes, a summary shows how many
   scores were imported and how many were skipped. The new profile is selected
   in the list.

### What it brings across

- **Profile** — display name, weight, birth year, player initials, and your
  avatar (`Avatar.png`).
- **Online keys** — GrooveStats (API key, username, pad-player flag) and
  ArrowCloud, written into the new profile's `groovestats.ini` /
  `arrowcloud.ini`. Your online history can then be pulled via
  **Options → Online Scoring → Score Import**.
- **Player options** — your Simply Love per-profile mods from
  `Simply Love UserPrefs.ini`: scroll speed and type, mini, spacing, noteskin,
  judgment / held / hold-judgment graphics, combo font, note-field offsets,
  visual delay, tilt, life-meter type, measure counter and lines, combo
  mode/colors, mini-indicator, error-bar style and trim, column-flash, data-
  visualization mode, and the many on/off display toggles. Settings DeadSync
  doesn't recognise — including custom theme graphics/fonts it doesn't ship —
  are left at their default rather than guessed.
- **Offline scores** — every high score in `Stats.xml` becomes a DeadSync local
  play, matched to the chart in your library.
- **Favorites** — your favorited songs from `favorites.txt` (those present in
  your DeadSync library). Custom favorites-section names aren't carried over.
- **ITL event progress** — your `ITL2026.json` (ITL Online scores, points, and
  unlocked folders), which DeadSync reads in the same format.

### Where it looks

The importer auto-detects ITGmania's per-user save directory:

| OS      | Scanned location                                              |
| ------- | ------------------------------------------------------------ |
| Windows | `%APPDATA%\ITGmania\Save\LocalProfiles\`                      |
| Linux   | `~/.itgmania/Save/LocalProfiles/`                            |
| macOS   | `~/Library/Application Support/ITGmania/Save/LocalProfiles/`  |

If your ITGmania uses a **portable install** (a `Portable.ini` next to the
executable, so its `Save/` lives in the game folder), nothing is auto-detected.
Pick **Browse for game directory…** in the import list and select your ITGmania
game folder — DeadSync applies ITGmania's own rule (portable → `<game>/Save`,
otherwise the per-user dir) to find the profiles, then adds them to the list.

### Limitations

- **Scores only attach to charts that exist in DeadSync's library.** ITGmania's
  `Stats.xml` keys a chart by its song folder + steps type + difficulty, not by
  hash, so DeadSync looks the chart up in your scanned songs to recover the
  GrooveStats hash. Scan your packs *before* importing, and add your ITGmania
  `Songs` folder (see [Songs folder](#7-songs-folder)) so more scores match.
  Charts that aren't found are counted as skipped in the summary, not imported.
- **EX / Hard-EX scores can't be recovered.** ITGmania doesn't store the FA+
  (W0) split, so imported plays show the ITG percent and grade but start with an
  EX score of 0.
- **Holds and rolls aren't distinguished** in `Stats.xml`; all hold-type
  judgments are folded together, and mine tallies are partial.

Rate-modded plays keep their music rate (DeadSync reads the `xMusic` modifier
from each score), and your judgment counts, grade, combo lamp, and play date all
carry across.

For the exact field-by-field mapping (every Simply Love setting and `Stats.xml`
element and where it lands in DeadSync), see
[itgmania-import-mapping.md](./itgmania-import-mapping.md).

## Manual migration

If you prefer to move pieces by hand, the sections below cover each item
individually.

### 1. Create a fresh DeadSync profile

In DeadSync: **Options → Manage Local Profiles → Create**. From the same
screen you can also **Rename** or **Delete** profiles.

### 2. Set your basic info

Some fields are editable in-game, some live only in the config file:

**In the UI**
- Display name — **Manage Local Profiles → Rename**.
- Player initials — the initials / name-entry screen after a high score.
- Scroll speed, noteskin, mods, perspective, mini, etc. — **Player Options**
  before a song.

**Config file only**
- Weight (pounds) and birth year. There is no in-game screen for these.
  With the game closed, edit
  `<data dir>/save/profiles/<your folder>/profile.ini` and adjust the values
  under `[userprofile]`:

  ```ini
  [userprofile]
  WeightPounds = 150
  BirthYear    = 1990
  ```

  Save and relaunch. (Note: the key is `WeightPounds`, not `Weight`. These
  fields are stored but not currently used by gameplay.)

### 3. Avatar

There is no avatar picker in the DeadSync UI. Drop an image into your profile
folder and DeadSync will pick it up automatically on launch.

DeadSync prefers `profile.png`; if that is missing it falls back to
`avatar.png` (matching is case-insensitive, so ITGmania's `Avatar.png` works
as-is). To bring across your ITGmania avatar, copy `<itg profile>/Avatar.png`
into `<data dir>/save/profiles/<your folder>/` and (optionally) rename it to
`profile.png`.

### 4. GrooveStats API key

The key does **not** go in `profile.ini`. It lives in its own file inside the
profile folder:

- **Path:** `<data dir>/save/profiles/<your folder>/groovestats.ini`
- **Contents:**

  ```ini
  [GrooveStats]
  ApiKey      = <your 64-character key>
  IsPadPlayer = 1
  Username    = <your GrooveStats username>
  ```

Use `IsPadPlayer = 1` for pad, `0` for keyboard.

You have two ways to populate this:

- **Copy from ITGmania.** Place `<itg profile>/GrooveStats.ini` into
  `<data dir>/save/profiles/<your folder>/` and rename it to lowercase
  `groovestats.ini`. Same section, same keys, done.
- **QR-code login.** **Options → Manage Local Profiles → (select profile) →
  Link GrooveStats**. DeadSync writes the resulting key into
  `groovestats.ini` for you.

### 5. ArrowCloud API key

Same pattern, sibling file in the profile folder:

- **Path:** `<data dir>/save/profiles/<your folder>/arrowcloud.ini`
- **Contents:**

  ```ini
  [ArrowCloud]
  ApiKey = <your key>
  ```

Three ways to populate it:

- **Copy from ITGmania.** If `<itg profile>/ArrowCloud.ini` exists, copy it
  into `<data dir>/save/profiles/<your folder>/` and rename it to lowercase
  `arrowcloud.ini`.
- **Create by hand.** Make the file above with your key.
- **QR-code login.** **Options → Manage Local Profiles → (select profile) →
  Link ArrowCloud**. DeadSync writes `arrowcloud.ini` for you.

### 6. Online vs offline scores

- **Online scores** — if you used GrooveStats or ArrowCloud in ITGmania,
  dropping the API key into the matching `.ini` (above) reconnects you to your
  existing online account. After that, **Options → Online Scoring →
  Score Import** can download your history.
- **Offline scores** from ITGmania's `Stats.xml` are brought across by the
  [automatic importer](#automatic-import-recommended) — there's no manual
  equivalent, because each play has to be matched to a chart in your library to
  recover its hash. If you migrate by hand, run the importer afterwards to pull
  in your offline history (remember it creates a separate new profile).

### 7. Songs folder

DeadSync's **Options → Folders** submenu only *opens* folders; it does not let
you set paths from the UI. Two options:

**Easiest** — move or symlink your existing pack folders into
`<data dir>/songs/`. The **Open** button on **Options → Folders → Songs**
will take you straight there.

**Or point DeadSync at your existing library** — with the game closed, edit
`<data dir>/deadsync.ini` and, under `[Options]`, set one of:

- `AdditionalSongFoldersReadOnly` — for a library you don't want DeadSync to
  modify (e.g. a shared ITGmania `Songs` folder).
- `AdditionalSongFoldersWritable` — for a library DeadSync may write to.

Each value is a comma-separated list of absolute paths. Example:

```ini
[Options]
AdditionalSongFoldersReadOnly = /home/me/itgmania/Songs,/mnt/packs/extras
```

Save and relaunch. DeadSync will scan those roots in addition to its default
`songs/` folder.

## Quick checklist

**Fastest path**

- [ ] Scan your packs first — add your ITGmania `Songs` folder via
      `AdditionalSongFoldersReadOnly` in `deadsync.ini`, or move packs into
      `<data dir>/songs/`, so imported scores match.
- [ ] **Options → Manage Local Profiles → Import from ITGmania**, pick your
      profile, and let it copy the profile, options, avatar, online keys and
      offline scores.
- [ ] Run **Options → Online Scoring → Score Import** to pull down your online
      history.

**Manual path (if the importer can't find your install)**

- [ ] Create a profile via **Options → Manage Local Profiles → Create**.
- [ ] Drop ITGmania's `Avatar.png` into the new profile folder (rename to
      `profile.png` if you want it to win over any existing `avatar.png`).
- [ ] Bring `GrooveStats.ini` across as lowercase `groovestats.ini`, *or* use
      **Link GrooveStats** QR login (if used).
- [ ] Bring `ArrowCloud.ini` across as lowercase `arrowcloud.ini`, *or* use
      **Link ArrowCloud** QR login (if used).
- [ ] Optionally edit `WeightPounds` / `BirthYear` in `profile.ini`.
- [ ] Point at your ITGmania `Songs` folder via `AdditionalSongFoldersReadOnly`
      in `deadsync.ini`, or move packs into `<data dir>/songs/`.
- [ ] Launch DeadSync, then run **Options → Online Scoring → Score Import** to
      pull down your online history.
