# Recorded production costs

These are the original DREAMING-ING!! preparation and live workloads, using the
preserved native baseline and final Rust executables. Inputs and executable
hashes are retained in the [paired run](production-loader-matched-v1/result.json).
Order was native, Rust, Rust, native for each workload. Each preparation process
ran three iterations; each live process ran 481 frames with 60 warmup frames.

| Observation | Native baseline | Rust |
| --- | ---: | ---: |
| Median preparation sample | 1.340 s | 0.611 s |
| Mean preparation thread cycles | 2.960 billion | 1.351 billion |
| Median preparation process peak RSS | 76.846 MiB | 32.969 MiB |
| Median preparation process peak commit | 68.246 MiB | 25.744 MiB |
| Median live sample | 16.853 ms | 16.376 ms |
| Mean live sample thread cycles | 37.166 million | 36.015 million |
| Median live process peak RSS | 87.348 MiB | 53.486 MiB |
| Median live process peak commit | 105.287 MiB | 71.287 MiB |

There are six preparation samples and 842 measured live samples per backend.
All runs completed their functional guards: unchanged original inputs, release
builds, expected frame counts, successful startup and no recorded runtime errors.
Memory is whole-process OS high-water working set/commit, including assets and
the surrounding tool. It is not a Lua-only heap measurement. Allocation-count
ratios are omitted because the native allocator is outside Rust GlobalAlloc.

No compiler was detected at preparation run boundaries. An unrelated kernel
build restarted during the live sequence: compilation was recorded after the
first Rust live run and at both boundaries of the remaining Rust/native runs.
The [strict repeat](production-loader-matched-v2/environment.json) refused to
start while that build remained active. No further retries or process stops
were used. Live timings are qualified observations under background build load,
not a controlled performance comparison. Ordinary system load and frequency
changes also remain uncontrolled. Independent non-Lua changes in the preserved
application snapshots prevent attributing every difference solely to the VM.
No LuaJIT or optimization target is implied.

## Paired bundled noteskins

The [noteskin run](production-loader-noteskin-matched-v1/summary.json) covers all
26 bundled dance/pump cases with three samples per backend per case. Every
complete output matches; only the deliberately changed cache identity is
normalized from version 29 to 30. The comparator executable is unchanged from
the earlier accepted noteskin build.

| Sum of each case's sample median | Native | Rust | Rust/native |
| --- | ---: | ---: | ---: |
| Wall time | 0.282046 s | 0.336903 s | 1.19449 |
| Thread cycles | 617.021 million | 736.836 million | 1.19418 |

No compilation was detected at this run's recorded boundaries. This is a paired
preparation comparison, not a general VM benchmark. It reports neither
allocation counts nor noteskin-specific peak memory.
