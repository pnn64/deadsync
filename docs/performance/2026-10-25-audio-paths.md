# Direct PCM repetition, stereo downmix and fade quantization

2026-10-25. Original production source: `d4c1d0aff`. Both Lua crates and concurrent unrelated edits are excluded. Measurements use the actual production functions, with the same final benchmark sources in every archived build.

## Three retained optimizations

1. **Repeat complete PCM frames directly.** Mono expansion fills each destination frame with its source sample. Stereo expansion fills pairs of i16 samples and copies the left sample into an odd final channel. This removes the cycling iterator's per-output advance and wrap decisions for one- and two-channel sources. Other layouts keep the original ordered cycle. A small private `#[inline(never)]` kernel keeps these loops out of the generic mapper: embedding them in that function caused substantial generic-path regressions in trials. It is one direct call per packet, with no new ownership or dispatch layer.
2. **Use fixed stereo frames for mono appends.** The existing mono append branch traverses `&[[i16; 2]]` and copies element zero. It replaces a runtime stride with a constant stride and retains existing prefixes and vector capacity. The streaming write mapper keeps its original dispatch and mono helper; routing all mono output through the append function was slower for other layouts and was discarded. An optimized standalone build of the actual append function at 2-to-1 emits SSE2 `movdqu`, `pshuflw`, `pshufhw`, `pshufd` and `punpcklqdq`, processing eight stereo frames per vector loop without unsafe intrinsics or additional CPU requirements.
3. **Quantize faded samples with a widened half bias.** Preserve the original f32 multiplication, widen its result to f64, add a signed half, then cast to i16. Remove the intermediate integer-to-float conversion, fractional subtraction, two threshold comparisons and integer corrections. A finite f32 in the PCM range cannot lie close enough below a half-integer for the f64 addition to cross that boundary early. Ties still round away from zero. Fade endpoints, fused interpolation, unity and silence shortcuts, and incomplete trailing frames retain their original behavior.

Three existing production functions change and one private kernel is added. The production diff is **32 added / 8 removed lines**, including comments; the added fixed-frame loops reduce executed work. There are no new production unsafe blocks, dependencies, public APIs or retained storage. Existing tests gain an allocation assertion, and the benchmark adds mono expansion, odd channel counts and generic-layout controls.

## Measurement

Intel Xeon E5-2696 v4, 22 cores / 44 logical processors; Windows x86_64 MSVC; rustc 1.99.1 (`58cf8f9c3`), LLVM 23.1.1. All retained measurements use `cargo test --release --locked` with the workspace optimization settings: opt-level 3, **full LTO**, static MSVC runtime, baseline target CPU. Early trials with LTO disabled are excluded from these tables.

The existing kernel harness pins its thread to logical CPU 2 and reads Windows `QueryThreadCycleTime`. Each executable reports the median of eleven batches after one warmup. Six process pairs alternate before/after order. Tables report medians of those six process medians; CPU throughput is before cycles divided by after cycles. Paired ranges are observed minimum/maximum per-process ratios, not confidence intervals. These counters measure scheduled thread cycles, not retired-instruction PMU events. No builds, functional tests or other benchmarks launched by this task overlap retained timing runs. The shared host still varies; controls and every process sample remain in the TSV files.

All arguments and production function pointers pass through `black_box`. Inputs and output capacities are prepared outside timing. Mapping replaces a complete output packet in reused storage, with relative input/output page offsets of 0 and 128 bytes. Tables below select phase 128; both phases remain in the data. Appends include restoring a two-sample prefix. Fades include copying the original input back before each call so samples do not decay across iterations. That reset cost is identical in both versions. Append copy controls do not fix relative allocation addresses and should not be read as memcpy improvements.

Four cumulative source stages isolate the changes: `baseline`, `mapping`, `fade`, `mono`. The first comparison measures baseline-to-mapping; the second mapping-to-fade; the third fade-to-mono. Separate baseline-to-final comparisons verify the combined mapper and fade results. Binary SHA-256 values are stored on every TSV row.

