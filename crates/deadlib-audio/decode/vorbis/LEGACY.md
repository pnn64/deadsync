# Explicit compatibility policies

All options below default to **false**. Strict rejection remains available and
is tested on the same malformed files. CRC, packet limits, sequence numbers,
continuation flags, BOS grouping and stream closure are still validated.

## Zero granule on an unfinished page

Some historical encoders/tag writers put zero on unfinished secondary-header
pages. An unfinished page has no completed packet and should have granule -1;
Xiph's [2009 clarification](https://lists.xiph.org/pipermail/vorbis-dev/2009-March/019830.html)
explains this distinction despite the header-section wording.

`OggOptions::allow_zero_span_granules` and the corresponding `SeekOptions` flag
ignore **only zero** on a nonempty page that completes no packet. The ignored
value never updates the timeline or enters the seek landmarks. Raw pages and
their CRCs are unchanged. Other nonzero span granules remain errors. Applied
exceptions are counted in `compatibility().zero_span_granules` (on the indexed
reader, use `index().container().compatibility()`). Reset clears the counts while
preserving the selected policy.

The pinned historical floor-zero file has one such page, at byte 4,310. With the
flag enabled, native pull/streaming/indexed paths produce **6,262,786 frames per
channel**, matching libvorbisfile counts and the existing floor-zero PCM bounds.

## Indexed clipping to an inconsistent final endpoint

The ITGmania-shipped `Mecha-Tribe Assault.ogg` has final granule 4,630,500 after
a preceding page at 4,692,672. Its declared endpoint is earlier than PCM that a
forward-only decoder may already have emitted. Strict mode rejects it. A
forward-only reader cannot safely repair that endpoint without advance knowledge
or unbounded buffering, so its options do not include this exception.

`SeekOptions::trim_to_final_granule` makes the indexed reader honor the declared
final endpoint, even when trimming crosses packets or preceding pages. Indexing
still verifies all packet-mode positions and non-EOS granules. Output is clipped
against the known endpoint throughout decoding. EOF may neither exceed the raw
decoded end nor precede the inferred retained start. Subsequent payloads are
still decoded/validated through EOS when reading the complete suffix.

The selected stream reports `extended_end_trim` and `end_padding_frames`. These
identify cross-packet trimming and the total number of raw trailing frames
discarded, rather than hiding the repair in an ordinary conformance result.
Decreasing EOS values are omitted from monotonic container/codec seek landmarks;
their original values remain in the indexed page headers. Index-free granule
bisection requires monotonic input and is not a repair path for this case.

For Mecha-Tribe Assault, native output contains **4,630,500 frames per channel**
and discards 132,828 raw trailing frames. libvorbisfile 1.3.7 reports that same
declared length but actually emits **4,762,304 frames**. The test compares native
output with reference samples inside the declared interval, without shifting,
resampling or gain adjustment; it separately records the reference count mismatch.

```rust
use std::io::Cursor;
use deadlib_audio_decode_vorbis::{OggSeekReader, SeekOptions};
# fn example(bytes: &[u8]) -> Result<(), deadlib_audio_decode_vorbis::Error> {
let mut reader = OggSeekReader::with_options(Cursor::new(bytes), SeekOptions {
    trim_to_final_granule: true,
    ..SeekOptions::default()
})?;
while reader.read_next(|event| { std::hint::black_box(event); })? {}
# Ok(())
# }
```

The independent container offers the lower-level `ValidationOptions` flags
`allow_zero_span_granules` and `allow_decreasing_eos_granule`. Container acceptance
does not itself repair PCM. `OggDecoder` enables only the first through its
explicit option; indexed Vorbis seeking owns the endpoint policy.

## Pinned evidence and reproduction

`validation/corpus.json` pins hashes, sizes and source URLs, including the exact
ITGmania commit and paired SSC charts. Media is downloaded to ignored scratch
storage and never redistributed. See `tests/oracle/README.md` for reference builds.

```powershell
python crates/deadlib-audio/decode/scripts/fetch_corpus.py
$env:DEADLIB_VORBIS_EXTERNAL_CORPUS = "$pwd/target/deadlib-audio-reference/corpus"
$env:DEADLIB_VORBIS_LEGACY_REPORT = "$pwd/crates/deadlib-audio/decode/vorbis/validation/legacy.tsv"
cargo test -p deadlib-audio-decode-vorbis --release --test legacy -- --include-ignored --nocapture
```

[Recorded results](validation/2026-10-26-legacy.tsv) cover both policies, two
strict historical/shipped songs, and 36 repeated native seeks against reference
PCM. The diagnostic example accepts `--allow-zero-span-granules` and
`--trim-to-final-granule` separately. No vendor-name heuristic, automatic fallback,
CRC bypass, packet-hole concealment or EOS fabrication is applied.
