# Linux/glibc locale compatibility

Linux/glibc now implements all six Lua 5.1 locale categories, including native
names and aliases, nil queries, unavailable locales, embedded-NUL boundaries,
environment defaults and restoration from a composite `LC_ALL` string.
Numeric conversion/formatting and stream scanning use the numeric decimal
point; source literals keep Lua's period syntax. Byte classes, case mapping,
identifier scanning and cached patterns follow ctype. Native collation handles
Lua's binary-string segments. Calendar text and errno messages use the selected
locale, preserving non-UTF-8 bytes through Lua errors and file-load diagnostics.

The [GNU libc manual](https://sourceware.org/glibc/manual/latest/html_node/Setting-the-Locale.html)
marks process-global `setlocale` as unsafe for concurrent use. The engine owns
locale objects created with `newlocale`/`duplocale`, changes only private
duplicates and releases retired objects with `freelocale`. A mutex pins each
object throughout `_l` native calls. `strftime_l` and `strerror_l` consume the
same engine selection as collation and character classification. No native
string pointer survives its guard. These declarations and the owned-handle
Send implementation remain isolated in the FFI module; no Cargo dependency or
C Lua linkage is added to production builds.

Selection is shared across Rust VMs. It does not mutate the host process-global
C locale. Arbitrary external native bindings, host locale synchronization and
other POSIX libc ABIs remain outside this proof. The implementation currently
targets Linux/glibc; it does not imply musl, Android or Apple locale support.

The threaded regression found that character-class publication could expose
cleared case mappings while switching to C. The class/case table and its C flag
now publish together under a read/write lock, and casing holds one read guard
for the string. This shared publication fix also applies on Windows.

`check_rust_lua_posix_locale.py` builds the real Rust CLI and the pinned,
unmodified official Lua 5.1.5 CLI. It generates private German ISO-8859-1,
French UTF-8 and Turkish ISO-8859-9 locales with `localedef`; it installs no
system locales. Thirty-six programs compare raw stdout, stderr and status.
Cases cover all 256 bytes, every byte class and its complement, high-byte
identifiers/escapes, numeric coercion, signed zero, decimal commas, numeric
streams, binary collation, localized dates/errors, category independence,
composite restoration, environment precedence and pattern invalidation inside
callbacks. NaN observations use a classification label and exclude payloads.
An independently compiled C fixture checks category constants/masks, langinfo
items, native integer/pointer widths and `tm` layout against the Rust boundary.

| Check | Result |
| --- | --- |
| Windows Rust tests | 627 pass: 73 unit, 550 integration, 4 documentation |
| Linux locale Rust groups | 3 pass, including four concurrent VM workers |
| Native locale CLI programs | 36/36 raw output/status matches |
| Native ABI fixture | Pass |
| Formatting, strict Clippy on both platforms | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source Windows C fixture | Raw output/status match; 36,766 dispatch/drop guards; panic=abort |
| Windows C-state lifetime | 73 created, 72 collected before shutdown, 73 destroyed |
| Windows process probes | 140/140 |
| Linux `main.lua`, C and Rust | Pass in both |
| Linux `all.lua`, C and Rust | Both fail at `big.lua:11`; strict audit exits 1 |
| Windows full entry points | Common `main.lua:53` failure; strict audit exits 1 |
| Broad Linux Rust audit | Six failed targets in parent and current; exit 101 retained |

The broad Linux audit runs every Rust target with `--no-fail-fast`. Its failed
targets are `debug_locals`, `io`, `native_errors`, `os`, `package` and `stdlib`.
They also fail in parent `6bf605e34`. Failures include a Windows-sized gsub
buffer fixture, GNU numeric-stream consumption, native argument validation
order, wildcard-rename fixture cleanup, a reader-quota fixture and an assertion
in the development C reference bridge. I/O mutex poisoning creates secondary
failures; those are recorded separately from the first mismatches. The existing
locale query and OS removal-error tests improve. This audit remains a failure,
and no assertion is disabled to obtain a passing count.

The unchanged full suite has the official README's path setup and no Lua
bootstrap. Rust now progresses through the preceding files to the same
large-string assertion as the x86-64 C reference. The four-GiB expression fits
the reference's 64-bit size limit. The harness keeps the assertion and failure;
it does not impose a 32-bit runtime limit. Internal T/testC and suite native
libraries remain absent in this stock mode, with their automatic skips visible.

Raw [locale matrix](2026-10-27-rust-lua-posix-locale-matrix.json),
[Linux suite](2026-10-27-rust-lua-posix-locale-linux.json),
[Windows suite](2026-10-27-rust-lua-posix-locale-windows.json),
[native C boundary](2026-10-27-rust-lua-posix-locale-boundary.json),
[Windows process](2026-10-27-rust-lua-posix-locale-process.json) and
[Linux audit](2026-10-27-rust-lua-posix-locale-linux-audit.json) reports are
retained. [Validation metadata](2026-10-27-rust-lua-posix-locale-validation.json)
pins source bytes, fresh build targets, native executables, locale data, system
headers and check logs. Full standalone Lua 5.1 parity, remaining C/auxiliary
APIs, production native loading and broader GC/debug/resource behavior remain
open. Later dialects, performance tuning and integrations stay deferred.