## Final mapper results

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `mapped/1to3/frames=4096/phase=128` | 32410.13 | 19734.00 | 1.642x | 1.558-1.710x | 14796.60 | 8993.83 |
| `mapped/1to6/frames=4096/phase=128` | 51268.15 | 29740.90 | 1.724x | 1.631-1.852x | 23371.60 | 13700.62 |
| `mapped/1to8/frames=4096/phase=128` | 63455.29 | 26169.08 | 2.425x | 2.122-2.937x | 28927.05 | 11934.42 |
| `mapped/2to3/frames=4096/phase=128` | 31939.96 | 17417.73 | 1.834x | 1.758-1.987x | 14584.88 | 7979.02 |
| `mapped/2to5/frames=4096/phase=128` | 41743.47 | 19791.20 | 2.109x | 2.070-2.388x | 19084.53 | 9018.04 |
| `mapped/2to6/frames=4096/phase=128` | 55224.57 | 19860.68 | 2.781x | 2.747-3.080x | 25177.16 | 9006.17 |
| `mapped/2to8/frames=4096/phase=128` | 63735.97 | 23493.69 | 2.713x | 2.569-2.881x | 29128.68 | 10756.56 |

## Isolated stereo-to-mono append results

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `append/2to1/frames=64` | 143.05 | 60.00 | 2.384x | 2.128-2.733x | 65.54 | 27.29 |
| `append/2to1/frames=256` | 364.96 | 196.01 | 1.862x | 1.668-2.009x | 166.28 | 89.11 |
| `append/2to1/frames=4096` | 5223.92 | 2784.68 | 1.876x | 1.777-2.280x | 2376.64 | 1265.58 |

## Final fade results

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `fade/channels=1/frames=64` | 871.17 | 762.14 | 1.143x | 1.051-1.192x | 397.95 | 347.94 |
| `fade/channels=2/frames=64` | 1356.16 | 1168.01 | 1.161x | 1.121-1.264x | 619.22 | 534.14 |
| `fade/channels=6/frames=64` | 3285.00 | 2676.46 | 1.227x | 1.152-1.312x | 1500.80 | 1222.30 |
| `fade/channels=1/frames=256` | 3287.33 | 2985.73 | 1.101x | 1.023-1.258x | 1502.38 | 1364.80 |
| `fade/channels=2/frames=256` | 5419.34 | 4558.56 | 1.189x | 1.042-1.365x | 2476.57 | 2080.94 |
| `fade/channels=6/frames=256` | 12871.66 | 10430.83 | 1.234x | 1.073-1.326x | 5888.06 | 4770.86 |
| `fade/channels=1/frames=4096` | 50687.40 | 44543.65 | 1.138x | 1.049-1.240x | 23162.50 | 20341.19 |
| `fade/channels=2/frames=4096` | 84510.65 | 67197.59 | 1.258x | 1.130-1.360x | 38610.86 | 30664.14 |
| `fade/channels=6/frames=4096` | 200286.55 | 160215.65 | 1.250x | 1.203-1.323x | 91575.82 | 73164.96 |

## Allocations and complete data

All **2,460 operation probes** report zero allocations, reallocations, frees, requested bytes, freed bytes and peak bytes in both versions. No allocation reduction is claimed. Resident fixture and output storage is outside the probes. The strengthened functional mapping test also asserts no allocation churn for every tested layout and length.

- [Isolated repetition stage: 960 probes](2026-10-25-audio-paths-mapping.tsv).
- [Isolated fade stage: 108 probes](2026-10-25-audio-paths-fade.tsv).
- [Isolated mono append stage, including controls: 324 probes](2026-10-25-audio-paths-mono.tsv).
- [Final mapper versus original: 960 probes](2026-10-25-audio-paths-final-mapping.tsv).
- [Final fades versus original: 108 probes](2026-10-25-audio-paths-final-fade.tsv).

## Controls and rejected trials

