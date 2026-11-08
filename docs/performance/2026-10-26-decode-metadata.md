# Direct WAV chunk reads and MP3 sync filtering

Three optimizations in `deadlib-audio-decode`, relative to the production source
at `c4c6ffd11`. These affect format/header probes, not PCM decoding arithmetic.
No production unsafe or public API was added. MP3 probing now directly depends
on the workspace's existing `memchr` 2.8.3 package; no package version changed.

## Execution changes

1. **Walk WAV chunks inside the existing buffer.** Stop once both format and data
   are known, and skip remaining chunks with `seek_relative`. Format parsing has
   already consumed its payload, so only its padding byte needs skipping. This
   removes unnecessary file seeks and buffer refills and deletes `padded_end`.
   The final absolute seek to audio data remains, preserving the decoder's input
   buffer setup. Reversed format/data order and nonzero container origins work
   as before.
2. **Consume unused WAV format extensions directly.** Read at most 40 header
   bytes into the existing stack array, then consume the rest from `BufReader`.
   This removes the temporary vector, zero initialization and payload copy for
   large format chunks. The complete tail is consumed before validating fields,
   retaining truncation-error priority. At EOF, `read_exact` supplies its original
   error instead of a replacement error string.
3. **Filter MP3 synchronization candidates before parsing headers.** Use packed
   `memchr` searches over gaps between `0xff` bytes. Dense candidates advance
   directly without a search call. A mask checks the remaining sync and Layer
   III bits in the second byte before invoking the unchanged header parser.
   Full compatibility checks and first-compatible-pair selection remain intact.
   The disabled Info-silence option still returns before opening a file.

The new cursor and tail-consumption loops replace repeated parsing, allocation,
copying and filesystem operations. They add no buffering layer or new production
helper function. WAV header storage remains the existing 40-byte stack array;
extension payloads use the reader's existing buffer.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.99.1 (`58cf8f9c3`,
LLVM 23.1.1). Release test builds use opt-level 3, full LTO, the static MSVC
runtime and the default target CPU. Registry and platform dependencies stay
cached across the final four builds.

| Stage | Production changes |
| --- | --- |
| `before-final` | Original WAV and MP3 source |
| `walk-final` | WAV chunk walk only |
| `tail-final` | Also direct WAV format-tail consumption |
| `after-checks` | Also MP3 sync filtering; full final release build |

Every candidate was measured before editing and after building. The final
comparison uses identical test sources, six rotating process runs and
42 workloads per stage. The existing harness pins its thread to logical CPU 2,
reads Windows `QueryThreadCycleTime`, warms up once, and reports the median of
eleven measured batches. CPU throughput is before cycles divided by after cycles.
Fixtures and allocation tracking are outside the cycle measurements. Whole-file
probes include file opening, buffer allocation and closing. These are warm-file
measurements and do not establish equivalent end-to-end audio decode speedups.

## Results

Medians of six process results, comparing each change with its immediate
predecessor stage:

| Operation | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| WAV walk, format before data, no junk | 29032.28 | 17384.01 | 1.670x |
| WAV walk, 17-byte junk chunk | 41965.00 | 20499.54 | 2.047x |
| WAV duration query, 17-byte junk chunk | 196118.42 | 177031.58 | 1.108x |
| Buffered 4 KiB WAV format | 579.49 | 101.25 | 5.723x |
| Buffered 16385-byte WAV format | 2177.89 | 133.51 | 16.312x |
| File-backed 16385-byte WAV format | 12928.00 | 11097.26 | 1.165x |
| MP3 scan, 4 KiB sparse junk before Info | 30189.37 | 227.62 | 132.631x |
| MP3 scan, 4 KiB dense false syncs | 33867.01 | 8867.83 | 3.819x |
| MP3 scan, 24 KiB sparse junk before Info | 177692.74 | 705.54 | 251.855x |
| MP3 file probe, 24 KiB junk before Info | 374878.89 | 202380.65 | 1.852x |

Normal WAV walk pairs improve 1.435-1.693x; buffered 16385-byte format pairs
improve 14.790-17.273x. The whole-file MP3 probe with 24 KiB junk improves
1.765-1.941x in all six pairs. Sparse scan speedups concern the bounded header
scan; they do not apply to normal MP3 decoding. A normal enabled MP3 file probe
measures 185180.84 -> 186480.07 cycles (0.993x); disabled probing remains about
13 cycles and performs no file I/O or allocation.

