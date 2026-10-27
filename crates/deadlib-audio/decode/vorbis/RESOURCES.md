# Allocation, process memory, and arbitrary seeking

The resource benchmark compares the native decoder, Symphonia 0.6.1, and the
pinned libvorbisfile 1.3.7 / libogg 1.3.6 source builds. It changes no production
decoder behavior and adds no production dependency. Run from the repository:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_resources.py
```

Optional positional files replace the default mono, stereo, eight-channel
silence, `in_two (loop).ogg`, and downloaded `goin-under.ogg` inputs. Files must
contain one nonempty, ordinarily trimmed stream with a zero retained timestamp
origin. No legacy repair policy is enabled. The eight-channel fixture is
silence because Symphonia 0.6.1's known dependent-coupling disagreement would
otherwise fail the required seek PCM comparison; see [COVERAGE.md](COVERAGE.md).
This fixture exercises channel count, rather than realistic multichannel work.

## Work and measurement boundaries

Every worker receives resident compressed input. File loading, process launch,
and common request-file parsing are outside timing and allocation windows.

* **Full:** construct a forward-only reader, decode every retained sample, and
  destroy the reader. The requested-heap window includes destruction and must
  return to its starting live-byte count.
* **Startup:** construct a forward-only reader and stop at its first nonempty
  PCM block. Snapshot requested heap before destroying it. Native callbacks can
  continue to the end of that page; the heap snapshot is inside the first PCM
  callback. RSS is a process high-water value, so it includes that page's work.
* **Seek open:** construct and validate a seek-ready reader. Requested heap is
  captured while it is alive; timing includes its destruction. Native validates
  the complete input and now shares one forward Ogg/Vorbis indexing pass (the
  older baselines below used two), while the references use their own default
  seek initialization. These are different startup guarantees; source
  byte/read/seek counts expose that work.
* **Seek batch:** reuse a reader warmed by a full decode and an untimed validated
  request batch. Measure the same 100 boundary/random requests, consuming up to
  256 frames after each seek. Construction, warm-up, and PCM comparison are
  excluded. Every timed subprocess repeats the comparison before measurement.

The first targets are 0, 1, 127, 255, the midpoint, and the final frame, clipped
to the stream. Remaining targets use the documented 64-bit LCG in the runner
with seed 1337. Each request consumes `min(256, frames - target)` frames. The
pinned independent C oracle generates planar reference PCM once for all workers.
It must report the exact seek position and requested frame count. All workers
check each channel's PCM before timing: floor-one peak error <= 3e-6 and RMS
<= 5e-7, scaled by `max(1, reference peak)`. The previously justified floor-zero
tier is peak 5e-5 / RMS 3e-6. No alignment search or fitted tolerance is used.

Native seeks prime the preceding packet and discard to the exact target.
Symphonia seeks one maximum block earlier, resets its codec, and discards using
packet PTS plus trim-start, as its packet-oriented interface requires. Its track
range can include a negative-PTS priming packet even for an ordinary stream;
the benchmark validates retained output positions and counts instead. libvorbisfile
uses `ov_pcm_seek`, checks `ov_pcm_tell`, then requests exactly the wanted frames.

The native public `read_next` processes a whole page, so it may synthesize and
expose far more PCM than requested. `api_exposed_samples` records this effect.
The initial baseline below uses that method. The packet-read update uses
`read_packet`, stopping after one complete overlap block and retaining the
page cursor. Both consumers still discard any packet tail beyond their request.
Symphonia can expose pre-roll and packet tails; libvorbisfile hides its internal
pre-roll behind its read API. This diagnostic is not an equal count of internal
synthesis across libraries. All consumers still require identical PCM windows.
libvorbisfile also defers synthesis initialization beyond `ov_open_callbacks`,
whereas the native constructor already owns its selected packet decoder. Its
seek-open time and retained heap therefore do not include equivalent synthesis
initialization. The warmed seek-batch comparison includes initialized codecs.

## Requested heap and process memory are separate metrics

`bench-alloc` installs a benchmark-only Rust global allocator that forwards to
`System` and tracks successful allocation/reallocation requests. C builds use
`DEADLIB_TRACK_ALLOCATIONS=ON` in a separate build directory, force-including
wrappers only in libogg, libvorbis, and libvorbisfile C translation units.
The C++ harness and input/request storage are outside these wrappers. The
production Rust library does not use this feature or contain allocator hooks.

Both counters report:

* successful allocation/reallocation operations (free operations excluded);
* total requested bytes, counting a successful realloc's entire new size;
* peak additional live requested bytes above the window's initial live bytes;
* signed live requested-byte change at the snapshot.

They track live allocations outside windows too, so freeing or resizing a
preexisting buffer during seeking does not underflow window-local accounting.
The actual instrumented workers self-test zero-fill, realloc, free, and exact
counts before a run. C additionally checks maximum alignment and overflow.
Counters exclude allocator size classes, metadata, fragmentation, stack/alloca,
mapped executable pages, and allocations made inside a system runtime. The C
wrapper's aligned size header is excluded from requested bytes. Rust's heap
scratch and C's stack/alloca scratch therefore have different accounting scopes.
Reported retained heap at startup/open is live reader state, not a leak.

**No instrumented timing or RSS is reported.** Uninstrumented workers separately
query Windows `GetProcessMemoryInfo` / `PeakWorkingSetSize`. Other platforms
currently report null. Each resource case runs in a fresh subprocess. Full,
startup, and seek-open RSS skip the allocation worker's registry warm-up.
Seek-batch RSS includes full warming and golden-window validation. Whole-process
RSS includes input loading and its temporary buffers, the golden workload when
needed, harness, runtime, code pages, and decoder. It is not codec-owned heap or
a baseline-subtracted incremental peak. Input loaders and Rust/static versus
C/dynamic runtime builds differ. Use requested-byte counts for their narrower,
explicit heap scope; neither metric alone describes total codec memory.

Timing uses uninstrumented builds, Rust `Instant` / C++ `steady_clock`, and
resident input. Source callback counters remain enabled in all workers (Rust
relaxed atomics, C ordinary counters). Construction/destruction are included for
seek-open. Seek batches reuse a warmed context. Calibration targets at least
150 ms per trial, capped at 10,000 iterations; at least five trials by default
shuffle engine order using seed 1337. Resource cases use three fresh processes
per phase/engine/instrumentation by default. The runner retains raw resource
rows, timing trials, summary, configuration, source/input/workload/binary hashes,
reference pins, build logs, and completion verification in ignored scratch.

## Results

Initial page-API baseline, measured 2026-10-27 on the shared Windows x64 Xeon E5-2696 v4 / 44 logical
CPU machine as the earlier pipeline reports. Rust uses opt-level 3, LTO, and the
workspace static CRT flags; C uses MSVC 19.44 Release /O2 /Ob2 /DNDEBUG and the
dynamic CRT. CPU frequency, affinity, and other work were not controlled. Five
shuffled timing trials and three resource processes per case produced 150 timing
rows and 360 resource rows. All allocation and I/O counters are identical across
the three process repetitions. RSS varies and is summarized by its median.

Peak additional **requested heap**, full forward decode, KiB (1,024 bytes):

| Input | Native | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: |
| Mono fixture | 96.4 | 1,323.3 | 125.6 |
| Stereo fixture | 372.8 | 1,804.0 | 160.0 |
| Eight-channel silence | 308.4 | 1,696.4 | 265.5 |
| `in_two` | 179.7 | 1,904.5 | 185.6 |
| `Goin' Under` | 394.2 | 1,734.7 | 169.1 |

