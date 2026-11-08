# Real-packet codebook activity

After the [concealment parity fix](PARITY.md), type-zero residue preflights
codewords before committing a partition. This diagnostic counts actual decoder
calls, so those codewords can appear twice in current call counts. Historical
tables below describe the earlier implementation's call pattern.

VQ decoding accounts for **92.49% / 87.44%** of codebook calls in `in_two` and
`Goin' Under`. Existing prefix tables resolve **83.51% / 85.98%** of calls
directly; **16.34% / 13.88%** continue through the tree. These are actual symbol
and branch counts, not CPU-time shares. Production code is unchanged.

The [stage diagnostic](SYNTHESIS_PROFILE.md) and
[residue follow-up](RESIDUE_PERFORMANCE.md) identify residue as a large synthesis
cost. The earlier isolated Huffman benchmark uses every active entry equally;
that workload does not represent an encoder's actual symbol frequencies. This
diagnostic supplies the missing per-book and per-entry distribution before
another production optimization is selected.

## Method and verification

```powershell
python crates/deadlib-audio/decode/scripts/fetch_corpus.py
python crates/deadlib-audio/decode/scripts/profile_codebooks.py
python crates/deadlib-audio/decode/scripts/profile_codebooks.py --allow-zero-span-granules target/deadlib-audio-reference/corpus/floor0/vorbis_floor_type_0.ogg
```

The runner generates an ignored standalone workspace containing a copy of the
complete current Vorbis source. Unique, checked anchors insert `Cell`/`RefCell`
counters only into the copy's `codebook.rs`. Production crates have no profiling
feature, API, counter or added dependency. Both decoders share the actual Ogg
crate and independently parse their own Vorbis setup. All copied source files
reconstruct exactly from the recorded production source and instrumentation.

Each raw packet is decoded by both actual production and instrumented decoders.
Every PCM value's bits, channel/frame count, raw position, block size, nominal
truncation flag and discarded-packet result must match. Eight files compare
**42,738,816 channel-sample bit patterns**. The six inputs shared with the prior
synthesis diagnostic also reproduce its complete PCM digests and counts.

The seven default files use strict Ogg validation. The separate original
floor-zero run explicitly permits its one known unfinished-page zero granule;
the same bytes are independently rejected without that option. Coyote has
four nominally truncated audio packets, with unchanged PCM and dispositions.
None of these eight traces has a failed codebook read: nominal truncation can
occur in other floor/residue bit fields before a codebook call.

Before every file, synthetic counter checks cover a 32-bit codeword,
continuation edges, a short direct leaf, insufficient lookahead, singleton
decoding with both bit values, and successful/failed scalar and VQ calls.
Counter/entry/length totals are checked again when generating the tables.
Both generated workspaces pass release Clippy; driver formatting, Python
syntax, changed-anchor rejection and repeated-instrumentation rejection pass.
This tooling milestone does not claim a new production sanitizer campaign.

## Observed activity, 2026-10-27

| Input | Codebook calls | VQ calls | Direct prefix | Continue tree | Tree edges per call |
| --- | ---: | ---: | ---: | ---: | ---: |
| Mono fixture | 156 | 76.92% | 87.82% | 8.97% | 0.192 |
| Stereo fixture | 1,277 | 71.26% | 68.91% | 30.93% | 0.576 |
| Eight-channel fixture | 4,831 | 75.26% | 75.51% | 24.36% | 0.355 |
| `in_two` | 7,179,461 | 92.49% | 83.51% | 16.34% | 0.235 |
| `Goin' Under` | 4,538,062 | 87.44% | 85.98% | 13.88% | 0.197 |
| Coyote / floor zero | 2,247,612 | 94.90% | 84.35% | 15.47% | 0.272 |
| aoTuV six-channel | 76,378 | 71.62% | 80.61% | 18.40% | 0.481 |
| Original floor-zero file | 5,336,726 | 96.57% | 86.73% | 13.15% | 0.254 |

Scalar call totals include calls made inside `decode_vector`; the VQ percentage
is that subset, rather than an additional category. The remaining branch share
is short-lookahead fallback near packet tails. No book in these traces uses the
singleton or no-table path. Tree-edge counts include successful one-bit reads
after a continuation or root fallback. Header parsing, raw floor fields,
classification arithmetic, vector scatter, transforms and overlap are outside
these codebook counters.

The hottest `in_two` books are two-dimensional residue VQ books 36 and 37,
each called 1,260,248 times. Book 37 has 121 active entries, a seven-bit prefix
and maximum codeword length eight; its 216,278 eight-bit words take one extra
tree edge. Goin' Under book 31, also two-dimensional residue VQ, has 1,540,192
calls, a five-bit prefix, and 221,578 continuation calls. Of those, 208,557 use
six-bit words. These are concrete candidates for reducing fallback overhead.

## Prefix-width model and limits

The per-book frequency table can predict how many observed successful words
fit a candidate width. Adding one bit to each existing nonzero width, capped
at eight and the book's maximum length, gives:

| Input | Current modeled direct share | One extra bit | Logical prefix elements, before -> after |
| --- | ---: | ---: | ---: |
| `in_two` | 83.66% | 90.30% | 21,888 -> 32,640 bytes |
| `Goin' Under` | 86.12% | 95.27% | 19,584 -> 30,208 bytes |

The model includes all configured books' table elements, including unused
books. It ignores packet-tail lookahead, vector spare capacity, allocator costs,
cache effects and construction time. It predicts neither a speedup nor actual
requested heap. Wider tables need a paired full/stream/startup plus resource and
seek comparison before adoption.

VQ-call frequency also motivates investigating `decode_vector`: successful
Huffman decoding already identifies an active entry, while its current call to
the public random-entry `vector` accessor repeats validation. That invariant
needs a correctness argument and measured pipeline improvement before changing
the implementation. The rejected [refill experiments](REFILL_EVALUATION.md)
show why a plausible inner-loop change is insufficient evidence.

The [follow-up prefix-path experiment](PREFIX_PATHS.md) rejects that direct VQ
shortcut and a stereo scatter specialization, then retains a combined bounded
prefix inlining policy with the original vector checks. Repeated traces match
all eight cases' PCM bits, packet geometry and symbol/counter tables exactly.

Instrumentation allocates per-entry frequency arrays, enlarges codec structs
and adds counter operations. Setup headroom and the expected actual prefix
widths are checked, but instrumented time/memory are deliberately not compared
with production. Role masks describe possible setup references (floor scalar,
floor VQ, residue classword, residue VQ); shared books are not split into
per-call contexts. These traces cover a small corpus, and raw-packet counts
exclude Ogg mapping/trimming and seeking. Broader conformance, native throughput,
sparse codec seeking and encoder quality/rate control remain required.

## Evidence

[Main manifest](validation/2026-10-27-codebooks-main-manifest.json),
[summary](validation/2026-10-27-codebooks-main-summary.tsv),
[books](validation/2026-10-27-codebooks-main-books.tsv),
[symbols](validation/2026-10-27-codebooks-main-symbols.tsv),
[floor-zero manifest](validation/2026-10-27-codebooks-floor0-manifest.json),
[summary](validation/2026-10-27-codebooks-floor0-summary.tsv),
and [checks](validation/2026-10-27-codebooks-checks.json) retain source/input/
generated/worker/table/binary hashes and validation commands. Generated source,
executables and external audio remain in ignored scratch storage.
