# deadlib-audio-decode-ogg

Independent, codec-neutral Ogg container implemented in Rust from RFC 3533
and Xiph's framing/mapping requirements. A runtime-guarded x86-64 CRC module
uses audited SIMD intrinsics; other code and the portable fallback remain safe
Rust. No production dependencies. This crate
is a workspace member and is not integrated into DeadSync's decoding pipeline.

The [container performance report](PERFORMANCE.md) compares page parsing, packet
assembly, fragmented reads, first-packet latency and isolated CRC with pinned
libogg. Exact page fields and complete packet bytes are checked before timing.
The measured eight-byte CRC update improves song packet demux 4.12x / 3.89x
against its paired native baseline, with 7 KiB additional shared table elements.
Native medians are close to libogg on those inputs; broader coverage remains.

```rust
use deadlib_audio_decode_ogg::{Demuxer, PageReader};
# fn example(bytes: &[u8]) -> Result<(), deadlib_audio_decode_ogg::Error> {
let mut pages = PageReader::new(bytes);
let mut streams = Demuxer::default();
while let Some(located) = pages.next_page()? {
    streams.push_page(located.page, located.offset, |packet| {
        // Consume packet.data here, or copy it for longer-lived storage.
        // packet.info.granule_position belongs only to the last packet
        // completed on a page; the codec defines what that value means.
    })?;
}
streams.finish()?; // Require complete logical streams, including EOS.
# Ok(())
# }
```

* `Page::parse` borrows and CRC-verifies one page, leaving trailing bytes to the
  caller. `fragments` exposes complete packets and continuation fragments.
* `PageReader` handles fragmented/Interrupted IO. `SyncMode::Recover` scans to a
  CRC-verified capture, including captures inside corrupt/truncated candidates.
  It reports discarded bytes. Strict parsing never silently recovers.
* `SyncBuffer` exposes libogg-compatible buffered page seeking, exact discarded
  byte counts and pageout hole notifications. `SyncReader` adds cached-only and
  bounded searches, read-ahead and repeated seek behavior. CRC-verified
  `SyncedPage` preserves raw versions/flags; `validated()` applies the strict
  page rules before use with the normal container APIs. See
  [buffered synchronization](BUFFERED_SYNC.md).
* `PageDecoder::push` is the incremental equivalent: it consumes at most one
  page, returns an exact consumed count and lends the page until the next call.
  `finish` checks transport truncation; use `Demuxer::finish` for stream closure.
* `Demuxer` validates stream sequencing, continuation, unique serials, BOS/EOS,
  initial header isolation, granules, group ordering and chaining. In-page
  packets borrow the page without allocation/copying. Only spanning packets
  require assembly storage. `Limits` imposes configurable resource policies,
  not format limits. Zero-length packets and nil EOS pages are supported.
* `ValidationOptions` enables two named legacy exceptions through
  `Demuxer::with_validation`, `PacketCapture::with_validation` (or
  `at_page_with_validation`), and `IndexOptions::validation`. Both default to
  false. `compatibility()` counts accepted exceptions; ordered seek landmarks
  omit ignored span granules and decreasing EOS granules. Raw page headers are
  preserved. A codec must supply any resulting sample-trimming policy; see
  [the Vorbis policies](../vorbis/LEGACY.md).
* `PacketWriter` paginates one logical stream, isolates the initial header and
  writes incrementally to `Write`. `finish` emits EOS, including a nil page when
  necessary. `write_page` is a lower-level payload-copy-free page writer.
* `OggIndex::read` validates a complete physical stream and builds flat page
  ranges for each serial/chain. `granule_at_or_before` finds the last known
  granule in logarithmic time; `packet_start_page` finds a packet's starting
  page even across long continuations. `IndexOptions` bounds the page count.
  `read_with_events` also lends each assembled packet and then its completing
  page to a fallible codec callback in physical order. Nil pages produce page
  events. Observations remain provisional until the full index succeeds; the
  callback can abort with its own error type. Vorbis uses this shared pass for
  header and sample-timing indexing without rereading or assembling packets again.
* `find_granule` performs index-free bisection with CRC capture recovery inside
  an explicit byte range, using one reusable page buffer. Reads stay inside the
  range and positions use integer u64 arithmetic. This search assumes
  nondecreasing known granules and does not validate the whole physical file.
* `PacketCapture` selects one serial after a seek, discards an unavailable
  initial continuation, then validates packet assembly and sequencing strictly.
  Indexed capture knows absolute packet numbers; unindexed capture reports
  unknown numbers unless it began at BOS. `reset_at` supports repeated seeks.
  `push_page_until` can pause after a complete packet via `ControlFlow::Break`;
  a false return means the identical page/offset must be resumed. A later
  `push_page` can drain its remainder. Structural/size checks cover the whole
  page before its first packet; completion and compatibility counts commit once
  at its end. `PageReader::current_page` reborrows the last verified page without
  IO, CRC work or a payload copy. Reset discards a paused cursor. Completed
  captures retain at most one page of assembly capacity for repeated seeks.

```rust
use std::io::Cursor;
use deadlib_audio_decode_ogg::{IndexOptions, Limits, OggIndex, PacketCapture, PageReader};
# fn seek_example(bytes: &[u8]) -> Result<(), deadlib_audio_decode_ogg::Error> {
let index = OggIndex::read(bytes, IndexOptions::default())?;
let stream = &index.streams()[0];
if let Some(page) = index.packet_start_page(stream.serial, 3) {
    let mut reader = PageReader::new(Cursor::new(bytes));
    reader.seek(page.offset)?;
    let mut capture = PacketCapture::at_page(page, Limits::default());
    while let Some(located) = reader.next_page()? {
        capture.push_page(located.page, located.offset, |packet| {
            // A starting page can contain packets before the requested number.
            if packet.info.number.is_some_and(|number| number >= 3) {
                std::hint::black_box(packet.data);
            }
        })?;
        if capture.is_ended() { break; }
    }
    capture.finish()?;
}
# Ok(())
# }
```

Use an index with the exact immutable byte source from which it was built.
Capture validates only its selected logical stream, including strict sequence
and continuation after the initial discarded fragment. It does not silently
conceal subsequent holes or validate the other interleaved serials. A codec
chooses an earlier packet for pre-roll and discards samples to its seek target;
a container granule landmark alone does not establish sample-accurate seeking.

Granules are uninterpreted `u64` bit patterns; `UNKNOWN_GRANULE_POSITION` is the
all-ones sentinel. This crate supplies no sample rate, PCM, codec metadata,
timestamp conversion or inter-stream timing policy. A codec mapping supplies
those. Muxers interleave whole pages generated by separate logical writers.

Errors in stateful parsing/assembly/IO are terminal until reset. Packet-writer
argument errors detected before mutation leave it usable. Recovery of transport
pages does not imply recovery of damaged logical packets; the strict demuxer
still reports holes. See [COVERAGE.md](COVERAGE.md) for the current limits.

Build/test by package name (`cargo test -p deadlib-audio-decode-ogg`) or manifest
path. The Rust `ogg` development dependency and the external
[libogg oracle](tests/oracle/README.md) are used only for differential tests.
Coverage-guided sanitizer targets live in the separate
[fuzz workspace](../fuzz/README.md), including checksum-aware container mutation.
