# Native-precision APNG composition fixtures

240 independent byte fixtures cover all 15 native formats, plain/Adam7, both
default-image roles and four geometry/disposal patterns. Each animation has
eight frames, giving 1,920 full-canvas RGBA16 answers. All six disposal/blend
states and nine adjacent disposal transitions occur. Pixels include hidden
colors, alpha endpoints, fractional alpha and values using the low 16-bit bits.

The Rust test-only fixture builder writes literal PNG chunks, bitwise CRCs,
stored DEFLATE and Adler checksums. It does not call the production encoder.
The Python generator reconstructs each original frame as a static PNG and
decodes its native samples with the external C libpng oracle. Python `Fraction`
then computes straight-alpha OVER, rounded to nearest RGBA16 after every display.
It snapshots the entire canvas and applies disposal immediately after recording
the displayed result. Production composition instead saves frame rectangles
and defers disposal until the next frame. An additional Rust u128 fraction
model checks the same answers independently of the production u64 weighted sum.

`manifest.json` records every PNG/golden hash, dimensions, frame counts, sample
format, C binary hash and Python model/generator hashes. Goldens use the same
`DLAPNGR1` container as the WPT answers, compressed with zlib. Tests compare
uncompressed answers so zlib version differences cannot change the result.

Reproduce into fresh scratch directories from the workspace root:

```powershell
cargo run -p deadlib-image-png --release --example compositor_fixtures -- target/compositor-inputs-fresh
python crates/deadlib-image/png/scripts/generate_compositor_goldens.py --input target/compositor-inputs-fresh --oracle target/deadlib-image-reference/oracle-build/Release/deadlib_png_oracle.exe --output target/compositor-answers-fresh
```

The optional `regenerate_precision_goldens_from_independent_c_frames_and_python_fractions`
test regenerates all answers and compares their uncompressed contents. Neither
Python, C nor a reference codec enters the production dependency graph.
