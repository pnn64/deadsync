# Packed half-size resizing and direct PCM layouts â€” 2026-10-25

Three safe Rust optimizations in `deadlib-assets` and `deadlib-audio-decode` preserve pixels, samples, frame boundaries, and append behavior. Both Lua crates are excluded.

## Changes and work removed

1. **Half-size textures:** average four packed byte channels together with masks, shifts, additions, and subtraction. Horizontal averages floor and the vertical average rounds, exactly matching the original 24-bit fixed-point interpolation. Each eligible output pixel avoids 24 scalar multiplications, and each pass avoids its horizontal coordinate vector. Other ratios use the existing interpolation. Dimensions above `2^23` also retain the existing path because its `f32` coordinates cannot represent every half-pixel step. Keep the packed loop out of line to isolate it from general interpolation.
2. **Planar PCM input:** append one strided channel at a time instead of visiting every channel vector and pushing a sample for every frame. This removes the nested frame/channel push loop and repeated destination length/capacity updates. Existing mono/stereo paths, capacity reservation, consumed-prefix reclamation, incomplete-frame handling, and normalization remain intact.
3. **PCM channel mapping:** copy complete repeated channel blocks and a final prefix. Compute quotient/remainder once per call, replacing modulo, indexed lookup, and `Vec::push` for every output sample. Existing same-layout, mono-to-stereo, and stereo-output paths remain intact; arbitrary layouts retain their cyclic channel order and existing output prefix.

Production code grows by **41 net lines**, excluding the test module declaration. Most growth is the dedicated packed resize loop and its precision guard. Execution loses allocations, arithmetic, nested pushes, and intermediate indexing rather than adding a runtime framework. No dependency, public API, production `unsafe`, or new CPU feature requirement is introduced.

General packed-channel interpolation, a wrapping difference-based interpolation, and in-place GIF expansion were tried and discarded because their CPU results did not justify retaining them. GIF decoding is unchanged and remains a benchmark control.

## Measurement

Intel Xeon E5-2696 v4; Windows x86_64; rustc 1.99.1 / LLVM 23.1.1. Every archived executable uses `--release --locked --config profile.release.lto=false` and the repository's static MSVC runtime flags. Assets and audio were built separately with the same package selection before and after. Production baselines are from `5a73b6c6e`; subsequent unrelated Lua work does not enter these packages.

Each implementation was built, archived, and measured before changing its production source. `before` has all original kernels; the audio `planar` stage has only the deinterleaving change; `after` has all retained changes. Benchmark bodies are the same across each comparison. Added correctness tests and formatting do not change the timed work.

Four fresh process runs rotate archive order. Each benchmark pins its thread to logical processor 2, discards a warmup batch, and takes the median of eleven batches using Windows `QueryThreadCycleTime`. Tables show medians across processes and before/after CPU-cycle throughput ratios. Process ranges are paired minimum/maximum ratios, not confidence intervals. Raw TSVs also contain elapsed nanoseconds and all allocation counters. Fixture creation and allocation probing occur outside timing. No compilation or other benchmark launched by this task overlapped the final measurements; the machine was shared with unrelated work.

Resize measurements include cloning the consumed input and dropping the output in both versions. Audio measurements reuse reserved buffers, include clearing/truncating them, and pass function pointers, dimensions, inputs, and outputs through `black_box`. Mapping preserves a two-sample output prefix. GIF controls decode complete warmed files, including I/O and decoder allocations.

These are local kernel measurements, not whole-game FPS claims. Half-size resizes improve **2.10â€“2.62x** in CPU throughput. The isolated planar change improves **1.42â€“1.53x** at 4,096 frames and 3â€“8 channels; final production improves **1.47â€“1.60x** against the starting version. Changed mapping layouts improve **1.73â€“5.65x** across measured sizes.

The controls include regressions: 16â†’32 resizing is 12.1% lower in CPU throughput, irregular resizing is 8.8% lower, and 64-frame mono-to-stereo mapping is 20.6% lower (about nine extra nanoseconds per buffer). Their algorithms are unchanged; code generation and shared-host variation still affect results. Other controls vary in both directions. The half-size 256â†’128 result has a wide process range; every process still improves. Full results follow rather than assuming unchanged paths have zero cost.

### Resize and unchanged GIF controls

