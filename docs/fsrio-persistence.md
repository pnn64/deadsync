# FSRio threshold persistence investigation and fix

Investigated 2026-10-24 against DeadSync `d54e3b225dcf7fcfac49bf71a8b74e2779d07d46`
and ADP staging `0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc`.
The reporter subsequently confirmed that unplugging USB restored the original
threshold on the baseline build. The patched build still needs physical
power-cycle verification.

## Implemented fix

The findings below describe the baseline commit above. This branch now follows
ADP Tool's persistence behavior:

- Successful threshold changes become pending saves. Two seconds without another
  edit sends output report `[0x04, 0x00]`, committing the configuration on the board.
- Leaving the configuration screen/overlay and clean shutdown flush pending
  changes immediately. Device teardown also makes a final save attempt.
- Failed saves remain pending, with up to three automatic attempts two seconds
  apart and another attempt when leaving/closing. No-op edits do not write to
  the board or postpone a pending save.
- Firmware identification selects legacy report `0x02` through version 1.2,
  including unidentified older boards. Newer firmware uses sensor report `0x0C`.
  Each sensor's release/press ratio is retained (rounded to the nearest raw
  unit), while mappings, resistor values, and flags are preserved. A zero press
  threshold uses zero release; inverted release values are bounded by press.
- Pending saves continue to be serviced outside the editor. Partial group-write
  failures retain successfully written sensor edits for saving.

The implementation is in `crates/deadsync-input-fsr/src/fsrio/protocol.rs`, with
the HID adapter and lifecycle handling in `fsrio.rs`. Transport tests model
active versus saved board state and power loss, along with batching, firmware
selection, metadata preservation, and transfer failures. Named FSRio profiles
remain a separate feature. A physical unplug/reconnect test is still required.

## Findings

DeadSync has a confirmed missing save operation. It writes FSRio thresholds to
the board's active configuration but neither requests nonvolatile storage nor
saves/restores FSRio tuning in a DeadSync pad profile. Whether that explains
Kigha's exact report still needs a test that distinguishes restarting the game
from removing power from the board.

### Configuration writes and persistent saves are separate operations

- In `crates/deadsync-input-fsr/src/fsrio.rs`, `Monitor::set_threshold` updates
  the cached configuration and calls `write_config`. That function sends HID
  feature report `0x02`, containing all twelve thresholds, the global release
  multiplier, and all twelve button mappings. There is no save command in this
  backend.
- ADP's [legacy configuration handler][sensor-reports] copies that report into
  the in-memory `configuration` object. It does not call `Save()`.
- ADP declares [save report ID `0x04`][structs]. Its [save handler][pad-reports]
  calls `ModuleConfigInstance.Save()`, which [writes the configuration][config]
  through the EEPROM abstraction. The [ATmega32u4 implementation][avr-eeprom]
  uses `eeprom_update_block`; the [ESP32-S3 implementation][esp-eeprom] writes
  bytes and commits them.
- The [ADP tool][device] marks successful sensor edits as unsaved and saves
  after more than 2000 ms without another edit. It also calls `SaveChanges()`
  for connected devices when its connection manager is destroyed.
- This is an **output report**, not another feature report. The tool's
  [reporter][reporter] sends `[0x04, 0x00]` through `hid_write`; its comment says
  Linux needs at least two bytes. A DeadSync implementation should follow the
  output-report path rather than append a feature-report call.

The expected result with this firmware is that an edit survives closing and
reopening DeadSync while the board remains powered, then reverts to the last
saved configuration after board power loss. This explains why the reported
two-launch Windows test can pass without demonstrating persistent storage.
Changing the operating system is not required to produce this distinction.
A PC reboot or shutdown is not a reliable substitute for removing board power,
because some USB ports and powered hubs continue supplying power.

### DeadSync's named pad configs currently exclude FSRio

`crates/deadsync-shell/src/pad_config.rs::pad_config_profile_cursor` returns a
device only for an SMX pad in the Song Select overlay. The FSRio case is
explicitly rejected and covered by the existing `profile_cursor_is_overlay_smx_only`
test. `app/smx_runtime.rs::sync_pad_config_fsr` uses this result to enable saving
and load the config list. Capture and apply in `smx_config.rs` also use the SMX
SDK exclusively.

The generic pad-config file format accepting `Backend=fsrio` does not provide a
working FSRio capture/restore path. Neither joining a player profile nor changing
the SMX managed-config option enables FSRio persistence. The introductory claim
in `stepmaniax.md` that FSRio shared the profile system was incorrect and has
been corrected on this investigation branch.

### Newer firmware needs care before adding automatic saves

The current backend always uses legacy report `0x02`. The [ADP tool][device]
switches to per-sensor report `0x0C` for firmware newer than 1.2. That report
contains separate press/release thresholds and other sensor settings.

In staging's [legacy compatibility handler][sensor-reports]:

1. Reading report `0x02` derives its release multiplier from **sensor 0's press
   threshold divided by its release threshold**.
