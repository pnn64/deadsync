# Native JPEG snapshot helper

Source: crates/deadlib-image/jpeg/bench/src/bin/native_snapshot.rs

The helper is ready for the root agent's format/build/review. This audit agent did
not compile it or run any decoder/test/benchmark. No production source was edited.

## Commands

From the repository root, using the same input list files on each run:

    native_snapshot size --files-from target/jpeg-perf-20261119/jpeg-corpus-speed.txt --files-from OTHER_LIST
    native_snapshot capture target/jpeg-perf-20261119/native-baseline --files-from LIST
    native_snapshot verify target/jpeg-perf-20261119/native-baseline --files-from LIST

The size mode decodes and serializes into a counting writer, creating no files;
the final COMPLETE line reports the exact projected saved byte count. Capture
requires a nonexistent output directory and never overwrites a snapshot. It caps
total encoded bytes, including the manifest, at 1,000,000,000 by default.
--max-bytes N overrides that cap. --relative-to DIR selects the relative-path
identity base; otherwise the working directory is used. Use the same base when
verifying. List entries are UTF-8 paths relative to the working directory, not to
the list's parent; blank lines and # comments are ignored. Explicit file inputs
accept any extension; directory recursion selects .jpg, .jpeg, and .jpe.

## Recorded native behavior

Each input is decoded without a reference-codec eligibility filter:

- Pixel API: strict true/false crossed with Nearest/Linear upsampling.
- Coefficient API: strict true/false, metadata retained.
- Lossless API: strict true/false, metadata retained.
- Hierarchical API: strict true/false, metadata retained.
- Every API Result preserves success/error identity; errors encode the exact
  enum variant and message (or exact Debug name for std::io::ErrorKind).
- Pixels include width, height, ColorType, bit depth, Samples variant and exact
  U8/U16 values.
- Coefficients include full frame/component specifications, dimensions, padded
  block geometry, quantizers, successive-approximation bands, every signed
  coefficient, color space, CMYK polarity, scans and consumed position.
- Lossless images include full frame/component specifications, exact plane
  samples, per-component predictor/point transform, color space, scans/consumed.
- Hierarchies include their header, frame order, per-frame header, expansion,
  offsets, first scan/count, DCT or lossless payload including coding, final
  color space, global scans/consumed.
- All raw APIs include ordered metadata marker, payload bytes, source offset and
  preceding_scans. No metadata is reduced to a color-hint-only representation.
- Every ordinal snapshot embeds its relative path with lossless platform path
  encoding, ordinal, corpus count, complete original input bytes and length,
  and default resource limits. Changed/reordered input cannot silently compare.

The helper excludes private allocation accounting and Vec capacities, since
those are implementation details under optimization. It does not separately
invoke coefficient/hierarchy reconstruction convenience APIs; native pixel
decoding and the requested raw coefficient/plane APIs are covered.

## Storage and verification

Numeric arrays use deterministic zero/literal runs with explicit little-endian
u16 literals; negative coefficients retain their exact two's-complement bits.
Identical pixel and raw-policy Results use explicit references only after full
field/array/error equality. There are no lossy fingerprints or probabilistic
hash comparisons.

The comparison writer streams baseline bytes through a 64 KiB reader and an
8 KiB comparison buffer. It reports the first differing byte and rejects both
short and trailing baseline data. The manifest is written last as a completion
record and binds the corpus's ordered paths and ordinal snapshot lengths. A
failed capture leaves an incomplete directory that must not be used as baseline;
a fresh capture directory is required. No unsafe code or dependencies were added.