| Workload | Before cycles/op | After cycles/op | CPU throughput | Process ratio range |
| --- | ---: | ---: | ---: | ---: |
| `resize/16x16/32x32` | 24501.68 | 27876.50 | 0.879x | 0.823?0.934x |
| `resize/256x256/128x128` | 767666.91 | 292957.05 | 2.620x | 1.588?7.835x |
| `resize/256x256/512x512` | 7295950.26 | 7115072.00 | 1.025x | 0.957?1.297x |
| `resize/1024x512/64x32` | 6389432.34 | 3045884.89 | 2.098x | 1.999?2.302x |
| `resize/1x256/2x512` | 35977.43 | 32084.72 | 1.121x | 0.822?1.408x |
| `resize/257x129/259x127` | 846460.56 | 928553.58 | 0.912x | 0.750?1.208x |
| `gif/1/offset=false` | 181356.73 | 188531.22 | 0.962x | 0.943?1.077x |
| `gif/1/offset=true` | 176807.67 | 176399.11 | 1.002x | 0.931?1.111x |
| `gif/16/offset=false` | 193432.66 | 188056.62 | 1.029x | 0.923?1.074x |
| `gif/16/offset=true` | 190199.76 | 195562.51 | 0.973x | 0.933?1.148x |
| `gif/256/offset=false` | 1694478.25 | 1712929.98 | 0.989x | 0.944?1.051x |
| `gif/256/offset=true` | 1673341.11 | 1641276.97 | 1.020x | 0.994?1.064x |
| `gif/1024/offset=false` | 20373671.14 | 19970802.59 | 1.020x | 0.906?1.132x |
| `gif/1024/offset=true` | 20016380.04 | 20436684.73 | 0.979x | 0.862?1.129x |

### Isolated planar input change

| Workload | Before cycles/op | After cycles/op | CPU throughput | Process ratio range |
| --- | ---: | ---: | ---: | ---: |
| `planar/channels=1/frames=64` | 53.48 | 50.12 | 1.067x | 0.930?1.275x |
| `planar/channels=1/frames=4096` | 1634.55 | 1591.83 | 1.027x | 0.797?1.182x |
| `planar/channels=2/frames=64` | 114.50 | 104.51 | 1.096x | 0.914?1.300x |
| `planar/channels=2/frames=4096` | 4799.76 | 4569.47 | 1.050x | 0.946?1.269x |
| `planar/channels=3/frames=64` | 711.94 | 602.33 | 1.182x | 1.144?1.421x |
| `planar/channels=3/frames=4096` | 41868.94 | 29046.49 | 1.441x | 1.275?1.818x |
| `planar/channels=4/frames=64` | 948.89 | 786.62 | 1.206x | 1.176?1.338x |
| `planar/channels=4/frames=4096` | 57700.62 | 37691.93 | 1.531x | 1.397?1.669x |
| `planar/channels=6/frames=64` | 1369.31 | 1137.12 | 1.204x | 1.125?1.273x |
| `planar/channels=6/frames=4096` | 87060.20 | 57919.55 | 1.503x | 1.333?1.730x |
| `planar/channels=8/frames=64` | 1683.00 | 1507.51 | 1.116x | 1.010?1.221x |
| `planar/channels=8/frames=4096` | 107229.35 | 75491.82 | 1.420x | 1.350?1.480x |

### Isolated channel mapping change

| Workload | Before cycles/op | After cycles/op | CPU throughput | Process ratio range |
| --- | ---: | ---: | ---: | ---: |
| `mapping/6to1/frames=64` | 704.10 | 405.28 | 1.737x | 1.631?1.818x |
| `mapping/6to1/frames=4096` | 42623.49 | 23587.48 | 1.807x | 1.704?1.928x |
| `mapping/8to1/frames=64` | 693.54 | 400.99 | 1.730x | 1.640?1.873x |
| `mapping/8to1/frames=4096` | 41252.73 | 22729.72 | 1.815x | 1.653?2.009x |
| `mapping/2to6/frames=64` | 3477.12 | 1400.58 | 2.483x | 2.378?2.573x |
| `mapping/2to6/frames=4096` | 217657.28 | 87225.50 | 2.495x | 2.391?2.586x |
| `mapping/2to8/frames=64` | 4504.64 | 1794.24 | 2.511x | 2.438?2.618x |
| `mapping/2to8/frames=4096` | 290457.86 | 111903.60 | 2.596x | 2.330?2.898x |
| `mapping/6to8/frames=64` | 4532.82 | 802.79 | 5.646x | 5.178?6.308x |
| `mapping/6to8/frames=4096` | 285515.53 | 50922.19 | 5.607x | 5.368?5.864x |
| `mapping/8to3/frames=64` | 1800.93 | 403.37 | 4.465x | 4.095?4.662x |
| `mapping/8to3/frames=4096` | 113964.54 | 22929.62 | 4.970x | 4.602?5.124x |
| `mapping/2to2/frames=64` | 41.91 | 43.92 | 0.954x | 0.686?1.022x |
| `mapping/2to2/frames=4096` | 2756.80 | 2639.85 | 1.044x | 0.968?1.129x |
| `mapping/1to2/frames=64` | 75.48 | 95.07 | 0.794x | 0.551?1.213x |
| `mapping/1to2/frames=4096` | 1110.09 | 1110.99 | 0.999x | 0.846?1.297x |
| `mapping/6to2/frames=64` | 159.75 | 150.53 | 1.061x | 0.865?1.369x |
| `mapping/6to2/frames=4096` | 5125.86 | 4904.57 | 1.045x | 0.983?1.090x |


