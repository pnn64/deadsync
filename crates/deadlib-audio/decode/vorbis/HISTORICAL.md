# Additional historical interoperability corpus

The seven additional files in `validation/corpus.json` come from FFmpeg's
[Vorbis samples](https://samples.ffmpeg.org/ogg/Vorbis/) and
[Ogg samples](https://samples.ffmpeg.org/ogg/). Each download has a pinned byte
length and SHA-256. Media stays in ignored scratch storage; no audio is added to
the repository. These are historical interoperability and rejection cases, not
an official Vorbis conformance suite or additional ITG song-pack coverage.

## Strict decoding and independent PCM

Both valid files decode without compatibility options. Complete native output
is compared with libvorbisfile 1.3.7 and Symphonia 0.6.1. Counts, 48 kHz rate,
channel layout, frame positions and timestamps must match exactly. Symphonia's
speaker-labelled planes are matched to the Vorbis channel order, including the
six-channel stream's centre, rear and LFE channels.

| File / encoder vendor | Channels | Frames per channel | Peak / RMS error against libvorbisfile | Peak / RMS against Symphonia |
| --- | ---: | ---: | ---: | ---: |
| `coyote.ogg` / Xiphophorus libVorbis I 20001031 | 2 | 3,671,424 | 1.886487007e-5 / 1.246926355e-6 | 1.862645149e-5 / 1.245805132e-6 |
| `test6.ogg` / AO; aoTuV b2 [20040420], based on Xiph.Org 1.0.1 | 6 | 508,416 | 1.797452569e-7 / 5.729303272e-9 | 1.192092896e-7 / 3.614739356e-9 |

That is 10,393,344 channel samples per independent decoder. The existing
floor-zero bounds (peak 5e-5, RMS 3e-6) apply to `coyote.ogg`; the existing
floor-one bounds (3e-6, 5e-7) apply to `test6.ogg`. Bounds scale by the greater
of one and the reference peak, as in the other differential tests. Neither
bound changed for these files. No alignment, gain, resampling or tolerance
adjustment is used.

Raw-packet libvorbis comparisons additionally verify every block size, overlap
frame count and untrimmed sample. Raw synthesis returns 3,672,128 frames for
`coyote.ogg` and 508,480 for `test6.ogg`, before final trimming. Four `coyote.ogg`
packets exercise nominal end-of-packet handling; `test6.ogg` has none. The
[Vorbis I specification, sections 2.1.8 and 4.3](https://xiph.org/vorbis/doc/Vorbis_I_spec.html)
permits this after a valid mode/window prefix. The test records this disposition
and compares the resulting PCM, rather than treating every end-of-packet
condition as corruption.

For each file, two passes through the same reset byte-stream decoder cycle
input lengths 1, 7, 17, 255, 4,096 and 65,025. Every returned PCM bit matches
sequential native output. Another 128 requests per pre-roll strategy cover
start/end boundaries and deterministic arbitrary positions: **512 seeks** in
total, using packet and page reads, both packet-mode and page-only pre-roll,
and complete suffixes at the endpoint. Returned PCM bits and absolute frame
positions must match the sequential output. This adds evidence for reset and
overlap state without using a numerical tolerance to hide seek differences.

The independent Ogg tests compare 381 page headers and 4,402 complete packet payloads
with libogg 1.3.6 for these two files and `1sec.ogg`. The latter has valid Ogg
framing despite its invalid codec endpoint; codec sample-count rules do not
belong in the shared container.

## Rejection cases

Five files retain the same specific errors through pull decoding, 17-byte
streaming, and indexed construction. Both zero-span policies and all four
indexed compatibility-option combinations are checked: **40 rejection paths**.
The existing exceptions remain limited to their documented cases in
[LEGACY.md](LEGACY.md).

| File | Observed rejection |
| --- | --- |
| `1sec.ogg` / Lavf55.37.100 | EOS granule 44,619 exceeds the 44,608 frames available from raw packet synthesis. |
| `virginradio-three-consecutive-chains.ogg` | First link jumps from page sequence 2 to 1,007; expected page 3, before any PCM output. |
| `mgs1-sample1.ogg` | Audio-page CRC: stored `442b2d41`, computed `32e99750`. |
| `mgs1-sample2.ogg` | Audio-page CRC: stored `f68f9f2e`, computed `1fc62c31`. |
| `mgs1-sample3.ogg` | Audio-page CRC: stored `40f0c90f`, computed `ebf89a80`. |

The `1sec.ogg` raw-packet differential test independently confirms every
libvorbis block size and frame count, plus PCM within the ordinary floor-one
bounds (peak 4.563480616e-8, RMS 6.539169770e-9 across 89,216 samples).
libvorbisfile reports and emits 44,608 frames, agreeing with both raw decoders,
despite the encoded EOS of 44,619. A positive initial offset of 11 could explain
that endpoint, but the file puts all audio packets on one page. Appendix A.2
requires the second audio packet to flush its page for a nonzero initial offset.
Native strict mapping rejects this framing. Endpoint clipping does not change
the inferred initial timeline or fabricate PCM.

The game files' [upstream readme](https://samples.ffmpeg.org/ogg/Vorbis/MetalGearSolid/readme.txt)
identifies their source and reports historical decoder failures. This test
only establishes the concrete CRC failures on the pinned bytes. It does not
infer whether further modified framing or codec payloads would decode after
repair. No CRC rewriting, sequence-gap concealment or automatic fallback is
performed.

## Reproduction

Build the pinned test oracles using [the Vorbis instructions](tests/oracle/README.md)
and [the Ogg instructions](../ogg/tests/oracle/README.md), then run:

```powershell
python crates/deadlib-audio/decode/scripts/fetch_corpus.py --group ffmpeg
$env:DEADLIB_VORBIS_EXTERNAL_CORPUS = "$pwd/target/deadlib-audio-reference/corpus"
$env:DEADLIB_OGG_EXTERNAL_CORPUS = $env:DEADLIB_VORBIS_EXTERNAL_CORPUS
$env:DEADLIB_VORBIS_HISTORICAL_REPORT = "$pwd/target/deadlib-audio-reference/historical.tsv"
cargo test -p deadlib-audio-decode-vorbis --release --test historical -- --include-ignored --nocapture
cargo test -p deadlib-audio-decode-ogg --release --test libogg libogg_historical_vorbis_pages_and_packets -- --include-ignored --nocapture
```

[PCM results](validation/2026-10-27-historical.tsv) and
[run provenance/checks](validation/2026-10-27-historical-checks.json) retain the
measurements, source/input/oracle hashes, commands and exit codes. Production
sources are unchanged from `73f09ed2f`. The three new Vorbis tests pass in
release and debug; the new Ogg test passes in release. Formatting and both
packages' all-target/all-feature Clippy checks pass. This validation adds no
codec dependency or application integration. Wider real song packs, official
conformance material and the remaining performance/encoder work are still required.