Native has 205/337/288/391/371 successful allocation/reallocation operations in
these full-decode windows, versus Symphonia's 1,347/1,609/1,584/12,370/12,444 and
libvorbisfile's 314/581/472/626/612. Every full window returns to zero live-byte
delta. Native uses 4.4-13.7x less requested peak heap than Symphonia here, but
does not consistently beat libvorbisfile, and C's stack scratch is excluded.
Startup/open snapshots and requested-byte totals are in the raw resource table.

Warmed 100-request batches allocate **0/0/0/0/3** times for native in the same
input order, **400/600/300/9,799/20,149** for Symphonia, and **zero** for
libvorbisfile. Native's `Goin' Under` batch requests 1,785 bytes in those three
operations, with zero additional requested-heap high water and zero net retained
change. It must not be described as universally allocation-free seeking.
Symphonia's two longer batches request 4.55/4.30 MB cumulatively while adding only
5,392/1,586 bytes of peak live heap; cumulative allocation traffic is not a live
memory peak.

Median times, milliseconds, including all 100 seeks and their requested PCM:

| Input / phase | Native | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: |
| Mono, seek batch | 2.226 | 1.072 | 0.925 |
| Stereo, seek batch | 24.952 | 8.435 | 5.787 |
| Eight-channel silence, seek batch | 24.091 | 11.653 | 11.402 |
| `in_two`, seek open | 33.521 | 2.301 | 0.179 |
| `in_two`, seek batch | 44.604 | 28.935 | 19.795 |
| `Goin' Under`, seek open | 18.508 | 2.550 | 0.217 |
| `Goin' Under`, seek batch | 71.238 | 28.480 | 15.395 |

