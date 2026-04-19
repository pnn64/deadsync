<div align="center">
    <img src="assets/graphics/logo.png" alt="DeadSync" width=25% height=25%>
</div>
<hr>

<div align="center">
DeadSync, as in "dead on sync", is a StepMania/ITG engine with Vulkan/OpenGL backends, focused on perfect sync and competitive-level performance. Cross-platform: Windows / Linux / BSD / macOS – x86_64 & ARM64.
<br><br>
⚠️ <b>DeadSync is under heavy development. Bugs are expected, features may change, and things *will* break. You have been warned.</b>
</div>
<hr>

## Prerequisites

Before building, ensure you have the following installed on your system:

1.  **Rust**: Install via [rustup](https://rustup.rs/).
2.  **Vulkan SDK**: Download and install the SDK for your operating system from the [LunarG website](https://www.lunarg.com/vulkan-sdk/).

### Windows build dependencies

-   **CMake**: Install from [cmake.org](https://cmake.org/download/).
-   **Ninja**: Install from [ninja-build.org](https://ninja-build.org/).

### Linux build dependencies (Ubuntu/Debian)
```bash
sudo apt update
sudo apt install --no-install-recommends build-essential cmake pkg-config libudev-dev libasound2-dev libvulkan-dev libgl1-mesa-dev
```

### macOS build dependencies (Homebrew)
```bash
xcode-select --install
brew install vulkan-loader molten-vk
```

If linking fails with `library 'vulkan' not found`, export the Homebrew library paths:
```bash
brew_prefix="$(brew --prefix)"
export LIBRARY_PATH="$brew_prefix/lib:$brew_prefix/opt/vulkan-loader/lib:${LIBRARY_PATH:-}"
export DYLD_FALLBACK_LIBRARY_PATH="$brew_prefix/lib:$brew_prefix/opt/vulkan-loader/lib:${DYLD_FALLBACK_LIBRARY_PATH:-}"
export RUSTFLAGS="-L native=$brew_prefix/lib -L native=$brew_prefix/opt/vulkan-loader/lib"
```

### BSD build dependencies (FreeBSD)
```bash
pkg install cmake python3 pkgconf alsa-lib alsa-plugins vulkan-validation-layers
```

## Getting Started

Follow these steps to get the game running:

1.  **Clone the Repository:**
    ```sh
    git clone https://github.com/pnn64/deadsync.git
    cd deadsync
    git submodule update --init
    ```

2.  **Add Songs:**
    Place your song packs in one of DeadSync's song scan roots:
    *   the `songs/` folder inside the data directory (see [Data Directories](#data-directories))
    *   the `songs/` folder next to the executable
    *   any folder listed in `AdditionalSongFolders`, `AdditionalSongFoldersReadOnly`, or `AdditionalSongFoldersWritable`

    *Example structure inside a song root: `<song-root>/MyPack/MySong/MySong.ssc`*

3.  **Build the Project:**
    Compile the game in release mode for optimal performance:
    ```sh
    cargo build --release
    ```

4.  **Run the Game:**
    After a successful build, run the executable from the project root:

    *   **On Windows:**
        ```sh
        .\target\release\deadsync.exe
        ```
    *   **On Linux:**
        ```sh
        ./target/release/deadsync
        ```
     *  **On macOS:**
        Before the first run, grant Input Monitoring permissions to `Terminal.app` in `System Settings > Privacy & Security > Input Monitoring`. Without this, deadsync will not receive any keystrokes. Then, run: 
        ```sh
        ./target/release/deadsync
        ```

## Configuration

After running the game for the first time, configuration files and a `save` directory will be generated.

### Game Settings
You can edit `deadsync.ini` to change various settings, including renderer, video resolution, VSync, `GfxDebug` (backend validation/debugging), and the default theme color.

### Input bindings

You can fully customize keyboard and gamepad controls in the `[Keymaps]` section of `deadsync.ini`. DeadSync maps **virtual actions** (e.g. `P1_Up`, `P1_Start`, `P1_Back`) to one or more **physical inputs**.

#### Keyboard

Use `KeyCode::<Name>` values, for example:

- `KeyCode::ArrowLeft`, `KeyCode::ArrowRight`
- `KeyCode::KeyA`, `KeyCode::KeyS`, `KeyCode::KeyD`, `KeyCode::KeyW`

Example:

```ini
[Keymaps]
P1_Up=KeyCode::ArrowUp,KeyCode::KeyW
P1_Start=KeyCode::Enter
P1_Back=KeyCode::Escape
```

#### Gamepad / Pad (low-level codes)

Gamepad/pad bindings are based on DeadSync’s `PadCode[...]` values emitted by the native input backend. The recommended way to bind a button is:

- `PadCode[0xDEADBEEF]` — bind to any gamepad button with that raw code.
- `PadCode[0xDEADBEEF]@0` — bind to that code, but only on gamepad `ID 0` (as shown in logs / sandbox).

To discover the codes for your device:

1. Start the game and go to the **Sandbox** screen by pressing `F4`.
2. Press buttons on your controller; you will see lines like:
   - `Gamepad 0 [uuid=...]: RAW BTN { PadCode[0x00030030], ... }`
3. Copy the `PadCode[...]` part (and optionally the `@0` device index) into `deadsync.ini`.

Example: bind P1 Start/Back to a specific button on gamepad 0:

```ini
[Keymaps]
P1_Start=PadCode[0x00030030]@0
P1_Back=PadCode[0x00030031]@0
```

Legacy high-level bindings like `PadDir::Up`, `PadButton::Confirm`, and `PadN::Dir::Left` are still accepted for convenience, but low-level `PadCode[...]` bindings are the most accurate and device-agnostic way to configure controllers.

### Profile & Online Features
A `save` directory is created inside the data directory to store your personal data (see [Data Directories](#data-directories) for its location).

*   To enable online features with **GrooveStats**, edit `<data dir>/save/profiles/00000000/groovestats.ini` and add your API key and username. This allows the game to fetch your online scores.
*   You can also change your in-game display name in `<data dir>/save/profiles/00000000/profile.ini`.

## Data Directories

By default, DeadSync stores user data outside the install directory so that upgrading the game doesn't risk overwriting your config, saves, or scores. Linux and FreeBSD use a single `~/.deadsync` root; Windows and macOS use platform-native locations.

### Default locations

| Platform | Data directory | Cache directory |
|----------|---------------|-----------------|
| **Linux / FreeBSD** | `~/.deadsync` | `~/.deadsync/cache` |
| **Windows** | `%APPDATA%\deadsync` | `%APPDATA%\deadsync\cache` |
| **macOS** | `~/Library/Application Support/deadsync` | `~/Library/Caches/deadsync` |

**Data directory** contains user data that should be backed up:

```
deadsync.ini          # game configuration
deadsync.log          # log file
save/
  profiles/           # player profiles, scores, settings
  screenshots/        # captured screenshots
songs/                # default song scan root
courses/              # course files
```

**Cache directory** contains regenerable data that can be freely deleted:

```
songs/                # parsed song metadata cache
banner/               # banner image cache
cdtitle/              # CD title image cache
downloads/            # temporary download data
noteskins/            # compiled noteskin cache
unlocks-cache.json    # online unlock cache
```

### Portable mode

If you prefer a fully self-contained install (e.g. for arcade cabs or USB sticks), create an empty file named **`portable.txt`** next to the executable. When this file is present, DeadSync stores everything in the executable's directory — the same behavior as older versions.

The file's contents are ignored; only its presence matters.

### Song directories

DeadSync scans for songs in the following locations:

1. The `songs/` folder inside the data directory.
2. The `songs/` folder next to the executable (in non-portable mode, so bundled songs are always found).
3. Any additional folders listed in `AdditionalSongFolders`, `AdditionalSongFoldersReadOnly`, or `AdditionalSongFoldersWritable` in `deadsync.ini`.

Course files follow the same pattern: DeadSync scans the data-directory `courses/` root first and, in non-portable mode, also scans the `courses/` folder next to the executable.

### Migration

On first run in non-portable mode, if DeadSync finds a `deadsync.ini` next to the executable but not in the data directory, it will automatically copy `deadsync.ini`, `save/`, and legacy cache subdirectories into the new data/cache locations. Install-folder `songs/` and `courses/` are **not** copied; they remain in place and are still scanned in non-portable mode. The originals are **not** deleted — you can clean them up manually after verifying everything works.

## Contributing

We welcome contributions of all sizes. These notes are directional, not law—open a discussion or draft PR if you are unsure.

- Keep code simple and direct; stick to functional or procedural styles and avoid OOP patterns (even in Rust).
- Write small, single-purpose functions with clear names; compose simple pieces instead of building deep abstractions.
- Prefer immutability and pure functions; minimize global state and side effects so behavior stays predictable and testable.
- Reduce duplication by reusing and composing functions rather than repeating logic or adding one-off helpers.
- Bias toward efficient, low-overhead code: favor `Vec` and iterators, borrow instead of clone, and keep dependencies lean.

## Collecting Logs

When reporting a bug, attaching a log file helps the team diagnose the problem quickly.

### 1. Enable file logging

Open `deadsync.ini` (created after the first run; see [Data Directories](#data-directories)) and set the following under `[Options]`:

```ini
[Options]
LogToFile=1
LogLevel=Debug
```

| Setting      | Values                                     | Default |
|--------------|--------------------------------------------|---------|
| `LogToFile`  | `1` (enabled) / `0` (disabled)             | `1`     |
| `LogLevel`   | `Error`, `Warn`, `Info`, `Debug`, `Trace`  | `Warn`  |

Use **Debug** for most bug reports. Use **Trace** only if asked—it produces significantly more output.

### 2. Reproduce the issue

Launch the game and reproduce the problem. The log is written to **`deadsync.log`** in the data directory (see [Data Directories](#data-directories) above).

### 3. Share the log

Attach `deadsync.log` to your GitHub issue or discussion. If the file is large, compress it first (`.zip` / `.gz`).

> **Privacy note:** The log does not contain passwords or API keys, but it does include file paths and song/pack names from your system. Review the file before sharing if that is a concern.

<h2>Acknowledgements</h2>
<p>
    DeadSync would not exist without years of work from the StepMania and ITG communities.
    In particular, we would like to acknowledge:
</p>
<ul>
    <li>
        <a href="https://github.com/stepmania/stepmania">StepMania</a> and its contributors
        for creating the original engine that made all of this possible, including the
        <a href="https://github.com/itgmania/itgmania/blob/beta/Docs/credits_old_Stepmania_Team.txt">original StepMania Team</a>
        and the
        <a href="https://github.com/itgmania/itgmania/blob/beta/Docs/credits_SM5.txt">StepMania 5 developers</a>.
    </li>
    <li>
        <a href="https://github.com/itgmania/itgmania">ITGmania</a> and its developers
        for shaping the modern ITG experience on dedicated machines.
    </li>
    <li>
        <a href="https://github.com/Simply-Love/Simply-Love-SM5">Simply Love</a>, its maintainers,
        and important forks such as
        <a href="https://github.com/zarzob/Simply-Love-SM5">zmod</a>, whose work defines much of
        the current ITG player experience.
    </li>
</ul>
<p>
    And more broadly, everyone who has written themes, noteskins, tools, and simfiles for the
    community over the years. ❤️
</p>
