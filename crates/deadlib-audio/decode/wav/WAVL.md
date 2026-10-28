# Segmented WAVE audio: wavl and slnt

Native PCM and IEEE float now support a LIST/wavl audio section containing
multiple data/slnt chunks. Borrowed `Wave`/`Pcm`, seekable `Reader`, Read-only
`ForwardReader` and `WavlWriter` share this layout. The ordinary contiguous data
path remains available and does not allocate a segment index.
Borrowed Microsoft/3-bit/4-bit IMA sequences also work through `AdpcmWavl`.
File-backed and forward ADPCM convenience wrappers still require flat data.

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
BW64 output now zeros the ds64 dummy words. Its wavl compatibility path uses a
literal 32-bit fact count and rejects larger counts before output; use RF64 for
larger virtual sequences. See [BW64.md](BW64.md) for the corrected distinction.

## Codec-independent segmented output

`WavlMuxer::new(sink, format, options, metadata)` accepts a validated raw fmt
payload and a Write+Seek sink. `with_lengths` provides Write-only output with
the same LIST byte/frame accounting as WavlWriter. `write_data(bytes, frames)`
emits one data child; `write_silence` and `write_chunk` handle held frames and
ancillary children. `frames_written`, `list_bytes_written` and `expected_length`
make the count contract inspectable. Finish is explicit; dropping or extracting
the sink does not finalize or check an incomplete declared sequence.

The format payload is written exactly, including odd size and unrecognized
extension bytes. The muxer borrows it only during construction, retains no format
copy and performs no codec dispatch. force_extensible does not rewrite a supplied
raw descriptor. Native PCM/float byte/frame counts are checked against alignment;
encoded block validity and duration are the producer's responsibility. Per-call
frame counts contribute to the aggregate fact count; WAV does not store a separate
decoded count beside each data child. Producers must choose segment boundaries
and padding that their codec's reader can reconstruct.

The wire layout follows the
[IBM/Microsoft data-list and mandatory-fact definitions, pages 3-26–3-28](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/riffmci.pdf).
Known lengths include the wavl type, every child header/payload/pad and all silent
frames. Native alignment, declared overfill, u64 arithmetic and BW64 fact limits
are checked before the offending record is written. Partial I/O poisons the
writer; counters retain only complete successful records. Individual children
remain below the existing u32 sentinel limit; RF64/BW64 promote the outer LIST
through its ds64 table entry and keep primary dataSize zero. BW64 dummy words
remain zero, with literal fact counts through u32::MAX only.

WavlWriter now handles native packing over this muxer. Its public sample methods
and serialized bytes remain unchanged; expected_length is also available there.
The wrapper's private constructor passes a descriptor already validated by
Spec::header, avoiding a second full parse. Public raw-format constructors always
validate the supplied descriptor. No production unsafe code or dependency is added.

Five tests bring the full suite to 83, passing on x64 debug/release and i686 debug,
with denied-warning Clippy (`validation/wavl-mux-checks*`). They independently
construct RIFF/RIFX/RF64/BW64 bytes for odd opaque descriptors, vendor GUIDs,
metadata/data pads, embedded forms, split records and both finalization paths.
Microsoft/3-bit/4-bit IMA and extensible IMA descriptors retain their encoded bytes.
Tests also check native wrapper equality, u64 fact promotion, a >4 GiB declared
LIST header, literal BW64 limits, reserved IDs, malformed formats, declared counts
and partial-write poisoning. The existing allocator probe now checks both public
muxer constructors with caller storage across all four containers: zero requested
allocations and bytes.

`wavl-mux-native-replay.json` passes all 708 retained native fixture gates, 4484
descriptor/read/range checks and 1912 full encoded-output comparisons under the
previously declared BW64 dummy-byte rule. All 1580 earlier corpus inputs, two
million checked mutations and 100000 fresh ASan executions pass; FUZZING.md
records the expanded properties and preserved corpus. This is retained sample
truth and byte parity, not a new external reader comparison. As documented below,
the reference drivers do not directly support wavl here.

Generic encoded serialization is implemented; borrowed compressed decoding is
described below. Segment-aware cached file seeking and forward ADPCM routing
remain open. The measured
native writer slowdown is retained as an outstanding optimization issue in
PERFORMANCE.md. This milestone does not claim completion of those requirements.

## Native fixture evidence

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

