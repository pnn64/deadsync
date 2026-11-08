# Windows numeric-stream lookahead

Windows numeric reads now preserve the pinned MSVC/UCRT floating parser's
lookahead loss. Its input adapter skips putting back NUL and signed `0xff`.
This happens at intermediate stage boundaries as well as when a scan ends.
For example, `0\0` followed by `12` is read as twelve, while `00\0` followed
by `12` leaves those final digits unread. A lost byte can also expose an
exponent or an `inf`/`nan` suffix to the next stage.

The scanner keeps the radix decision tied to the original character after the
first zero. Losing a later sentinel cannot turn a later `x` into a hexadecimal
prefix. Successful infinity words fetch a following byte. Generic NaN payloads
stop at their closing parenthesis, while CRT special payload paths fetch
lookahead. The special paths also include indeterminate spellings after a
partial signaling-NaN match. These distinctions affect the file position and
remaining bytes, even when the returned number has the same classification.

`Stream` drops only an already fetched numeric sentinel. It does not fetch an
extra byte to perform that cleanup. Parser transitions apply the same rule,
and ordinary binary reads retain NUL and `0xff`. All implementation changes
use safe Rust and add no dependencies or production C Lua linkage.

The standalone audit expands the earlier 1,706 inputs to 15,883. It includes
all 256 bytes after 26 selected prefixes, sentinel pairs, follow-on digits,
exponents and special-word suffixes, alongside the existing random hex floats
and malformed inputs. Each engine uses a fresh file per input. Observations
retain a value/classification, zero-length-read result, offset and unread-byte
hex, with raw stdout/stderr/status. Finite numbers use 17-digit formatting;
NaN payload bits remain excluded. All earlier inputs remain in the corpus.
The official Lua source, compiler flags and assertions remain unchanged.

| Check | Result |
| --- | --- |
| Windows expanded streams, parent `5fe4ea19e` | 14,661/15,883; 1,222 mismatches; strict exit 1 |
| Windows expanded streams, current | 15,883/15,883 |
| Linux/glibc expanded streams | 15,883/15,883 |
| Math/error expressions | 70/70 on each platform |
| Earlier Windows failures | All 62 close; all 1,706 earlier reference observations retained |
| Windows Rust tests | 628 pass: 73 unit, 551 integration, 4 documentation |
| I/O groups | 23/23 on each platform, including the new 288-case stage regression |
| GNU regression / native-error / locale groups | 2/2, 9/9 and 3/3 |
| Extended GNU locale CLI programs | 36/36 raw output/status matches |
| Formatting / strict Clippy | Pass on Windows and Linux |
| Normal/build dependency tree | Only deadlib-lua |
| Shared-source Windows C fixture | Raw output/status match; 36,766 dispatch/drop guards under panic=abort |
| C-state lifetime | 73 created, 72 collected before shutdown, 73 destroyed |
| Linux independent official files | 18/18 in reference, Rust and both fresh-process cache modes |
| Linux full suite | `main.lua` passes in both; `all.lua` fails at `big.lua:11` in both |
| Broad Linux Rust tests | Four existing failed targets; strict exit 101 retained |

The broad Linux failures remain `debug_locals`, `os`, `package` and `stdlib`.
They include buffer-size and reader-quota fixture assumptions, wildcard-rename
cleanup, and a `lua_rawgeti` assertion in the development C reference.
The unchanged failed-target list does not establish absence of regressions or
full parity. No assertion is disabled to obtain these results.

The unchanged `all.lua` large-string assertion still fails on both 64-bit
engines. Independent file probes use their existing `_U`/`Message` bootstrap;
the full entry points use only the official path setup and no Lua bootstrap.
Stock T/testC and suite native libraries remain absent. Complete C/auxiliary
API behavior, production native loading and broader GC/debug/resource behavior
also remain open. Full standalone Lua 5.1 parity is not claimed.

Raw [Windows before](2026-10-27-rust-lua-ucrt-streams-before.json),
[Windows after](2026-10-27-rust-lua-ucrt-streams-windows.json),
[Linux](2026-10-27-rust-lua-ucrt-streams-linux.json),
[locales](2026-10-27-rust-lua-ucrt-streams-locale.json),
[native boundary](2026-10-27-rust-lua-ucrt-streams-boundary.json),
[official files](2026-10-27-rust-lua-ucrt-streams-files.json),
[full suite](2026-10-27-rust-lua-ucrt-streams-suite.json) and
[Linux audit](2026-10-27-rust-lua-ucrt-streams-linux-audit.json) retain strict
outcomes. [Validation metadata](2026-10-27-rust-lua-ucrt-streams-validation.json)
pins source bytes, fresh targets, native artifacts, locale inputs, the installed
CRT source headers consulted for the adapter behavior, and check logs.
Performance tuning, benchmarks, later dialects and integrations remain deferred.
