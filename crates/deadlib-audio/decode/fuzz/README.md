# Native codec fuzzing

This separate, unpublished Cargo workspace contains test-only libFuzzer targets.
It adds no dependencies to either production crate and does not register a codec
with DeadSync. `Cargo.lock` pins the fuzz tooling dependencies.

| Target | Input and invariants |
| --- | --- |
| `ogg_container` | One control byte, then Ogg bytes. Strict/explicit compatibility flags and chunk size vary. Pull and incremental parsing must return identical packet summaries, errors and exception counts. Recovery exposes only CRC-valid pages and terminates. Accepted index-event packet summaries match full demux; full-page and packet-paused capture at first/last pages also agree, including exception counts. |
| `vorbis_headers` | Selector byte: identification, comments, or 30-byte identification followed by setup. Accepted identification/comments round-trip exactly; owned metadata preserves binary fields. Accepted setups construct a decoder under separate memory limits and consume probe packets. Every bounded input also exercises mixed bit-reader lookaheads/reads against direct wire-bit indexing, including invalid widths, exact positions/remaining bits and sticky end-of-packet state. |
| `vorbis_packets` | Fixture selector byte, then repeated little-endian u16 lengths and packet bytes (a final oversized length consumes the remainder). At most 32 packets. Two reset/replays must agree on PCM bits, errors and positions. A known-good replay after malformed input must match its original result. The high control bit tests positions near u64 overflow. |
| `vorbis_stream` | Control byte then Ogg bytes, with arbitrary input chunks. Pull and two reset/push replays must match PCM/event summaries, errors and exception counts. Accepted indices compare page, packet, and mixed reads after end/middle/start/last-frame seeks, including deterministic payload errors and recovery by seeking. |
| `vorbis_encode` | Six policy bytes then little-endian i16 PCM, or raw f32 for range/NaN rejection. Vary legal blocks, 1..255 channels, four precision stages, rates, input chunks and page targets. Whole interleaved and reset/chunked planar encoding must produce identical bytes. Native decoding checks framing, finite PCM, exact positions and final trimming. Invalid PCM must emit nothing and preserve position. |

PCM must be finite under the default synthesis policy. Explicit libvorbis IEEE
propagation accepts NaN/infinity and canonicalizes NaN payload bits in the hash;
finite samples and infinity signs retain their exact bits. Summaries use an order-sensitive
64-bit hash plus exact counts rather than retaining untrusted output; a matching
hash is a practical fuzz oracle, not a mathematical proof of equality. Separate
normal/reference tests compare complete PCM values and exact positions.

The two Ogg targets use a custom mutator that repairs checksums for three
quarters of mutations. It changes no other page field and leaves malformed
headers/lacing, sequence holes and invalid granules available to the validator.
The other quarter retains ordinary mutations for CRC rejection/recovery paths.
This follows the Rust Fuzz Book's
[structure-aware mutation approach](https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html).

## Run on Windows

