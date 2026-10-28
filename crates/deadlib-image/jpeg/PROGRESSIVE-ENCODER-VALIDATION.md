# Native scan generation and successive approximation

Sequential and progressive output can use caller scan scripts. Progressive
Huffman SOF2 and arithmetic SOF10 both implement spectral selection, initial
point transforms, DC/AC refinement and restart resets. Existing defaults still
write full-precision spectral scans and retain all 600 original Huffman output
hashes exactly.

`CodingOptions::successive_approximation` selects the initial Al (0..=13) for
generated progressive scans. Al=0 keeps the original spectral strategy. For
Al>0, the writer emits an interleaved initial DC scan and one initial AC scan
per component, followed by DC/AC refinements at every bit down to Al=0. The
generated count is `(initial_Al + 1) * (component_count + 1)`.

`CodingOptions::scan_script` borrows a slice of `ScanSpec`, whose component
selectors are frame component IDs. It overrides generated scan layout. Set
`progressive` to select progressive or sequential parameter validation.
`CodingOptions<'a>` and `EncodeOptions<'a>` remain Copy and borrow descriptors
only during encoding. No encoder-owned scan vector is needed. Use component
IDs from the coefficient frame; RGB/CMYK identifiers need not be numerical.

```rust
use deadlib_image_jpeg::{CodingOptions, EntropyCoding, ScanSpec};
# fn example() -> Result<(), Box<dyn std::error::Error>> {
let generated = CodingOptions {
    progressive: true,
    successive_approximation: 3,
    entropy_coding: EntropyCoding::Arithmetic,
    ..CodingOptions::default()
};
let script = [
    ScanSpec::new(&[1], 0, 0, 0, 1)?,
    ScanSpec::new(&[1], 6, 63, 0, 0)?,
    ScanSpec::new(&[1], 1, 5, 0, 0)?,
    ScanSpec::new(&[1], 0, 0, 1, 0)?,
];
let custom = CodingOptions { scan_script: &script, ..generated };
# let _ = custom;
# Ok(())
# }
```

This example describes a one-component frame with ID 1. Scripts may split
bands, reorder separate component scans and assign independent precision
levels to different bands. Every frame component must receive an initial DC
scan. Progressive scripts may end with AC bands absent or low bits omitted:
the output then retains only the explicitly transmitted precision. Sequential
scripts transmit each component exactly once. Partial progressions are an
explicit script choice; generated progressions always finish at Al=0.

The writer validates scan count, IDs, duplicate components, spectral bounds,
Ah/Al transitions, band history, DC-before-AC and missing components before
emitting output. T.81 A.2.3/B.2.3 requires component selectors within one scan
to follow frame order. The encoder and decoder now reject violations of that
rule. Across separate scans the component order is unrestricted. The ten-block
MCU limit applies to selected components in each interleaved scan. Separate
scans therefore support valid frames with a larger global sampling sum,
including YCCK 4:1:0 and four 4x4-sampled components.

DC initial passes use arithmetic right shifts; refinements transmit the
original two's-complement bit, including for negative values. AC point
transforms preserve sign and truncate magnitude. Huffman refinement encodes
newly nonzero values, ZRLs, EOBs and buffered corrections according to G.1.2.3.
Correction storage is a u64 holding at most 63 bits. Each block's EOB is emitted
immediately, so no unbounded inter-block correction buffer is allocated.
Arithmetic refinement uses Table G.2 and bypasses EOB decisions before the
prior nonzero band end. Both entropy paths share block/MCU traversal.

Point transforms also permit full signed 16-bit coefficient extrema to fit the
transmitted Huffman categories: the C decoder exactly recovers the arithmetic
model fixtures after Huffman progression from Al=13, at both sample precisions.
These are entropy-model tests; pixel/IDCT tolerances remain in the other
validation documents.

## External evidence

The pinned reference remains libjpeg-turbo 3.2.1 with the C coefficient oracle
described in tests/oracle/README.md. All **5,214 scan-script encodes** from
**409 distinct JPEG inputs** match coefficients and quantizers exactly in both
the C and native decoders, with zero C warnings. The inputs comprise the pinned
Huffman/arithmetic fixtures, native pixel/CMYK corpora, 38 coefficient-extrema
sources and 24 additional sampling layouts.

The matrix covers generated Al=1/3/13, custom split bands with reversed
component scan order, separate and mixed sequential scans, partial DC-only
progressions, restart intervals 0/1/7 and fixed/optimized Huffman policies.
Partial expectations are computed directly from original C coefficient values
using the DC point transform and zero AC bands. The 24 sampling inputs include
4x4 factors, fractional ratios, and 4:1:0, across RGB/YCbCr/CMYK/YCCK at both
precisions. Configurations are selected per input class; this is not a claim
that every policy is crossed with every case. See tests/reports/scans-turbo.json.

Another **128 deterministic randomized legal scan graphs** match C/native
coefficients exactly, with zero warnings. They contain 47..427 scans per file,
independent band partitions/initial Al values, mixed DC/AC and component scan
order, both entropy coders, restart intervals and both table policies.
The seven inputs include 8/12-bit gray/color and full 16-bit model extrema.
Seeds and hashes are in tests/reports/scan-fuzz-turbo.json. This is a valid-script
smoke gate; coverage-guided malformed-input fuzzing remains required.

All **600 default Huffman rewrites** still match the original byte hashes and
C coefficients exactly. The repeated report is
tests/reports/rewrite-scan-refactor-turbo.json. Rust tests cover generated and
custom progressions, signed endpoints, partial precision, invalid scripts,
scan budgets, SOS ordering and the pixel API with large sampling sums.

```text
cargo build -p deadlib-image-jpeg --release --example rewrite_scans --example inspect --example scan_fixtures
cargo run -p deadlib-image-jpeg --release --example scan_fixtures -- <sampling-inputs>
python crates/deadlib-image/jpeg/tests/reference_scans.py --native <rewrite_scans> --inspect <inspect> --oracle <jpeg-oracle> --corpus <pixel-corpus> --corpus <cmyk-corpus> --sampling-corpus <sampling-inputs> --model-corpus <extrema-inputs> --out <scan-gate>
python crates/deadlib-image/jpeg/tests/reference_scan_fuzz.py --native <rewrite_scans> --inspect <inspect> --oracle <jpeg-oracle> --out <scan-fuzz-gate>
```

Drivers snapshot executables and retain source/tool/output hashes. The main
driver reuses one case directory to bound disk usage. Checkpoints record cases
only after both decoders pass; resumption verifies unchanged tools and input
hashes. Individual generated case files are ephemeral. Temporary build/output
locations can be selected with Cargo's `--target-dir` and the drivers' `--out`.

Remaining work includes full component counts, lossless/hierarchical modes,
DNL/abbreviated streams, incremental I/O, independent conformance and other C
references, coverage-guided fuzzing, EOB aggregation, per-scan table optimization
and measured performance/size/quality work. The complete goal remains in PLAN.md.

Normative source: ITU-T T.81, A.2.3, B.2.3 and Annex G,
https://www.w3.org/Graphics/JPEG/itu-t81.pdf.