Native seek batches remain 1.5-2.5x slower than Symphonia and 2.3-4.6x slower
than libvorbisfile on the two longer files. Native `in_two` batch min/median/max
is 42.444/44.604/48.122 ms; `Goin' Under` is 67.973/71.238/71.374 ms. Construction
is also substantially slower and currently scans the entire file twice.
These results demonstrate open performance work despite the smaller requested
heap and allocation traffic.

The longer batches each consume exactly 50,690 channel samples. Native exposes
462,262 / 1,059,258 samples (9.1x / 20.9x demand) through its page callbacks;
Symphonia exposes 642,664 / 639,360. Native reads 0.921 / 0.845 MB per batch,
versus Symphonia's 12.111 / 17.129 MB and libvorbisfile's 7.525 / 8.043 MB.
The dense native index reduces source probing, while page-sized synthesis work
remains a measured cost. Input is memory-resident, so these timings do not
predict relative disk or network behavior.

Native seek-open reads exactly twice the file length: 9,878,028 / 4,700,278 bytes.
Its live requested heap at the ready snapshot is 390,687 / 495,515 bytes, versus
Symphonia's 1,938,223 / 1,763,631 and libvorbisfile's 67,938 / 74,079. Remember
libvorbisfile's deferred synthesis initialization when interpreting this phase.

Median **whole-process** peak working set for full decoding, MiB (1,048,576 bytes):

| Input | Native | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: |
| Mono fixture | 3.828 | 5.617 | 4.230 |
| Stereo fixture | 4.082 | 5.945 | 4.281 |
| Eight-channel silence | 4.078 | 5.805 | 4.445 |
| `in_two` | 12.965 | 12.965 | 12.352 |
| `Goin' Under` | 8.027 | 8.168 | 8.129 |

The longer files' input-loading peaks dominate some process readings. Equal RSS
does not contradict the narrower requested-heap difference. These are process
measurements with the harness/runtime caveats above, not isolated codec RSS.

Evidence: [manifest](validation/2026-10-27-resources.json),
[all resource rows](validation/2026-10-27-resources.tsv),
[seek trials](validation/2026-10-27-seek-trials.tsv),
[seek summary](validation/2026-10-27-seek-summary.tsv),
[completion](validation/2026-10-27-resources-complete.json), and
[validation checks](validation/2026-10-27-resources-checks.json).
All 500 workload requests pass exact count/position and channel PCM checks in
the actual instrumented and uninstrumented workers. Production source is
unchanged; release tests, both Clippy feature configurations, independent offline
package tests/dependency checks, and both allocator self-tests pass.

## Packet-at-a-time seeking update, 2026-10-27

`OggSeekReader::read_packet` synthesizes at most one packet per call, retaining a
cursor into the CRC-verified page. It shares overlap, trim, timing, and error
handling with the existing `read_next` page method. It adds no PCM copy or
synthesis buffer; callers receive a complete borrowed overlap block. Calls may
alternate between both methods. A packet call can return progress without PCM
for priming, a discarded packet, or a spanning fragment. Page granule and EOS
checks finish when the page is consumed; PCM remains provisional until then.

