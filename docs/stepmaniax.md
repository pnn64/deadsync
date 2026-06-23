# StepManiaX (SMX) Pad Support

DeadSync can talk to StepManiaX dance pads directly over USB (via the
[`rustmaniax-sdk`](https://crates.io/crates/rustmaniax-sdk) crate), use them as
gameplay input on every platform, and read/edit/store their sensor thresholds —
either as a one-off tuning or as named **pad config profiles** that DeadSync
re-applies automatically.

This document covers the whole flow: enabling SMX, the two ways to configure a
pad, the per-profile config system, how it's stored, the firmware/hardware
caveats, the internal architecture, and how to capture diagnostics for a bug
report.

> FSRio pads share the same Configure Pads UI and profile system; most of this
> applies to them too. SMX-specific behavior is called out where it matters.

## Contents

- [Why connect to the pads directly?](#why-connect-to-the-pads-directly)
- [1. Quick start](#1-quick-start)
- [2. The StepManiaX options page](#2-the-stepmaniax-options-page)
  - [2a. Which pad is P1 vs P2?](#2a-which-pad-is-p1-vs-p2)
- [3. Two ways to configure a pad](#3-two-ways-to-configure-a-pad)
- [4. The Configure Pads screen](#4-the-configure-pads-screen)
- [5. Pad profiles (Song Select)](#5-pad-profiles-song-select)
- [6. How configs are stored](#6-how-configs-are-stored)
- [7. Pad types & firmware](#7-pad-types--firmware)
- [8. Architecture (for contributors)](#8-architecture-for-contributors)
- [9. Diagnostics & bug reports](#9-diagnostics--bug-reports)
- [10. Troubleshooting cheatsheet](#10-troubleshooting-cheatsheet)
- [11. Pad light GIF animations](#11-pad-light-gif-animations)
  - [11a. GIF format](#11a-gif-format)
  - [11b. File locations](#11b-file-locations)
  - [11c. Role names and fallback chains](#11c-role-names-and-fallback-chains)
  - [11d. Authoring GIFs](#11d-authoring-gifs)
  - [11e. Pack metadata (gifpack.ini)](#11e-pack-metadata-gifpackini)

---

## Why connect to the pads directly?

Reading the pads through the SDK instead of as generic OS gamepad input buys two
things you don't get otherwise:

- **Readable input labels.** Each panel reports as a named SMX trigger (e.g.
  `SMX[40ea] L`, where `40ea` is the start of the pad's serial), so bindings and
  the Test Input screen show *which pad and which panel* an input came from
  instead of an anonymous button number.
- **Stable, per-pad player assignment.** An SMX pad carries its own serial and
  its own player side, so DeadSync keys bindings and saved configs to the
  *physical pad* and a pad always maps to the same player. Generic OS gamepad
  input instead assigns players by USB **enumeration order** (first pad → P1,
  second → P2), which the OS can reshuffle on a reconnect or reboot — so P1 and
  P2 silently swap when the OS decides the other pad enumerated first. Talking to
  the pads directly avoids that entirely.

---

## 1. Quick start

1. **Options → Input Options → Input Options** (the input *backend* page).
2. Turn on **Use FSRs**. This reveals the **StepManiaX** entry on the same page.
   (FSR/SMX pad config is gated behind this toggle; the SMX page and the FSR
   debug dump only appear when it's on.)
3. Open **StepManiaX** and turn on **Use StepManiaX**.
   - This change takes effect on the **next launch** — the SMX SDK and its event
     listeners are wired once at startup (same as the gamepad-backend option).
4. Restart DeadSync. Your pads now act as input, and **Configure Pads** (under
   Options → Input Options) shows their live sensor values.

---

## 2. The StepManiaX options page

`Options → Input Options → Input Options → StepManiaX` (shown only while **Use
FSRs** is on):

| Setting | Values | Notes |
| --- | --- | --- |
| **Use StepManiaX** | No / Yes | Master switch for native SMX input. Applied on next launch. |
| **DeadSync Manages Pad Config** | No / Yes | See [§3](#3-two-ways-to-configure-a-pad). When on, DeadSync writes a resolved config to each pad. |
| **USB Polling** | 500–1000 µs (50 µs steps) | How often the SDK polls the pads over USB. Lower = more responsive, more CPU. Applied **live**. Default 1000 µs. |
| **Default Pad Config** | Low / Medium / High | Built-in sensitivity preset (matches the official SMX tool's presets). Used as the fallback when DeadSync manages config and nothing else resolves. |
| **Pad Player** | Player 1 / Player 2 | Shown only when **one** pad is connected. Picks which player that lone pad is, overriding its jumper. See [§2a](#2a-which-pad-is-p1-vs-p2). |
| **Assign Pads to Players** | (opens screen) | Shown only when **two** pads are connected. Press-a-panel flow to choose which physical pad is P1 vs P2. See [§2a](#2a-which-pad-is-p1-vs-p2). Its help text shows the current mapping live. |
| **Swap P1/P2 Pads** | (action) | Shown only when **two** pads are connected. One-tap swap of the two pads' player assignment. |

These persist to `deadsync.ini` under `[Options]` (`SmxInput`,
`SmxManagesPadConfig`, `SmxUsbPollingUs`, `SmxDefaultPadConfig`, and the pad
assignment serials `SmxP1Serial` / `SmxP2Serial`).

---

## 2a. Which pad is P1 vs P2?

By default each pad's **player side is decided by its hardware P1/P2 jumper**:
the SDK orders the pads so the P1-jumpered pad is Player 1 and the P2-jumpered
pad is Player 2. DeadSync keys both **input routing** and **pad-config profiles**
off this slot order, so in the normal case there's nothing to do, and once two
correctly-jumpered pads are seen the mapping is saved automatically.

Two situations need a manual assignment:

- **Both pads share a jumper** (e.g. both set to P1). The SDK can't tell them
  apart, so the main Menu shows an amber *"both pads share a jumper, assign
  pads"* warning and the assignment screen auto-opens.
- **Pads installed on the wrong sides** (jumpers are correct, but the pads are
  physically swapped). Moving a 100-lb pad is hard, so swap them in software.

**Assign Pads to Players** walks you through it: step on a panel of the pad you
want as **Player 1** (it lights **blue**), then the pad you want as **Player 2**
(it lights **red**). The pressed pad's serial is pinned to that player slot, which
overrides the jumper. **Swap P1/P2 Pads** does the same in one tap when both pads
are connected. The assignment is stored by serial in `deadsync.ini`
(`SmxP1Serial` / `SmxP2Serial`) and pushed to the SDK on launch, so it survives
reconnects and restarts.

### Single pad: which player?

With only **one pad** connected and no serial assignment saved, the pad follows
its hardware **P1/P2 jumper** (P1-jumper → Player 1, P2-jumper → Player 2), and
DeadSync saves that side automatically. A single stage therefore works out of the
box on whichever side its jumper selects.

To play a lone pad as the other player, use the **Pad Player** row in the
StepManiaX options (it appears only when one pad is connected). Pick Player 1 or
Player 2 and DeadSync pins the pad's serial to that side, overriding the jumper.
The picker reflects the pad's live player side, so it stays correct as pads
connect, disconnect, or are swapped while the screen is open.

If you later connect a second pad, clear or update the assignment through the
Assign Pads to Players flow (or edit `deadsync.ini` directly — see below).

### Manual serial assignment

You can bypass the in-game flow and assign pads directly in `deadsync.ini`. This
is useful if:

- You want to pre-configure a single pad as **P1** or **P2** before first launch
  (the in-game **Pad Player** option does the same once the pad is connected).
- You want to pre-configure two-pad assignments before first launch.
- The in-game assignment screen isn't cooperating.

#### Finding your pad's serial

Your pad's full 32-character serial appears in several places:

1. **Trace logs** — set `LogLevel=Trace` in `deadsync.ini`, launch the game, and
   look for the connect line:
   ```
   SMX: pad connected at slot 0: P2 fw=5 serial=fbd71fe2ad721359d4a4a9fcbbb32785
   ```

2. **FSR debug dump** — from Options → Input Options → Input Options →
   StepManiaX, the FSR debug dump (when available) prints each pad's serial.

3. **HID capture** — if you run with `SMX_CAPTURE_DIR` set, decode the
   `.smxhid` file; the `DeviceInfo` response shows the serial.

#### Editing deadsync.ini

Under `[Options]`, set one or both:

```ini
[Options]
SmxP1Serial=fbd71fe2ad721359d4a4a9fcbbb32785
SmxP2Serial=
```

- **`SmxP1Serial`** — the serial of the pad that should be Player 1.
- **`SmxP2Serial`** — the serial of the pad that should be Player 2.

Leave a field empty (or remove the line) to not pin that side — it will fall
back to the hardware jumper for any pad not explicitly assigned.

To force a single pad to P2 (overriding its jumper), set its serial as
`SmxP2Serial` and leave `SmxP1Serial` empty:

```ini
[Options]
SmxP1Serial=
SmxP2Serial=fbd71fe2ad721359d4a4a9fcbbb32785
```

Changes take effect on the next launch of DeadSync.

---

## 3. Two ways to configure a pad

This is the key mental model. **DeadSync Manages Pad Config** flips between them.

### Off — direct editing (the pad is the source of truth)

The **Configure Pads** screen writes thresholds straight to the pad, and the pad
keeps them in its own memory across restarts. DeadSync never overwrites them.
Use this if you tune your pads with the official SMX tool, or want one fixed
tuning that isn't tied to a player profile.

### On — managed profiles (DeadSync is the source of truth)

Every non-gameplay frame, DeadSync resolves a config for each connected pad and
writes it. Resolution order, per pad:

1. **This pad's saved default** — a profile config marked default for *this
   pad's serial*.
2. **A global default** — a profile config marked as the global default.
3. **The built-in preset** — `Default Pad Config` (Low/Medium/High). Also the
   fallback for a Guest / no-profile player.

It re-applies on **launch** and whenever a resolution input changes — the active
**profile**, **play style**, the **pad's serial** (a reconnect), or the chosen
**preset**. A cheap per-pad signature means it does **no file I/O and no pad
write** unless one of those actually changed, so a manual tweak isn't clobbered
mid-session.

> **Consequence:** with management on, edits made on the standalone Configure
> Pads screen are **transient** — they're re-applied over on the next resolve
> (e.g. next launch). To persist a tuning, save it as a **pad profile** from
> Song Select ([§5](#5-pad-profiles-song-select)). The Configure Pads screen
> shows a caption reminding you of this while management is on.
>
> Resolution is **skipped entirely during gameplay/practice** — pad config never
> changes mid-song; a hot-plug just re-resolves on the next menu frame.

---

## 4. The Configure Pads screen

`Options → Input Options → Configure Pads` (and as an overlay in Song Select).
Shows every connected pad side by side.

**Navigation is keyboard / dedicated-menu-button only** — stepping on a panel to
watch its sensor never moves the cursor or changes a value.

- **Left/Right** — move the cursor across all bars.
- **Up/Down** — adjust the focused threshold (**Shift** = fine, ±1 instead of ±5).
- **Start** — drill into **Advanced** for the pad under the cursor.
- **Back** — leave (or, in Advanced, return to the simple view).

### Simple view

One bar per panel (**L / D / U / R**), editing every sensor in that panel to a
single threshold.

### Advanced view (FSR pads only)

- **Per-sensor thresholds** — each panel's four edge sensors individually.
- **Per-sensor enable/disable** — turn a noisy/unused sensor off (**Start**).
- **Extra Advanced** (pad-level): **auto-recalibration** on/off, and **panel
  debounce** (0.5–25 ms).

Load-cell pads are **Simple-only** (no per-sensor config); they show their four
corner readings as separate bars sharing one threshold.

---

## 5. Pad profiles (Song Select)

Saving and managing named configs is offered **in-session**, for a pad mapped to
a **joined local profile** (a Guest can recall presets but can't save). The
standalone Options screen can't save — it has no profile context.

Open the **Song Select** menu and use:

- **Configure Pads** — the same editor as an overlay. Inside it:
  - **Select** opens the **Profiles** list for the cursor pad.
- **Profiles** list actions:
  - **Save current as new** — capture the pad's live tuning under a name.
  - **Activate** (Start) — write a saved config to the pad now.
  - **Set default** (Select) — make it this pad's default (per serial).
  - **Rename** / **Delete** / **Overwrite** — manage saved configs (delete asks
    for a confirm press).
- **Quick recall** — the Song Select menu also lists each pad's presets
  (`Sensitivity: Low/Medium/High`) and saved configs (`Pad Profile: <name>`) for
  one-press recall. A `*` marks the config currently applied; `(default)` marks
  this pad's default.

Defaults are **per pad**: any config can be the default for any pad (keyed by
serial), so two pads can share a config or use different ones. In **Doubles**,
both pads belong to the one joined player.

---

## 6. How configs are stored

Each local profile keeps its pad configs in **`padconfig.ini`** in that
profile's folder. It's human-readable and hand-editable:

```ini
[PadProfile0]
Name=Soft
Backend=smx
PadType=fsr
Serial=303030414243...
DefaultFor=303030414243... 303030414449...
GlobalDefault=0
Panel0.FsrLow=152 152 152 152
Panel0.FsrHigh=153 153 153 153
Panel0.LoadCellLow=20
Panel0.LoadCellHigh=25
Panel0.Enabled=1 1 1 1
; ...panels 1-8...
AutoCalibrationMaxTare=65535
DebounceMs=4
```

- **Backend** (`smx`) and **PadType** (`fsr` / `loadcell`) scope a config so it
  only ever applies to a matching pad — an FSR-tuned config is never written to
  a load-cell pad, and vice versa.
- **Serial** is provenance (which pad it was captured from / the overwrite
  target). **DefaultFor** is the set of pad serials this config is the default
  for; **GlobalDefault** is the fallback for pads without their own default.
- The `Panel*` / `AutoCalibrationMaxTare` / `DebounceMs` keys are an opaque
  threshold bag owned by the engine layer (see `engine::smx::PadConfigData`).

---

## 7. Pad types & firmware

DeadSync detects a pad's sensor technology from its config (master version ≥ 4
with the FSR flag set ⇒ **FSR**, else **load-cell**) and adapts the UI and the
editable ranges:

| | FSR pad | Load-cell pad (pre-v5) |
| --- | --- | --- |
| Sensors per panel | 4 edges (L/R/U/D) | 4 corners (1–4) |
| Threshold range | 5–250 | 20–200 |
| Per-sensor edits | ✅ | ❌ (one threshold/panel) |
| Advanced view | ✅ | ❌ (Simple-only) |
| Auto-recal / debounce | ✅ | ❌ |

**Firmware gate:** FSR threshold, per-sensor, auto-recalibration and debounce
edits require **firmware ≥ 5**. On older firmware those edits are silently
rejected — the trace logs ([§9](#9-diagnostics--bug-reports)) say exactly why.

---

## 8. Architecture (for contributors)

Layered so the storage and UI never depend on the SDK or each other:

| Layer | Module | Responsibility |
| --- | --- | --- |
| Shared SDK manager | `engine::smx` | One process-wide `SmxManager`; routes pad/system events to listeners; encode/decode `PadConfigData`; presets; FSR-vs-load-cell detection. |
| Backend-agnostic pad model | `engine::input::fsr` (`smx.rs`, `fsrio.rs`) | Exposes every pad as a `PadView` (buttons/sensors/live values); applies threshold/sensor/recal/debounce edits to hardware. |
| Config UI | `screens::pad_config` | The Configure Pads screen + Song Select overlay. Pure state machine over `PadView` + `InputEvent`; emits `PadCommand`s and `EditResult`s. |
| Resolution controller | `app::pad_config_sync` + `App::apply_smx_managed_preset` | App-owned source of truth for which config is applied to each pad and the active marker; drains UI **intents**; the per-pad resolve signature. |
| Storage | `game::pad_profiles` | `padconfig.ini` load/save and the pure default/resolve logic. No `engine`/`config` deps. |

The UI can't reach the app controller directly (screens don't depend on app), so
it queues `PadConfigIntent`s (`Override` / `Invalidate` / `RefreshList`) on the
Song Select state that the app drains each frame. Markers, the resolve cache,
and intents are all keyed by **pad slot (0/1)**, the one always-unambiguous key.

Pad slot is the single source of truth for player side. The SDK orders the slots
(slot 0 = P1, slot 1 = P2) from the jumper, or from the saved serial→player
assignment when one is set (`SmxManager::set_player_assignment`, pushed at init
and on change from `engine::smx`). Input routing and config→profile mapping both
key off the slot, so they never diverge even when two pads share a jumper. The
assignment screen lives in `screens::smx_assign`; the App auto-saves a clean
jumper-derived map and auto-prompts on an unresolved conflict
(`App::reconcile_smx_assignment` / `maybe_autoprompt_smx_assign`).

The `SmxManager` event callback fires while the SDK holds its internal lock, so
the callback must never call back into the manager (it would deadlock the USB
thread); identity (UUID/serial) is cached at connect and read from our own
mutexes instead. See the comments in `engine/smx.rs`.

---

## 9. Diagnostics & bug reports

Because much of the SMX path is hardware-dependent (firmware revisions, FSR vs
load-cell, connect timing), and the in-gameplay overlays run on the per-frame
render path, a few tools make remote debugging and performance triage possible.

### Trace logs

Set the log level to **Trace** (Options, or `LogLevel=Trace` under `[Options]`
in `deadsync.ini`), reproduce the issue, and grep the log for `SMX:`.

What you'll see:

- **Lifecycle** — init, connect/disconnect (with player side, firmware, serial),
  serial assignment, USB polling changes.
- **Resolution** (one line per actual re-resolve, `debug`) —
  `SMX: pad 0 resolved config 'Soft' (serial=…, fw=5, type=fsr, profile=…, applied=true)`.
  The first thing to check for "why did my pad get this config" or a mis-detected
  pad type.
- **Edit rejections** (`trace`) — every place an edit silently no-ops says why,
  e.g. `set_threshold pad 0 panel 3 rejected (fsr, fw 4, value 120 not in 5..=250)`
  or `set_sensor_enabled pad 0 rejected (load-cell pad has no per-sensor toggle)`.
- **Config availability** — `apply_config_data pad 0 skipped (config unavailable)`
  while a freshly-connected pad's config hasn't arrived yet (DeadSync retries).

### HID capture (`SMX_CAPTURE_DIR`)

To capture the raw USB/HID traffic — invaluable for diagnosing a pad we can't
test directly — launch DeadSync with the env var set to a directory path:

```sh
# Linux / macOS
SMX_CAPTURE_DIR=/tmp/smx-capture ./deadsync
```

```powershell
# Windows (PowerShell)
$env:SMX_CAPTURE_DIR = 'C:\tmp\smx-capture'
.\deadsync.exe
```

```cmd
# Windows (Command Prompt / cmd.exe)
set SMX_CAPTURE_DIR=C:\tmp\smx-capture
deadsync.exe
```

The directory is **created automatically** if it doesn't exist — no need to
create it manually.

The SDK wraps the HID enumerator with a recorder that writes a **`.smxhid`**
file per opened device into that directory (overwriting previous captures),
logging every read/write with timestamps. Reproduce the problem, quit, and send
the `.smxhid` files — they can be replayed through the SDK to reproduce the
exact device behavior offline.

> Leave `SMX_CAPTURE_DIR` unset for normal play; it's purely a debugging aid.

### Performance profiling (`DEADSYNC_SMX_PROFILE`)

If the in-gameplay FSR sensor overlay seems to cost frame rate (most noticeable
with vsync off, where the per-frame budget is tiny), this opt-in profiler shows
where the time goes. Launch DeadSync with the env var set:

```sh
# Linux / macOS
DEADSYNC_SMX_PROFILE=1 ./deadsync
```

```powershell
# Windows (PowerShell)
$env:DEADSYNC_SMX_PROFILE = '1'
.\deadsync.exe
```

```cmd
# Windows (Command Prompt / cmd.exe)
set DEADSYNC_SMX_PROFILE=1
deadsync.exe
```

Then play a song with the FSR sensor overlay enabled. Once a second, the log
prints a `smx-profile:` line timing the overlay's two per-frame costs, in
microseconds:

```
smx-profile: read avg=4.6us max=29.9us n=60 | draw avg=15.1us max=42.5us n=60
```

- **read**: fetching the latest pad sensor values each frame.
- **draw**: building the on-screen sensor bars and value text.
- **avg / max**: the average and the worst single sample over the last second.
- **n**: how many samples fell in that second (read is sampled at a fixed rate,
  draw tracks the frame rate).

The line logs at `warn`, so it shows at the default log level without raising it
to Trace, and it only prints while you are in a song with the overlay active. If
you are reporting an overlay performance issue, a few seconds of these lines is
the data to send.

> Leave `DEADSYNC_SMX_PROFILE` unset for normal play; it's purely a diagnostic.

---

## 10. Troubleshooting cheatsheet

| Symptom | Likely cause / what to check |
| --- | --- |
| SMX page missing in Options | **Use FSRs** is off (it gates the page). |
| Pad input doesn't work | **Use StepManiaX** needs a **restart** to take effect. |
| Editing thresholds does nothing | Firmware < 5, or it's a load-cell pad rejecting per-sensor edits. Check the `SMX: … rejected …` trace logs. |
| Configure Pads edits don't stick | **DeadSync Manages Pad Config** is on — they're re-applied on launch. Save as a profile from Song Select instead. |
| Pad detected as wrong type | Capture with `SMX_CAPTURE_DIR` and share the `.smxhid`; the resolve log shows the detected `type=`. |
| Can't save a profile | Only available in Song Select with a **joined local profile** (not Guest, not the Options screen). |
| Pads act as the wrong player (P1/P2 swapped), or a "share a jumper" warning | Use **Assign Pads to Players** (or **Swap P1/P2 Pads**) on the StepManiaX page; see [§2a](#2a-which-pad-is-p1-vs-p2). |

### Linux: pads not detected (USB permissions)

On Linux, HID devices under `/dev/hidraw*` are typically only accessible by
root. If DeadSync can't see your pads, you need a **udev rule** to grant
permission.

#### Identifying your SMX pads

Find which hidraw devices are your SMX pads:

```bash
# List all hidraw devices with vendor/product info
for dev in /sys/class/hidraw/hidraw*; do
  echo "$(basename $dev): $(cat $dev/device/uevent 2>/dev/null | grep HID_NAME)"
done

# Or filter directly for the SMX vendor/product ID (2341:8037)
lsusb | grep 2341:8037

# Check a specific hidraw device's vendor/product
udevadm info /dev/hidraw0 | grep -E "VENDOR_ID|PRODUCT_ID"
```

#### Adding the udev rule

Create a rule that grants read/write access to SMX pads for all users:

```bash
# bash / zsh
sudo tee /etc/udev/rules.d/99-stepmaniax.rules <<EOF
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="2341", ATTRS{idProduct}=="8037", MODE="0666"
EOF
```

```fish
# fish
echo 'SUBSYSTEM=="hidraw", ATTRS{idVendor}=="2341", ATTRS{idProduct}=="8037", MODE="0666"' | sudo tee /etc/udev/rules.d/99-stepmaniax.rules
```

> **Alternative approaches:**
>
> The rule above uses `MODE="0666"` which grants access to all users on the
> system — simplest and works on every distro. For a dance pad with no sensitive
> data this is fine.
>
> If you prefer tighter permissions, you can use the systemd/logind `uaccess`
> tag instead, which only grants access to the user physically logged in at the
> machine:
>
> ```
> SUBSYSTEM=="hidraw", ATTRS{idVendor}=="2341", ATTRS{idProduct}=="8037", TAG+="uaccess"
> ```
>
> Add `TAG+="udev-acl"` as well for backward compatibility with older
> ConsoleKit-based distros. The downside of `uaccess` is it can fail on
> non-systemd distros, headless setups, or if DeadSync is launched from a
> different session (e.g. SSH/tmux).
>
> You may also see `KERNEL=="hidraw*"` used instead of `SUBSYSTEM=="hidraw"` —
> they're functionally equivalent for this purpose.

Then reload the rules and trigger them:

```sh
sudo udevadm control --reload-rules
sudo udevadm trigger
```

**Unplug and replug** your pads after applying the rule, then restart DeadSync.

#### Verifying it worked

```bash
# Check permissions on hidraw devices — should show crw-rw-rw- (world read/write)
ls -l /dev/hidraw*

# Find which hidraw devices are SMX pads (vendor 2341, product 8037) and show permissions
for dev in /sys/class/hidraw/hidraw*; do
  name=$(basename $dev)
  hid_id=$(cat "$dev/device/uevent" 2>/dev/null | grep HID_ID | cut -d= -f2)
  if echo "$hid_id" | grep -qi "00002341:00008037"; then
    echo "/dev/$name is an SMX pad: $(ls -l /dev/$name | awk '{print $1, $3, $4}')"
  fi
done
```

---

## 11. Pad light GIF animations

DeadSync drives the SMX pad LEDs directly and supports a library of animated
GIFs for backgrounds, judgement feedback, and press feedback. All GIF loading
and decoding happens at startup (or when options change); nothing touches the
filesystem during gameplay.

### 11a. GIF format

Every GIF is a standard animated GIF with one special convention: a **marker
row** at the bottom of each frame. Each pixel in that row at x=0 or x=1 with
alpha=255 and R>=128 (white-ish) flags a timing or playback event.

**Full-pad backgrounds** (used for `default`, `gameplay`, `results`, etc.):

| Canvas size | Pad layout |
| --- | --- |
| 23x24 | 25-LED pads (SMX Gen 5+, the common format) |
| 14x15 | 16-LED pads (older gen) |

The 23x24 canvas is a 3x3 grid of 7x7 panel slots with 1px gaps between them,
plus a 24th row for the marker row. The 14x15 canvas uses 4x4 slots with 1px
gaps. Each panel block maps directly to its physical LED layout. The 25-LED
inner-ring LEDs sit at odd-x, odd-y positions inside each slot.

**Per-panel judgement/press GIFs** (tap grades, freezes, rolls, press):

| Canvas size | Pad layout |
| --- | --- |
| 7x8 | 25-LED (7x7 panel + 1-row marker row) |
| 4x5 | 16-LED (4x4 panel + 1-row marker row) |

A bare 7x7 or 4x4 (no marker row) is also accepted and simply loops the whole
sequence with no outro.

**Marker row flags:**

| Column | Meaning |
| --- | --- |
| x=0 | Loop point: playback returns here after the last frame |
| x=1 | Loop end (per-panel GIFs only): frames after this form an **outro** played on panel release |

For a sustain animation (freeze, roll) the section from `loop_frame` to
`loop_end` loops while the note is held; frames after `loop_end` play as an
outro when the panel is released. For a one-shot animation (tap judgement,
press) the sequence plays once and stops; the outro is played if it exists.

**BPM variants** (full-pad backgrounds only): You can author multiple GIFs for
the same role, each tagged with a beat count and reference BPM. DeadSync picks
the variant whose reference BPM is closest to the song's BPM, so the animation
stays beat-locked across tempo ranges. An untagged file is used as a single
variant when only one BPM is needed.

Example: `gameplay_25@4b120.gif` and `gameplay_25@4b240.gif` give DeadSync two
120-BPM and 240-BPM variants for the `gameplay` role; it picks whichever is
closer to the song tempo at resolution time.

All GIFs run at the real-time duration encoded in their GIF frame delay fields;
DeadSync does not re-time them. The pad's hardware LED update rate is capped at
30Hz, so author GIFs at 30fps or slower for smooth playback.

---

### 11b. File locations

DeadSync looks for GIFs in two directory trees rooted at the app's assets path:

```
assets/
  smx-pad-lights/       <- full-pad background GIFs
    common/
      common/           <- shipped default pack
    dance/
      <your-pack>/      <- user-authored packs
        gifpack.ini    <- optional pack metadata (see §11e)
  smx-judge-lights/     <- per-panel judgement/press GIFs
    common/
      common/
    dance/
      <your-pack>/
        gifpack.ini
```

**Per-song and per-pack backgrounds:** DeadSync also checks `smx-pad-lights/`
inside the song's folder and its parent pack folder before consulting the global
registry. This lets you ship a custom background alongside a simfile.

```
Songs/
  MyPack/
    smx-pad-lights/           <- applies to every song in MyPack
      default_25.gif
    MySong/
      smx-pad-lights/         <- applies only to MySong, overrides MyPack level
        gameplay_25.gif
```

The scoped lookup uses the same role names and BPM-variant filename conventions
as the global registry.

**Filename convention:**

```
{role}_{size}[@{beats}b{bpm}].gif      <- background with optional BPM tag
{role}_{size}[@{grade}].gif            <- grade-specific background
```

- `{role}` is one of the role names listed in [§11c](#11c-role-names-and-fallback-chains).
- `{size}` is `25` (25-LED pads) or `16` (16-LED pads). DeadSync tries the
  requested size first, then the other size as a fallback.
- The optional `@` suffix always comes **after** the size.
  - `@{beats}b{bpm}`: BPM variant tag (backgrounds only). `{beats}` is the loop
    length in beats; `{bpm}` is the reference tempo (both numeric).
  - `@{grade}`: grade suffix for results backgrounds (e.g. `@S+`, `@B-`,
    `@*****`). The grade tag starts with a letter or `*` (not a digit), so
    DeadSync can always distinguish it from a BPM tag.

Examples:
- `default_25.gif`
- `gameplay_25@4b120.gif`
- `results_25.gif`
- `results_25@S+.gif`
- `results_25@B-.gif`
- `results_25@*****.gif`
- `fantastic_blue_25.gif`
- `press_25.gif`

---

### 11c. Role names and fallback chains

#### Full-pad backgrounds (`smx-pad-lights/`)

These set the background for an entire pad (all 9 panels) based on the current
screen. DeadSync resolves the background through this chain for each screen:

1. Per-song `smx-pad-lights/<role>` (song folder)
2. Per-pack `smx-pad-lights/<role>` (pack folder)
3. Global registry: selected pack's `<role>`
4. If the selected pack declares a fallback (see §11e): fallback pack's `<role>`
5. Global registry: selected pack's `default`
6. If the selected pack declares a fallback: fallback pack's `default`

If no pack is selected (or the `common` pack is selected), steps 3 and 5 use
`common` directly and steps 4 and 6 are skipped.

If a pack has no `gifpack.ini` or does not declare a fallback, steps 4 and 6
are skipped and a missing role shows **solid black** on the pad. This is not
the same as the pad's own auto-lights: when Panel Lights is on the game holds
ownership of the LEDs at all times and the firmware's built-in animations are
suppressed. Auto-lights only resume when Panel Lights is turned off.

The table below lists **role names** (the internal key used for lookup). The
corresponding filename is `{role}_{size}.gif` — for grade-tagged roles like
`results@S+` the `@` suffix goes after the `_size` in the filename:
`results_25@S+.gif`.

| Role | When active |
| --- | --- |
| `default` | All screens not covered by a more specific role; also the ultimate fallback |
| `gameplay` | During a song (Gameplay screen) |
| `song_select` | Song/course selection screen |
| `results` | Evaluation/results screen (grade unknown or no grade-specific gif found) |
| `results@*****` | Results screen, Quint (5-star) grade |
| `results@****` | Results screen, Quad (4-star) grade |
| `results@***` | Results screen, Triple (3-star) grade |
| `results@**` | Results screen, Double (2-star) grade |
| `results@*` | Results screen, Single (1-star) grade |
| `results@S+` | Results screen, S+ grade |
| `results@S` | Results screen, S grade |
| `results@S-` | Results screen, S- grade |
| `results@A+` | Results screen, A+ grade |
| `results@A` | Results screen, A grade |
| `results@A-` | Results screen, A- grade |
| `results@B+` | Results screen, B+ grade |
| `results@B` | Results screen, B grade |
| `results@B-` | Results screen, B- grade |
| `results@C+` | Results screen, C+ grade |
| `results@C` | Results screen, C grade |
| `results@C-` | Results screen, C- grade |
| `results@D` | Results screen, D grade |
| `results@F` | Results screen, F grade |

**Grade fallback chain for `results@<grade>` roles:**

Grade-specific results backgrounds fall back gracefully so you only have to
author what you want to customize:

```
results@S+  -->  results@S  -->  results  -->  default
results@S-  -->  results@S  -->  results  -->  default
results@A+  -->  results@A  -->  results  -->  default
results@A-  -->  results@A  -->  results  -->  default
results@B+  -->  results@B  -->  results  -->  default
results@B-  -->  results@B  -->  results  -->  default
results@C+  -->  results@C  -->  results  -->  default
results@C-  -->  results@C  -->  results  -->  default
results@*****  -->  results  -->  default   (no base-letter fallback)
results@****   -->  results  -->  default
results@***    -->  results  -->  default
results@**     -->  results  -->  default
results@*      -->  results  -->  default
results@S      -->  results  -->  default
results@D      -->  results  -->  default
results@F      -->  results  -->  default
```

So authoring `results_25@A.gif` covers A+, A, and A- automatically unless you
also provide the `+`/`-` variants.

#### Per-panel judgement/press GIFs (`smx-judge-lights/`)

These animate individual panels on top of (or instead of) the background.

| Name | When played |
| --- | --- |
| `fantastic_blue` | Fantastic judgment (FA+ blue window) |
| `fantastic_white` | Fantastic judgment (standard white window) |
| `excellent` | Excellent judgment |
| `great` | Great judgment |
| `decent` | Decent judgment |
| `way_off` | Way Off judgment |
| `miss` | Miss judgment |
| `mine` | Mine explosion |
| `ok` | Freeze/roll/lift held successfully (on release) |
| `bad` | Freeze/roll/lift dropped |
| `freeze` | Looping sustain while a freeze note is held |
| `roll` | Looping sustain while a roll note is held |
| `press` | Generic press feedback: any panel press with no note (outside gameplay) |

`press` also plays during gameplay on any panel that has no note — the
judgement and sustain layers draw on top of it, so it is always overridden by
real hits. Outside gameplay and practice the `press` gif fires on every raw SMX
panel press, giving tactile feedback while navigating menus.

Judgement packs follow the same fallback logic as backgrounds. A selected pack
is tried first; if a gif is not found and the pack declares a fallback in its
`gifpack.ini`, the fallback pack is checked. With no fallback declared, a
missing gif plays nothing for that event (the panel stays black or shows
whatever the background gif has rendered beneath it).

---

### 11e. Pack metadata (`gifpack.ini`)

Each user pack can include an optional `gifpack.ini` file in its pack directory.
The file uses a simple `key = "value"` format (no TOML library required; only
the keys listed here are read; others are ignored).

**Supported keys:**

| Key | Values | Effect |
| --- | --- | --- |
| `fallback` | `"common"`, any pack name, or `"none"` | When a GIF is not found in this pack, try the named pack before giving up. Omitting the key or setting it to `"none"` means no fallback: a missing GIF shows nothing for that event. |

**Example:**

```ini
# gifpack.ini -- place in assets/smx-pad-lights/dance/<your-pack>/
# (and optionally in smx-judge-lights/dance/<your-pack>/)

fallback = "common"
```

This tells DeadSync: if a role or judgement GIF is not found in this pack,
fall through to the `common` pack before giving up. Without this line the pack
stands alone -- a missing GIF shows solid black.

**The `common` pack never needs a `gifpack.ini`.** It is the terminal fallback
and has no further pack to fall back to.

**Fallback applies to both background and judgement packs independently.** A
`gifpack.ini` in `smx-pad-lights/dance/<pack>/` controls background fallback;
one in `smx-judge-lights/dance/<pack>/` controls judgement fallback. You can
declare different fallbacks (or none) for each tree.

**When to use fallback:** The most common use case is wanting to replace just a
handful of GIFs from an existing pack. Instead of copying the whole pack into
your own folder, create a new pack directory with only the GIFs you want to
replace and point its `gifpack.ini` fallback at the source pack. DeadSync will
use your overrides for the roles you've provided and pull the rest from the
source pack.

Example: you like `cool-pack` but want a different `gameplay` background.

```
smx-pad-lights/
  dance/
    cool-pack/          <- the pack you like; no gifpack.ini (or fallback = "none")
      gameplay_25.gif
      results_25.gif
      default_25.gif
      ...
    my-overrides/
      gifpack.ini       <- fallback = "cool-pack"
      gameplay_25.gif   <- your replacement; everything else comes from cool-pack
```

Select `my-overrides` in the options and DeadSync serves your `gameplay_25.gif`
for the gameplay role and every other role from `cool-pack`.

**Combining GIFs from more than two packs** is not directly supported: `fallback`
is a single value pointing to one pack, so the chain is always at most two deep
(your pack, then one fallback). If you want to mix GIFs from several different
packs, create your own custom pack directory and copy the files you want into it
directly.

---

### 11d. Authoring GIFs

The **stepmaniax-gif-maker** is a desktop tool for authoring and previewing SMX
pad GIFs. It renders frames in real time to a connected SMX pad over USB, so
you see exactly what will appear on the hardware while you edit.

Features include:

- Full-pad and single-panel authoring modes
- Loop and outro region markers
- HSV adjustment (hue shift, saturation and value gain + bias) per frame or
  across all frames, with live preview
- BPM-variant export
- Hold playback simulation for reviewing sustain/outro animations

The tool is part of the
[`stepmaniax-gif-maker`](https://github.com/fchorney/stepmaniax-gif-maker)
project (separate repo). Build and run it with a connected SMX pad to author
or tweak animations before dropping them into your DeadSync assets folder.
