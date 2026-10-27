# Initial native Vorbis I encoder

`PacketEncoder` performs native PCM analysis, quantization and audio-packet
writing. `OggEncoder<W: Write>` adds the independent native Ogg paginator and
the three Vorbis headers. Production code calls no external codec and uses no
pre-encoded audio, reference codebooks, unsafe code or additional dependencies.
This is an initial fixed-block encoder. Rate control, perceptual analysis and
competitive encoding performance remain required work.

## Ogg output

```rust
use deadlib_audio_decode_vorbis::{Comments, EncodeOptions, OggEncoder, Error};

fn encode_stereo(pcm: &[f32]) -> Result<Vec<u8>, Error> {
    let comments = Comments {
        vendor: b"my application",
        comments: vec![b"TITLE=Example"],
    };
    let options = EncodeOptions { sample_rate: 48000, ..EncodeOptions::default() };
    // Choose a unique serial within the physical stream.
    let mut encoder = OggEncoder::new(Vec::new(), 123, options, &comments)?;
    for chunk in pcm.chunks(2 * 257) {
        encoder.push_interleaved(chunk)?;
    }
    encoder.finish()?;
    Ok(encoder.into_inner())
}
```

Input chunks may have any whole-frame length, including zero. `push_planar`
accepts equally sized planes. Both layouts produce identical packets for the
same sample sequence, independent of chunk boundaries. Samples must be finite
and within `[-4, 4]`; this is an encoder input policy allowing headroom over
nominal unit amplitude. The encoder rejects invalid input without clipping it.
It performs no resampling or channel permutation. Supply Vorbis mapping-zero
channel order; `channel_order` describes the assigned layouts through eight
channels. Counts 9..255 preserve caller plane order with unassigned speaker
identities. Sample rate is a positive u32 metadata value.

`EncodeOptions` defaults to stereo, 48 kHz, block size 1024, three residue stages,
64 MiB maximum packet length, 128 MiB maximum owned raw encoder elements, and
an input frame limit within the signed Ogg granule range. Both identification
block sizes equal the configured power of two, 64..8192. Precision stages 1..4
select 4/8/12/16-bit scalar quantization. Actual payload length depends on the
first-stage entropy book, partition sparsity and framing/classification costs.
They are precision choices; they provide no target bitrate or perceptual-quality
guarantee. Bitrate hints in the identification header are zero (unknown).

The analysis hop and lookahead are half a block. The first priming packet is
emitted after half a block of input; the first retained overlap block can be
decoded after a full block of input. `finish` pads the remaining analysis tail
and writes EOS at exactly the number of submitted frames per channel. Empty
streams contain one zero-energy priming audio packet and have length zero.
One-frame streams, exact hops and partial final hops retain exactly their input
length. Priming and padding are transport timing, never extra retained samples.

Headers are written during construction, with the identification header alone
on its BOS page and audio beginning on a fresh page. `with_page_options` controls
the Ogg body target. Large packets may span pages. `finish` is explicit and
idempotent; dropping or calling `into_inner` does not finish or flush a stream.
The caller controls any underlying `Write::flush`. Concatenate completed streams
with distinct serials for a chain. Multiplexing uses raw packets and the Ogg
crate's independent per-stream writers.

## Raw packets, reuse and errors

`PacketEncoder::new` constructs all analysis tables, setup data and scratch.
`identification().encode()`, caller-owned `Comments::encode`, and `setup_packet()`
provide the headers. The fallible callback on `push_interleaved`, `push_planar`
and `finish` receives borrowed `EncodedPacket` data. Consume it before returning.
Absolute packet numbering starts at three, and each packet carries its retained
granule position and EOS flag. Raw users control transport and header delivery.

`position()` reports accepted input frames. Shape, sample-range and frame-limit
errors validate the whole supplied chunk before modifying state. Errors during
analysis or output callbacks make the stream terminal until `reset`; a caught
callback panic also leaves it terminal. `reset` begins another raw stream with
the same configuration and retains capacity. The caller must restart transport
and headers for that stream. Ogg writer failures are terminal; construct another
Ogg encoder for a new logical stream.

Normal raw push/finish/reset allocate nothing after construction. A counting
allocator checks repeated dense encoding for mono/N64, stereo/N1024,
eight-channel/N1024 and 255-channel/N64 at four-stage precision. Output callback
allocations are excluded. `storage_bytes()` and `max_memory` count retained raw
encoder table/buffer capacities, excluding struct/allocator overhead, temporary
construction data, caller PCM, comments and Ogg/output storage. Packet capacity
is reserved from the worst-case policy before input is accepted; encoded audio
and setup packets must fit `max_packet_bytes`. This is separate from process
working set or a bound on an arbitrary output writer.

