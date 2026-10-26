# deadlib-audio-decode-vorbis

Independent, in-development Vorbis I implementation. The only production
dependency is the sibling `deadlib-audio-decode-ogg` container. Symphonia and
libvorbis serve only as test oracles. Neither new crate is integrated into
DeadSync's decoder or playback pipeline.

Current APIs operate on **raw Vorbis packets**, independently of Ogg:

* `bitpack::{BitReader, BitWriter}`: native LSB-first arbitrary-width integers
  (0..=32), lookahead and sticky end-of-packet semantics.
* `Identification::{parse, encode}`: stream parameters, bitrate hints, block
  sizes and framing validation. `channel_order` exposes mapping-zero speaker
  order for one through eight channels; larger counts remain explicitly unassigned.
* `Comments::{parse, encode}`: borrowed eight-bit-clean metadata, duplicates and
  unknown/unstructured vectors preserved. `fields` and `values` expose valid
  field names and ASCII-insensitive lookup. Strict metadata parse errors are
  separate from a future decoder's mandated nonfatal comment error handling.
* `codebook::{Codebook, Huffman}`: ordered/unordered/sparse entry lengths,
  entry-order Huffman assignment, scalar and VQ decode, lookup types 0/1/2,
  exact integer lattice sizes and the specification's single-entry erratum.
* `Setup::parse`: codebooks, time placeholders, floor-zero/one configuration,
  all residue configurations, mapping-zero submaps/coupling, modes and framing.
  Index/range/tree validation precedes use. Floor-one neighbors and X ordering
  are precomputed. `read_audio_header` parses a raw packet's mode/window prefix.

```rust
use deadlib_audio_decode_vorbis::{Identification, Limits, Setup};
# fn example(id_packet: &[u8], setup_packet: &[u8]) -> Result<(), deadlib_audio_decode_vorbis::Error> {
let id = Identification::parse(id_packet)?;
let setup = Setup::parse(setup_packet, &id, &Limits::default())?;
assert!(!setup.modes().is_empty());
# Ok(())
# }
```

**PCM synthesis, seeking and native audio encoding are not implemented yet.**
Header writers are not audio encoders. Do not use this crate as a replacement
for a complete decoder until those stages and the full conformance gates pass.
See [COVERAGE.md](COVERAGE.md) and the parent work plan for the remaining scope.

Run `cargo test -p deadlib-audio-decode-vorbis`, or use its manifest path.
The parent `scripts/verify_independent.ps1` copies only the two crates into a
separate workspace, runs their tests offline and checks production dependency
trees. Optional [external tests](tests/oracle/README.md) require pinned C oracle
builds. Resource limits are configurable application policies, not format limits.
