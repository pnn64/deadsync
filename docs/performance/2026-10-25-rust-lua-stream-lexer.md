# Resumable scanner and frontend scaling

Fixed-source compilation now shares a resumable byte scanner. The harness adds
six compilation workloads: long comments and quoted string payloads of 10 KB,
100 KB and 1 MB. Both engines execute the resulting chunk and check its result
before timing. Source construction is outside measurements; both use the chunk
name `=(frontend)`. Compilation includes program/function destruction.

Each workload has five paired wall time samples in alternating engine order.
The reference is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT,
in the C locale. Ratios below are medians of paired Rust/reference durations;
lower is better. Payload sizes are decimal byte counts, excluding source wrappers.

| Payload | Initial scanner | Bulk scanning | Confirmation |
| --- | ---: | ---: | ---: |
| Comment, 10 KB | 0.836 | 0.432 | 0.397 |
| Comment, 100 KB | 0.789 | 0.308 | 0.302 |
| Comment, 1 MB | 0.877 | 0.286 | 0.282 |
| Quoted text, 10 KB | 0.451 | 0.291 | 0.281 |
| Quoted text, 100 KB | 0.643 | 0.268 | 0.251 |
| Quoted text, 1 MB | 0.615 | 0.303 | 0.303 |

The initial scanner saved ordinary literal bytes one at a time. The final path
copies runs up to a quote, escape or newline, and skips comment runs up to a
closing delimiter candidate or newline. Special characters still use resumable
state. These are safe slice operations and preserve the chunk-boundary tests.

For the 1 MB cases, paired ratios improve about 3.1-fold for comments and 2-fold
for quoted text. Median Rust comment compilation drops from 2.351 ms to 0.567 ms
in the bulk run and 0.724 ms in confirmation; quoted compilation drops from
4.080 ms to 1.519/1.904 ms. Reference timings also vary materially, so raw
before/after absolute times overstate the isolated change. The machine is not
CPU-isolated; paired ratios are more useful here.

For a 1 MB quoted payload, requested Rust allocation bytes per compilation fall
from approximately 4.10 MB to 2.00 MB as bulk copying avoids repeated geometric
string-buffer growth. These counters sum allocation/reallocation requests, not
live bytes or memory traffic. Comment parsing does not allocate its payload.
Raw counts and all six scaling curves are retained in the reports.

The reports also retain the existing twenty execution workloads, startup/drop,
small-chunk compilation and heap estimates. Most runtime workloads remain slower
than the reference; these frontend measurements do not establish an overall
engine speed improvement. The scanner's one-byte-delivery behavior is exercised
by tests; these timings use complete fixed source.

- [Initial resumable scanner](2026-10-25-rust-lua-stream-lexer.json)
- [Bulk scanning](2026-10-25-rust-lua-stream-lexer-bulk.json)
- [Confirmation](2026-10-25-rust-lua-stream-lexer-confirm.json)

Each JSON has matching TSV/build logs and source/build/machine metadata. The
initial report stores the new compilation samples without a compilation-ratio
summary; the final runner checks all six paired compilation workloads and stores
that summary. Source hashes in both final reports match the final implementation.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

Instrumentation counts Rust `GlobalAlloc` calls; direct native C malloc is
excluded. Retained heap estimates are capacity-based, not exact allocator peaks
or RSS. Reader-based loading still needs parser resumption, protected callbacks
and end-to-end validation. See the
[scanner validation](../testing/2026-10-25-rust-lua-stream-lexer.md) for coverage.
