# Native libpng oracle (test only)

The C helper emits packed native samples without color, gamma, alpha, or depth
transformation. It retains libpng's error/setjmp boundary inside a separate
process. CMake and a C compiler are needed only for this optional external check.

From the workspace root (PowerShell; choose any scratch reference directory):

```powershell
git clone --depth 1 --branch v1.6.50 https://github.com/pnggroup/libpng.git target/deadlib-image-reference/libpng
git clone --depth 1 --branch v1.3.1 https://github.com/madler/zlib.git target/deadlib-image-reference/zlib
$referenceRoot = (Resolve-Path target/deadlib-image-reference).Path
cmake -S crates/deadlib-image/png/tests/oracle -B target/deadlib-image-reference/oracle-build "-DREFERENCE_ROOT=$referenceRoot" -A x64
cmake --build target/deadlib-image-reference/oracle-build --config Release --target deadlib_png_oracle --parallel 4
$env:DEADLIB_LIBPNG_ORACLE = (Resolve-Path target/deadlib-image-reference/oracle-build/Release/deadlib_png_oracle.exe).Path
cargo test -p deadlib-image-png --release --test libpng -- --ignored
cargo test -p deadlib-image-png --release --test profiles -- --ignored
```

Omit `-A x64` and use the platform's executable path for other CMake generators.
Pinned source commits: libpng `2b978915d82377df13fcbb1fb56660195ded868a`, zlib
`51b7f2abdade71cd9bb0e7a373ef2610ec6f9daf`. Reference source stays outside
the production crate and is never linked to it. The test fails if an oracle is
missing; normal `cargo test` explicitly reports it as ignored.

`--metadata FILE.png` emits ICC length and Exif length (two big-endian u32s),
followed by the exact uncompressed profile and Exif bytes retained by libpng.
The profiles test checks 480 encodings across eight pinned independent ICC v2/v4
profiles, all 15 native PNG formats, both interlace settings, both native writers
and both Exif byte orders. This supplements the existing 1,061 native-sample
comparisons; it does not introduce any C linkage into the production library.