Install a nightly toolchain, MSVC x64 tools and C++ AddressSanitizer as described
in the [Windows setup guide](https://rust-fuzz.github.io/book/cargo-fuzz/windows/setup.html).
The runner locates MSVC with `vswhere` (or accepts `-MsvcBin`). It does not change
the default Rust toolchain. From the repository root:

```powershell
cargo install cargo-fuzz --locked --root target/deadlib-audio-reference/fuzz-tools
crates/deadlib-audio/decode/scripts/fuzz.ps1 -Seconds 300
```

The script starts one hidden process per selected target, concurrently. Each
gets its own corpus, artifact directory, log and RNG seed. `-Targets` selects a
subset; `-RunRoot` must name a new directory. Runs preserve build logs, initial
seed hashes, source/binary hashes, Rust/tool versions, command arguments, final
statistics and exit codes. Artifacts, evolving corpora and binaries stay under
ignored `target/`, not in the crate. A failed target makes the runner fail after
retaining its artifact and all other results.

Use `-ResumeCorpus path/to/earlier-run/corpus` to copy that run's retained
coverage inputs into a fresh campaign. Imported seeds are named and recorded by
SHA-256; the earlier run is preserved. Use the same optional floor-zero fixture.

For the downloaded, hash-pinned historical floor-zero file:

```powershell
python crates/deadlib-audio/decode/scripts/fetch_corpus.py --group floor0
crates/deadlib-audio/decode/scripts/fuzz.ps1 -Seconds 300 -FloorZero target/deadlib-audio-reference/corpus/floor0/vorbis_floor_type_0.ogg
```

Four small repository fixtures seed stereo, mono, dependent eight-channel
coupling and eight-channel silence. Generated chains, multiplexing, a 70,000-byte
packet, empty packets and nil EOS seed container states. The optional floor-zero
file supplies its original headers, early container bytes and complete raw audio
packets without redistributing media. `-ExtraSeeds` adds header/container seeds
from other single-stream Vorbis files. Raw fixture selection depends on whether
the optional floor-zero fixture is present: preserve the run's environment when
replaying. The manifest records its exact external source hash.

Header and streaming inputs also exercise both `Residue2Layout` policies. The
header target's high control bit selects the libvorbis layout when `kind % 3`
selects setup parsing. Bits six and five enable empty books and IEEE propagation
respectively (controls 2 / 224 exercise strict / all three compatibility options).
Streaming control bits three, four and five select layout, empty books and IEEE
propagation for pull, push, reset and indexed seeking. The raw target uses its
normative fixture contexts. Procedural empty-book and unaligned
geometry streams seed both policies with strict framing and independent C sample
counts; these are bounded-state oracles, separately from C PCM parity tests.

The input cap is 128 KiB plus one control byte. Setup/decode element budgets are
8 MiB each; container pending bytes are 256 KiB, logical streams 16, active Vorbis
streams four, and aggregate codec elements 16 MiB. Indexing/streaming exercises
at most 128 pages; raw synthesis at most 32 packets per replay. The fuzzer has a
20-second per-input timeout and 2 GiB RSS limit. These are fuzz harness policies,
not format restrictions. Sanitizer RSS includes its allocator/quarantine and
must not be reported as production decoder memory consumption.

Encoder inputs are capped at 2,048 frames and 8 MiB of raw encoder elements;
oversized channel/block combinations are skipped. Encoded packets are capped at
2 MiB. The generated stream's decoder has a 32 MiB element budget. Synthetic
integer PCM and raw zero, NaN, infinity and amplitude-boundary seeds exercise
analysis, reset, input validation and page-spanning packets. This round-trip
oracle checks structure and determinism; independent decoder parity and lossy
source error are measured in the normal encoder reference tests.

The leading residue book now uses three-, eight- and nine-bit entry-order
codewords, while later stages retain eight-bit books. Encoder replays therefore
exercise mixed codeword lengths and unaligned packet tails. All 256 leading-book
words have separate exact-length native decode checks and independent libvorbis
codebook/VQ checks; the PCM quantizer and stage digits are unchanged.

## Replay and extend

Keep a failure unchanged before minimizing it. For a direct replay on this
Windows build, load the recorded MSVC path and the same optional fixture:

```powershell
$run = 'target/deadlib-audio-reference/fuzz-<id>'
$provenance = Get-Content "$run/provenance.json" -Raw | ConvertFrom-Json
$env:PATH = "$($provenance.msvc_bin);$env:PATH"
# Set DEADLIB_VORBIS_FLOOR0 to the recorded, hash-verified file when present.
& target/deadlib-audio-fuzz/x86_64-pc-windows-msvc/release/vorbis_packets.exe 'path/to/artifact' -runs=1
```

Use `cargo +nightly fuzz tmin --fuzz-dir crates/deadlib-audio/decode/fuzz TARGET
ARTIFACT` for minimization, or `cargo +nightly fuzz run` for an individual target.
Both need the same sanitizer runtime path. Promote a real failure to a small
named regression in the affected production crate's tests before fixing it.
Do not weaken an oracle simply to hide a failing input.

Fast harness validation, without a fuzz campaign:

```powershell
cargo test --manifest-path crates/deadlib-audio/decode/fuzz/Cargo.toml --lib
cargo clippy --manifest-path crates/deadlib-audio/decode/fuzz/Cargo.toml --all-targets --features seed -- -D warnings
```

The targets can also be run with cargo-fuzz on supported Unix toolchains; the
PowerShell campaign runner and its retained measurements are Windows-specific.
Coverage counters/features are libFuzzer feedback values, not source-line
coverage percentages. A bounded campaign supplements conformance/differential
tests and does not establish absence of bugs or completion of fuzz validation.

## Recorded initial campaign (2026-10-26)

Windows x86-64, rustc 1.101.0-nightly (07e526db9), LLVM 23.1.4, cargo-fuzz 0.13.2,
libfuzzer-sys 0.4.13, MSVC 14.44.35207. Four targets ran concurrently with a
300-second budget each, seeds 1337..1340, and the pinned floor-zero file enabled.
All exited zero, with no artifacts, assertion failures, sanitizer reports,
timeouts or RSS-limit failures. Total: **183,746 executions**.

| Target | Executions | Coverage counters | Features | Peak sanitizer RSS (MiB) |
| --- | ---: | ---: | ---: | ---: |
| Ogg container | 48,516 | 732 | 2,748 | 495 |
| Vorbis headers | 127,539 | 1,563 | 3,322 | 464 |
| Raw packets | 3,777 | 1,517 | 3,072 | 136 |
| Streaming/seeking | 3,914 | 2,864 | 8,230 | 472 |

[Statistics](validation/2026-10-26-asan.tsv),
[provenance/source hashes](validation/2026-10-26-provenance.json),
[initial corpus hashes](validation/2026-10-26-initial-corpus.sha256.json), and
[binary hashes/arguments](validation/2026-10-26-results.json) preserve this
measurement. These include the uncommitted fuzz harness's source hashes at run
time; the parent Git revision also contains unrelated ongoing work. Fuzz/source
hashes describe checkout bytes on Windows, including line endings. Logs and the
evolving corpus remain in ignored scratch storage. A separate resume smoke run
loaded a previous header corpus and completed successfully.