Both 4096-byte and 16385-byte format chunks go from one temporary allocation
and free per parse to zero, in both buffered and file-backed cases. The removed
allocation is the full chunk size. Other probes retain their allocation counts:
reused-reader WAV walks and MP3 slice scans allocate nothing; complete duration
and enabled MP3 file probes retain their file-path and reader/prefix allocations.
The TSV contains all **1008 cycle, throughput and allocation measurements**.

Some controls regress. The large reversed-order WAV walk is 41471.45 -> 43229.38
cycles for the walk change (0.959x); original-to-final process ratios are
0.899-0.980x. A zero-junk Xing scan is 95.77 -> 105.84 cycles (0.905x for the
scan change; process ratios 0.808-1.060x). The extra fixed work has a small cost
when scanning is unnecessary. The normal complete MP3 file probe stays within
about one percent at the median; no gain is claimed for those controls.

An initial `memchr_iter` trial was rejected: calling the search iterator for
every dense `0xff` candidate slowed the 24 KiB false-sync case to about 0.416x.
The retained cursor and second-byte mask improve that workload by 3.772x instead.
Trial measurements remain in `target/audio-decode-metadata/scan.tsv`.

The two production files add 43 lines and remove 18, including deletion of the
old padded-end helper and large-format vector branch. The added cursor/mask
logic removes full header parsing on rejected bytes; the WAV changes remove
intermediate storage and redundant filesystem operations.

## Behavior checks

The MP3 scan is compared with its frozen original across offsets, truncation,
dense false syncs, random bytes and all possible second header bytes with several
first/third-byte combinations. Benchmarks also check their result against that
reference before timing. Existing MP3 fixture tests retain PCM hashes, seek
positions and option-dependent timelines.

The WAV walk is compared with a frozen original for normal and reversed chunk
order, odd padding, large junk chunks, truncated files, container origins zero
and seven, and buffer capacities 1, 32 and 8192. It compares metadata, error text,
stream positions and remaining bytes. Existing format-parser checks compare
small and large chunks, invalid fields, truncated tails and error priority with
their original implementation. Existing decoding tests cover all WAV encodings,
packet boundaries, truncated packets and seeks.

All **66 final release tests and 66 debug tests pass**, with 18 opt-in benchmarks
ignored normally. Each of the three intermediate archived stages passes its
19 probe/parser tests. Clippy completes with existing duplicate-module and
type-complexity warnings; formatting, Python compilation and whitespace checks
pass.

After measurement, three needless closure borrows were removed from allocation
probes to clear new benchmark-only Clippy warnings. The timed closures and timed
calls are unchanged. The audit verifies that those three calls are the entire
harness difference, and all 13 tests in the rebuilt metadata test target pass.
The exact measured harness remains at
`target/audio-decode-metadata/measured-metadata_probes.rs`.

## Reproduction and evidence

Use isolated checkouts with the final tests, shared support, dependency manifest,
lockfile and configuration held constant. Start from the original `wav.rs` and
`mp3.rs` at `c4c6ffd11`, then apply the three changes above one at a time. Build
and archive each intermediate stage with `--test metadata_probes --test
wav_format_kernel`; build the final stage with all crate tests:

```powershell
cargo test -p deadlib-audio-decode --release --no-run --message-format=json > target/decode-metadata-build.jsonl
python scripts/benchmark_audio_decode.py --root target/audio-decode-metadata --capture after-checks --build-log target/decode-metadata-build.jsonl
python scripts/benchmark_audio_decode.py --root target/audio-decode-metadata --stage before-final --stage walk-final --stage tail-final --stage after-checks --target wav-walk --target wav-length --target wav-format --target mp3-scan --target mp3-file --runs 6 --output docs/performance/2026-10-26-decode-metadata.tsv
cargo test -p deadlib-audio-decode -- --test-threads=1
cargo clippy -p deadlib-audio-decode --all-targets
```

[All measurements](2026-10-26-decode-metadata.tsv),
[source/configuration and executable hashes](2026-10-26-decode-metadata.manifest.json),
and [validation results](../testing/2026-10-26-decode-metadata.json) record the
evidence. Archived executables, intermediate source snapshots, process logs and
rejected trials remain under `target/audio-decode-metadata`.
