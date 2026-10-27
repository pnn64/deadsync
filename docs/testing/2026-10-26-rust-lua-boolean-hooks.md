# Negated logical values and short-circuit hook boundaries

Repeated negation of logical values now preserves separate short-circuit tests
and final boolean materialization. The previous value-copy lowering executed
too many hook events on shortcuts such as not not (a or b). It could also read
a local before the callback belonging to its condition. The new value-context
lowering retains the terminal operand and pending true/false edges until the
final destination is known. It keeps named destinations unchanged until the
corresponding result write and preserves callback/metamethod order.

A copyable compiler descriptor and flat continuation jobs replace recursive
visits. Pending edges form linked lists in unused jump-target slots until
patched. Head/tail links permit constant-time append/join; no per-expression
patch-list vector, runtime state or opcode is added. Local conditions read
their original registers. Literal shortcut tests retain their original value
types, including strings and numbers. Comparisons keep their original
predicate and deferred result boundary. The no-instruction-hook interpreter
path fuses a verified comparison/branch pair.

Private instruction markers identify comparison branches and boolean skips.
The verifier checks producer/consumer registers, valid marker values, boolean
loads, skip targets, and forbidden entry into internal steps. Native export
emits comparison/JMP pairs and physical LOADBOOL skip pairs. Relay insertion
keeps a LOADBOOL and its skipped word adjacent, including a 200,000-word
long-branch test with the pair at a relay cut. The private cache keeps the
same wire layout; Cargo's compiler identity invalidates preceding builds.

| Check | Result |
| --- | --- |
| Tests | 535: 68 unit, 463 integration, 4 doc |
| Hook integration groups | 28/28, four new groups |
| Generated short-circuit programs | 576: 24 expressions x 6 inputs x 4 negation counts |
| Named-destination/local-mutation programs | 90 |
| Multiline/callback/GC/metamethod programs | 17 |
| Long source chains | 64, 1,024 and 8,192 operands; true and false outcomes |
| New native export group | 36 layouts; shared VM/C export and source comparisons |
| Verifier corruption cases / relay adjacency group | 7 / 1 |
| Strict Clippy / format | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Current CLI | 171/209 raw; 209/209 after exact known leading banner removal |
| Preserved parent CLI | 168/209 raw; 206/209 after the same banner removal |
| New CLI cases | 3 parent failures; 3 current/reference exact matches |
| Process probes | 140/140 |
| Official reference/Rust/cache store/cache hit | 17/18 each |
| Official cache outcomes | 18 Stored; 18 fresh-process Hit |
| Build identity | 16/16 |

The earlier compound-expression group now includes thirteen more negated
logical layouts (59 total). New groups compare complete line/count event
strings across nil, false, true, numbers and strings; both sides of and/or;
nested/mixed operators and negations; hook mutation of every parameter at
different visits; and callback-visible old/new named destinations. Additional
checks retain original global lookup order, RHS suppression, weak-root release
and deferred metamethod comparisons with collection. Native export runs all
36 layouts through both interpreters and checks the source reference. Cache
coverage adds a metadata round trip and a fourth valid fresh-process entry
(three text, one native), each stored then hit in a fresh child process.

The parent ordinary CLI was copied before rebuilding and SHA-verified against
the committed 56bc0c2a7 audit. Its new probe intentionally uses --rust-binary;
current hashes describe the case corpus, while that earlier ordinary-build
audit establishes binary provenance. Only logical_shortcut_hook_events,
logical_comparison_hook_events and logical_condition_mutation fail the
supplementary comparison. All three current cases match raw reference stdout,
stderr and exit status. Parent [audit](2026-10-26-rust-lua-boolean-hooks-parent-audit.json) and current
[audit](2026-10-26-rust-lua-boolean-hooks-audit.json) are retained.

The strict current audit exits 1 for 38 intentional version-banner differences.
Supplementary comparison removes at most one exact known leading banner.
The ordinary release CLI build in the isolated checkout runs after its benchmark builds;
focused timing then reads the executables without rebuilding them. Reference
archive SHA remains 2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333.
Current binary/source hashes are verified. The byte-exact contract remains x64
Windows, MSVC /O2 /MT, binary64, 64-bit Lua integers, 32-bit native ints, C locale.

All eighteen official entries retain prior source hashes and unchanged
assertions. Each mode fails only files.lua:59 at the existing Windows open-input
rename; C Lua fails there too. Suite exit 1 remains expected. Bootstrap _U=true,
100,000,000 instructions and 60-second file deadlines remain. Archive SHA is
49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad.
These independent entries do not establish all.lua/main.lua or complete
standalone compatibility; the earlier Windows module/shell failures remain open.

All thirteen source maps match the Lua sources and recorded build inputs.
Both full reference runs
retain 81 runtime and 22 frontend workloads. 0 of the prior 103 layouts
change; both current runs agree. Five cache fixtures, six callback workloads
and paired parent/current/C logical process measurements are retained.
Vm stays 4,440 B, Value 16 B and Frame 64 B. The new compiler/interpreter paths
use safe Rust. Existing isolated native
platform shims remain, with zero normal/build dependencies. [Measurements](../performance/2026-10-26-rust-lua-boolean-hooks.md) retain
selected gains, slow paths and accounting limits. Raw [process](2026-10-26-rust-lua-boolean-hooks-process.json),
[suite](2026-10-26-rust-lua-boolean-hooks-suite.json) and [identity](2026-10-26-rust-lua-boolean-hooks-identity.json) results are retained.
Complete hook/predicate/debug/resource parity, full Lua 5.1, later dialects,
native compilation and gameplay integration remain unfinished. The full goal
stays active.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-boolean-hooks-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-boolean-hooks-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-boolean-hooks-identity.json
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-boolean-hooks-audit.json
~~~
