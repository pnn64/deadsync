# Current ITGmania bitmap controls

`capture.json` records actual primary PNG/BMP/GIF/JPEG loader code from
`704cdd5624155f743b59b8b64fedeb0588dae067`, using the harness Rust/native modules
at `91f69c2333db9600c7ab251145ac7269a46cfd64` (0.1.95). The private codec adapter
is labeled `0.1.95+primary-codecs`; it is not the unmodified pinned harness.
`build-adapter.patch`, binary/source hashes and seventeen identical support-file
proofs record the adaptation. Ordinary tests need only repository files.

There are 35 defined BMP/GIF decoding controls, with source pixels and 33 complete
prepared upload images. The two Sprite images with allocation padding retain
source pixels and actual prepared dimensions; undefined allocation bytes are
not serialized. The primary loaders accept both BMP inputs named `.png`.

The minimal original OS/2 one-bit BMP exposes an upstream stb bug: its palette
size is `(32 - 14 - 24) / 3 = -2`. The loader does not initialize its palette
before indexing it. Such pixel values cannot be a valid parity reference.
`capture.json` retains a defined header-only observation for that original.
`os2-1-padded.bmp` retains the same header, palette and pixel rows while adding
twelve gap bytes before the pixel data. This makes the native palette size two
and provides defined observations for both Model and Sprite paths.

The original Windows `win8-unused-entry.bmp` stores four palette entries while
referencing index 31. Its primary decoder pixels are also undefined. That input
retains a header-only native observation and a Rust rejection check.
`win8-full-table.bmp` provides a valid full palette for defined pixel comparison.

Build the codec oracle without modifying either sibling checkout:

```powershell
python -B -X utf8 scripts/build_primary_oracle_adapter.py --harness-source ../itgmania-harness --itgmania-source ../itgmania --codecs-only --output target/primary-codec-oracle
```

This validates the image-loader component. It does not certify the remaining
primary engine domains or full-engine harness integration.

Refresh these observations from the bundled recipe with the recorded adapter:

```powershell
python -B -X utf8 scripts/refresh_primary_bitmap_fixtures.py --adapter target/primary-codec-oracle/adapter.json --itgmania-source ../itgmania --output-root .
```
