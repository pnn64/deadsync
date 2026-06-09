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
| **Assign Pads to Players** | (opens screen) | Press-a-panel flow to choose which physical pad is P1 vs P2. See [§2a](#2a-which-pad-is-p1-vs-p2). Its help text shows the current mapping live. |
| **Swap P1/P2 Pads** | (action) | One-tap swap of the two pads' player assignment. Both pads must be connected. |

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
load-cell, connect timing), two tools make remote debugging possible.

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
