# Native file-backed seeking

`SeekReader<R: Read + Seek>` builds on the native streaming reader and preserves
its frame, PCM and predictor buffers across seeks. Construction reads metadata
only from the source's current position, so embedded FLAC streams at nonzero
file offsets are supported. Wrap files in `BufReader` for efficient metadata I/O.
The reader owns its source; external changes to those bytes invalidate cached
positions. `into_inner` returns the source, whose physical cursor may be ahead
of the next logical frame because of buffered reads.

`seek(sample)` makes the next returned PCM begin at that exact sample, including
targets inside a block and EOF. `position()` reflects the pending sought sample
until that frame is returned. Seeking zero restarts sequential MD5 verification;
nonzero seeks report partial integrity and still check header/frame CRCs,
integer reconstruction, numbering, block sizes and known sample totals. Unknown
totals require reading through the requested position or EOF. Invalid positions
beyond a known total leave the previous state intact; other seek/read errors are
terminal until an explicit successful seek restores a checked starting point.

The first nonzero seek validates the first frame to establish the actual blocking
strategy and nominal fixed block size. A lazy, sorted frame index retains byte
offsets, coded numbers and sample positions. A seek starts at the nearest cached
frame or a later SEEKTABLE hint, then decodes only through its target. The target
frame is retained as pending PCM instead of decoded twice.

SEEKTABLE points are untrusted. Each used hint must have a valid offset and sample
alignment and point to a complete CRC-validated frame with the expected coded
number, blocking strategy and block size. Incorrect targets are discarded and
the reader falls back to the earlier checked frame; no timestamp is inferred
from an unchecked table claim. Allocation, I/O and resource-policy failures are
reported. A successful hint does not validate skipped audio or its MD5; a full
sequential pass is required for whole-stream integrity.

`SeekOptions` embeds the normal decode limits, permits at most 4,194,304 cached
frames by default, and allows table use to be disabled. `seek_stats()` exposes
successful and rejected hints for diagnostics. `index_len()` reports retained
frame entries. Metadata, seek hints and frame-index memory scale with the input;
the complete file and complete PCM are never retained. Repeated seeks reuse
buffer capacity; measured allocation and throughput results remain future work.
The borrowed-input `Decoder` remains useful for existing slices and uses its
original lazy sequential index rather than SEEKTABLE hints.

## Evidence

Four local tests exercise fixed/variable numbering, every sample position
backwards and forwards, 17-byte source reads, seek tables and no tables, nonzero
file origins, unknown totals, empty input, EOF, MD5 reset, index limits and error
recovery. Corrupted hints include offset overflow, an offset to the wrong valid
frame, a non-frame boundary, zero block size and a wrong block size. All return
the exact expected PCM through fallback. A 50,000-sample case verifies that a
working table reads less than one quarter of the source for a near-EOF seek;
disabling table use reads through the entire source.

The official conformance test now also opens each of the 73 native files through
`std::fs::File`, checks the same 21 positions per file as the borrowed-input
decoder, and compares exact rate/depth/channels/PCM against libFLAC. All 1,533
file-backed positions pass, including variable block lengths, property changes,
maximum blocks, old numbering and metadata extremes. See
[validation/native-seeking.tsv](validation/native-seeking.tsv) for per-file
seek counts and table diagnostics. Reproduce with the conformance command and
`DEADLIB_FLAC_SEEK_REPORT` variable in [COVERAGE.md](COVERAGE.md).
