# Predictive lossless validation

The SOF3 decoder implements T.81 Annex H for unsigned precision P=2..16.
`decode_lossless(input, limits)` returns native `u16` planes, one
`LosslessCoding { predictor, point_transform }` record per component, borrowed
APPn/COM segments, scan count and the byte position after EOI. It does not apply
the DCT, a level shift, color conversion or precision scaling.

The reference equations and scan rules are in
[ITU-T T.81](https://www.w3.org/Graphics/JPEG/itu-t81.pdf), H.1.1–H.1.2.2,
H.2, A.2.2–A.2.4 and B.2.3. All seven selection values are implemented with
unclamped integer predictions and arithmetic shifts for negative half values.
Differences/reconstruction wrap modulo 2^16 independently of precision P.
Huffman category 16 represents the single modulo difference 32768 without
appended magnitude bits. Point transform Pt reconstructs
`(source >> Pt) << Pt`; only Pt=0 promises all original source bits.

Each frame can have 1..255 components with factors 1..4. Interleaved scans
select at most four components in frame order and at most ten samples per MCU;
single-component scans visit the component sample grid. Dummy samples at odd
interleaved edges are decoded for prediction and discarded. Each component is
transmitted once, while separate scans may visit components in any order.
Restart intervals must cover whole MCU rows; prediction resets at the first
row of each restart interval. Different scans can select different predictors
and Pt values. Huffman destinations 0..3 are supported. Padding, restart
sequence, missing/repeated components, sample range and allocation budgets are
validated.

The decoder allocates component sample buffers once, compacts padded rows in
place, and exposes no DCT/coefficient intermediary. `into_planar` moves these
buffers without copying them. Its gray/RGB/CMYK/YCCK convenience conversions
accept P=2..16; 8-bit shared output stays U8 and other precisions scale to U16.
Native planes always retain unscaled integers. Color interpretation follows
the same prefix Adobe/JFIF/component-ID conventions as the DCT path, without
implicitly applying ICC profiles. Color multiplication uses wide intermediates
to cover 16-bit samples. CMYK's RGB approximation remains uncalibrated.

## Independent reference gate

The test-only `tests/oracle/lossless.c` client uses the public sample API of
libjpeg-turbo 3.2.1. The pinned upstream archive SHA-256 is
`f18088e5528c2d3a615fa6f50de222fc46c92fb208ed29cd3ecd77be79cab42a`.
It is built statically with MSVC 19.44, SIMD disabled for this correctness
gate. The Rust package links no reference library and has no codec dependency.

```text
cmake --build crates/deadlib-image/jpeg/experiments/oracle-build --config Release --target jpeg-lossless --parallel 1
cargo build --offline --release -p deadlib-image-jpeg --example lossless_inspect --jobs 1 --target-dir crates/deadlib-image/jpeg/experiments/arithmetic-encode-target
python crates/deadlib-image/jpeg/tests/reference_lossless.py --native crates/deadlib-image/jpeg/experiments/arithmetic-encode-target/release/examples/lossless_inspect.exe --oracle crates/deadlib-image/jpeg/experiments/oracle-build/Release/jpeg-lossless.exe --out crates/deadlib-image/jpeg/experiments/lossless-gate --fixtures crates/deadlib-image/jpeg/tests/fixtures/lossless
```

All **4,628 cases** pass exact native/C/source comparisons, with zero reference
warnings. The driver snapshots tools and records hashes/resumable checkpoints, keeping
one reusable case directory. Every generated file is independently encoded and
decoded by C, then decoded by Rust. Both decoded results must match exactly,
and every sample must match the original input with that component's Pt applied.
The matrix covers every precision, all seven predictors, interleaved/separate
scans, mixed per-component predictor/Pt values, 1..4 components, restart rows,
full-range random samples, alternating extrema, polynomial ramps, narrow/tall
grids and restart sequence wraparound. The extended boundary matrix tests
every Pt at P=2,8,12,15,16. Retained evidence is in
`tests/reports/lossless-turbo.json`.

Forty-five C-encoded files and exact planar results are checked in under
`tests/fixtures/lossless`; their manifest records hashes. The `JLP1` format
starts with the magic and four LE u32 values P,W,H,N, then for each component
five LE u32 values id,H,V,width,height followed by visible planar LE u16 samples.
Cargo tests require no reference compiler or library. They also check every
truncated fixture prefix and 4,500 bounded mutations, storage limits, metadata
borrowing, trailing bytes, public model mutations and malformed entropy/scans.

## Reference limits and remaining work

libjpeg-turbo's lossless decoder accepts only 1x1 factors. Independent literal
Huffman fixtures therefore test horizontal/vertical subsampling, partial MCU
padding and restart prediction within a vertically sampled component. These
fixtures use manually derived samples/codes, rather than the native encoder.

The reference encoder also emits a valid ten-component lossless stream which
its own decoder rejects at the fifth scan with `Invalid component ID 4 in SOS`.
That case is retained under `tests/fixtures/lossless-special`; its expected
values are constructed directly from the original source integers. The native
decoder matches them. A separate literal-table test covers a 255-component
frame with 4x4 factors and reversed separate scan order. These are native
standards regression tests, not successful C decoder comparisons.

## Native SOF3 encoding

`encode_lossless(&shared_image, LosslessOptions::default())` accepts grayscale
and RGB without changing their original precision or integers. Shared one-bit,
alpha and indexed sources require explicit caller conversion.
`encode_lossless_samples` accepts native interleaved U8/U16 gray, RGB, YCbCr,
CMYK and YCCK at P=2..16. U8 storage requires P<=8. The native planar constructor
`LosslessImage::from_planes` accepts original sampling, IDs and 1..255 components;
`LosslessImage::encode` can rewrite decoded samples without a raster copy.
There is no implicit scaling, color transform or downsampling. Conventional
CMYK is the raw API default, and `cmyk_polarity` labels complemented CMYK
without changing its integers. RGB/YCbCr/YCCK use Adobe transform hints. No JFIF
marker is generated for the lossless process. Metadata variants share the DCT
writer's borrowed additions and ICC preserve/remove/replace controls.

`LosslessOptions` selects predictor 1..7, Pt, fixed or Annex K optimized Huffman
tables and restart rows. The optimized path collects two shared category
histograms; the fixed path skips that pass. Both write to bounded output with
no entropy/raster intermediary and no predictor row buffer. The encoder reads
previous original samples at the scan's Pt and replicates edge values for
dummy samples. A fixed fallback gives categories 0..16 five-bit codes, including
the category-16 special case. Literal native output tests match independently
derived Annex H bytes for the 16-bit midpoint difference and unclamped/wrapped
two-dimensional prediction.

Generated scans pack consecutive components, splitting at four components or
ten samples per actual interleaved MCU; a single component may have 4x4 factors.
`separate_components` forces one scan per component. A borrowed `LosslessScan`
script specifies IDs, predictor, Pt and restart rows per scan. Separate scans
can visit components in any order, and IDs inside one scan follow frame order.
Each component appears exactly once. The emitted DRI is MCU columns multiplied
by restart rows, must fit u16, and is redefined (including zero) when a later
scan requires another interval. Pixel/sample/metadata/scan/output budgets and
public model mutations are checked. Planar rewriting retains the source image's
limits even if the caller supplies looser encoding limits.

The independent encoder gate uses native output and both C/native decoders:

```text
python crates/deadlib-image/jpeg/tests/reference_lossless_encode.py --native crates/deadlib-image/jpeg/experiments/arithmetic-encode-target/release/examples/lossless_encode.exe --inspect crates/deadlib-image/jpeg/experiments/arithmetic-encode-target/release/examples/lossless_inspect.exe --oracle crates/deadlib-image/jpeg/experiments/oracle-build/Release/jpeg-lossless.exe --out crates/deadlib-image/jpeg/experiments/lossless-encode-gate
```

Every native file must decode identically in C and Rust and match the exact
input integers after its explicit per-component Pt. The matrix covers all
precisions, all seven predictors, both Huffman policies, all five color spaces,
both CMYK conventions, four scan layouts, row restarts and independently
C-encoded source rewrites. Boundary cases cover every Pt at P=2,8,12,15,16,
alternating full-range extrema, predictor over/underflow and restart sequence
wraparound. Tools are snapshotted and hashed; checkpoints support resuming.
Evidence is retained in `tests/reports/lossless-encode-turbo.json`.
All **4,448 cases** pass exactly, with zero C warnings: 2,520 raw encodes,
848 boundary encodes and 1,080 rewrites of independently C-encoded sources.

Native tests additionally cover common/fractional sampling, up to 4x4 factors,
odd edge padding, 255 components, mixed custom scans, DRI changes, source
polarity, shared U8/U16 input, metadata/ICC roundtrips, and invalid inputs and
limits. These extend beyond the C decoder's lossless sampling/component limits
and are not claimed as successful C comparisons. Extracting the common metadata
helpers leaves all 600 earlier C-verified DCT encoder hashes unchanged; see
`tests/reports/rewrite-lossless-refactor-turbo.json`.

Arithmetic SOF11, differential/hierarchical lossless, DNL and
incremental input/output remain required. Independent conformance corpora,
IJG/mozjpeg checks, coverage-guided fuzzing and measured lossless performance
also remain open. No complete-conformance or throughput claim is made.
