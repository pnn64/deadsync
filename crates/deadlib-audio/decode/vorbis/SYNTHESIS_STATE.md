# Raw synthesis clocks and PCM queues

Follow-up: [reference seeking](REFERENCE_SEEKING.md) integrates this state into
the indexed reader's explicit reference mode and matches all 172 recorded seek
differences. The rest of this document records the earlier synthesis milestone.

The target is complete parity: exact acceptance, behavior, counts and positions,
with the original PCM bounds. This milestone adds `SynthesisDecoder`, the raw
state required to reproduce libvorbisfile's seek scheduling. **The 172 recorded
seek differences remain unresolved.** `OggSeekReader` still uses its existing
retained-PCM seeking rules; this change does not redefine those differences as
successes. No performance work or application integration is included.

`PacketDecoder` exposes raw overlap blocks. `SynthesisDecoder` additionally
accepts a `PacketTiming` containing packet-number bits, a signed granule position
and EOS. It tracks the reference sequence, granule and overlap-sample clocks,
performs bounded head/tail clipping and exposes borrowed planar PCM through
`pcm()`. `consume()` supports partial reads. Insertion requires the previous
queue to be consumed. The consumed-frame position is separate from declared
granules and track-only advancement.

`track()` reads the same type/mode/window prefix as synthesis, then updates
clocks and block sizes without decoding floors/residues or creating a time
block. This behavior is needed for the reference seek preroll. A discarded
prefix leaves clocks and queued PCM unchanged. Valid input with pending PCM
returns an insertion error without changing the exposed state or PCM.

Restart resets sequence/granule/sample clocks, priming, EOS and the API's consumed
position. It retains the previous block size and PCM backing storage, exactly
as `vorbis_synthesis_restart` does. Ordinary replay primes before exposing PCM;
explicit track/read schedules can reveal retained samples. Direct C comparisons
of the entire buffer caught an initial implementation that incorrectly reset
the previous block size. Constructing a new decoder gives fresh zeroed storage.

Sequence/sample clocks preserve signed 64-bit wrap and the reference's -1
sentinel. Granule clipping narrows through the host's C `long`, including its
32-bit width on this Windows reference build. Nonnegative `consume()` counts
follow the reference signed 32-bit read guard, including its initial -1 priming
cursor. Counts outside the nonnegative i32 range are rejected. Negative C read
requests are outside this public API and are not claimed as covered behavior.

Floor, residue, codebook and IEEE policies remain explicit existing options.
The finite-output policy validates overlap results before changing persistent
PCM or clocks. Required decode memory includes both the raw decoder scratch and
the additional channel-major reference PCM buffer; the aggregate budget is
checked before allocation. The implementation remains safe Rust with no new
production dependencies.

The shared floor/residue/IMDCT path now produces an unwindowed time block.
`PacketDecoder` applies its existing window/overlap afterwards; `SynthesisDecoder`
applies the reference's four block transitions using its queue buffer. A compiled
before/after check against milestone `921cf044` compares raw PCM bits, errors,
discard/truncation decisions, counts and positions across 172 schedules. This
protects the existing raw API separately from reference PCM tolerance checks.

Independent validation uses pinned libvorbis 1.3.7 and libogg 1.3.6. The
`synthesis-ops` C helper calls their unmodified functions. It returns the complete
PCM backing buffer after every operation, including samples outside the exposed
queue. Tests compare every acceptance/insertion/read result, queue cursor,
block size, sequence, granule, sample count and EOS flag exactly. They compare
finite PCM per operation and overall using peak/RMS bounds 3e-6/5e-7, or
5e-5/3e-6 for floor zero, scaled by `max(1, reference peak)`. NaN classes and
infinity signs match; NaN payloads are unspecified.

Procedural schedules cover partial and invalid reads, pending queues, restart
history, discarded/truncated prefixes, all mode-table sizes selected by the
matrix, both floor types, coupling/submaps, all window transitions, legal block
extremes, unusual channel counts, signed counter boundaries, sequence gaps,
granule clipping and finite/nonfinite time blocks. A six-batch offline golden
contains 392 operations, including the exact eight-operation DSP schedule from
known seek case 115. Its request and response match the captured C schedule
byte for byte.

The full synthesis comparisons check 250,635 operations: 88,711 accepted inputs,
16,482 discarded prefixes, 22,635 rejected pending-PCM insertions, 133,738 read
attempts and 22,918 read rejections. Accepted-input counts include inputs whose
subsequent insertion is rejected. They compare 355,093,102 finite buffer values
and 19,415,954 nonfinite classes across 1,342 comparisons.
An independent receipt checker parses all 1,342 complete C responses and matches
these counts, including a diagnostic split by interleaved test progress output.

An additional trace oracle compiles a copied `vorbisfile.c` with observation
hooks, preserving the pinned checkout and ordinary helper. For 70 streams
represented in the known seek inventory, controls and four initial-offset
probes, tracing produces byte-identical ordinary file output. Replaying the
captured operations through unmodified raw C synthesis reproduces 2,515 DSP
states and all 434 returned PCM chunks across 450 seek requests. Native tests
replay those same retained schedules with the numerical bounds above.

A separate experimental controller derives all 450 requests from input bytes
and requested positions, using native Rust synthesis at runtime. It reproduces
the captured DSP schedules byte for byte, exact reported positions/counts and
all 434 PCM chunks within tolerance. This checks the small-search, single-link
control flow independently of replaying preselected operations. It remains an
ignored diagnostic prototype: large bisection, continuation/raw-seek fallback,
chains and production indexed-reader integration are still required.

The new `vorbis_synthesis` fuzz target exercises bounded decode/track/read/drain/
restart schedules under both finite and explicit IEEE policies. Paired contexts
retain identical histories so restart tests account for retained storage. The
fuzz oracle checks determinism and failure preservation; independent C tests
provide the numerical and behavioral reference.

Four 300-second ASAN runs complete 28,482 executions without failure artifacts
or slow inputs. The first streaming run replays the entire earlier corpus and
adds no mutation units. An additional streaming run with 176 C-checked source
seeds completes 5,710 executions and retains 304 new units. The synthesis and raw
packet campaigns complete 11,438 and 8,940 executions respectively. The retained
evidence distinguishes corpus replay from mutation coverage.

The next seek work must reproduce reference page selection, packet numbering,
track-only preroll, queued PCM consumption, cursor updates and repeated/backward
seek history, including broadcast offsets, continuation pages and chains. Passing
raw synthesis tests alone is insufficient to close those differences or claim
complete Vorbis I parity.

The [validation record](validation/2026-10-27-synthesis-state-checks.json) retains
commands, source/corpus hashes, reference provenance, full comparison counts,
the controller prototype, sanitizer results and the earlier coverage assertions.
All 206 release tests, 139 offline tests and three fuzz harness tests pass.
The offline run leaves 83 external/manual tests ignored; the full release run
executes the external tests and skips only manual golden regeneration. Both
codec Clippy configurations and all fuzz targets pass with warnings denied.
