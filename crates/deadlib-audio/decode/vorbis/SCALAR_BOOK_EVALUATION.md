# Scalar first-book evaluation

The scalar-book candidate is **rejected as the production default**. It preserves
decoded PCM and reduces setup memory, but its default three-stage song files
save only 0.8–0.9% while native decoding takes 12–15% longer. The committed
[pair book](ENCODING_ENTROPY.md) remains unchanged. This milestone adds decoder
cost measurement and preserves the experiment so the decision is reproducible.

## Candidate and functional checks

The candidate replaces the leading two-dimensional, 256-entry VQ book with a
one-dimensional, 16-entry book. Digit eight uses one bit, seven two bits,
zero/fifteen five bits and the other twelve digits six bits. The complete-tree
sum is `1/2 + 1/4 + 2/32 + 12/64 = 1`. A shared 512-byte encoder table packs two
scalar words into one writer call. Quantization, floor values and later-stage
books stay unchanged, while the first-stage decoder reads two entries per
previous pair.

The [candidate patch](validation/2026-10-27-scalar-evaluation-candidate.patch)
applies to native encoder source at `77747ae52` with `git apply --unidiff-zero`.
It uses a context-free diff so the retained artifact has no blank-context
trailing spaces. Its source hash and reproduction
checks are in the [validation manifest](validation/2026-10-27-scalar-evaluation-checks.json).
The patch is an experimental artifact; it is not applied to production source.

Focused tests passed all 256 packed scalar pairs, exact word lengths, warmed
allocations, libvorbis codebook values/words, 144 independently decoded encoder
cases, native/Symphonia parity, seeking and write failures. The measured six-file
comparison then validates 66 encoded outputs through native, libvorbisfile and
Symphonia with exact frames/rates/layouts and the existing PCM tolerances.
All 24 native candidate/pair cases have identical decoded PCM bits through all
three decoders, covering 3,658,112 channel samples per decoder. The profiler also
combines scalar entries into canonical digit pairs and requires identical
floor/classification/pair hashes through the complete packet tails.

## Encoding and decoder measurement

The previous encoding comparison is retained: common resident PCM, fresh
encoder/mux per iteration, warm process, 4,096-frame pushes, output discarded,
file IO excluded. Four scalar and four pair settings plus three Xiph settings
produce 66 quality rows and 462 shuffled encoding timing rows over seven trials.
In the retained experiment, producer `native` means the scalar candidate;
`native-baseline` means the committed pair encoder. The same current encoding
adapter is compiled against both production-source snapshots, with matching
profiles and registry dependencies.

The new `--decode-cost` option measures the actual consumer cost of the paired
output representations. The **same decoder executable** reads both versions:
native, Symphonia 0.6.1, or pinned libvorbisfile 1.3.7. Each full decode includes
fresh header/setup/codec construction, forward resident-input reading, complete
PCM synthesis and cleanup. Input file loading and PCM retention are excluded;
one warm run precedes timing. Every iteration must emit the exact expected
channel-sample count. The original pipeline worker supplies the measurement;
no decoder implementation or precision option is changed for either file.

Four precision settings, two native representations, three decoder engines,
six sources and seven trials yield **1,008 decoder timing rows**. Ordering is
shuffled. [Raw trials](validation/2026-10-27-scalar-evaluation-decode-trials.tsv)
and the [summary](validation/2026-10-27-scalar-evaluation-decode-summary.tsv)
retain medians, min/max and median absolute deviations. Maximum relative decode
MAD across these cases is 7.96%; this remains a desktop measurement.

