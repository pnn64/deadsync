# Segmented WAVE audio: wavl and slnt

Native PCM and IEEE float now support a LIST/wavl audio section containing
multiple data/slnt chunks. Borrowed `Wave`/`Pcm`, seekable `Reader`, Read-only
`ForwardReader` and `WavlWriter` share this layout. The ordinary contiguous data
path remains available and does not allocate a segment index.

The [IBM/Microsoft WAVE specification, sections 3-26–3-28](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/riffmci.pdf)
defines wavl, silence, its mandatory fact count and compressed-data cue addressing.
Silence holds the last complete interleaved frame, preserving each channel's
value. Leading silence uses digital zero: 128 for unsigned PCM8, zero bytes for
signed integers and positive IEEE zero. Seeking into silence reconstructs the
preceding frame, so it agrees with reading continuously from the beginning.
Float NaN payloads, signed zero and unused integer bits are repeated as bytes.

Every child header, payload and odd pad is bounded by the LIST payload, independently
of the enclosing form. Unknown children remain inspectable and do not change the
held frame. Empty data and zero-count silence do not reset it. Native data chunks
must contain whole frames; slnt has exactly one u32 frame count. fact is required
and must equal the native sequence's actual logical frame count. A trailing fact
works with borrowed/seekable input and is checked when forward input finishes.
Multiple top-level audio sections remain rejected as ambiguous; multiple data
chunks within the single wavl audio section are supported.

## Access and memory

`Wave::audio_chunk()` and `Reader::audio_chunk()` identify either the data chunk
or the LIST/wavl chunk. `Wave::wavl_chunks()` exposes child payloads, unknown IDs
and original padding; Reader's version exposes descriptors. `Reader::read_metadata` also accepts
its retained child descriptors and preserves audio position. `Wave::as_bytes()`
still returns the original complete form without rewriting anything.

Several accessors now explicitly represent the absence of a contiguous sample
slice: `Wave::data()`/`data_offset()`, `Reader::data_chunk()`,
`ForwardReader::data_chunk()` and `Pcm::as_bytes()` return Option. They return Some
for the previous flat layout and None for wavl. Direct typed views likewise return
None for a segmented view. A single data child's bytes can still be passed to
`Pcm::new` for compatible zero-copy access. `Wave::pcm()` now borrows its Wave
owner's segment index, so keep the Wave binding alive while using the view.
`Pcm::samples()` returns u128, preserving the exact product of u64 frames and
channel count even for very long virtual silence.

`Pcm::read_frames(start, output)` copies native whole frames into caller storage;
the existing typed methods and `frame_range` also work across segments. The index
stores offsets/counts, never decoded audio. Borrowed and seekable views find a
segment with binary search. Reader adds one held-frame buffer, whose size depends
on block alignment, and reads the preceding physical frame only when necessary.
Seek failures poison reads until a successful seek, including held-frame loads.

Forward input keeps one frame and no segment inventory. `read_frames_with_metadata`
visits ancillary children encountered between audio segments. Leading/trailing
metadata callbacks retain their full payload, including LIST type bytes inspected
internally. Typed reads use the same native frame source. `position()` counts
logical output frames; `data_bytes_read()` counts physical data payload bytes,
excluding virtual silence. `frames()` uses an encountered leading fact (resolving
its RF64 sentinel through ds64) and otherwise becomes available at the wavl
boundary. An unselected raw ds64 count is not an upfront duration. Call finish to
check the required fact and remaining form; finish skips unread silence without
materializing it. Structural/I/O/callback failures poison forward reads.

## Writing

`WavlWriter::new` incrementally emits data, slnt and ancillary child chunks to a
Write+Seek sink, patching headers on finish. `write_frames` and the native typed
methods emit one complete data chunk per call. `write_integers` explicitly packs
integer containers, preserving their container scale. Endian conversion uses
bounded stack scratch even when a channel frame is larger than that scratch.
`write_chunk` preserves ancillary payloads. All odd pads are canonical zero.

