# Noteskin regression expectations

The tests run the Rust `deadlib-lua` compiler through the production noteskin
compiler. These complete bundle expectations were captured independently from
the former Lua 5.1.5 compiler using the original authored and bundled fixtures.
Every capture passed the original field-by-field comparison against the Rust
compiler before its expected data was saved.

The loader and actor version transition from 29 to 30 is checked separately.
The bundle snapshot lists every field, sorts map keys and retains frame-delay
bits, including signed zero and nonfinite payloads. Only registered temporary
directories, their derived resource-manifest keys and the repository root are
normalized. Array order, command text, loader entries and resource declarations
remain part of the comparison.

`../cache-version-29.bin` is the independently compiled old cache used to verify
invalidation without deleting the old file. `../callback-version-30.bin` is the
original callback fixture's encoded bundle with the two compiler versions
adjusted to 30, preserving the original byte comparison.

Tests never regenerate expected data from the implementation under test.