The codec-independent Ogg additions are `PageReader::current_page` and
`PacketCapture::push_page_until`. The latter validates the entire page's
structural/size constraints before its first callback and can pause after a
packet, resuming the identical page without IO, copying or another CRC pass.
Final sequence/EOS/compatibility accounting commits once at page completion.
Completed captures also keep their existing bounded assembly capacity at EOS
(at most one maximum page), avoiding repeated small spanning-packet allocations.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_resources.py --baseline-revision 58ca53b37 --repeats 7
```

This paired run extracts the previous production source into an ignored isolated
workspace. It uses the current workload/verification adapters, changing exactly
one benchmark call to the previous revision's available `read_next` API. The
manifest records this adjustment and every adapter hash. Cargo profiles and all
17 registry artifacts match. Seven shuffled trials interleave both native builds
and the two references; no branch or original source tree is switched.

Median milliseconds per **100-request batch**, with the same reference PCM:

| Input | Native page baseline | Native packet | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: | ---: |
| Mono fixture | 1.959 | 1.682 | 0.947 | 0.830 |
| Stereo fixture | 19.863 | 11.270 | 6.987 | 4.977 |
| Eight-channel silence | 21.810 | 17.060 | 10.283 | 9.667 |
| `in_two` | 47.636 | 32.945 | 31.114 | 22.095 |
| `Goin' Under` | 73.908 | 43.208 | 29.530 | 16.715 |

Observed native speedups are **1.16-1.76x**, including **1.45x / 1.71x** on the
longer files. Native `in_two` packet min/median/max is 26.405/32.945/35.600 ms,
versus 38.536/47.636/48.667 for the page baseline. `Goin' Under` is
39.094/43.208/45.862 versus 63.357/73.908/88.652 ms. This remains a shared,
unlocked machine with reference drift; use the interleaved trials for the
native comparison rather than comparing only against the earlier run above.

On those longer files, exposed PCM falls from 462,262 / 1,059,258 to
132,534 / 135,738 channel samples for the same 50,690 consumed samples. This is
about 2.6-2.7x demand, including whole packet tails. Source IO remains exactly
920,907 / 844,721 bytes and 100 seeks per batch. The diagnostic excludes discarded
pre-roll and cannot be compared to libvorbisfile's hidden internal synthesis.
All five warmed native batches now have **zero allocation/reallocation requests**,
including the historical song's previous three requests. Full-decode and
seek-open requested heap/count measurements remain equal to the initial run.

Seek construction is substantially unchanged: current/baseline medians are
30.609/30.564 ms for `in_two` and 16.939/16.399 for `Goin' Under`, with overlapping
trial ranges. The eager index still reads every byte twice. Native batches still
trail Symphonia by about 1.06x / 1.46x and libvorbisfile by 1.49x / 2.58x on these
files. Packet-sized reads address unused page tails; pre-roll within the indexed
page interval, eager construction, and the remaining synthesis gap need further
measurement and optimization. This is not a competitive-performance claim.

Evidence: [paired manifest](validation/2026-10-27-packet-seek-resources.json),
[280 timing trials](validation/2026-10-27-packet-seek-trials.tsv),
[summary](validation/2026-10-27-packet-seek-summary.tsv),
[360 resource rows](validation/2026-10-27-packet-seek-resources.tsv),
[completion](validation/2026-10-27-packet-seek-complete.json), and
[validation/provenance checks](validation/2026-10-27-packet-seek-checks.json).
All source/input/worker hashes were rechecked at completion. The 500 shared
request windows pass in both native builds and both references.

Validation includes **94 passing release tests** with external differential
checks enabled (only fixture regeneration excluded), all **1,900 corpus seeks**,
**528 upstream vectors / 2,640 packet-read seeks**, historical floor-zero and
explicit legacy endpoint cases, exhaustive page-API short-fixture seek targets, and new
packet/mixed suffix, interrupted-page, span/mux, limit, compatibility and reset
tests. Independent offline tests pass (75 tests; 20 external cases opt-in), as
do Clippy in both feature configurations and the expanded fuzz harness's tests.
Two 300-second sanitizer campaigns complete **23,776 executions**, with no
artifacts: [statistics](validation/2026-10-27-packet-seek-fuzz.tsv),
[results](validation/2026-10-27-packet-seek-fuzz-results.json),
[provenance](validation/2026-10-27-packet-seek-fuzz-provenance.json), and
[initial seed hashes](validation/2026-10-27-packet-seek-fuzz-initial.sha256.json).
All fuzz code hashes still match; its README was updated after the campaign,
which is explicitly recorded rather than rewriting the original provenance.

