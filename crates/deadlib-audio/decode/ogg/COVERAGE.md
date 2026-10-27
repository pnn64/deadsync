# Ogg coverage and evidence

Status: **in development; full requested conformance/performance not claimed**.
Sources are linked in the parent directory's work plan.

## Implemented and tested (2026-10-26)

* Version-zero page reading/writing, little-endian fields, reserved-flag/version
  rejection, lacing, CRC, maximum page size, zero-size packets, payload borrowing.
  The eight-byte CRC update is independently checked against the bitwise
  polynomial across table powers, seeded/aligned/tail/split inputs and reference
  page/packet bytes; [paired measurements](PERFORMANCE.md) record its speed and
  additional 7 KiB of shared read-only table elements.
* Logical stream packet assembly across arbitrary page boundaries, including
  a 255-byte packet's zero terminator on its following continuation page.
  Only the last completed packet gets the page granule even when another packet
  starts but does not finish on the same page.
* Strict physical validation: isolated complete BOS headers, sequence tracking
  with wrapping arithmetic, consistent continuation, unfinished EOS rejection,
  unique serials across groups/chains, grouped BOS ordering, stream closure,
  nondecreasing known granules, nil EOS pages.
* Push parsing with bounded page buffering and exact consumption; pull IO with
  partial/Interrupted reads/writes; strict/recovering capture; terminal error and
  explicit reset behavior; packet/stream/pending-byte policy limits.
* Streaming packet pagination with initial-header isolation, exact lacing for
  empty/255-byte-multiple packets, multi-page packets, granules and EOS.
* Validated complete-file page index with per-stream flat ranges, physical
  offsets, absolute packet counts and packet-start pages; logarithmic numeric
  granule predecessor lookup across unknown-granule packet spans. Index page
  count policy and storage reporting are available.
  Fallible borrowed packet/page events share that validated pass with codec
  index construction. Packet summaries match complete demux across spans,
  multiplexed chains and nil EOS; callback/CRC failures return no partial index.
* Index-free numeric granule bisection, skipping competing serials and unknown
  granules; explicit byte range fencing and exact u64 positions above 2^54.
  Capture at arbitrary page boundaries with initial continuation discard,
  indexed absolute/unknown unindexed packet numbers, terminal errors and
  repeated resets. Empty continued pages, zero terminators and nil EOS work.
* Recovery scans buffered chunks, retaining split capture prefixes and searching
  failed CRC candidates before reading beyond them. Fragmented one-byte input,
  multiple captures per chunk, EOF prefixes and returning available pages before
  a subsequent blocking/failing read are tested. An 8 KiB noise fixture takes
  fewer than 15 underlying reads with a source permitting 4 KiB reads.
* Explicit, independently selectable compatibility flags for zero granules on
  unfinished nonempty pages and decreasing EOS granules. Strict defaults reject
  both. Three tests cover exception counts, reset, sorted seek landmarks and
  continued rejection of nonzero span granules, non-EOS decreases, CRC failures,
  sequence holes and packet-limit violations. Compatibility changes neither
  page bytes nor unrelated validation. Codec endpoint handling is separate.

Evidence on Windows x86-64, rustc 1.99.1:

* `cargo test -p deadlib-audio-decode-ogg`: 13 container tests, including seeded
  500-packet differential corpus, header/payload bit corruption, every truncated
  page prefix and one-byte push input; seven seeking/capture tests. Four external
  tests explicitly ignored unless their executable/corpus inputs are supplied.
* Native writer -> Rust ogg 0.9.2 reader, and Rust ogg writer -> native reader.
* External libogg 1.3.6 at commit
  `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`: both writer directions verified.
  All **38 repository Ogg assets, 8,733 pages and 119,144 packets** matched
  libogg's page fields, packet bytes, BOS/EOS and packet granules exactly.
  Command: `cargo test -p deadlib-audio-decode-ogg --release --test libogg --
  --ignored --nocapture` with the oracle/corpus variables described in its README.
* libogg capture parity at every generated page boundary, plus **114 indexed
  captures across all 38 assets/8,733 indexed pages**: **181,245 returned packets**
  match bytes, flags and granules. Every indexed packet start matches complete
  demux positions; index-free searches match indexed/linear landmarks on generated
  multiplexed chains and those corpus targets. Repeated seeks cover continued
  and nil EOS pages. This validates container capture, not codec sample pre-roll.
* `cargo tree -p deadlib-audio-decode-ogg --edges normal`: only this package.
* Parent `scripts/verify_independent.ps1`: copied only Ogg and Vorbis into a
  separate workspace and passed offline package tests and dependency trees.
* Coverage-guided Windows AddressSanitizer campaign: 300 seconds per target,
  **48,516 container executions**, no crash/assertion/timeout/RSS-limit failure.
  Pull/push agreement, strict/explicit compatibility, CRC-aware mutation,
  recovering transport and accepted-index capture are exercised. See the
  [fuzz workspace](../fuzz/README.md) and its pinned campaign evidence. This is
  an initial bounded campaign, not exhaustive validation.

## Outstanding work

* Codec-specific pre-roll/sample-accurate seeking remains in the codec crate.
  `PageReader::seek` resets transport only; pair it with PacketCapture reset and
  an independent codec reset. Index-free CRC capture cannot prove full physical
  validity or exclude a deliberately embedded valid page inside a packet payload;
  use the fully validated index for trusted page boundaries.
* Additional independent generated multiplexed/chained and malformed-stream
  differential corpora, official conformance material, longer fuzz campaigns and
  wider ITGmania/StepMania song-pack validation beyond two pinned shipped songs
  covered by the Vorbis tests (theme assets are not song packs).
* The [initial container benchmark](PERFORMANCE.md) now measures throughput,
  first-packet startup and CRC against libogg with exact wire verification.
  Ogg-only allocation/memory/index-seek measurements and subsequent optimization
  remain; no competitive performance claim yet.
* Broader platforms and benchmark targets.

Transport validation does not validate codec headers or mapping-specific packet
ordering. Metadata, channel layout, sample counts and exact seeking belong to
the codec. Named legacy exceptions and their evidence are documented in
[Vorbis LEGACY.md](../vorbis/LEGACY.md); they are disabled by default.
