# Residue scatter optimization

The type-two residue scatter now advances channel/bin cursors through the
logical interleave. Paired full-file song medians improve **1.24x / 1.03x**;
4 KiB reads improve **1.09x / 1.10x**. Warmed seek-batch medians improve
**1.07x / 1.03x**. Small-file results remain mixed, with overlapping trial ranges.
Native throughput still trails Symphonia on these songs.

## Change and correctness boundary

The [real-packet diagnostic](SYNTHESIS_PROFILE.md) measured residue at about
55% / 42% of recorded song stage time. The old scatter computed quotient and
remainder by the runtime bundle size for every coefficient. The new loop
computes them once at each partition's start, increments the channel cursor,
and advances the bin only when the channel wraps. The cursor persists between
VQ vectors within the partition, including vectors that start between channels.
Type-one scatter also hoists its fixed plane offset out of the vector loop.

The logical interleave follows [Vorbis I section 8.5](https://xiph.org/vorbis/doc/Vorbis_I_spec.html).
Codeword reads, complete-vector preservation on truncation and per-bin float
addition order are unchanged. Type-zero scatter, arithmetic precision, memory
layout/budgets and public APIs remain unchanged. No dependency or unsafe/SIMD
code is added. A synthetic test compares the cursor to the direct interleave
formula using reversed bundles up to 255 entries, starts between channels,
partitions crossing vector boundaries, and empty/eight-bit/sixteen-bit tails.
It checks every output cell and exact consumed bits after end-of-packet.

## Paired pipeline and seek measurements, 2026-10-27

```powershell
python crates/deadlib-audio/decode/scripts/benchmark.py --baseline-revision 29e8cc20c --repeats 7
python crates/deadlib-audio/decode/scripts/benchmark_resources.py --baseline-revision 29e8cc20c --repeats 7
```

Both native builds use current adapters with no adjustments, matching Cargo
profiles and all 17 registry artifact IDs. Reference pins and numerical gates
are unchanged. This is the shared, unlocked Xeon/Windows desktop: seven shuffled
trials, requested 150 ms minimum with the existing iteration cap. The five-file
pipeline retains 420 trials; resources retain 480 rows plus 280 seek/open trials.
All engines validate identical counts; seek workers check the same 500 reference
PCM windows. The known dependent eight-channel Symphonia discrepancy remains
separate; the resource suite uses eight-channel silence as before.

Median milliseconds:

| Input / phase | Native before | Native after | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: | ---: |
| `in_two`, full | 381.354 | 308.447 | 208.761 | 342.207 |
| `in_two`, 4 KiB reads | 326.486 | 299.654 | 193.842 | 317.610 |
| `in_two`, first PCM | 0.766 | 0.748 | 1.757 | 0.655 |
| `Goin' Under`, full | 281.282 | 273.794 | 176.262 | 254.081 |
| `Goin' Under`, 4 KiB reads | 324.964 | 295.581 | 185.394 | 259.208 |
| `Goin' Under`, first PCM | 1.159 | 1.146 | 2.097 | 0.668 |

The two native full-file ranges overlap their baselines: 283.577-365.692 versus
315.569-392.661 ms, and 258.462-334.148 versus 278.358-347.391 ms. Maximum relative
pipeline MAD is 7.45%. Native full decoding is 1.48x / 1.55x slower than Symphonia
on these medians. It is about 1.11x faster than libvorbisfile for `in_two` and
1.08x slower for Goin' Under. This is not a cross-platform ranking.

Small-file full/fragmented/startup movements go in both directions. Mono first
PCM rises 0.566 -> 0.606 ms; its full decode rises 0.625 -> 0.642 ms. Stereo full
decode improves 1.342 -> 1.315 ms while fragmented reads rise 1.288 -> 1.332 ms.
All individual results and spreads remain in the retained tables.

Median milliseconds per 100-request warmed seek batch:

| Input | Native before | Native after | Symphonia | libvorbisfile |
| --- | ---: | ---: | ---: | ---: |
| Mono fixture | 1.298 | 1.296 | 0.917 | 0.817 |
| Stereo fixture | 7.510 | 7.722 | 7.495 | 5.279 |
| Eight-channel silence | 16.209 | 15.725 | 11.047 | 10.589 |
| `in_two` | 15.008 | 14.084 | 28.208 | 21.233 |
| `Goin' Under` | 11.159 | 10.824 | 26.978 | 14.086 |

Song native ranges are 12.794-15.051 / 10.178-12.487 ms after versus
14.861-16.295 / 10.793-13.055 ms before. Stereo regresses about 2.8%; mono is
essentially unchanged. Seek/open groups have maximum relative MAD 10.15%.
Construction has no intended algorithm change: song open medians move
16.719 -> 16.748 / 11.130 -> 10.825 ms. Small-file and overlapping movements
remain part of the evidence, not a universal speedup claim.

All paired native allocation, requested/peak/live heap, source IO and exposed
PCM fields match exactly across every resource group and all three trials.
Every warmed native batch remains allocation-free. Indexing still reads one
full file. The update adds only local cursor values, with no retained storage.

## Diagnostic follow-up and evidence

The six-file diagnostic again checks 32,343,680 current-production PCM values
bit for bit against its instrumented copy. Geometry, discard/truncation counts
and 64-bit FNV PCM digests also match the pre-change run on every file. This
digest comparison is not an exhaustive before/after PCM proof.
Recorded song residue medians fall 207.082 -> 147.624 ms and
149.430 -> 105.173 ms; stage shares fall to 49.8% / 36.9%. Those separate
instrumented runs change layout and include machine drift, so the paired
uninstrumented comparisons above are the throughput evidence. The unchanged
floor-zero path has a different dominant cost; its explicit container policy
still records one legacy zero-span exception, with none on the other five files.

[Pipeline summary](validation/2026-10-27-residue-cursor-pipeline-summary.tsv),
[420 trials](validation/2026-10-27-residue-cursor-pipeline-trials.tsv),
[manifest](validation/2026-10-27-residue-cursor-pipeline.json),
[seek summary](validation/2026-10-27-residue-cursor-seek-summary.tsv),
[280 trials](validation/2026-10-27-residue-cursor-seek-trials.tsv),
[resources](validation/2026-10-27-residue-cursor-resources.tsv),
[resource manifest](validation/2026-10-27-residue-cursor-resources.json),
[profile](validation/2026-10-27-residue-cursor-profile.json), and
[validation checks](validation/2026-10-27-residue-cursor-checks.json) pin the data,
source/input/adapter/archive/binary hashes and PCM comparison scopes. Media,
PCM windows, source extracts and saved binaries remain under ignored `target/`.

Validation passes **112 release tests** and **89 extracted offline tests**:
all-size direct/independent transforms, 38-file raw/full PCM, 1,900 corpus seeks,
528 upstream vectors / 2,640 packet seeks, historical floor-zero, explicit legacy
trimming, the 144-case encoding matrix and warmed allocation checks. No PCM
tolerance changes. Formatting and default/all-feature Clippy pass. Resumed raw
packet, streaming/indexed and native encoding sanitizer targets run 300 seconds
each, completing **18,638 executions** without failures or artifacts:
[results](validation/2026-10-27-residue-cursor-fuzz-results.tsv) and
[provenance](validation/2026-10-27-residue-cursor-fuzz-provenance.json).

Broader conformance and platform coverage, competitive synthesis throughput,
Ogg-only measurements, cold startup, index-free codec seeking and encoder
quality/rate control remain open. The libraries remain independent of DeadSync.
