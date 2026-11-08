# Teejusb FSR controllers

DeadSync configures boards running the [teejusb/fsr firmware](https://github.com/teejusb/fsr)
through their USB serial interface. This includes the
[teejusb travel pad](https://github.com/teejusb/teejusb-pad), Teensy boards,
Arduino Leonardo/Pro Micro keyboard or joystick builds, RP2040 builds, and
custom boards that preserve the same serial protocol.

Gameplay still uses the board's normal keyboard/joystick input and your existing
input mappings. Serial readings are used for configuration, not gameplay timing.
Live sensor polling stops when the editor closes; reconnect restoration and
pending saves continue on the worker.

## Quick start

1. Enable **Use FSRs** in **Options → Input Options → Input Options**.
2. Close the teejusb web server, Arduino Serial Monitor, and other applications
   using the board's serial port.
3. Open **Configure Pads** under **Options → Input Options**, or the pad editor
   in Song Select. Discovery can take a few seconds.
4. Use Left/Right to select a sensor, Up/Down to adjust its threshold, and Shift
   for one-unit adjustments. Start opens Advanced mode. Large configurations
   automatically page as the selection moves.

Thresholds use the firmware's 0–1023 units and a linear display. Sensors are
numbered from **1 in the UI**, corresponding to **0-based firmware indices** in
configuration files. Sensor counts are read from the board; four sensors are not
assumed. With serial pads connected, physical keyboard arrow keys control the
editor and ordinary gameplay key presses are consumed so keyboard-mode panels
do not change thresholds while being tested. Enter and Escape keep their normal
menu mappings. Custom firmware that emits navigation keys instead of upstream's
letter keys cannot be distinguished from a real keyboard at this input layer.

## Settings and persistence

The serial worker stores device settings in **`save/teejusb.json` inside the DS
data directory**. It creates the file when a board is identified. Exit DS before
editing it; settings are read when the serial worker starts.

Confirmed threshold changes are saved on the computer after two seconds without
another edit, or on leaving the editor, disabling FSR support, or clean shutdown.
DS also sends the firmware's `s` command when supported. Older firmware without
that command still gets computer-side persistence: DS restores the recorded
thresholds when the same board reconnects while Use FSRs is enabled. Previously
identified boards can reconnect outside the editor; automatic probing of new
boards happens only while the editor is open.

The first connection reads existing settings without applying defaults. After an
edit has been confirmed and recorded, the DS copy becomes the reconnect baseline.
To adopt settings from another tool, exit DS and clear that device's `thresholds`
array before restarting. A changed sensor count prevents restoration of the old
array. Review group mappings after reflashing a different sensor layout, even if
the number of sensors is unchanged: this protocol does not expose a layout ID.

Unique USB serial numbers identify boards across port renumbering. Boards without
a unique serial use the port instead; keep those on consistent ports and review
their saved settings when replacing hardware. Multiple connected devices with
the same USB serial use separate port identities.
Once a duplicate is detected, its serial is recorded in `ambiguous_serials` so
unplugging one board cannot make the other inherit its settings.

Board save acknowledgements do not prove that settings survive a power cycle.
In particular, upstream's current RP2040 EEPROM code omits the initialization and
commit calls required by Arduino-Pico. The computer copy covers reconnects to DS;
retaining settings while DS is absent still depends on the firmware's storage
implementation. Set `save_to_board` to `false` to use only computer persistence.

## Custom ports, groups, and older firmware

Automatic discovery recognizes common native USB microcontrollers and FSR-related
USB descriptions, and validates both threshold and sensor-value responses.
Unknown USB IDs, serial adapters, and platforms without port enumeration can be
configured explicitly. For example:

```json
{
  "auto_detect": true,
  "devices": [
    {
      "port": "COM8",
      "name": "My dance pad",
      "protocol": "auto",
      "groups": [[0, 1], [2, 3], [4, 5], [6, 7]],
      "thresholds": [],
      "save_to_board": true
    }
  ]
}
```

Use the appropriate device path on other systems, such as `/dev/ttyACM0`,
`/dev/cu.usbmodem...`, or `/dev/cuaU0`. Set `auto_detect` to `false` to open only
configured/previously identified devices. Each detected entry receives an `id`;
retain it to preserve USB serial identity. Each entry addresses one board.

The protocol reports sensors, not panel directions, output keys/buttons, or
which sensors share a panel. By default every sensor gets its own bar. `groups`
provides display groups for multi-sensor panels; it does **not** change firmware
wiring or button mappings. Unlisted sensors remain visible individually. Simple
adjustments preserve the sensors' individual threshold offsets, and Advanced
edits address the original firmware indices even when a group reorders them.
Duplicate or invalid group indices are rejected.

Protocol choices:

- **`auto`** (default): identifies modern firmware using a same-value threshold
  command that old firmware rejects. It never guesses legacy from a timeout.
  Unidentified boards remain visible but read-only.
- **`modern`**: explicitly selects the `3 180` command format introduced in
  February 2022. Use only when the firmware is known to implement that format.
- **`legacy`**: selects the earlier `3180` format. It addresses at most ten
  sensors. Choose this for old firmware that appears read-only with `auto`.

Firmware supports at most 255 nonzero sensor IDs. Its seven-character modern
command limit restricts sensors with zero-based indices 100–254 to thresholds
of at most 999; the editor reflects that bound. Hardware and firmware may have
lower limits. Sensor enable, configurable debounce, and automatic offset
calibration are not exposed by upstream's protocol and are hidden in DS. DS never
sends the global offset-reset command as part of discovery or threshold editing.

## Diagnostics and validation

**FSR Dump** includes the serial monitor's settings path, connected boards,
sensor values, thresholds, pending edits, and recent errors. Serial I/O runs on
a worker with bounded responses; a missing board does not block the render loop.
Edits are checked against the complete returned threshold array before being
recorded, and stale commands cannot target a newly connected device.

Automated coverage exercises modern/legacy commands, split and invalid replies,
negative AVR readings, absent save support, failed acknowledgements, host restore,
changed sensor counts, multi-board identities, large layouts, grouped edits, and
keyboard-mode editor controls. Physical validation remains necessary on Teensy,
ATmega32U4, and RP2040, especially serial-port access, unplug/reconnect, full power
cycles, and gameplay while the editor is closed.