## Compact packet-mode pre-roll index, 2026-10-27

The default index now retains two logical bits per packet: discarded prefix,
short block, or long block. These come from the already-validated mode prefixes;
index construction still does not synthesize floors, residues, or PCM. Starting
at a page landmark, seek advances exact raw overlap ends by
`(previous_block_size + next_block_size) / 4`, leaving the latest valid packet
whose end does not exceed the target as its pre-roll packet. Discarded prefixes
do not advance overlap. Timestamp conversion uses the existing signed origin,
so initial edits and large broadcast timestamps keep their exact positions.

The mode bytes are flat and use the existing `max_index_memory` accounting,
including vector capacity and containing stream elements. They add no
per-seek allocation. `SeekOptions::packet_preroll` defaults to true; set false
to retain page-only pre-roll when minimizing index storage matters more than
seek work. Both options remain sample-accurate. Payloads preceding the chosen
pre-roll packet are skipped, as seeking already skips earlier pages; the index
does not promise to validate those audio payloads. Full decoding still does.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_resources.py --baseline-revision af518655b --repeats 7
```

Both native builds use the same `read_packet` benchmark adapter here; there are
**no adapter adjustments**. The baseline contains the previous production
source, built independently with matching Cargo profiles and all 17 registry
artifacts. Seven shuffled trials per case yield 280 timing rows alongside 360
resource rows. All workers validate the same 500 reference PCM windows before
timing, and source/input/binary hashes match at completion.

Median milliseconds per **100-request seek batch**:

| Input | Previous packet API | Compact mode index | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: | ---: |
| Mono fixture | 2.035 | 1.601 | 1.121 | 0.995 |
| Stereo fixture | 12.741 | 9.064 | 8.068 | 5.664 |
| Eight-channel silence | 19.340 | 18.866 | 11.799 | 11.388 |
| `in_two` | 31.256 | 15.687 | 28.937 | 19.844 |
| `Goin' Under` | 43.150 | 13.568 | 28.734 | 16.167 |

The two longer tracks improve **1.99x / 3.18x**. Native is faster than both
references on those two workloads in this run, while the three short fixtures
still trail them. Mono/stereo improve about 1.27x/1.41x against the paired native
baseline. Eight-channel silence improves only about 1.03x, with overlapping
ranges (current 18.365-19.591 ms, baseline 18.407-22.580); that small difference
does not establish a reliable speedup.

Native longer-track min/median/max is 14.275/15.687/17.430 ms and
12.485/13.568/14.338 ms. Baseline is 29.012/31.256/32.505 and
42.493/43.150/46.071 ms. Reference drift and uncontrolled machine activity still
apply; this is a paired result on a shared machine, not a broad performance
claim. Full-file throughput and truly cold startup were not rebenchmarked here.

Native source bytes per batch fall from 920,907 / 844,721 to
**541,441 / 484,647** (41.2% / 42.6% less), with 100 seeks unchanged. Read calls
fall from 600 each to 354 / 345. Exposed PCM remains exactly 132,534 / 135,738
channel samples for 50,690 consumed samples, so the improvement removes work
before the target rather than changing the requested output or its packet tail.
This diagnostic still does not count libvorbisfile's internal pre-roll.

All five warmed native batches remain allocation-free. Full/startup requested
heap measurements are unchanged. On each longer track, seek-open live requested
heap rises by **4,224 bytes**: to 394,911 / 499,739 bytes. Construction adds eleven
small vector allocation/reallocation requests per track, totaling 799 / 755
operations rather than 788 / 744. Capacity growth and the stream's new field
are included in those measurements; two logical bits are not an exact allocator
footprint. The short fixtures add 132 live requested bytes each.

Open time remains substantially unchanged: current/baseline medians are
33.637/33.325 ms and 17.600/17.721 ms, with overlapping ranges. Construction still
reads the entire file twice (9,878,028 / 4,700,278 bytes). Faster seek batches do
not remove the substantial native indexed-open gap.

