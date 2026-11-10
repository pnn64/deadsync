# Isolated FLAC fuzz validation

This unpublished workspace keeps libFuzzer and reference dependencies outside the
production graph. It exercises exact integer PCM and state transitions; it does
not register FLAC with DeadSync. `Cargo.lock` pins the test dependencies.

| Target | Invariants |
| --- | --- |
| primitives | Bit reads agree with direct wire-bit indexing; writing those fields preserves complete bytes. Accepted metadata serializes identically, STREAMINFO roundtrips, and streaming MD5 agrees across chunk sizes. |
| frame | STREAMINFO plus raw frame bytes; reusable decoder results and errors agree before/after malformed input. Accepted frames reject a truncated body. Small decoded frames re-encode and decode to identical PCM. |
| native | Borrowed and short-read decoding return identical frame properties, positions and PCM. Complete bounded streams exercise file-backed seeks with/without table hints against the linear samples. |
| ogg | Pull decoding, two incremental/reset passes and indexed seeks return identical events, metadata, PCM, positions and integrity. |
| encode | Whole interleaved and chunked planar input produce identical native bytes; native and Ogg decoding reproduce exact source integers. Symphonia independently decodes generated frames except its separately recorded 32-bit stereo defect class. |

The frame target's custom mutator repairs header/frame checksums on three quarters
of mutations. The Ogg target similarly repairs page checksums and complete
in-page frame checksums. Other mutations retain checksum-error coverage. No
syntax, numbering or PCM value is repaired. Decoder input is capped at 128 KiB;
metadata blocks at 256, raw frame bytes at 128 KiB, and native decoded observations
at 64 frames/262,144 sample elements (plus the last frame). Ogg preflight caps
declared work at 1,048,576 sample elements, 512 packets and 256 pages. These are
fuzz policies; normal tests separately cover maximum frames and larger metadata.

## Reproduce

On Windows install nightly Rust, cargo-fuzz and the MSVC x64 AddressSanitizer
component. The runner locates MSVC via vswhere, keeps processes hidden, and adds
its runtime DLL directory only to the child environment. It also recognizes
`target/deadlib-audio-reference/fuzz-tools/bin` as a local cargo-fuzz installation.

```powershell
cargo install cargo-fuzz --locked --root target/deadlib-audio-reference/fuzz-tools
python crates/deadlib-audio/decode/flac/fuzz/run.py --run-root target/flac-fuzz-fresh --corpus target/deadlib-flac-reference/conformance --seconds 120 --targets primitives frame native ogg
python crates/deadlib-audio/decode/flac/fuzz/run.py --run-root target/flac-generated-fresh --corpus target/deadlib-flac-reference/conformance --seconds 120 --targets encode
```

The same cargo-fuzz targets can run on supported Unix toolchains. The C compiler
wrapper is built as a native Rust executable. It changes arithmetic settings
only for `symphonia_bundle_flac`; every native crate retains `-Cdebug-assertions`,
AddressSanitizer and SanitizerCoverage. The exact passed arguments are audited.

## Reference arithmetic regression

The initial encoder seed `generated-32-1-8` triggers a checked-add overflow in
Symphonia 0.6.1's LPC reconstruction at decoder.rs:734. The native decoder passes
its exact PCM and MD5 checks first. libFLAC independently reproduces all 257 mono
32-bit samples and verifies the digest. Symphonia's normal release arithmetic
also reproduces those exact samples. The input is preserved as
[symphonia-lpc-overflow.bin](regressions/symphonia-lpc-overflow.bin).

The fuzz compiler wrapper appends `overflow-checks=off` to that reference crate
only, matching its release arithmetic. It retains reference debug assertions,
sanitizer instrumentation and all native overflow checks. Native PCM is always
compared exactly; no sample tolerance is introduced. The first attempt at this
policy reused a cached reference artifact and still failed. The runner now uses
a fresh build per campaign, and its argument audit proves the final policy was
applied. The failed initial attempts remain in ignored scratch directories and
are not counted as successful encoder campaigns.

```powershell
# Normal release replay must pass; a normal debug replay reproduces the reference panic.
cargo run --manifest-path crates/deadlib-audio/decode/flac/fuzz/Cargo.toml --target-dir target/deadlib-flac-fuzz-check --release --features seed --bin replay -- encode crates/deadlib-audio/decode/flac/fuzz/regressions/symphonia-lpc-overflow.bin
```

`replay TARGET FILE_OR_DIRECTORY` replays inputs without instrumentation.
`replay export INPUT OUTPUT.flac` exports an encoder input as native FLAC and
right-aligned interleaved `OUTPUT.i32le` for an independent C decoder check.
