# Buffered synchronization for reference seeking

The active target is complete parity with exact behavior/counts and the original
Vorbis PCM tolerances. Full Vorbis seeking needs the reference's actual buffered
transport behavior. Searching only indexed outer pages misses captures inside
packet data, partial capture recovery, cached-only reads and repeated seeks that
preserve read-ahead. This milestone supplies the shared Ogg synchronization
primitive. **The 172 recorded Vorbis seek differences remain open.**

`SyncBuffer` accepts arbitrary chunks and exposes `page_seek()` and `page_out()`.
Page seeking reports exact skipped-byte counts, NeedMore or a complete page.
After a failed capture/checksum it advances to the next possible `O`, matching
libogg's observable skip granularity. It waits for at least 27 bytes before
examining a capture and waits for the complete declared lacing/body before CRC
validation. Pageout reports one Hole per loss of synchronization, suppressing
further hole notifications until it returns a page. Reset discards buffered data
and hole state while retaining allocations.

`SyncedPage` contains complete CRC-verified wire bytes and preserves unknown
versions and reserved flags, matching transport synchronization. Its
`validated()` conversion checks published version/flag rules before producing
the existing `Page` type. `Page::parse`, `PageReader`, `PageDecoder`, `Demuxer`
and `PacketCapture` keep their existing validation policies. Packet and stream
grammar remain separate from synchronization.

`SyncReader` combines this buffer with Read/Seek. Its default 2,048-byte reads
match pinned libvorbisfile; other read sizes are configurable. Cached-only
queries never request more bytes. A positive byte boundary limits a page's
starting position; the returned page may extend past it. EOF with an incomplete
capture is distinct from reaching a search boundary. Repeated EOF queries still
call Read, matching the reference. IO errors retain unread data and the logical position reached before the
failed callback.

The source initially starts at byte zero. `seek(offset)` preserves cached bytes
and avoids an underlying seek when the logical position already equals offset.
Read-ahead can leave the underlying reader further forward. This is the
reference seek-helper contract, including cached partial captures at EOF.
`get_ref()` and `into_inner()` expose the source; `position()` reports the logical
consumed-byte position.

Buffer limits are explicit resource policies. SyncBuffer defaults to 1 MiB of
unread bytes; SyncReader defaults to one maximum Ogg page plus 4,096 bytes.
The read scratch is separately included in `storage_bytes()`. Allocation growth
is bounded by the configured buffer policy. Rejected appends preserve bytes,
position, hole state and the last returned page. Byte-position/search-boundary
overflow returns a resource error. These limits do not restrict Ogg packet size.

Independent C comparisons use pinned libogg 1.3.6 with its unmodified
`ogg_sync_pageseek` and `ogg_sync_pageout`. A test-only memory-source driver
follows pinned libvorbisfile's bounded read/seek contract without linking any
codec. Every status, unread count, page byte, logical/source position and
read/seek callback count is compared exactly, including injected failures.

The matrix contains 528 mixed buffer schedules with 95,298 operations, 91 reader
schedules with 8,554 operations, and 260 raw-header/lacing batches with 138,500
operations: **242,352 operations across 879 cases/batches**. It covers all 65,536
version/flag byte pairs and all 256 lacing-table lengths with four lace values
(1,024 layouts), including maximum pages, empty packets/pages, partial headers,
CRC failure, false captures, nested pages, mixed polling, read-ahead, boundary
fences, repeated seeks and cached EOF. Four offline golden batches retain exact
requests and C responses. No floating-point tolerance is involved in transport.

The new `ogg_sync` fuzz target varies chunking, buffer/pageout state, reader
boundaries, seeks, caps and reset. Returned bytes pass a separate bitwise CRC
check and explicit version/flag conversion checks. Paired contexts check exact
results, positions, unread bytes and ordered page hashes. The fuzz hash is a
practical deterministic oracle; normal C comparisons check complete bytes.

Validation passes all 42 Ogg release/reference tests, 108 normal Vorbis
regression tests, 142 isolated offline tests and four fuzz harness tests. The
offline run leaves 87 external/manual tests ignored; the Ogg reference run
executes its external tests and skips only manual golden regeneration. Both
codec Clippy configurations and all seven fuzz targets pass with warnings
denied. Two 300-second ASAN campaigns complete 50,016 executions (7,961 sync,
42,055 container), with no artifacts or slow-input failures. All source/fixture
hashes match the compiled snapshots. A compiled reference audit also verifies
47 old helper responses byte for byte and proves every preexisting Ogg/Vorbis
production method unchanged after removing only new exports and comments.

The [validation record](validation/2026-10-28-buffered-sync-checks.json) retains
complete protocol receipts, commands, source/corpus/reference hashes, seeds,
golden recaptures, old helper responses and sanitizer results. The four offline
golden batches need no reference installation.

Remaining reference seek integration must also reproduce queued packet
peek/pop behavior, hole markers, initial continuation discard and EOS tagging
before combining transport with SynthesisDecoder, page bisection, raw-seek
fallback, track-only preroll and file PCM cursors. The existing validated
PacketCapture intentionally reports framing errors that the reference packet
queue can retain as holes. Its strict behavior must remain available.