Evidence: [paired manifest](validation/2026-10-27-mode-index-resources.json),
[trials](validation/2026-10-27-mode-index-trials.tsv),
[summary](validation/2026-10-27-mode-index-summary.tsv),
[resource rows](validation/2026-10-27-mode-index-resources.tsv),
[completion](validation/2026-10-27-mode-index-complete.json), and
[validation checks](validation/2026-10-27-mode-index-checks.json).
The **96 passing release tests** include all external differential cases except
fixture regeneration, 1,900 corpus seeks, 528 upstream vectors / 2,640 packet
seeks, floor-zero and explicit legacy trimming. New tests compare both strategies
with edits/large timestamps, discarded packets, spanning audio, interrupted and
mixed reads, and a mode index that must obey the configured memory budget.
Independent offline tests pass (77 tests, 20 optional external/generation cases
ignored), along with Clippy in both feature configurations and fuzz-harness tests.

The resumed streaming sanitizer campaign varies both pre-roll strategies and
completes **6,989 executions** without artifacts. The Ogg and synthesis code is
unchanged from the preceding milestones. All recorded campaign source hashes
still match: [statistics](validation/2026-10-27-mode-index-fuzz.tsv),
[results](validation/2026-10-27-mode-index-fuzz-results.json),
[provenance](validation/2026-10-27-mode-index-fuzz-provenance.json), and
[initial seed hashes](validation/2026-10-27-mode-index-fuzz-initial.sha256.json).

Sparse codec seeking, faster indexing, Ogg-only throughput, truly cold startup,
broader files/platforms, synthesis profiling/optimization, and native encoding
remain open. The existing DeadSync decoding and playback integrations are
unchanged.

## One-pass container and Vorbis indexing, 2026-10-27

`VorbisIndex` now builds header/mode/timing information during the Ogg index's
existing demultiplexing pass. The codec-neutral `OggIndex::read_with_events`
lends assembled packets and then each completing page to a fallible callback;
nil pages still produce page events. Callback errors abort construction, and
events remain provisional until complete physical-stream validation succeeds.
The ordinary `OggIndex::read` delegates to the same implementation.

Vorbis retains the validated setup packet and extracts only its small mode
vector for construction. Codebooks, floors and residues are released immediately
after resource preflight, so multiplexed streams do not retain multiple parsed
setups. Mode/window parsing is shared with the raw decoder. Temporary active
construction state is released before returning the index; retained index budgets
and per-setup/selected-decoder limits still apply. The returned index layout and
nearest-packet pre-roll policy are unchanged.

Construction rewinds once, then reads the input once. It validates all Ogg
CRC/grammar and Vorbis headers; audio indexing reads only mode prefixes.
Floors/residues are validated when synthesized. This remains a complete linear
source scan; index-free codec seeking is still open.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_resources.py --baseline-revision d351a0a49 --repeats 7
```

The runner now builds counted and uninstrumented baseline resource workers
alongside its baseline timing worker. Both native versions use the same current
adapters, matching Cargo profiles and all 17 registry dependency artifact IDs,
with no adapter changes. All instrumented workers self-test their allocators.
Five files yield **480 resource rows** (three trials per group) and **280 timing
rows** (seven trials), with 500 common reference PCM windows checked by every
engine. Default mode also passes a one-file smoke run: 24 resource / 18 timing rows.

Median milliseconds per indexed open, including selected-decoder construction
and destruction:

| Input | Two passes | One pass | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: | ---: |
| Mono fixture | 1.078 | 1.054 | 1.723 | 0.049 |
| Stereo fixture | 2.279 | 2.251 | 2.244 | 0.110 |
| Eight-channel silence | 1.513 | 1.535 | 1.884 | 0.064 |
| `in_two` | 34.956 | 18.799 | 2.400 | 0.166 |
| `Goin' Under` | 19.385 | 11.661 | 2.672 | 0.217 |

Song medians improve **1.86x / 1.66x**. Trial ranges are 18.014-19.704 /
11.103-11.889 ms after versus 32.724-37.912 / 17.850-20.167 ms before. Small-file
open times move roughly -2.2% to +1.4%. This shared, unlocked Xeon/Windows run
has maximum relative timing MAD 11.97%. Native open still substantially trails
the references, whose initialization performs different validation/synthesis work.

