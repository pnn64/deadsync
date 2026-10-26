# Standards and validation evidence

The implementation is based on the official
[GIF87a specification](https://www.w3.org/Graphics/GIF/spec-gif87.txt) and
[GIF89a specification](https://www.w3.org/Graphics/GIF/spec-gif89a.txt), including
the deferred-clear clarification. Production code is safe Rust and depends only
on shared `deadlib-image` storage. No application loader is connected to this crate.

| Capability | Implementation and executable evidence |
|---|---|
| GIF87a/89a header, logical screen, color resolution, aspect ratio, background | `Header`, incremental parser; native, malformed and giflib tests |
| Global/local tables, 2 through 256 entries, exact indices/colors, sort flags | native palette-size matrix; corpus comparisons; giflib palette bytes (local sort unavailable in giflib) |
| Native variable-width LZW, 2..8 minimum, 12-bit maximum, little-endian packing | both reference encode/decode directions in `native.rs`; independent code lists in `malformed.rs` |
| Clear anywhere, optional initial clear, repeated clear, frozen full dictionary, EOI widths | `deferred_clear_frozen_dictionary_and_end_width_boundaries` |
| KwKwK, longest legal phrase, exact expansion bounds | hand-packed fixtures and `longest_dictionary_chain_and_exact_output_limits` |
| Four-pass interlacing, short and odd heights | palette/dimension matrix, upstream interlaced samples, giflib rewrites |
| Multiple images, offsets, frame boundaries, exact delay and input flags | native matrix, corpus, giflib protocol and independent compositing fixtures |
| Transparency without modifying destination pixels | corpus vs gif-dispose; manual expected RGBA and independent full-canvas model |
| All disposal values; Background/Previous; consecutive and first Previous | `animation.rs`, 512-graphic independent snapshot model, corpus vs gif-dispose |
| Comments, application, plain text and unknown extensions | payload/sub-block/order roundtrips, XMP/ICC corpus, empty extensions, scope tests |
| Plain-text control scope, cells, palette, clipping, transparency and disposal | `extensions_roundtrip_scope_and_text_rendering`, `TextRasterizer` interface |
| NETSCAPE2.0 and ANIMEXTS1.0 loops | typed helper/recognizer, finite/infinite tests and corpus; playback policy documented in README |
| Incremental arbitrary fragments, buffer reuse | every split of independently constructed metadata-rich file, 1-byte through 1024-byte chunks, recycling tests and fuzz target |
| Indexed encoding, no remapping or quantization | native/Rust gif/giflib differential directions; exact semantic model roundtrips |
| Cumulative input, pixel, metadata, frame and block limits | `cumulative_limits_and_encoder_validation`, sanitizer fuzzing |
| Malformed/truncated streams, poisoned decoder/writer, strictness controls | all truncations of hand-built GIF, invalid syntax/palette/LZW/rectangles, seven upstream regression/crash/stall fixtures |

## Executed validation (2026-10-26)

- Windows x86-64 Rust 1.99.1: normal tests and all-target Clippy with warnings
  denied. The corpus compares all accepted frame indices, local/global palettes,
  rectangles, controls and full RGBA canvases with gif 0.14.2/gif-dispose 6.0.0.
- Debian WSL x86-64, giflib 5.2.2: 73 oracle files; 64 native files/256 frames
  decoded by giflib and independently recompressed with EGifSpew, then decoded
  by this crate. Nine strict-accepted upstream samples also compared with giflib.
- Nightly Rust, LLVM coverage and AddressSanitizer: decode 29,392 runs and LZW
  66,637 runs, 61 seconds each, seed 1327; committed smoke logs in `fuzz/reports`.
  Fuzzing has bounded resources and is not an exhaustive proof.

## Corpus and oracle policy differences

The upstream image-gif corpus is pinned with license and SHA-256 checksums.
Ten of its thirteen samples pass strict whole-stream decoding. `moon_impact`
has a reserved GCE bit and data after the trailer; its fourteen frames are
additionally verified through the compatibility/embedded-stream API.
`gifplayer-muybridge` lacks a mandatory EOI; `set_hsts` uses an out-of-range
background index. These are explicitly rejected, including in compatibility
mode for the latter two. Three upstream regression images are valid (two
clear-heavy images, one unknown extension); all are retained and roundtripped.
The four crash/stall images are rejected under bounded resources.

giflib 5.2.2 rejects the valid `bufo-thinks-about-fishsticks` empty-comment sample;
Rust gif, an independent empty-comment fixture and the normal suite cover it.
giflib also loses local sort flags during decode/rewrite; its oracle checks
all palette bytes but does not use it as evidence for that flag.
The Rust gif high-level reader defaults a missing control to Keep; the native
model preserves its absence. Explicit controls are compared exactly.
`gif-dispose` uses transparent canvas/background disposal. Comparisons select
that same explicit native policy. Specification background behavior is tested
against independently specified expected pixels, not inferred from that policy.

## Deliberate boundaries

- Raw decoding can retain a stream without a color table. Rendering requires
  the local/global/external table appropriate to that graphic. External tables
  support GIF's saved-global-table protocol without inventing colors.
- Text fonts are not defined by GIF. The caller supplies a `TextRasterizer`;
  the codec performs all GIF layout and animation operations. No platform font
  or font file dependency is introduced.
- Playback scheduling, finite/infinite repetition execution and user-input
  events belong to the host. Original timing and control values are retained.
- Application data (including ICC/XMP) remains opaque; color management and
  optional lossy palette quantization are outside this codec.
- Encoding normalizes GCE placement and recompresses LZW; it preserves valid
  indexed samples, palettes and meaningful extension data, not original bytes.
- Bounds and strictness settings are explicit. The encoder writes conforming
  rectangles/palettes and will reject malformed compatibility-decoded models.
- Input is incremental, output events own one complete graphic. Mid-image row
  callbacks are not provided; one image's decoded indices must remain in memory
  for the output-referencing LZW dictionary. Recycle frames for bounded playback.

Performance evidence and remaining tuning opportunities are in `BENCHMARKS.md`.
