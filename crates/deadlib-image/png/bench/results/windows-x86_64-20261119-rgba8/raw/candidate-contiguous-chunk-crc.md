# Inactive contiguous chunk CRC experiment

One production-line change in codec::chunk: hash the contiguous chunk type and payload in a single CRC update.

No production file has been edited. No compilation, timing, test execution, or candidate application was performed while preparing this file.

## Why this path

The four-byte type and payload are adjacent in every buffered PNG chunk. The old call performs a separate scalar update for the type, then another update for the payload. The candidate removes that update boundary while preserving the exact byte sequence.

This can shorten serial scalar CRC tails as well as remove one update dispatch. For example, IHDR currently processes 4 bytes then 13 bytes. A single 17-byte update can use the existing 16-byte slicing kernel plus one trailing byte. Seven-byte tIME and 37-byte tEXt payloads similarly avoid separate small update boundaries.

No benefit magnitude has been measured. This is a cheap fallback experiment, not a claim that CRC dominates photo or RGBA8 decoding.

## Expected target cases

- assets/graphics/icon/icon-16.png and icon-32.png: thirteen chunks each. Both include cHRM, bKGD, pHYs, tIME, a 983-byte zTXt, and three tEXt payloads (37, 37, and 40 bytes).
- assets/graphics/step_stats_gifs/catjam 11x14.png: 202 IDAT chunks.
- assets/fonts/cjk/_jfonts 16px [kanji-jis1] 32x32 (doubleres).png: 81 IDAT chunks.
- assets/graphics/step_stats_gifs/snoop 8x8.png: 45 IDAT chunks.
- target/png-kodak-20261117/kodim04.png is a useful large-photo control: only two large IDAT payloads, so a large end-to-end improvement from this patch would need close scrutiny.

Counts and lengths above come from the existing inventory plus read-only chunk framing inspection, not new performance measurements.

## Behavior and validation

The same length overflow, truncation, and chunk-type checks precede the CRC. The same expected CRC and same payload slice are returned. Empty chunks, optional-metadata recovery, APNG chunks, chunk ordering, input limits, and subsequent output remain unchanged. No unsafe code or allocation is added.

Existing checksum tests compare every length through 521 bytes and fragmented boundaries against a bitwise IEEE oracle; buffered/streaming decoder comparisons and metadata CRC recovery tests should also remain exact. Pay attention to payload lengths 124..127 because including the type moves them across the existing 128-byte hardware-dispatch threshold.

The root agent should check and apply the patch only after its current baseline is complete. Measure public decode and equivalent RGBA8 consumption, including all validation and metadata work. Retain only if repeated targeted and full-corpus measurements justify it.
