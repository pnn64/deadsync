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
  - [11e. Pack metadata (gifpack.ini)](#11e-pack-metadata-gifpackini)
  - [11d. Authoring GIFs](#11d-authoring-gifs)

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
the variant with the smallest reference BPM at or above the song's tempo: the
densest gif that still plays at or under the pad's 30Hz LED cap. Note this is
not "nearest BPM": a 130-BPM song with 129- and 225-BPM variants uses the 225
one (129 would need to play faster than its reference to keep up). A song
faster than every variant uses the highest-reference one, which then plays
half-time. An untagged file is used as a single variant when only one BPM is
needed.

Example: `gameplay_25@4b120.gif` and `gameplay_25@4b240.gif` give DeadSync
120-BPM and 240-BPM variants for the `gameplay` role; a 100-BPM song plays the
120 variant, a 200-BPM song the 240 variant.

**Playback timing:** only song select drives beat-locked playback. There a
BPM-tagged background ignores its own frame delays entirely: DeadSync maps the
music preview's live beat straight to a frame (one pass through the loop region
spans the tagged beat count), so the animation follows the music exactly,
tempo changes included. On every other screen, gameplay included, a background
plays in real time at the durations encoded in its GIF frame delay fields; the
BPM tag still selects which variant plays, so a variant authored at the song's
tempo stays close to the beat even without the lock. The pad's hardware LED
update rate is capped at 30Hz, so author GIFs at 30fps or slower for smooth
playback.

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

Both trees also ship a `dance/none` pack: an empty pack whose `gifpack.ini`
declares `Fallback = "none"`, so every role resolves to nothing. Select it to
turn that group off entirely without touching the master Panel Lights toggle,
and independently per group and per player. What "off" looks like on the pad
is controlled by **Idle Pad Lights** (see below): the pads either revert to
their firmware lighting or hold solid black.

**Selecting a pack:** the machine defaults live on the StepManiaX options
page (**Pad Lights Pack** for backgrounds, **Judgement Pack** for judgement
gifs, and **Idle Pad Lights** for what an empty pad shows; the rows appear
once Panel Lights is on). Each player can override both packs per profile in
**Player Options**; a profile with no override follows the machine default. A pack dropped into `dance/` while the game is running shows
up in the selectors right away (the list is re-scanned each time), but its
GIFs are only decoded at first use after launch, so restart the game to
actually see a brand-new pack's animations.

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
{role}_{size}[@{beats}b{bpm}].gif           <- background with optional BPM tag
{role}_{size}[@{difficulty}][@{grade}].gif  <- results background, difficulty and/or grade
```

- `{role}` is one of the role names listed in [§11c](#11c-role-names-and-fallback-chains).
- `{size}` is `25` (25-LED pads) or `16` (16-LED pads). DeadSync tries the
  requested size first, then the other size as a fallback.
- The optional `@` suffix(es) always come **after** the size.
  - `@{beats}b{bpm}`: BPM variant tag (backgrounds only). `{beats}` is the loop
    length in beats; `{bpm}` is the reference tempo (both numeric).
  - `@{difficulty}`: results-only, the chart's difficulty, lowercase: one of
    `beginner`, `easy`, `medium`, `hard`, `challenge`, `edit`. This is the
    chart's actual file-level difficulty, not a display name — a `Challenge`
    chart tagged for on-screen display as "Novice" or "Expert" elsewhere still
    tags its gif `challenge`.
  - `@{grade}`: grade suffix for results backgrounds (e.g. `@S+`, `@B-`,
    `@star5`). The grade tag starts with a letter (not a digit), so
    DeadSync can always distinguish it from a BPM tag.
  - A results file can combine both: `@{difficulty}@{grade}` (difficulty
    first, then grade).

Examples:
- `default_25.gif`
- `gameplay_25@4b120.gif`
- `results_25.gif`
- `results_25@S+.gif`
- `results_25@B-.gif`
- `results_25@star5.gif`
- `results_25@hard.gif`
- `results_25@hard@S+.gif`
- `results_25@edit@star5.gif`
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
4. If the selected pack declares a `Fallback` (see §11e): that pack's `<role>`
5. Global registry: `common`'s `<role>` (automatic; see §11e for how to opt out)
6. Global registry: selected pack's `default`
7. If the selected pack declares a `Fallback`: that pack's `default`
8. Global registry: `common`'s `default`

If no pack is selected (or the `common` pack is selected), steps 3-5 and 6-8
collapse to `common` directly.

Every pack automatically falls back to `common` for any role it doesn't
supply (steps 5 and 8) — a pack only has to author what it wants to
customize. A pack can opt individual roles, or itself entirely, out of this
via `gifpack.ini` (see §11e). A role the selected pack lists under
`CanBeEmpty` (and doesn't supply) ends the chain at its step outright: no
`Fallback` pack, no `common`, and no `default`-role fallback (steps 6-8 are
skipped) — that role shows no animation at all.

What a pad shows when nothing resolves at all is controlled by the **Idle Pad
Lights** option on the StepManiaX options page. During gameplay the game
always owns the LEDs (judgement effects can fire at any moment), so an empty
background is **solid black** there regardless of the option. On every other
screen, **Firmware** (the default) hands an empty pad back to its built-in
lighting — the idle and step animations stored on the pad — while **Black**
keeps ownership of the LEDs and holds the pads dark; panel press animations
still play, since the game is still driving the panels.

Ownership is decided **per pad**. Because each player can pick their own packs,
one pad may resolve a background while the other resolves nothing: the first
keeps animating and the second goes back to its firmware (or holds black),
independently. A pad handed back to its firmware shows the firmware's own step
lighting rather than the game's press animation, since the game no longer drives
it. Frames for the two pads are still sent together, so pads that are both driven
stay in step with each other.

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
| `results@star5` | Results screen, Quint (5-star) grade |
| `results@star4` | Results screen, Quad (4-star) grade |
| `results@star3` | Results screen, Triple (3-star) grade |
| `results@star2` | Results screen, Double (2-star) grade |
| `results@star1` | Results screen, Single (1-star) grade |
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
results@star5  -->  results  -->  default   (no base-letter fallback)
results@star4  -->  results  -->  default
results@star3  -->  results  -->  default
results@star2  -->  results  -->  default
results@star1  -->  results  -->  default
results@S      -->  results  -->  default
results@D      -->  results  -->  default
results@F      -->  results  -->  default
```

So authoring `results_25@A.gif` covers A+, A, and A- automatically unless you
also provide the `+`/`-` variants.

These grade chains are walked with the same `CanBeEmpty` rule as the main
resolution chain: a candidate the selected pack lists under `CanBeEmpty` (and
doesn't supply) stops the walk right there, so e.g. `CanBeEmpty = "results"`
shows nothing for any grade you didn't author a grade-specific gif for,
instead of falling through to `default`.

**Difficulty tagging:** any `results@<grade>` role above can also be
qualified with the difficulty of the chart that earned the grade:
`results@<difficulty>@<grade>`, e.g. `results_25@hard@S+.gif`. Difficulty is
one of `beginner`, `easy`, `medium`, `hard`, `challenge`, `edit` (the file's
actual difficulty, not a display name — see the filename convention above).

Difficulty slots into the existing grade chain as an extra tier tried
*before* the difficulty-agnostic role at each grade level, so packs that
don't care about difficulty keep working exactly as before:

```
results@hard@S+  -->  results@S+  -->  results@hard@S  -->  results@S
  -->  results@hard  -->  results  -->  default
```

That is: DeadSync first tries the exact difficulty+grade combo, then the
existing grade-only file (unchanged from before difficulty tagging existed),
then the same two steps for the grade's base letter (e.g. `S` for `S+`/`S-`),
then a difficulty-only file with no grade, then the plain `results` role, then
`default`. You only need to author the specific combinations you want to
differentiate -- e.g. add just `results_25@hard@F.gif` to give Hard-chart
fails a distinct look, and every other difficulty/grade combination keeps
using whatever it already resolved to.

**Where BPM tagging actually matters:** `@{beats}b{bpm}` variants are only
useful on `gameplay` and `song_select` — the only two roles that resolve
while DeadSync has an actual song BPM to match against (the playing song
during Gameplay/Practice, or the highlighted song on Song/Course Select).
Every other role (`default`, `options`, `results`, and every `results@...`
grade/difficulty variant) always resolves with no song BPM available, since
there's no "current song" concept on those screens. Authoring several
`@{beats}b{bpm}` variants of one of those roles isn't an error, but
`select_variant` deterministically always picks the lowest-reference-BPM
variant in that case — the rest just go unused.

**BPM variants don't pool across packs by default.** Each `(pack, role,
size)` combination has its own separate variant list — only the BPM-tagged
files *that pack itself* authored for that role/size are considered. If your
pack supplies *any* `song_select` gif at all, `common`'s `song_select`
variants are never looked at, even if one of them would actually fit the
song's tempo better. See `MergeCommonBPMVariants`/`MergeFallbackBPMVariants`
(§11e) to opt a role into pooling instead.

#### Per-panel judgement/press GIFs (`smx-judge-lights/`)

These animate individual panels on top of (or instead of) the background.
Unlike backgrounds, judgement GIFs support neither the `@{beats}b{bpm}` BPM
tag nor the `@{difficulty}`/`@{grade}` tags — there's exactly one GIF per
name per size per pack. A judgement plays once (or loops) per event
regardless of song tempo or grade, so there's no variant to pick among.
`MatchColorToDifficulty` (§11e) is likewise background-only; declaring it in
a `smx-judge-lights/` `gifpack.ini` has no effect.

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

Judgement packs follow the same fallback logic as backgrounds, including the
automatic fallback to `common` (see §11e): a selected pack is tried first,
then its declared `Fallback` pack if any, then `common`. Unlike a missing
background (which can reasonably show nothing), leaving a judgement
completely unhandled would mean that event gives no pad feedback at all, so
this automatic step matters more here — a pack only has to author the
judgements it wants to customize and everything else still shows *something*.
A pack can still opt specific names (or itself entirely) out via `gifpack.ini`
if it genuinely wants an event to show no gif.

---

### 11e. Pack metadata (`gifpack.ini`)

Every pack automatically falls back to `common` for any role or judgement it
doesn't supply -- you only have to author what you want to customize. Each
user pack can additionally include an optional `gifpack.ini` file in its pack
directory to adjust that behaviour. The file uses a simple `key = "value"`
format (no TOML library required; only the keys listed here are read; others
are ignored).

**Supported keys:**

| Key | Values | Effect |
| --- | --- | --- |
| `Fallback` | any pack name, or `"none"` | Try the named pack (both LED sizes) before falling back to `common`. `"none"` opts the *whole pack* out of the automatic `common` fallback -- every role/judgement this pack doesn't supply shows nothing rather than pulling from `common`. Omitting the key is the default: no extra pack to try, but the automatic `common` fallback still applies. |
| `CanBeEmpty` | comma-separated list of role or judgement names | These specific names never fall back to anything when this pack doesn't supply them -- not `Fallback`, not `common`, and for background roles not the `default`-role fallback either (the resolution chain stops dead at the listed name). They show nothing for that name specifically, while every other name still falls back normally. |
| `MatchColorToDifficulty` | comma-separated list of base role names (background packs only, e.g. `"results"`) | Whatever gif actually resolves for that role gets recolored to match the played chart's difficulty color. See "Difficulty color matching" below. |
| `MergeCommonBPMVariants` | comma-separated list of base role names (background packs only, e.g. `"song_select"`) | Pool this pack's own BPM-tagged variants for that role with `common`'s, instead of using only this pack's own variants. See "Merging BPM variants across packs" below. |
| `MergeFallbackBPMVariants` | comma-separated list of base role names (background packs only) | Same as `MergeCommonBPMVariants`, but pools with the declared `Fallback` pack's variants instead. No-op if this pack has no `Fallback` declared. |

**Example:**

```ini
# gifpack.ini -- place in assets/smx-pad-lights/dance/<your-pack>/
# (and optionally in smx-judge-lights/dance/<your-pack>/)

Fallback = "cool-pack"
CanBeEmpty = "miss, ok, bad"
```

This tells DeadSync: if a role or judgement GIF is not found in this pack, try
`cool-pack` first, then `common` -- except for `miss`, `ok`, and `bad`, which
show nothing if this pack doesn't provide them (they never pull from
`cool-pack` or `common`).

**The `common` pack never needs a `gifpack.ini`.** It is the terminal fallback
and has no further pack to fall back to.

**`Fallback` and `CanBeEmpty` apply to background and judgement packs
independently.** A `gifpack.ini` in `smx-pad-lights/dance/<pack>/` controls
background behaviour; one in `smx-judge-lights/dance/<pack>/` controls
judgement behaviour. A pack of the same name can declare different (or no)
metadata in each tree.

**When to use `Fallback`:** The most common use case is wanting to replace
just a handful of GIFs from an existing pack. Instead of copying the whole
pack into your own folder, create a new pack directory with only the GIFs you
want to replace and point its `gifpack.ini` fallback at the source pack.
DeadSync will use your overrides for the roles you've provided and pull the
rest from the source pack (then `common` for anything neither pack has).

Example: you like `cool-pack` but want a different `gameplay` background.

```
smx-pad-lights/
  dance/
    cool-pack/          <- the pack you like; no gifpack.ini needed
      gameplay_25.gif
      results_25.gif
      default_25.gif
      ...
    my-overrides/
      gifpack.ini       <- Fallback = "cool-pack"
      gameplay_25.gif   <- your replacement; everything else comes from cool-pack
```

Select `my-overrides` in the options and DeadSync serves your `gameplay_25.gif`
for the gameplay role and every other role from `cool-pack` (or `common` if
`cool-pack` doesn't have it either).

**When to use `CanBeEmpty`:** Use this when you deliberately want certain
events to show no gif at all rather than borrowing `common`'s (or a
`Fallback` pack's) version -- for example a minimalist judgement pack that
only lights up misses and mines, and wants everything else to stay dark
instead of showing `common`'s style for the rest. For background packs the
opt-out is total: a pack that supplies its own `default` but declares
`CanBeEmpty = "gameplay"` gets no background during gameplay at all -- the
`gameplay` role does not slide over to the pack's `default` gif.

**Combining GIFs from more than two packs** is not directly supported beyond
the automatic `common` step: `Fallback` is a single value pointing to one
pack, so the declared chain is always at most two deep (your pack, then one
`Fallback` pack) before the automatic `common` fallback. If you want to mix
GIFs from several different non-`common` packs, create your own custom pack
directory and copy the files you want into it directly.

**Difficulty color matching (`MatchColorToDifficulty`):** background packs
only. Rather than hand-authoring a separately-colored gif per difficulty (see
`results@<difficulty>@<grade>` in §11c), you can author **one grayscale gif**
(every pixel R=G=B, varying only in brightness) and have DeadSync recolor it
automatically to match whichever difficulty the player is looking at results
for. List the base role name(s) this applies to:

```ini
# gifpack.ini -- place in assets/smx-pad-lights/dance/<your-pack>/
MatchColorToDifficulty = "results"
```

Whatever gif actually wins the normal `results` resolution chain (plain
`results`, a grade-specific file, a difficulty-specific file, whatever you've
authored) gets recolored before it's sent to the pad: each pixel's R, G, and B
values are multiplied by the target difficulty color's R, G, and B (each
scaled to `0.0..=1.0`). White pixels become the target color exactly, black
stays black, and grays become dimmed versions of the target color.

The target color is the same theme-relative color the rest of the UI already
uses for difficulty (`Challenge` = your current theme color; `Hard`,
`Medium`, `Easy`, `Beginner` step backward around the same color wheel one
step at a time; `Edit` is a fixed grey). So a Hard-difficulty result tints
differently depending on what theme color you've picked, but always in a way
consistent with how difficulty is colored everywhere else in the game.

Before the multiply, the target color is adapted for the pad LEDs: the theme
palette is sRGB (authored for screens), while the LEDs are linear in the byte
value, so raw palette bytes would render the pastel difficulty colors as
washed-out near-white. DeadSync gamma-expands each channel relative to the
brightest one, keeping peak brightness and hue while dropping the floor
channels to the light level the color actually encodes. Author your grayscale
art normally; the tint colors come out vivid on the pad automatically.

**This only works correctly on grayscale source art.** If your gif has actual
color in it, the same per-channel multiply still runs, but the result
generally will not look like a clean recolor -- a red pixel tinted toward
blue doesn't become a *different shade of blue*, it becomes mostly black
(red's green and blue channels multiplied by blue's target color both land
near zero). There's no way for DeadSync to tell which color in your gif is
the "primary" one to shift, so this feature assumes lightness-only source art
and leaves genuine hue/saturation-aware shifting as a possible-but-more-
complex future extension.

Recoloring is computed once per (pack, role, difficulty) combination and
cached -- not per frame -- and only recomputed when the player's theme color
changes, so it costs nothing during normal gameplay.

**Merging BPM variants across packs (`MergeCommonBPMVariants` /
`MergeFallbackBPMVariants`):** background packs only. By default, BPM
variants don't pool across packs (see the note in §11c) — if your pack
supplies any `song_select` (or `gameplay`) gif at all, `common`'s variants for
that role are ignored entirely, even ones that would fit a song's tempo
better. These two keys opt a role into pooling instead:

```ini
# gifpack.ini
Fallback = "cool-pack"
MergeCommonBPMVariants = "song_select"
MergeFallbackBPMVariants = "song_select"
```

- `MergeCommonBPMVariants` pools this pack's variants for the listed role(s)
  with `common`'s.
- `MergeFallbackBPMVariants` pools with the declared `Fallback` pack's
  variants instead (a no-op if no `Fallback` is declared).
- Both, as above: all three sources (your pack, the `Fallback` pack, and
  `common`) are pooled into one set, and DeadSync picks whichever variant
  best fits the song's actual BPM from the combined set.
- Neither (the default): only your pack's own variants are considered, as
  described in §11c.

Each key is independent and only affects the role names it lists — a role not
listed in either key keeps the default (own-pack-only) behavior even if the
pack sets one of these keys for a *different* role.

**Exact BPM-tag collisions still respect the normal precedence.** If two
sources happen to author the same reference BPM for a role (say your pack
and `common` both have a `@2b129` variant), pooling doesn't create a
duplicate or pick arbitrarily — your pack's own variant always wins, then the
`Fallback` pack's, then `common`'s, exactly the same precedence order as the
regular (non-merged) resolution chain.

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
