# Collector control integer arithmetic

Collector pause thresholds now divide the estimate into whole hundreds before
multiplying by the pause setting, with pointer-width unsigned wrapping. Negative
settings retain Lua 5.1's conversion; zero no longer receives an artificial
64 KiB minimum threshold. The setting still takes effect when a cycle finishes,
and restart replaces the old threshold.

Step limits now evaluate `(1024u / 100) * gcstepmul` as a 32-bit unsigned product
before assigning it to pointer-sized signed `l_mem`. A zero product selects the
unlimited case; a negative signed limit still performs the first incremental
operation. This distinction matters across targets:

| Multiplier | Unsigned product | 64-bit signed limit | 32-bit signed limit |
| --- | --- | --- | --- |
| -1 | 4294967286 | 4294967286 | -10 |
| -429496729 | 6 | 6 | 6 |
| -2147483648 | 0 | unlimited | unlimited |
| -2000000000 | 1474836480 | 1474836480 | 1474836480 |

The expressions and types come from the pinned Lua 5.1.5 `lgc.c`, `llimits.h`
and `luaconf.h`. Conversion into this collector's tracing units uses a wide
intermediate before narrowing. Saturating the multiplication at `usize` first
incorrectly reduced a large i686 budget before the subsequent division.

The [public-header Windows comparison](2026-10-28-rust-lua-gc-integers-current.json),
[checked Windows reference](2026-10-28-rust-lua-gc-integers-checked.json) and
[i686 release/checked comparison](2026-10-28-rust-lua-gc-integers-i686-current.json)
match all 1,063 observations in each profile:

- 972 step cases cover eighteen multiplier values, main-thread and five selected
  thread contexts, successful finalizers and eight error-value types.
- 90 pause cases cover negative, zero and positive settings before/after a cycle
  and across restart. Weak reachability tests verify automatic collection.
- A 150,000-table live graph verifies that the wrapped positive limit completes
  the collection and invokes its finalizer on both widths.

The [Windows baseline](2026-10-28-rust-lua-gc-integers-before.json) preserves the
committed checkpoint's failure after 45 observations; the
[i686 baseline](2026-10-28-rust-lua-gc-integers-i686-before.json) fails after 81.
Saved executables and SDK hashes bind those comparisons to commit `41976ab63`'s
preceding call/yield checkpoint. The
[intermediate large-graph comparison](2026-10-28-rust-lua-gc-integers-wide-budget-before.json)
records both i686 references completing while Rust stopped early. Its failing
Rust executable is preserved separately. The first 1,062-case corpus and its
validation records also remain retained.

The first fixture misclassified a large positive wrapped product below the
signed boundary, and expected deferred cleanup finalizers to run on the earlier
selected thread. Both assumptions were corrected against the reference. The
larger boundary corpus also exhausted the harness's default ten-million
instruction limit. Its Rust run now explicitly receives fifty million; other
boundary probes retain the default. This budget is recorded in the report.

Two portable arithmetic unit tests cover rounding, wrapping, signed conversion
and the large final budget. Two Lua regressions cover negative pause settings
and the large live graph. All four also pass in the separate
[build without default features](2026-10-28-rust-lua-gc-integers-portable-no-default.json).
The [full validation](2026-10-28-rust-lua-gc-integers-validation.json) passes
801 Rust tests on Windows x64 and 802 on Linux x64 and i686 SSE2, strict Clippy,
formatting, builds without default features and the empty normal/build
dependency tree. The [library argument audit](2026-10-28-rust-lua-gc-integers-stdlib.json)
matches all 5,507 cases. Both native-enabled i686 stock entry points pass with
T/testC absent. Windows retains 17/18 independent passes in all four modes and
the shared `files.lua:59` rename failure.

The earlier call/yield, unprotected-call, waiting-error, allocation, selected-GC
and concatenation corpora, their C-call count controls and twenty terminating
stdout/file scenarios are rechecked. The 39 successful native boundary runs
balance 1,017,881 operations, 12,141 state lifetimes and 49 library lifetimes;
25 expected terminating processes are counted separately. The
[evidence manifest](2026-10-28-rust-lua-gc-integers-evidence.json) binds current
sources, reports, logs and artifacts; a separate
[i686 verification](2026-10-28-rust-lua-gc-integers-i686-verification.json) checks
Linux artifact/source hashes and the assertion-enabled reference build.

Full Lua 5.1 parity remains unfinished. Heap counts still estimate this engine's
storage, and collector work scheduling, allocation charges and general timing
remain incomplete. Strict raw-count and unsettled timing comparisons stay
recorded as failures. General reader/body/API continuations, the unrooted
package-name mismatch, lifecycle/allocator APIs and resource/OOM behavior also
remain open. Core declaration coverage remains 76/80, with `lua_close`,
`lua_getallocf`, `lua_newstate` and `lua_setallocf` missing.
