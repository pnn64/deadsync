# Compact first-stage entropy coding

The initial [encoding baseline](ENCODING_PERFORMANCE.md) measured an eight-bit
first-stage VQ alphabet whose four central pairs accounted for 89–95% of symbols
in two-stage song encoding. The encoder now gives those pairs three-bit words.
On the same five-second excerpts, two-stage files are **26.45% / 28.70% smaller**
with bit-identical PCM through native, libvorbisfile and Symphonia. The quantizer,
floor endpoints, stage vectors, channel order and retained frame counts are
unchanged. Encoding remains lossy relative to its source PCM.

## Representation and memory

The first two-dimensional radix-16 VQ book uses a complete entry-order tree:

| Pair entries | Length | Count |
| --- | ---: | ---: |
| 119, 120, 135, 136 (digits seven/eight in each component) | 3 | 4 |
| 0, 15, 240, 255 (corners) | 8 | 4 |
| Remaining entries | 9 | 248 |

The Kraft sum is `4/2^3 + 4/2^8 + 248/2^9 = 1`. Setup writes an unordered,
nonsparse length list; all VQ lookup values retain their original identities.
The classifier and later-stage books keep their ordered one-/eight-bit trees.
This follows the entry-order and unordered-book grammar in section 3 of the
[Vorbis I specification](https://www.xiph.org/vorbis/doc/Vorbis_I_spec.html).

The encoder generates its LSB-first codewords at compile time by choosing the
lowest available prefix in a nine-bit left-edge domain. One shared 512-byte
read-only table packs word and length into u16 entries. Construction performs
no runtime table building or per-instance table allocation. The packet loop
selects the compact first-stage path outside its pair loop; later-stage writing
retains the fixed eight-bit path.

The worst-case packet reservation includes nine-bit first-stage words, even
though typical payloads shrink. Other symbol distributions can grow first-stage
payloads by up to 12.5% relative to the fixed-eight-bit book. At default
stereo/N1024/three-stage settings,
owned raw encoder elements rise **29,401 -> 29,623 bytes**, a 222-byte increase:
158 setup bytes plus 64 bytes of worst-case packet capacity. The four-stage
setup grows by 159 bytes due to final bit alignment. Separate
[before](validation/2026-10-27-encoder-entropy-storage-before.tsv) /
[after](validation/2026-10-27-encoder-entropy-storage-after.tsv) probes record
four precision settings at mono/N64, stereo/N1024, eight-channel/N8192 and
255-channel/N512. These are retained buffer capacities, excluding allocator,
struct, transient constructor, Ogg and caller storage. The shared table is
separate from those per-encoder counts.
The small [probe source](validation/2026-10-27-encoder-entropy-storage-probe.rs)
calls the public storage/setup getters for the recorded configurations.

Normal raw push/finish/reset still perform zero allocations after construction,
including dense four-stage 255-channel input. Memory and packet policies
continue to reject configurations whose reserved worst case exceeds the limits.

## Paired size and CPU measurements

Baseline production sources are pinned to `4c1565220` (the same encoder as
`0e5ecb23d`). Current and baseline builds use identical current benchmark adapters,
matching Cargo profiles and all 17 registry artifacts. No adapter adjustment is
needed. Every one of the baseline's 24 native files reproduces the prior
committed output SHA-256 exactly. Input PCM and original media hashes also match
the preceding baseline.

The existing six inputs and four native/three Xiph settings are retained. Four
baseline native settings add paired measurements: **66 independently decoded
outputs and 462 shuffled timing rows**, seven trials per configuration. Input is
resident interleaved f32 in 4,096-frame pushes; each measured iteration constructs
and finishes a fresh encoder and Ogg mux into a discard sink. File IO is outside
the timers. The [manifest](validation/2026-10-27-encoder-entropy-benchmark.json)
records source/binary/input hashes, profiles, toolchains, phases and policies.
The original [timing definitions](ENCODING_PERFORMANCE.md) still apply.

| Five-second excerpt | Stages | Before bytes | After bytes | Reduction | Full ms before -> after |
| --- | ---: | ---: | ---: | ---: | ---: |
| in_two | 1 | 46,874 | 32,319 | 31.05% | 9.581 -> 9.427 |
| in_two | 2 | 216,856 | 159,500 | 26.45% | 11.176 -> 11.093 |
| in_two | 3 | 455,861 | 370,626 | 18.70% | 13.838 -> 13.681 |
| in_two | 4 | 730,502 | 625,768 | 14.34% | 16.902 -> 16.582 |
| Goin' Under | 1 | 35,218 | 23,928 | 32.06% | 9.235 -> 8.830 |
| Goin' Under | 2 | 257,950 | 183,907 | 28.70% | 11.736 -> 11.165 |
| Goin' Under | 3 | 567,538 | 455,229 | 19.79% | 14.778 -> 14.262 |
| Goin' Under | 4 | 786,210 | 668,935 | 14.92% | 17.445 -> 16.627 |

Observed song full-time medians fall 0.75–4.86%. The largest relative full-time
MAD across all configurations is 5.18%, so small timing differences should be
read with that variation. Size reductions are exact file-byte comparisons.
These paired trials establish the cost of this representation change; native
and Xiph quality controls still describe different rate-distortion points.

Startup/header costs increase in most cases. For two-stage in_two, open rises
31.58 -> 36.62 microseconds. Tiny mono/stereo one-stage fixtures grow
376 -> 510 and 606 -> 709 bytes; their payload savings do not repay the larger
setup. Eight-channel silence grows by 158–160 bytes across the stages. The
retained [summary](validation/2026-10-27-encoder-entropy-summary.tsv) includes
all short-file cases, open/first-packet timers, ranges and MAD.

## Unchanged symbols and PCM

The profiler accepts the original fixed-eight-bit and the compact first book.
It derives word assignment using an independent exhaustive prefix-occupancy
oracle, consumes all packets through zero padding, and hashes decoded floor
flags/endpoints, classifications and every VQ entry in packet order. All 24
paired native configurations have identical symbol hashes. Actual first-stage
bit counts confirm the projected coding savings; complete file reductions also
include changed lacing, rounding and page counts. See the
[symbol rows](validation/2026-10-27-encoder-entropy-symbols.tsv) and
[analysis provenance](validation/2026-10-27-encoder-entropy-symbols.json).

All 24 pairs compare **3,658,112 channel samples per decoder bit-for-bit**.
Native and Symphonia compare f32 bit patterns; independent libvorbisfile output
protocols compare byte-for-byte. The [exact report](validation/2026-10-27-encoder-entropy-exact.json)
records each case. The larger 66-output gate checks **10,059,808 channel samples
per decoder** with exact counts/rates/layouts and the existing numerical bounds.
No alignment, gain fitting or tolerance change is used.

All legal block sizes, selected channel counts through 255, four stages, empty/one/partial lengths,
mixed silence, reset/chunk determinism and spanning packets remain covered.
Internal tests encode/decode every leading word and check exact consumed lengths.
Independent libvorbis codebook tests verify every book's lengths, all exactly
representable VQ values and all reference-encoded entry words. Prefix uniqueness
connects those complete enumerations to the same entry-order representation.

The older 65,810-byte-per-packet stream that triggered libvorbisfile 1.3.7's
PCM-seek-zero loss is now a pinned synthetic fixture. Smaller current packets
would otherwise remove that trigger. Its bytes were reproduced exactly from
the old native source, and both reference seek paths remain checked. See the
[fixture provenance](tests/fixtures/README.md).

## Regression gates and reproduction

* 108 release tests pass with external reference/corpus cases enabled; only the
  explicit golden regeneration utility is excluded. This includes the 528-file
  pinned upstream grid, repository songs, historical media and seek checks.
* An extracted offline workspace passes 85 tests; 24 external/generator cases
  stay ignored. Default/all-feature Clippy and fuzz harness checks pass.
  Raw warmed encoder allocation counts remain zero.
* Two concurrent Windows AddressSanitizer targets run for a 300-second budget
  each: `vorbis_encode` 3,120 executions, `vorbis_stream` 4,260. Both exit zero
  with no artifacts, sanitizer reports, assertion failures, timeouts or RSS-limit
  failures. Peak sanitizer RSS is 484 / 457 MiB, including harness and sanitizer
  overhead. This bounded campaign supplements the independent parity tests.

A further 300-second streaming/seeking campaign resumes the retained corpus and
adds four compact native mono/stereo/silence/effect files as explicit seeds.
It completes 7,271 executions, exits zero and produces no artifacts or failures;
peak sanitizer RSS is 491 MiB. The three target runs total **14,651 executions**.
Its [results](validation/2026-10-27-encoder-entropy-seeded-fuzz-results.json) and
[provenance](validation/2026-10-27-encoder-entropy-seeded-fuzz-provenance.json)
record the generated seed hashes. All recorded source/binary hashes are rechecked.

[Validation/source hashes](validation/2026-10-27-encoder-entropy-checks.json),
[sanitizer results](validation/2026-10-27-encoder-entropy-fuzz-results.json) and
[sanitizer provenance](validation/2026-10-27-encoder-entropy-fuzz-provenance.json)
retain the evidence. Corpora, executables and generated media remain in ignored
`target/`. No external song is redistributed.

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_encoding.py --run-root target/deadlib-audio-reference/entropy-new --baseline-revision 4c1565220 --seconds 5 --repeats 7 --min-seconds 0.15
python crates/deadlib-audio/decode/scripts/profile_encoding.py target/deadlib-audio-reference/entropy-new
```

The same histograms identify a further candidate: a dimension-one book with
digit eight at length one, seven at length two, zero/fifteen at length five and
the other twelve at length six. Its Kraft sum is
`1/2 + 1/4 + 2/32 + 12/64 = 1`. Coding two scalar entries per current pair would
save a projected additional 4,908 / 5,796 first-stage payload bytes in the
two-stage songs and reduce length-list overhead. The subsequent
[scalar-book evaluation](SCALAR_BOOK_EVALUATION.md) validates that representation
and measures all three decoders. Its default-stage size gain is only 0.8–0.9%
while native song decoding slows 12–15%, so the pair book remains the production
default. The experimental source and exact-PCM comparisons are retained.

Adaptive floors, transient blocks, channel coupling, perceptual/rate control,
broader encoder resource/quality comparisons, decoder throughput/startup work
and sparse seeking remain in the original goal. This changes the native
encoder's packet representation without integrating either codec into DeadSync
or changing production decoder math.
