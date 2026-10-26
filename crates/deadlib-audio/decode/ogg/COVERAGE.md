# Ogg coverage and evidence

Status: **in development; full requested conformance/performance not claimed**.
Sources are linked in the parent directory's work plan.

## Implemented and tested (2026-10-26)

* Version-zero page reading/writing, little-endian fields, reserved-flag/version
  rejection, lacing, CRC, maximum page size, zero-size packets, payload borrowing.
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

Evidence on Windows x86-64, rustc 1.99.1:

* `cargo test -p deadlib-audio-decode-ogg`: 12 container tests, including seeded
  500-packet differential corpus, header/payload bit corruption, every truncated
  page prefix and one-byte push input. Two external tests explicitly ignored.
* Native writer -> Rust ogg 0.9.2 reader, and Rust ogg writer -> native reader.
* External libogg 1.3.6 at commit
  `be05b13e98b048f0b5a0f5fa8ce514d56db5f822`: both writer directions verified.
  All **38 repository Ogg assets, 8,733 pages and 119,144 packets** matched
  libogg's page fields, packet bytes, BOS/EOS and packet granules exactly.
  Command: `cargo test -p deadlib-audio-decode-ogg --release --test libogg --
  --ignored --nocapture` with the oracle/corpus variables described in its README.
* `cargo tree -p deadlib-audio-decode-ogg --edges normal`: only this package.
* Parent `scripts/verify_independent.ps1`: copied only Ogg and Vorbis into a
  separate workspace and passed offline package tests and dependency trees.

## Outstanding work

* Indexed and indexless granule seeking, packet capture/pre-roll after seeks,
  repeated seeks and index validation. `PageReader::seek` currently resets only
  the transport cursor, not logical-stream or codec state.
* A packet-level recovery/capture API distinct from strict physical validation.
* Additional independent generated multiplexed/chained and malformed-stream
  differential corpora, official conformance material, sustained fuzzing and
  actual ITGmania/StepMania song-pack validation (assets are not song packs).
* Reproducible comparative throughput/startup/allocation/memory/seek benchmarks,
  and subsequent measured optimization. No competitive performance claim yet.
* Broader platforms and benchmark targets.

Transport validation does not validate codec headers or mapping-specific packet
ordering. Metadata, channel layout, sample counts and exact seeking belong to
the codec. No legacy compatibility exceptions have been introduced.
