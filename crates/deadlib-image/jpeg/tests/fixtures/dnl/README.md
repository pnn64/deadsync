# DNL regression corpus

Twenty-five files are independently authored by the pinned JPEG XT encoder's
`-n` option. Only its documented empty DAC/EXP00 segments are removed. Original
DNL, zero SOF heights and all entropy bytes remain unchanged. One native baseline
file is additionally retained with an exact original-header C coefficient check.
All thirteen JPEG-1 frame processes are represented at the relevant precision
boundaries. The manifest distinguishes original-header C successes from
explicit known-header entropy comparisons for the reference's DNL limitations.

`*.frame-i.coef`/`*.frame-i.samples` contain JCO1/JLP1 frame expectations checked
against independent C observations. `*.samples` contains matching native target
planes; mixed DCT/lossless targets depend on the native IDCT. These files are
codec regressions, not an independent standards conformance corpus.

The Rust suite checks every frame, target, all 11,861 truncated prefixes and
2,600 bounded mutations without requiring a reference library. Generate the C
inputs with `tests/generate_dnl_reference.py`; validate/retain them through
`tests/reference_dnl.py --only-additional --fixtures <directory>`, then add the
baseline using `tests/pin_dnl_baseline.py`. DNL-VALIDATION.md gives complete
commands, reference patches, numerical scope and observed oracle limitations.