2. Writing the report sets every legacy sensor's release threshold to its press
   threshold **multiplied by that multiplier**.

For example, sensor 0 with press 200 / release 100 produces a multiplier of 2,
then a later bulk write produces release thresholds twice their press thresholds.
Even a corrected global ratio cannot represent independently tuned per-sensor
release values. The per-sensor handler preserves a distinct release value for
each sensor; firmware [uses that value for button release][module-pad].

This is a separate compatibility issue, not proof of Kigha's persistence failure.
Adding a save command to the existing bulk writer without addressing it could
persist release settings that the user did not intend to change.

## Reproduction and information to collect

Record the DeadSync version, FSRio board/firmware version, operating system,
whether ADP Tool or another configuration tool is running, and what "between
sessions" means in the affected setup. Keep other configuration tools closed
during the first test.

1. Record sensor 2's original threshold. Set it to a distinct value in DeadSync
   and wait a moment with the configuration screen open for the queued edit to
   reach the backend.
2. Exit and restart DeadSync without disconnecting or depowering the board.
   Check the value again. Persistence here only establishes active board state.
3. Exit DeadSync and remove **all** power from the FSRio, including any hub or
   external supply. Reconnect it and start DeadSync. Check the value again.
   On the baseline build, reversion to the previously saved value matches the
   missing-save finding. On the patched build, the new value should survive.
4. As a control, set the same threshold in ADP Tool, wait more than two seconds,
   close the tool, and repeat the full board power cycle. The value should now
   survive if that firmware's save operation and storage work correctly.
5. Restore the original tuning with ADP Tool and allow it to save.

DeadSync's existing **FSR Dump** action writes `fsrdump.txt` in its data
directory. Before/after dumps can capture threshold arrays, mappings, device
identity, and raw identification reports (`0x09` / `0x0E`) for comparing firmware
and state. The dump compares cached monitor configuration with a fresh device
read; that is useful if values disappear even without board power loss.
On the examined staging firmware, reading legacy report `0x02` also clears its
light module, so dump collection is not entirely free of device side effects.

If step 2 fails while board power is definitely maintained, the missing-save
operation alone does not explain it. Investigate board resets, custom firmware,
another application writing configuration, rejected writes, or stale cached
state. DeadSync currently drops the result of `monitor.set_threshold` in
`apply_pad_commands`, and its FSRio writer does not read configuration back to
verify a successful transfer.

## Original implementation scope

The following was proposed during research. Items 1-3 and transport regression
coverage in item 4 are now implemented; physical hardware verification remains.

1. Read firmware identification/capabilities and use per-sensor reports where
   supported. Preserve per-sensor release thresholds, mappings, resistor values,
   and flags; define a policy for scaling release thresholds when press values
   change. Keep legacy handling for older supported firmware.
2. Mark only successful, changed configuration writes as pending persistence.
   Batch writes to nonvolatile storage after an idle interval, following ADP's
   two-second approach, and flush on leaving the editor and clean shutdown.
   Do not write EEPROM on every held-key repeat or sensor polling frame.
3. Service pending saves even after the configuration screen closes; current
   FSR monitor polling is driven by the configuration screen/overlay. Log save
   failures and retain pending state for a bounded retry while connected.
4. Add transport-level regression coverage for write/save ordering, batching,
   failure handling, and per-sensor release preservation, then verify on hardware
   with a complete power cycle on Windows and Linux.
5. Treat named FSRio profiles/default restoration as a separate feature. The
   board already has nonvolatile storage; a host-side profile system is not
   required just to retain its current tuning. Such a system would additionally
   need stable board identity and explicit multi-board behavior.

[sensor-reports]: https://github.com/electromuis/analog-dance-pad/blob/0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc/firmware/src/Reports/SensorReports.cpp
[structs]: https://github.com/electromuis/analog-dance-pad/blob/0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc/firmware/src/Structs.hpp
[pad-reports]: https://github.com/electromuis/analog-dance-pad/blob/0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc/firmware/src/Reports/PadReports.cpp
[config]: https://github.com/electromuis/analog-dance-pad/blob/0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc/firmware/src/Modules/ModuleConfig.cpp
[avr-eeprom]: https://github.com/electromuis/analog-dance-pad/blob/0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc/firmware/src/hal/32u4/EEPROM.cpp
[esp-eeprom]: https://github.com/electromuis/analog-dance-pad/blob/0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc/firmware/src/hal/esp32s3/EEPROM.cpp
[device]: https://github.com/electromuis/analog-dance-pad/blob/0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc/adp-tool/src/Model/Device.cpp
[reporter]: https://github.com/electromuis/analog-dance-pad/blob/0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc/adp-tool/src/Model/Reporter.cpp
[module-pad]: https://github.com/electromuis/analog-dance-pad/blob/0804d8cfe59c947bd89043ef72c7dfbde6aa9bdc/firmware/src/Modules/ModulePad.cpp