## Analysis and packet construction

The forward transform independently transposes the native synthesis fold,
reuses its symmetric DCT-IV factorization and applies the Vorbis `4/N`
normalization. A length-N/4 complex FFT uses 7N bytes of table/scratch elements.
The standard Vorbis window is applied to every time block. A two-half input
ring avoids moving the preceding half block between analysis calls.
[MDCT.md](MDCT.md) gives the equations and independent checks.

Each channel uses a flat adaptive floor one with two equal endpoints. The
endpoint is the smallest inverse-dB table entry sufficient to keep the normalized
spectral peak below 28. Residue one partitions contain 16 coefficients. A
one-dimensional classification book separates exactly zero partitions from
coded partitions. All books are generated from the wire grammar: complete
Huffman trees and lookup-one two-dimensional radix-16 VQ books. The
classifier and later stages use ordered fixed-length trees; the first stage
uses a compact unordered three-/eight-/nine-bit tree. No external training data
or encoder packet template is embedded. [ENCODING_ENTROPY.md](ENCODING_ENTROPY.md)
records its measured costs and unchanged-symbol/PCM comparisons.

For P stages, `levels = 16^P`. A normalized scalar v is rounded to
`q = round(v * levels / 64 + levels/2)`, bounded to its unsigned quantizer range.
The first additive stage reconstructs `-32 + 4*d0`; subsequent stages contribute
`(4/16^s)*ds`. Stage digits are emitted most significant first. Two scalars share
one eight-bit VQ entry. Zero partitions carry no stages, and wholly zero channels
omit floor and residue. Mapping zero has one submap, no channel coupling and one
short mode with equal identification block sizes. The configuration and every
audio payload are generated natively from PCM.

