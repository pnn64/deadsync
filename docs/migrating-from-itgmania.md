# Migrating from ITGmania to DeadSync

There is no automatic ITGmania → DeadSync profile import. The two games use
different on-disk formats, and DeadSync does not read ITGmania's `Stats.xml`.
This guide explains what carries over by hand, what doesn't, and exactly which
files to copy or edit.

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

## What you can actually migrate

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
- **Offline scores** from ITGmania's `Stats.xml` have **no migration path**.
  DeadSync does not read `Stats.xml`. Those scores stay in ITGmania.

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
