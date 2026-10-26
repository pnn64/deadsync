# Lua 5.1 native work and sort boundaries

Pattern matching no longer rejects searches after ten million internal steps.
Finite scans/backtracking and deferred pattern errors follow the reference without
a synthetic work counter. The flat choice/capture storage and 32-capture language
limit remain. Table concat/insert no longer reject ranges based on their width:
concat reports the first invalid value, while insertion performs the raw shift.
These native loops run between VM driver budget checks; instruction quotas do
not interrupt one in progress. Reference-defined errors and actual resource
exhaustion remain distinct from instruction quotas.

Sort now checks its 40-slot C-stack reserve after validating the table and before
validating the comparator. It then normalizes its argument stack to table plus
comparator, pads a missing comparator with nil and releases unused argument roots.
Direct host entry moves scratch storage past that padded slot when necessary.
Return hooks retain the normalized arguments. Sort's existing flat partition
storage and comparison schedule are reused; private C scratch-slot equivalence
and extreme table borders remain unfinished.

All **430 tests** pass: 43 unit, 386 integration and one documentation example.
Eight additions cover concat error precedence over full-width integer intervals,
insertion at -10,000,001, finite backtracking/searches beyond the old counter,
late malformed-pattern errors, twelve sort stack/comparator boundary cases,
collection of discarded arguments, return-hook padding and direct host entry
with/without a standalone context. Focused tests reproduced the old concat/root
failures and caught a padding/scratch overlap before final validation.
Formatting, strict all-target Clippy, whitespace checks and the normal/build
dependency tree pass. No production dependency, unsafe code or C link is added.

The [release CLI audit](2026-10-26-rust-lua-work-limits-audit.json) adds twenty-four cases against the
unmodified official Lua 5.1.5 executable. All new cases match complete process
status, stdout/stderr and file bytes. The full audit retains **112/150 raw matches**;
all thirty-eight differences are intentional version banners. Removing at most
one exact known leading banner from each stderr gives **150/150**. Both raw
observations and banners remain recorded. The strict audit still exits 1.

All [140 process probes](2026-10-26-rust-lua-work-limits-process.json) pass. The eighteen
[official isolated files](2026-10-26-rust-lua-work-limits-suite.json) keep their previous checksums:
**17/18 pass in both engines**. Both retain files.lua:59's Windows open-input
rename failure. The command still exits 1 with _U=true, 100-million-instruction
limits and 60-second deadlines. This does not establish all.lua, internal testC
or complete standalone coverage.

The pinned reference's [string library](https://www.lua.org/source/5.1/lstrlib.c.html)
has no work-count cap; its [table library](https://www.lua.org/source/5.1/ltablib.c.html)
specifies sort's reserve and argument trimming. The standalone archive SHA-256 is
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333, built
unmodified with MSVC /O2 /MT on Windows x64. Development binding patches remain
isolated from production. Both oracles keep Lua 5.1's native numeric/stack widths.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-work-limits-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-work-limits-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-work-limits-suite.json
```

All final recorded source hashes match the tree. Two [measurement runs](../performance/2026-10-26-rust-lua-work-limits.md)
cover 76 runtime and 22 frontend workloads. Layout probes measure the x64 VM struct
at 4,432 bytes after removing the counter, versus 4,440 before. Full resource and
memory bounds, collector timing/pauses, diagnostics/debug scratch slots, console
signals/modes, native modules, later dialects, persistent caches, Rust callbacks,
JIT and song/noteskin integrations remain open. The full goal is unfinished.