The normative grammar and mapping are from the
[official Vorbis I specification](https://www.xiph.org/vorbis/doc/Vorbis_I_spec.html),
particularly sections 2, 3, 4 and Appendix A. This encoder selects a valid subset
of the available coding tools; its decoder continues to support the other
normative floors, residues, mappings and window transitions.

## Independent validation and measured source error

The retained [144-case matrix](validation/2026-10-27-encoder-entropy-matrix.tsv) checks
libvorbisfile 1.3.7 decoding of native output: 1/2/3/6/8/16/255 channels, four
precision stages, zero/one/partial/multiple-block lengths, mixed silent channels
and packets larger than one Ogg page. Exact declared and emitted frame counts
are required. All 2,375,544 decoded channel samples meet the existing floor-one
parity bounds: peak error at most `3e-6 * max(1, reference_peak)` and RMS at most
`5e-7 * max(1, reference_peak)`. Observed maximum peak error is 5.067e-7; the
largest per-case RMS is 1.036e-7. No alignment or gain fitting is applied.

Symphonia 0.6.1 independently decodes 128 small cases across all assigned channel
layouts and all stages, plus the large eight-channel spanning case. Its API
returns canonical speaker order, so the adapter compares planes using declared
speaker positions. This encoder uses no coupling. The previously documented
dependent-coupling decoder discrepancy therefore does not affect these files.

Source reconstruction error is measured separately from decoder parity. The
matrix uses deterministic tones, noise and impulses at 48 kHz. For its longer
cases (2N+17 frames), measured source SNR ranges are:

| Stages | Scalar precision | Source SNR range (dB) |
| --- | --- | ---: |
| 1 | 4 bits | 15.25–16.57 |
| 2 | 8 bits | 19.17–35.29 |
| 3 | 12 bits | 41.31–59.36 |
| 4 | 16 bits | 65.38–83.61 |

The current 273-frame stereo/N128 example occupies 611/1088/1500/1913 total Ogg
bytes at these four settings, including all headers and padding. These short
synthetic measurements establish lossy reconstruction and size tradeoffs. They
do not establish perceptual transparency or compression competitiveness. The
flat floor, fixed block, uncoupled channels and uniform fixed-length VQ books
can spend substantially more bits than established perceptual encoders.

The forward transform also matches libvorbis's analysis transform at every
legal size: observed maximum coefficient error 8.196e-8, with unchanged gates
of peak 3e-7 and RMS 5e-8 at unit peak. Direct f64 cosine and unquantized
analysis/window/overlap tests are separate from the reference implementation.
Native exact seeking, invalid input, resource limits, callback/write failures,
reset determinism and allocation checks are in `tests/encoding.rs` and
`tests/allocations.rs`. Decoder regression gates are rerun because synthesis
and analysis now share the FFT butterfly helper.

### Pinned libvorbisfile seek discrepancy

With the default paginator, eight channels, N8192, four stages and 16,401 input
frames, each audio packet has 65,810 bytes. The priming packet spans two pages.
libvorbisfile 1.3.7 `ov_pcm_seek(0)` then reports total length 16,401 but emits
12,305 frames: its PCM is exactly the suffix beginning at frame 4,096. Native
and Symphonia decode all 16,401 frames. libvorbis raw synthesis also returns all
five overlap blocks before final trimming.

`ov_raw_seek(0)` followed by the same `ov_read_float` calls returns the full,
correctly trimmed PCM and declared length. The encoder matrix uses the oracle's
`oggpcmraw` mode for this raw-start path. A named regression checks both modes,
their exact counts and their respective PCM intervals; it does not silently
adjust the normal parity comparison. Existing decoder corpus tests retain their
original `oggpcm` seek-to-PCM-zero mode.

## Reproduce and remaining work

Build the pinned [test oracle](tests/oracle/README.md), then:

```powershell
$env:DEADLIB_LIBVORBIS_ORACLE = "$pwd/target/deadlib-audio-reference/vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe"
$env:DEADLIB_VORBIS_ENCODE_REPORT = "$pwd/target/encode-matrix.tsv"
cargo test -p deadlib-audio-decode-vorbis --release --test encoding --test allocations -- --include-ignored --nocapture
```

The [fuzz workspace](../fuzz/README.md) adds `vorbis_encode`: integer/raw-float
PCM, legal policy combinations, invalid samples, arbitrary planar/interleaved
chunks, reset replay, Ogg pagination and exact decoded counts. Retained source,
input, reference-binary, validation and sanitizer hashes accompany this
milestone's [check manifest](validation/2026-10-27-encode-checks.json).

The initial encoder milestone's release gate passed 106 tests with reference/corpus cases enabled
(only the fixture regeneration utility is excluded). An extracted, offline
workspace passes 84 tests, with 23 optional external/generator cases ignored;
its normal dependency graph is Ogg alone for Vorbis and empty for Ogg. Clippy
passes with default/all features and for every fuzz/seed target.

For that initial milestone, two concurrent Windows AddressSanitizer targets ran with a 300-second budget
each, the pinned floor-zero fixture and recorded RNG seeds:

| Target | Executions | Coverage counters | Features | Peak sanitizer RSS (MiB) |
| --- | ---: | ---: | ---: | ---: |
| `vorbis_encode` | 4,111 | 1,825 | 5,288 | 479 |
| `vorbis_stream` | 4,310 | 3,158 | 9,149 | 444 |

Both exited zero with no artifacts, assertion failures, sanitizer reports,
timeouts or RSS-limit failures. All recorded source and binary hashes were
rechecked. [Results](validation/2026-10-27-encode-fuzz-results.json),
[provenance](validation/2026-10-27-encode-fuzz-provenance.json) and
[initial corpus hashes](validation/2026-10-27-encode-fuzz-initial-corpus.sha256.json)
preserve the run; evolving corpora and binaries remain in ignored scratch
storage. Sanitizer RSS includes harness/allocator overhead, not just encoder
elements. Coverage counters are libFuzzer feedback, not line-coverage percentages.
This bounded campaign supplements the independent parity tests.

Remaining encoder work includes adaptive floor fitting, transient block
selection, channel coupling, entropy/VQ optimization, perceptual masking,
quality and bitrate control, wider real-music rate-distortion evaluation and
comparative throughput/startup/memory measurements against libvorbis encoding.
The first [resident-PCM encoding baseline](ENCODING_PERFORMANCE.md) now checks
42 streams through native, libvorbisfile and Symphonia, including two real song
excerpts. It measures source error, output size, full encoding and warm
initialization/first-packet time. In that baseline, Xiph q0.9 used lower bitrate
and achieved higher source SNR than native stage 2. Packet symbol profiling identifies
a concrete entropy-coding opportunity. The subsequent
[compact-book update](ENCODING_ENTROPY.md) implements it: paired two-stage song
files shrink 26–29% with bit-identical PCM, zero raw warmed allocations and
documented tiny-file header costs. The baseline measurements retain their
original provenance. Perceptual/rate control and broader resource/quality
comparisons remain open.
The broader decoder conformance, sparse seeking and performance work in
[COVERAGE.md](COVERAGE.md) also remains part of the requested goal.
