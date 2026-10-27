# Residue geometry and reference layout

The decoder now supports the published type-two interleave rule and an explicit
libvorbis layout for unaligned ranges. This extends exact behavior/count parity
without replacing the standards behavior with an undocumented reference quirk.
The original PCM tolerances apply to every reference block.

## The reference difference

Vorbis I sections 8.5/8.6 describe a flattened, interleaved vector, with begin,
end and partition size expressed in scalars. Deinterleaving scalar `s` places it
in channel `s % channels`, bin `s / channels`. The specification requires whole
VQ vectors per partition; it does not require channel-aligned partition ends.

Pinned libvorbis 1.3.7's `vorbis_book_decodevv_add` instead starts each partition
at channel zero and bin `offset / channels`, and stops at bin
`(offset + length) / channels`. Both integer divisions truncate. Its effective
scalar range is therefore:

```text
first = floor(offset / channels) * channels
end   = floor((offset + length) / channels) * channels
```

This can change placement and the number of codewords consumed. A final VQ
vector can contribute only some components. The two-channel probe with begin
one produced about `1.728` peak / `0.531` RMS disagreement before selecting the
reference layout; the reference-layout result is within the original bounds.

`Residue2Layout::Interleaved` remains the default. Set
`DecodeOptions::residue2_layout` to `Residue2Layout::Libvorbis` and construct a
raw decoder with `PacketDecoder::with_options` for the reference behavior.
The same field in `OggOptions` and `SeekOptions` carries this choice through
streaming, reset, index ownership and indexed synthesis. The layout controls
type-two scalar placement and consumption. Other residue types retain their
existing behavior.

```rust
use deadlib_audio_decode_vorbis::{DecodeOptions, PacketDecoder, Residue2Layout};
# fn example(setup: deadlib_audio_decode_vorbis::Setup) -> Result<(), deadlib_audio_decode_vorbis::Error> {
let decoder = PacketDecoder::with_options(setup, DecodeOptions {
    residue2_layout: Residue2Layout::Libvorbis,
    ..DecodeOptions::default()
})?;
# let _ = decoder;
# Ok(())
# }
```

The reference path rounds the endpoints before scattering. It consumes the last
codeword but clips components outside the rounded range, matching the C guard.
The normative path clips to its exact range. For conforming whole-vector
partitions, the latter preserves previous behavior. Setup validation still
requires the partition size to be a multiple of each selected VQ dimension.