| Five-second excerpt | Stages | Pair bytes | Scalar bytes | Native decode ms, pair -> scalar | Change |
| --- | ---: | ---: | ---: | ---: | ---: |
| in_two | 1 | 32,319 | 28,929 | 8.837 -> 9.227 | +4.41% |
| in_two | 2 | 159,500 | 154,386 | 11.486 -> 12.651 | +10.15% |
| in_two | 3 | 370,626 | 367,298 | 15.436 -> 17.364 | +12.49% |
| in_two | 4 | 625,768 | 623,349 | 20.302 -> 22.517 | +10.91% |
| Goin' Under | 1 | 23,928 | 21,491 | 8.246 -> 8.430 | +2.23% |
| Goin' Under | 2 | 183,907 | 177,902 | 12.012 -> 13.406 | +11.60% |
| Goin' Under | 3 | 455,229 | 451,607 | 17.034 -> 19.576 | +14.92% |
| Goin' Under | 4 | 668,935 | 668,010 | 20.771 -> 22.692 | +9.25% |

The default three-stage regression is consistent across engines. Symphonia's
song medians increase 12.05% / 10.60%; libvorbisfile increases 11.56% / 11.44%.
One-stage Symphonia is faster on these files, and some small-file cases benefit
from the smaller setup. Those results remain in the summary rather than being
collapsed into a universal claim. The default-stage throughput cost outweighs
the small size gain for the requested performance priority.

Encoding shows no compensating default-stage speed benefit: in_two changes
13.404 -> 13.528 ms and Goin' Under 14.008 -> 14.447 ms. Different Xiph/native
quality settings remain separate rate-distortion points. The candidate/pair
comparison is exact-PCM; it is not a matched-perceptual-quality comparison with
another encoder.

## Memory tradeoff and decision

At stereo/N1024/three stages, the first decoded codebook's retained elements
fall **5,368 -> 328 bytes**. Total setup falls **17,568 -> 12,528 bytes**;
synthesis buffers/tables remain 34,816 bytes. Raw encoder storage rises
29,623 -> 29,665 bytes: the shorter setup is offset by the larger worst-case
packet reservation for two six-bit words per pair. Setup packet length falls
294 -> 144 bytes. These public-getter probes exclude allocator/struct overhead,
transient constructor storage, input/output buffers and process working set.
See [pair](validation/2026-10-27-scalar-evaluation-storage-pair.tsv) /
[scalar](validation/2026-10-27-scalar-evaluation-storage-scalar.tsv) results and
[probe source](validation/2026-10-27-scalar-evaluation-storage-probe.rs).

This is a valid lower-memory representation with a measured CPU tradeoff.
Production retains the pair book, whose bytes reproduce the prior committed
output hashes. No new public policy option or codec dependency is introduced.
All production Ogg/Vorbis source files match `77747ae52`; the previous 108-test
and sanitizer evidence continues to describe that unchanged codec source.
This experiment is not presented as a new production sanitizer campaign.

## Reproduction and tooling checks

Use an isolated checkout containing the current benchmark tools, apply the
retained candidate patch there, and use clean pinned references from the
[original baseline](ENCODING_PERFORMANCE.md):

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_encoding.py --run-root target/deadlib-audio-reference/scalar-new --baseline-revision 77747ae52 --decode-cost --seconds 5 --repeats 7 --min-seconds 0.15
python crates/deadlib-audio/decode/scripts/profile_encoding.py target/deadlib-audio-reference/scalar-new
```

The [experiment manifest](validation/2026-10-27-scalar-evaluation-benchmark.json)
records the candidate source hashes. The check manifest explicitly distinguishes
that archived source from the restored production encoder, verifies the patch
reconstructs the measured bytes, and records preserved worker hashes. Binaries,
corpora and generated media remain under ignored `target/`. Original media and
common PCM hashes match the previous comparison.

The default benchmark mode is smoke-tested after restoration and reproduces all
four prior native mono output hashes. Formatting, all-target/all-feature Clippy,
Python syntax checks, and the new full/count/exact-symbol/PCM comparisons pass.
The decoder-cost option remains available for future representation changes.
Further work returns to measured decoder throughput/startup bottlenecks, sparse
seeking, broader conformance and competitive encoder quality/rate control.