These are packet-kernel results, not whole-game FPS or loading-time claims. The optimizations target mono/stereo sources; other channel layouts do not gain consistently. The following slower final controls are included explicitly. These source paths retain their original loops, but surrounding code and compiler layout can still affect performance; their losses are not subtracted from or hidden behind the targeted gains.

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `mapped/3to5/frames=64/phase=128` | 764.86 | 807.90 | 0.947x | 0.849-1.033x | 348.80 | 368.31 |
| `mapped/2to1/frames=4096/phase=128` | 4215.26 | 4423.28 | 0.953x | 0.910-1.097x | 1920.90 | 2013.62 |
| `mapped/6to2/frames=4096/phase=0` | 4893.32 | 5129.01 | 0.954x | 0.942-1.005x | 2226.23 | 2354.71 |
| `mapped/6to8/frames=4096/phase=0` | 64644.54 | 65169.91 | 0.992x | 0.896-1.063x | 29477.05 | 29759.83 |

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `append/2to6/frames=4096` | 87836.43 | 97965.82 | 0.897x | 0.876-0.949x | 40121.11 | 44748.97 |
| `append/6to1/frames=64` | 143.25 | 155.72 | 0.920x | 0.862-0.944x | 65.41 | 70.92 |

Rejected trials: applying the widened bias to the general resampler conversion had mixed gains and regressions; replacing generic cycling with many tiny copies slowed short frames; embedding the repetition kernels in the main mapper slowed generic layouts; routing all mono writes through the append API slowed non-stereo sources. None of those changes is retained. The original generic loop and streaming mono route remain.

## Correctness and reproduction

The existing differential tests compare original/reference output, complete-frame counts, prefixes and tails across zero/mono/stereo/generic layouts. Fade checks cover every i16 value at multiple rising, falling and extreme envelopes, plus randomized interpolation cases and unity thresholds. The resampler's unrelated NaN/infinity and half-boundary tests also remain unchanged and pass.

Validation: **94 tests pass in debug and 94 in release**, with 15 opt-in benchmarks ignored normally in each profile. Clippy completes with existing warnings. The changed benchmark file passes rustfmt; the production file's only remaining rustfmt difference is the pre-existing iterator formatting near line 78. Scoped `git diff --check` passes.

Build each production stage with identical final test/support sources, then archive the `compiler-artifact.executable` paths emitted by Cargo:

```powershell
cargo test -p deadlib-audio-decode --release --locked --test mapped_output_kernel --test slice_kernels --no-run --message-format=json
```

Use `scripts/benchmark_slice_kernels.py` for each pair. For example, after archiving the named executables:

```powershell
python scripts/benchmark_slice_kernels.py --before target/audio-conversion/release/baseline/mapped_output_kernel.exe --after target/audio-conversion/release/mapping/mapped_output_kernel.exe --filter benchmark_mapped_output --runs 6 --output docs/performance/2026-10-25-audio-paths-mapping.tsv
python scripts/benchmark_slice_kernels.py --before target/audio-conversion/release/mapping/slice_kernels.exe --after target/audio-conversion/release/fade/slice_kernels.exe --filter benchmark_fade_envelope --runs 6 --output docs/performance/2026-10-25-audio-paths-fade.tsv
python scripts/benchmark_slice_kernels.py --before target/audio-conversion/release/fade/slice_kernels.exe --after target/audio-conversion/release/mono/slice_kernels.exe --filter benchmark_slice_kernels --runs 6 --output docs/performance/2026-10-25-audio-paths-mono.tsv
```

The final comparison files use `baseline` versus `mono` with the mapper and fade filters above. `target/audio-conversion/sources/` retains the exact source stages locally; `target/audio-conversion/release/` retains archived executables and detailed run logs. To reconstruct elsewhere, start with the production source at `d4c1d0aff`, apply only the repetition function/helper changes for `mapping`, add the fade-body change for `fade`, then the mono append branch for `mono`. Keep the final benchmark/support sources identical throughout.

```powershell
cargo test -p deadlib-audio-decode -p deadlib-audio --locked
cargo test -p deadlib-audio-decode -p deadlib-audio --release --locked
cargo clippy -p deadlib-audio-decode -p deadlib-audio --lib --tests --locked
```