All five current native opens read exactly one file's bytes; baseline opens
read two. Song reads fall **9,878,028 -> 4,939,014** and
**4,700,278 -> 2,350,139 bytes**. Read calls fall 6,505 -> 3,253 and
3,337 -> 1,669; source seeks fall three -> two, including final pre-roll selection.
The 17-byte-input regression independently enforces one read per source byte
and only the initial rewind inside index construction.

Retained and peak requested heap match the baseline on all five opens. Song
live/peak heap remains **394,911 / 499,739 bytes**. Allocation requests fall
799 -> 795 and 755 -> 749; total requested bytes fall 893,712 -> 882,039 and
1,110,220 -> 1,085,632. Fresh-process peak working-set medians are essentially
unchanged: 12.957 -> 12.961 MiB and 8.020 -> 8.023 MiB. This establishes no
process-memory improvement. Forward full/startup and warmed-seek heap counters,
source IO and exposed PCM work match the baseline exactly. All five warmed
native batches still allocate nothing.

Seek batches do not remove synthesis work. Their initial timings were mixed,
so a recheck reused the exact saved native binaries and request files for 15
shuffled trials with seed 1338 and the same 150 ms target:

| Input | Initial batch ms, before -> after | Recheck batch ms, before -> after |
| --- | ---: | ---: |
| Mono fixture | 1.311 -> 1.354 | 1.432 -> 1.555 |
| Stereo fixture | 7.524 -> 7.980 | 7.492 -> 7.755 |
| Eight-channel silence | 18.311 -> 18.704 | 16.063 -> 16.037 |
| `in_two` | 16.233 -> 16.252 | 15.768 -> 15.963 |
| `Goin' Under` | 11.990 -> 13.056 | 12.102 -> 11.775 |

The recheck adds **150 timing rows**. Count, PCM, IO and exposed work remain
exact on every invocation. Mono/stereo batch medians regress in both runs;
song movements change between runs and all native trial ranges overlap.
This is an indexed-open improvement with mixed batch timings that remain an
investigation item during synthesis profiling. No general decode-speed or
seek-batch speedup is claimed.

Evidence: [manifest](validation/2026-10-27-single-pass-resources.json),
[resources](validation/2026-10-27-single-pass-resources.tsv),
[summary](validation/2026-10-27-single-pass-summary.tsv),
[trials](validation/2026-10-27-single-pass-trials.tsv),
[recheck summary](validation/2026-10-27-single-pass-batch-recheck-summary.tsv),
[recheck trials](validation/2026-10-27-single-pass-batch-recheck-trials.tsv), and
[checks](validation/2026-10-27-single-pass-checks.json) pin source, input, workload
and worker hashes. Corpora, PCM windows and saved binaries stay under ignored
`target/`; no external media is redistributed.

Validation passes **111 release tests** and **88 extracted offline tests**,
including 38-file raw/full PCM, 1,900 repository-corpus seeks, 528 upstream
vectors / 2,640 packet seeks, historical floor-zero and legacy trimming, and the
native encoding matrix. Numerical tolerances remain unchanged. New tests cover
index-event ordering, nil pages, spans/mux/chains, callback/CRC failures and the
single-read fragmented source. Default/all-feature Clippy, formatting and fuzz
harness tests pass. Resumed container, raw-packet and streaming/indexed sanitizer
targets run 300 seconds each, completing **32,968 executions** without failures
or artifacts: [results](validation/2026-10-27-single-pass-fuzz-results.tsv) and
[provenance](validation/2026-10-27-single-pass-fuzz-provenance.json).

Broader conformance/legacy material, Ogg-only comparative throughput, truly cold
startup, synthesis profiling, index-free codec seeking and competitive encoder
quality/rate control remain open. Neither crate is integrated into DeadSync.

The subsequent [eight-byte CRC comparison](../ogg/PERFORMANCE.md#effect-on-the-vorbis-pipeline)
improves paired song indexed-open medians another 2.81x / 2.01x, with identical
heap, source IO and exposed-PCM counters. Its additional 7 KiB static table
elements are outside requested-heap counters. Ogg-only throughput is now
measured; remaining Ogg allocation/index-seek and codec work stays open.
