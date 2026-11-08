# Pinned WPT APNG rendering corpus

The original files are from
[web-platform-tests/wpt](https://github.com/web-platform-tests/wpt/tree/84dd53268b3fa142337931b8ab4706e59ffc8d12/png/apng),
commit `84dd53268b3fa142337931b8ab4706e59ffc8d12`. All 85 upstream files retain
their exact bytes. `upstream.json` records paths, sizes and SHA-256 hashes;
`LICENSE.md` retains the upstream BSD-3-Clause license. The original HTML,
generation sources and descriptions remain available for inspection.

The support directory contains 35 APNG inputs and seven static PNG references.
The animations cover both default-image roles, SOURCE/OVER, all three disposal
operations, first-frame PREVIOUS, frame rectangles, delays/plays, empty and split
fdAT, a 128-frame repeated blend, palette/transparency, grayscale/alpha and
16-bit RGBA. The WPT sources describe how their display references were made.

`goldens/` contains 201 complete displayed canvases at native RGBA16 precision.
These are produced without calling this Rust codec: the pinned test-only libpng
oracle decodes each frame as a standalone PNG, preserving its native samples;
`scripts/generate_apng_render_goldens.py` expands those samples and composites
them using Python's exact `Fraction` arithmetic. It snapshots the whole canvas
and applies disposal after recording a frame, independently of the native
implementation's saved-region and deferred-disposal strategy. Each displayed
frame rounds to the nearest integer at 16-bit precision. No floating point,
gamma, ICC or HDR transformation is applied.

`024.png` is valid: its default fcTL precedes acTL. PNG requires acTL before
IDAT, with no extra requirement that it precede fcTL. This input exposed the
native parser's former ordering restriction; it must render its animation.

The golden files are zlib-compressed binary records, independently decompressed
by flate2 in the Rust test. The uncompressed header is `DLAPNGR1`, then six
big-endian u32 fields: width, height, animation flag, default-image role, plays,
frame count. Each frame contains the 22 fcTL bytes after its sequence number,
then the entire canvas as big-endian straight RGBA16. `goldens/manifest.json`
records the generator/oracle/input/output hashes and per-file dimensions/counts.

Regenerate into a fresh directory using the oracle built from `tests/oracle`:

```powershell
python crates/deadlib-image/png/scripts/generate_apng_render_goldens.py --oracle target/deadlib-image-reference/oracle-build/Release/deadlib_png_oracle.exe --output target/wpt-apng-regenerated
cargo test -p deadlib-image-png --release --test apng_rendered
cargo test -p deadlib-image-png --release --test apng_rendered regenerate_exact_render_goldens_with_independent_libpng_and_fractions -- --ignored --nocapture
```

Set `DEADLIB_LIBPNG_ORACLE` for the ignored regeneration test, and optionally
`DEADLIB_PYTHON` if Python 3 is not called `python`. Normal offline tests consume
the checked-in answers and do not require either executable. Regeneration checks
uncompressed answers, allowing different zlib versions to choose different
compressed representations.

The 28 applicable published final-state references also match at their 8-bit
precision in the recorded run. The harness allows one 8-bit RGB level for
reference quantization, while the separate RGBA16 golden comparisons are exact.
The remaining display reftest, `062.png`, requires Display-P3 to sRGB color
management; its stored-space frames still have exact RGBA16 goldens. Color
management is explicitly caller-controlled by the native compositor API.