Sources: [Vorbis I sections 8.3–8.6](https://xiph.org/vorbis/doc/Vorbis_I_spec.html),
[pinned vector decode](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/codebook.c),
[pinned residue decode](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/res0.c).

## Procedural coverage

`tests/support/residue_vectors.rs` constructs headers/audio independently of the
native encoder. Its four classifications have cascade masks `0`, `0x85`, `0x7e`
and `0xff`; active stages select different one-, two-, three- and four-dimensional
VQ books. Signed, exactly representable values expose interleave mistakes.
Phrasebook dimensions one/two/three cover complete and partial final groups.

The aligned matrix crosses 1/2/3/5/6/8/17/255 channels with 1/2/3/16 submaps,
all three residue types, clipped/beyond-block ranges, unoccupied submaps,
noncontiguous channel bundles, absent floors and coupling across submaps.
The unaligned matrix checks explicit reference behavior for 2/3/5/7/17 channels,
one/two submaps and unaligned begin, partition size or both.

Each case resets synthesis, supplies two pre-roll packets, a candidate byte
prefix and two recovery packets. Up to eight channels use every byte prefix;
17/255-channel runs select packet ends, short prefixes and deterministic
interior boundaries to bound reference output. Codewords, stage/classification
boundaries and floor-presence combinations vary across six seeds and both block
sizes, including all long-window flag pairs.

The shared C comparison checks exact discard decisions, block sizes, frames and
raw positions, with the unchanged per-block PCM gate. Direct scatter tests cover
both layouts, every touched/untouched plane location, exact bit positions,
truncation, zero-length rounded ranges and partial final vectors.

The 45,904-byte `residue-layout-v1.libvorbis-pcm` golden retains four independent
C responses for normal tests. Streaming tests use strict Ogg framing and an exact
368-frame endpoint. They check both layouts, fragmented input, reset and 208
repeated/boundary/backwards seeks through both pre-roll strategies, with bit-exact
native suffix PCM. The two layouts must produce different PCM on the probes.

## Final validation

| Layout matrix | Combinations | Cases | Packet outcomes | Discarded | PCM channel samples |
| --- | ---: | ---: | ---: | ---: | ---: |
| Channel-aligned, normative | 120 | 252,186 | 1,260,930 | 3,600 | 1,669,153,936 |
| Unaligned, explicit libvorbis | 30 | 80,646 | 403,230 | 900 | 166,905,184 |
| Total | 150 | 332,832 | 1,664,160 | 4,500 | 1,836,059,120 |

All 12,109 reference batches pass the original per-block peak/RMS bounds
(`3e-6` / `5e-7`, scaled by `max(1, reference peak)`), with exact counts and
positions. These are reset/prefix cases, including repeated silent/control
inputs, rather than a count of unique encoded streams. The aligned request bytes
also match the preserved initial run before adding the layout option.

Full final-source gates pass **138 release tests**, **103 extracted offline
tests** (38 external/manual tests ignored there), formatting, default/all-feature
Clippy and the two fuzz-harness unit tests. The earlier 31,606 configuration,
7,014 prefix and 55,845 finite-reference bit cases retain their coverage/counts.
The shipped assets, 528 upstream vectors, pinned external corpus and 144 native
encoding cases remain enabled. The instrumented diagnostic decoder also compiles
and lints against the new API; no profiling/timing run is performed.

The first/middle/last cases of representative batches from every geometry,
plus the four golden cases, pass **898 fresh-process C decodes** byte for byte
against the reset protocol. Independent libvorbisfile decoding of all four Ogg
probes reports/emits 368 frames per channel and matches C raw PCM bits. Its 44
sample-accurate seeks reproduce the corresponding raw suffixes exactly. Golden
recapture is also byte-identical.

Three 300-second AddressSanitizer campaigns (headers, packets, streaming) complete
**231,500 executions** without failure artifacts or slow units. Sixteen targeted
seeds exercise both layouts. Compiled inputs and binaries match the final tree;
the captured fuzz README predates its new control explanation, and the manifest
records both document hashes. Fuzzing checks bounded behavior/state consistency,
separately from independent PCM parity.

The [checks manifest](validation/2026-10-27-residue-geometry-checks.json) records
source/input/reference/binary hashes and retained artifacts. The
[reference probe](validation/2026-10-27-residue-geometry-reference-probe.py) verifies
the request digest, reset protocol, exact golden and independent Ogg/seek paths.
The [validation probe](validation/2026-10-27-residue-geometry-validation-probe.py)
records complete gates and corpus/report environment variables. Pinned references
remain libvorbis 1.3.7 `0657aee69dec8508a0011f47f3b69d7538e9d262` and libogg 1.3.6
`be05b13e98b048f0b5a0f5fa8ce514d56db5f822`; the C oracle is unchanged.

With `DEADLIB_LIBVORBIS_ORACLE` configured as in the oracle README:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --test residue_geometry -- --include-ignored --skip regenerate_residue_geometry_golden --nocapture
cargo test -p deadlib-audio-decode-ogg -p deadlib-audio-decode-vorbis --release
```

## Scope

This option addresses the type-two endpoint/phase difference specifically.
The [strict empty-book discrepancy](CONFIGURATION_PARITY.md) and the eleven
[nonfinite-reference cases](BIT_MUTATIONS.md) remain explicit. The finite-output
guard remains part of synthesis. The broader corpus, streaming, encoding and
seeking gates remain separate evidence; neither this matrix nor fuzzing proves
complete conformance. Application integration and performance optimization are
outside this milestone.
