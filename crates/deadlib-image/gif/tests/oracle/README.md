# giflib differential oracle

The standalone test helper calls giflib `DGifSlurp`, exports native raster
indices, palette bytes, rectangles, interlace flags, logical-screen fields and
graphic controls, and optionally rewrites the file through `EGifSpew`.
It does not implement or share the Rust parser, compressor, or decompressor.

Linux (giflib development headers/library and a C compiler required):

```sh
cc -O2 -Wall -Wextra tests/oracle/oracle.c -lgif -o /tmp/deadlib-gif-oracle
DEADLIB_GIFLIB_ORACLE=/tmp/deadlib-gif-oracle \
  cargo test -p deadlib-image-gif --test giflib -- --ignored --nocapture
```

Run the compiler from the GIF crate directory and Cargo from the workspace
root, or supply `--manifest-path`. The oracle path must be absolute. The test
uses every accepted upstream sample and a deterministic matrix of native files,
then decodes giflib's independently compressed rewrites. Temporary files are
removed on success. This optional test fails if its executable is unavailable;
ordinary tests need no native libraries.