## Allocations and correctness

All **288 audio probes** report zero allocations, reallocations, frees, requested bytes, and peak bytes. Each resize half-step removes one coordinate-vector allocation: 256â†’128 falls from 3 to 2 allocations and saves 1,536 requested bytes; 1024Ã—512â†’64Ã—32 falls from 9 to 5 and saves 11,520 bytes. Counts include the input clone and result allocation. General resizes and GIF controls retain their allocation counts and bytes. The three TSVs contain all **400 operation probes**.

All **138 release tests** pass: 95 asset unit tests, nine asset integration tests, 32 audio unit tests, and two audio layout tests. Four focused debug tests also pass. Clippy completes for both packages and their tests with existing repository warnings and none in the changed code. New benchmark files pass `rustfmt --check`; the changed files pass `git diff --check`.

Pixel checks compare the packed path with the original interpolator over every ordered endpoint-byte pair, independent neighboring channels, repeated half-size passes, odd/non-power-of-two dimensions, and the existing fixture corpus. A very wide image verifies preservation of the original float-coordinate bounds failure rather than silently changing that behavior. Audio checks cover every i16 value, incomplete frames, prefixes, zero/empty layouts, 0â€“9 input/output channels, packet consumption, compaction, and existing capacity behavior. No floating-point operation is reassociated.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-decode --lib --tests
cargo test --locked -p deadlib-assets --lib half_resize
cargo test --locked -p deadlib-audio-decode --test planar_kernel
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-assets -p deadlib-audio-decode --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib --no-run --message-format=json
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-decode --test planar_kernel --no-run --message-format=json
python scripts/benchmark_direct_media.py --archive before=target/direct-media/before --archive after=target/direct-media/after --target assets --runs 4 --output docs/performance/2026-10-25-direct-media-assets.tsv
python scripts/benchmark_direct_media.py --archive before=target/direct-media/before --archive planar=target/direct-media/planar --archive after=target/direct-media/after --target planar --runs 4 --output docs/performance/2026-10-25-direct-media-planar.tsv
python scripts/benchmark_direct_media.py --archive planar=target/direct-media/planar --archive after=target/direct-media/after --target mapping --runs 4 --output docs/performance/2026-10-25-direct-media-mapping.tsv
```

Copy each Cargo `compiler-artifact.executable` into its archive as `assets.exe` or `planar.exe`. Build package selections separately and finish compilation before measuring. To reconstruct the baseline in another worktree, use `5a73b6c6e` and copy in the final benchmark/test files, the `direct_media_perf` module declaration, and the test-only `write_gif` visibility change. For the intermediate audio stage, additionally apply only the `PlanarAccum::push_i16_interleaved` change. New correctness tests also pass on the baseline. The original before audio binary predates the mapping benchmark; use the intermediate archive for its isolated before measurement.

Local source snapshots, initial/rejected measurements, archived executables, final process logs, and validation logs are under `target/direct-media`. Committed raw data: [assets](2026-10-25-direct-media-assets.tsv), [planar](2026-10-25-direct-media-planar.tsv), [mapping](2026-10-25-direct-media-mapping.tsv).

| Archive | SHA-256 |
| --- | --- |
| `before/assets.exe` | `bfedc5b21924c3c5b1afad1d85a76181f3e96510029509969df7f4b0ac19eb84` |
| `after/assets.exe` | `92f9d8f7450ce3b673a86312b3e3e1a5c5104aeb81d49ce281e682af6517df24` |
| `before/planar.exe` | `86d8b0e52ef598d3b7115a55986200646980d7b73d30c1ee7c02347106046a88` |
| `planar/planar.exe` | `24db076d29fe10d68cfc1de315c74863c6e21248848275368bad420d2360d645` |
| `after/planar.exe` | `ac4adb3c1b2e405ccb592dcf284dfd77e35d67e1e46dae07c8ba8e85671f985e` |
