# Source-pyramid encoder regressions

Twelve native raster encodes cover P=2/16 predictive coding through both entropy
coders and P=8/12 sequential/progressive DCT through both coders. The DCT cases
start with a partial DC-only progression and end with a same-resolution spatial
correction. All completed component planes match their sampled source exactly.

`*.source.raw` contains original interleaved little-endian u16 integers.
`*.frame-i.coef` and `*.frame-i.samples` contain JCO1/JLP1 expectations serialized
before entropy encoding. `*.samples` contains the exact final JLP1 target.
The JSON manifest records generator parameters in each case name, original and
C-input hashes, tool hashes, comparisons and every binary file hash.

Generate and independently check these with `examples/pyramid_encode.rs` and
`tests/reference_pyramid.py --fixtures <directory>`. The complete 560-case
report is `tests/reports/pyramid-jpegxt.json`. Base Pt workarounds and the C
decoder's omitted-EXP defect are recorded explicitly; the native JPEG bytes
are not changed. See HIERARCHICAL-VALIDATION.md for interpretation and commands.

These are independently checked native encoder regressions, not an independent
JPEG standards conformance corpus. The Rust suite checks the original source,
all 30,164 truncated JPEG prefixes and 1,200 bounded mutations without requiring
a reference executable.
