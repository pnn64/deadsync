# RE:Flex V2

DeadSync has a native backend for the RE:Flex V2 I/O board. It connects at startup
and reconnects automatically, without Playground, keyboard emulation, or a Lua
connection command. Sensor input continues while the pad editor is closed.

The implementation follows the upstream V2 HID protocol. Automated tests cover
decoding, thresholds, calibration, disconnects, lighting, and editor behavior;
physical hardware validation is still pending on every platform.

## First connection

1. Close Playground and other applications controlling the pad, then connect it
   and start DS. Hotplug discovery runs every two seconds.
2. Open **Options → Input Options → Configure Pads**. A connected RE:Flex enables
   the editor even when **Use FSRs** is off. Song Select's pad editor also works.
3. Select the pad and press **Start** to enter Advanced mode. **Step completely
   off all panels**, then press **F5 twice**. Keep the panels unloaded while DS
   samples the baseline for at least half a second. Another key cancels the
   confirmation. Movement during sampling rejects the calibration and preserves
   the previous baseline.
4. Test each panel. The first connection adds editable L/D/U/R mappings for its
   assigned player while preserving existing mappings. Further connections do
   not overwrite your mapping choices.

First-use mapping installation waits until you leave gameplay or practice, so
connecting a new board cannot reset another controller's held inputs mid-song.
Previously mapped boards reconnect during gameplay immediately.

A new pad sends no gameplay presses until it is calibrated. DS never silently
calibrates a pad at startup, since someone might be standing on it. A stable load
cannot be distinguished from an unloaded panel in software: stepping off before
calibration is essential. Recalibrate after changing the pad's mechanical setup.

## Thresholds and controls

All 16 sensors remain independent: four per panel in L, D, U, R order. A panel is
pressed when **any enabled sensor** is pressed. It releases only after every
enabled sensor releases. DS does not sum or average sensors for input detection.

Press and release thresholds are ADC counts above that sensor's saved baseline,
with `0 <= release < press <= 4095`. Defaults are press **30**, release **25**.
Each sensor uses hysteresis: it presses at or above its press threshold and stays
pressed until it reaches or falls below its release threshold. The live bars
show baseline-relative pressure; negative deltas display as zero.

The shared editor's Left/Right selection, Up/Down adjustment, Shift fine
adjustment, sensor toggles, and threshold-to-live-reading shortcut also apply.
Advanced mode provides a separate release selection for every sensor, shown as
a blue line (orange when selected). Changing press preserves the hysteresis gap
where the range permits; changing release leaves press unchanged. Simple edits
adjust the panel's sensors together while preserving their threshold offsets.

Additional keys in Advanced mode:

| Key | Action |
| --- | --- |
| F5, then F5 | Confirm unloaded calibration |
| F6 | Swap RE:Flex P1/P2 assignments; input briefly reconnects |
| F7 | Cycle Input, Chart, Judgment, and Off lighting |
| - / + | Change brightness by 5 percentage points |
| F8 | Test panel colors for three seconds |

Function keys keep calibration and assignment separate from the letter keys
emitted by common keyboard-mode teejusb boards when both kinds are connected.

Default brightness is 40%. Input lights follow the actual sensor-derived panel
state. Chart lights follow DS's note pulses; Judgment lights fade the latest tap
judgment color over 250 ms. Dance single, versus, double, and routine layouts
are supported, including a single player on P2. The **Test Lights** screen
overrides the selected mode. Rendering delays cannot block sensor reads: light
writes run on a separate worker, and stale chart/judgment data goes dark after
500 ms. Every light update sends all 16 segments required by the firmware.

## Saved settings and device identity

DS writes **`save/reflex.json` in the DS data directory**, atomically, within
about half a second of a change and on clean shutdown. Calibration, thresholds,
enabled sensors, light mode, brightness, and player assignment live on the
computer: the board does not provide commands to store them. Mapping bindings
are stored in the usual DS input configuration. Settings save failures appear
in the pad editor and log.

Pads are identified by USB serial, so unplugging or changing ports preserves
their settings and side. Up to two boards are active at once. Known disconnected
boards keep their player reservations. F6 swaps those reservations as well as
connected boards. For replacement hardware, exit DS and remove the old serial's
entry from `devices`, or set that entry's `enabled` to `false`.

Exit DS before manually editing the file; it is read at startup. The top-level
`enabled: false` disables native RE:Flex support. Each device also has an
`enabled` switch. Invalid configuration disables the service and preserves the
file for repair. Missing or duplicate USB serials are rejected to avoid applying
another pad's calibration. Detected duplicate serials are recorded in
`ambiguous_serials`; fix the identity conflict, then clear the affected entry
while DS is closed.

## USB and diagnostics

The backend requires the upstream RE:Flex product identity, VID **0483**, PID
**5750**, and vendor HID interface. It validates 64-byte input reports containing
16 little-endian 12-bit readings. Firmware with another descriptor, packet
layout, or sensor ordering needs a separate compatibility check.

Windows uses the built-in HID transport; Linux uses hidraw, macOS shared HID
handles, and FreeBSD the existing HIDAPI dependency. OS device permissions still
apply. On Linux, the user must have read/write access to this device's hidraw
node. DS logs open/read/write failures; **Debug FSR Dump** includes RE:Flex raw
readings, baselines, thresholds, and recent diagnostics.

Unplugging, read failure, shutdown, or 250 ms without a valid sensor report
releases held inputs. A failed light write leaves sensor input running and is
reported in the editor; reconnecting retries the output endpoint. USB reports
have no device timestamp, sequence number, or per-panel freshness marker. DS
timestamps receipt on the host; unchanged readings alone cannot reveal an
internally stalled panel. Lighting is best effort, not timing-critical input.

## Hardware validation checklist

- Connect before startup and during a song; reconnect after unplugging while
  holding a panel. Verify releases, restored settings, and no duplicated presses.
- Test all 16 sensors independently, two sensors on one panel, overlapping
  panels, disabled sensors, press/release boundaries, and unloaded calibration.
- Test two boards, port changes, player swapping, P2 single, double, and routine.
  Check coexistence with native gamepads, SMX, FSRio, and teejusb controllers.
- Verify full-panel colors, brightness, all modes, Test Lights, and clean-exit
  blackout. Check that sustained light updates do not add input delays.
- Measure real input latency/jitter and report cadence on Windows, Linux,
  macOS, and FreeBSD; exercise USB permission and output-endpoint failures.

## Protocol references

- [RE:Flex firmware](https://github.com/ReflexCreations/io-firmware/tree/af0d20123e80a0dd59bba8f784a2756a7412ec54)
- [Official Playground software](https://github.com/brittanyb/playground/tree/0c9c5b8738324d710b60d790f0ffb070ee87d5c5)
- [Taro's ITGmania driver](https://github.com/rehtlaw/itgmania/blob/beta/src/arch/InputHandler/InputHandler_Reflex.cpp)