Compressed file-backed/forward ADPCM wrappers still reject segmented data;
the borrowed implementation below now decodes it. Broader extensible compressed
mappings, reference support, comparative segmented
throughput/RSS and application-profile/metadata semantic certification remain open.

## Borrowed compressed sequences

`AdpcmWavl::new(&wave)` supports Microsoft mono/stereo and 3/4-bit IMA, including
the extensible IMA mapping, in RIFF/RIFX/RF64/BW64. Keep the Wave binding alive:
the view borrows its already checked child inventory and original encoded bytes.
Container parsing stays codec independent. The separate flat AdpcmData API and
its Copy behavior, 88-byte x64 state and zero-allocation construction are unchanged.

Each data child restarts a block sequence; complete short last blocks contribute
their actual capacity. Empty data, zero silence and unknown children preserve the
held frame. Initial silence is i16 zero; later silence repeats the final decoded
interleaved frame of the last nonempty data child. A range beginning inside
silence decodes only that child's final block to reconstruct the frame. Binary
search selects the first child for a range, and only requested blocks are decoded.
The view reads natural physical child order. Cue/playlist metadata is preserved;
automatic playlist execution is not implemented for native or compressed audio.

The mandatory global fact count limits the complete natural-order timeline.
Capacity is the sum of every data child's block capacity and every slnt count;
fact above that capacity is rejected. A smaller fact truncates the sequence's
end. There is no serialized per-child decoded count here: padding in an interior
encoded data child remains audible. Producers must choose independently decodable
boundaries and account for padding before subsequent silence/data. Supplying a
smaller decoded count to WavlMuxer does not encode a hidden per-child trim. The
primary specification defines compressed block addressing but does not supply
such a trim field; this API makes its natural-order policy explicit rather than
guessing how to distribute global padding across children.

`read_frames(start, output, scratch)` returns interleaved samples, leaves an
incomplete output frame untouched and requires one reusable full-block scratch.
Construction checks descriptor geometry, all child extents and aggregate counts
without decoding headers. Malformed block predictors/indices fail when requested
audio or a held frame depends on them; output can contain a decoded prefix.
The borrowed view remains reusable for independent ranges after an error.
Tiny files with u64 silence never allocate or iterate over that logical duration.

The added index stores 16 bytes per inventoried child on x64/i686, in one fallible
Vec allocation (small capacities can round up). It borrows the container's larger
inventory. An eight-child allocator probe requests one allocation/128 bytes for
construction and zero allocations/bytes for reads. Caller scratch/output, Wave's
existing inventory and process RSS are outside those counts. PERFORMANCE.md
records object sizes and the resident throughput/seek measurements.

`validation/adpcm-wavl-references-final.*` records 216 independently packed
fixtures, all passing 1296 full/range checks. The generator's scalar Microsoft
and IMA arithmetic does not call production code. Cases cover 1/2/6/33 channels
where applicable, both IMA headers, all four containers, short and repeated
blocks, leading/held/zero silence, empty children, odd metadata, trailing fact,
empty duration and final truncation. Every external driver rejects the direct
segmented fixtures. Ten predeclared RIFF mono/stereo flat block components pass:
Microsoft against FFmpeg, 4-bit IMA against dr_wav/libsndfile, 3-bit IMA against
FFmpeg's alternate combined rounding. These are component checks, not direct
external wavl parity. Every outcome, fixture and truth hash is retained.
The initial matching report predates the benchmark manifest entry; its exact
Cargo snapshot is retained. Both fixture archives are byte-identical.

Four new tests bring the suite to 87, passing x64 debug/release and i686 debug
with denied-warning Clippy. Fixed scalar vectors check every frame position and
buffer split, malformed geometry/lazy headers, repeatable post-error access,
RF64 silence above u32 and BW64's literal fact/dummy-word distinction. The native
replay retains all 708 gates, 4484 descriptor/read/range checks and 1912 complete
writer outputs under the existing BW64 rule. Two million checked mutations,
1581 prior corpus inputs and 100000 ASan executions pass; FUZZING.md retains the
new 1827-file corpus and budgets. Initial check logs retain a corrected Clippy
test-style failure and an i686 incremental-cache missing-object error; final
checks disable incremental compilation and pass, with exact source snapshots.

```sh
python crates/deadlib-audio/decode/wav/tests/reference_adpcm_wavl.py --output target/wav/new-adpcm-wavl.json
cargo run --locked -p deadlib-audio-decode-wav --example adpcm_wavl -- INPUT.wav START FRAMES
```