`write_silence(u64)` splits large counts into u32 slnt records without allocating
sample data; zero emits a zero-count record. Counts and size limits are checked
before I/O, and a failed write poisons the writer. Dropping/extracting the writer
does not finalize its headers. A data/metadata child must fit its ordinary 32-bit
length field; stream larger audio using multiple bounded data writes.

`with_lengths` also accepts a Write-only sink. `DataLength.bytes` includes the four
wavl type bytes and every child header/payload/pad; `frames` includes silence.
Finalization requires exact declared counts. Known lengths choose RF64 up front
when necessary; unknown-length RIFF output reserves a 40-byte JUNK payload for
promotion. RF64/BW64 mechanics use a ds64 LIST size-table entry and a zero primary
dataSize because the audio is in a LIST. Children retain ordinary 32-bit lengths;
ds64 replacements apply to top-level chunks in this implementation. These
mechanics do not certify a broadcast/ADM application profile.

## Evidence and limits

`validation/wavl-references.{json,build.log,corpus.zip}` records 248 independent
fixtures: 240 canonical padding profiles and eight explicit nonzero-pad
compatibility cases. They cover all six native formats, RIFF/RIFX/RF64/BW64,
1/2/6/33 channels, masks and reduced valid bits, leading/held/zero silence, empty
data, odd unknown children, empty audio and leading/trailing fact. The independent
generator packs both the segmented form and a logically equivalent flat waveform
from integer sample/IEEE words. It does not call a production writer or decoder.

All 1736 applicable descriptor/complete-read/range checks and 992 writer cases
pass. Both wavl writer finalization modes preserve child IDs/payloads and emit
identical output; both ordinary writer modes flatten to independently expected
native bytes and also agree. Encoded physical audio is checked directly in Python
before relying on a decoder. Original and generated fixtures are archived with
hashes, source/binary fingerprints, compiler versions and every reference outcome.

None of the four pinned reference drivers decodes this wavl corpus directly.
dr_wav exits during initialization; libsndfile reports missing/unsupported data;
Symphonia errors (with its previously observed 33-channel debug panic in six cases);
FFmpeg returns unsupported/missing-data errors. Direct failures are retained.
The 24 predeclared nonempty full-precision mono/stereo RIFF cases do match all four
readers when independently flattened and when flattened by this crate. Wider flat
reference limitations remain visible; this is not direct external wavl parity.

Nine integration tests add fixed raw truth, every truncation prefix, malformed
counts/boundaries/pads, shared chunk budgets, metadata between reads, I/O failure
recovery, typed frames larger than stack scratch and RF64 promotion. Real u64
addressing is exercised with a virtual physical extent above 4 GiB, including a
held-frame load above that boundary, on both x64 and i686. Separate tiny RF64/BW64
fixtures seek within over eight billion logical silence frames. All 69 crate tests
pass on x64 debug/release and i686 debug, and Clippy with denied warnings passes.
The earlier 204 Microsoft/IMA real-file/range cases retain their decoded hashes.

Two million checked mutations and 100000 fresh ASan executions pass with the new
segmented reader/writer properties, as do 2507 unique retained earlier corpus
inputs. FUZZING.md gives campaign bounds, hashes and archives. Ordinary flat PCM
reference/benchmark regression evidence is recorded in PCM.md and PERFORMANCE.md.

Reproduce from the workspace root, choosing fresh report stems:

```sh
cargo test --locked -p deadlib-audio-decode-wav --tests --target-dir target/wav
python crates/deadlib-audio/decode/wav/tests/reference_wavl.py --output target/wav/new-wavl.json
```

Compressed wavl audio is retained as container chunks but is not yet decoded by
the ADPCM convenience APIs; those reject segmented encoded data explicitly.
Extensible compressed mappings, broader reference support, comparative segmented
throughput/RSS and application-profile/metadata semantic certification remain open.
