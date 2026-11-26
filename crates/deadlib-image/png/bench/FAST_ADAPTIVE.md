# Default Fast/Adaptive PNG encoding

The default encoder uses fewer CPU cycles than Rust `png` Fast/Adaptive
in two repeated runs across RGBA8, native formats and APNG. Both public
Deadlib encoding APIs also beat the reference on the separate opaque
RGBA8 workload.

| Workload | Matched inputs | Deadlib/Rust png cycles, runs 1–2 | Deadlib/Rust png bytes |
| --- | ---: | ---: | ---: |
| RGBA8, writer | 30 | 0.543 / 0.545 | 1.010 |
| Game captures within RGBA8 | 8 | 0.561 / 0.564 | 1.085 |
| Kodak photos within RGBA8 | 6 | 0.653 / 0.647 | 1.082 |
| Native-format writer | 30 | 0.882 / 0.874 | 0.751 |
| APNG writer | 9 | 0.629 / 0.641 | 0.795 |
| Opaque RGBA8, buffered `encode` | 24 | 0.506 / 0.515 | 0.959 |
| Opaque RGBA8, `encode_writer` | 24 | 0.527 / 0.530 | 0.959 |

The writer wins **48/60** matched still PNG cases in both runs, including
**8/8** screenshots and **6/6** photos. It wins **9/9** APNG cases. Buffered
`encode` wins **24/24** on its opaque RGBA8 workload. On the eight opaque
screenshots, the writer moves from **2.87×** the reference cycles in the
before run to **0.576–0.577×** in the final runs.

| Native format depth | Matched cases | Deadlib/Rust png cycles, runs 1–2 | Deadlib/Rust png bytes |
| --- | ---: | ---: | ---: |
| 1-bit | 4 | 0.935 / 0.959 | 0.994 |
| 2-bit | 4 | 0.992 / 1.012 | 0.986 |
| 4-bit | 4 | 0.883 / 0.862 | 1.000 |
| 8-bit | 10 | 0.764 / 0.767 | 1.000 |
| 16-bit | 8 | 0.939 / 0.922 | 0.585 |

Two-bit totals are effectively tied. Sixteen-bit wins three individual
cases per run, with lower aggregate cycles and **42% fewer total bytes**.
A focused before/after check of the four-bit packer, in both executable
orders, reduced native cycles by **15–19%** with the same encoded sizes.

## Reference implementation

Rust `png` 0.18.1 applies its adaptive predictor to each row and feeds the
filter tag and residuals to `fdeflate`'s ultra-fast encoder. That encoder uses
one DEFLATE block, a trained Huffman tree, and only distance-one matches for
zero runs. The buffered PNG API compares the compressed result with stored
DEFLATE and can retry with unfiltered stored rows.
[png 0.18.1 encoder](https://docs.rs/crate/png/0.18.1/source/src/encoder.rs) · [fdeflate compressor](https://docs.rs/crate/fdeflate/0.3.7/source/src/compress.rs)

On the 1600×900 `windows-before.png` capture, the original Deadlib encoder
spent about **166 million cycles** on fixed-Huffman LZ77 compression of
the filtered bytes. Adaptive filtering took about **12 million cycles**.
On `initials-mega-verified.png`, the corresponding stage counts were
**8.5 million** and **7.3 million**. These are isolated stage timings with
their own allocations and input preparation.

## Implementation

- PNG and APNG Fast/Adaptive encode samples up to eight bits with an analytic
  residual tree and distance-one zero runs in 65,535-byte blocks. Each block
  can fall back to stored DEFLATE. Both public encoders route through this
  policy, and the streaming writer sizes small buffers from the filtered
  length.
- Sixteen-bit Fast/Adaptive still uses fixed Huffman LZ77 with a 32 KiB
  history. It tries the newest hash hit and skips chain maintenance. The
  established sixteen-bit gradient compression test remains a guard.
- The AVX2 adaptive filter computes the same residuals and signed-byte
  scores while the vectors are in registers. It keeps the existing scoring
  limits, tie order and scalar tails. Four-bit samples use vector packing
  where available. All packed samples use whole-word range validation.

## Measurement

The comparison pins png 0.18.1 and measures both codecs in the same process.
All inputs and produced PNGs stay in memory, and the measured operation
includes output destruction. Verification independently decodes each
output and compares samples, palette, color type and depth. The APNG check
also compares frame rectangles, delays, blend and disposal.

The RGBA8 corpus contains eight game captures, six Kodak photos and 16
generated images. The native-format corpus adds 60 inputs covering all 15
legal color-depth combinations with and without Adam7. Rust png 0.18.1
encodes the 30 noninterlaced variants. The nine APNG inputs cover full frames,
cropped subframes and a separate default image. The Windows harness pins
the timing thread and reports the median CPU cycles and wall time of five
batches per input and mode.

The RGBA8 comparison supplies the same bytes to both APIs. In the native
format workload the reference accepts already packed rows, while Deadlib
accepts full sample arrays and does packing and 16-bit byte-order conversion
in the measured operation.

The timing runs used five batches of at least 20 ms on a Windows AVX2
Xeon E5-2696 v4. The legacy writer harness pins CPU 0, and the cross-codec
harness pins CPU 2. The focused four-bit runs used 35 ms batches. The
release test suite passed **222 tests** with debug assertions enabled,
including the 16-bit gradient compression guard, the 65,535-byte block
boundary, and packed-byte and predictor checks. The portable code compiled
for Windows ARM64 and i686.

## Reproduce

Build the cross-codec harness and prepare inputs with
[`setup.ps1`](contenders/setup.ps1). Run from the repository root.

```powershell
$env:PNG_BENCH_MS = '20'
$env:PNG_BENCH_CPU = '2'
$env:PNG_COMPARE_ENCODE_MODES = 'deadlib-fast,rust-png-fast'
$base = 'target/png-codec-comparison'
$exe = "$base/rust-build/release/compare.exe"
& $exe encode "$base/encode-inputs.tsv"
& $exe animation "$base/animation-inputs.tsv"
```

The [encoder harness instructions](README.md#encoder-workload) document
the separate buffered and streaming measurements. For this run use sizes
`17x13,640x360,1920x1080,3840x2160` and the eight screenshot paths
listed in the input manifest.

- [Cycle and size summary](results/windows-x86_64-20261126-fast-adaptive/summary.json.gz) · [Input hashes](results/windows-x86_64-20261126-fast-adaptive/inputs.json.gz) · [Build and artifact identities](results/windows-x86_64-20261126-fast-adaptive/manifest.json)
- Static encode [run 1](results/windows-x86_64-20261126-fast-adaptive/encode-run1.tsv.gz) · [run 2](results/windows-x86_64-20261126-fast-adaptive/encode-run2.tsv.gz)
- APNG [run 1](results/windows-x86_64-20261126-fast-adaptive/animation-run1.tsv.gz) · [run 2](results/windows-x86_64-20261126-fast-adaptive/animation-run2.tsv.gz)
- Buffered and streaming [before](results/windows-x86_64-20261126-fast-adaptive/before.tsv.gz) · [run 1](results/windows-x86_64-20261126-fast-adaptive/writer-run1.tsv.gz) · [run 2](results/windows-x86_64-20261126-fast-adaptive/writer-run2.tsv.gz)
- [Release test log](results/windows-x86_64-20261126-fast-adaptive/gate-tests.log.gz)
